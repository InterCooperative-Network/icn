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
                                 MergeToolError, StrategyInvalid,
                                 TransportIndeterminate)
from icn_merge_pr.policy import POLICY_PATH                                  # noqa: E402
from icn_merge_pr.ghclient import GhCli                                     # noqa: E402
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
ACTIONS_APP = 15368          # this repository pins every required check to the Actions App
IMPOSTOR_APP = 99999

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
        },
        "review_pages": [[]],
        "thread_pages": [[{"isResolved": True}, {"isResolved": True}]],
        "check_pages": [[{"__typename": "CheckRun", "name": name, "status": "COMPLETED",
                          "conclusion": "SUCCESS",
                          "checkSuite": {"app": {"databaseId": ACTIONS_APP}}}
                         for name in REQUIRED]
                        + [{"__typename": "CheckRun", "name": "Compare Against Base",
                            "status": "COMPLETED", "conclusion": "FAILURE",
                            "checkSuite": {"app": {"databaseId": ACTIONS_APP}}}]],
        "protection": {"required_contexts": list(REQUIRED),
                       "required_bindings": {name: ACTIONS_APP for name in REQUIRED},
                       "required_approving_review_count": 0, "strict": True,
                       "enforce_admins": True, "bypass_allowances": [],
                       "required_conversation_resolution": True, "review_protection": True},
        "branch_rules": [],
        "rulesets_listing": [],
        "rulesets": {},
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
        if "default_branch_oid_raw" in self.w:
            client = GhCli()
            client._graphql = lambda q, v: {
                "defaultBranchRef": {"name": self.w["default_branch"],
                                     "target": {"oid": self.w["default_branch_oid_raw"]}},
                "mergeCommitAllowed": False, "squashMergeAllowed": True,
                "rebaseMergeAllowed": True}
            return client.repository_metadata(owner, name)
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
        if "pr_object" in self.w:
            # Drive the REAL transport guard over a non-object `pullRequest`.
            client = GhCli()
            client._graphql = lambda q, v: {"pullRequest": self.w["pr_object"]}
            return client.pull_request_core(owner, name, number)
        return copy.deepcopy(self.w["pr"])

    def _page(self, pages, after, extra=None):
        index = 0 if after is None else int(after)
        has_next = index + 1 < len(pages)
        return {"totalCount": sum(len(p) for p in pages),
                "pageInfo": {"hasNextPage": has_next,
                             "endCursor": str(index + 1) if has_next else None},
                "nodes": pages[index], **(extra or {})}

    def opinionated_reviews_page(self, owner, name, number, after):
        return self._page(self.w["review_pages"], after)

    def review_threads_page(self, owner, name, number, after):
        return self._page(self.w["thread_pages"], after)

    def check_contexts_page(self, owner, name, number, after):
        return self._page(self.w["check_pages"], after,
                          {"head_oid": self.w.get("rollup_head", self.w["pr"]["headRefOid"])})

    def rulesets(self, owner, name):
        if self.w.get("rulesets_error"):
            raise EvidenceUnavailable(self.w["rulesets_error"])
        return list(self.w["rulesets_listing"])

    def branch_rules(self, owner, name, branch):
        if self.w.get("branch_rules_error"):
            raise EvidenceUnavailable(self.w["branch_rules_error"])
        return list(self.w["branch_rules"])

    def ruleset(self, owner, name, ruleset_id):
        if self.w.get("ruleset_error"):
            raise EvidenceUnavailable(self.w["ruleset_error"])
        detail = self.w["rulesets"].get(ruleset_id)
        if detail is None:
            raise EvidenceUnavailable(f"ruleset {ruleset_id} is unreadable")
        return dict(detail)

    def branch_protection(self, owner, name, branch):
        if self.w.get("protection_error"):
            raise EvidenceUnavailable(self.w["protection_error"])
        return copy.deepcopy(self.w["protection"])

    def blob_text(self, owner, name, oid, path):
        self.blob_reads.append((oid, path))
        return self.w["blobs"].get(path)

    def merge_pull_request(self, owner, name, number, *, sha, merge_method):
        self.merge_calls.append({"number": number, "sha": sha, "merge_method": merge_method})
        if self.w.get("merge_transport_lost"):
            raise TransportIndeterminate(self.w["merge_transport_lost"])
        if self.w.get("merge_refused"):
            raise GitHubRefused(self.w["merge_refused"])
        return self.w.get("merge_response", {"merged": True, "sha": MERGE_COMMIT})

    def object_oid(self, owner, name, oid, path):
        return self.w.get("object_oids", {}).get((oid, path), f"tree-of-{path}")

    def pull_request_merge_state(self, owner, name, number):
        if self.w.get("post_merge_error"):
            raise EvidenceUnavailable(self.w["post_merge_error"])
        if "post_merge_object" in self.w:
            # Drive the REAL transport guard over a non-object `pullRequest`.
            client = GhCli()
            client._graphql = lambda q, v: {"pullRequest": self.w["post_merge_object"]}
            return client.pull_request_merge_state(owner, name, number)
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


def policy_with(fn) -> dict:
    """A world serving the real policy with one mutation applied at the pinned revision."""
    document = json.loads(POLICY_TEXT)
    fn(document)
    w = world()
    w["blobs"] = {POLICY_PATH: json.dumps(document), ADR_PATH: ADR_TEXT}
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
# The wrapper is told to quote the structured reason and observed state rather than translate a
# refusal into "the PR did not merge". That instruction is only honest if the evidence is here.
_, already = evaluate_world(mutate(state="MERGED"))
check("a refusal on an already-merged target reports the OBSERVED state, not an assumed one",
      already.evidence["state"] == "MERGED"
      and any("MERGED" in r.detail for r in already.reasons), f"{already.reasons[:1]}")
_, closed_pr = evaluate_world(mutate(state="CLOSED"))
check("a refusal on a closed target names CLOSED",
      closed_pr.evidence["state"] == "CLOSED"
      and any("CLOSED" in r.detail for r in closed_pr.reasons), f"{closed_pr.reasons[:1]}")
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
       world(review_pages=[[{"state": "CHANGES_REQUESTED"}]]), codes.REFUSED_REVIEW)
expect("an objection ONLY on a later page of opinionated reviews",
       world(review_pages=[[{"state": "APPROVED"}] * 100, [{"state": "CHANGES_REQUESTED"}]]),
       codes.REFUSED_REVIEW)


class ShortReviewCount(FakeGitHub):
    """GitHub reports more opinionated reviews than it hands back."""

    def opinionated_reviews_page(self, owner, name, number, after):
        page = super().opinionated_reviews_page(owner, name, number, after)
        page["totalCount"] += 3
        return page


