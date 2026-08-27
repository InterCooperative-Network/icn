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
_HTTP_STATUS = re.compile(r"\bHTTP\s+[1-5]\d\d\b")

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
            ... on CheckRun{ name status conclusion checkSuite{ app{ databaseId } } }
            ... on StatusContext{ context state }
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
            if not _HTTP_STATUS.search(detail):
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
        raw = self._run(["api", path])
        try:
            return json.loads(raw)
        except ValueError as exc:
            raise EvidenceUnavailable(f"GitHub returned undecodable JSON: {exc}") from exc

    # -- named operations ----------------------------------------------------------------------

    def repository_metadata(self, owner: str, name: str) -> dict:
        repo = self._graphql(_REPO_META, {"owner": owner, "name": name})
        ref = _dig(repo, "defaultBranchRef")
        return {
            "default_branch": _dig(ref, "name"),
            "default_branch_oid": _dig(ref, "target", "oid"),
            "merge_allowed": bool(repo.get("mergeCommitAllowed")),
            "squash_allowed": bool(repo.get("squashMergeAllowed")),
            "rebase_allowed": bool(repo.get("rebaseMergeAllowed")),
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
        return _dig(repo, "pullRequest")

    def review_threads_page(self, owner: str, name: str, number: int, after: str | None) -> dict:
        repo = self._graphql(_THREADS, {"owner": owner, "name": name, "number": number,
                                        "after": after})
        return _dig(repo, "pullRequest", "reviewThreads")

    def opinionated_reviews_page(self, owner: str, name: str, number: int,
                                 after: str | None) -> dict:
        repo = self._graphql(_REVIEWS, {"owner": owner, "name": name, "number": number,
                                        "after": after})
        return _dig(repo, "pullRequest", "latestOpinionatedReviews")

    def check_contexts_page(self, owner: str, name: str, number: int, after: str | None) -> dict:
        repo = self._graphql(_CHECKS, {"owner": owner, "name": name, "number": number,
                                       "after": after})
        nodes = _dig(repo, "pullRequest", "commits", "nodes")
        if not isinstance(nodes, list) or not nodes:
            raise EvidenceUnavailable("pull request reports no head commit")
        commit = _dig(nodes[0], "commit")
        rollup = commit.get("statusCheckRollup")
        if rollup is None:
            # No rollup at all. Not "no required checks" — it is no evidence about them.
            raise EvidenceUnavailable(
                f"head commit {commit.get('oid')} has no status check rollup")
        return {"head_oid": commit.get("oid"), **_dig(rollup, "contexts")}

    def branch_protection(self, owner: str, name: str, branch: str) -> dict:
        """Live required-check configuration for the branch actually being merged into.

        Never `branches/main/protection`: the branch is a value, resolved externally, because a
        baked branch name is wrong for any repository whose default branch is not `main`.
        """
        doc = self._rest(f"repos/{owner}/{name}/branches/{quote(branch, safe='')}/protection")
        if not isinstance(doc, dict):
            raise EvidenceUnavailable("branch protection response was not an object")
        checks = doc.get("required_status_checks") or {}
        if not isinstance(checks, dict):
            raise EvidenceUnavailable("branch protection required_status_checks was not an object")
        # `checks[]` carries the PRODUCER as well as the name. A positive `app_id` pins the
        # required check to one GitHub App; `-1` (or the legacy `contexts[]` form, which cannot
        # express a producer at all) permits any. Discarding it would let a green check of the
        # right NAME from the wrong source satisfy a gate its configured producer never passed.
        contexts: list[str] = []
        bindings: dict[str, int | None] = {}
        for entry in checks.get("checks") or []:
            if isinstance(entry, dict) and isinstance(entry.get("context"), str):
                contexts.append(entry["context"])
                app = entry.get("app_id")
                bindings[entry["context"]] = app if type(app) is int and app > 0 else None
        if not contexts:
            for entry in checks.get("contexts") or []:
                if isinstance(entry, str):
                    contexts.append(entry)
                    bindings[entry] = None
        reviews = doc.get("required_pull_request_reviews") or {}
        count = reviews.get("required_approving_review_count") if isinstance(reviews, dict) else 0
        return {
            "required_contexts": contexts,
            "required_bindings": bindings,
            "required_approving_review_count": count if type(count) is int else 0,
            "strict": bool(checks.get("strict")),
            "configured": "required_status_checks" in doc,
        }

    def blob_text(self, owner: str, name: str, oid: str, path: str) -> str | None:
        """Read one file at a PINNED commit. Returns None when the path does not exist there."""
        repo = self._graphql(_BLOB, {"owner": owner, "name": name, "expr": f"{oid}:{path}"})
        blob = repo.get("object")
        if blob is None:
            return None
        if blob.get("isTruncated"):
            raise EvidenceUnavailable(f"{path} at {oid} was truncated by the API")
        text = blob.get("text")
        return text if isinstance(text, str) else None

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
        commit = pr.get("mergeCommit") or {}
        return {
            "state": pr.get("state"),
            "merged": pr.get("merged"),
            "merge_commit_sha": commit.get("oid") if isinstance(commit, dict) else None,
        }
