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
from re import error

# ── detection ─────────────────────────────────────────────────────────────────
#
# Structure, learned from two rounds of adversarial review:
#
#   * Quoted spans are computed on the RAW text, BEFORE comment stripping. Doing it the other
#     way let a `#` inside a string eat the rest of the line — including the loop's `done`.
#   * A quoted span is not automatically inert. `bash -c "<loop>"`, `sh -c`, `eval` and heredoc
#     bodies are EXECUTED, so they are recursively re-parsed as fragments. Treating them as
#     text allowed 19/19 non-terminating waits through, `bash -c "..."` being a one-token
#     rewrite of the real incident loop.
#   * Bounding tokens only count OUTSIDE quotes, so `echo "x; break"` cannot disarm the guard.

MAX_FRAGMENT_DEPTH = 4

# Bash starts a comment only where `#` BEGINS A WORD. Treating any `#` as a comment ate the
# rest of the line for an ordinary URL fragment (`curl http://h/p#f`), taking the loop's `done`
# with it and disarming the guard.
COMMENT_RE = re.compile(r"(?:(?<=\s)|^)#[^\n]*")

LOOP_RE = re.compile(
    r"\b(?P<kw>until|while)\b(?P<cond>.*?)(?:;|\n)\s*do\b(?P<body>.*?)\bdone\b",
    re.S | re.I,
)

PGREP_RE = re.compile(
    r"\bpgrep\b[^|;&\n]*?-[a-zA-Z]*f[a-zA-Z]*\s+(?:--\s+)?"
    r"(?:(?P<q>['\"])(?P<qpat>[^'\"]*)(?P=q)|(?P<pat>[^'\"\s|;&)]+))"
)

# `ps … | grep PATTERN` — the same self-matching defect. `grep -c` is excluded: counting is the
# documented way to compensate for the self-match, and flagging it produced a refusal that
# named `]` as the pattern.
PS_GREP_RE = re.compile(
    r"\bps\b[^|;\n]*\|(?P<grep>[^|;\n]*?\bgrep\b[^|;\n]*?)\s+"
    # Skip leading flags so the PATTERN is captured, not `-c`. Without this the count form was
    # read as pattern="-c" and the exclusion below never fired.
    r"(?:-\S+\s+)*"
    r"(?:(?P<q2>['\"])(?P<qpat2>[^'\"]*)(?P=q2)|(?P<pat2>[A-Za-z0-9_./][A-Za-z0-9_./-]+))"
)

BRACKET_TRICK_RE = re.compile(r"\[[^\]]\]")
SWALLOW_RE = re.compile(r"(2>\s*/dev/null|&>\s*/dev/null|>&\s*/dev/null)")
# A FILE sentinel: an explicit file test, or a grep whose target looks like a path. Requiring
# this stops a pipeline predicate (`kubectl get … | grep -q Running 2>/dev/null`) — which is
# not a sentinel wait at all — from being refused.
FILE_TEST_RE = re.compile(r"(\btest\b|\[)\s+-[efsrd]\b")
GREP_FILE_RE = re.compile(r"\bgrep\b[^|;\n]*\s(?P<target>[~./][^\s|;&]*|\$\{?\w+\}?)\s*(?:2>|&>|>&|$)")

BOUNDED_RE = re.compile(
    r"(^|[;&|(]\s*)timeout\s+[\d.]+"
    r"|\$SECONDS\b|\bSECONDS\s*[-=<>]|\(\(\s*SECONDS\b"
    r"|(^|[;&|\n]\s*)break\b"
    # `exit` must be at a command position. As a bare word under re.I it matched the path
    # /tmp/scratch/EXIT and silently disarmed the guard on the very sentinel shape it refuses.
    r"|(^|[;&|\n]\s*)exit\b"
    r"|(^|[;&|(]\s*)icn-wait\b"
    r"|\$(?:deadline|DEADLINE|max_?(?:tries|attempts|wait))\b"
    r"|\b(?:deadline|DEADLINE|max_?(?:tries|attempts|wait))\s*=",
    re.M | re.I,
)

# Wrappers whose quoted argument is executed, not quoted text.
EXEC_WRAPPER_RE = re.compile(
    r"\b(?:ba|z|k|da)?sh\b[^|;&\n]*?-c\s+(?P<q>['\"])(?P<code>.*?)(?<!\\)(?P=q)"
    r"|\beval\s+(?P<q2>['\"])(?P<code2>.*?)(?<!\\)(?P=q2)",
    re.S,
)
# Only a heredoc fed to a SHELL is executed. `cat >> README.md <<EOF` is documentation, and
# recursing into it refused an agent's attempt to document this very defect.
HEREDOC_RE = re.compile(
    r"\b(?:ba|z|k|da)?sh\b[^\n]*?<<-?\s*['\"]?(?P<tag>\w+)['\"]?\n(?P<code>.*?)\n\s*(?P=tag)\b",
    re.S,
)


ANY_HEREDOC_RE = re.compile(r"<<-?\s*['\"]?(?P<tag>\w+)['\"]?\n(?P<code>.*?)\n\s*(?P=tag)\b", re.S)


