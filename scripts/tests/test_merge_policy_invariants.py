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

# The whole policy blob must not compare any field against a value its enum cannot hold.
blob = json.dumps({k: v for k, v in merge.items() if k != "admin_bypass"}) + json.dumps(
    {k: v for k, v in bypass.items() if k != "field_note"})
check("no `mergeStateStatus=MERGEABLE` anywhere in the policy",
      "mergeStateStatus=MERGEABLE" not in blob)

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
    for mirror in mirrors:
        check(f"{name}: provider mirror {mirror.relative_to(ROOT)} is byte-identical",
              mirror.read_text(encoding="utf-8") == body)

# CONTROL: the regex must actually catch the pre-fix skill text.
check("CONTROL: the pre-#2651 skill body would FAIL the strategy-literal check",
      bool(MERGE_CMD_RE.search("3. If all checks are green, merge:\n   - `gh pr merge --merge`")))
check("CONTROL: a bare `gh pr view --json state` would FAIL the addressing check",
      not re.search(r"gh pr (view|checks|merge) +(<N>|\$)", "gh pr view --json state"))

print()
if failures:
    print(f"check-merge-policy: {len(failures)} failure(s)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("check-merge-policy: clean")
