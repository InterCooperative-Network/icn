#!/usr/bin/env python3
"""Controls for scripts/check-merge-policy-schema.py (icn#2651).

This tests the VALIDATOR, by calling `validate()` on mutated policy objects. It does not read or
parse any Markdown, and it does not assert anything about how a skill is worded — those were the
representation mistakes that made the previous merge-policy suite grow past 200 string assertions
while real defects kept getting through (icn#2656).

Every MUST-FAIL case below is either a reconstruction of real pre-fix state or a specific
fail-open a reviewer found. The MUST-PASS controls prove the validator is not simply rejecting
everything.

Run: python3 scripts/tests/test_merge_policy_schema.py
"""

import copy
import importlib.util
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "check_merge_policy_schema", ROOT / "scripts" / "check-merge-policy-schema.py")
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)
validate = _mod.validate

BASE = json.loads((ROOT / "ops" / "state" / "truth" / "policy.json").read_text(encoding="utf-8"))

failures = []


def check(desc, cond):
    print(f"  {'ok  ' if cond else 'FAIL'} {desc}")
    if not cond:
        failures.append(desc)


def mutated(fn):
    """A deep copy of the real policy with `fn` applied."""
    d = copy.deepcopy(BASE)
    fn(d)
    return d


def must_fail(desc, fn, *, expect=None):
    errs = validate(mutated(fn))
    hit = bool(errs) and (expect is None or any(expect in e for e in errs))
    check(f"MUST FAIL: {desc}" + ("" if hit else f"  (got: {errs[:2]})"), hit)


def must_pass(desc, fn):
    errs = validate(mutated(fn))
    check(f"MUST PASS: {desc}" + ("" if not errs else f"  (got: {errs[:2]})"), not errs)


print("the live policy is valid")
check("the committed policy passes its own validator", not validate(BASE))

# --- strategy is a closed set owned by CODE, not by the document ------------------------------
print("strategy is a closed enum")
must_fail("default_strategy 'admin' (would reconstruct --admin)",
          lambda d: d["merge"].update(default_strategy="admin"), expect="default_strategy")
must_fail("default_strategy 'auto'", lambda d: d["merge"].update(default_strategy="auto"))
must_fail("an arbitrary/unknown strategy",
          lambda d: d["merge"].update(default_strategy="fast-forward"))
must_fail("default_strategy widened by adding it to the file itself",
          lambda d: d["merge"].update(default_strategy="admin", strategy_enum=["admin"]))
must_fail("exception.strategy 'admin'",
          lambda d: d["merge"]["exception"].update(strategy="admin"), expect="exception.strategy")
must_fail("exception reverted to prose",
          lambda d: d["merge"].update(exception="Subtree merges must use --merge."),
          expect="exception")
must_fail("exception that is not an exception (same as default)",
          lambda d: d["merge"]["exception"].update(strategy=d["merge"]["default_strategy"]))
must_fail("strategy_note removed", lambda d: d["merge"].pop("strategy_note"))
for s in ("merge", "squash", "rebase"):
    must_pass(f"a real strategy {s!r} is accepted",
              lambda d, s=s: d["merge"].update(
                  default_strategy=s,
                  exception={**d["merge"]["exception"],
                             "strategy": next(iter({"merge", "squash", "rebase"} - {s}))}))

# --- the original icn#2651 defects, reconstructed from main ------------------------------------
print("the pre-#2651 shapes on main are rejected")
must_fail("admin_bypass.condition prose (`mergeStateStatus=MERGEABLE`, unsatisfiable)",
          lambda d: d["merge"].__setitem__("admin_bypass", {
              "allowed": True,
              "condition": "Required checks green AND mergeStateStatus=MERGEABLE but stalled",
              "never_for": "Bypassing genuinely failing required checks"}))
must_fail("auto_merge.command baking --squash",
          lambda d: d["merge"].__setitem__("auto_merge", {
              "command": "gh pr merge <N> --auto --squash",
              "use_when": "Required checks are still pending (not failed)"}))
must_fail("readiness prose restating the enum values",
          lambda d: d["readiness_definition"].__setitem__(
              4, "mergeStateStatus is MERGEABLE or UNSTABLE"), expect="enum values")
must_fail("readiness prose restating the required-check count",
          lambda d: d["readiness_definition"].__setitem__(0, "All 11 required CI checks passed"),
          expect="count")

# --- cross-enum leakage, as a value comparison -------------------------------------------------
print("no cross-enum leakage")
must_fail("MergeableState value in ready_when.merge_state_status_in",
          lambda d: d["merge"]["ready_when"].update(merge_state_status_in=["MERGEABLE", "UNSTABLE"]))
must_fail("MergeableState value in the bypass gate",
          lambda d: d["merge"]["admin_bypass"]["requires"].update(
              merge_state_status_in=["MERGEABLE"]))
must_fail("a non-MergeableState `mergeable`",
          lambda d: d["merge"]["ready_when"].update(mergeable="CLEAN"))
must_fail("ordinary gate without CLEAN (the state it exists for)",
          lambda d: d["merge"]["ready_when"].update(merge_state_status_in=["UNSTABLE"]))
