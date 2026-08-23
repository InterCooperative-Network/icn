#!/usr/bin/env python3
"""test_pre_bash_guard.py — the Bash guard must block exactly the two unsafe wait shapes.

The two shapes have DIFFERENT properties, and the guard must state each correctly:

  A. self-matching `pgrep -f`  — LOGICALLY NON-TERMINATING. The observer matches itself, so no
     future event from any process can satisfy the predicate.

  B. unbounded sentinel wait   — NOT logically impossible. Another process may legitimately
     create the file later. The defect is that, with stderr swallowed and no bound and no
     producer identity, it cannot distinguish a working producer from a dead one, a deleted
     scratch directory, or a sentinel that will never arrive — so it can spin indefinitely
     while appearing active. Blocked in that unbounded form only; bounded forms are fine.

Overstating B as "impossible" would be a false claim in an error message an agent is expected
to trust, so the wording of each block is asserted, not just the exit code.

Two failure modes matter equally and are tested equally:

  FALSE NEGATIVE  a non-terminating loop gets through. That is how a merged lane ends up
                  pinned for days.
  FALSE POSITIVE  a correct wait gets refused. A guard that cries wolf gets worked around,
                  and a worked-around guard protects nothing. The `[m]utate.py` bracket idiom
                  is the specific safe pattern that must never be flagged.

The blocked cases are the ACTUAL command lines recovered from icn-dev's process table during
the #2644 lifecycle investigation, not invented examples.

Usage: python3 scripts/tests/test_pre_bash_guard.py
Refs icn#2653.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

GUARD = Path(__file__).resolve().parents[2] / ".claude" / "hooks" / "pre-bash-guard.py"

BLOCK = 2
ALLOW = 0

CASES: list[tuple[str, int, str]] = [
    # ── recovered live from icn-dev (PIDs 296141, 2843900, 220945) ──
    (
        'until ! pgrep -f "scratchpad/mutate.py" > /dev/null; do sleep 20; done',
        BLOCK,
        "self-matching pgrep (PID 296141, ran 2.8 days)",
    ),
    (
        'until ! pgrep -f "scratchpad/run_mutations.sh" >/dev/null 2>&1; do sleep 5; done',
        BLOCK,
        "self-matching pgrep (PID 2843900, ran 1.7 days)",
    ),
    (
        'until grep -q "^EXIT=" /tmp/claude/scratchpad/full-test-3.log 2>/dev/null; do sleep 20; done',
        BLOCK,
        "sentinel wait with swallowed ENOENT (PID 220945, ran 1.2 days)",
    ),
    (
        'until ! pgrep -f "/cargo build -p icn-net" >/dev/null 2>&1; do sleep 10; done',
        BLOCK,
        "same defect found in a second lane (PID 1023810)",
    ),
    # ── variants of the same defect ──
    ('while pgrep -f my-script.sh > /dev/null; do sleep 3; done', BLOCK, "while-form self-match"),
    ("until ! pgrep -af build.sh; do sleep 2; done", BLOCK, "-af flag ordering"),
    # ── must NOT be blocked ──
    (
        'until ! pgrep -f "[m]utate.py"; do sleep 5; done',
        ALLOW,
        "SAFE: bracket trick cannot match the observing shell",
    ),
    (
        'until ! pgrep -f "[c]argo build"; do sleep 5; done',
        ALLOW,
        "SAFE: bracket trick, spaces in pattern",
    ),
    ("pgrep -f cargo", ALLOW, "single pgrep, not a loop"),
    ("pgrep -f cargo && echo running", ALLOW, "pgrep in a conditional, not a loop"),
    (
        'timeout 600 bash -c "until ! pgrep -f foo; do sleep 5; done"',
        ALLOW,
        "bounded by an external timeout",
    ),
    (
        'until [ -f /tmp/x ]; do sleep 1; SECONDS -gt 60 && break; done',
        ALLOW,
        "has its own escape hatch",
    ),
    (
        "icn-wait match 'scratchpad/mutate.py' --timeout 600",
        ALLOW,
        "the supported helper must never be blocked",
    ),
    (
        "icn-wait file /tmp/x.log --pattern '^EXIT=' --timeout 600",
        ALLOW,
        "supported sentinel wait",
    ),
    ("cargo test --workspace", ALLOW, "ordinary command"),
    ("git checkout main", ALLOW, "advisory note only, never blocked"),
    (
        'until [ -f /tmp/ready ]; do sleep 5; done',
        ALLOW,
        "bare file poll without swallowed stderr is not the refused shape",
    ),
    # ── shape B is about BOUNDEDNESS + EVIDENCE, not about waiting on files ──
    (
        'until grep -q "^EXIT=" /tmp/run.log 2>/dev/null; do sleep 5; timeout 60 true; done',
        ALLOW,
        "SAFE: sentinel wait that is bounded is allowed",
    ),
    (
        'until grep -q "^EXIT=" /tmp/run.log 2>/dev/null; do sleep 5; [ $SECONDS -gt 600 ] && break; done',
        ALLOW,
        "SAFE: sentinel wait with its own bound is allowed",
    ),
    (
        "icn-wait file /tmp/run.log --pattern '^EXIT=' --source-pid 4242 --timeout 600",
        ALLOW,
        "SAFE: the supported form supplies both a bound and producer evidence",
    ),
    # ── ordinary legitimate loops must not be caught ──
    ("for f in *.rs; do echo $f; done", ALLOW, "ordinary for-loop"),
    (
        'while read -r line; do echo "$line"; done < input.txt',
        ALLOW,
        "ordinary while-read loop",
    ),
    (
        'while [ $i -lt 5 ]; do i=$((i+1)); sleep 1; done',
        ALLOW,
        "bounded counting loop with a sleep",
    ),
    (
        "cargo test 2>/dev/null || echo failed",
        ALLOW,
        "swallowed stderr outside any loop",
    ),
]

# The block message must describe the ACTUAL property of each shape. An error message an agent
# is told to trust must not overstate its case.
MESSAGE_CONTRACT = [
    (
        'until ! pgrep -f "scratchpad/mutate.py"; do sleep 20; done',
        ["can never terminate", "matches full command lines", "icn-wait"],
        ["may legitimately create"],
        "self-match message claims impossibility (correctly)",
    ),
    (
        'until grep -q "^EXIT=" /tmp/x.log 2>/dev/null; do sleep 20; done',
        ["not impossible in principle", "producer died", "--source-pid", "unbounded"],
        ["can never terminate", "no future event"],
        "sentinel message describes indistinguishability, NOT impossibility",
    ),
]


def run(cmd: str) -> tuple[int, str]:
    p = subprocess.run(
        [sys.executable, str(GUARD)],
        input=json.dumps({"tool_input": {"command": cmd}}),
        capture_output=True,
        text=True,
    )
    return p.returncode, p.stderr


def main() -> int:
    failures = 0
    for cmd, want, label in CASES:
        got, err = run(cmd)
        if got != want:
            failures += 1
            verdict = "let through" if want == BLOCK else "wrongly blocked"
            print(f"  FAIL  ({verdict}) {label}\n        cmd: {cmd}", file=sys.stderr)
            if err:
                print(f"        stderr: {err.strip()[:200]}", file=sys.stderr)
        else:
            print(f"  ok    {'BLOCK' if want == BLOCK else 'allow'}  {label}")

    # A block must explain itself accurately: an unexplained refusal is indistinguishable from
    # a bug, and an OVERSTATED one teaches the agent something false.
    checks = 0
    for cmd, must_have, must_not_have, label in MESSAGE_CONTRACT:
        _, err = run(cmd)
        low = err.lower()
        ok_msg = True
        for needle in must_have:
            checks += 1
            if needle.lower() not in low:
                failures += 1
                ok_msg = False
                print(f"  FAIL  message missing {needle!r} ({label})", file=sys.stderr)
        for needle in must_not_have:
            checks += 1
            if needle.lower() in low:
                failures += 1
                ok_msg = False
                print(f"  FAIL  message OVERSTATES: contains {needle!r} ({label})", file=sys.stderr)
        if ok_msg:
            print(f"  ok    {label}")

    print(f"\npassed: {len(CASES) + checks - failures}  failed: {failures}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
