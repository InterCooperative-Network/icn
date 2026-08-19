#!/usr/bin/env python3
"""Synthetic tests for the volatile-currency + SessionStart guards (icn#2634).

Both guards exist to stop a specific, observed failure: `ops/state/sprint/current.json`
stood as the registered answer to "what is being worked on" while describing a sprint
closed five months earlier, and `.claude/settings.json`'s SessionStart hook grepped two
planning files that were absent from the repo.

A guard that cannot fail is not a guard, so every MUST-FAIL case below is a
reconstruction of the real pre-fix state, and every MUST-PASS case is a control
proving the guard is not simply failing on everything.

Run: python3 scripts/tests/test_sprint_state_invariants.py
"""
import importlib.util
import json
import os
import pathlib
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPINE = ROOT / "scripts" / "check-truth-spine.py"

spec = importlib.util.spec_from_file_location("cts_sprint_under_test", SPINE)
cts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cts)

failures = []


def check(desc, cond):
    if cond:
        print(f"  ok   {desc}")
    else:
        print(f"  FAIL {desc}")
        failures.append(desc)


# ---------------------------------------------------------------------------
# 1. declares_dormancy — the predicate the currency invariant turns on.
# ---------------------------------------------------------------------------
print("declares_dormancy")

check("cadence: dormant is a dormancy declaration", cts.declares_dormancy({"cadence": "dormant"}))
check("lifecycle: inactive is a dormancy declaration", cts.declares_dormancy({"lifecycle": "inactive"}))
check("active_sprint: null is a dormancy declaration", cts.declares_dormancy({"active_sprint": None}))
check("case/whitespace tolerant", cts.declares_dormancy({"cadence": "  Dormant "}))

# The pre-fix record: closed, five months old, and silent about dormancy.
PRE_FIX = {"sprint": 26, "status": "closed", "start_date": "2026-03-23", "tasks": []}
check("pre-fix closed sprint record does NOT declare dormancy", not cts.declares_dormancy(PRE_FIX))
check("empty owner does not declare dormancy by omission", not cts.declares_dormancy({}))
check("an ACTIVE sprint does not declare dormancy", not cts.declares_dormancy({"cadence": "active", "active_sprint": 31}))
# Fail-open probe: an absent active_* key must not read as null.
check("absent active_* key is not dormancy", not cts.declares_dormancy({"sprint": 26}))
# A self-contradictory owner has declared nothing: an explicit ACTIVE cadence
# must not be rescued into "dormant" by an `active_*: null` sibling, or a
# terminal record slips past the invariant by accident.
check("cadence: active + active_sprint: null is NOT dormancy",
      not cts.declares_dormancy({"cadence": "active", "active_sprint": None}))
check("lifecycle: running + active_sprint: null is NOT dormancy",
      not cts.declares_dormancy({"lifecycle": "running", "active_sprint": None}))


# ---------------------------------------------------------------------------
# 2. Volatile-currency invariant, end to end through main().
# ---------------------------------------------------------------------------
print("volatile currency invariant")


def run_spine(owner_payload, stability="volatile", settings=None):
    """Build a minimal repo, run the checker, return (exit_code, stdout)."""
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td)
        (root / "ops/state/truth").mkdir(parents=True)
        (root / "ops/state/sprint").mkdir(parents=True)
        (root / "ops/state/truth/sources.json").write_text(json.dumps({
            "domains": {
                "sprint_state": {
                    "owner": "ops/state/sprint/current.json",
                    "stability": stability,
                    "description": "test",
                }
            }
        }))
        (root / "ops/state/sprint/current.json").write_text(json.dumps(owner_payload))
        if settings is not None:
            (root / ".claude").mkdir(parents=True)
            (root / ".claude/settings.json").write_text(json.dumps(settings))

        import contextlib, io
        cts.warnings.clear()
        cts.errors.clear()
        argv = sys.argv[:]
        sys.argv = ["check-truth-spine.py", "--repo-root", str(root)]
        buf = io.StringIO()
        try:
            with contextlib.redirect_stdout(buf):
                code = cts.main()
        finally:
            sys.argv = argv
        return code, buf.getvalue()


