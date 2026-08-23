#!/usr/bin/env python3
"""PreToolUse(Bash) guard.

Two jobs:
  1. Advisory note on direct-main operations (pre-existing behaviour, unchanged).
  2. BLOCK two specific unbounded wait shapes that get agents permanently stuck.

TWO DEFECTS, TWO DIFFERENT PROPERTIES — do not conflate them:

  A. SELF-MATCHING pgrep -f  is LOGICALLY NON-TERMINATING.
     `pgrep -f` matches full command lines, and the waiting shell's own command line contains
     the pattern, so the shell matches itself. The predicate can never become false. No future
     event, from any process, can end this loop. It is impossible, not merely risky.

  B. UNBOUNDED SENTINEL WAIT  is NOT logically impossible — and this guard must not claim it
     is. Another process may legitimately create or update the file later, and such a wait can
     complete perfectly normally.

     Its defect is INDISTINGUISHABILITY. With stderr swallowed and no bound and no producer
     identity, the loop cannot tell apart:
         - the producer is still working          (wait is correct)
         - the producer died                       (wait is futile)
         - the scratch directory was deleted       (wait is futile)
         - the sentinel is never going to arrive   (wait is futile)
     Three of those four are terminal, and the loop treats all four identically. So it can spin
     indefinitely while looking active, and cannot report why. That is a real, observed failure
     mode — one such loop was found here waiting on a file whose scratch directory had been
     cleaned up — but it is a design defect, not a mathematical impossibility.

WHY BLOCKING BOTH, NOT WARNING
    These are not style preferences. Three such loops were recovered from this VM's process
    table pinning a merged lane's 114 GB of build output for up to 2.8 days, each spawning a
    fresh child every few seconds so every naive liveness metric read healthy. A warning
    printed into a transcript does not prevent that; refusing the command does.

    Shape B is refused only in its UNBOUNDED, ERROR-SWALLOWING form. A sentinel wait that has a
    timeout, a break, or producer evidence is allowed, because the fix is to supply those —
    not to stop waiting on files. Anything merely suspicious is left alone: a guard that cries
    wolf gets worked around, and a worked-around guard protects nothing.

Exit 0 = allow. Exit 2 = block (stderr is shown to the agent).
Refs icn#2653.
"""

import json
import re
import sys

# ── detection ─────────────────────────────────────────────────────────────────
#
# The earlier version pattern-matched over the whole command string. That was wrong in both
# directions and an independent review found 11 evasions and 5 misfires. Two structural
# mistakes caused nearly all of them:
#
#   * it looked for `sleep` adjacent to `do`/`done`, so ANY extra statement after the sleep
#     (`do sleep 30; date; done`) disarmed the guard — a one-token rewrite of the real incident
#     loop walked straight through;
#   * it looked for the defect anywhere in the command, so a legitimate loop whose BODY happened
#     to contain `grep ... 2>/dev/null` or the word `test` was blocked.
#
# So: parse the loop, split condition from body, and require the defect in the CONDITION (that
# is what decides termination) and the sleep in the BODY (that is what makes it a poll).

COMMENT_RE = re.compile(r"(?<!\\)#[^\n]*")

# `until|while <condition> ; do <body> done`  — condition and body captured separately.
LOOP_RE = re.compile(
    r"\b(?P<kw>until|while)\b(?P<cond>.*?)(?:;|\n)\s*do\b(?P<body>.*?)\bdone\b",
    re.S | re.I,
)

# `pgrep -f PATTERN` (any flag order), quoted or bare.
PGREP_RE = re.compile(
    r"\bpgrep\b[^|;&\n]*?-[a-zA-Z]*f[a-zA-Z]*\s+(?:--\s+)?"
    r"(?:(?P<q>['\"])(?P<qpat>[^'\"]*)(?P=q)|(?P<pat>[^'\"\s|;&)]+))"
)

# `ps ... | grep PATTERN` — the other canonical self-matching idiom. grep's own argv appears in
# ps output, so it is non-terminating for exactly the same reason as pgrep -f.
PS_GREP_RE = re.compile(
    r"\bps\b[^|;\n]*\|[^|;\n]*\bgrep\b[^|;\n]*?"
    r"(?:(?P<q2>['\"])(?P<qpat2>[^'\"]*)(?P=q2)|(?P<pat2>[^'\"\s|;&)]+))\s*$"
)

# The bracket trick — `[m]utate.py` — cannot match the shell that names it. Always safe.
BRACKET_TRICK_RE = re.compile(r"\[[^\]]\]")

# stderr swallowed: `2>/dev/null` or bash's `&>/dev/null`.
SWALLOW_RE = re.compile(r"(2>\s*/dev/null|&>\s*/dev/null|>&\s*/dev/null)")
# a file/content predicate
FILE_PRED_RE = re.compile(r"\b(grep|test)\b|\[\s")

