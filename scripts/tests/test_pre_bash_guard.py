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

    # ── EVASIONS AND MISFIRES FOUND BY INDEPENDENT REVIEW ──
    # An earlier detection pass matched over the whole command string. It had 11 evasions and
    # 5 misfires, nearly all from two structural mistakes: requiring `sleep` to sit adjacent to
    # `do`/`done`, and looking for the defect anywhere rather than in the loop's own condition.
    # Every case below is verbatim from that review and is now pinned.
    (
        'while pgrep -f "cargo build -p icn-net" >/dev/null; do date; sleep 30; date; done',
        BLOCK,
        "FN: a statement after sleep must not disarm the guard (one-token rewrite of the real loop)",
    ),
    (
        'while ! pgrep -f "cargo build" >/dev/null; do\n  echo waiting;\n  sleep 30\ndone',
        BLOCK,
        "FN: multi-line loop body",
    ),
    (
        'while pgrep -f "cargo build" >/dev/null; do echo "checking every 5 seconds"; sleep 5; done',
        BLOCK,
        "FN: the English word 'seconds' must not read as a bound",
    ),
    (
        'while pgrep -f "cargo test --timeout 60" >/dev/null; do sleep 5; done',
        BLOCK,
        "FN: 'timeout' inside the pgrep pattern must not read as a bound",
    ),
    (
        'while pgrep -f "cargo build" >/dev/null; do sleep 60; done  # will break when the build ends',
        BLOCK,
        "FN: 'break' in a comment must not read as an escape hatch",
    ),
    (
        'while ps aux | grep -q "cargo build -p icn-net"; do sleep 30; done',
        BLOCK,
        "FN: ps|grep is the same self-matching defect as pgrep -f",
    ),
    (
        'until grep -q "^EXIT=" /tmp/s/out.log &>/dev/null; do sleep 10; done',
        BLOCK,
        "FN: bash's &>/dev/null swallows stderr identically to 2>/dev/null",
    ),
    (
        'while ! [ -f /tmp/scratch/EXIT ] 2>/dev/null; do sleep 10; done',
        BLOCK,
        "FN: the [ ... ] test form, not just `test`",
    ),
    (
        'while [ -n "$(pgrep -f \'cargo build\')" ]; do echo poll; sleep 20; echo again; done',
        BLOCK,
        "FN: $() command-substitution form",
    ),
    (
        'until grep -q READY /tmp/s/out 2>/dev/null; do echo .; sleep 5; echo .; done',
        BLOCK,
        "FN: statements around the sleep, sentinel shape",
    ),
    (
        'while IFS= read -r f; do grep -H TODO "$f" 2>/dev/null; sleep 0.1; done < files.txt',
        ALLOW,
        "FP: grep 2>/dev/null in the BODY of a read-loop is output, not a predicate",
    ),
    (
        'while read -r pkg; do cargo test -p "$pkg" 2>/dev/null; sleep 1; done < pkgs.txt',
        ALLOW,
        "FP: the word 'test' in `cargo test` is not a file predicate",
    ),
    (
        'while ! nc -z localhost 5432; do sleep 1; done\nnpm test 2>/dev/null',
        ALLOW,
        "FP: a statement on the next line is outside the loop entirely",
    ),
    (
        'while ! curl -sf localhost:3000/health >/dev/null; do sleep 2; done && cargo test --workspace 2>/dev/null',
        ALLOW,
        "FP: chained statement after the loop must not leak into its condition",
    ),
    (
        'n=0\nwhile [ $n -lt 10 ]; do n=$((n+1)); grep -c ERROR app.log 2>/dev/null; sleep 1; done',
        ALLOW,
        "FP: an explicitly bounded counting loop",
    ),
    (
        'until [ -f /tmp/scratch/DONE ]; do sleep 10; done',
        ALLOW,
        "deliberate: a bare test with no swallowed stderr is not the refused shape",
    ),

    # ── ROUND 2 of independent review: bounding words that were left unanchored ──
    # `timeout`, `SECONDS`, `break` and `icn-wait` were anchored to command positions but
    # `deadline` and `max_?wait` were not, so ANY occurrence disarmed the guard — including
    # inside the pgrep pattern itself, which is the evasion class the docstring claims to fix.
    (
        'until ! pgrep -f "deadline-runner.py"; do sleep 20; done',
        BLOCK,
        "FN: 'deadline' inside the pgrep pattern must not read as a bound",
    ),
    (
        'until ! pgrep -f "max_wait.sh"; do sleep 20; done',
        BLOCK,
        "FN: 'max_wait' inside the pgrep pattern must not read as a bound",
    ),
    (
        'until ! pgrep -f "mutate.py"; do sleep 20; echo "past deadline"; done',
        BLOCK,
        "FN: 'deadline' in loop-body output must not read as a bound",
    ),
    (
        'DEADLINE=$((SECONDS+600)); until ! pgrep -f foo; do sleep 5; [ $SECONDS -gt $DEADLINE ] && break; done',
        ALLOW,
        "a REAL deadline variable still bounds the loop",
    ),
    # ── the guard must not block writing ABOUT the defect ──
    (
        'git commit -m "block until ! pgrep -f x; do sleep 5; done loops"',
        ALLOW,
        "FP: a loop inside a quoted commit message is text, not a command",
    ),
    (
        "cat >> README.md <<'EOF'\nuntil ! pgrep -f \"mutate.py\"; do sleep 20; done\nEOF",
        ALLOW,
        "FP: a loop inside a heredoc body is documentation, not a command",
    ),
    (
        'echo \'until ! pgrep -f x; do sleep 5; done\' > example.txt',
        ALLOW,
        "FP: a single-quoted loop being written to a file",
    ),

    # ── ROUND 3: execution wrappers. A quoted span is not automatically inert — the shell RUNS
    # `bash -c "..."`. Treating it as text let 19/19 non-terminating waits through, and
    # `bash -c` is a one-token rewrite of the real incident loop.
    ('bash -c "until ! pgrep -f qq_marker; do sleep 1; done"', BLOCK, "FN: bash -c wrapper"),
    ('sh -c "until ! pgrep -f zz_marker; do sleep 2; done"', BLOCK, "FN: sh -c wrapper"),
    ('eval "until ! pgrep -f ev_marker; do sleep 2; done"', BLOCK, "FN: eval wrapper"),
    ('bash <<EOF\nuntil ! pgrep -f hd_marker; do sleep 2; done\nEOF', BLOCK,
     "FN: heredoc fed to a shell IS executed"),
    # ── comment/quote parsing order ──
    ('until ! pgrep -f b.sh; do sleep 5; echo "#"; done', BLOCK,
     "FN: a '#' inside a string must not eat the loop's done"),
    ('until ! pgrep -f z.sh; do sleep 5; curl http://h/p#f; done', BLOCK,
     "FN: a URL fragment is not a comment (bash needs # at a word start)"),
    # ── bounding tokens only count outside quotes ──
    ('until ! pgrep -f x.sh; do sleep 5; echo "x; break"; done', BLOCK,
     "FN: 'break' inside a string is not an escape hatch"),
    ('until ! pgrep -f y.sh; do sleep 5; echo "; timeout 5"; done', BLOCK,
     "FN: 'timeout' inside a string is not a bound"),
    ('until ! pgrep -f "deadline=runner.py"; do sleep 5; done', BLOCK,
     "FN: 'deadline=' inside the pattern is not a bound"),
    ('until ! pgrep -f "max_wait=30s-runner"; do sleep 5; done', BLOCK,
     "FN: 'max_wait=' inside the pattern is not a bound"),
    ('while ps aux | grep -q "cargo build -p icn-net"; do sleep 30; done', BLOCK,
     "FN: ps|grep -q is the same self-match defect"),
    # ── correct waits that must NOT be refused ──
    ('end=$((SECONDS+600)); while (( SECONDS < end )) && ! test -f /tmp/f 2>/dev/null; do sleep 5; done',
     ALLOW, "FP: SECONDS in arithmetic context is a real bound"),
    ('dl=$(($(date +%s)+600)); until grep -q OK /tmp/f 2>/dev/null; do [ $(date +%s) -ge $dl ] && exit 1; sleep 5; done',
     ALLOW, "FP: 'exit' is an escape hatch just like 'break'"),
    ('i=0; until grep -q OK /tmp/f 2>/dev/null; do i=$((i+1)); [ $i -gt 60 ] && exit 1; sleep 5; done',
     ALLOW, "FP: a bounded counter"),
    ('until kubectl get pod x -o json | grep -q Running 2>/dev/null; do sleep 5; done',
     ALLOW, "FP: a pipeline predicate is not a file sentinel"),
    ('while [ "$(ps aux | grep -c foo)" -gt 1 ]; do sleep 5; done',
     ALLOW, "FP: grep -c is the documented self-match compensation"),
    ('while ! [ -f /tmp/scratch/EXIT ] 2>/dev/null; do sleep 10; done', BLOCK,
     "FN: a path containing 'EXIT' must not read as an exit statement"),

    # ── ROUND 5: a dead store is not a bound. Round 4 moved the bound from fragment scope to
    # loop scope; the escape hatch simply moved INSIDE the loop, which is the more natural
    # place to write it. `max_tries=3` is exactly what an agent writes while TRYING to add a
    # bound, and accepting it disarmed the guard on a provably non-terminating loop.
    ('while pgrep -f ZQmk >/dev/null; do max_tries=3; sleep 2; done', BLOCK,
     "FN: an assignment bounds nothing (max_tries=)"),
    ('while pgrep -f ZQmk >/dev/null; do DEADLINE=99; sleep 2; done', BLOCK,
     "FN: an assignment bounds nothing (DEADLINE=)"),
    ('while pgrep -f ZQmk >/dev/null; do max_wait=0; sleep 2; done', BLOCK,
     "FN: an assignment bounds nothing (max_wait=)"),
    # ...but a READ of the same variable genuinely can bound it.
    ('until ! pgrep -f foo; do sleep 5; [ $max_tries -lt 3 ] || exit 1; done', ALLOW,
     "a READ of the counter, plus exit, is a real bound"),
    ('DEADLINE=$((SECONDS+600)); until ! pgrep -f foo; do sleep 5; [ $SECONDS -gt $DEADLINE ] && break; done',
     ALLOW, "assignment PLUS a read and a break is a real bound"),
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