code, out = run_spine(PRE_FIX)
check("MUST FAIL: closed volatile owner with no dormancy declaration", code == 1)
check("  ...and says why (terminal record)", "terminal" in out)

code, out = run_spine({**PRE_FIX, "cadence": "dormant"})
check("MUST PASS: same closed record once dormancy is declared", code == 0)

code, out = run_spine({"cadence": "active", "active_sprint": 31, "status": "active", "tasks": []})
check("MUST PASS: a genuinely active sprint", code == 0)

# Control: the invariant is scoped to `volatile`. A slow-changing domain that
# legitimately archives terminal records must not be caught by it.
code, out = run_spine(PRE_FIX, stability="slow-changing")
check("CONTROL: identical closed record is fine for a slow-changing domain", code == 0)

# Calendar superstition must be gone: a dormant record is never "too old".
code, out = run_spine({"cadence": "dormant", "status": "closed", "start_date": "2020-01-01"})
check("MUST PASS: dormant record is not nagged for age", code == 0)
check("  ...and emits no staleness warning", "old (>" not in out)

# ...but an ACTIVE volatile owner still gets the freshness check.
code, out = run_spine({"cadence": "active", "active_sprint": 9, "status": "active", "start_date": "2020-01-01"})
check("CONTROL: an ACTIVE volatile owner is still checked for staleness", "old (>" in out)


# ---------------------------------------------------------------------------
# 3. SessionStart source-existence guard.
# ---------------------------------------------------------------------------
print("SessionStart source guard")

OK_SETTINGS = {"hooks": {"SessionStart": [{"matcher": "startup", "hooks": [
    {"type": "command", "command": "echo hi"}]}]}}
code, out = run_spine({"cadence": "dormant"}, settings=OK_SETTINGS)
check("MUST PASS: SessionStart naming no repo paths", code == 0)

# The real pre-fix command, verbatim in shape.
DEAD_SETTINGS = {"hooks": {"SessionStart": [{"matcher": "startup", "hooks": [{
    "type": "command",
    "command": ("SPRINT=$(grep -m1 '^## ' docs/strategy/ICN-Active-Sprint.md 2>/dev/null "
                "|| grep -m1 'Phase 2' docs/planning/FORWARD_PLAN_2026-03.md 2>/dev/null)"),
}]}]}}


def run_with_docs(settings, make_paths=()):
    """Same as run_spine but with a `docs/` tree, so the first-segment scoping
    rule engages exactly as it does in the real repo."""
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td)
        (root / "ops/state/truth").mkdir(parents=True)
        (root / "ops/state/sprint").mkdir(parents=True)
        (root / "docs").mkdir(parents=True)
        (root / ".claude").mkdir(parents=True)
        (root / "ops/state/truth/sources.json").write_text(json.dumps({"domains": {}}))
        (root / ".claude/settings.json").write_text(json.dumps(settings))
        for rel in make_paths:
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("# fixture\n")
        import contextlib, io
        cts.warnings.clear()
        cts.errors.clear()
        argv = sys.argv[:]
        sys.argv = ["check-truth-spine.py", "--repo-root", str(root)]
        buf = io.StringIO()
        try:
            with contextlib.redirect_stdout(buf):
                code = cts.main()
        finally:
            sys.argv = argv
        return code, buf.getvalue()


code, out = run_with_docs(DEAD_SETTINGS)
check("MUST FAIL: SessionStart greps two missing planning files", code == 1)
check("  ...naming the missing sprint doc", "ICN-Active-Sprint.md" in out)
check("  ...naming the missing forward plan", "FORWARD_PLAN_2026-03.md" in out)

code, out = run_with_docs(DEAD_SETTINGS, make_paths=(
    "docs/strategy/ICN-Active-Sprint.md", "docs/planning/FORWARD_PLAN_2026-03.md"))
check("CONTROL: same command passes once those files exist", code == 0)

