#!/usr/bin/env python3
"""Merge-policy invariants (icn#2651, icn#2656).

`ops/state/truth/policy.json` is the registered owner of merge requirements. Two classes of
drift got past every existing gate:

  1. The owner named a field that cannot hold the value it was compared against.
     `admin_bypass.condition` read `mergeStateStatus=MERGEABLE`. `MERGEABLE` is a member of
     GitHub's `MergeableState` enum (the `mergeable` field); `mergeStateStatus` is
     `MergeStateStatus` and has no such member. The documented admin exception was therefore
     unsatisfiable, and nothing noticed because prose is not type-checked.

  2. A consumer restated a value the owner owns. `.agents/skills/merge-pr/SKILL.md` merged with
     `--merge` while `default_strategy` was `squash`, and `auto_merge.command` baked `--squash`
     into a string inside the owner itself — so the owner duplicated its own field and could
     contradict it.

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
STRATEGY_FLAG_RE = re.compile(r"--(merge|squash|rebase)\b")

failures = []


def check(desc, cond):
    if cond:
        print(f"  ok   {desc}")
    else:
        print(f"  FAIL {desc}")
        failures.append(desc)


merge = json.loads(POLICY.read_text(encoding="utf-8"))["merge"]

# --- (1) impossible field/value combinations in the canonical policy ---------
print("field/value coherence")

strategy = merge.get("default_strategy")
check(f"default_strategy {strategy!r} is a real gh pr merge strategy",
      strategy in GH_MERGE_STRATEGIES)

bypass = merge.get("admin_bypass", {})
req = bypass.get("requires")
check("admin_bypass declares machine-checkable `requires`, not only prose",
      isinstance(req, dict) and req != {})
check("admin_bypass still declares never_for", bool(bypass.get("never_for")))
check("admin_bypass declares fail_closed behaviour", bool(bypass.get("fail_closed")))

if isinstance(req, dict):
    check(f"requires.mergeable {req.get('mergeable')!r} is a MergeableState member",
          req.get("mergeable") in MERGEABLE_STATE)
    mss = req.get("merge_state_status_in", [])
    check("requires.merge_state_status_in is a non-empty list",
          isinstance(mss, list) and len(mss) > 0)
    check(f"every merge_state_status_in value is a MergeStateStatus member: {mss}",
          isinstance(mss, list) and all(v in MERGE_STATE_STATUS for v in mss))
    # THE REGRESSION. The pre-fix owner compared mergeStateStatus against MERGEABLE.
    check("no MergeableState value has leaked into a mergeStateStatus field",
          isinstance(mss, list)
          and not (set(mss) & (MERGEABLE_STATE - MERGE_STATE_STATUS)))
    # CLEAN needs no bypass; permitting it would make the exception a general bypass.
    check("admin bypass is not permitted on a CLEAN merge state",
          isinstance(mss, list) and "CLEAN" not in mss)

# The WHOLE FILE must not associate either field with a value the other's enum owns. Review on
# #2656 caught the first version of this check: it rejected the one literal
# `mergeStateStatus=MERGEABLE` and sailed past `readiness_definition`'s
# "mergeStateStatus is MERGEABLE or UNSTABLE" — the identical truth conflict, spelled with a
# word instead of an `=`, in a top-level key the check never looked at. A guard that only knows
# one spelling of a defect is a guard against that spelling, not against the defect.
print("no field is associated with the other enum's values, anywhere in the file")

# Values each enum owns EXCLUSIVELY. UNKNOWN is in both, so it proves nothing and is omitted.
MERGEABLE_ONLY = MERGEABLE_STATE - MERGE_STATE_STATUS       # MERGEABLE, CONFLICTING
STATUS_ONLY = MERGE_STATE_STATUS - MERGEABLE_STATE          # DIRTY, BLOCKED, BEHIND, ...

# Keys whose whole purpose is to DOCUMENT the confusion. Excluded by name, not by pattern, so
# adding a new prose key cannot silently widen the exemption.
DOC_KEYS = {"field_note", "allowlist_note", "scope_note", "note",
            "agent_tooling_check_note", "description"}


def walk_strings(node, key=None, path="$"):
    """Yield (path, key, string) for every string value in the document."""
    if isinstance(node, dict):
        for k, v in node.items():
            yield from walk_strings(v, k, f"{path}.{k}")
    elif isinstance(node, list):
        for n, v in enumerate(node):
            yield from walk_strings(v, key, f"{path}[{n}]")
    elif isinstance(node, str):
        yield path, key, node


# An association holds only inside ONE clause about ONE field. The window therefore ends at the
# next mention of EITHER field — otherwise "mergeable is MERGEABLE, and mergeStateStatus is
# CLEAN" reads as `mergeable … CLEAN` and the checker flags the sentence that states the rule
# correctly. It also ends at a sentence break or a negation, so prose that says a field NEVER
# takes a value is not read as saying it does.
# CASE-SENSITIVE on purpose: the field names are camelCase and the enum values are UPPERCASE,
# so an IGNORECASE pattern made `mergeable` match the VALUE `MERGEABLE` and truncated the
# window to nothing — which silently disarmed the whole check. Caught by its own controls.
FIELD_RE = re.compile(r"\bmergeStateStatus\b|\bmergeable\b")


def associations(text: str, field: str, values: set[str]) -> list[str]:
    hits = []
    for m in re.finditer(rf"\b{field}\b", text):
        rest = text[m.end():]
        nxt = FIELD_RE.search(rest)
        window = rest[: nxt.start()] if nxt else rest[:80]
        window = re.split(r"[.;]|\bnever\b|\bnot\b|\bdifferent\b", window, maxsplit=1)[0]
        for v in values:
            if re.search(rf"\b{v}\b", window):
                hits.append(f"{field} … {v}")
    return hits


whole = json.loads(POLICY.read_text(encoding="utf-8"))
conflicts = []
for path, key, text in walk_strings(whole):
    if key in DOC_KEYS:
        continue
    conflicts += [f"{path}: {h}" for h in associations(text, "mergeStateStatus", MERGEABLE_ONLY)]
    conflicts += [f"{path}: {h}" for h in associations(text, "mergeable", STATUS_ONLY)]
check(f"no cross-enum field/value association in any string: {conflicts}", not conflicts)

# CONTROLS: the scanner must catch BOTH spellings of the defect, in ANY key.
_c1 = associations("Required checks green AND mergeStateStatus=MERGEABLE but stalled",
                   "mergeStateStatus", MERGEABLE_ONLY)
check("CONTROL: catches the `mergeStateStatus=MERGEABLE` spelling", bool(_c1))
_c2 = associations("mergeStateStatus is MERGEABLE or UNSTABLE", "mergeStateStatus", MERGEABLE_ONLY)
check("CONTROL: catches the `mergeStateStatus is MERGEABLE` spelling", bool(_c2))
_c3 = associations("mergeable is CLEAN", "mergeable", STATUS_ONLY)
check("CONTROL: catches the reverse confusion (`mergeable is CLEAN`)", bool(_c3))
# ...and must NOT fire on the note that documents the distinction.
_c4 = associations("`mergeStateStatus` is a DIFFERENT enum and never takes the value MERGEABLE",
                   "mergeStateStatus", MERGEABLE_ONLY)
check("CONTROL: does not fire on prose documenting the distinction", not _c4)
_c5 = associations("mergeable is MERGEABLE, and mergeStateStatus is CLEAN or UNSTABLE",
                   "mergeable", STATUS_ONLY)
check("CONTROL: a correct two-field sentence is not a conflict", not _c5)
_c6 = associations("mergeable is MERGEABLE, and mergeStateStatus is CLEAN or UNSTABLE",
                   "mergeStateStatus", MERGEABLE_ONLY)
check("CONTROL: ...and its second clause is clean too", not _c6)

# --- (1b) the bypass gate must be fail-closed and revocable ------------------
# Review on #2656: a DENYLIST of bad conclusions silently permits any state nobody enumerated.
# GitHub's check-run conclusions include `stale` and `startup_failure`; legacy commit statuses
# add `error`. Only an allowlist is fail-closed against states GitHub has not invented yet.
print("admin bypass is fail-closed and revocable")

check("admin_bypass exposes an `allowed` switch the owner can flip",
      isinstance(bypass.get("allowed"), bool))
check("admin_bypass uses no conclusion DENYLIST",
      isinstance(req, dict) and "no_required_check_concluded" not in req)
if isinstance(req, dict):
    allow_done = req.get("required_check_conclusion_allowlist")
    allow_pend = req.get("required_check_pending_allowlist")
    check("required_check_conclusion_allowlist is a non-empty list",
          isinstance(allow_done, list) and len(allow_done) > 0)
    check("required_check_pending_allowlist is a non-empty list",
          isinstance(allow_pend, list) and len(allow_pend) > 0)
    # A bypass that tolerates a failure is not a bypass exception, it is a bypass.
    FORBIDDEN = {"FAILURE", "TIMED_OUT", "CANCELLED", "ACTION_REQUIRED",
                 "STALE", "STARTUP_FAILURE", "ERROR"}
    leaked = FORBIDDEN & set(allow_done or []) | FORBIDDEN & set(allow_pend or [])
    check(f"no failing/aborted state is allowlisted: {sorted(leaked)}", not leaked)
    check("the two allowlists are disjoint",
          not (set(allow_done or []) & set(allow_pend or [])))

# The skill must honour the off switch, and must consult BOTH allowlists.
# (asserted against the skill body in section 4)

# --- (1c) the bypass must not skip gates readiness independently requires ----
# `--admin` bypasses EVERY branch protection, not only the check gate. Review on #2656: with a
# stalled runner AND a CHANGES_REQUESTED review, every check-shaped requirement still held.
print("admin bypass reproduces the readiness review gates")

readiness = " ".join(whole.get("readiness_definition", []))
if isinstance(req, dict):
    check("bypass refuses a draft PR", req.get("is_draft") is False)
    # An ALLOWLIST, like every other state requirement here. A `*_not_in` denylist would let a
    # fourth PullRequestReviewDecision value pass by omission — the same fail-open the
    # conclusion denylist had (icn#2656 review).
    check("review decision uses an allowlist, not a denylist",
          "review_decision_not_in" not in req
          and isinstance(req.get("review_decision_allowlist"), list))
    rd_allow = req.get("review_decision_allowlist") or []
    check("bypass refuses CHANGES_REQUESTED", "CHANGES_REQUESTED" not in rd_allow)
    check("bypass refuses REVIEW_REQUIRED", "REVIEW_REQUIRED" not in rd_allow)
    check("the live null decision is admitted EXPLICITLY, not by omission", None in rd_allow)
    check("every non-null allowlisted decision is a real PullRequestReviewDecision",
          all(v in REVIEW_DECISIONS for v in rd_allow if v is not None))
    check("bypass refuses unresolved review threads",
          req.get("unresolved_review_threads") == 0)
    # Tie the two documents together so neither can drift alone.
    if "CHANGES_REQUESTED" in readiness:
        check("readiness requires no CHANGES_REQUESTED, and so does the bypass",
              "CHANGES_REQUESTED" not in rd_allow)
    if "threads resolved" in readiness:
        check("readiness requires resolved threads, and so does the bypass",
              req.get("unresolved_review_threads") == 0)
check("admin_bypass states why it must mirror readiness",
      bool(bypass.get("scope_note")))

# --- (2) required/non-required sets cannot overlap ---------------------------
required = set(merge.get("required_checks", []))
non_blocking = set(merge.get("non_blocking_checks", []))
check("required_checks is non-empty", len(required) > 0)
check(f"required_checks and non_blocking_checks are disjoint: {sorted(required & non_blocking)}",
      not (required & non_blocking))

# --- (3) the owner must not duplicate its own strategy -----------------------
print("no duplicated strategy inside the owner")
auto = merge.get("auto_merge", {})
check("auto_merge carries no executable command string",
      not any(k in auto for k in ("command", "cmd", "run")))
check("auto_merge points at default_strategy rather than naming a strategy",
      auto.get("strategy_from") == "default_strategy")
auto_blob = json.dumps({k: v for k, v in auto.items() if k != "note"})
check(f"auto_merge hardcodes no --merge/--squash/--rebase flag: {auto_blob[:80]}",
      STRATEGY_FLAG_RE.search(auto_blob) is None)
check("auto_merge still declares its flags structurally",
      isinstance(auto.get("gh_flags"), list) and auto["gh_flags"])

# CONTROL: the checks above would be vacuous if they passed on the pre-fix shape.
_pre_fix_auto = {"command": "gh pr merge <N> --auto --squash", "use_when": "..."}
check("CONTROL: the pre-#2656 auto_merge shape would FAIL these checks",
      "command" in _pre_fix_auto and STRATEGY_FLAG_RE.search(json.dumps(_pre_fix_auto)))
_pre_fix_bypass = {"condition": "Required checks are green AND mergeStateStatus=MERGEABLE ..."}
check("CONTROL: the pre-#2656 admin_bypass shape would FAIL these checks",
      "requires" not in _pre_fix_bypass
      and "mergeStateStatus=MERGEABLE" in json.dumps(_pre_fix_bypass))

# Commands live in fenced blocks or in `- \`...\`` bullets. Prose about a command is not a
# command: an earlier revision of this checker flagged the sentence explaining why a bare
# `gh pr view` is wrong, which is the checker failing on its own documentation.
EVAL_RE = re.compile(r"\beval\s+[\"'`$]")


def extract_commands(markdown: str) -> list[str]:
    out, in_fence = [], False
    for line in markdown.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            if line.strip():
                out.append(line.strip())
            continue
        # An inline command BULLET only: `- \`gh pr checks <N>\`, ...`. A command named
        # mid-paragraph is prose about a command, not an instruction to run one.
        if not line.lstrip().startswith("- "):
            continue
        for m in re.finditer(r"`([^`]+)`", line):
            tok = m.group(1).strip()
            if tok.startswith(("gh ", "git ", "jq ", "eval ")):
                out.append(tok)
    return out


# --- (4) consumers must not restate what they claim to load ------------------
print("consumers do not duplicate policy values")

registry = json.loads(SKILLS.read_text(encoding="utf-8"))
skill_paths = []
for entry in registry.get("skills", {}).get("icn_level", []):
    if entry.get("name") in ("merge-pr", "merge-prs", "integrate-pr-stack"):
        skill_paths.append((entry["name"], ROOT / entry["canonical_path"],
                            [ROOT / m["path"] for m in entry.get("provider_mirrors", [])]))
check("the merge skills are resolvable from the canonical registry", len(skill_paths) >= 1)

# `merge-pr` is the thin skill that declares policy.json canonical AND lists the strategy under
# never_hardcode. It is held to its own contract: no literal strategy flag in a merge command.
MERGE_CMD_RE = re.compile(r"gh pr merge[^\n`]*--(merge|squash|rebase)\b")
for name, canonical, mirrors in skill_paths:
    body = canonical.read_text(encoding="utf-8")
    if name != "merge-pr":
        continue
    hits = MERGE_CMD_RE.findall(body)
    check(f"{name}: no `gh pr merge --<strategy>` literal; the strategy is substituted: {hits}",
          not hits)
    check(f"{name}: substitutes the strategy from the policy", "${STRATEGY}" in body)
    # Only real commands are inspected, never prose. A sentence explaining why a bare
    # `gh pr view` is wrong must not be mistaken for one.
    commands = extract_commands(body)
    check(f"{name}: does not shell-eval policy values",
          not any(EVAL_RE.search(c) for c in commands))
    bare = [c for c in commands
            if re.search(r"gh pr (view|checks|merge)\b", c)
            and not re.search(r"gh pr (view|checks|merge) +(<N>|\$)", c)]
    check(f"{name}: every gh pr view/checks/merge command is explicitly addressed: {bare}",
          not bare)
    # Review on #2656: `allowed: false` must actually revoke the bypass. A skill that only
    # checks `.requires` treats a present `false` as neither absent nor ambiguous.
    check(f"{name}: gates the admin path on .merge.admin_bypass.allowed",
          "admin_bypass.allowed" in body)
    check(f"{name}: consults both required-check allowlists",
          "required_check_conclusion_allowlist" in body
          and "required_check_pending_allowlist" in body)
    check(f"{name}: names the states an allowlist must exclude",
          "STARTUP_FAILURE" in body and "STALE" in body)
    # The bypass overrides every protection, so the skill must check the review gates too.
    check(f"{name}: checks the gates --admin would also bypass",
          all(t in body for t in ("is_draft", "review_decision_allowlist",
                                  "unresolved_review_threads")))

    for mirror in mirrors:
        check(f"{name}: provider mirror {mirror.relative_to(ROOT)} is byte-identical",
              mirror.read_text(encoding="utf-8") == body)

# CONTROL: the regex must actually catch the pre-fix skill text.
check("CONTROL: the pre-#2651 skill body would FAIL the strategy-literal check",
      bool(MERGE_CMD_RE.search("3. If all checks are green, merge:\n   - `gh pr merge --merge`")))
check("CONTROL: a bare `gh pr view --json state` would FAIL the addressing check",
      not re.search(r"gh pr (view|checks|merge) +(<N>|\$)", "gh pr view --json state"))

# --- (5) a disabled bypass must be expressible and must be honoured ----------
print("a disabled bypass is expressible")

_disabled = dict(bypass)
_disabled["allowed"] = False
check("CONTROL: `allowed: false` is a valid shape the owner can write",
      _disabled["allowed"] is False and "requires" in _disabled)
# The skill's own text must reach `allowed` BEFORE the detailed requirements, otherwise a
# revoked bypass is still performed.
for name, canonical, _m in skill_paths:
    if name != "merge-pr":
        continue
    body = canonical.read_text(encoding="utf-8")
    i_allowed = body.find("admin_bypass.allowed")
    i_requires = body.find(".requires.mergeable")
    check("merge-pr: `allowed` is checked before the detailed requirements",
          i_allowed != -1 and i_requires != -1 and i_allowed < i_requires)

print()
if failures:
    print(f"check-merge-policy: {len(failures)} failure(s)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("check-merge-policy: clean")
