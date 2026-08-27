#!/usr/bin/env python3
"""Behaviour controls for the ordinary-merge executable (icn#2651 stage B).

This tests THE PROGRAM, by running its real decision path against a fake GitHub. It does not
assert anything about how a skill is worded: the suite this replaces grew past two hundred string
assertions over Markdown while real defects kept getting through, because a phrase in a document
is not the thing that merges a pull request (icn#2656).

The policy the fake serves is the REAL `ops/state/truth/policy.json` from this checkout, read
through the same pinned-blob path the live program uses. So these cases exercise the landed
contract rather than a convenient copy of it.

Run: python3 scripts/tests/test_icn_merge_pr.py
"""

from __future__ import annotations

import copy
import io
import json
import pathlib
import sys
from contextlib import redirect_stdout, redirect_stderr

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "icn-merge-pr"))

from icn_merge_pr import cli, codes                                          # noqa: E402
from icn_merge_pr.errors import (EvidenceUnavailable, GitHubRefused,         # noqa: E402
                                 MergeToolError, StrategyInvalid)
from icn_merge_pr.policy import POLICY_PATH                                  # noqa: E402
from icn_merge_pr.run import run                                             # noqa: E402
from icn_merge_pr.strategy import api_merge_method                           # noqa: E402

ADR_PATH = "docs/adr/ADR-0016-admin-merge-exception-policy.md"
POLICY_TEXT = (ROOT / POLICY_PATH).read_text(encoding="utf-8")
ADR_TEXT = (ROOT / ADR_PATH).read_text(encoding="utf-8")
REQUIRED = json.loads(POLICY_TEXT)["merge"]["required_checks"]

HEAD = "1" * 40
BASE = "2" * 40
DEFAULT_OID = "2" * 40           # a ready PR is up to date; base and default agree
MERGE_COMMIT = "3" * 40

failures: list[str] = []


def check(desc: str, cond: bool, extra: str = "") -> None:
    print(f"  {'ok  ' if cond else 'FAIL'} {desc}{'' if cond else f'  ({extra})'}")
    if not cond:
        failures.append(desc)


# --- the fake ---------------------------------------------------------------------------------

def world(**overrides) -> dict:
    """A world in which every gate passes. Each case perturbs exactly one thing."""
    base = {
        "default_branch": "main",
        "default_branch_oid": DEFAULT_OID,
        "merge_allowed": False,          # this repository disallows merge commits, as it really does
        "squash_allowed": True,
        "rebase_allowed": True,
        "merge_queue": False,
        "pr": {
            "number": 1, "state": "OPEN", "isDraft": False, "headRefOid": HEAD,
            "baseRefName": "main", "baseRefOid": BASE, "mergeable": "MERGEABLE",
            "mergeStateStatus": "CLEAN", "reviewDecision": None, "isInMergeQueue": False,
            "mergeQueueEntry": None, "autoMergeRequest": None,
            "latestOpinionatedReviews": {"nodes": []},
        },
        "thread_pages": [[{"isResolved": True}, {"isResolved": True}]],
        "check_pages": [[{"__typename": "CheckRun", "name": name, "status": "COMPLETED",
                          "conclusion": "SUCCESS"} for name in REQUIRED]
                        + [{"__typename": "CheckRun", "name": "Compare Against Base",
                            "status": "COMPLETED", "conclusion": "FAILURE"}]],
        "protection": {"required_contexts": list(REQUIRED),
                       "required_approving_review_count": 0, "strict": True},
        "blobs": {POLICY_PATH: POLICY_TEXT, ADR_PATH: ADR_TEXT},
        "post_merge": {"state": "MERGED", "merged": True, "merge_commit_sha": MERGE_COMMIT},
    }
    base.update(overrides)
    return base


