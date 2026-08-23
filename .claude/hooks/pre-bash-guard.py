#!/usr/bin/env python3
"""PreToolUse(Bash) guard.

Two jobs:
  1. Advisory note on direct-main operations (pre-existing behaviour, unchanged).
  2. BLOCK the shell wait loops that provably cannot terminate.

WHY BLOCKING, NOT WARNING
    These are not style preferences. Three of them were recovered from this VM's process table
    pinning a merged lane's 114 GB of build output for up to 2.8 days, each spawning a fresh
    child every few seconds so every naive liveness metric read healthy. A warning printed into
    a transcript does not prevent that; refusing the command does.

    Both shapes are refused only when they are UNCONDITIONALLY broken — a loop that no future
    event can end. Anything merely suspicious is left alone, because a guard that cries wolf
    gets worked around, and a worked-around guard protects nothing.

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
SENTINEL_RE = re.compile(r"\b(grep|test|\[)\b[^;]*2>\s*/dev/null", re.I)

# Anything that gives the loop a way out other than the predicate.
BOUNDED_RE = re.compile(
    r"\btimeout\b|\bSECONDS\b|\bicn-wait\b|\bbreak\b|\bdeadline\b|\bmax_?(?:tries|attempts|wait)\b",
    re.I,
)

ADVICE = """
Use ops/scripts/icn-wait instead — it is bounded and cannot wait on itself:

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
        return None  # has an escape hatch; not unconditionally broken

    for m in PGREP_RE.finditer(cmd):
        pat = m.group("qpat") if m.group("qpat") is not None else m.group("pat")
        if not pat:
            continue
        if BRACKET_TRICK_RE.search(pat):
            continue  # the safe idiom — explicitly supported
        # The pattern is a literal substring of the very command line that runs pgrep, so
        # `pgrep -f` matches this shell. The predicate can never become false.
        return (
            f"This wait can never terminate.\n\n"
            f"  pattern: {pat}\n\n"
            f"`pgrep -f` matches full command lines, and THIS shell's command line contains "
            f"that pattern, so the shell matches itself and the loop condition stays true "
            f"forever. Three loops of exactly this shape were found on icn-dev pinning a "
            f"merged lane's 114 GB of build output for up to 2.8 days."
        )

    if SENTINEL_RE.search(cmd):
        return (
            "This wait can never terminate if the sentinel is missing.\n\n"
            "Redirecting stderr to /dev/null turns \"this file does not exist\" into "
            "\"not ready yet\", so a sentinel whose producer died — or whose scratchpad was "
            "cleaned up — is indistinguishable from one still coming. A loop of exactly this "
            "shape was found on icn-dev waiting on a file that no longer existed."
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
