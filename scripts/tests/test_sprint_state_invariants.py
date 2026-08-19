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


print()
if failures:
    print(f"test_sprint_state_invariants: {len(failures)} FAILURE(S)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("test_sprint_state_invariants: all checks passed")