class FakeGitHub:
    """Named GitHub operations, page by page. Pagination loops stay in the code under test."""

    def __init__(self, w: dict) -> None:
        self.w = w
        self.loads = 0
        self.merge_calls: list[dict] = []
        self.blob_reads: list[tuple[str, str]] = []

    def repository_metadata(self, owner, name):
        self.loads += 1
        hook = self.w.get("on_refresh")
        if hook and self.loads == 2:          # the refresh immediately before mutation
            hook(self.w)
        if self.w.get("metadata_error"):
            raise EvidenceUnavailable(self.w["metadata_error"])
        return {"default_branch": self.w["default_branch"],
                "default_branch_oid": self.w["default_branch_oid"],
                "merge_allowed": self.w["merge_allowed"],
                "squash_allowed": self.w["squash_allowed"],
                "rebase_allowed": self.w["rebase_allowed"]}

    def merge_queue_present(self, owner, name, branch):
        if self.w.get("queue_evidence_error"):
            raise EvidenceUnavailable(self.w["queue_evidence_error"])
        return bool(self.w["merge_queue"])

    def pull_request_core(self, owner, name, number):
        return copy.deepcopy(self.w["pr"])

    def _page(self, pages, after, extra=None):
        index = 0 if after is None else int(after)
        has_next = index + 1 < len(pages)
        return {"totalCount": sum(len(p) for p in pages),
                "pageInfo": {"hasNextPage": has_next,
                             "endCursor": str(index + 1) if has_next else None},
                "nodes": pages[index], **(extra or {})}

    def review_threads_page(self, owner, name, number, after):
        return self._page(self.w["thread_pages"], after)

    def check_contexts_page(self, owner, name, number, after):
        return self._page(self.w["check_pages"], after,
                          {"head_oid": self.w.get("rollup_head", self.w["pr"]["headRefOid"])})

    def branch_protection(self, owner, name, branch):
        if self.w.get("protection_error"):
            raise EvidenceUnavailable(self.w["protection_error"])
        return copy.deepcopy(self.w["protection"])

    def blob_text(self, owner, name, oid, path):
        self.blob_reads.append((oid, path))
        return self.w["blobs"].get(path)

    def merge_pull_request(self, owner, name, number, *, sha, merge_method):
        self.merge_calls.append({"number": number, "sha": sha, "merge_method": merge_method})
        if self.w.get("merge_refused"):
            raise GitHubRefused(self.w["merge_refused"])
        return self.w.get("merge_response", {"merged": True, "sha": MERGE_COMMIT})

    def pull_request_merge_state(self, owner, name, number):
        if self.w.get("post_merge_error"):
            raise EvidenceUnavailable(self.w["post_merge_error"])
        return copy.deepcopy(self.w["post_merge"])


def evaluate_world(w: dict, **kwargs):
    fake = FakeGitHub(w)
    return fake, run(fake, "example", "icn", 1, authorize=False, **kwargs)


def merge_world(w: dict, **kwargs):
    fake = FakeGitHub(w)
    return fake, run(fake, "example", "icn", 1, authorize=True, **kwargs)


def expect(desc: str, w: dict, outcome: str, **kwargs) -> None:
    _, result = evaluate_world(w, **kwargs)
    check(f"{desc} -> {outcome}", result.outcome == outcome,
          f"got {result.outcome}: {[r.detail[:90] for r in result.reasons][:2]}")


def expect_merge(desc: str, w: dict, outcome: str, **kwargs):
    fake, result = merge_world(w, **kwargs)
    check(f"{desc} -> {outcome}", result.outcome == outcome,
          f"got {result.outcome}: {[r.detail[:90] for r in result.reasons][:2]}")
    return fake, result


def mutate(**pr_fields) -> dict:
    w = world()
    w["pr"].update(pr_fields)
    return w


# --- the ready control ------------------------------------------------------------------------
print("a clean PR is ready")
fake, result = evaluate_world(world())
check("every required signal green -> READY", result.outcome == codes.READY,
      f"got {result.outcome}: {[r.detail[:120] for r in result.reasons]}")
