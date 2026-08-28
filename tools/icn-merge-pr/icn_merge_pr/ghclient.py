"""GitHub transport. The ONLY module that knows how bytes reach GitHub.

Everything above this line works on typed values, so the whole decision path can be exercised
against a fake that implements these same named operations. That is deliberate: the defect class
this program replaces was untestable because the semantics lived in a shell pipeline inside a
Markdown file, and the only way to run it was to merge something.

`gh` is the transport because it is what the repository already authenticates. The client owns
query text; callers pass values. No caller ever interpolates a value into a query string.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
from urllib.parse import quote

from .errors import EvidenceUnavailable, GitHubRefused, TransportIndeterminate

_TIMEOUT = 120
# `gh` renders an HTTP status when GitHub actually answered. Its absence means the
# request may never have been answered at all, which is a different fact entirely.
_HTTP_STATUS = re.compile(r"\bHTTP\s+([1-5]\d\d)\b")
# The default-branch tip is THE trust root: policy is loaded from it and the evaluator's whole
# authority chain hangs off it. A revision expression such as `refs/pull/7/head` is not an object
# id and must never stand in for one.
_GIT_OID = re.compile(r"\A[0-9a-fA-F]{40}\Z")


def definitive_http_failure(detail: str) -> bool:
    """Did GitHub DECIDE, or did something merely go wrong?

    A 4xx is a decision: 405 Method Not Allowed, 409 Conflict, 422 Unprocessable. GitHub read the
    request and declined it. A 5xx is not — the server or a gateway failed, and it may have failed
    AFTER the mutation was dispatched, so an immediate read showing `merged == false` proves
    nothing. No status at all means no answer was rendered. Both of those are indeterminate, and
    for a mutation the difference decides whether a caller may treat the result as final.
    """
    found = _HTTP_STATUS.search(detail or "")
    return bool(found) and not found.group(1).startswith("5")

_REPO_META = """
query($owner:String!,$name:String!){
  repository(owner:$owner,name:$name){
    defaultBranchRef{ name target{ oid } }
    mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed
  }
}
"""

_MERGE_QUEUE = """
query($owner:String!,$name:String!,$branch:String!){
  repository(owner:$owner,name:$name){ mergeQueue(branch:$branch){ id } }
}
"""

_PR_CORE = """
query($owner:String!,$name:String!,$number:Int!){
  repository(owner:$owner,name:$name){
    pullRequest(number:$number){
      number state isDraft headRefOid baseRefName baseRefOid
      mergeable mergeStateStatus reviewDecision
      isInMergeQueue
      mergeQueueEntry{ position }
      autoMergeRequest{ enabledAt }
    }
  }
}
"""

_REVIEWS = """
query($owner:String!,$name:String!,$number:Int!,$after:String){
  repository(owner:$owner,name:$name){
    pullRequest(number:$number){
      latestOpinionatedReviews(first:100, after:$after){
        totalCount
        pageInfo{ hasNextPage endCursor }
        nodes{ state }
      }
    }
  }
}
"""

_THREADS = """
query($owner:String!,$name:String!,$number:Int!,$after:String){
  repository(owner:$owner,name:$name){
    pullRequest(number:$number){
      reviewThreads(first:100, after:$after){
        totalCount
        pageInfo{ hasNextPage endCursor }
        nodes{ isResolved }
      }
    }
  }
}
"""

_CHECKS = """
query($owner:String!,$name:String!,$number:Int!,$after:String){
  repository(owner:$owner,name:$name){
    pullRequest(number:$number){
      commits(last:1){ nodes{ commit{ oid statusCheckRollup{
        contexts(first:100, after:$after){
          totalCount
          pageInfo{ hasNextPage endCursor }
          nodes{
            __typename
            ... on CheckRun{ name status conclusion startedAt databaseId
                             checkSuite{ app{ databaseId } } }
            ... on StatusContext{ context state createdAt }
          }
        }
      } } } }
    }
  }
}
"""

_BLOB = """
query($owner:String!,$name:String!,$expr:String!){
  repository(owner:$owner,name:$name){
    object(expression:$expr){ ... on Blob { text isTruncated } }
  }
}
"""

_OBJECT_OID = """
query($owner:String!,$name:String!,$expr:String!){
  repository(owner:$owner,name:$name){ object(expression:$expr){ oid } }
}
"""

_PR_MERGE_STATE = """
query($owner:String!,$name:String!,$number:Int!){
  repository(owner:$owner,name:$name){
    pullRequest(number:$number){ number state merged mergeCommit{ oid } }
  }
}
"""


def _dig(node, *path):
    """Walk a decoded response, raising EvidenceUnavailable rather than TypeError/KeyError.

    A response that does not have the shape we asked for is missing evidence, not a crash: a
    traceback reports nothing a caller can act on, and an evaluator that dies mid-decision is
    indistinguishable from one that has not run.
    """
    cur = node
    for key in path:
        if not isinstance(cur, dict) or key not in cur or cur[key] is None:
            raise EvidenceUnavailable(f"GitHub response missing {'.'.join(map(str, path))}")
        cur = cur[key]
    return cur


class GhCli:
    """Named GitHub operations over the `gh` CLI.

    Pagination is NOT done here. It lives in the snapshot loader, so the loop that decides "have I
    seen every review thread" is the code under test rather than a property of a fake.
    """

    def __init__(self, gh: str = "gh", timeout: int = _TIMEOUT) -> None:
        self.gh = gh
        self.timeout = timeout

    # -- plumbing ------------------------------------------------------------------------------

    def _run(self, argv: list[str], *, on_failure=EvidenceUnavailable) -> str:
        if shutil.which(self.gh) is None:
            raise EvidenceUnavailable(f"{self.gh} is not on PATH; GitHub evidence is unreadable")
        try:
            proc = subprocess.run([self.gh, *argv], capture_output=True, text=True,
                                  timeout=self.timeout, check=False)
        except (OSError, subprocess.SubprocessError) as exc:
            # No answer was ever received. For a mutation that is NOT the same as a refusal.
            raise TransportIndeterminate(
                f"{self.gh} {' '.join(argv[:2])} did not complete: {exc}") from exc
        if proc.returncode != 0:
            detail = (proc.stderr or proc.stdout or "").strip()[:600]
            summary = f"{self.gh} {' '.join(argv[:2])} exited {proc.returncode}: {detail}"
            if not definitive_http_failure(detail):
                raise TransportIndeterminate(summary)
            raise on_failure(summary)
        return proc.stdout

    def _graphql(self, query: str, variables: dict) -> dict:
        argv = ["api", "graphql", "-f", f"query={query}"]
        for key, value in variables.items():
            if value is None:
                continue                       # omitted == null; never send an empty string
            argv += (["-F", f"{key}={value}"] if isinstance(value, int)
                     else ["-f", f"{key}={value}"])
        raw = self._run(argv)
        try:
            doc = json.loads(raw)
        except ValueError as exc:
            raise EvidenceUnavailable(f"GitHub returned undecodable JSON: {exc}") from exc
        if isinstance(doc, dict) and doc.get("errors"):
            raise EvidenceUnavailable(f"GitHub GraphQL errors: {json.dumps(doc['errors'])[:400]}")
        return _dig(doc, "data", "repository")

    def _rest(self, path: str) -> object:
        """ONE REST document. For a paginated collection use `_rest_paginated_list`.

        Deliberately not made to paginate: two of its callers read a single object, and a client
        that paginated everything would have to guess which shape it was looking at.
        """
        raw = self._run(["api", path])
        try:
            return json.loads(raw)
        except ValueError as exc:
            raise EvidenceUnavailable(f"GitHub returned undecodable JSON: {exc}") from exc

    def _rest_paginated_list(self, path: str) -> list:
        """EVERY page of a paginated REST collection, or refuse.

        Verified against the installed gh (2.97.0) rather than assumed from the flag names:
        `--paginate` alone streams ONE JSON DOCUMENT PER PAGE, which is not a single decodable
        value — concatenating those and hoping `json.loads` copes is the trap. `--paginate --slurp`
        wraps them into one array whose elements are the page arrays, which is what this decodes.

        Partial enumeration is never a result. A page that will not decode, a document that is not
        an array of arrays, or a `gh` invocation that fails part-way through pagination all raise:
        "no further bypass actors were found" must never be inferred from pages nobody read.
        """
        raw = self._run(["api", "--paginate", "--slurp", path])
        try:
            pages = json.loads(raw)
        except ValueError as exc:
            raise EvidenceUnavailable(
                f"GitHub returned undecodable paginated JSON for {path}: {exc}") from exc
        if not isinstance(pages, list):
            raise EvidenceUnavailable(
                f"paginated response for {path} was not an array of pages")
        items: list = []
        for index, page in enumerate(pages, 1):
            if not isinstance(page, list):
                raise EvidenceUnavailable(
                    f"page {index} of {path} was not a list; an unreadable page is not an empty "
                    "one")
            items.extend(page)
        return items

    # -- named operations ----------------------------------------------------------------------

    def repository_metadata(self, owner: str, name: str) -> dict:
        repo = self._graphql(_REPO_META, {"owner": owner, "name": name})
        ref = _dig(repo, "defaultBranchRef")
        allowed = {}
        for field, key in (("mergeCommitAllowed", "merge_allowed"),
                           ("squashMergeAllowed", "squash_allowed"),
                           ("rebaseMergeAllowed", "rebase_allowed")):
            value = repo.get(field)
            # `bool("false")` is True. Casting here would let unreadable metadata say a strategy
            # is permitted, and the run would reach the mutation before GitHub disagreed.
            if type(value) is not bool:
                raise EvidenceUnavailable(
                    f"GitHub did not report a readable {field} ({value!r}); which merge methods "
                    "the repository permits is not something this program will assume")
            allowed[key] = value
        default_oid = _dig(ref, "target", "oid")
        if not isinstance(default_oid, str) or not _GIT_OID.match(default_oid):
            raise EvidenceUnavailable(
                f"GitHub reported {default_oid!r} as the default-branch tip, which is not a full "
                "Git object id; this program will not pin policy to a revision it cannot name")
        return {
            "default_branch": _dig(ref, "name"),
            "default_branch_oid": default_oid,
            **allowed,
        }

    def merge_queue_present(self, owner: str, name: str, branch: str) -> bool:
        """True when the base branch has a merge queue configured.

        A bare merge against a queued base ENQUEUES rather than merges, which is exactly the
        deferred outcome this primitive may not produce. If this cannot be determined the call
        raises, and unavailable is not ready.
        """
        repo = self._graphql(_MERGE_QUEUE, {"owner": owner, "name": name, "branch": branch})
        return repo.get("mergeQueue") is not None

    def pull_request_core(self, owner: str, name: str, number: int) -> dict:
        repo = self._graphql(_PR_CORE, {"owner": owner, "name": name, "number": number})
        pr = _dig(repo, "pullRequest")
        # Same rule as the post-read: `_dig` proves the key exists and is not null, not that the
        # value is an object, and every caller goes straight to `.get`.
        if not isinstance(pr, dict):
            raise EvidenceUnavailable(
                f"GitHub returned a pull request that is not an object for #{number} ({pr!r})")
        return pr

    def review_threads_page(self, owner: str, name: str, number: int, after: str | None) -> dict:
        repo = self._graphql(_THREADS, {"owner": owner, "name": name, "number": number,
                                        "after": after})
        return self._page(_dig(repo, "pullRequest", "reviewThreads"), "review threads")

    @staticmethod
    def _page(value, label: str) -> dict:
        """One connection page, established as an object. `_dig` does not type-check its result."""
        if not isinstance(value, dict):
            raise EvidenceUnavailable(f"{label} returned a page that is not an object ({value!r})")
        return value

    def opinionated_reviews_page(self, owner: str, name: str, number: int,
                                 after: str | None) -> dict:
        repo = self._graphql(_REVIEWS, {"owner": owner, "name": name, "number": number,
                                        "after": after})
        return self._page(_dig(repo, "pullRequest", "latestOpinionatedReviews"),
                          "opinionated reviews")

    def check_contexts_page(self, owner: str, name: str, number: int, after: str | None) -> dict:
        repo = self._graphql(_CHECKS, {"owner": owner, "name": name, "number": number,
                                       "after": after})
        nodes = _dig(repo, "pullRequest", "commits", "nodes")
        if not isinstance(nodes, list) or not nodes:
            raise EvidenceUnavailable("pull request reports no head commit")
        commit = _dig(nodes[0], "commit")
        if not isinstance(commit, dict):
            raise EvidenceUnavailable(
                f"GitHub returned a head commit that is not an object ({commit!r})")
        rollup = commit.get("statusCheckRollup")
        if rollup is None:
            # No rollup at all. Not "no required checks" — it is no evidence about them.
            raise EvidenceUnavailable(
                f"head commit {commit.get('oid')} has no status check rollup")
        return {"head_oid": commit.get("oid"),
                **self._page(_dig(rollup, "contexts"), "the status check rollup")}

    def branch_protection(self, owner: str, name: str, branch: str) -> dict:
        """Live required-check configuration for the branch actually being merged into.

        Never `branches/main/protection`: the branch is a value, resolved externally, because a
        baked branch name is wrong for any repository whose default branch is not `main`.
        """
        doc = self._rest(f"repos/{owner}/{name}/branches/{quote(branch, safe='')}/protection")
        if not isinstance(doc, dict):
            raise EvidenceUnavailable("branch protection response was not an object")
        # Does protection apply to administrators and other bypass-capable roles? This program
        # sends an ordinary merge request with an ordinary credential; whether that request is
        # actually subject to the gates depends on the server, not on the shape of the request.
        admins = doc.get("enforce_admins")
        enabled = admins.get("enabled") if isinstance(admins, dict) else admins
        if type(enabled) is not bool:
            raise EvidenceUnavailable(
                f"branch protection did not report a readable enforce_admins setting "
                f"({admins!r}); whether protection applies to the caller is not something this "
                "program will assume")
        # Does the SERVER refuse a merge while a conversation is unresolved? The client-side
        # thread gate can only describe a moment that has passed: resolving or opening a thread
        # does not move the head SHA, so the head pin cannot bind thread state to the mutation.
        resolution = doc.get("required_conversation_resolution")
        resolution_enabled = (resolution.get("enabled") if isinstance(resolution, dict)
                              else resolution)
        if type(resolution_enabled) is not bool:
            raise EvidenceUnavailable(
                f"branch protection did not report a readable required_conversation_resolution "
                f"setting ({resolution!r}); whether the server enforces thread resolution is not "
                "something this program will assume")
        checks = doc.get("required_status_checks") or {}
        if not isinstance(checks, dict):
            raise EvidenceUnavailable("branch protection required_status_checks was not an object")
        # `checks[]` carries the PRODUCER as well as the name. A positive `app_id` pins the
        # required check to one GitHub App; `-1` (or the legacy `contexts[]` form, which cannot
        # express a producer at all) permits any. Discarding it would let a green check of the
        # right NAME from the wrong source satisfy a gate its configured producer never passed.
        contexts: list[str] = []
        bindings: dict[str, int | None] = {}
        modern = checks.get("checks")
        if modern is not None:
            # PRESENT means authoritative, including when it is empty. Skipping members it cannot
            # read and then falling through to the legacy array was a silent DEGRADATION: the
            # legacy form cannot express a producer, so every check came back unbound and a green
            # run from any source would satisfy it.
            if not isinstance(modern, list):
                raise EvidenceUnavailable(
                    "branch protection reported a required-check collection that is not a list")
            for entry in modern:
                if not isinstance(entry, dict) or not isinstance(entry.get("context"), str):
                    raise EvidenceUnavailable(
                        f"branch protection reported an unreadable required-check entry "
                        f"({entry!r}); an entry this program cannot read is not one it may skip")
                context = entry["context"]
                app = entry.get("app_id")
                # null and -1 are GitHub's documented "any producer". Everything else must be a
                # real app id: mapping an unreadable value like "15368" to None would silently
                # UNBIND the check, and an unbound check accepts a green run from any source.
                # `-1.0 == -1` is True, so a float would have satisfied the unbound sentinel and
                # silently removed a producer binding. The sentinel is an integer or it is not the
                # sentinel.
                if app is None or (type(app) is int and app == -1):
                    binding = None
                elif type(app) is int and app > 0:
                    binding = app
                else:
                    raise EvidenceUnavailable(
                        f"branch protection reports an unreadable producer for {context!r} "
                        f"(app_id={app!r}); an unreadable binding is not an absent one")
                # A context named twice with DIFFERENT producers states two requirements, and the
                # snapshot collapses contexts into a set. Keeping the last one seen would drop a
                # requirement that never passed, so a conflict is unavailable evidence. An exact
                # repeat states the same requirement twice and is harmless.
                if context in bindings and bindings[context] != binding:
                    raise EvidenceUnavailable(
                        f"branch protection reports {context!r} twice with different producers "
                        f"({bindings[context]!r} and {binding!r}); this program cannot satisfy "
                        "both from one result and will not choose between them")
                if context not in bindings:
                    contexts.append(context)
                bindings[context] = binding
        else:
            legacy = checks.get("contexts")
            if legacy is not None:
                if not isinstance(legacy, list) or not all(isinstance(e, str) for e in legacy):
                    raise EvidenceUnavailable(
                        "branch protection reported an unreadable legacy required-check list")
                for entry in legacy:
                    if entry not in bindings:
                        contexts.append(entry)
                    bindings[entry] = None      # the legacy form cannot express a producer
        # An ABSENT review requirement genuinely means none. A requirement that is PRESENT but
        # whose count cannot be read is unavailable evidence: defaulting it to zero would let an
        # unreadable protection response retire the approval gate, and policy admits a null
        # review decision, so nothing downstream would notice.
        reviews = doc.get("required_pull_request_reviews")
        # Its PRESENCE is the server-side review gate: with pull-request review protection
        # configured, GitHub refuses a merge while a review requests changes, whatever the
        # approving-review count. Absent, nothing server-side preserves that requirement once
        # this program has looked away.
        review_protection = reviews is not None
        if reviews is None:
            count = 0
        elif not isinstance(reviews, dict):
            raise EvidenceUnavailable(
                "branch protection required_pull_request_reviews was not an object")
        else:
            count = reviews.get("required_approving_review_count")
            if type(count) is not int:
                raise EvidenceUnavailable(
                    "branch protection reports a review requirement whose approving-review count "
                    f"is unreadable ({count!r}); an unreadable requirement is not no requirement")
        strict = checks.get("strict")
        if type(strict) is not bool:
            # `bool("false")` is True. Requiring exactness for the approval count and casting here
            # would have left the up-to-date requirement satisfiable by unreadable evidence.
            raise EvidenceUnavailable(
                f"branch protection did not report a readable strict setting ({strict!r})")
        # Classic pull-request bypass allowances. Every key is inspected, not a known list of
        # them: a denylist of actor types admits whichever type nobody enumerated.
        allowances: list[str] = []
        reviews_raw = doc.get("required_pull_request_reviews")
        if reviews_raw is not None:
            if not isinstance(reviews_raw, dict):
                raise EvidenceUnavailable(
                    "branch protection required_pull_request_reviews was not an object")
            grants = reviews_raw.get("bypass_pull_request_allowances")
            if grants is not None:
                if not isinstance(grants, dict):
                    raise EvidenceUnavailable(
                        f"branch protection reported unreadable bypass allowances ({grants!r})")
                for kind in sorted(grants):
                    holders = grants[kind]
                    if not isinstance(holders, list):
                        raise EvidenceUnavailable(
                            f"branch protection reported unreadable bypass allowances for "
                            f"{kind!r} ({holders!r})")
                    for holder in holders:
                        label = holder.get("slug") or holder.get("login") or holder.get("name") \
                            if isinstance(holder, dict) else holder
                        allowances.append(f"{kind}:{label}")
        return {
            "required_contexts": contexts,
            "required_bindings": bindings,
            "bypass_allowances": allowances,
            "required_approving_review_count": count,
            "strict": strict,
            "enforce_admins": enabled,
            "required_conversation_resolution": resolution_enabled,
            "review_protection": review_protection,
            "configured": "required_status_checks" in doc,
        }

    def branch_rules(self, owner: str, name: str, branch: str) -> list:
        """The rules ACTIVELY in force on `branch`, from every source GitHub applies.

        This is the applicability oracle. Enumerating rulesets by hand would mean re-implementing
        GitHub's condition matching (`~DEFAULT_BRANCH`, include/exclude patterns) and would miss
        organisation and enterprise rulesets outright — `orgs/{org}/rulesets` needs `admin:org`,
        which an ordinary merger's credential has no business holding. This endpoint reports
        inherited rules to a caller with ordinary repository access.
        """
        return self._rest_paginated_list(
            f"repos/{owner}/{name}/rules/branches/{quote(branch, safe='')}")

    def rulesets(self, owner: str, name: str) -> list:
        """Every ruleset visible for this repository, INCLUDING those inherited from a parent."""
        # Paginated for the same reason as the branch rules, and NOT because the review named it:
        # it is the identical page-one defect, and a ruleset listed only on a later page would go
        # unenumerated while this program claimed enumeration was available.
        return self._rest_paginated_list(f"repos/{owner}/{name}/rulesets?includes_parents=true")

    def ruleset(self, owner: str, name: str, ruleset_id: int) -> dict:
        doc = self._rest(f"repos/{owner}/{name}/rulesets/{ruleset_id}")
        if not isinstance(doc, dict):
            raise EvidenceUnavailable(f"ruleset {ruleset_id} response was not an object")
        return doc

    def blob_text(self, owner: str, name: str, oid: str, path: str) -> str | None:
        """Read one file at a PINNED commit. Returns None when the path does not exist there."""
        repo = self._graphql(_BLOB, {"owner": owner, "name": name, "expr": f"{oid}:{path}"})
        blob = repo.get("object")
        if blob is None:
            return None
        if not isinstance(blob, dict):
            raise EvidenceUnavailable(
                f"GitHub returned an object at {oid}:{path} that is not a blob ({blob!r})")
        if blob.get("isTruncated"):
            raise EvidenceUnavailable(f"{path} at {oid} was truncated by the API")
        text = blob.get("text")
        return text if isinstance(text, str) else None

    def object_oid(self, owner: str, name: str, oid: str, path: str) -> str | None:
        """The git object id of `path` at commit `oid`, or None when it is not there.

        Used to ask one narrow question: has the evaluator's own source changed since the copy
        running right now was installed. Comparing tree ids answers it without reading any content.
        """
        repo = self._graphql(_OBJECT_OID, {"owner": owner, "name": name, "expr": f"{oid}:{path}"})
        obj = repo.get("object")
        if not isinstance(obj, dict):
            return None
        found = obj.get("oid")
        return found if isinstance(found, str) else None

    # -- the one mutation ----------------------------------------------------------------------

    def merge_pull_request(self, owner: str, name: str, number: int, *, sha: str,
                           merge_method: str) -> dict:
        """ONE head-pinned ordinary merge. `sha` is the head GitHub must still see.

        The structured `merge_method` field is why this uses the merge API rather than a CLI
        flag: there is no string to interpolate, so there is no shape in which a policy value
        could become a command-line option.
        """
        raw = self._run(
            ["api", "--method", "PUT", f"repos/{owner}/{name}/pulls/{number}/merge",
             "-f", f"sha={sha}", "-f", f"merge_method={merge_method}"],
            on_failure=GitHubRefused)
        try:
            decoded = json.loads(raw)
        except ValueError:
            # The request was accepted; we simply cannot read the reply. The post-read decides.
            return {}
        # `null` and `[]` are valid JSON and neither is a merge response. Returning one would make
        # the caller raise AttributeError AFTER a mutation was dispatched — a crash exactly where
        # the program is supposed to report uncertainty.
        return decoded if isinstance(decoded, dict) else {}

    def pull_request_merge_state(self, owner: str, name: str, number: int) -> dict:
        repo = self._graphql(_PR_MERGE_STATE, {"owner": owner, "name": name, "number": number})
        pr = _dig(repo, "pullRequest")
        if not isinstance(pr, dict):
            # `_dig` proves a key exists and is not null; it does not prove the VALUE is an object.
            # A syntactically valid response carrying `[]`, `"x"` or `7` here reached `.get` and
            # raised AttributeError — after the mutation had been dispatched, which is exactly
            # where a traceback destroys the MERGE_UNCONFIRMED contract.
            raise EvidenceUnavailable(
                f"GitHub returned a pull request that is not an object for #{number} ({pr!r})")
        commit = pr.get("mergeCommit") or {}
        merged = pr.get("merged")
        if type(merged) is not bool:
            # The single fact the whole post-read exists to establish. Missing or oddly typed, it
            # is unreadable evidence — and must never reach a caller that would read it as False.
            raise EvidenceUnavailable(
                f"GitHub did not report a readable merged flag for #{number} ({merged!r})")
        return {
            "state": pr.get("state"),
            "merged": merged,
            "merge_commit_sha": commit.get("oid") if isinstance(commit, dict) else None,
        }
