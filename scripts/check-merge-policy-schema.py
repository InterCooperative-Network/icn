#!/usr/bin/env python3
"""check-merge-policy-schema.py — validate the STRUCTURED merge policy (icn#2651).

`ops/state/truth/policy.json#merge` is the registered owner of merge requirements. This validates
its shape and values so a consumer can deserialize it and act on it without re-deriving meanings.

CONTRACT
    validate(value) -> list[str]

TOTAL over any JSON-shaped input. It must never raise merely because the decoded document is
malformed — a validator that crashes on bad input reports nothing, and nothing reads as clean.
Every type is therefore established before the value is used: nothing is passed to `set()`,
`sorted()`, `in <set>` or `.get()` until it is known safe, and unhashable members (objects, arrays)
are rejected as data rather than reaching a hash (icn#2658 review).

JSON TYPES MEAN JSON TYPES
`isinstance(False, int)` is True in Python, so JSON `false` satisfied an integer check and
`false == 0` compared equal — a malformed policy was reported clean while a strictly typed consumer
would reject it. Integer fields use `type(v) is int`; booleans require real booleans.

WHY STRUCTURED-ONLY
Two classes of drift got past every prior gate: the owner named a field that cannot hold the value
it was compared against (`mergeStateStatus=MERGEABLE` — `MERGEABLE` belongs to `MergeableState`, so
the documented exception was unsatisfiable), and the owner duplicated values it owned
(`auto_merge.command` baked `--squash`). An earlier attempt caught these by scanning the file's
PROSE, which needed a clause window and a negation rule and kept producing false negatives. Facts
that must mechanically agree therefore live in structured policy and are compared here as values.
Prose is checked only by ABSENCE — an assertion with no grammar to get wrong.

TWO AUTHORITY RULES THIS FILE ENFORCES
1. The strategy enum is CODE, NOT DATA. `GH_MERGE_STRATEGIES` is hardcoded and never read from the
   document under validation. A consumer interpolating `default_strategy` would let a policy saying
   `admin` reconstruct `gh pr merge --admin`, and for a stacked PR the base supplying that file can
   be contributor-controlled. A closed set owned by code cannot be widened by data.
2. POLICY DATA MAY NOT SPELL COMMANDS. No operative field may hold a command- or flag-shaped
   string. `gh_flags: ["--auto"]` was arbitrary CLI authority as data: `["--admin"]` and
   `["--disable-auto"]` validated clean. Intent is declared symbolically; only code maps it to
   flags.

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
GH_MERGE_STRATEGIES = {"merge", "squash", "rebase"}          # CLOSED. Owned by code.
MERGEABLE_ONLY = MERGEABLE_STATE - MERGE_STATE_STATUS
STATUS_ONLY = MERGE_STATE_STATUS - MERGEABLE_STATE
TERMINAL_SAFE = {"SUCCESS", "NEUTRAL", "SKIPPED"}
LIVE_SOURCES = {"github_branch_protection"}                  # CLOSED symbolic sources.
# A value that looks like a shell command or a CLI flag has no business in policy data.
COMMAND_SHAPED = re.compile(r"(^|\s)(gh|git|jq|eval|bash|sh)\s|(^|\s)--[a-z]")


# --- one reusable type mechanism ---------------------------------------------------------------
# Each returns (ok, value_or_None). Nothing downstream touches a value these have rejected.
def as_obj(v):
    return (True, v) if isinstance(v, dict) else (False, None)


def as_str(v):
    return (True, v) if isinstance(v, str) and v else (False, None)


def as_exact_bool(v):
    return (True, v) if type(v) is bool else (False, None)


def as_exact_int(v):
    # `type(v) is int` on purpose: bool is a subclass of int, so JSON `false` would pass isinstance.
    return (True, v) if type(v) is int else (False, None)


def as_str_list(v):
    if not isinstance(v, list):
        return (False, None)
    return (True, v) if all(isinstance(x, str) for x in v) else (False, None)


def as_str_or_null_list(v):
    if not isinstance(v, list):
        return (False, None)
    return (True, v) if all(x is None or isinstance(x, str) for x in v) else (False, None)


def validate(policy) -> list[str]:
    """Total over any JSON-shaped value. Returns failure messages; empty means sound."""
    bad: list[str] = []

    def req(cond, msg):
        if not cond:
            bad.append(msg)
        return bool(cond)

    ok, policy = as_obj(policy)
    if not req(ok, "policy: top-level value is not a JSON object"):
        return bad

    ok, merge = as_obj(policy.get("merge"))
    if not req(ok, "merge: missing or not an object"):
        return bad

    def field(obj, key, caster, label):
        """Type-check one field, recording a message on failure. Returns (ok, value)."""
        got, val = caster(obj.get(key))
        req(got, f"{label}: missing or wrong JSON type")
        return got, val

    # --- (1) strategy: a value from a CLOSED set owned by code ---------------------------------
    got, strategy = field(merge, "default_strategy", as_str, "default_strategy")
    if got:
        req(strategy in GH_MERGE_STRATEGIES,
            f"default_strategy {strategy!r} is not one of {sorted(GH_MERGE_STRATEGIES)}")
    field(merge, "strategy_note", as_str, "strategy_note")

    got, exc = field(merge, "exception", as_obj,
                     "exception (must be structured, not prose — a strategy a consumer cannot "
                     "select is not a policy)")
    if got:
        eok, estrat = field(exc, "strategy", as_str, "exception.strategy")
        if eok:
            req(estrat in GH_MERGE_STRATEGIES,
                f"exception.strategy {estrat!r} is not one of {sorted(GH_MERGE_STRATEGIES)}")
            req(estrat != strategy,
                "exception.strategy equals default_strategy — then it is not an exception")
        field(exc, "applies_to", as_str, "exception.applies_to")

    # --- (2) check sets -------------------------------------------------------------------------
    rok, required = field(merge, "required_checks", as_str_list, "required_checks")
    nok, non_blocking = field(merge, "non_blocking_checks", as_str_list, "non_blocking_checks")
    if rok:
        req(len(required) > 0, "required_checks: empty")
        if nok:
            overlap = sorted(set(required) & set(non_blocking))
            req(not overlap, f"required_checks and non_blocking_checks overlap: {overlap}")
        aok, atc = field(merge, "agent_tooling_check", as_str, "agent_tooling_check")
        if aok:
            req(atc in required, "agent_tooling_check is not itself a required check")
    lok, live_src = field(merge, "required_checks_live_source", as_str,
                          "required_checks_live_source")
    if lok:
        req(live_src in LIVE_SOURCES,
            f"required_checks_live_source {live_src!r} is not one of {sorted(LIVE_SOURCES)}")
    req("verify_required_checks" not in merge,
        "verify_required_checks: an executable gh command string must not be carried as data "
        "(use the symbolic required_checks_live_source)")

    # --- (3) the ordinary-merge gate -----------------------------------------------------------
    got, ready = field(merge, "ready_when", as_obj, "ready_when")
    if got:
        mok, mval = field(ready, "mergeable", as_str, "ready_when.mergeable")
        if mok:
            req(mval in MERGEABLE_STATE,
                f"ready_when.mergeable {mval!r} is not a MergeableState member")
        sok, mss = field(ready, "merge_state_status_in", as_str_list,
                         "ready_when.merge_state_status_in")
        if sok:
            req(len(mss) > 0, "ready_when.merge_state_status_in: empty")
            stray = sorted(set(mss) - MERGE_STATE_STATUS)
            req(not stray, f"ready_when.merge_state_status_in has non-MergeStateStatus values: {stray}")
            leak = sorted(set(mss) & MERGEABLE_ONLY)
            req(not leak, f"ready_when: MergeableState value leaked into a mergeStateStatus field: {leak}")
            req("CLEAN" in mss, "ready_when: CLEAN must be admitted — it is the state it exists for")
        dok, draft = field(ready, "is_draft", as_exact_bool, "ready_when.is_draft")
        if dok:
            req(draft is False, "ready_when.is_draft must be false")
        req("review_decision_not_in" not in ready,
            "ready_when: review decision must use an allowlist, not a denylist")
        vok, rd = field(ready, "review_decision_allowlist", as_str_or_null_list,
                        "ready_when.review_decision_allowlist")
        if vok:
            req("CHANGES_REQUESTED" not in rd, "ready_when allowlists CHANGES_REQUESTED")
            req("REVIEW_REQUIRED" not in rd, "ready_when allowlists REVIEW_REQUIRED")
            req(None in rd, "ready_when: the live null review decision must be admitted EXPLICITLY")
            badrd = sorted(v for v in rd if v is not None and v not in REVIEW_DECISIONS)
            req(not badrd, f"ready_when: non-PullRequestReviewDecision values allowlisted: {badrd}")
        tok, threads = field(ready, "unresolved_review_threads", as_exact_int,
                             "ready_when.unresolved_review_threads (integer; JSON true/false is "
                             "not an integer)")
        if tok:
            req(threads == 0, "ready_when.unresolved_review_threads must be 0")
        cok, done = field(ready, "required_check_conclusion_allowlist", as_str_list,
                          "ready_when.required_check_conclusion_allowlist")
        if cok:
            req(len(done) > 0, "ready_when.required_check_conclusion_allowlist: empty")
            strayc = sorted(set(done) - TERMINAL_SAFE)
            req(not strayc,
                f"ready_when: conclusion allowlist holds non-terminal-safe values: {strayc} "
                "(a pending value here would make a running check read as ready)")
        req("required_check_pending_allowlist" not in ready,
            "ready_when: pending is not ready — it must allowlist no pending state at all")

        nok2, nd = field(ready, "not_deferred", as_obj, "ready_when.not_deferred")
        if nok2:
            for key, want in (("merge_queue_absent", True), ("is_in_merge_queue", False),
                              ("auto_merge_request_absent", True)):
                bok, bval = field(nd, key, as_exact_bool, f"ready_when.not_deferred.{key}")
                if bok:
                    req(bval is want,
                        f"ready_when.not_deferred.{key} must be {json.dumps(want)} — an ordinary "
                        "merge completes or refuses, it never arms something to happen later")

        OPERATIVE = {"mergeable", "merge_state_status_in", "is_draft", "review_decision_allowlist",
                     "unresolved_review_threads", "required_check_conclusion_allowlist",
                     "not_deferred"}
        extra = sorted(k for k in ready
                       if isinstance(k, str) and k not in OPERATIVE and not k.endswith("note"))
        req(not extra, f"ready_when: non-structured operative fields present: {extra}")

    # --- (4) the human admin exception is NOT owned here ----------------------------------------
    got, bypass = field(merge, "admin_bypass", as_obj, "admin_bypass")
    if got:
        field(merge["admin_bypass"], "allowed", as_exact_bool, "admin_bypass.allowed")
        dok, decision = field(bypass, "decision", as_str, "admin_bypass.decision")
        if dok:
            req(decision == "human", "admin_bypass.decision must be 'human'")
        eok, agent_exec = field(bypass, "agent_execution", as_exact_bool,
                                "admin_bypass.agent_execution")
        if eok:
            req(agent_exec is False, "admin_bypass.agent_execution must be false")
        sok, src = field(bypass, "authoritative_source", as_str, "admin_bypass.authoritative_source")
        if sok:
            req((ROOT / src).is_file(),
                f"admin_bypass.authoritative_source does not exist: {src}")
        for key in ("never_for", "eligibility_note", "fail_closed", "consumer_note"):
            field(bypass, key, as_str, f"admin_bypass.{key}")
        # A PARTIAL ELIGIBILITY REPLICA MUST NOT REAPPEAR. ADR-0016 requires five conditions; a
        # structured `requires` here encoded only two, so a maintainer following it could believe a
        # merge was permitted while the ADR said wait. The fix is ONE OWNER — not a fuller copy,
        # which would rot the moment the ADR changed (icn#2658 review).
        REPLICA_KEYS = {"condition", "conditions", "requires", "prerequisites", "criteria",
                        "eligibility", "stalled_required_check", "merge_state_status_in",
                        "mergeable", "required_check_conclusion_allowlist",
                        "required_check_pending_allowlist", "review_decision_allowlist",
                        "unresolved_review_threads", "is_draft"}
        replicas = sorted(k for k in bypass if isinstance(k, str) and k in REPLICA_KEYS)
        req(not replicas,
            f"admin_bypass restates eligibility that ADR-0016 owns: {replicas} — this object is "
            "non-authoritative; consult authoritative_source instead of duplicating it")

    # --- (5) the owner must not duplicate its own values, nor spell commands --------------------
    got, auto = field(merge, "auto_merge", as_obj, "auto_merge")
    if got:
        field(auto, "enabled", as_exact_bool, "auto_merge.enabled")
        aok, sfrom = field(auto, "strategy_from", as_str, "auto_merge.strategy_from")
        if aok:
            req(sfrom == "default_strategy",
                "auto_merge.strategy_from must point at default_strategy rather than naming one")
        FLAG_KEYS = {"gh_flags", "flags", "args", "argv", "command", "cmd", "run", "exec"}
        raw = sorted(k for k in auto if isinstance(k, str) and k in FLAG_KEYS)
        req(not raw,
            f"auto_merge holds raw CLI authority as data: {raw} — a contributor-controlled base "
            "could spell --admin or --disable-auto; declare intent symbolically and let code map it")

    # POLICY DATA MAY NOT SPELL COMMANDS. Applies to every operative string in the merge subtree;
    # `*note` keys are documentation and are exempt by name.
    def scan(node, path):
        if isinstance(node, dict):
            for k, v in node.items():
                if isinstance(k, str) and k.endswith("note"):
                    continue
                scan(v, f"{path}.{k}")
        elif isinstance(node, list):
            for i, v in enumerate(node):
                scan(v, f"{path}[{i}]")
        elif isinstance(node, str) and COMMAND_SHAPED.search(node):
            bad.append(f"{path}: operative field holds a command- or flag-shaped string "
                       f"({node[:48]!r}) — policy declares intent, only code spells commands")

    scan(merge, "merge")

    # --- (6) prose carries no operative value, checked by ABSENCE -------------------------------
    rdok, readiness = as_str_list(policy.get("readiness_definition"))
    if req(rdok, "readiness_definition: missing or not a list of strings"):
        req(len(readiness) > 0, "readiness_definition: empty")
        text = " ".join(readiness)
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