check("evaluation mutates nothing", fake.merge_calls == [])
check("policy is read only at the externally resolved default-branch OID",
      all(oid == DEFAULT_OID for oid, _ in fake.blob_reads) and fake.blob_reads != [],
      f"{fake.blob_reads}")
check("the strategy came from trusted policy, not an argument",
      result.strategy == {"selected": "squash", "source": "policy_default", "reason": None},
      f"{result.strategy}")

# --- the trust sequence -----------------------------------------------------------------------
print("the trust sequence refuses before policy is loaded")
stacked = mutate(baseRefName="feat/some-stack")
fake = FakeGitHub(stacked)
result = run(fake, "example", "icn", 1, authorize=False)
check("a non-default/stacked base -> REFUSED_NOT_DEFAULT_BASE",
      result.outcome == codes.REFUSED_NOT_DEFAULT_BASE, f"got {result.outcome}")
check("no policy was loaded from the non-default base", fake.blob_reads == [],
      f"{fake.blob_reads}")
check("the refusal names the stack flow",
      any("stack flow" in r.detail for r in result.reasons))

# --- PR state ---------------------------------------------------------------------------------
print("PR state")
expect("a draft", mutate(isDraft=True), codes.REFUSED_DRAFT)
expect("a CLOSED target", mutate(state="CLOSED"), codes.REFUSED_STATE)
expect("an already MERGED target", mutate(state="MERGED"), codes.REFUSED_STATE)
expect("CONFLICTING", mutate(mergeable="CONFLICTING"), codes.REFUSED_NOT_MERGEABLE)
expect("UNKNOWN mergeability", mutate(mergeable="UNKNOWN"), codes.REFUSED_NOT_MERGEABLE)
for state in ("DIRTY", "BLOCKED", "BEHIND", "HAS_HOOKS", "UNKNOWN"):
    expect(f"mergeStateStatus {state}", mutate(mergeStateStatus=state), codes.REFUSED_MERGE_STATE)