# Bounding constructs, as real tokens. `timeout` must be a command with a duration, SECONDS a
# shell variable, break a statement — previously the bare WORDS matched, so `--timeout 60`
# inside a pgrep pattern, the English word "seconds", or `break` in a comment all disarmed it.
BOUNDED_RE = re.compile(
    r"(^|[;&|(]\s*)timeout\s+[\d.]+"      # timeout 600 ...
    r"|\$SECONDS\b|\bSECONDS\s*[-=]"       # $SECONDS / SECONDS=
    r"|(^|[;&|\n]\s*)break\b"              # break as a statement
    r"|(^|[;&|(]\s*)icn-wait\b"             # the supported helper
    r"|\bdeadline\b|\bmax_?(?:tries|attempts|wait)\b",
    re.M,
)


def strip_comments(cmd: str) -> str:
    return COMMENT_RE.sub("", cmd)


def _pattern_of(m: re.Match) -> str | None:
    for g in ("qpat", "pat", "qpat2", "pat2"):
        try:
            v = m.group(g)
        except IndexError:
            continue
        if v:
            return v
    return None


def blocked_reason(cmd: str) -> str | None:
    """Return a refusal reason, or None to allow.

    Refuses only the two unbounded shapes documented above, and only when the defect is in the
    loop's own termination condition.
    """
    clean = strip_comments(cmd)

    for loop in LOOP_RE.finditer(clean):
        cond, body = loop.group("cond"), loop.group("body")

        # A polling loop sleeps somewhere in its body. Position within the body is irrelevant.
        if not re.search(r"\bsleep\b", body, re.I):
            continue

        # An escape hatch anywhere in the COMMAND means the loop can end or report failure —
        # including a wrapper outside it, e.g. `timeout 600 bash -c "until ...; done"`. Scoping
        # this to the loop text alone wrongly blocked exactly that form. Safe to widen now that
        # comments are stripped and the bounding tokens are anchored to command positions.
        if BOUNDED_RE.search(clean):
            continue

        # ── Defect A: the condition matches the observer itself ──
        for m in list(PGREP_RE.finditer(cond)) + list(PS_GREP_RE.finditer(cond)):
            pat = _pattern_of(m)
            if not pat or BRACKET_TRICK_RE.search(pat):
                continue
            safe_hint = f"[{pat[:1]}]{pat[1:]}" if pat else "[p]attern"
            return (
                f"This wait can never terminate.\n\n"
                f"  pattern: {pat}\n\n"
                f"`pgrep -f` (and `ps | grep`) matches full command lines, and THIS shell's own "
                f"command line contains that pattern — so the shell matches itself and the loop "
                f"condition stays true forever. No future event from any process can end it. "
                f"Two loops of exactly this shape were found on icn-dev pinning a merged lane's "
                f"build output for up to 2.8 days.\n\n"
                f"If you must match by pattern, the bracket idiom `{safe_hint}` does not match "
                f"the shell that names it — but an exact PID is better still."
            )

        # ── Defect B: unbounded sentinel wait whose failure is swallowed ──
        if SWALLOW_RE.search(cond) and FILE_PRED_RE.search(cond):
            return (
                "This wait is unbounded and cannot detect its own failure.\n\n"
                "A sentinel wait is not impossible in principle — another process may "
                "legitimately create or update the file later, and this loop would then finish "
                "normally. The defect is that it cannot tell the difference between:\n"
                "    - the producer is still working      (waiting is correct)\n"
                "    - the producer died                   (waiting is futile)\n"
                "    - the scratch directory was deleted   (waiting is futile)\n"
                "    - the sentinel will never arrive      (waiting is futile)\n\n"
                "Discarding stderr collapses \"cannot read this file\" into \"not ready yet\", so "
                "three terminal cases look exactly like the healthy one. With no bound, the loop "
                "can spin indefinitely while appearing active and never report why. A loop of "
                "this shape was found on icn-dev waiting on a file whose scratch directory had "
                "been cleaned up.\n\n"
                "Supply a bound and producer evidence and this is fine — that is exactly what "
                "the icn-wait form below does."
            )
    return None


ADVICE = """
Use ops/scripts/icn-wait instead — every form is bounded, and the file form takes producer
evidence so it can fail fast instead of spinning:

  icn-wait cmd  --timeout 3600 -- <command>     # you launched it: waits on its own child
  icn-wait pid  <PID> --timeout 600             # exact, no pattern
  icn-wait file <PATH> --pattern '^EXIT=' --timeout 600 --source-pid <PID>
  icn-wait match '<pattern>' --timeout 600      # last resort; excludes the observer

For a long build, add --supervise so the lane is not judged abandoned while it runs:

  icn-wait cmd --supervise --timeout 3600 -- cargo test --workspace

It resolves your session from the worktree; no session id needed.
"""


def main() -> int:
    try:
        data = json.load(sys.stdin)
    except Exception:
        return 0

    cmd = data.get("tool_input", {}).get("command", "") or ""

    reason = blocked_reason(cmd)
    if reason:
        print(f"[icn-dev GUARD] {reason}\n{ADVICE}", file=sys.stderr)
        return 2

    if "git checkout main" in cmd or "git push origin main" in cmd:
        print("[icn-dev GUARD] Direct main branch op -- ICN uses feature branches. Confirm intentional.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
