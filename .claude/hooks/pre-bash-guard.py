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

# A polling loop: `until <cond>; do ... done` or `while <cond>; do ... done`.
LOOP_RE = re.compile(r"\b(until|while)\b", re.I)
SLEEP_IN_LOOP_RE = re.compile(r"\bdo\b[^;]*\bsleep\b|\bsleep\b[^;]*;\s*done", re.I | re.S)

# `pgrep -f PATTERN` / `pgrep -af PATTERN`, capturing the pattern.
#
# Quoted and unquoted forms are matched separately on purpose: a single alternation that tries
# to span both cannot allow spaces inside quotes without also swallowing the rest of an
# unquoted command line. Getting this wrong is not academic — the real loop found in a second
# lane used `pgrep -f "/cargo build -p icn-net"`, whose pattern contains spaces, and an
# earlier single-branch regex let it straight through.
PGREP_RE = re.compile(
    r"\bpgrep\b[^|;&\n]*?-[a-zA-Z]*f[a-zA-Z]*\s+(?:--\s+)?"
    r"(?:"
    r"(?P<q>['\"])(?P<qpat>[^'\"]*)(?P=q)"   # quoted: spaces allowed
    r"|(?P<pat>[^'\"\s|;&)]+)"               # bare: stops at whitespace
    r")"
)

# The bracket trick — `[m]utate.py` — is the documented way to write a pattern that cannot
# match the shell whose command line contains it, because the literal text `[m]utate.py` is
# not matched by the regex `[m]utate.py`. Patterns using it are SAFE and must not be flagged.
BRACKET_TRICK_RE = re.compile(r"\[[^\]]\]")

# A sentinel wait whose failure is swallowed: `grep -q ... FILE 2>/dev/null` inside a loop.
# The 2>/dev/null is the load-bearing part: it is what collapses "cannot read this file" into
# "not ready yet", erasing the difference between a live producer and a dead one.
SENTINEL_RE = re.compile(r"\b(grep|test|\[)\b[^;]*2>\s*/dev/null", re.I)

# Anything that bounds the loop or gives it evidence, so it can end or report failure.
# Presence of ANY of these means the loop is not the unbounded form this guard refuses.
BOUNDED_RE = re.compile(
    r"\btimeout\b|\bSECONDS\b|\bicn-wait\b|\bbreak\b|\bdeadline\b"
    r"|\bmax_?(?:tries|attempts|wait)\b|--source-pid\b",
    re.I,
)

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


def blocked_reason(cmd: str) -> str | None:
    if not LOOP_RE.search(cmd) or not SLEEP_IN_LOOP_RE.search(cmd):
        return None  # not a polling loop; nothing here applies
    if BOUNDED_RE.search(cmd):
        return None  # bounded, or has evidence: it can end and can report failure

    for m in PGREP_RE.finditer(cmd):
        pat = m.group("qpat") if m.group("qpat") is not None else m.group("pat")
        if not pat:
            continue
        if BRACKET_TRICK_RE.search(pat):
            continue  # the safe idiom — explicitly supported
        # The pattern is a literal substring of the very command line that runs pgrep, so
        # `pgrep -f` matches this shell. The predicate can never become false. (Defect A.)
        return (
            f"This wait can never terminate.\n\n"
            f"  pattern: {pat}\n\n"
            f"`pgrep -f` matches full command lines, and THIS shell's command line contains "
            f"that pattern, so the shell matches itself and the loop condition stays true "
            f"forever. No future event from any process can end it. Two loops of exactly this "
            f"shape were found on icn-dev pinning a merged lane's build output for up to "
            f"2.8 days.\n\n"
            f"If you must match by pattern, the bracket idiom `[{pat[:1]}]{pat[1:]}` does not "
            f"match the shell that names it — but an exact PID is better still."
        )

    if SENTINEL_RE.search(cmd):
        return (
            "This wait is unbounded and cannot detect its own failure.\n\n"
            "A sentinel wait is not impossible in principle — another process may legitimately "
            "create or update the file later, and this loop would then finish normally. The "
            "defect is that it cannot tell the difference between:\n"
            "    - the producer is still working      (waiting is correct)\n"
            "    - the producer died                   (waiting is futile)\n"
            "    - the scratch directory was deleted   (waiting is futile)\n"
            "    - the sentinel will never arrive      (waiting is futile)\n\n"
            "Redirecting stderr to /dev/null collapses \"cannot read this file\" into \"not "
            "ready yet\", so three terminal cases look exactly like the healthy one. With no "
            "bound, the loop can spin indefinitely while appearing active and never report why. "
            "A loop of this shape was found on icn-dev waiting on a file whose scratch "
            "directory had been cleaned up.\n\n"
            "Supply a bound and producer evidence and this is fine — that is exactly what the "
            "icn-wait form below does."
        )
    return None


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
