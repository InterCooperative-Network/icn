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

# --- TOTALITY: validate() must never raise on JSON-shaped input --------------------------------
# icn#2658 review (Copilot): set()/sorted()/`in <set>` over malformed arrays raised TypeError, and
# a non-object top level raised AttributeError. A validator that crashes reports nothing, and
# nothing reads as clean. This is checked as a CLASS over every collection field it consumes, not
# as three patched lines.
print("validate() is total over JSON-shaped input")

JSON_VALUES = [None, True, False, 0, 1, -1, 1.5, "", "s", [], {}, [None], [{}], [[]],
               [1, "a"], [None, "a"], {"k": None}, {"k": []}]


def never_raises(desc, value):
    try:
        errs = validate(value)
    except Exception as exc:                       # noqa: BLE001 - the point is that none escape
        check(f"TOTAL: {desc} -> raised {type(exc).__name__}: {exc}", False)
        return
    check(f"TOTAL: {desc} -> {len(errs)} error(s), no raise", isinstance(errs, list))


for _v in JSON_VALUES:
    never_raises(f"top-level {_v!r}", _v)

# Every collection field the validator consumes, fed every representative malformed member type.
COLLECTION_FIELDS = [
    ("merge.required_checks", lambda d, v: d["merge"].__setitem__("required_checks", v)),
    ("merge.non_blocking_checks", lambda d, v: d["merge"].__setitem__("non_blocking_checks", v)),
    ("ready_when.merge_state_status_in",
     lambda d, v: d["merge"]["ready_when"].__setitem__("merge_state_status_in", v)),
    ("ready_when.review_decision_allowlist",
     lambda d, v: d["merge"]["ready_when"].__setitem__("review_decision_allowlist", v)),
    ("ready_when.required_check_conclusion_allowlist",
     lambda d, v: d["merge"]["ready_when"].__setitem__("required_check_conclusion_allowlist", v)),
    ("readiness_definition", lambda d, v: d.__setitem__("readiness_definition", v)),
]
for _name, _set in COLLECTION_FIELDS:
    for _v in (None, True, 3, "str", {}, [None], [{}], [[]], [1, "a"], [None, "a"], [{"a": 1}]):
        never_raises(f"{_name} = {_v!r}", mutated(lambda d, _s=_set, _val=_v: _s(d, _val)))

# Nested objects given the wrong type must be reported, not treated as objects.
NESTED_OBJECTS = [
    ("merge", lambda d, v: d.__setitem__("merge", v)),
    ("merge.exception", lambda d, v: d["merge"].__setitem__("exception", v)),
    ("merge.ready_when", lambda d, v: d["merge"].__setitem__("ready_when", v)),
    ("merge.ready_when.not_deferred",
     lambda d, v: d["merge"]["ready_when"].__setitem__("not_deferred", v)),
    ("merge.admin_bypass", lambda d, v: d["merge"].__setitem__("admin_bypass", v)),
    ("merge.auto_merge", lambda d, v: d["merge"].__setitem__("auto_merge", v)),
]
for _name, _set in NESTED_OBJECTS:
    for _v in (None, True, 5, "str", [], [1]):
        never_raises(f"{_name} = {_v!r}", mutated(lambda d, _s=_set, _val=_v: _s(d, _val)))
        must_fail(f"{_name} = {_v!r} is rejected", lambda d, _s=_set, _val=_v: _s(d, _val))

# --- EXACT JSON TYPES: bool is not an integer ---------------------------------------------------
print("JSON types are exact")
must_fail("unresolved_review_threads = false (bool is not an integer)",
          lambda d: d["merge"]["ready_when"].update(unresolved_review_threads=False),
          expect="unresolved_review_threads")
must_fail("unresolved_review_threads = true",
          lambda d: d["merge"]["ready_when"].update(unresolved_review_threads=True))
must_fail("unresolved_review_threads = 0.0 (float is not an integer)",
          lambda d: d["merge"]["ready_when"].update(unresolved_review_threads=0.0))
must_fail("is_draft = 0 (int is not a bool)",
          lambda d: d["merge"]["ready_when"].update(is_draft=0))
must_fail("not_deferred.is_in_merge_queue = 0 (int is not a bool)",
          lambda d: d["merge"]["ready_when"]["not_deferred"].update(is_in_merge_queue=0))
must_fail("admin_bypass.agent_execution = 0 (int is not a bool)",
          lambda d: d["merge"]["admin_bypass"].update(agent_execution=0))
must_fail("default_strategy = true (bool is not a string)",
          lambda d: d["merge"].update(default_strategy=True))

