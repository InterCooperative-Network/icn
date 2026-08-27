#!/usr/bin/env python3
"""check-merge-policy-schema.py — validate the STRUCTURED merge policy (icn#2651).

`ops/state/truth/policy.json#merge` is the registered owner of merge requirements. This validates
its shape and values so that a consumer can deserialize it and act on it without re-deriving what
the fields mean.

WHY THIS EXISTS, AND WHY IT IS STRUCTURED-ONLY.

Two classes of drift got past every prior gate:

  1. The owner named a field that cannot hold the value it was compared against.
     `admin_bypass.condition` read `mergeStateStatus=MERGEABLE`. `MERGEABLE` belongs to
     `MergeableState` (the `mergeable` field); `mergeStateStatus` is `MergeStateStatus` and has no
     such member, so the documented exception was unsatisfiable and nothing noticed.

  2. The owner duplicated its own values. `auto_merge.command` baked `--squash` while
     `default_strategy` was the owner of that choice, so the two could contradict each other.

An earlier attempt caught these by scanning the file's PROSE for `field ... VALUE` associations.
That needed a clause window, a negation rule and an exempt-key set, and review kept finding inputs
it got wrong — its failure mode is a false negative, silence that reads as proof. Facts that must
mechanically agree therefore live in STRUCTURED policy, compared here as JSON values against
pinned enums. Prose is checked only by ABSENCE — an assertion with no grammar to get wrong.

THE STRATEGY ENUM IS CODE, NOT DATA.

`GH_MERGE_STRATEGIES` below is hardcoded and is NOT read from the file being validated. That is
the point: a consumer that interpolated `default_strategy` straight into a command would let a
policy saying `admin` reconstruct `gh pr merge --admin`, and for a stacked PR the base supplying
that file can be contributor-controlled. A closed set owned by code cannot be widened by data.

Run: python3 scripts/check-merge-policy-schema.py
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
POLICY = ROOT / "ops" / "state" / "truth" / "policy.json"

# --- pinned vocabularies. Owned HERE, never read from the document under validation. -----------
# gh api graphql -f query='{m:__type(name:"MergeableState"){enumValues{name}}
#                           s:__type(name:"MergeStateStatus"){enumValues{name}}
#                           r:__type(name:"PullRequestReviewDecision"){enumValues{name}}}'
# Verified 2026-08-27 against the live API.
MERGEABLE_STATE = {"MERGEABLE", "CONFLICTING", "UNKNOWN"}
MERGE_STATE_STATUS = {"DIRTY", "UNKNOWN", "BLOCKED", "BEHIND", "UNSTABLE", "HAS_HOOKS", "CLEAN"}
REVIEW_DECISIONS = {"CHANGES_REQUESTED", "APPROVED", "REVIEW_REQUIRED"}
# `gh pr merge` accepts exactly these. A CLOSED set: anything else is not a strategy.
GH_MERGE_STRATEGIES = {"merge", "squash", "rebase"}
# Values each merge-state enum owns EXCLUSIVELY (UNKNOWN is in both, so it proves nothing).
MERGEABLE_ONLY = MERGEABLE_STATE - MERGE_STATE_STATUS
STATUS_ONLY = MERGE_STATE_STATUS - MERGEABLE_STATE
# A check that ran and did not pass. No gate may allowlist one.
FAILED_CONCLUSIONS = {"FAILURE", "TIMED_OUT", "CANCELLED", "ACTION_REQUIRED",
                      "STALE", "STARTUP_FAILURE", "ERROR"}
# Terminal-and-benign. A conclusion allowlist may hold nothing else — in particular no pending
# state, which is a different axis and would make a still-running check read as ready.
TERMINAL_SAFE = {"SUCCESS", "NEUTRAL", "SKIPPED"}
PENDING_STATES = {"QUEUED", "IN_PROGRESS", "WAITING", "REQUESTED", "PENDING", "EXPECTED"}
# States meaning a runner has already picked the job up.
ASSIGNED_STATES = {"IN_PROGRESS", "COMPLETED"}
STRATEGY_FLAG_RE = re.compile(r"--(merge|squash|rebase)\b")


def validate(policy: dict) -> list[str]:
    """Return a list of failure messages; empty means the policy is structurally sound."""
    bad: list[str] = []

    def req(cond, msg):
        if not cond:
            bad.append(msg)

    merge = policy.get("merge")
    req(isinstance(merge, dict), "merge: missing or not an object")
    if not isinstance(merge, dict):
        return bad

    # --- (1) strategy: a value from a CLOSED set owned by code ---------------------------------
    strategy = merge.get("default_strategy")
    req(strategy in GH_MERGE_STRATEGIES,
        f"default_strategy {strategy!r} is not one of {sorted(GH_MERGE_STRATEGIES)}")
    req(bool(merge.get("strategy_note")),
        "strategy_note: missing — the closed-set rule must be stated where the value lives")

    exc = merge.get("exception")
    req(isinstance(exc, dict), "exception: must be structured, not prose (a strategy a consumer "
                               "cannot select is not a policy)")
    if isinstance(exc, dict):
        req(exc.get("strategy") in GH_MERGE_STRATEGIES,
            f"exception.strategy {exc.get('strategy')!r} is not one of {sorted(GH_MERGE_STRATEGIES)}")
        req(bool(exc.get("applies_to")), "exception.applies_to: missing")
        req(exc.get("strategy") != strategy,
            "exception.strategy equals default_strategy — then it is not an exception")

    # --- (2) required/non-required sets ---------------------------------------------------------
    required = set(merge.get("required_checks") or [])
    non_blocking = set(merge.get("non_blocking_checks") or [])
    req(len(required) > 0, "required_checks: empty")
    overlap = sorted(required & non_blocking)
    req(not overlap, f"required_checks and non_blocking_checks overlap: {overlap}")
    req(merge.get("agent_tooling_check") in required,
        "agent_tooling_check is not itself a required check")

    # --- (3) shared shape for every gate that names GitHub merge state --------------------------
    def check_gate(label, gate, *, must_include_clean):
        req(isinstance(gate, dict) and gate != {}, f"{label}: missing machine-checkable fields")
        if not isinstance(gate, dict):
            return
        req(gate.get("mergeable") in MERGEABLE_STATE,
            f"{label}.mergeable {gate.get('mergeable')!r} is not a MergeableState member")
        mss = gate.get("merge_state_status_in")
        req(isinstance(mss, list) and len(mss) > 0, f"{label}.merge_state_status_in: empty")
        if isinstance(mss, list):
            stray = sorted(set(mss) - MERGE_STATE_STATUS)
            req(not stray, f"{label}.merge_state_status_in has non-MergeStateStatus values: {stray}")
            # THE #2651 REGRESSION, as a value comparison rather than a prose scan.
            leak = sorted(set(mss) & MERGEABLE_ONLY)
            req(not leak, f"{label}: MergeableState value leaked into a mergeStateStatus field: {leak}")
            req(("CLEAN" in mss) == must_include_clean,
                f"{label}: CLEAN must be {'present' if must_include_clean else 'absent'}")
        req(gate.get("is_draft") is False, f"{label}.is_draft must be false")
        req("review_decision_not_in" not in gate and isinstance(gate.get("review_decision_allowlist"), list),
            f"{label}: review decision must use an allowlist, not a denylist")
        rd = gate.get("review_decision_allowlist") or []
        req("CHANGES_REQUESTED" not in rd, f"{label}: allowlists CHANGES_REQUESTED")
        req("REVIEW_REQUIRED" not in rd, f"{label}: allowlists REVIEW_REQUIRED")
        req(None in rd, f"{label}: the live null review decision must be admitted EXPLICITLY")
        badrd = sorted(v for v in rd if v is not None and v not in REVIEW_DECISIONS)
        req(not badrd, f"{label}: non-PullRequestReviewDecision values allowlisted: {badrd}")
        req(gate.get("unresolved_review_threads") == 0,
            f"{label}.unresolved_review_threads must be 0")
        done = gate.get("required_check_conclusion_allowlist")
        req(isinstance(done, list) and len(done) > 0,
            f"{label}.required_check_conclusion_allowlist: empty")
        strayc = sorted(set(done or []) - TERMINAL_SAFE)
        req(not strayc,
            f"{label}: conclusion allowlist holds non-terminal-safe values: {strayc} "
            f"(a pending value here would make a running check read as ready)")

    ready = merge.get("ready_when")
    check_gate("ready_when", ready, must_include_clean=True)
    if isinstance(ready, dict):
        # The gate must stay STRUCTURED: operative fields are these keys with these types, and
        # anything free-form must live in a `*note` key that no consumer evaluates.
        OPERATIVE = {"mergeable": str, "merge_state_status_in": list, "is_draft": bool,
                     "review_decision_allowlist": list, "unresolved_review_threads": int,
                     "required_check_conclusion_allowlist": list, "not_deferred": dict}
        extra = sorted(k for k in ready
                       if k not in OPERATIVE and not k.endswith(("note", "note_ref")))
        req(not extra, f"ready_when: non-structured operative fields present: {extra}")
        missing = sorted(k for k in OPERATIVE if k not in ready)
        req(not missing, f"ready_when: missing operative fields: {missing}")
        mistyped = sorted(k for k, ty in OPERATIVE.items()
                          if k in ready and not isinstance(ready[k], ty))
        req(not mistyped, f"ready_when: fields with the wrong type: {mistyped}")
        req("required_check_pending_allowlist" not in ready,
            "ready_when: pending is not ready — it must allowlist no pending state at all")
        nd = ready.get("not_deferred") or {}
        req(nd.get("merge_queue_absent") is True
            and nd.get("is_in_merge_queue") is False
            and nd.get("auto_merge_request_absent") is True,
            "ready_when.not_deferred must require all three: no merge queue, not in the queue, "
            "no existing auto-merge request")

    bypass = merge.get("admin_bypass")
    req(isinstance(bypass, dict), "admin_bypass: missing")
    if isinstance(bypass, dict):
        req(isinstance(bypass.get("allowed"), bool),
            "admin_bypass.allowed: missing revocation switch")
        req("condition" not in bypass,
            "admin_bypass.condition: prose condition replaced by structured `requires` (icn#2651)")
        for k in ("never_for", "fail_closed", "scope_note", "field_note", "agent_execution"):
            req(bool(bypass.get(k)), f"admin_bypass.{k}: missing")
        breq = bypass.get("requires")
        check_gate("admin_bypass.requires", breq, must_include_clean=False)
        if isinstance(breq, dict):
            pend = breq.get("required_check_pending_allowlist")
            req(isinstance(pend, list) and len(pend) > 0,
                "admin_bypass.requires.required_check_pending_allowlist: empty")
            strayp = sorted(set(pend or []) - PENDING_STATES)
            req(not strayp, f"admin_bypass: non-pending values in the pending allowlist: {strayp}")
            assigned = sorted(ASSIGNED_STATES & set(pend or []))
            req(not assigned,
                f"admin_bypass: pending allowlist admits a started job: {assigned} "
                "(ADR-0016 permits bypassing only checks not yet assigned a runner)")
            req(not (set(breq.get("required_check_conclusion_allowlist") or []) & set(pend or [])),
                "admin_bypass: the two allowlists overlap")
            stall = breq.get("stalled_required_check")
            req(isinstance(stall, dict), "admin_bypass.requires.stalled_required_check: missing")
            if isinstance(stall, dict):
                qual = set(stall.get("qualifying_states") or [])
                excl = set(stall.get("excluded_states") or [])
                req(bool(qual) and bool(excl),
                    "stalled_required_check: qualifying_states/excluded_states must both be set "
                    "(a bare threshold reads as `any pending state`)")
                req(not (qual & ASSIGNED_STATES),
                    f"stalled_required_check: a started job qualifies as stalled: "
                    f"{sorted(qual & ASSIGNED_STATES)}")
                req(ASSIGNED_STATES <= excl,
                    "stalled_required_check: started states must be excluded BY NAME, not omitted")
                req(not (qual & excl), "stalled_required_check: qualifying/excluded overlap")
                req(set(pend or []) == qual,
                    "stalled_required_check.qualifying_states must equal the pending allowlist")
                req(stall.get("min_pending_minutes") == 30 and stall.get("elapsed_seconds") == 0,
                    "stalled_required_check: threshold must be 30 minutes at 0s elapsed (ADR-0016)")
                req(bool(stall.get("note")), "stalled_required_check.note: missing")

    # --- (4) the owner must not duplicate its own values ----------------------------------------
    auto = merge.get("auto_merge") or {}
    req(not any(k in auto for k in ("command", "cmd", "run")),
        "auto_merge: carries an executable command string (icn#2651)")
    req(auto.get("strategy_from") == "default_strategy",
        "auto_merge: must point at default_strategy rather than naming a strategy")
    req(isinstance(auto.get("gh_flags"), list) and auto["gh_flags"],
        "auto_merge.gh_flags: missing")
    blob = json.dumps({k: v for k, v in auto.items() if k != "note"})
    req(STRATEGY_FLAG_RE.search(blob) is None,
        "auto_merge: hardcodes a --merge/--squash/--rebase flag")

    # --- (5) prose carries no operative value, checked by ABSENCE -------------------------------
    readiness = policy.get("readiness_definition")
    req(isinstance(readiness, list) and readiness, "readiness_definition: missing")
    text = " ".join(readiness or [])
    strays = sorted(v for v in (MERGEABLE_ONLY | STATUS_ONLY) if re.search(rf"\b{v}\b", text))
    req(not strays, f"readiness_definition restates merge-state enum values: {strays}")
    counts = re.findall(r"\b(\d+)\s+required\b", text)
    req(not counts, f"readiness_definition restates a required-check count: {counts}")
    req("merge.ready_when" in text,
        "readiness_definition must point at the structured owner instead of restating it")
    return bad


def main() -> int:
    try:
        policy = json.loads(POLICY.read_text(encoding="utf-8"))
    except Exception as exc:                                    # unreadable == unusable
        print(f"check-merge-policy-schema: cannot read {POLICY}: {exc}")
        return 1
    failures = validate(policy)
    for f in failures:
        print(f"  FAIL {f}")
    if failures:
        print(f"check-merge-policy-schema: {len(failures)} failure(s)")
        return 1
    print("check-merge-policy-schema: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
