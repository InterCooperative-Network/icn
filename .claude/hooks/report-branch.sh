#!/usr/bin/env bash
# Print the current branch for the Edit|Write PreToolUse context line.
#
# This exists as a file rather than inline in settings.json for a checkable reason.
# `check-agent-runtime-adoption.py` proves that every command settings.json runs AS argv0 is a
# repository executable that exists and has its bit set. It deliberately does NOT interpret
# shell: a command substitution can execute a repository hook without naming one --
# `echo "$(find . -name hook-health.sh -exec {} \;)"` is the case that settled it (icn#2691
# review) -- and the only alternative was a blocklist of `-exec`/`xargs`/`sh -c`/`eval`/git
# aliases, which the sibling PR spent nine review rounds proving does not terminate.
#
# So the shell lives here, where it is a program with an owner, and settings.json invokes a
# path the gate can actually prove things about. Behaviour is unchanged.
set -uo pipefail

branch="$(git branch --show-current 2>/dev/null || true)"
printf 'branch: %s\n' "${branch:-detached}"