expect("a mergeable value this program does not know", mutate(mergeable="SOMETHING_NEW"),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("a draft state GitHub did not report is not a non-draft PR", mutate(isDraft=None),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("a missing base branch is unreadable evidence, not a stacked base",
       mutate(baseRefName=None), codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("a merge state this program does not know", mutate(mergeStateStatus="SOMETHING_NEW"),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)

# --- review ------------------------------------------------------------------------------------
print("review")
expect("reviewDecision CHANGES_REQUESTED", mutate(reviewDecision="CHANGES_REQUESTED"),
       codes.REFUSED_REVIEW)
expect("REVIEW_REQUIRED is not in the allowlist", mutate(reviewDecision="REVIEW_REQUIRED"),
       codes.REFUSED_REVIEW)
expect("a reviewer's latest opinionated review requests changes",
       mutate(latestOpinionatedReviews={"nodes": [{"state": "CHANGES_REQUESTED"}]}),
       codes.REFUSED_REVIEW)
needs_approval = world()
needs_approval["protection"]["required_approving_review_count"] = 1
expect("live protection requires an approval that has not been given", needs_approval,
       codes.REFUSED_REVIEW)
approved = world()
approved["protection"]["required_approving_review_count"] = 1
approved["pr"]["reviewDecision"] = "APPROVED"
expect("live protection satisfied by an approval", approved, codes.READY)

# --- threads -----------------------------------------------------------------------------------
print("review threads, across every page")
page_one = world(thread_pages=[[{"isResolved": False}], [{"isResolved": True}]])
expect("an unresolved thread on page 1", page_one, codes.REFUSED_THREADS)
page_two = world(thread_pages=[[{"isResolved": True}] * 100, [{"isResolved": False}]])
expect("an unresolved thread ONLY on a later page", page_two, codes.REFUSED_THREADS)
clean_pages = world(thread_pages=[[{"isResolved": True}] * 100, [{"isResolved": True}]])
expect("many threads, all resolved, across pages", clean_pages, codes.READY)


class ShortCount(FakeGitHub):
    """GitHub reports more threads than it hands back — a count without the nodes behind it."""

    def review_threads_page(self, owner, name, number, after):
        page = super().review_threads_page(owner, name, number, after)
        page["totalCount"] += 5
        return page


short = ShortCount(world())
short_result = run(short, "example", "icn", 1, authorize=False)
check("a thread count larger than the threads actually readable -> "
      "REFUSED_UNAVAILABLE_EVIDENCE",
      short_result.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {short_result.outcome}")
check("the refusal says the unread threads cannot be shown resolved",
      any("cannot be shown to be resolved" in r.detail for r in short_result.reasons))

# --- required checks ----------------------------------------------------------------------------
print("required checks")


def checks_with(name: str, **fields) -> dict:
    w = world()
    for node in w["check_pages"][0]:
        if node.get("name") == name:
            node.update(fields)
    return w


expect("one required check still running",
       checks_with(REQUIRED[0], status="IN_PROGRESS", conclusion=None),
       codes.REFUSED_REQUIRED_CHECK_PENDING)
expect("one required check queued",
       checks_with(REQUIRED[1], status="QUEUED", conclusion=None),
       codes.REFUSED_REQUIRED_CHECK_PENDING)
expect("one required check failed", checks_with(REQUIRED[2], conclusion="FAILURE"),
       codes.REFUSED_REQUIRED_CHECK_FAILED)
expect("one required check cancelled", checks_with(REQUIRED[3], conclusion="CANCELLED"),
       codes.REFUSED_REQUIRED_CHECK_FAILED)
missing = world()
missing["check_pages"] = [[n for n in missing["check_pages"][0] if n.get("name") != REQUIRED[4]]]
expect("one required check entirely absent from the rollup", missing,
       codes.REFUSED_REQUIRED_CHECK_MISSING)
noise = copy.deepcopy(missing)
noise["check_pages"][0].append({"__typename": "CheckRun", "name": "Some Other Job",
                                "status": "COMPLETED", "conclusion": "SUCCESS"})
expect("a green NON-required check does not substitute for the missing required one", noise,
       codes.REFUSED_REQUIRED_CHECK_MISSING)
rerun = world()
rerun["check_pages"][0].append({"__typename": "CheckRun", "name": REQUIRED[0],
                                "status": "COMPLETED", "conclusion": "FAILURE"})
expect("a re-run that went green does not erase a red occurrence", rerun,
       codes.REFUSED_REQUIRED_CHECK_FAILED)
neutral = checks_with(REQUIRED[5], conclusion="NEUTRAL")
expect("a policy-allowlisted terminal conclusion is accepted", neutral, codes.READY)
paged = world()
paged["check_pages"] = [paged["check_pages"][0][:3], paged["check_pages"][0][3:]]
expect("required checks split across rollup pages are all accounted for", paged, codes.READY)
stale_rollup = world(rollup_head="9" * 40)
expect("a rollup belonging to a different head is inconsistent evidence", stale_rollup,
       codes.REFUSED_UNAVAILABLE_EVIDENCE)

# --- policy / live drift --------------------------------------------------------------------------
print("policy and live configuration must agree")
drift_missing = world()
drift_missing["protection"]["required_contexts"] = list(REQUIRED[:-1])
expect("live protection is missing a policy-declared required check", drift_missing,
       codes.REFUSED_POLICY_DRIFT)
drift_extra = world()
drift_extra["protection"]["required_contexts"] = list(REQUIRED) + ["A New Gate"]
expect("live protection requires a check policy does not declare", drift_extra,
       codes.REFUSED_POLICY_DRIFT)

# --- deferral ---------------------------------------------------------------------------------------
print("nothing may be deferred")
expect("a merge queue on the base branch", world(merge_queue=True), codes.REFUSED_MERGE_QUEUE)
expect("the PR is already in a merge queue", mutate(isInMergeQueue=True),
       codes.REFUSED_ALREADY_QUEUED)
expect("the PR has a merge-queue entry", mutate(mergeQueueEntry={"position": 3}),
       codes.REFUSED_ALREADY_QUEUED)
expect("another actor already armed auto-merge",
       mutate(autoMergeRequest={"enabledAt": "2026-08-27T00:00:00Z"}),
       codes.REFUSED_ALREADY_AUTO_ARMED)
_, armed = evaluate_world(mutate(autoMergeRequest={"enabledAt": "2026-08-27T00:00:00Z"}))
check("an existing auto-merge request is reported, never disarmed",
      any("never disarmed" in r.detail for r in armed.reasons))

# --- unavailable evidence -----------------------------------------------------------------------------
print("unavailable evidence is not ready")
expect("branch-protection evidence unavailable", world(protection_error="403 Forbidden"),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("merge-queue evidence unavailable", world(queue_evidence_error="field unavailable"),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("repository metadata unavailable", world(metadata_error="network down"),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
no_policy = world()
no_policy["blobs"].pop(POLICY_PATH)
expect("policy absent at the pinned revision", no_policy, codes.REFUSED_UNAVAILABLE_EVIDENCE)

# --- malformed policy ------------------------------------------------------------------------------
print("policy must satisfy its own schema before it is consumed")
expect("policy that is not decodable JSON", world(blobs={POLICY_PATH: "{not json",
                                                        ADR_PATH: ADR_TEXT}),
       codes.REFUSED_POLICY_INVALID)


def policy_with(fn) -> dict:
    document = json.loads(POLICY_TEXT)
    fn(document)
    w = world()
    w["blobs"] = {POLICY_PATH: json.dumps(document), ADR_PATH: ADR_TEXT}
    return w


expect("policy naming a strategy outside the closed set",
       policy_with(lambda d: d["merge"].update(default_strategy="admin")),
       codes.REFUSED_POLICY_INVALID)
expect("policy admitting a merge state that is not readiness",
       policy_with(lambda d: d["merge"]["ready_when"]["merge_state_status_in"].append("DIRTY")),
       codes.REFUSED_POLICY_INVALID)
expect("policy allowlisting CHANGES_REQUESTED",
       policy_with(lambda d: d["merge"]["ready_when"]["review_decision_allowlist"]
                   .append("CHANGES_REQUESTED")),
       codes.REFUSED_POLICY_INVALID)
expect("policy tolerating an unresolved thread",
       policy_with(lambda d: d["merge"]["ready_when"].update(unresolved_review_threads=1)),
       codes.REFUSED_POLICY_INVALID)
adr_gone = world()
adr_gone["blobs"].pop(ADR_PATH)
expect("the admin-bypass authoritative source missing at the pinned revision", adr_gone,
       codes.REFUSED_POLICY_INVALID)

# --- strategy -----------------------------------------------------------------------------------------
print("the strategy enum is closed and owned by code")
expect("an arbitrary strategy argument", world(), codes.REFUSED_STRATEGY_INVALID,
       requested_strategy="tarball")
expect("`admin` as a strategy", world(), codes.REFUSED_STRATEGY_INVALID,
       requested_strategy="admin")
expect("`rebase` is in the enum but is neither the default nor the documented exception",
       world(), codes.REFUSED_STRATEGY_INVALID, requested_strategy="rebase")
expect("the documented exception selected without a stated reason", world(),
       codes.REFUSED_STRATEGY_INVALID, requested_strategy="merge")
expect("the documented exception, stated, but disallowed by the repository", world(),
       codes.REFUSED_STRATEGY_UNAVAILABLE, requested_strategy="merge",
       exception_reason="squashed subtree import")
allows_merge = world(merge_allowed=True)
_, exception_result = evaluate_world(allows_merge, requested_strategy="merge",
                                     exception_reason="squashed subtree import")
check("the documented exception, stated, on a repository that allows it -> READY",
      exception_result.outcome == codes.READY, f"got {exception_result.outcome}")
check("the operator's reason is recorded with the result",
      exception_result.strategy["reason"] == "squashed subtree import"
      and exception_result.strategy["source"] == "operator_exception",
      f"{exception_result.strategy}")

for bad in ("admin", "--admin", "--auto", "", None, 7):
    try:
        api_merge_method(bad)
        check(f"strategy mapping refuses {bad!r}", False, "it returned a value")
    except StrategyInvalid:
        check(f"strategy mapping refuses {bad!r}", True)
check("no policy value can become a command-line option in the strategy mapping",
      "--" not in (ROOT / "tools" / "icn-merge-pr" / "icn_merge_pr" / "strategy.py")
      .read_text(encoding="utf-8").split('"""', 2)[2])

# --- the command line -----------------------------------------------------------------------------------
print("the command line is closed")


def parse_outcome(argv: list[str]) -> str:
    try:
        cli.parse(argv)
    except MergeToolError as exc:
        return exc.outcome
    return "PARSED"


for token in ("--admin", "--auto", "--disable-auto", "--force", "--squash", "--merge",
              "--rebase", "--enqueue"):
    check(f"`merge 1 --authorize {token}` -> REFUSED_FORBIDDEN_OPTION",
          parse_outcome(["merge", "1", "--authorize", token]) == codes.REFUSED_FORBIDDEN_OPTION,
          parse_outcome(["merge", "1", "--authorize", token]))
check("a forbidden option carrying an inline value is still refused",
      parse_outcome(["merge", "1", "--authorize", "--admin=yes"])
      == codes.REFUSED_FORBIDDEN_OPTION)
check("an unknown option is refused",
      parse_outcome(["check", "1", "--yolo"]) == codes.REFUSED_USAGE)
check("mutation without explicit authorization is refused",
      parse_outcome(["merge", "1"]) == codes.REFUSED_USAGE)
check("`check` cannot be given the mutation authorization",
      parse_outcome(["check", "1", "--authorize"]) == codes.REFUSED_USAGE)
check("a non-numeric PR is refused", parse_outcome(["check", "abc"]) == codes.REFUSED_USAGE)
check("an unknown command is refused", parse_outcome(["merge-all"]) == codes.REFUSED_USAGE)
check("a repeated option is refused",
      parse_outcome(["check", "1", "--repo", "a/b", "--repo", "c/d"]) == codes.REFUSED_USAGE)
check("`check 1` parses", parse_outcome(["check", "1"]) == "PARSED")
check("`merge 1 --authorize` parses",
      parse_outcome(["merge", "1", "--authorize"]) == "PARSED")
with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
    admin_exit = cli.main(["merge", "1", "--authorize", "--admin"])
check("a privileged option exits non-zero without touching GitHub",
      admin_exit == codes.EXIT_USAGE, f"exit={admin_exit}")

# This suite runs from the source tree, which is exactly the situation the gate exists for.
buffer = io.StringIO()
with redirect_stdout(buffer), redirect_stderr(io.StringIO()):
    source_exit = cli.main(["merge", "1", "--authorize", "--repo", "example/icn"])
payload = json.loads(buffer.getvalue())
check("a source-tree copy refuses to mutate even with an explicit --repo",
      payload["outcome"] == codes.REFUSED_NOT_INSTALLED and source_exit != codes.EXIT_OK,
      f"{payload['outcome']} exit={source_exit}")
check("the refusal points at the installer",
      "install.py" in payload["reasons"][0]["detail"])
buffer = io.StringIO()
with redirect_stdout(buffer), redirect_stderr(io.StringIO()):
    cli.main(["check", "1"])
check("evaluation from source is refused for want of a repository, not for want of an install",
      json.loads(buffer.getvalue())["outcome"] == codes.REFUSED_USAGE,
      json.loads(buffer.getvalue())["outcome"])

# --- races ---------------------------------------------------------------------------------------------
print("evaluation is not permission: everything is re-read before mutating")


def racing(mutation) -> dict:
    return world(on_refresh=mutation)


def set_head(w):
    w["pr"]["headRefOid"] = "9" * 40


fake, _ = expect_merge("the head moves between evaluation and merge", racing(set_head),
                       codes.REFUSED_HEAD_CHANGED)
check("no merge was attempted after a head race", fake.merge_calls == [])

fake, _ = expect_merge("the PR base commit moves",
                       racing(lambda w: w["pr"].update(baseRefOid="8" * 40)),
                       codes.REFUSED_BASE_CHANGED)
check("no merge was attempted after a base race", fake.merge_calls == [])
fake, _ = expect_merge("the PR is retargeted onto a non-default base on the refresh",
                       racing(lambda w: w["pr"].update(baseRefName="feat/stack")),
                       codes.REFUSED_NOT_DEFAULT_BASE)
check("no merge was attempted after a retarget", fake.merge_calls == [])
fake, _ = expect_merge("the default-branch commit moves",
                       racing(lambda w: w.update(default_branch_oid="7" * 40)),
                       codes.REFUSED_DEFAULT_BRANCH_CHANGED)
check("no merge was attempted after a default-branch race", fake.merge_calls == [])


def change_policy(w):
    document = json.loads(POLICY_TEXT)
    document["merge"]["race_note"] = "the document changed underneath the evaluation"
    w["blobs"][POLICY_PATH] = json.dumps(document)


fake, _ = expect_merge("policy content changes underneath the evaluation", racing(change_policy),
                       codes.REFUSED_POLICY_DRIFT)
check("no merge was attempted after a policy race", fake.merge_calls == [])


def redden(w):
    for node in w["check_pages"][0]:
        if node.get("name") == REQUIRED[0]:
            node["conclusion"] = "FAILURE"


fake, _ = expect_merge("a required check goes red on the refresh", racing(redden),
                       codes.REFUSED_REQUIRED_CHECK_FAILED)
check("no merge was attempted after a check went red", fake.merge_calls == [])


def repend(w):
    for node in w["check_pages"][0]:
        if node.get("name") == REQUIRED[0]:
            node.update(status="IN_PROGRESS", conclusion=None)


expect_merge("a required check goes pending on the refresh", racing(repend),
             codes.REFUSED_REQUIRED_CHECK_PENDING)
fake, _ = expect_merge("a new unresolved thread appears on the refresh",
                       racing(lambda w: w["thread_pages"][0].append({"isResolved": False})),
                       codes.REFUSED_THREADS)
check("no merge was attempted after a new thread appeared", fake.merge_calls == [])
expect_merge("an auto-merge request appears on the refresh",
             racing(lambda w: w["pr"].update(autoMergeRequest={"enabledAt": "now"})),
             codes.REFUSED_ALREADY_AUTO_ARMED)
expect_merge("the PR is closed on the refresh",
             racing(lambda w: w["pr"].update(state="CLOSED")), codes.REFUSED_STATE)
fake, refreshed = merge_world(racing(set_head))
check("the refusal after a race carries the refreshed evidence",
      refreshed.evidence["head_oid"] == "9" * 40, f"{refreshed.evidence['head_oid']}")

# --- mutation ---------------------------------------------------------------------------------------------
print("the mutation itself")
fake, merged = merge_world(world())
check("a ready, authorised merge -> MERGED", merged.outcome == codes.MERGED,
      f"got {merged.outcome}: {[r.detail[:120] for r in merged.reasons]}")
check("EXACTLY one merge request was issued", len(fake.merge_calls) == 1, f"{fake.merge_calls}")
check("the expected head SHA was pinned in the request",
      fake.merge_calls and fake.merge_calls[0]["sha"] == HEAD, f"{fake.merge_calls}")
check("the strategy reached GitHub only through the closed code mapping",
      fake.merge_calls and fake.merge_calls[0]["merge_method"] == "squash", f"{fake.merge_calls}")
check("the resulting merge commit is reported",
      merged.merge["merge_commit_sha"] == MERGE_COMMIT and merged.merge["confirmed_merged"],
      f"{merged.merge}")

refused_world = world(merge_refused="405 Method Not Allowed",
                      post_merge={"state": "OPEN", "merged": False, "merge_commit_sha": None})
fake, refused = merge_world(refused_world)
check("GitHub refusing the merge -> REFUSED_GITHUB", refused.outcome == codes.REFUSED_GITHUB,
      f"got {refused.outcome}")
check("a GitHub refusal is final — no second attempt, no weaker flags",
      len(fake.merge_calls) == 1, f"{fake.merge_calls}")
check("a refused merge does not claim a commit",
      refused.merge == {"attempted": True, "confirmed_merged": False, "merge_commit_sha": None},
      f"{refused.merge}")
check("the refusal is confirmed by a fresh read, not by the call having failed",
      any("fresh read confirms" in r.detail for r in refused.reasons))

# Once a request has been dispatched, "nothing happened" is a claim that needs evidence.
lost = world(merge_refused="context deadline exceeded",
             post_merge={"state": "MERGED", "merged": True, "merge_commit_sha": MERGE_COMMIT})
fake, result = merge_world(lost)
check("a failed request whose PR now reads MERGED -> MERGE_UNCONFIRMED",
      result.outcome == codes.MERGE_UNCONFIRMED, f"got {result.outcome}")
check("it does not claim this run caused the merge",
      any("does not claim" in r.detail for r in result.reasons))
check("one request was still the only request", len(fake.merge_calls) == 1)

blind = world(merge_refused="context deadline exceeded", post_merge_error="network down")
_, result = merge_world(blind)
check("a failed request that cannot be read back -> MERGE_UNCONFIRMED",
      result.outcome == codes.MERGE_UNCONFIRMED, f"got {result.outcome}")
check("an unreadable outcome is reported as UNKNOWN, never as a refusal",
      any("UNKNOWN" in r.detail for r in result.reasons))

_, result = merge_world(world(post_merge_error="network down"))
check("an ACCEPTED request that cannot be read back -> MERGE_UNCONFIRMED",
      result.outcome == codes.MERGE_UNCONFIRMED, f"got {result.outcome}")

unconfirmed = world(post_merge={"state": "OPEN", "merged": False, "merge_commit_sha": None})
fake, result = merge_world(unconfirmed)
check("API success with a post-read that does not say merged -> MERGE_UNCONFIRMED",
      result.outcome == codes.MERGE_UNCONFIRMED, f"got {result.outcome}")
check("an unconfirmed merge is not success", codes.exit_code(result.outcome) != codes.EXIT_OK)
check("an unconfirmed merge does not claim confirmation",
      result.merge["confirmed_merged"] is False, f"{result.merge}")

null_merged = world(post_merge={"state": "OPEN", "merged": None, "merge_commit_sha": None})
_, result = merge_world(null_merged)
check("a post-read that reports nothing about merging is not success",
      result.outcome == codes.MERGE_UNCONFIRMED, f"got {result.outcome}")

fake, _ = merge_world(mutate(isDraft=True))
check("a refused evaluation never reaches the mutation", fake.merge_calls == [])
fake, _ = evaluate_world(world())
check("`check` never mutates even when everything is green", fake.merge_calls == [])

# --- the result envelope ------------------------------------------------------------------------------------
print("results are machine-readable")
_, ready = evaluate_world(world())
payload = json.loads(json.dumps(ready.as_dict()))
check("the result serialises to JSON with a stable outcome code",
      payload["outcome"] in codes.ALL_CODES and payload["pr"] == 1)
check("every reason carries a stable code",
      all(r["code"] in codes.ALL_CODES for r in payload["reasons"]))
_, drifted = evaluate_world(drift_missing)
check("a refusal reports which evidence produced it",
      drifted.reasons and "policy-only" in drifted.reasons[0].detail)
check("there is no admin outcome to report",
      not any("ADMIN" in c for c in codes.ALL_CODES), sorted(codes.ALL_CODES))

print()
if failures:
    print(f"icn-merge-pr behaviour tests: {len(failures)} failure(s)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("icn-merge-pr behaviour tests: clean")