must_fail("bypass gate WITH CLEAN (a clean PR needs no bypass)",
          lambda d: d["merge"]["admin_bypass"]["requires"].update(
              merge_state_status_in=["CLEAN", "BLOCKED"]))

# --- allowlists must be allowlists, and on the right axis --------------------------------------
print("allowlists are fail-closed and on the right axis")
must_fail("a failing conclusion allowlisted",
          lambda d: d["merge"]["ready_when"]["required_check_conclusion_allowlist"].append("FAILURE"))
must_fail("a PENDING value in the CONCLUSION allowlist",
          lambda d: d["merge"]["ready_when"]["required_check_conclusion_allowlist"].append("PENDING"))
must_fail("IN_PROGRESS in the conclusion allowlist",
          lambda d: d["merge"]["admin_bypass"]["requires"]
                     ["required_check_conclusion_allowlist"].append("IN_PROGRESS"))
must_fail("a review-decision DENYLIST",
          lambda d: d["merge"]["ready_when"].update(
              review_decision_not_in=["CHANGES_REQUESTED"],
              **{"review_decision_allowlist": None}))
must_fail("CHANGES_REQUESTED allowlisted",
          lambda d: d["merge"]["ready_when"]["review_decision_allowlist"].append("CHANGES_REQUESTED"))
must_fail("the live null review decision admitted only by omission",
          lambda d: d["merge"]["ready_when"].update(review_decision_allowlist=["APPROVED"]))
must_fail("unresolved threads tolerated",
          lambda d: d["merge"]["ready_when"].update(unresolved_review_threads=1))
must_fail("a draft PR tolerated", lambda d: d["merge"]["ready_when"].update(is_draft=True))
must_fail("ready_when growing a pending allowlist (pending is not ready)",
          lambda d: d["merge"]["ready_when"].update(required_check_pending_allowlist=["QUEUED"]))

# --- the gate must stay structured -------------------------------------------------------------
print("the gate stays structured")
must_fail("an operative claim smuggled into the gate as prose",
          lambda d: d["merge"]["ready_when"].update(
              condition="merge when mergeStateStatus is MERGEABLE"))
must_fail("an operative field removed",
          lambda d: d["merge"]["ready_when"].pop("required_check_conclusion_allowlist"))
must_fail("an operative field with the wrong type",
          lambda d: d["merge"]["ready_when"].update(unresolved_review_threads="none"))
must_pass("an added *note key is fine (explanation is not operative)",
          lambda d: d["merge"]["ready_when"].update(extra_note="why this exists"))

# --- deferral: an ordinary merge completes or refuses, it never arms ---------------------------
print("no deferred merge is tolerated")
must_fail("merge-queue presence tolerated",
          lambda d: d["merge"]["ready_when"]["not_deferred"].update(merge_queue_absent=False))
must_fail("an existing auto-merge request tolerated",
          lambda d: d["merge"]["ready_when"]["not_deferred"].update(auto_merge_request_absent=False))
must_fail("queue membership tolerated",
          lambda d: d["merge"]["ready_when"]["not_deferred"].update(is_in_merge_queue=True))
must_fail("not_deferred removed entirely",
          lambda d: d["merge"]["ready_when"].pop("not_deferred"))

# --- the queue-stall qualifier (ADR-0016) ------------------------------------------------------
print("the queue-stall qualifier matches ADR-0016")
must_fail("IN_PROGRESS in the bypass pending allowlist",
          lambda d: d["merge"]["admin_bypass"]["requires"]
                     ["required_check_pending_allowlist"].append("IN_PROGRESS"))
must_fail("IN_PROGRESS qualifying as a stall",
          lambda d: d["merge"]["admin_bypass"]["requires"]["stalled_required_check"]
                     ["qualifying_states"].append("IN_PROGRESS"))
must_fail("started states no longer excluded by name",
          lambda d: d["merge"]["admin_bypass"]["requires"]["stalled_required_check"]
                     .update(excluded_states=[]))
must_fail("qualifying_states diverging from the pending allowlist",
          lambda d: d["merge"]["admin_bypass"]["requires"]["stalled_required_check"]
                     .update(qualifying_states=["QUEUED"]))
must_fail("the stall threshold weakened",
          lambda d: d["merge"]["admin_bypass"]["requires"]["stalled_required_check"]
                     .update(min_pending_minutes=1))
must_fail("the whole stall qualifier removed",
          lambda d: d["merge"]["admin_bypass"]["requires"].pop("stalled_required_check"))
must_fail("the bypass revocation switch removed",
          lambda d: d["merge"]["admin_bypass"].pop("allowed"))
must_fail("the bypass losing its human-decision note",
          lambda d: d["merge"]["admin_bypass"].pop("agent_execution"))

# --- check sets --------------------------------------------------------------------------------
print("required-check configuration")
must_fail("required and non-blocking sets overlapping",
          lambda d: d["merge"]["non_blocking_checks"].append(d["merge"]["required_checks"][0]))
must_fail("required_checks emptied", lambda d: d["merge"].update(required_checks=[]))
must_fail("the agent tooling check not being required",
          lambda d: d["merge"].update(agent_tooling_check="Not A Required Check"))

print()
if failures:
    print(f"check-merge-policy-schema tests: {len(failures)} failure(s)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("check-merge-policy-schema tests: clean")
