#!/usr/bin/env python3
"""check-agent-runtime-adoption.py — the lifecycle contract must not silently disappear.

A runtime that only one manually-launched session uses is not integrated. The lifecycle hooks
live in .claude/settings.json, which is edited by hand and by tooling, so nothing stops a
future change from dropping an event and quietly turning lifecycle tracking off for every
session. This checker makes that a build failure instead of a mystery.

It also reports launcher COVERAGE honestly, including the launchers that cannot support the
contract today. An unsupported launcher listed explicitly is a known gap; an unsupported
launcher nobody wrote down is a false claim of integration.

Usage:
    python3 scripts/check-agent-runtime-adoption.py [--repo-root PATH] [--verbose]

Exit 0 = the contract is wired. Exit 1 = it is not. Refs icn#2653.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import subprocess
import tempfile
import sys
from pathlib import Path

HOOK = ".claude/hooks/session-lifecycle.sh"

# Every event the lifecycle depends on, and what breaks if it goes missing. The message is the
# point: a bare "missing hook" tells a future maintainer nothing about the consequence.
REQUIRED_EVENTS = {
    "SessionStart": "sessions would never register; the whole registry goes dark",
    "PostToolUse": "progress would never advance; every live lane would look stalled",
    "Stop": "interaction/liveness would never be reported between tool calls",
    "SessionEnd": "sessions would never release; every lane would leak until TTL expiry",
}

# Matchers that must be covered, not merely present. Keyed by event.
REQUIRED_MATCHER_TOKENS = {
    "SessionStart": ["startup", "resume"],
    "PostToolUse": ["Bash"],
}


# Stands in for a `$` the shell would NOT expand. Printable on purpose: a NUL byte makes
# Path.resolve() raise ValueError rather than simply failing to match.
LITERAL_DOLLAR = "__ICN_LITERAL_DOLLAR__"


_PLAIN, _SINGLE, _DOUBLE, _ESCAPED = "plain", "single", "double", "escaped"


def _shell_states(command: str) -> list:
    """How a shell reads each character of `command`: plain, single-quoted, double-quoted or
    backslash-escaped.

    ONE quoting model, three consumers -- `$` masking, comment stripping and separator
    detection. There used to be two, and they disagreed: masking tracked quotes properly while
    comment stripping was an unconditional `split("#", 1)`. A parser and a splitter holding
    different opinions about quoting is the defect this file has now hit three times.
    """
    states = []
    i, n = 0, len(command)
    in_single = in_double = False
    while i < n:
        c = command[i]
        if in_single:
            # Inside '...' NOTHING is special except the closing quote -- not even backslash.
            states.append(_SINGLE)
            if c == "'":
                in_single = False
        elif c == "\\" and i + 1 < n:
            # A backslash escapes in the unquoted and double-quoted contexts alike.
            states.append(_DOUBLE if in_double else _PLAIN)
            states.append(_ESCAPED)
            i += 2
            continue
        elif in_double:
            states.append(_DOUBLE)   # `$` IS expanded inside double quotes.
            if c == '"':
                in_double = False
        else:
            states.append(_PLAIN)
            if c == "'":
                in_single = True
            elif c == '"':
                in_double = True
        i += 1
    return states


def _mask_unexpanded_dollars(command: str) -> str:
    """
    Neutralise every `$` a shell would NOT expand.

    A shell expands `$VAR` when it is bare or inside DOUBLE quotes, and leaves it literal inside
    SINGLE quotes or after a backslash. `shlex.split` strips quoting before we ever see it, so
    `'$CLAUDE_PROJECT_DIR'/.claude/hooks/session-lifecycle.sh` — which a shell runs as a literal
    path, failing with exit 127 and no lifecycle tracking at all — arrived here looking exactly
    like the expanded form and resolved to the real hook file.

    THIS TRACKS BOTH QUOTE STATES, and the first version did not. Tracking only single quotes
    meant an ordinary apostrophe inside a double-quoted word — `NOTE="agent's lane"` — toggled
    the tracker, desynchronising it by one, so a genuinely single-quoted `$CLAUDE_PROJECT_DIR`
    later on the same line was seen as unquoted and left unmasked. Measured: the gate reported
    "25 check(s) passed, 0 failure(s)" while `bash -c` on the identical string exited 127.
    A quoting model that is wrong about quoting is worse than no model, because it looks like one.
    """
    return "".join(
        LITERAL_DOLLAR if c == "$" and st in (_SINGLE, _ESCAPED) else c
        for c, st in zip(command, _shell_states(command)))


def _strip_shell_comment(command: str) -> str:
    """`command` with every unquoted comment removed.

    A comment runs from an unquoted `#` that BEGINS A WORD to the end of ITS LINE. Both halves
    of that matter, and the previous `command.split("#", 1)[0]` had neither:

      * it cut inside quotes, so `echo "ticket #123"` was truncated mid-string and the leftover
        unmatched quote came back as "unparseable command" -- the required workflow red on a
        command the shell runs perfectly well; and
      * it cut to the end of the STRING, so everything after a comment vanished, including a
        second command on a later line.
    """
    states = _shell_states(command)
    out: list[str] = []
    i, n = 0, len(command)
    while i < n:
        if (command[i] == "#" and states[i] == _PLAIN
                and (i == 0 or command[i - 1] in " \t\n")):
            nl = command.find("\n", i)
            if nl < 0:
                break
            i = nl
            continue
        out.append(command[i])
        i += 1
    return "".join(out)


# Characters that make a command more than one simple invocation. Split by WHERE a shell
# still acts on them: `$` and a backtick expand inside double quotes as well as bare, while
# the rest are only operators when unquoted.
_EXPANDS_IN_DOUBLE_QUOTES = set("$`")


def _unquoted_operators(command: str, chars=None, also_in_double=frozenset()) -> set:
    """Shell operator characters the command carries UNQUOTED.

    Read from the character states, not from the token list. shlex strips quote provenance in
    posix mode, so a data argument made entirely of punctuation -- `echo ";"`, `echo '&&'`,
    `echo \\;` -- came back as a token indistinguishable from a real operator and the gate
    went red on a command bash runs normally. Another false rejection from asking a reader a
    question it had already thrown away the answer to.
    """
    chars = _SHELL_OPERATOR_CHARS if chars is None else chars
    return {c for c, st in zip(command, _shell_states(command))
            if c in chars and (st == _PLAIN
                               or (st == _DOUBLE and c in also_in_double))}


def _substitution_present(command: str):
    r"""`"$(...)"` or `` "`...`" `` if the command contains one at all, else None.

    CATEGORICAL. This used to refuse only substitutions NAMING a repository path, which left
    the ones that FIND one: `echo "$(find . -name hook-health.sh -exec {} \;)"` executes a
    repository hook without containing a single slash-bearing token, so the target left the
    derived set and its executable bit went unchecked (icn#2691 review).

    The alternative was a blocklist -- `-exec`, `-execdir`, `xargs`, `sh -c`, `eval`, `git`
    aliases -- and the sibling PR spent nine rounds establishing where blocklists in this
    codebase end up. A substitution is another command; the supported language is a SINGLE
    SIMPLE command; so substitutions are outside it, and that is provable rather than
    argued.

    Live `.claude/settings.json` had exactly one, and it moved into
    `.claude/hooks/report-branch.sh` where the shell has an owner and the gate can prove what
    it needs about argv0. Maintainer-authorised, icn#2691.
    """
    states = _shell_states(command)
    for i, c in enumerate(command):
        st = states[i] if i < len(states) else _PLAIN
        if st in (_PLAIN, _DOUBLE):
            if command.startswith("$(", i):
                return "$(...)"
            if c == "`":
                return "backtick"
    return None


def _has_unquoted_newline(command: str) -> bool:
    """Does a bash COMMAND SEPARATOR hide in here as a literal newline?

    `shlex` reads a newline as ordinary whitespace, so `echo hi\n<hook>` tokenised to
    `["echo", "hi", "<hook>"]`, argv0 was `echo`, and the entry classified NON_HOOK -- while
    bash runs both lines and returns 126 from the second if the hook is not executable. The
    hook then left the derived set, taking its executable check and its share of the expected
    count with it. `;`, `&&` and `|` were already caught as tokens; a newline is the spelling
    that never reaches the lexer.
    """
    return any(c == "\n" and st == _PLAIN
               for c, st in zip(command, _shell_states(command)))


_PROJECT_DIR_TOKEN = re.compile(r"\$(?:\{CLAUDE_PROJECT_DIR\}|CLAUDE_PROJECT_DIR(?![A-Za-z0-9_]))")
_PROJECT_DIR_TOKEN_OPT_SLASH = re.compile(_PROJECT_DIR_TOKEN.pattern + "/?")


def _sub_project_dir(text: str, replacement: str = "") -> str:
    """Replace the project-dir variable AT A TOKEN BOUNDARY, and only there.

    A plain `.replace("$CLAUDE_PROJECT_DIR", ...)` also rewrites the prefix of a LONGER name:
    `$CLAUDE_PROJECT_DIRoops/../.claude/hooks/hook-health.sh` became the real hook, so the
    executable check passed for a command bash resolves to something else entirely and exits
    127. A shell expands the longest valid name, not the prefix we hoped for.

    Five call sites asked this same question in four slightly different ways. They ask it here
    now, because a reader corrected in one place and not the others is how this file has been
    wrong four times.
    """
    # ONCE. Replacing EVERY occurrence collapsed
    # `$CLAUDE_PROJECT_DIR/$CLAUDE_PROJECT_DIR/.claude/hooks/hook-health.sh` to the real hook,
    # while bash expands both and attempts a doubled absolute path, exiting 127. A second
    # occurrence now keeps its `$`, and the unresolved-expansion rule refuses the command --
    # the right answer, reached by an existing rule rather than a new one.
    if replacement:
        return _PROJECT_DIR_TOKEN.sub(lambda _: replacement, text, count=1)
    # Removing the variable also removes the slash that FOLLOWED it, so `$VAR/x` becomes `x`
    # rather than `/x`. Only that slash: my first version stripped any leading `/`, which
    # turned the token `/dev/null` -- an absolute path outside the repo, and the live
    # `2>/dev/null` redirection -- into the repo-relative `dev/null`, and the gate went red on
    # its own settings.json. The live run caught it, which is the point of running it.
    # ONE pass, not two. Two subs each capped at count=1 still remove TWO occurrences, which
    # is the defect this cap exists to close -- the second token has to survive so the
    # unresolved-expansion rule can refuse it by name rather than by an incidental
    # containment failure. The optional trailing slash goes with the token it follows.
    return _PROJECT_DIR_TOKEN_OPT_SLASH.sub("", text, count=1)


def _leading_assignment_words(command: str) -> int:
    """How many leading words are REAL `VAR=value` assignment prefixes.

    Decided before quoting is discarded. shlex strips quote provenance, so `FOO\\=bar <hook>`,
    `'FOO=bar' <hook>` and `"FOO=bar" <hook>` all arrived looking like assignments -- and bash
    treats every one of them as the COMMAND NAME, exiting 127 without ever running the hook,
    while the classifier skipped the word and certified the hook as a direct target.

    A word is an assignment only when its `NAME=` prefix is entirely unquoted.
    """
    states = _shell_states(command)
    count, i, n = 0, 0, len(command)
    while i < n:
        while i < n and command[i] in " \t" and states[i] == _PLAIN:
            i += 1
        start = i
        while i < n and not (command[i] in " \t" and states[i] == _PLAIN):
            i += 1
        word, word_states = command[start:i], states[start:i]
        eq = word.find("=")
        if eq <= 0:
            return count
        if not re.match(r"^[A-Za-z_][A-Za-z0-9_]*=$", word[:eq + 1]):
            return count
        if any(st != _PLAIN for st in word_states[:eq + 1]):
            return count                    # the `=` (or the name) was quoted or escaped
        count += 1
    return count


def _invokes_hook(command: str, root: Path) -> bool:
    """
    The hook must be THE COMMAND — the program actually executed — not merely the last word
    on the line.

    This used to be `stripped.endswith(HOOK)`, which any string ENDING in the path satisfies.
    Rewriting all five hooks as `true "$CLAUDE_PROJECT_DIR"/.claude/hooks/session-lifecycle.sh`
    — lifecycle tracking completely off, nothing registered, nothing released — left this gate
    reporting "25 check(s) passed, 0 failure(s)". A gate whose entire purpose is to prove the
    runtime is wired must not accept a command that provably never runs it.
    """
    # Mask literal `$` BEFORE shlex removes the quoting that decides whether it expands.
    stripped = _mask_unexpanded_dollars(_strip_shell_comment(command).strip())

    # NO SHELL OPERATORS. Everything after argv0 used to be ignored, so appending ` </dev/null`
    # to each hook left the gate at 25/0 while the hook received no payload at all and answered
    # "DEGRADED — hook payload unparseable" on every single event: no register, no progress, no
    # release. A redirection, a pipe or a second command can change what runs or what it reads,
    # and none of them belong in a hook invocation.
    #
    # QUOTE-AWARE, for the reason the classifier already is. A plain `re.search` treated a
    # data argument made of punctuation -- `<hook> ";"` -- as a redirection and returned
    # False, so the event was reported as not invoking the hook at all while the classifier
    # called the very same command a direct invocation. Two siblings reading one command
    # string must not disagree about what an operator is; that disagreement is how one of
    # them ends up wrong, and here it would have been a false rejection of a valid config.
    #
    # The two functions still differ DELIBERATELY on `$` and backticks inside double quotes.
    # The classifier asks what program argv0 is, and `<hook> "$HOME"` runs the hook; this
    # asks whether the invocation is unadorned, and an expansion is something else deciding
    # what the hook receives. That asymmetry is fail-CLOSED -- the gate goes red on a config
    # that would work -- and it is pinned by tests rather than left to be rediscovered.
    if _unquoted_operators(_sub_project_dir(stripped),
                           set("<>;&|`$(){}"), _EXPANDS_IN_DOUBLE_QUOTES):
        return False

    try:
        tokens = shlex.split(stripped)
    except ValueError:
        return False
    # Leading VAR=value environment assignments are legitimate and are not the program.
    # Same question, same answer: an escaped or quoted `=` makes the word a command name.
    idx = _leading_assignment_words(stripped)
    if idx >= len(tokens):
        return False
    argv0 = tokens[idx]
    # An explicit interpreter is legitimate (`bash <hook>`), but then the hook must be its
    # FIRST argument — not something appended after an unrelated program.
    if os.path.basename(argv0) in {"bash", "sh", "zsh"} and idx + 1 < len(tokens):
        argv0 = tokens[idx + 1]

    # IT MUST BE THE HOOK FILE, not a string that merely ends like it. Pointing every command
    # at `.claude/hooks/DISABLED/session-lifecycle.sh` — a path that does not exist, so the hook
    # exits 127 and lifecycle tracking is entirely off — satisfied `endswith()` and left the
    # gate reporting 25 checks passed. Resolve the path and compare it to the file the gate
    # separately execs.
    resolved = _sub_project_dir(argv0, str(root))
    candidate = Path(resolved)
    if not candidate.is_absolute():
        candidate = root / resolved
    try:
        return candidate.resolve() == (root / HOOK).resolve() and candidate.is_file()
    except (OSError, ValueError):
        # An unresolvable path is not the hook. Fail CLOSED.
        return False


# Executables this gate requires regardless of how settings.json is wired. Hook targets are
# NOT listed here -- they are DERIVED from .claude/settings.json below, because a hardcoded
# list is a second copy of the wiring and it rotted: hook-health.sh was invoked directly at
# settings.json while being committed 100644, so it exited 126 at every session start and this
# gate never looked at it. Deriving means a newly added direct hook is covered on the day it
# is added (Refs icn#2691).
REQUIRED_EXECUTABLES = [
    "ops/scripts/icn-agent-session",
    "ops/scripts/icn-wait",
]


def direct_hook_targets(settings: dict, root: Path):
    """Repo paths that settings.json runs AS the command, so the kernel must exec them.

    SYNTACTIC, not existence-filtered. Filtering on is_file() removed a configured hook from
    this list the moment its file was deleted, so both the executable loop and the derived
    check count shrank by one and the gate reported no `missing:` failure while settings.json
    went on invoking a command that was not there. Identification is by shape -- a relative
    path with a directory component, which `echo` and other bare builtins do not have -- and
    the existing missing/executable branches then fail closed on it.

    A file run through an interpreter (`python3 .../pre-tool-guard.py`) is not argv0 and does
    not need the bit, which is why the three .py hooks are correctly 100644.
    """
    found: list[str] = []
    interpreted: list[str] = []
    unclassified: list[tuple[str, str]] = []

    def walk(node) -> None:
        if isinstance(node, dict):
            if node.get("type") == "command" and isinstance(node.get("command"), str):
                # A command this function cannot parse yields no target. It must NOT stop the
                # traversal: one malformed entry would otherwise skip whatever sits below it,
                # and the "derived from settings.json" guarantee is only as good as the walk.
                kind, target, detail = classify_hook_command(node["command"], root)
                if kind == HookCommandKind.UNCLASSIFIED:
                    unclassified.append((node["command"], detail))
                elif kind == HookCommandKind.DIRECT and target not in found:
                    found.append(target)
                elif (kind == HookCommandKind.INTERPRETED and target
                      and target not in interpreted):
                    interpreted.append(target)
            for v in node.values():
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)

    walk(settings.get("hooks", {}))
    return found, interpreted, unclassified


# Shell launchers that exec the command they are handed and return its status. `help command`
# and `env(1)` both confirm the target is attempted and its exit status propagates, so a hook
# behind one of these is still a direct invocation and its executable bit still matters.
# The supported hook-command language, derived from live .claude/settings.json rather than
# from what a shell can express. That file holds 18 command hooks in exactly three shapes:
# 13 direct repository executables, 3 python3-invoked scripts, 2 echo. Zero launchers, no
# `true`, no `:`, and no top-level shell composition.
#
# Launcher support (command/env/exec/nohup) was REMOVED. Nothing used it, and one flag set
# shared across four programs mis-classified: `command -v <hook>` only PRINTS a description
# and `env -0 <hook>` refuses to run a command at all, yet both were certified as direct
# execution -- the gate claiming executable-bit coverage for commands that never invoke the
# target. Sharing a grammar across four languages was the defect; no grammar is smaller than
# a wrong one. A future `env … hook.sh` makes this gate red as an unclassified command form
# until support is added deliberately, which is fail-closed evolution rather than regression.
_INTENTIONAL_NON_HOOK = ("echo",)
_INTERPRETERS = ("python", "python3")

# Operators that compose several commands into one hook entry. Detected as TOKENS through
# shlex punctuation_chars so quoting is respected -- the live
# `echo "branch: $(git … || echo detached)"` carries `||` inside a command substitution and is
# correctly NOT composition, which a substring search would have got wrong.
_SHELL_OPERATOR_CHARS = set(";&|<>()")


# How many checks a COMPLETE run performs. Asserted, not decorative — see the floor in main().
#
# Split into a fixed part and a derived part (icn#2691). The fixed part stays EXACT for the
# reason the original comment gives: a check that skipped itself is not a check that passed.
# The hook-executable checks are derived from settings.json, so their count legitimately
# changes when a hook is added or removed — pinning that to a literal would make adding a hook
# a spurious failure, and the temptation would be to lower the number rather than look.
EXPECTED_STATIC_CHECKS = 24

PROVIDER_MCP_CONFIGS = [
    (".mcp.json", ["mcpServers", "icn-ops"]),
    (".cursor/mcp.json", ["mcpServers", "icn-ops"]),
]


COVERAGE = {
    "supported": {
        "claude-code (icn-start)": "project .claude/settings.json hooks; auto-registers",
        "claude-code (icn-claude, ssh)": "same hooks; lands in the mcp-host worktree",
        "claude-code (remote/ccd-cli)": "same hooks (--setting-sources includes project)",
        "any Claude Code session opened in the repo": "project settings are inherited",
    },
    "partial": {
        ".cursor adapter":
            "declares the ops MCP in .cursor/mcp.json, so icn_ops_agent_runtime and the "
            "session tools work — but it does not execute Claude Code hooks, so it must call "
            "register_session explicitly",
        ".codex / .opencode adapters":
            "do NOT declare the ops MCP at all (.codex/mcp/servers.example.json is an example "
            "that omits it and points at the retired ~/projects/icn path; .opencode/opencode.json "
            "has no MCP block). They get the capability manifest as a file and nothing else.",
    },
    "unsupported": {
        "Claude Code subagents (Agent tool)":
            "run in-process: no SessionStart event, no separate pid, no MCP client of their "
            "own, so they cannot be auto-registered. Provider limitation, not an omission.",
        "CI agents (.github/workflows)":
            "ephemeral, no worktree, no persistent registry — deliberately out of scope.",
    },
}


class HookCommandKind:
    DIRECT = "direct repo executable"
    INTERPRETED = "interpreter-invoked repo script"
    NON_HOOK = "intentional non-hook command"
    UNCLASSIFIED = "unclassified"


def _tokenize(cmd):
    """Tokens with shell operators separated and quoting respected, or None if unlexable."""
    try:
        lexer = shlex.shlex(cmd, posix=True, punctuation_chars=True)
        lexer.whitespace_split = True
        # THE THIRD READER. `_strip_shell_comment` already removed comments quote-awarely, and
        # shlex's own commenter then removed them AGAIN by a different rule -- one that fires
        # mid-word. Bash treats `#` inside a word as part of it, so
        # `.claude/hooks/hook-health.sh#missing` was truncated to the real hook, whose bit was
        # checked, while Claude runs the suffixed path and exits 127. Comments are stripped in
        # exactly one place.
        lexer.commenters = ""
        return list(lexer)
    except ValueError:
        return None


def classify_hook_command(command: str, root: Path | None = None):
    """Classify one settings.json hook command: (kind, target_or_None, detail).

    UNCLASSIFIED is a gate FAILURE, never silence. An unreadable form used to yield no
    target, which dropped it from the derived set -- and the expected check count derives
    from that same list, so the loss concealed itself and the gate stayed green.
    """
    if _has_unquoted_newline(command):
        # Checked on the RAW command: a newline inside a comment still ends that comment and
        # starts a new one, and the lexer below never sees a newline at all.
        return (HookCommandKind.UNCLASSIFIED, None,
                "contains a literal newline, which bash reads as a command separator; only a "
                "single simple command is supported")
    # THE COMMENT-STRIPPED SPELLING, with quoting still intact, is what every reader below
    # uses. Scanning the raw command for operators flagged text inside an explanatory comment
    # -- `<hook> # use && fallback` was reported as composition and the gate went red on a
    # command bash runs normally. Stripping only removes comment ranges, so quote provenance
    # survives it.
    no_comment = _strip_shell_comment(command)
    cmd = _mask_unexpanded_dollars(no_comment.strip())
    if not cmd:
        return HookCommandKind.NON_HOOK, None, "empty command"
    tokens = _tokenize(cmd)
    if tokens is None:
        return HookCommandKind.UNCLASSIFIED, None, "unparseable command"
    if not tokens:
        return HookCommandKind.NON_HOOK, None, "empty command"

    ops = _unquoted_operators(no_comment)
    if ops:
        # `true && hook`, `echo x; hook`, `echo x | hook`: argv0 is not the only program, and
        # a later one may be a hook whose executable bit decides whether it runs.
        return (HookCommandKind.UNCLASSIFIED, None,
                "top-level shell composition (%s); only a single simple command is supported"
                % " ".join(sorted(ops)))

    inside = _substitution_present(no_comment)
    if inside is not None:
        # A COMMAND SUBSTITUTION IS EXECUTABLE SHELL, including inside a quoted argument of an
        # otherwise exempt command. `echo "$(<hook>)"` RUNS the hook -- and the outer echo
        # returns 0 whatever the hook does, so a mode-0644 file reported permission denied
        # while the gate stayed green and the target never entered the derived set.
        #
        # Only a substitution NAMING A REPOSITORY PATH is refused, not every substitution:
        # live settings.json carries `echo "branch: $(git … || echo detached)"`, which names
        # no repository file and is what this gate is meant to accept.
        return (HookCommandKind.UNCLASSIFIED, None,
                "contains a %s command substitution; substitutions are executable shell and "
                "are outside the supported hook-command language" % inside)

    # Leading VAR=value assignments are kept, unlike launchers. `_invokes_hook` in this same
    # file already treats them as part of a direct invocation, and the suite exercises that
    # shape -- two siblings disagreeing about what a hook command looks like is how one of
    # them ends up wrong. Launchers had no such sibling support and no live use.
    n_assign = _leading_assignment_words(cmd)
    assigned = n_assign > 0
    tokens = tokens[n_assign:]
    if not tokens:
        return HookCommandKind.UNCLASSIFIED, None, "assignments with no command"

    argv0 = tokens[0]
    argv0 = _sub_project_dir(argv0)
    if "$" in argv0:
        # AN UNRESOLVED EXPANSION IS NOT A PATH SEGMENT. `$CLAUDE_PROJECT_DIRoops/../.claude/
        # hooks/hook-health.sh` survives the token-boundary substitution correctly -- and then
        # got joined to the root as a LITERAL segment, where the `..` cancelled it and the
        # path resolved to the real hook. Bash expands the longer name to empty and attempts
        # `/../.claude/...`, exiting 127. Fixing the substitution was necessary and not
        # sufficient: what remains is a value this gate cannot know, so it says so.
        return (HookCommandKind.UNCLASSIFIED, None,
                "%s contains a shell expansion this gate cannot resolve, so the path bash "
                "actually runs is not knowable here" % argv0)
    base = argv0.rsplit("/", 1)[-1]

    # CONTAINMENT IS DECIDED PHYSICALLY, AND BEFORE ANY NAME-BASED EXEMPTION.
    #
    # Two opposite failures came from getting that order and that test wrong, and both were
    # fail-open -- the target left the derived set, and the expected check count derives from
    # that same list, so each loss concealed itself:
    #
    #   * The interpreter exemption was applied to the BASENAME first, so a repository hook
    #     NAMED `python3` (`$CLAUDE_PROJECT_DIR/.claude/hooks/python3`) was read as "runs the
    #     interpreter" although it is argv0 itself. Mode 0644 on it left the gate green while
    #     Claude got exit 126.
    #   * Containment was tested LEXICALLY (`startswith("..")`), which sees only the FIRST
    #     component. `.claude/../../../usr/bin/env <hook>` has no leading `..`, so it passed
    #     as a repository path, and joining it to the root then escaped the tree entirely --
    #     the executable check certified /usr/bin/env while the hook `env` actually runs, and
    #     its executable bit, went unexamined.
    #
    # So: anything spelled as a PATH is resolved against the root and classified by where it
    # physically lands. A BARE name keeps shell semantics -- it is looked up on PATH, never
    # against the repository -- so `python3` is the interpreter even if a file of that name
    # sits at the root, and `echo` is the builtin.
    if "/" in argv0:
        if root is None:
            return HookCommandKind.UNCLASSIFIED, None, "path command, no root to resolve against"
        if ".." in Path(argv0).parts:
            # NO `..` IN A HOOK PATH. `Path.resolve()` is non-strict, so
            # `$CLAUDE_PROJECT_DIR/missing/../.claude/hooks/hook-health.sh` collapses to the
            # real hook and the executable check passes -- while bash cannot traverse a
            # component that does not exist and exits 127. Resolving strictly would trade
            # that for a crash on a legitimately missing file, and verifying every
            # intermediate component is machinery. No live hook path contains `..`, so the
            # supported language simply does not have it -- which also retires the earlier
            # out-of-repository traversal case by construction rather than by containment.
            return (HookCommandKind.UNCLASSIFIED, None,
                    "%s contains a `..` component; a hook path must name its target "
                    "directly, because a traversal through a component that does not exist "
                    "resolves cleanly here and fails in the shell" % argv0)
        try:
            base_dir = Path(root).resolve()
            candidate = Path(argv0)
            resolved = (candidate if candidate.is_absolute() else base_dir / candidate).resolve()
        except (ValueError, OSError) as exc:
            return HookCommandKind.UNCLASSIFIED, None, "unresolvable path (%s)" % exc
        if resolved.is_relative_to(base_dir):
            # Reported in RESOLVED form. The caller joins this to the root, so handing back
            # the raw spelling would hand back the traversal with it.
            return (HookCommandKind.DIRECT, resolved.relative_to(base_dir).as_posix(),
                    "direct repo executable")
        # NO NAME EXEMPTION OUT HERE. The interpreter exemption used to apply to an external
        # path whose BASENAME looked right, so `/tmp/python3` -- a symlink to `/usr/bin/env`,
        # or any program at all -- was certified as "runs the interpreter" and the hook it
        # actually launches left the derived set. Verifying the binary would mean resolving
        # symlinks, or executing it, to defend a spelling NOTHING USES: live settings.json
        # runs 13 repository paths, 3 bare `python3` and 2 bare `echo`, and not one absolute
        # interpreter. So the claim is removed rather than defended, exactly as launcher
        # support was. A future `/usr/bin/python3 x.py` makes this gate red until support is
        # added deliberately.
        #
        # NOT harmless. `/usr/bin/env <hook>` runs the hook and returns 126 when it is not
        # executable; `/bin/sh -c …` can run anything. An arbitrary external executable may
        # wrap, source or otherwise invoke a repository hook, so it stays unclassified until
        # the repository actually needs one and specifies it.
        return (HookCommandKind.UNCLASSIFIED, None,
                "%s resolves to %s, outside the repository: not a supported command form, "
                "and not provably harmless -- it may invoke a repository hook"
                % (argv0, resolved))

    # A COMMAND-LOCAL ASSIGNMENT AND A BARE NAME DO NOT MIX. A bare name is resolved through
    # PATH, and an assignment can BE the PATH: `PATH=/tmp:$PATH python3 <hook>` runs whatever
    # /tmp/python3 is -- a symlink to `env`, say, which then launches the hook directly and
    # returns 126 on a mode-0644 file, while the name exemption reported "runs python3" and
    # dropped the target. Enumerating which variables matter (PATH, ENV, LD_PRELOAD, ...) is a
    # list to be wrong about; a name-based exemption simply requires that nothing local could
    # have changed what the name resolves to. Assignments stay supported in front of a repo
    # PATH, which is the shape live settings.json would use and the one the suite exercises,
    # because a path is not looked up.
    if assigned:
        return (HookCommandKind.UNCLASSIFIED, None,
                "%r is a bare name behind a command-local assignment, which can change what "
                "it resolves to" % argv0)
    if base in _INTERPRETERS:
        # THE ARGUMENT IS PART OF THE GRAMMAR. A basename-only exemption made `python3` mean
        # "not a hook invocation" whatever followed it, so
        # `python3 -c 'import os; os.system("…/hook-health.sh")'` classified INTERPRETED and
        # the hook left the derived set -- and the nested shell's permission-denied is
        # invisible because os.system's return value is discarded and python exits 0.
        #
        # The supported form is the one live settings.json uses, three times and in one
        # shape: the interpreter followed by a repository .py script. `-c`, `-m` and every
        # other flag are refused rather than reasoned about, because reasoning about what a
        # Python string argument executes is not something this gate can do.
        rest = tokens[1:]
        if not rest or rest[0].startswith("-"):
            return (HookCommandKind.UNCLASSIFIED, None,
                    "%s is invoked with %s; the supported form is the interpreter followed by "
                    "a repository .py script, and what any other argument executes is not "
                    "something this gate can establish"
                    % (base, ("no argument" if not rest else "the option %r" % rest[0])))
        script = rest[0]
        script = _sub_project_dir(script)
        # THE SAME TWO RULES AS argv0. They were written for argv0 and not carried to the
        # interpreter's argument, so `python3 $CLAUDE_PROJECT_DIR/missing/../<guard>.py`
        # normalised to the real guard while bash cannot traverse `missing`. A rule applied
        # to one path and not the other is how this file has been wrong repeatedly.
        if "$" in script:
            return (HookCommandKind.UNCLASSIFIED, None,
                    "%s is handed %s, which contains a shell expansion this gate cannot "
                    "resolve" % (base, script))
        if ".." in Path(script).parts:
            return (HookCommandKind.UNCLASSIFIED, None,
                    "%s is handed %s, which contains a `..` component; a traversal through a "
                    "component that does not exist resolves cleanly here and fails in the "
                    "shell" % (base, script))
        try:
            base_dir = Path(root).resolve() if root is not None else None
            cand = Path(script)
            resolved = None if base_dir is None else (
                cand if cand.is_absolute() else base_dir / cand).resolve()
        except (ValueError, OSError) as exc:
            return HookCommandKind.UNCLASSIFIED, None, "unresolvable script path (%s)" % exc
        if resolved is None:
            return HookCommandKind.UNCLASSIFIED, None, "interpreter script, no root to resolve against"
        if not resolved.is_relative_to(base_dir) or resolved.suffix != ".py":
            return (HookCommandKind.UNCLASSIFIED, None,
                    "%s is handed %s, which is not a repository .py script" % (base, script))
        # The SCRIPT PATH is returned, not None. Classifying from containment and the `.py`
        # suffix alone meant a deleted or renamed guard stayed INTERPRETED with no target, so
        # nothing checked that the file exists -- Claude gets a Python file-not-found and the
        # derived count never moves. It is a separate target class from the direct ones: an
        # interpreted script is argv[1], so it must EXIST and be readable, and must NOT be
        # required to be executable. That is why the three .py guards are correctly 100644.
        return (HookCommandKind.INTERPRETED,
                resolved.relative_to(base_dir).as_posix(),
                "runs %s on %s" % (base, script))
    if base in _INTENTIONAL_NON_HOOK:
        return HookCommandKind.NON_HOOK, None, "%s is not a hook" % base
    return (HookCommandKind.UNCLASSIFIED, None,
            "%r is neither a repository path nor a recognised non-hook command" % argv0)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo-root", default=None)
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()

    root = Path(args.repo_root) if args.repo_root else Path(
        subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True,
                       text=True, check=True).stdout.strip()
    )

    failures: list[str] = []
    checked = 0

    def ok(msg: str) -> None:
        nonlocal checked
        checked += 1
        if args.verbose:
            print(f"  ok    {msg}")

    # 1. every required lifecycle event is wired to the hook
    settings_path = root / ".claude" / "settings.json"
    try:
        settings = json.loads(settings_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        failures.append(f".claude/settings.json unreadable: {exc}")
        settings = {}

    hooks = settings.get("hooks") or {}
    for event, consequence in REQUIRED_EVENTS.items():
        # A SUBSTRING test passed for `true  # .claude/hooks/session-lifecycle.sh`, i.e. the
        # runtime fully off while the gate printed ok. Require the hook to be the command.
        entries = [
            (entry.get("matcher", ""), h.get("command", ""))
            for entry in (hooks.get(event) or [])
            for h in (entry.get("hooks") or [])
        ]
        invoking = [m for m, c in entries if _invokes_hook(c, root)]
        if not invoking:
            failures.append(f"{event} does not invoke {HOOK} — {consequence}")
            continue

        # MATCHERS WERE NEVER INSPECTED. Narrowing PostToolUse to `NotebookEdit`, or
        # SessionStart to `fork`, left the gate green while producing exactly the consequence
        # the gate itself prints.
        required = REQUIRED_MATCHER_TOKENS.get(event)
        if required:
            covered = any(
                all(re.search(rf"(^|\|){re.escape(tok)}($|\|)", m) for tok in required)
                for m in invoking
            )
            if not covered:
                failures.append(
                    f"{event} invokes {HOOK} but its matcher {invoking!r} does not cover "
                    f"{required} — {consequence}"
                )
                continue
        ok(f"{event} -> {HOOK} (matcher {invoking[0]!r})")

    # 2. the things the hooks call actually exist and can be executed
    hook_targets, interpreted_targets, unclassified_cmds = direct_hook_targets(settings, root)
    for cmd, detail in unclassified_cmds:
        failures.append(
            f"settings.json hook command cannot be classified: {cmd!r} ({detail}). The gate "
            "cannot prove whether this executes a repository hook, and silently ignoring it "
            "would drop an executable check while shrinking the expected count to match")
    for rel in REQUIRED_EXECUTABLES + hook_targets:
        path = root / rel
        if not path.is_file():
            failures.append(f"missing: {rel}")
        elif not os.access(path, os.X_OK):
            failures.append(f"not executable: {rel} (chmod +x)")
        else:
            ok(f"executable: {rel}")

    # 2a. An INTERPRETED script is argv[1], so its executable bit does not matter -- but its
    # EXISTENCE does. Renaming one left the gate green while Claude got a file-not-found.
    for rel in interpreted_targets:
        path = root / rel
        if not path.is_file():
            failures.append(
                f"missing: {rel} — settings.json runs it through an interpreter, so the bit "
                "is not required, but the file has to be there")
        elif not os.access(path, os.R_OK):
            failures.append(f"not readable: {rel} (the interpreter must be able to open it)")
        else:
            ok(f"interpreted script present: {rel}")

    # 3. provider adapters still reach the ops MCP
    for rel, keypath in PROVIDER_MCP_CONFIGS:
        path = root / rel
        if not path.is_file():
            # NOT "continue # optional adapter". A missing file is the MOST likely way to lose
            # a provider's only capability route, and skipping it silently dropped the check
            # while COVERAGE below went on printing that the adapter "declares the ops MCP in
            # .cursor/mcp.json" — a claim about a file that did not exist. Deleting
            # .cursor/mcp.json passed 24/0; deleting .mcp.json passed 24/0. Both now fail.
            failures.append(
                f"missing: {rel} — the gate claims this adapter reaches the ops MCP, so its "
                "absence is a coverage failure, not an optional extra"
            )
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except ValueError as exc:
            failures.append(f"{rel} is not valid JSON: {exc}")
            continue
        node = data
        for key in keypath:
            node = node.get(key) if isinstance(node, dict) else None
        if node is None:
            failures.append(
                f"{rel} no longer declares {'.'.join(keypath)} — that provider would lose the "
                "ops MCP surface, which is its ONLY capability route (it gets no hooks)"
            )
        else:
            ok(f"{rel} declares {'.'.join(keypath)}")

    # 4. the hook must be resilient: it may never hard-fail a session.
    #
    # This used to grep for the literal string "exit 0" and then PRINT "exits 0 on every path".
    # A hook whose only `exit 0` was unreachable passed while hard-failing every session. Run
    # it instead, with the payload shapes that actually reach it, and check the exit status.
    hook_path = root / HOOK
    if hook_path.is_file() and os.access(hook_path, os.X_OK):
        probes = [
            ("SessionStart", '{"hook_event_name":"SessionStart","session_id":"probe","cwd":"%s"}' % root),
            ("PostToolUse", '{"hook_event_name":"PostToolUse","session_id":"probe","cwd":"%s","tool_name":"Bash"}' % root),
            ("Stop", '{"hook_event_name":"Stop","session_id":"probe","cwd":"%s"}' % root),
            ("UserPromptSubmit", '{"hook_event_name":"UserPromptSubmit","session_id":"probe","cwd":"%s"}' % root),
            ("SessionEnd", '{"hook_event_name":"SessionEnd","session_id":"probe","cwd":"%s","reason":"clear"}' % root),
            ("malformed", "not json at all"),
            ("empty", ""),
        ]
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ)
            env["ICN_OPS_DB"] = str(Path(tmp) / "probe.db")
            env["ICN_ROOT"] = tmp
            env["CLAUDE_PROJECT_DIR"] = str(root)
            for label, payload in probes:
                try:
                    r = subprocess.run(
                        ["bash", str(hook_path)], input=payload, capture_output=True,
                        # The declared budget in settings.json, not an arbitrary 30s: a 12s
                        # hook passed this gate and was then killed live by the harness.
                        text=True, env=env, timeout=(10 if label == "SessionStart" else 5),
                    )
                except subprocess.TimeoutExpired:
                    failures.append(f"{HOOK} timed out on a {label} payload; it must never block a session")
                    continue
                if r.returncode != 0:
                    failures.append(
                        f"{HOOK} exited {r.returncode} on a {label} payload; a lifecycle hook "
                        "must never fail a session"
                    )
                    continue
                ok(f"{HOOK} exits 0 on a {label} payload")

                # Exit status alone proved nothing: the hook exits 0 on every path BY DESIGN,
                # so eight behavioural mutations (SessionStart printing nothing, the banner
                # suppressed, the registry call removed) all passed. Assert the OUTPUT.
                if label == "SessionStart":
                    out = r.stdout
                    if "ICN agent runtime" not in out:
                        failures.append(
                            f"{HOOK} produced no startup context on SessionStart; an agent "
                            "would never learn the runtime exists"
                        )
                    elif "DEGRADED" not in out and "session:" not in out:
                        failures.append(
                            f"{HOOK} startup context does not state the session's lifecycle "
                            "status; it must never leave that ambiguous"
                        )
                    else:
                        ok(f"{HOOK} emits startup context naming its lifecycle status")
                elif label in ("PostToolUse", "Stop", "UserPromptSubmit", "SessionEnd"):
                    # These must stay SILENT with a well-formed payload — a banner after every
                    # tool call was a real regression.
                    if r.stdout.strip():
                        failures.append(
                            f"{HOOK} printed to stdout on {label}; only SessionStart and an "
                            "unknown-event degrade may emit context"
                        )
                    else:
                        ok(f"{HOOK} stays silent on {label}")

    # 2b. THE DEGRADED PATH, probed in a degraded fixture.
    #
    # The healthy probes above cannot see the banner at all: with a working helper the hook
    # emits the normal context and the banner never fires. So suppressing the banner entirely,
    # or printing it after every tool call, both passed. Recreate the hook in a scratch tree
    # with NO helper and assert both halves of the contract.
    if hook_path.is_file():
        with tempfile.TemporaryDirectory() as tmp:
            fake = Path(tmp) / "fakeroot"
            (fake / ".claude" / "hooks").mkdir(parents=True)
            (fake / ".claude" / "hooks" / "session-lifecycle.sh").write_text(
                hook_path.read_text(encoding="utf-8"), encoding="utf-8"
            )
            env = dict(os.environ)
            env["CLAUDE_PROJECT_DIR"] = str(fake)   # no ops/scripts/icn-agent-session here
            env["ICN_OPS_DB"] = str(fake / "x.db")
            env["ICN_ROOT"] = str(fake)
            probes = {
                "SessionStart": ('{"hook_event_name":"SessionStart","session_id":"p","cwd":"%s"}' % fake, True),
                "PostToolUse": ('{"hook_event_name":"PostToolUse","session_id":"p","cwd":"%s","tool_name":"Bash"}' % fake, False),
                "Stop": ('{"hook_event_name":"Stop","session_id":"p","cwd":"%s"}' % fake, False),
            }
            for label, (payload, expect_banner) in probes.items():
                r = subprocess.run(
                    ["bash", str(fake / ".claude" / "hooks" / "session-lifecycle.sh")],
                    input=payload, capture_output=True, text=True, env=env, timeout=15,
                )
                got_banner = "DEGRADED" in r.stdout
                if expect_banner and not got_banner:
                    failures.append(
                        f"{HOOK} did not announce DEGRADED on {label} with the helper missing; "
                        "the runtime must never be disabled silently"
                    )
                elif not expect_banner and got_banner:
                    failures.append(
                        f"{HOOK} printed the DEGRADED banner on {label}; only SessionStart and "
                        "an unknown event may emit it, or it repeats after every tool call"
                    )
                else:
                    ok(f"{HOOK} degraded-path behaviour correct on {label}")

    # 4b. THE HOOK MUST ACTUALLY WRITE TO THE REGISTRY.
    #
    # stdout probes cannot see this: neutering the helper-invoking wrapper leaves the startup
    # context intact while progress, interaction and release silently never happen. Drive a
    # real SessionStart + PostToolUse against a scratch registry and assert the row moved.
    if hook_path.is_file() and (root / "ops" / "mcp" / "dist" / "cli" / "session.js").is_file():
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ)
            env["ICN_OPS_DB"] = str(Path(tmp) / "probe.db")
            env["ICN_ROOT"] = tmp
            env["CLAUDE_PROJECT_DIR"] = str(root)
            sid = "adoption-probe-session"
            for payload in (
                '{"hook_event_name":"SessionStart","session_id":"%s","cwd":"%s"}' % (sid, root),
                '{"hook_event_name":"PostToolUse","session_id":"%s","cwd":"%s","tool_name":"Bash"}' % (sid, root),
            ):
                subprocess.run(["bash", str(hook_path)], input=payload, capture_output=True,
                               text=True, env=env, timeout=20)
            status = subprocess.run(
                [str(root / "ops" / "scripts" / "icn-agent-session"), "status",
                 "--harness-key", sid],
                capture_output=True, text=True, env=env, timeout=20,
            )
            try:
                row = json.loads(status.stdout or "{}")
            except ValueError:
                row = {}
            if not row.get("registered"):
                failures.append(
                    f"{HOOK} did not register a session in the registry; the startup context "
                    "would claim tracking is active while nothing is recorded"
                )
            elif not row.get("progress_count"):
                failures.append(
                    f"{HOOK} registered a session but PostToolUse recorded no progress; "
                    "every lane would look stalled"
                )
            else:
                ok(f"{HOOK} writes registration and progress to the registry")


    # A CHECK THAT VANISHES MUST NOT READ AS A CHECK THAT PASSED.
    #
    # Several blocks below skip themselves when their input is absent, so the total silently
    # dropped to 24 without ops/mcp/dist (which is gitignored), 24 without .mcp.json, and 23
    # without both — every one of them exiting 0. The strongest check in the file, the registry
    # write-through, was one of the ones that disappeared. CI happens to build first today, so
    # this was latent rather than live; a floor makes it neither.
    # EXACT, not a floor. `checked < EXPECTED_CHECKS` let a spurious extra ok() report
    # "26 check(s) passed" and exit 0 — which is how a duplicated check masks a lost one.
    expected = EXPECTED_STATIC_CHECKS + len(hook_targets) + len(interpreted_targets)
    if checked != expected:
        failures.append(
            f"{checked} checks ran, expected exactly {expected} — a check that skipped "
            "itself is "
            "not a check that passed. Something the gate depends on is missing (an unbuilt "
            "ops/mcp/dist, a deleted adapter); fix that rather than lowering "
            "EXPECTED_STATIC_CHECKS"
        )

    print(f"agent-runtime adoption: {checked} check(s) passed, {len(failures)} failure(s)")
    if args.verbose or failures:
        print("\nLauncher coverage:")
        for status, entries in COVERAGE.items():
            print(f"  {status.upper()}:")
            for name, note in entries.items():
                print(f"    - {name}: {note}")

    if failures:
        print("\nFAILURES:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
