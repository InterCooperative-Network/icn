#!/usr/bin/env python3
"""Merge-policy invariants (icn#2651, icn#2656).

`ops/state/truth/policy.json` is the registered owner of merge requirements. Two classes of
drift got past every existing gate:

  1. The owner named a field that cannot hold the value it was compared against.
     `admin_bypass.condition` read `mergeStateStatus=MERGEABLE`. `MERGEABLE` is a member of
     GitHub's `MergeableState` enum (the `mergeable` field); `mergeStateStatus` is
     `MergeStateStatus` and has no such member. The documented exception was therefore
     unsatisfiable, and nothing noticed because prose is not type-checked.

  2. A consumer restated a value the owner owns. `.agents/skills/merge-pr/SKILL.md` merged with
     `--merge` while `default_strategy` was `squash`.

WHAT THIS FILE DOES NOT DO, on purpose (icn#2656 final correction). An earlier revision tried to
close (1) by scanning every string in the policy for `field ... VALUE` associations, with a
clause window that ended at a sentence break or a negation. Review kept finding inputs it got
wrong — the last being `mergeStateStatus must not remain UNKNOWN and instead must be MERGEABLE`,
where a `not` negating one value silently truncated the window before another. Each fix added
grammar. That is a natural-language parser wearing a test's clothes, and its failure mode is a
false negative: silence that reads as proof.

The fix was structural, not grammatical. Facts that must mechanically agree now live in
STRUCTURED policy (`.merge.ready_when`), where they are compared as JSON values against pinned
GitHub enums. Prose that used to carry those facts now points at the structured owner instead.

Prose is still checked, but only by ABSENCE: "this token appears nowhere". An absence assertion
has no grammar to get wrong — no clause window, no negation handling, so no false negative of
the kind that kept reappearing. Where prose must not carry an operative value, the test proves
the value is not there, rather than trying to work out what the sentence means.

Every MUST-FAIL case below is a reconstruction of real pre-fix state; the controls prove the
checks are not simply failing on everything.

Run: python3 scripts/tests/test_merge_policy_invariants.py
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
POLICY = ROOT / "ops" / "state" / "truth" / "policy.json"
SKILLS = ROOT / "ops" / "state" / "truth" / "skills.json"

# GitHub GraphQL enums, pinned so this runs offline. Re-verify with:
#   gh api graphql -f query='{m:__type(name:"MergeableState"){enumValues{name}}
#                             s:__type(name:"MergeStateStatus"){enumValues{name}}}'
# Verified 2026-08-26 against the live API.
MERGEABLE_STATE = {"MERGEABLE", "CONFLICTING", "UNKNOWN"}
MERGE_STATE_STATUS = {
    "DIRTY", "UNKNOWN", "BLOCKED", "BEHIND", "UNSTABLE", "HAS_HOOKS", "CLEAN",
}
# `gh pr merge` accepts exactly these strategies.
GH_MERGE_STRATEGIES = {"merge", "squash", "rebase"}
# gh api graphql -f query='{__type(name:"PullRequestReviewDecision"){enumValues{name}}}'
REVIEW_DECISIONS = {"CHANGES_REQUESTED", "APPROVED", "REVIEW_REQUIRED"}
# Values each enum owns EXCLUSIVELY. UNKNOWN is in both, so it proves nothing.
MERGEABLE_ONLY = MERGEABLE_STATE - MERGE_STATE_STATUS       # MERGEABLE, CONFLICTING
STATUS_ONLY = MERGE_STATE_STATUS - MERGEABLE_STATE          # DIRTY, BLOCKED, BEHIND, ...
# No merge gate may accept a check that ran and did not pass.
FAILED_CONCLUSIONS = {"FAILURE", "TIMED_OUT", "CANCELLED", "ACTION_REQUIRED",
                      "STALE", "STARTUP_FAILURE", "ERROR"}

STRATEGY_FLAG_RE = re.compile(r"--(merge|squash|rebase)\b")

failures = []


def check(desc, cond):
    if cond:
        print(f"  ok   {desc}")
    else:
        print(f"  FAIL {desc}")
        failures.append(desc)


whole = json.loads(POLICY.read_text(encoding="utf-8"))
merge = whole["merge"]

# --- (1) the canonical policy is structurally coherent ----------------------
print("field/value coherence")

strategy = merge.get("default_strategy")
check(f"default_strategy {strategy!r} is a real gh pr merge strategy",
      strategy in GH_MERGE_STRATEGIES)

# Review of 58d29370: `.merge.exception` was prose, so the skill printed it while STRATEGY
# stayed unconditionally `default_strategy` — the exempt category was squash-merged anyway and
# the exception could not be applied. A strategy the procedure cannot select is not a policy.
exc = merge.get("exception")
check("merge.exception is structured, not prose", isinstance(exc, dict))
if isinstance(exc, dict):
    check(f"exception.strategy {exc.get('strategy')!r} is a real gh pr merge strategy",
          exc.get("strategy") in GH_MERGE_STRATEGIES)
    check("exception declares the category it applies to", bool(exc.get("applies_to")))
    check("the exception names a DIFFERENT strategy from the default",
          exc.get("strategy") != strategy)
check("CONTROL: the pre-#2656 prose exception WOULD be caught",
      not isinstance("Subtree merge commits must use --merge. State reason explicitly.", dict))

required = set(merge.get("required_checks", []))
non_blocking = set(merge.get("non_blocking_checks", []))
check("required_checks is non-empty", len(required) > 0)
check(f"required_checks and non_blocking_checks are disjoint: {sorted(required & non_blocking)}",
      not (required & non_blocking))
check("the agent tooling check is itself a required check",
      merge.get("agent_tooling_check") in required)


def check_state_gate(label, gate, *, must_include_clean):
    """Structural validation shared by every gate that names GitHub merge state."""
    check(f"{label}: declares machine-checkable fields, not only prose",
          isinstance(gate, dict) and gate != {})
    if not isinstance(gate, dict):
        return
    check(f"{label}: mergeable {gate.get('mergeable')!r} is a MergeableState member",
          gate.get("mergeable") in MERGEABLE_STATE)
    mss = gate.get("merge_state_status_in")
    check(f"{label}: merge_state_status_in is a non-empty list",
          isinstance(mss, list) and len(mss) > 0)
    check(f"{label}: every merge_state_status_in value is a MergeStateStatus member: {mss}",
          isinstance(mss, list) and all(v in MERGE_STATE_STATUS for v in mss))
    # THE #2651 REGRESSION, now a value comparison rather than a prose scan.
    check(f"{label}: no MergeableState value has leaked into a mergeStateStatus field",
          isinstance(mss, list) and not (set(mss) & MERGEABLE_ONLY))
    check(f"{label}: CLEAN is {'required' if must_include_clean else 'excluded'}",
          isinstance(mss, list) and (("CLEAN" in mss) == must_include_clean))
    # Review gates, as allowlists. A denylist admits any value nobody enumerated.
    check(f"{label}: refuses a draft PR", gate.get("is_draft") is False)
    check(f"{label}: review decision uses an allowlist, not a denylist",
          "review_decision_not_in" not in gate
          and isinstance(gate.get("review_decision_allowlist"), list))
    rd = gate.get("review_decision_allowlist") or []
    check(f"{label}: refuses CHANGES_REQUESTED", "CHANGES_REQUESTED" not in rd)
    check(f"{label}: refuses REVIEW_REQUIRED", "REVIEW_REQUIRED" not in rd)
    check(f"{label}: the live null decision is admitted EXPLICITLY, not by omission", None in rd)
    check(f"{label}: every non-null allowlisted decision is a real PullRequestReviewDecision",
          all(v in REVIEW_DECISIONS for v in rd if v is not None))
    check(f"{label}: refuses unresolved review threads",
          gate.get("unresolved_review_threads") == 0)
    done = gate.get("required_check_conclusion_allowlist")
    check(f"{label}: required_check_conclusion_allowlist is a non-empty list",
          isinstance(done, list) and len(done) > 0)
    leaked = FAILED_CONCLUSIONS & set(done or [])
    check(f"{label}: no failing/aborted conclusion is allowlisted: {sorted(leaked)}", not leaked)


# `.merge.ready_when` is the ONLY gate an agent skill evaluates (icn#2656). It is the ordinary
# merge gate, so CLEAN must be admitted — that is the state it exists for.
print("ready_when: the structured ordinary-merge gate")
ready = merge.get("ready_when", {})
check_state_gate("ready_when", ready, must_include_clean=True)
# Pending is not ready. An ordinary merge waits; it never arms and never bypasses, so a pending
# allowlist here would be a state nobody decided about.
check("ready_when allowlists no pending state at all",
      isinstance(ready, dict) and "required_check_pending_allowlist" not in ready)
check("ready_when points at the enum note rather than restating it",
      isinstance(ready, dict) and ready.get("field_note_ref") == "merge.admin_bypass.field_note")
# The gate must stay STRUCTURED. This is what replaces the deleted prose scanner: rather than
# parsing explanation to find an operative claim, the gate is closed against explanation
# entirely — an operative field must be one of these keys, with the type given, and anything
# free-form has to live in a `*note` key that no consumer evaluates.
READY_OPERATIVE = {
    "mergeable": str,
    "merge_state_status_in": list,
    "is_draft": bool,
    "review_decision_allowlist": list,
    "unresolved_review_threads": int,
    "required_check_conclusion_allowlist": list,
}
if isinstance(ready, dict):
    extra = sorted(k for k in ready
                   if k not in READY_OPERATIVE and not k.endswith(("note", "note_ref")))
    check(f"ready_when carries only structured gate fields; explanation lives in *note: {extra}",
          not extra)
    missing = sorted(k for k in READY_OPERATIVE if k not in ready)
    check(f"ready_when declares every operative gate field: missing {missing}", not missing)
    mistyped = sorted(k for k, ty in READY_OPERATIVE.items()
                      if k in ready and not isinstance(ready[k], ty))
    check(f"every operative ready_when field has its structured type: {mistyped}", not mistyped)
    # CONTROL: an operative claim smuggled in as prose must be rejected, which is the defect
    # class the whole-file scanner was trying to catch by reading English.
    _prose_gate = dict(ready, condition="merge when mergeStateStatus is MERGEABLE")
    check("CONTROL: an operative prose field added to the gate WOULD be caught",
          bool([k for k in _prose_gate
                if k not in READY_OPERATIVE and not k.endswith(("note", "note_ref"))]))

# `.merge.admin_bypass` remains: docs/adr/ADR-0016 owns the HUMAN queue-stall exception and
# .agents/skills/watch-ci-and-advance references it. No agent skill executes it (asserted in
# section 3). Its structural validity is still checked — that is the #2651 regression's home.
print("admin_bypass: still structurally valid, no longer agent-executable")
bypass = merge.get("admin_bypass", {})
req = bypass.get("requires", {})
check_state_gate("admin_bypass.requires", req, must_include_clean=False)
check("admin_bypass exposes an `allowed` switch the owner can flip",
      isinstance(bypass.get("allowed"), bool))
check("admin_bypass still declares never_for", bool(bypass.get("never_for")))
check("admin_bypass declares fail_closed behaviour", bool(bypass.get("fail_closed")))
check("admin_bypass uses no conclusion DENYLIST",
      isinstance(req, dict) and "no_required_check_concluded" not in req)
if isinstance(req, dict):
    pend = req.get("required_check_pending_allowlist")
    check("admin_bypass required_check_pending_allowlist is a non-empty list",
          isinstance(pend, list) and len(pend) > 0)
    check("admin_bypass: no failing/aborted state is allowlisted as pending",
          not (FAILED_CONCLUSIONS & set(pend or [])))
    check("admin_bypass: the two allowlists are disjoint",
          not (set(req.get("required_check_conclusion_allowlist") or []) & set(pend or [])))
check("admin_bypass records that no agent skill executes it",
      bool(bypass.get("agent_execution")))

# CONTROLS: the shape checks would be vacuous if the pre-fix policy passed them.
_pre_fix_bypass = {"condition": "Required checks are green AND mergeStateStatus=MERGEABLE ..."}
check("CONTROL: the pre-#2656 admin_bypass shape would FAIL these checks",
      "requires" not in _pre_fix_bypass
      and "mergeStateStatus=MERGEABLE" in json.dumps(_pre_fix_bypass))
_leaky = {"mergeable": "MERGEABLE", "merge_state_status_in": ["MERGEABLE", "UNSTABLE"]}
check("CONTROL: a cross-enum leak in merge_state_status_in is caught by value comparison",
      bool(set(_leaky["merge_state_status_in"]) & MERGEABLE_ONLY))
_denylisted = {"review_decision_not_in": ["CHANGES_REQUESTED"]}
check("CONTROL: a review-decision DENYLIST shape would FAIL",
      "review_decision_not_in" in _denylisted
      and not isinstance(_denylisted.get("review_decision_allowlist"), list))

# --- (2) the owner must not duplicate its own values ------------------------
print("no duplicated values inside the owner")
auto = merge.get("auto_merge", {})
check("auto_merge carries no executable command string",
      not any(k in auto for k in ("command", "cmd", "run")))
check("auto_merge points at default_strategy rather than naming a strategy",
      auto.get("strategy_from") == "default_strategy")
auto_blob = json.dumps({k: v for k, v in auto.items() if k != "note"})
check(f"auto_merge hardcodes no --merge/--squash/--rebase flag: {auto_blob[:80]}",
      STRATEGY_FLAG_RE.search(auto_blob) is None)
_pre_fix_auto = {"command": "gh pr merge <N> --auto --squash", "use_when": "..."}
check("CONTROL: the pre-#2656 auto_merge shape would FAIL these checks",
      "command" in _pre_fix_auto and STRATEGY_FLAG_RE.search(json.dumps(_pre_fix_auto)))

# `readiness_definition` is EXPLANATORY prose. The operative gate is `.merge.ready_when`, so the
# prose must not carry the values — checked by ABSENCE, which cannot produce the false negatives
# the old semantic scanner did. It is also why no negation grammar is needed: there is no
# sentence to interpret, only a token that must not be present.
print("readiness_definition carries no operative values")
readiness = whole.get("readiness_definition", [])
check("readiness_definition is a non-empty list", isinstance(readiness, list) and readiness)
readiness_text = " ".join(readiness)
strays = sorted({v for v in (MERGEABLE_ONLY | STATUS_ONLY)
                 if re.search(rf"\b{v}\b", readiness_text)})
check(f"readiness_definition restates no merge-state enum value: {strays}", not strays)
counts = re.findall(r"\b(\d+)\s+required\b", readiness_text)
check(f"readiness_definition restates no required-check count: {counts}", not counts)
check("readiness_definition names the structured owner instead",
      "merge.ready_when" in readiness_text)
# CONTROLS: the absence checks must fire on the prose they replaced.
_old_readiness = ("All 11 required CI checks passed. mergeable is MERGEABLE, and "
                  "mergeStateStatus is CLEAN or UNSTABLE.")
check("CONTROL: the pre-#2656 readiness prose WOULD be caught (enum literal)",
      bool([v for v in (MERGEABLE_ONLY | STATUS_ONLY)
            if re.search(rf"\b{v}\b", _old_readiness)]))
check("CONTROL: the pre-#2656 readiness prose WOULD be caught (restated count)",
      bool(re.findall(r"\b(\d+)\s+required\b", _old_readiness)))

# --- (3) the merge skill's authority surface --------------------------------
print("merge-pr: reduced authority and no duplicated policy values")

registry = json.loads(SKILLS.read_text(encoding="utf-8"))
skill_paths = []
for entry in registry.get("skills", {}).get("icn_level", []):
    if entry.get("name") == "merge-pr":
        skill_paths.append((entry["name"], ROOT / entry["canonical_path"],
                            [ROOT / m["path"] for m in entry.get("provider_mirrors", [])]))
check("merge-pr is resolvable from the canonical registry", len(skill_paths) == 1)

EVAL_RE = re.compile(r"\beval\s+[\"'`$]")
MERGE_CMD_RE = re.compile(r"gh pr merge[^\n`]*--(merge|squash|rebase)\b")
# A privileged invocation is a single line: inside a fence, or a one-line command. Prose may
# name the flag in order to forbid it; nothing may be positioned to RUN it.
ADMIN_FLAG_RE = re.compile(r"--admin\b")
ADMIN_INVOKE_RE = re.compile(r"gh pr merge[^\n]*--admin")
AUTO_FLAG_RE = re.compile(r"--auto\b|auto_merge")
AUTO_INVOKE_RE = re.compile(r"gh pr merge[^\n]*--auto\b")


def fenced_lines(markdown: str) -> list[str]:
    out, in_fence = [], False
    for line in markdown.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence and line.strip():
            out.append(line.strip())
    return out


def extract_commands(markdown: str) -> list[str]:
    """Fenced-block lines, plus inline command BULLETS. Prose about a command is not a command:
    an earlier revision flagged the sentence explaining why a bare `gh pr view` is wrong."""
    out = fenced_lines(markdown)
    in_fence = False
    for line in markdown.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence or not line.lstrip().startswith("- "):
            continue
        for m in re.finditer(r"`([^`]+)`", line):
            tok = m.group(1).strip()
            if tok.startswith(("gh ", "git ", "jq ", "eval ")):
                out.append(tok)
    return out


for name, canonical, mirrors in skill_paths:
    body = canonical.read_text(encoding="utf-8")
    commands = extract_commands(body)
    cmd_text = " ".join(commands)
    fenced = fenced_lines(body)
    lines = body.splitlines()
    frontmatter = body.split("---", 2)[1] if body.startswith("---") else ""

    # -- (3a) THE REDUCED AUTHORITY SURFACE. The heart of icn#2656.
    check(f"{name}: no command carries --admin: "
          f"{[c for c in commands if ADMIN_FLAG_RE.search(c)]}",
          not any(ADMIN_FLAG_RE.search(c) for c in commands))
    check(f"{name}: no fenced block carries --admin",
          not any(ADMIN_FLAG_RE.search(f) for f in fenced))
    check(f"{name}: no line positions --admin as a merge invocation",
          not any(ADMIN_INVOKE_RE.search(ln) for ln in lines))
    check(f"{name}: --admin is not advertised in argument-hint",
          not ADMIN_FLAG_RE.search(frontmatter))
    # An ordinary invocation must not be able to become a deferred one either.
    check(f"{name}: no command arms auto-merge: "
          f"{[c for c in commands if AUTO_FLAG_RE.search(c)]}",
          not any(AUTO_FLAG_RE.search(c) for c in commands))
    check(f"{name}: no fenced block arms auto-merge",
          not any(AUTO_FLAG_RE.search(f) for f in fenced))
    check(f"{name}: no line positions --auto as a merge invocation",
          not any(AUTO_INVOKE_RE.search(ln) for ln in lines))
    # Exactly one way to merge. Two merge commands is two authority levels.
    merges = [c for c in commands if re.search(r"gh pr merge\b", c)]
    check(f"{name}: exactly one merge invocation exists: {len(merges)}", len(merges) == 1)
    check(f"{name}: that invocation pins the inspected head",
          bool(merges) and all("--match-head-commit" in c for c in merges))
    check(f"{name}: states that a GitHub refusal terminates rather than escalating",
          bool(re.search(r"that refusal is the answer", " ".join(body.split()), re.I)))
    # The output contract must not admit an outcome the skill can no longer produce.
    outputs = body.split("## Output", 1)[-1]
    check(f"{name}: the output contract offers no auto-merge outcome",
          "Auto-merge armed" not in outputs and "**Merged**" in outputs
          and "**Not merged**" in outputs)

    # -- (3b) evidence must be current, complete, and pinned.
    check(f"{name}: reads the structured readiness gate rather than restating it",
          "ready_when" in body)
    check(f"{name}: no `gh pr merge --<strategy>` literal; the strategy is substituted: "
          f"{MERGE_CMD_RE.findall(body)}", not MERGE_CMD_RE.findall(body))
    check(f"{name}: substitutes the strategy from the policy", "${STRATEGY}" in body)
    check(f"{name}: does not shell-eval policy values",
          not any(EVAL_RE.search(c) for c in commands))
    bare = [c for c in commands
            if re.search(r"gh pr (view|checks|merge)\b", c)
            and not re.search(r"gh pr (view|checks|merge) +(<N>|\$)", c)]
    check(f"{name}: every gh pr view/checks/merge command is explicitly addressed: {bare}",
          not bare)
    check(f"{name}: captures headRefOid to pin against", "headRefOid" in body)
    check(f"{name}: reads protection for the PR's actual base, not a hardcoded main",
          "${BASE_ENC}/protection" in body and "branches/main/protection" not in body)
    check(f"{name}: no protection path interpolates the UNENCODED base",
          "${BASE}/protection" not in body)
    check(f"{name}: encodes the base with a real encoder, not a hand-rolled substitution",
          "@uri" in cmd_text
          and not re.search(r"BASE.*(//|s#|tr ).*%2F", body)
          and not re.search(r"\$\{BASE//", body))
    check(f"{name}: names baseRefName as the sole branch-identity authority",
          bool(re.search(r"baseRefName[^.]{0,40}sole authority",
                         " ".join(body.split()), re.I)))
    check(f"{name}: proves policy-required checks against live protection contexts",
          "required_status_checks.contexts" in cmd_text)
    # Review of 58d29370: `gh pr checks` sorts into FIVE buckets (pass, fail, pending,
    # skipping, cancel), so collecting only the pending and failing names silently dropped
    # `cancel` — a cancelled required check read as green. The gate is now a row per required
    # check with an explicit value for the unreported ones, so there is nothing to drop.
    squashed = cmd_text.replace(" ", "")
    check(f"{name}: builds one row per required check over POLICY union LIVE",
          "REQUIRED_STATE" in cmd_text and "INDEX(" in cmd_text
          and "$policy+$live|unique" in squashed)
    check(f"{name}: an unreported required check becomes an explicit value, not a gap",
          '"ABSENT"' in cmd_text)
    check(f"{name}: reads each check's state, not only its bucket",
          "name,state,bucket" in cmd_text)
    check(f"{name}: gates on no single pending spelling",
          'state=="PENDING"' not in squashed)
    # A bare `gh pr merge` against a merge-queue base ENQUEUES rather than merging, which is a
    # deferred merge on stale evidence — the thing dropping `--auto` was meant to prevent.
    check(f"{name}: detects a merge-queue base and the PR's queue membership",
          "mergeQueue" in cmd_text and "isInMergeQueue" in cmd_text)
    # The documented strategy exception must be selectable, not merely displayed.
    check(f"{name}: can actually select the documented strategy exception",
          "merge.exception.strategy" in cmd_text)
    check(f"{name}: reads review threads, and paginates them rather than reading one page",
          "reviewThreads" in cmd_text
          and all(tok in cmd_text for tok in ("--paginate", "hasNextPage", "endCursor")))
    check(f"{name}: treats an unsuccessful protection load as missing evidence",
          bool(re.search(r"unsuccessful load is missing evidence", " ".join(body.split()), re.I))
          and "LIVE=UNAVAILABLE" in body)

    # -- (3c) success is only ever reported from freshly re-read state.
    check(f"{name}: confirms merged state before the post-merge steps",
          "state,mergedAt,mergeCommit" in cmd_text)
    check(f"{name}: requires a fresh MERGED state before reporting a merge",
          bool(re.search(r"Never report a merge that a fresh `state: MERGED` has not confirmed",
                         " ".join(body.split()))))
    check(f"{name}: pulls the actual base branch, not a hardcoded main",
          "git checkout main" not in body)

    for mirror in mirrors:
        check(f"{name}: provider mirror {mirror.relative_to(ROOT)} is byte-identical",
              mirror.read_text(encoding="utf-8") == body)

# CONTROLS: every authority check must reject the pre-#2656 skill text.
_old_admin = ('   ```bash\n   gh pr merge <N> --match-head-commit "${HEAD_OID}" '
              '--admin --"${STRATEGY}"\n   ```\n')
check("CONTROL: the pre-#2656 --admin command WOULD be caught in a fence",
      any(ADMIN_FLAG_RE.search(f) for f in fenced_lines(_old_admin)))
check("CONTROL: the pre-#2656 --admin command WOULD be caught as an invocation line",
      any(ADMIN_INVOKE_RE.search(ln) for ln in _old_admin.splitlines()))
_old_auto = ('   ```bash\n   gh pr merge <N> --match-head-commit "${HEAD_OID}" \\\n'
             "     $(jq -r '.merge.auto_merge.gh_flags|join(\" \")' ops/state/truth/policy.json)"
             ' --"${STRATEGY}"\n   ```\n')
check("CONTROL: the pre-#2656 auto-merge command WOULD be caught",
      any(AUTO_FLAG_RE.search(f) for f in fenced_lines(_old_auto)))
check("CONTROL: `argument-hint: \"[PR number] [--admin]\"` WOULD be caught",
      bool(ADMIN_FLAG_RE.search('argument-hint: "[PR number] [--admin]"')))
check("CONTROL: the pre-#2651 skill body would FAIL the strategy-literal check",
      bool(MERGE_CMD_RE.search("3. If all checks are green, merge:\n   - `gh pr merge --merge`")))
check("CONTROL: a bare `gh pr view --json state` would FAIL the addressing check",
      not re.search(r"gh pr (view|checks|merge) +(<N>|\$)", "gh pr view --json state"))
check("CONTROL: two merge invocations would FAIL the single-invocation check",
      len(["gh pr merge <N> --squash", "gh pr merge <N> --admin"]) != 1)

# --- (4) every non-ready state terminates -----------------------------------
# The procedure's whole contract is "merge, or stop". A state that is neither merged nor
# explicitly stopped is a state nobody decided about — the defect class that produced an
# unreachable branch twice in this PR's history. Each class below must be named as a stop.
print("every non-ready state terminates")

TERMINATING = {
    "a pending required check": r"required check still pending",
    "a stalled runner (reported, never escalated)": r"whether or not the\s+runner is stalled",
    "a failing or aborted required check": r"STARTUP_FAILURE",
    "an unknown mergeable state": r"mergeable: `?UNKNOWN",
    "a blocked/behind merge state": r"`BLOCKED`, `BEHIND`",
    "a draft or unreviewed PR": r"a draft PR, `CHANGES_REQUESTED`",
    "evidence that could not be loaded": r"could not be loaded",
    "a required check GitHub never reported": r"`ABSENT`",
    "a base that defers merges to a queue": r"deferred to a merge queue",
}
for _n, _c, _m in skill_paths:
    body = _c.read_text(encoding="utf-8")
    flat = " ".join(body.split())
    for label, pattern in TERMINATING.items():
        check(f"merge-pr: {label} is named as a stop condition",
              bool(re.search(pattern, body) or re.search(pattern, flat)))
    check("merge-pr: states there is no weaker route out of the decision step",
          bool(re.search(r"no weaker route out of this step", flat, re.I)))
    check("merge-pr: states that stopping is the complete outcome, not a deferral",
          bool(re.search(r"Stopping is the complete and correct outcome", flat, re.I)))
    check("merge-pr: states it does not route to the ADR-0016 human exception",
          bool(re.search(r"does not execute it, evaluate it, or route to it", flat, re.I)))

print()
if failures:
    print(f"check-merge-policy: {len(failures)} failure(s)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("check-merge-policy: clean")