# --- AUTHORITY AS DATA: policy may not spell commands or CLI flags -----------------------------
print("policy data cannot spell commands or CLI flags")
must_fail("auto_merge.gh_flags reappearing",
          lambda d: d["merge"]["auto_merge"].update(gh_flags=["--auto"]), expect="raw CLI authority")
must_fail("auto_merge.gh_flags = ['--admin']",
          lambda d: d["merge"]["auto_merge"].update(gh_flags=["--admin"]))
must_fail("auto_merge.gh_flags = ['--disable-auto']",
          lambda d: d["merge"]["auto_merge"].update(gh_flags=["--disable-auto"]))
must_fail("auto_merge.command reappearing",
          lambda d: d["merge"]["auto_merge"].update(command="gh pr merge <N> --auto"))
must_fail("auto_merge.args reappearing",
          lambda d: d["merge"]["auto_merge"].update(args=["--admin"]))
must_fail("a flag-shaped string in any operative field",
          lambda d: d["merge"]["exception"].update(applies_to="use --admin here"))
must_fail("a command-shaped string in any operative field",
          lambda d: d["merge"].update(required_checks_live_source="gh api repos/x/branches/main"))
must_fail("verify_required_checks carried forward as an executable string",
          lambda d: d["merge"].update(
              verify_required_checks="gh api repos/x/branches/main/protection --jq '.contexts'"))
must_fail("required_checks_live_source outside the closed symbolic set",
          lambda d: d["merge"].update(required_checks_live_source="scrape_the_web"))
must_pass("the semantic auto-merge declaration remains expressible",
          lambda d: d["merge"].update(auto_merge={
              "enabled": True, "use_when": "pending", "strategy_from": "default_strategy",
              "note": "semantic"}))
must_pass("auto-merge can be declared disabled",
          lambda d: d["merge"]["auto_merge"].update(enabled=False))

# --- ONE OWNER: ADR-0016 owns admin-bypass eligibility ------------------------------------------
print("ADR-0016 is the sole owner of admin-bypass eligibility")
must_fail("a structured `requires` eligibility replica returning",
          lambda d: d["merge"]["admin_bypass"].update(requires={"mergeable": "MERGEABLE"}),
          expect="restates eligibility")
must_fail("the prose `condition` returning",
          lambda d: d["merge"]["admin_bypass"].update(
              condition="green AND mergeStateStatus=MERGEABLE but stalled"))
must_fail("a partial replica under another name",
          lambda d: d["merge"]["admin_bypass"].update(prerequisites={"x": 1}))
must_fail("individual eligibility fields hoisted onto the bypass object",
          lambda d: d["merge"]["admin_bypass"].update(stalled_required_check={"m": 30}))
must_fail("bypass claiming to be an agent execution route",
          lambda d: d["merge"]["admin_bypass"].update(agent_execution=True))
must_fail("bypass decision not human",
          lambda d: d["merge"]["admin_bypass"].update(decision="agent"))
must_fail("authoritative_source pointing at a file that does not exist",
          lambda d: d["merge"]["admin_bypass"].update(authoritative_source="docs/adr/NOPE.md"))
must_fail("authoritative_source removed",
          lambda d: d["merge"]["admin_bypass"].pop("authoritative_source"))
must_fail("the fail-closed statement removed",
          lambda d: d["merge"]["admin_bypass"].pop("fail_closed"))

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
must_fail("the whole pre-#2651 admin_bypass object (prose `condition`, no owner pointer)",
          lambda d: d["merge"].__setitem__("admin_bypass", {
              "allowed": True,
              "condition": "Required checks green AND mergeStateStatus=MERGEABLE but stalled",
              "never_for": "Bypassing genuinely failing required checks"}))
must_fail("the pre-#2651 auto_merge object (baked command string)",
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
must_fail("a non-MergeableState `mergeable`",
          lambda d: d["merge"]["ready_when"].update(mergeable="CLEAN"))
must_fail("ordinary gate without CLEAN (the state it exists for)",
          lambda d: d["merge"]["ready_when"].update(merge_state_status_in=["UNSTABLE"]))

# --- allowlists must be allowlists, and on the right axis --------------------------------------
print("allowlists are fail-closed and on the right axis")
must_fail("a failing conclusion allowlisted",
          lambda d: d["merge"]["ready_when"]["required_check_conclusion_allowlist"].append("FAILURE"))
must_fail("a PENDING value in the CONCLUSION allowlist",
          lambda d: d["merge"]["ready_when"]["required_check_conclusion_allowlist"].append("PENDING"))
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
must_fail("the bypass revocation switch removed",
          lambda d: d["merge"]["admin_bypass"].pop("allowed"))

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