def quoted_spans(text: str) -> list[tuple[int, int]]:
    """Character ranges the shell will NOT execute as commands of this fragment.

    Covers single/double quotes and EVERY heredoc body. A heredoc fed to a shell is executed —
    but that is handled by recursing into it as its own fragment, not by parsing it here; a
    heredoc fed to `cat` is documentation, and parsing it in place refused an agent's attempt to
    write about this very defect.
    """
    spans: list[tuple[int, int]] = [(m.start("code"), m.end("code")) for m in ANY_HEREDOC_RE.finditer(text)]
    i, n = 0, len(text)
    while i < n:
        ch = text[i]
        if inside_span(i, spans):
            i += 1
            continue
        if ch in "\"'":
            j = i + 1
            while j < n and text[j] != ch:
                if text[j] == "\\":
                    j += 1
                j += 1
            spans.append((i, min(j, n)))
            i = j + 1
            continue
        i += 1
    return spans


def inside_span(pos: int, spans: list[tuple[int, int]]) -> bool:
    return any(a <= pos <= b for a, b in spans)


def strip_comments_outside_quotes(text: str) -> str:
    spans = quoted_spans(text)
    out, last = [], 0
    for m in COMMENT_RE.finditer(text):
        if inside_span(m.start(), spans):
            continue
        out.append(text[last:m.start()])
        last = m.end()
    out.append(text[last:])
    return "".join(out)


def fragments(cmd: str, depth: int = 0, ancestor_bounded: bool = False) -> list[tuple[str, bool]]:
    """(fragment, bounded_by_ancestor) for the command and every body the shell executes.

    The ancestor flag matters: in `timeout 600 bash -c "<loop>"` the bound lives in the PARENT,
    so a nested fragment judged in isolation would be refused despite being perfectly bounded.
    """
    mine = ancestor_bounded or _bounded(cmd)
    found = [(cmd, ancestor_bounded)]
    if depth >= MAX_FRAGMENT_DEPTH:
        return found
    for m in EXEC_WRAPPER_RE.finditer(cmd):
        code = m.group("code") or m.group("code2")
        if code:
            found.extend(fragments(code, depth + 1, mine))
    for m in HEREDOC_RE.finditer(cmd):
        found.extend(fragments(m.group("code"), depth + 1, mine))
    return found


def _pattern_of(m: re.Match) -> str | None:
    for g in ("qpat", "pat", "qpat2", "pat2"):
        try:
            v = m.group(g)
        except (IndexError, error):
            continue
        if v:
            return v
    return None


def _bounded(fragment: str) -> bool:
    """A bounding construct that is really executed — not one sitting inside a string."""
    spans = quoted_spans(fragment)
    return any(not inside_span(m.start(), spans) for m in BOUNDED_RE.finditer(fragment))


def blocked_reason(cmd: str) -> str | None:
    """Return a refusal reason, or None to allow."""
    for fragment, ancestor_bounded in fragments(cmd):
        if ancestor_bounded:
            continue
        clean = strip_comments_outside_quotes(fragment)
        spans = quoted_spans(clean)

        for loop in LOOP_RE.finditer(clean):
            # A loop keyword inside quotes is text UNLESS this fragment is itself an executed
            # body — and in that case we are already recursing into it as its own fragment.
            if inside_span(loop.start("kw"), spans):
                continue
            cond, body = loop.group("cond"), loop.group("body")
            if not re.search(r"\bsleep\b", body, re.I):
                continue
            if _bounded(loop.group(0)) or _bounded(clean):
                continue

            # ── Defect A: the condition matches the observer itself ──
            for m in list(PGREP_RE.finditer(cond)) + list(PS_GREP_RE.finditer(cond)):
                # Check the WHOLE match: the grep group is non-greedy, so `-c` can fall outside
                # it and the exclusion silently never fired.
                if re.search(r"\bgrep\b[^|;\n]*?(-\w*c\b|--count)", m.group(0)):
                    continue  # `grep -c` counts: the documented self-match compensation
                pat = _pattern_of(m)
                if not pat or BRACKET_TRICK_RE.search(pat):
                    continue
                if not re.search(r"[A-Za-z0-9]", pat):
                    continue  # punctuation is not a process pattern
                safe_hint = f"[{pat[:1]}]{pat[1:]}"
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

            # ── Defect B: unbounded FILE sentinel whose failure is swallowed ──
            if (
                SWALLOW_RE.search(cond)
                and "|" not in cond
                and (FILE_TEST_RE.search(cond) or GREP_FILE_RE.search(cond))
            ):
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

    # Defensive: the payload shape is the harness's, not ours. Previously a non-dict
    # tool_input or a non-string command raised AttributeError/TypeError and the hook exited 1
    # with a traceback — a guard that CRASHES fails open, which is the worst of both worlds.
    tool_input = data.get("tool_input") if isinstance(data, dict) else None
    cmd = tool_input.get("command") if isinstance(tool_input, dict) else None
    if not isinstance(cmd, str) or not cmd:
        return 0

    try:
        reason = blocked_reason(cmd)
    except Exception as exc:  # noqa: BLE001 - never fail a session on a guard bug
        print(f"[icn-dev GUARD] internal error, command allowed: {exc}", file=sys.stderr)
        return 0
    if reason:
        print(f"[icn-dev GUARD] {reason}\n{ADVICE}", file=sys.stderr)
        return 2

    if "git checkout main" in cmd or "git push origin main" in cmd:
        print("[icn-dev GUARD] Direct main branch op -- ICN uses feature branches. Confirm intentional.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