short_reviews = ShortReviewCount(world())
short_reviews_result = run(short_reviews, "example", "icn", 1, authorize=False)
check("a review count larger than the reviews actually readable -> "
      "REFUSED_UNAVAILABLE_EVIDENCE",
      short_reviews_result.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE,
      f"got {short_reviews_result.outcome}")
check("the refusal says the unread reviews cannot be shown not to object",
      any("cannot be shown not to object" in r.detail for r in short_reviews_result.reasons))
# Policy and live both say one approval, so these exercise the REVIEW gate rather than the
# drift gate — raising only the live count would be configuration drift, which is tested below.
needs_approval = policy_with(lambda d: d["branch"].update(required_approvals=1))
needs_approval["protection"]["required_approving_review_count"] = 1
expect("a required approval that has not been given", needs_approval, codes.REFUSED_REVIEW)
approved = policy_with(lambda d: d["branch"].update(required_approvals=1))
approved["protection"]["required_approving_review_count"] = 1
approved["pr"]["reviewDecision"] = "APPROVED"
expect("a required approval that was given", approved, codes.READY)

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
                                "status": "COMPLETED", "conclusion": "SUCCESS",
                                "checkSuite": {"app": {"databaseId": ACTIONS_APP}}})
expect("a green NON-required check does not substitute for the missing required one", noise,
       codes.REFUSED_REQUIRED_CHECK_MISSING)
# A check can fail and be re-run green on the SAME commit. The CURRENT run decides: an older
# superseded failure must not poison readiness, and an older success must not mask a newer failure.
def runs(name, *occurrences):
    """A world where `name` has several runs, given as (conclusion, startedAt, databaseId)."""
    w = world()
    w["check_pages"][0] = [n for n in w["check_pages"][0] if n.get("name") != name]
    for conclusion, started, seq in occurrences:
        w["check_pages"][0].append({
            "__typename": "CheckRun", "name": name,
            "status": "IN_PROGRESS" if conclusion is None else "COMPLETED",
            "conclusion": conclusion, "startedAt": started, "databaseId": seq,
            "checkSuite": {"app": {"databaseId": ACTIONS_APP}}})
    return w


expect("a failure superseded by a later successful re-run",
       runs(REQUIRED[0], ("FAILURE", "2026-08-27T10:00:00Z", 1),
            ("SUCCESS", "2026-08-27T11:00:00Z", 2)), codes.READY)
expect("a success superseded by a later failure is not masked",
       runs(REQUIRED[0], ("SUCCESS", "2026-08-27T10:00:00Z", 1),
            ("FAILURE", "2026-08-27T11:00:00Z", 2)), codes.REFUSED_REQUIRED_CHECK_FAILED)
expect("a failure followed by a re-run still in progress is pending, not failed",
       runs(REQUIRED[0], ("FAILURE", "2026-08-27T10:00:00Z", 1),
            (None, "2026-08-27T11:00:00Z", 2)), codes.REFUSED_REQUIRED_CHECK_PENDING)
expect("a same-second re-run is ordered by the monotonic run id",
       runs(REQUIRED[0], ("FAILURE", "2026-08-27T10:00:00Z", 1),
            ("SUCCESS", "2026-08-27T10:00:00Z", 2)), codes.READY)
