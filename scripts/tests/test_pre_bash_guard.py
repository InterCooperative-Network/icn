#!/usr/bin/env python3
"""test_pre_bash_guard.py — the Bash guard must block exactly the loops that cannot terminate.

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
        "bare file poll without swallowed stderr is not provably broken",
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

    # A block must explain itself: an unexplained refusal is indistinguishable from a bug.
    _, err = run('until ! pgrep -f "scratchpad/mutate.py"; do sleep 20; done')
    for needle in ("icn-wait", "matches full command lines"):
        if needle not in err:
            failures += 1
            print(f"  FAIL  block message does not mention {needle!r}", file=sys.stderr)
        else:
            print(f"  ok    block message mentions {needle!r}")

    print(f"\npassed: {len(CASES) + 2 - failures}  failed: {failures}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