# One level deep: moving the dead path into the invoked script must not evade it.
NESTED = {"hooks": {"SessionStart": [{"matcher": "startup", "hooks": [{
    "type": "command", "command": '"$CLAUDE_PROJECT_DIR"/.claude/hooks/orient.sh'}]}]}}
code, out = run_with_docs(NESTED, make_paths=(".claude/hooks/orient.sh",))
check("CONTROL: hook script that names nothing is fine", code == 0)

with tempfile.TemporaryDirectory() as td:
    root = pathlib.Path(td)
    for d in ("ops/state/truth", "docs", ".claude/hooks"):
        (root / d).mkdir(parents=True, exist_ok=True)
    (root / "ops/state/truth/sources.json").write_text(json.dumps({"domains": {}}))
    (root / ".claude/settings.json").write_text(json.dumps(NESTED))
    (root / ".claude/hooks/orient.sh").write_text(
        "#!/bin/bash\ncat docs/planning/FORWARD_PLAN_2026-03.md\n")
    import contextlib, io
    cts.warnings.clear(); cts.errors.clear()
    argv = sys.argv[:]
    sys.argv = ["check-truth-spine.py", "--repo-root", str(root)]
    buf = io.StringIO()
    try:
        with contextlib.redirect_stdout(buf):
            code = cts.main()
    finally:
        sys.argv = argv
    out = buf.getvalue()
check("MUST FAIL: dead path hidden inside the invoked hook script", code == 1)
check("  ...attributed to the script, not the settings file", "orient.sh" in out)

# System paths must never be mistaken for missing repo files.
SYS = {"hooks": {"SessionStart": [{"matcher": "startup", "hooks": [{
    "type": "command", "command": "touch /tmp/.guard-marker.txt && cat /etc/os-release.conf"}]}]}}
code, out = run_with_docs(SYS)
check("CONTROL: absolute system paths are not flagged as repo files", code == 0)


# ---------------------------------------------------------------------------
# 4. SessionStart hook runtime semantics (icn#2634).
#
# The hook is the surface an agent sees BEFORE it can check anything, so its
# failure modes are the ones that matter most. Two rules are pinned here:
#
#   (a) dormancy must be DECLARED. An owner silent about its cadence, or one
#       whose cadence and active_sprint contradict each other, must resolve to
#       "unresolved" — never to "none active". Reporting silence as dormancy is
#       a positive claim the hook has not established, and it is exactly the
#       fail-open declares_dormancy() refuses above.
#   (b) the hook's dormancy vocabulary must not drift from the checker's. This
#       is asserted BEHAVIOURALLY, by driving the hook with each value in
#       cts.DORMANT_VALUES / cts.ACTIVE_VALUES, rather than by grepping the
#       script for literals — a source scan would pass on a script that never
#       runs.
#
# The hook must also never block a session: every case asserts exit status 0.
# ---------------------------------------------------------------------------
print("SessionStart hook runtime semantics")

import subprocess

HOOK = ROOT / ".claude" / "hooks" / "session-orient.sh"
GOOD_SOURCES = {"domains": {"sprint_state": {
    "owner": "ops/state/sprint/current.json", "stability": "volatile"}}}


def run_hook(owner, sources=None, write_owner=True):
    """Run the real hook against a synthetic project dir; return (rc, sprint line)."""
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td)
        (root / "ops/state/truth").mkdir(parents=True)
        (root / "ops/state/sprint").mkdir(parents=True)
        (root / "ops/state/truth/sources.json").write_text(
            sources if isinstance(sources, str)
            else json.dumps(GOOD_SOURCES if sources is None else sources))
        if write_owner:
            (root / "ops/state/sprint/current.json").write_text(
                owner if isinstance(owner, str) else json.dumps(owner))
        proc = subprocess.run(
            ["bash", str(HOOK)], capture_output=True, text=True,
            env={**os.environ, "CLAUDE_PROJECT_DIR": str(root)})
    line = ""
    for ln in proc.stdout.splitlines():
        if ln.startswith("ICN Session |"):
            line = ln.split("sprint: ", 1)[-1]
    return proc.returncode, line