expect("several runs with no ordering evidence cannot be told apart",
       runs(REQUIRED[0], ("FAILURE", None, None), ("SUCCESS", None, None)),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("two runs sharing an identical position cannot be told apart",
       runs(REQUIRED[0], ("FAILURE", "2026-08-27T10:00:00Z", 1),
            ("SUCCESS", "2026-08-27T10:00:00Z", 1)), codes.REFUSED_UNAVAILABLE_EVIDENCE)
_, ambiguous_result = evaluate_world(
    runs(REQUIRED[0], ("FAILURE", None, None), ("SUCCESS", None, None)))
check("the ambiguity refusal says it will not guess",
      any("guessing would do one or the other" in r.detail for r in ambiguous_result.reasons))
expect("a single run needs no ordering evidence at all",
       runs(REQUIRED[0], ("SUCCESS", None, None)), codes.READY)
neutral = checks_with(REQUIRED[5], conclusion="NEUTRAL")
expect("a policy-allowlisted terminal conclusion is accepted", neutral, codes.READY)
paged = world()
paged["check_pages"] = [paged["check_pages"][0][:3], paged["check_pages"][0][3:]]
expect("required checks split across rollup pages are all accounted for", paged, codes.READY)
print("a required check is a name AND a producer")
impostor = world()
for node in impostor["check_pages"][0]:
    if node.get("name") == REQUIRED[0]:
        node["checkSuite"] = {"app": {"databaseId": IMPOSTOR_APP}}
expect("a green check of the right name from the wrong App", impostor,
       codes.REFUSED_REQUIRED_CHECK_MISSING)
_, impostor_result = evaluate_world(impostor)
check("the refusal names the required producer",
      any("required producer" in r.detail for r in impostor_result.reasons))
status_only = world()
status_only["check_pages"] = [[n for n in status_only["check_pages"][0]
                               if n.get("name") != REQUIRED[1]]
                              + [{"__typename": "StatusContext", "context": REQUIRED[1],
                                  "state": "SUCCESS"}]]
expect("a plain commit status standing in for a producer-bound required check", status_only,
       codes.REFUSED_REQUIRED_CHECK_MISSING)
for bad in ("15368", 15368.0, [], {}, True):
    junk = world()
    junk["protection"]["required_bindings"] = dict(world()["protection"]["required_bindings"])
    junk["protection"]["required_bindings"][REQUIRED[0]] = bad
    _, r = evaluate_world(junk)
    check(f"a required-check producer of {bad!r} is unreadable, not unbound",
          r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {r.outcome}")

unbound = world()
unbound["protection"]["required_bindings"] = {}
for node in unbound["check_pages"][0]:
    if node.get("name") == REQUIRED[0]:
        node["checkSuite"] = {"app": {"databaseId": IMPOSTOR_APP}}
expect("protection that pins no producer accepts any producer", unbound, codes.READY)

class ShortRollupCount(FakeGitHub):
    """The rollup reports more results than it hands back.

    The dangerous shape is not a missing NAME — that already refuses — but a missing OCCURRENCE of
    a name whose other occurrence is green.
    """

    def check_contexts_page(self, owner, name, number, after):
        page = super().check_contexts_page(owner, name, number, after)
        page["totalCount"] += 1
        return page


short_rollup = ShortRollupCount(world())
short_rollup_result = run(short_rollup, "example", "icn", 1, authorize=False)
check("a rollup count larger than the results actually readable -> "
      "REFUSED_UNAVAILABLE_EVIDENCE",
      short_rollup_result.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE,
      f"got {short_rollup_result.outcome}")
check("the refusal says an unread occurrence cannot be shown green",
      any("cannot be shown to be green" in r.detail for r in short_rollup_result.reasons))
unreadable_node = world()
unreadable_node["check_pages"][0].append({"__typename": "SomethingNew", "name": "x"})
expect("a rollup entry this program cannot read", unreadable_node,
       codes.REFUSED_UNAVAILABLE_EVIDENCE)

print("malformed external evidence refuses instead of crashing")
for field in ("state", "mergeable", "mergeStateStatus"):
    for shape in (["OPEN"], {"v": "OPEN"}, 7, None):
        try:
            _, r = evaluate_world(mutate(**{field: shape}))
            got = r.outcome
        except Exception as exc:                              # noqa: BLE001 — that is the point
            got = f"raised {type(exc).__name__}"
        check(f"{field}={shape!r} refuses instead of raising",
              got == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {got}")
for shape in ("not-an-object", 7, []):
    bad_suite = world()
    bad_suite["check_pages"][0][0] = dict(bad_suite["check_pages"][0][0], checkSuite=shape)
    try:
        _, r = evaluate_world(bad_suite)
        got = r.outcome
    except Exception as exc:                                  # noqa: BLE001
        got = f"raised {type(exc).__name__}"
    check(f"a checkSuite of {shape!r} refuses instead of raising",
          got == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {got}")
bad_app = world()
bad_app["check_pages"][0][0] = dict(bad_app["check_pages"][0][0], checkSuite={"app": "nope"})
_, r = evaluate_world(bad_app)
check("an unreadable producing app refuses", r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE,
      f"got {r.outcome}")
bad_app_id = world()
bad_app_id["check_pages"][0][0] = dict(bad_app_id["check_pages"][0][0],
                                       checkSuite={"app": {"databaseId": "15368"}})
_, r = evaluate_world(bad_app_id)
check("an unreadable producer id is not treated as unbound",
      r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {r.outcome}")
expect("a rollup that names no head commit at all", world(rollup_head=None),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)

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
loose = world()
loose["protection"]["strict"] = False
expect("the up-to-date requirement policy relies on is switched off live", loose,
       codes.REFUSED_POLICY_DRIFT)
_, loose_result = evaluate_world(loose)
check("the strict drift refusal says which setting disagrees",
      any("strict_up_to_date" in r.detail for r in loose_result.reasons))
expect("policy that declares strict_up_to_date as something other than a boolean",
       policy_with(lambda d: d["branch"].update(strict_up_to_date="yes")),
       codes.REFUSED_POLICY_INVALID)
silent = policy_with(lambda d: d["branch"].pop("strict_up_to_date"))
silent["protection"]["strict"] = False
expect("a policy that makes no strict claim has no strict drift to report", silent,
       codes.READY)
weakened = policy_with(lambda d: d["branch"].update(required_approvals=1))
expect("live protection erasing an approval requirement policy states", weakened,
       codes.REFUSED_POLICY_DRIFT)
_, weak_result = evaluate_world(weakened)
check("the approval drift refusal says which requirement was erased",
      any("required_approvals" in r.detail for r in weak_result.reasons))
expect("policy that declares required_approvals as something other than an integer",
       policy_with(lambda d: d["branch"].update(required_approvals="one")),
       codes.REFUSED_POLICY_INVALID)
expect("a boolean is not an approval count",
       policy_with(lambda d: d["branch"].update(required_approvals=True)),
       codes.REFUSED_POLICY_INVALID)
no_claim = policy_with(lambda d: d["branch"].pop("required_approvals"))
no_claim["protection"]["required_approving_review_count"] = 0
expect("a policy that makes no approval claim has no approval drift to report", no_claim,
       codes.READY)
print("bypass evidence is enumerated across EVERY page")
from icn_merge_pr.ghclient import GhCli                            # noqa: E402


def paginated(pages, gh_fails=False):
    """Drive the real transport over a `gh api --paginate --slurp` document."""
    client = GhCli()

    def run(argv, on_failure=EvidenceUnavailable):
        check(f"pagination asks for every page ({argv[:3]})",
              "--paginate" in argv and "--slurp" in argv, f"{argv}")
        if gh_fails:
            raise EvidenceUnavailable("gh failed part-way through pagination")
        return pages if isinstance(pages, str) else json.dumps(pages)

    client._run = run
    try:
        return client.branch_rules("o", "n", "main")
    except EvidenceUnavailable as exc:
        return f"REFUSED: {exc.detail}"


two_pages = paginated([[{"type": "pull_request", "ruleset_id": 1}],
                       [{"type": "required_status_checks", "ruleset_id": 99}]])
check("rules spread over two pages are all returned",
      not isinstance(two_pages, str) and [r["ruleset_id"] for r in two_pages] == [1, 99],
      f"{two_pages}")
check("a single empty page is an empty enumeration, not a failure",
      paginated([[]]) == [], f"{paginated([[]])}")
check("a later page that is not a list refuses",
      isinstance(paginated([[{"type": "x", "ruleset_id": 1}], {"not": "a list"}]), str))
check("a document that is not an array of pages refuses",
      isinstance(paginated({"not": "pages"}), str))
check("undecodable pagination output refuses", isinstance(paginated("{not json"), str))
check("gh failing part-way through pagination refuses",
      isinstance(paginated([[]], gh_fails=True), str))

# The gate must act on the COMPLETE enumeration: a bypass actor first seen on page two.
LATER = 99
later_page_bypass = world()
later_page_bypass["branch_rules"] = [{"type": "pull_request", "ruleset_id": 1},
                                     {"type": "required_status_checks", "ruleset_id": LATER}]
later_page_bypass["rulesets_listing"] = [{"id": 1, "name": "first", "enforcement": "active"},
                                         {"id": LATER, "name": "inherited", "enforcement": "active"}]
later_page_bypass["rulesets"] = {
    1: {"id": 1, "name": "first", "enforcement": "active", "bypass_actors": []},
    LATER: {"id": LATER, "name": "inherited", "enforcement": "active",
            "bypass_actors": [{"actor_id": 3, "actor_type": "Team", "bypass_mode": "always"}]}}
expect("an active bypass actor reachable only on a later page", later_page_bypass,
       codes.REFUSED_PROTECTION_BYPASSABLE)
_, later_result = evaluate_world(later_page_bypass)
check("the refusal names the later-page ruleset",
      any("inherited" in r.detail for r in later_result.reasons))
fake, later_merge = merge_world(later_page_bypass)
check("no mutation occurs when a later-page bypass path exists", fake.merge_calls == [])
non_enforcing_later = dict(later_page_bypass)
non_enforcing_later["rulesets"] = dict(later_page_bypass["rulesets"])
non_enforcing_later["rulesets"][LATER] = dict(later_page_bypass["rulesets"][LATER],
                                              enforcement="disabled")
expect("a later-page DISABLED ruleset keeps its established non-active semantics",
       non_enforcing_later, codes.READY)

print("a policy rejecting CHANGES_REQUESTED requires SERVER review protection")
unprotected_reviews = world()
unprotected_reviews["protection"]["review_protection"] = False
expect("policy rejecting change requests while no review protection is configured",
       unprotected_reviews, codes.REFUSED_POLICY_DRIFT)
_, review_result = evaluate_world(unprotected_reviews)
check("the refusal explains the head pin cannot bind review state",
      any("does not change the head SHA" in r.detail and "change request" in r.detail
          for r in review_result.reasons))
for bad in (None, "true", 1, [], {}):
    broken = world()
    broken["protection"]["review_protection"] = bad
    _, r = evaluate_world(broken)
    check(f"a review-protection state of {bad!r} is unreadable, not enforced",
          r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {r.outcome}")


def drop_review_protection(w):
    w["protection"]["review_protection"] = False


fake, review_race = merge_world(world(on_refresh=drop_review_protection))
check("review protection disappearing on the refresh refuses",
      review_race.outcome == codes.REFUSED_POLICY_DRIFT, f"got {review_race.outcome}")
check("no mutation once server review protection is gone", fake.merge_calls == [])
expect("the client-side review gate still refuses an existing change request",
       world(review_pages=[[{"state": "CHANGES_REQUESTED"}]]), codes.REFUSED_REVIEW)

print("the trust pin must be a full Git object id")
for bad in ("refs/pull/7/head", "main", "HEAD", "abc1234", "z" * 40, DEFAULT_OID + " ", "", 7,
            None, []):
    _, r = evaluate_world(world(default_branch_oid_raw=bad))
    check(f"a default-branch tip of {bad!r} is not a usable trust pin",
          r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {r.outcome}")
_, valid_pin = evaluate_world(world())
check("a full 40-hex object id is accepted as the pin",
      valid_pin.evidence["policy"]["loaded_from_oid"] == DEFAULT_OID)

print("a malformed commit-status state refuses instead of crashing")
for shape in ([], {"a": 1}, 7, True, None):
    bad_state = world()
    bad_state["check_pages"][0] = [{"__typename": "StatusContext", "context": REQUIRED[0],
                                    "state": shape}] + bad_state["check_pages"][0][1:]
    try:
        _, r = evaluate_world(bad_state)
        got = r.outcome
    except Exception as exc:                                  # noqa: BLE001 — that is the point
        got = f"raised {type(exc).__name__}"
    check(f"a commit-status state of {shape!r} -> REFUSED_UNAVAILABLE_EVIDENCE",
          got == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {got}")
    fake, _ = merge_world(bad_state)
    check(f"no mutation from a commit-status state of {shape!r}", fake.merge_calls == [])

print("a zero-thread policy requires the SERVER to enforce conversation resolution")
unenforced = world()
unenforced["protection"]["required_conversation_resolution"] = False
expect("policy requiring zero threads while the server does not enforce resolution", unenforced,
       codes.REFUSED_POLICY_DRIFT)
_, unenforced_result = evaluate_world(unenforced)
check("the refusal explains that the head pin cannot bind thread state",
      any("does not change the head SHA" in r.detail for r in unenforced_result.reasons))
for bad in (None, "true", "false", 1, 0, [], {}):
    broken = world()
    broken["protection"]["required_conversation_resolution"] = bad
    _, r = evaluate_world(broken)
    check(f"a conversation-resolution setting of {bad!r} is unreadable, not enforced",
          r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {r.outcome}")
absent_resolution = world()
absent_resolution["protection"].pop("required_conversation_resolution")
expect("live protection reporting nothing about conversation resolution", absent_resolution,
       codes.REFUSED_UNAVAILABLE_EVIDENCE)


def stop_enforcing_resolution(w):
    w["protection"]["required_conversation_resolution"] = False


fake, resolution_race = merge_world(world(on_refresh=stop_enforcing_resolution))
check("server enforcement dropping on the refresh refuses",
      resolution_race.outcome == codes.REFUSED_POLICY_DRIFT, f"got {resolution_race.outcome}")
check("no mutation occurs once the server stops enforcing resolution", fake.merge_calls == [])
# ONE owner: the requirement is derived from policy, so a policy that permitted unresolved threads
# would not demand an enforcement it no longer needs. (The landed schema pins 0, so this is a
# statement about the derivation, exercised through a mutated policy document.)
permissive = policy_with(lambda d: d["merge"]["ready_when"].update(unresolved_review_threads=1))
permissive["protection"]["required_conversation_resolution"] = False
_, permissive_result = evaluate_world(permissive)
check("the derived requirement is not asked for when policy does not need it",
      permissive_result.outcome == codes.REFUSED_POLICY_INVALID,
      f"got {permissive_result.outcome}")
still_gated = world()
still_gated["thread_pages"] = [[{"isResolved": False}]]
expect("the client-side thread gate still refuses an existing unresolved thread", still_gated,
       codes.REFUSED_THREADS)

print("the ordinary merger mutates only when NO server-side bypass path exists")


def with_ruleset(enforcement, actors, ruleset_id=7, name="a ruleset"):
    w = world()
    w["branch_rules"] = [{"type": "pull_request", "ruleset_id": ruleset_id,
                          "ruleset_source_type": "Organization"}]
    w["rulesets_listing"] = [{"id": ruleset_id, "name": name, "enforcement": enforcement}]
    w["rulesets"] = {ruleset_id: {"id": ruleset_id, "name": name, "enforcement": enforcement,
                                  "bypass_actors": actors}}
    return w


ACTOR = [{"actor_id": 5, "actor_type": "Team", "bypass_mode": "always"}]

for kind in ("users", "teams", "apps"):
    grant = world()
    grant["protection"]["bypass_allowances"] = [f"{kind}:someone"]
    expect(f"a classic pull-request bypass allowance for {kind}", grant,
           codes.REFUSED_PROTECTION_BYPASSABLE)
_, grant_result = evaluate_world(
    dict(world(), protection=dict(world()["protection"], bypass_allowances=["teams:reviewers"])))
check("the refusal lists the configured bypass path, whoever it belongs to",
      any("teams:reviewers" in r.detail for r in grant_result.reasons))
check("the refusal says an ordinary merger requires no bypass path at all",
      any("no server-side bypass path exists at all" in r.detail
          for r in grant_result.reasons))

expect("an ACTIVE ruleset carrying a bypass actor", with_ruleset("active", ACTOR),
       codes.REFUSED_PROTECTION_BYPASSABLE)
expect("an inherited applicable ruleset carrying a bypass actor",
       with_ruleset("active", ACTOR, ruleset_id=99, name="org-wide"),
       codes.REFUSED_PROTECTION_BYPASSABLE)
# Exact semantics, documented by test: a bypass actor on a ruleset that does not ENFORCE cannot
# open a path, because that ruleset gates nothing in the first place.
expect("a DISABLED ruleset carrying a bypass actor is not an open path",
       with_ruleset("disabled", ACTOR), codes.READY)
expect("an EVALUATE-mode ruleset carrying a bypass actor is not an open path",
       with_ruleset("evaluate", ACTOR), codes.READY)
expect("an active ruleset with no bypass actors", with_ruleset("active", []), codes.READY)
expect("an active ruleset reporting no bypass_actors field at all",
       with_ruleset("active", None), codes.READY)

expect("an enforcement mode this program does not know", with_ruleset("shadow", []),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("bypass actors that are not a list", with_ruleset("active", "nobody"),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("a bypass actor that is not an object", with_ruleset("active", ["someone"]),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("ruleset enumeration unavailable", world(rulesets_error="403 Forbidden"),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
expect("branch-rule enumeration unavailable", world(branch_rules_error="502 Bad Gateway"),
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
untraceable = world()
untraceable["branch_rules"] = [{"type": "pull_request"}]
expect("a rule in force that cannot be traced to a ruleset", untraceable,
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
unreadable_detail = with_ruleset("active", [])
unreadable_detail["ruleset_error"] = "404 Not Found"
expect("an applicable ruleset whose detail cannot be read", unreadable_detail,
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
for bad in (None, "none", {}, [1]):
    malformed = world()
    malformed["protection"]["bypass_allowances"] = bad
    _, r = evaluate_world(malformed)
    check(f"classic bypass evidence of {bad!r} is unreadable, not absent",
          r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {r.outcome}")


def grant_bypass(w):
    w["protection"]["bypass_allowances"] = ["users:someone"]


fake, bypass_appeared = merge_world(world(on_refresh=grant_bypass))
check("a bypass actor appearing on the refresh -> REFUSED_PROTECTION_BYPASSABLE",
      bypass_appeared.outcome == codes.REFUSED_PROTECTION_BYPASSABLE,
      f"got {bypass_appeared.outcome}")
check("no merge was attempted once a bypass path appeared", fake.merge_calls == [])
_, clean_bypass = evaluate_world(world())
check("a repository with no bypass paths reports none",
      clean_bypass.evidence["bypass"]["open_paths"] == [], f"{clean_bypass.evidence['bypass']}")

print("an ordinary merge is ordinary only if the server enforces protection on the caller")
bypassable = world()
bypassable["protection"]["enforce_admins"] = False
expect("live protection that does not apply to bypass-capable roles", bypassable,
       codes.REFUSED_PROTECTION_BYPASSABLE)
_, bypass_result = evaluate_world(bypassable)
check("the refusal explains that the SERVER must re-enforce protection",
      any("enforces protection against the caller" in r.detail for r in bypass_result.reasons))
for bad in (None, "true", "false", 1, 0, [], {}):
    unreadable = world()
    unreadable["protection"]["enforce_admins"] = bad
    _, r = evaluate_world(unreadable)
    check(f"an enforce_admins setting of {bad!r} is unreadable evidence, not enforcement",
          r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {r.outcome}")
missing_admins = world()
missing_admins["protection"].pop("enforce_admins")
expect("live protection that reports nothing about bypass at all", missing_admins,
       codes.REFUSED_UNAVAILABLE_EVIDENCE)
policy_says_bypassable = policy_with(lambda d: d["branch"].update(enforce_admins=False))
expect("policy declaring bypassable protection while live enforces it", policy_says_bypassable,
       codes.REFUSED_POLICY_DRIFT)
expect("policy that declares enforce_admins as something other than a boolean",
       policy_with(lambda d: d["branch"].update(enforce_admins="yes")),
       codes.REFUSED_POLICY_INVALID)


def drop_enforcement(w):
    w["protection"]["enforce_admins"] = False


fake, bypass_race = merge_world(world(on_refresh=drop_enforcement))
check("protection becoming bypassable on the refresh -> REFUSED_PROTECTION_BYPASSABLE",
      bypass_race.outcome == codes.REFUSED_PROTECTION_BYPASSABLE, f"got {bypass_race.outcome}")
check("no merge was attempted once protection stopped applying to the caller",
      fake.merge_calls == [])

other_protection_objects = world()
other_protection_objects["protection"]["required_approving_review_count"] = 0
expect("the drift gate does not reach into protection objects policy does not own",
       other_protection_objects, codes.READY)

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

# A lost answer is not a refusal, and a read taken straight afterwards cannot make it one.
lost_answer = world(merge_transport_lost="context deadline exceeded",
                    post_merge={"state": "OPEN", "merged": False, "merge_commit_sha": None})
fake, result = merge_world(lost_answer)
check("a transport failure with an immediate negative read -> MERGE_UNCONFIRMED, not a refusal",
      result.outcome == codes.MERGE_UNCONFIRMED, f"got {result.outcome}")
check("the refusal explains that a point-in-time read cannot prove a dispatched request finished",
      any("cannot " in r.detail and "finished" in r.detail for r in result.reasons))
check("a lost answer still issued exactly one request", len(fake.merge_calls) == 1)
_, result = merge_world(world(merge_transport_lost="connection reset",
                              post_merge={"state": "MERGED", "merged": True,
                                          "merge_commit_sha": MERGE_COMMIT}))
check("a transport failure whose PR now reads MERGED -> MERGE_UNCONFIRMED",
      result.outcome == codes.MERGE_UNCONFIRMED, f"got {result.outcome}")

unconfirmed = world(post_merge={"state": "OPEN", "merged": False, "merge_commit_sha": None})
fake, result = merge_world(unconfirmed)
check("API success with a post-read that does not say merged -> MERGE_UNCONFIRMED",
      result.outcome == codes.MERGE_UNCONFIRMED, f"got {result.outcome}")
check("an unconfirmed merge is not success", codes.exit_code(result.outcome) != codes.EXIT_OK)
check("an unconfirmed merge does not claim confirmation",
      result.merge["confirmed_merged"] is False, f"{result.merge}")

# `null` and `[]` are valid JSON and neither is a merge response. Crashing on one AFTER the
# mutation was dispatched is the failure this program exists to avoid.
for shape in (None, [], "ok", 7):
    odd = world(merge_response=shape,
                post_merge={"state": "OPEN", "merged": False, "merge_commit_sha": None})
    try:
        _, result = merge_world(odd)
        got = result.outcome
    except Exception as exc:                                  # noqa: BLE001 — that is the point
        got = f"raised {type(exc).__name__}"
    check(f"a merge response of {shape!r} reports an outcome instead of raising",
          got == codes.MERGE_UNCONFIRMED, f"got {got}")

print("MERGED requires the merge commit's identity, not just a merged flag")
for label, sha in (("null", None), ("missing", "__omit__"), ("an integer", 7),
                   ("a list", []), ("an object", {}), ("an empty string", ""),
                   ("a short sha", "abc1234")):
    post = {"state": "MERGED", "merged": True}
    if sha != "__omit__":
        post["merge_commit_sha"] = sha
    fake, r = merge_world(world(post_merge=post, merge_response={"merged": True}))
    check(f"merged with {label} commit identity -> MERGE_UNCONFIRMED",
          r.outcome == codes.MERGE_UNCONFIRMED, f"got {r.outcome}")
    check(f"still exactly one mutation with {label} commit identity",
          len(fake.merge_calls) == 1, f"{fake.merge_calls}")
_, identity = merge_world(world(post_merge={"state": "MERGED", "merged": True,
                                            "merge_commit_sha": None},
                                merge_response={"merged": True}))
check("the detail says the merge happened but its identity is missing",
      any("merge commit identity could not be established" in r.detail
          for r in identity.reasons))
check("an unestablished identity is never reported as a refusal",
      not identity.outcome.startswith("REFUSED"), identity.outcome)
_, fallback = merge_world(world(post_merge={"state": "MERGED", "merged": True,
                                            "merge_commit_sha": None},
                                merge_response={"merged": True, "sha": MERGE_COMMIT}))
check("a valid merge-response sha is accepted when the post-read omits one",
      fallback.outcome == codes.MERGED
      and fallback.merge["merge_commit_sha"] == MERGE_COMMIT, f"{fallback.merge}")

print("a malformed pull request object refuses instead of crashing")
for shape in ([], "x", 7, 0.5):
    try:
        _, r = evaluate_world(world(pr_object=shape))
        got = r.outcome
    except Exception as exc:                                  # noqa: BLE001 — that is the point
        got = f"raised {type(exc).__name__}"
    check(f"an initial pull request of {shape!r} -> REFUSED_UNAVAILABLE_EVIDENCE",
          got == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {got}")
    fake, _ = merge_world(world(pr_object=shape))
    check(f"no mutation from an initial pull request of {shape!r}", fake.merge_calls == [])

print("a malformed check-run status refuses instead of crashing")
for shape in ([], {"a": 1}, 7, True, None):
    bad_status = world()
    bad_status["check_pages"][0][0] = dict(bad_status["check_pages"][0][0], status=shape)
    try:
        _, r = evaluate_world(bad_status)
        got = r.outcome
    except Exception as exc:                                  # noqa: BLE001
        got = f"raised {type(exc).__name__}"
    check(f"a check-run status of {shape!r} -> REFUSED_UNAVAILABLE_EVIDENCE",
          got == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {got}")
    fake, _ = merge_world(bad_status)
    check(f"no mutation from a check-run status of {shape!r}", fake.merge_calls == [])

print("a malformed post-read object cannot crash after dispatch")
for shape in ([], "x", 7, 0.5):
    fake, r = merge_world(world(post_merge_object=shape))
    check(f"a post-read pull request of {shape!r} -> MERGE_UNCONFIRMED",
          r.outcome == codes.MERGE_UNCONFIRMED, f"got {r.outcome}")
    check(f"exactly one merge request preceded the bad post-read ({shape!r})",
          len(fake.merge_calls) == 1, f"{fake.merge_calls}")

print("ruleset enforcement is a string before it is a set member")
for shape in ([], {"a": 1}, 7, True, None):
    try:
        _, r = evaluate_world(with_ruleset(shape, []))
        got = r.outcome
    except Exception as exc:                                  # noqa: BLE001 — that is the point
        got = f"raised {type(exc).__name__}"
    check(f"enforcement of {shape!r} refuses instead of raising",
          got == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {got}")

print("a refusal is confirmed only by an exact merged=false")
for label, value in (("null", None), ("missing", "__omit__"), ("the string 'false'", "false"),
                     ("0", 0), ("a list", []), ("an object", {})):
    post = {"state": "OPEN", "merge_commit_sha": None}
    if value != "__omit__":
        post["merged"] = value
    fake, r = merge_world(world(merge_refused="405 Method Not Allowed (HTTP 405)",
                                post_merge=post))
    check(f"a 4xx refusal with a merged flag that is {label} -> MERGE_UNCONFIRMED",
          r.outcome == codes.MERGE_UNCONFIRMED, f"got {r.outcome}")
    check(f"malformed merged evidence ({label}) is never read as false",
          not any("confirms PR" in x.detail for x in r.reasons))
    check(f"still exactly one mutation with a {label} merged flag", len(fake.merge_calls) == 1)
_, exact_false = merge_world(world(merge_refused="405 Method Not Allowed (HTTP 405)",
                                   post_merge={"state": "OPEN", "merged": False,
                                               "merge_commit_sha": None}))
check("an exact merged=false supports a confirmed REFUSED_GITHUB",
      exact_false.outcome == codes.REFUSED_GITHUB, f"got {exact_false.outcome}")
_, exact_true = merge_world(world(merge_refused="405 Method Not Allowed (HTTP 405)",
                                  post_merge={"state": "MERGED", "merged": True,
                                              "merge_commit_sha": MERGE_COMMIT}))
check("an exact merged=true keeps the attribution-safe unconfirmed path",
      exact_true.outcome == codes.MERGE_UNCONFIRMED, f"got {exact_true.outcome}")

# The residual base-retarget race is a PLATFORM limitation, accepted and documented. These pin the
# claim so a later change cannot quietly start asserting the base is atomically bound.
merge_doc = (ROOT / "tools" / "icn-merge-pr" / "icn_merge_pr" / "merge.py").read_text(
    encoding="utf-8")
check("the mutation documents that GitHub exposes no expected-base precondition",
      "expected-BASE precondition" in merge_doc and "expectedHeadOid" in merge_doc)
flat_doc = " ".join(merge_doc.split())          # the claim is wrapped across lines
check("the mutation does not claim the verified base is the base GitHub merges into",
      "does NOT guarantee that the base it verified is the base GitHub merges into" in flat_doc)
fake, merged_ok = merge_world(world())
check("the merge request pins the head and names no base precondition",
      fake.merge_calls == [{"number": 1, "sha": HEAD, "merge_method": "squash"}],
      f"{fake.merge_calls}")

null_merged = world(post_merge={"state": "OPEN", "merged": None, "merge_commit_sha": None})
_, result = merge_world(null_merged)
check("a post-read that reports nothing about merging is not success",
      result.outcome == codes.MERGE_UNCONFIRMED, f"got {result.outcome}")

fake, _ = merge_world(mutate(isDraft=True))
check("a refused evaluation never reaches the mutation", fake.merge_calls == [])
fake, _ = evaluate_world(world())
check("`check` never mutates even when everything is green", fake.merge_calls == [])

print("an evaluator the default branch has moved past does not merge")
INSTALLED = "5" * 40
_, current = merge_world(world(), installed_commit=INSTALLED)
check("an installed commit behind an unchanged evaluator still merges",
      current.outcome == codes.MERGED, f"got {current.outcome}")
fake, unrelated = merge_world(world(), installed_commit=DEFAULT_OID)
check("an installed commit equal to the live tip needs no comparison at all",
      unrelated.outcome == codes.MERGED, f"got {unrelated.outcome}")

changed = world(object_oids={(INSTALLED, "tools/icn-merge-pr"): "old-tree",
                             (DEFAULT_OID, "tools/icn-merge-pr"): "new-tree"})
fake, stale = merge_world(changed, installed_commit=INSTALLED)
check("the evaluator's own source changing on the default branch -> REFUSED_EVALUATOR_STALE",
      stale.outcome == codes.REFUSED_EVALUATOR_STALE, f"got {stale.outcome}")
check("no merge was attempted with a stale evaluator", fake.merge_calls == [])
check("the refusal names the path that changed and tells the operator to reinstall",
      any("tools/icn-merge-pr" in r.detail and "reinstall" in r.detail for r in stale.reasons))

validator_changed = world(
    object_oids={(INSTALLED, "scripts/check-merge-policy-schema.py"): "old-blob",
                 (DEFAULT_OID, "scripts/check-merge-policy-schema.py"): "new-blob"})
_, stale2 = merge_world(validator_changed, installed_commit=INSTALLED)
check("the vendored policy validator changing is also staleness",
      stale2.outcome == codes.REFUSED_EVALUATOR_STALE, f"got {stale2.outcome}")

vanished = world(object_oids={(INSTALLED, "tools/icn-merge-pr"): None})
_, stale3 = merge_world(vanished, installed_commit=INSTALLED)
check("a path that cannot be resolved at either commit counts as changed",
      stale3.outcome == codes.REFUSED_EVALUATOR_STALE, f"got {stale3.outcome}")
_, checked = evaluate_world(world(object_oids={(INSTALLED, "tools/icn-merge-pr"): "old-tree",
                                               (DEFAULT_OID, "tools/icn-merge-pr"): "new-tree"}))
check("evaluation is never blocked by staleness — it mutates nothing",
      checked.outcome == codes.READY, f"got {checked.outcome}")

print("review and protection evidence must be readable")
expect("an opinionated review state this program does not know",
       world(review_pages=[[{"state": "SOMETHING_NEW"}]]), codes.REFUSED_UNAVAILABLE_EVIDENCE)
for bad in (None, "false", "true", 1, 0, []):
    broken = world()
    broken["protection"]["strict"] = bad
    _, r = evaluate_world(broken)
    check(f"a strict setting of {bad!r} is unreadable evidence, not a satisfied requirement",
          r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {r.outcome}")
for bad in (None, "two", True, 1.5, [], {}):
    broken = world()
    broken["protection"]["required_approving_review_count"] = bad
    _, r = evaluate_world(broken)
    check(f"an approving-review count of {bad!r} is unreadable evidence, not zero",
          r.outcome == codes.REFUSED_UNAVAILABLE_EVIDENCE, f"got {r.outcome}")

print("branch protection is read, never degraded")
from icn_merge_pr.ghclient import GhCli                            # noqa: E402


def read_protection(required_status_checks, enforce_admins={"enabled": True}, reviews=None,
                    resolution={"enabled": True}):
    """Drive the real transport parser over one branch-protection document."""
    client = GhCli()
    doc = {"required_status_checks": required_status_checks, "enforce_admins": enforce_admins,
           "required_conversation_resolution": resolution}
    if reviews is not None:
        doc["required_pull_request_reviews"] = reviews
    client._rest = lambda path: doc
    try:
        return client.branch_protection("o", "n", "main")
    except EvidenceUnavailable as exc:
        return f"REFUSED: {exc.detail}"


for label, doc in (
    ("a check entry with no context", {"checks": [{}], "strict": True}),
    ("a check collection that is not a list", {"checks": {"a": 1}, "strict": True}),
    ("a check entry that is not an object", {"checks": ["Build"], "strict": True}),
    ("a legacy list holding a non-string", {"contexts": ["Build", 7], "strict": True}),
    ("a strict flag that is not a boolean", {"checks": [], "strict": "true"}),
):
    got = read_protection(doc)
    check(f"{label} is unreadable evidence", isinstance(got, str), f"got {got}")

for label, admins in (("an enforce_admins object with no enabled flag", {}),
                      ("an enforce_admins flag that is a string", {"enabled": "true"}),
                      ("no enforce_admins key at all", None)):
    got = read_protection({"checks": [], "strict": True}, enforce_admins=admins)
    check(f"{label} is unreadable evidence", isinstance(got, str), f"got {got}")
for label, res in (("an object with no enabled flag", {}), ("a string flag", {"enabled": "yes"}),
                   ("no key at all", None)):
    got = read_protection({"checks": [], "strict": True}, resolution=res)
    check(f"conversation resolution as {label} is unreadable evidence", isinstance(got, str),
          f"got {got}")
resolved = read_protection({"checks": [], "strict": True}, resolution={"enabled": True})
check("a readable conversation-resolution flag is carried through",
      not isinstance(resolved, str) and resolved["required_conversation_resolution"] is True,
      f"{resolved}")

enforced = read_protection({"checks": [], "strict": True}, enforce_admins={"enabled": True})
check("a readable enforce_admins flag is carried through",
      not isinstance(enforced, str) and enforced["enforce_admins"] is True, f"{enforced}")

# `checks` PRESENT is authoritative even when empty: falling through to the legacy array would
# unbind every producer, which is the degradation this rejects.
empty_modern = read_protection({"checks": [], "contexts": ["Build Release"], "strict": True})
check("an empty modern collection does not fall back to the legacy array",
      empty_modern["required_contexts"] == [], f"{empty_modern}")
legacy_only = read_protection({"contexts": ["Build Release"], "strict": True})
check("a legacy-only document is read, with no producer bound",
      legacy_only["required_contexts"] == ["Build Release"]
      and legacy_only["required_bindings"] == {"Build Release": None}, f"{legacy_only}")
conflicting = read_protection({"checks": [{"context": "Test", "app_id": 15368},
                                          {"context": "Test", "app_id": 99999}], "strict": True})
check("a context named twice with different producers is unavailable evidence",
      isinstance(conflicting, str) and "different producers" in conflicting, f"{conflicting}")
repeated = read_protection({"checks": [{"context": "Test", "app_id": 15368},
                                       {"context": "Test", "app_id": 15368}], "strict": True})
check("an exact repeat states the same requirement twice and is harmless",
      not isinstance(repeated, str) and repeated["required_contexts"] == ["Test"]
      and repeated["required_bindings"] == {"Test": 15368}, f"{repeated}")

bound = read_protection({"checks": [{"context": "Build Release", "app_id": 15368},
                                    {"context": "Test", "app_id": -1}], "strict": True})
check("producers are carried through, and -1 means any",
      bound["required_bindings"] == {"Build Release": 15368, "Test": None}, f"{bound}")

print("classic bypass allowances are read, and every actor kind is inspected")
plain = {"checks": [], "strict": True}
for label, reviews in (
    ("a user allowance", {"required_approving_review_count": 0,
                          "bypass_pull_request_allowances": {"users": [{"login": "someone"}]}}),
    ("a team allowance", {"required_approving_review_count": 0,
                          "bypass_pull_request_allowances": {"teams": [{"slug": "reviewers"}]}}),
    ("an app allowance", {"required_approving_review_count": 0,
                          "bypass_pull_request_allowances": {"apps": [{"slug": "a-bot"}]}}),
    ("an actor kind nobody enumerated", {"required_approving_review_count": 0,
                                         "bypass_pull_request_allowances":
                                             {"custom_roles": [{"name": "releaser"}]}}),
):
    got = read_protection(plain, reviews=reviews)
    check(f"{label} is carried through as an open path",
          not isinstance(got, str) and len(got["bypass_allowances"]) == 1, f"{got}")
empty = read_protection(plain, reviews={"required_approving_review_count": 0,
                                        "bypass_pull_request_allowances":
                                            {"users": [], "teams": [], "apps": []}})
check("structurally empty allowances are no path at all",
      not isinstance(empty, str) and empty["bypass_allowances"] == [], f"{empty}")
absent = read_protection(plain, reviews={"required_approving_review_count": 0})
check("absent allowances are no path at all",
      not isinstance(absent, str) and absent["bypass_allowances"] == [], f"{absent}")
for bad in ("everyone", 7, [{"login": "x"}]):
    got = read_protection(plain, reviews={"required_approving_review_count": 0,
                                          "bypass_pull_request_allowances": bad})
    check(f"bypass allowances of {bad!r} are unreadable evidence", isinstance(got, str), f"{got}")

print("repository merge-method metadata must be exact booleans")
from icn_merge_pr.ghclient import GhCli as _Gh                     # noqa: E402


def read_repo_metadata(**fields):
    client = _Gh()
    base = {"defaultBranchRef": {"name": "main", "target": {"oid": "a" * 40}},
            "mergeCommitAllowed": False, "squashMergeAllowed": True, "rebaseMergeAllowed": True}
    base.update(fields)
    client._graphql = lambda query, variables: base
    try:
        return client.repository_metadata("o", "n")
    except EvidenceUnavailable as exc:
        return f"REFUSED: {exc.detail}"


for field in ("mergeCommitAllowed", "squashMergeAllowed", "rebaseMergeAllowed"):
    for bad in ("false", "true", 0, 1, None, [], {}):
        got = read_repo_metadata(**{field: bad})
        check(f"{field}={bad!r} is unreadable evidence, not a permitted method",
              isinstance(got, str), f"got {got}")
good = read_repo_metadata()
check("readable merge-method metadata is carried through exactly",
      not isinstance(good, str) and good["merge_allowed"] is False
      and good["squash_allowed"] is True and good["rebase_allowed"] is True, f"{good}")

print("a server error is not a decision")
from icn_merge_pr.ghclient import definitive_http_failure          # noqa: E402
for detail, definitive in (("gh: Method Not Allowed (HTTP 405)", True),
                           ("Conflict (HTTP 409)", True),
                           ("Unprocessable Entity (HTTP 422)", True),
                           ("Internal Server Error (HTTP 500)", False),
                           ("Bad Gateway (HTTP 502)", False),
                           ("Service Unavailable (HTTP 503)", False),
                           ("Gateway Timeout (HTTP 504)", False),
                           ("connection reset by peer", False),
                           ("", False)):
    verb = "is a decision" if definitive else "is NOT a decision"
    check(f"{detail!r} {verb}", definitive_http_failure(detail) is definitive)

print("the bootstrap recipe does not stage through a predictable path")
skill_text = (ROOT / ".agents" / "skills" / "merge-pr" / "SKILL.md").read_text(encoding="utf-8")
readme = (ROOT / "tools" / "icn-merge-pr" / "README.md").read_text(encoding="utf-8")
for name, text in (("the skill", skill_text), ("the README", readme)):
    check(f"{name} stages the trusted installer in a private temporary directory",
          "mktemp -d" in text and "/tmp/icn-install.py" not in text)
    check(f"{name} takes the installer from the default-branch ref, not the working tree",
          "show origin/" in text)

print("the exit-code contract does not invite a retry")
for name, text in (("the CLI usage text", cli.USAGE), ("the README", readme)):
    check(f"{name} says exit 1 covers MERGE_UNCONFIRMED, not refusal alone",
          "MERGE_UNCONFIRMED" in text and "Exit 1" in text)
# A malformed installed commit must never reach the staleness report's string slicing.
_, typed = merge_world(world(), installed_commit=7)
check("a non-string installed commit refuses instead of raising",
      typed.outcome == codes.REFUSED_NOT_INSTALLED, f"got {typed.outcome}")
fake, typed = merge_world(world(), installed_commit=7)
check("no merge is attempted from a record this program cannot read", fake.merge_calls == [])

# The bootstrap must be able to refuse before it may import anything, so it carries its own copy
# of this code spelling. The two must agree.
bootstrap = (ROOT / "tools" / "icn-merge-pr" / "icn_merge_pr" / "__main__.py").read_text(
    encoding="utf-8")
check("the bootstrap's refusal code matches the shared vocabulary",
      f'_REFUSED = "{codes.REFUSED_NOT_INSTALLED}"' in bootstrap)
check("the bootstrap does not import the package before verifying the tree",
      bootstrap.index("_verify_closed_tree()") < bootstrap.index("from icn_merge_pr.cli import"))
check("the bootstrap seals the import path before verifying",
      bootstrap.index("_seal_import_path()") < bootstrap.index("sys.path.insert(0, _LIB)"))

check("both exit codes for MERGED and MERGE_UNCONFIRMED differ from each other",
      codes.exit_code(codes.MERGED) != codes.exit_code(codes.MERGE_UNCONFIRMED))

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