def hook_case(desc, owner, expect_prefix, **kw):
    rc, line = run_hook(owner, **kw)
    check(f"{desc} -> {expect_prefix!r}", line.startswith(expect_prefix))
    check(f"  ...and never blocks the session (rc=0), got rc={rc}", rc == 0)


# --- (a) declared states resolve; silence and contradiction do not -----------
hook_case("dormant owner", {"cadence": "dormant", "active_sprint": None,
                            "status": "closed"}, "none active")
hook_case("active owner", {"cadence": "active", "active_sprint": 31,
                           "status": "in-progress"}, "31 (in-progress)")
hook_case("active_sprint: null with no cadence key",
          {"active_sprint": None, "status": "closed"}, "none active")

# THE FAIL-OPEN: the pre-v2 record says nothing about cadence and carries no
# active_sprint key. `.get()` returns None for both, so an `is None` test reads
# silence as dormancy and the banner asserts "no sprint is running" on evidence
# it does not have.
hook_case("MUST NOT infer dormancy from an absent active_sprint key",
          {"sprint": 26, "status": "closed", "tasks": []}, "unresolved")
hook_case("MUST NOT resolve a contradictory owner (active + null)",
          {"cadence": "active", "active_sprint": None, "status": "in-progress"},
          "unresolved")
hook_case("MUST NOT resolve a contradictory owner (dormant + a sprint number)",
          {"cadence": "dormant", "active_sprint": 31}, "unresolved")
hook_case("MUST NOT trust an unrecognised cadence",
          {"cadence": "weekly", "active_sprint": 31, "status": "in-progress"},
          "unresolved")

# --- degradation paths: honest, specific, non-blocking ----------------------
hook_case("owner file missing", None, "unresolved", write_owner=False)
hook_case("owner is malformed JSON", '{"cadence": "dormant",,,', "unresolved")
hook_case("owner is not an object", ["not", "an", "object"], "unresolved")
hook_case("truth spine is malformed JSON", {"cadence": "dormant"},
          "unresolved", sources='{"domains":{,,')
hook_case("truth spine is not an object", {"cadence": "dormant"},
          "unresolved", sources="[1,2,3]")
hook_case("no sprint_state owner registered", {"cadence": "dormant"},
          "unresolved", sources={"domains": {"sprint_state": {"stability": "volatile"}}})
# A registered owner may carry a #fragment; the hook must strip it, not fail.
hook_case("owner path carries a #fragment",
          {"cadence": "dormant", "active_sprint": None}, "none active",
          sources={"domains": {"sprint_state": {
              "owner": "ops/state/sprint/current.json#cadence",
              "stability": "volatile"}}})

# --- (b) vocabulary drift guard, asserted through the running hook ----------
for value in sorted(cts.DORMANT_VALUES):
    _, line = run_hook({"cadence": value})
    check(f"checker dormancy value {value!r} reads as dormant in the hook",
          line.startswith("none active"))
for value in sorted(cts.ACTIVE_VALUES):
    _, line = run_hook({"cadence": value, "active_sprint": 7, "status": "open"})
    check(f"checker active value {value!r} reads as active in the hook",
          line.startswith("7 ("))
# Control: the drift guard above would be vacuous if EVERY cadence read as
# dormant, so prove an out-of-vocabulary value does not.
_, line = run_hook({"cadence": "sporadic"})
check("CONTROL: an out-of-vocabulary cadence is neither dormant nor active",
      line.startswith("unresolved"))

# The checked-in owner must itself resolve cleanly through the real hook.
rc, line = run_hook(json.loads((ROOT / "ops/state/sprint/current.json").read_text()))
check("the checked-in sprint owner resolves without 'unresolved'",
      rc == 0 and not line.startswith("unresolved"))


print()
if failures:
    print(f"test_sprint_state_invariants: {len(failures)} FAILURE(S)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("test_sprint_state_invariants: all checks passed")
