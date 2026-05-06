#!/usr/bin/env bash
# PreToolUse hook: Warns when adding TODO/FIXME comments without an issue number.
# Enforces the convention: // TODO(#123): description
# Non-blocking — advisory only.

set -euo pipefail

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

# Only check source files (Rust and TypeScript)
if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi

if ! echo "$FILE_PATH" | grep -qE '\.(rs|ts|tsx|js)$'; then
  exit 0
fi

NEW_CONTENT=$(echo "$INPUT" | jq -r '.tool_input.new_string // .tool_input.content // empty' 2>/dev/null)

if [[ -z "$NEW_CONTENT" ]]; then
  exit 0
fi

# Find TODO/FIXME/HACK lines that don't have an issue number
# Valid formats: // TODO(#123): ..., // FIXME(#456): ..., // TODO(username#123): ...
#
# Pipefail handling: a blanket `|| true` would also swallow real
# failures (regex error, missing tool, broken pipe other than head's
# expected SIGPIPE close, etc.). Instead, run the search with pipefail
# off, capture per-stage exit codes via PIPESTATUS, and accept only:
#   0   — match found
#   1   — no match at this stage (the common case)
#   141 — SIGPIPE: head -5 closed the pipe early when there are >5
#         matches; upstream greps die from SIGPIPE
# Anything else (e.g. grep exit 2 = regex/file error, exit 127 =
# command not found) is reported and the hook returns non-zero so the
# Claude Code harness surfaces the failure rather than silently
# advising-nothing.
SEARCH_OUT=$(mktemp 2>/dev/null) || SEARCH_OUT="/tmp/todo-guard.$$.out"
trap 'rm -f "$SEARCH_OUT"' EXIT

set +o pipefail
echo "$NEW_CONTENT" \
  | grep -nE '(//|#)\s*(TODO|FIXME|HACK)\b' \
  | grep -vE '(//|#)\s*(TODO|FIXME|HACK)\(#[0-9]+\)' \
  | grep -vE '(//|#)\s*(TODO|FIXME|HACK)\([a-zA-Z0-9_-]+#[0-9]+\)' \
  | head -5 \
  > "$SEARCH_OUT"
SEARCH_RC=("${PIPESTATUS[@]}")
set -o pipefail

# Acceptable per-stage exit codes: 0 (matches), 1 (no match), 141
# (SIGPIPE from head -5 closing the pipe early). Anything else is a
# real error.
for rc in "${SEARCH_RC[@]}"; do
  case "$rc" in
    0|1|141) ;;
    *)
      echo "todo-guard: search pipeline stage failed with exit $rc — likely regex error, missing tool, or unexpected signal" >&2
      exit 2
      ;;
  esac
done

BARE_TODOS=$(cat "$SEARCH_OUT")

if [[ -n "$BARE_TODOS" ]]; then
  COUNT=$(echo "$BARE_TODOS" | wc -l | tr -d ' ')
  cat <<EOF
{"continue": true, "systemMessage": "TODO debt warning in ${FILE_PATH}: ${COUNT} TODO/FIXME/HACK comment(s) without an issue number.\nUse the format: // TODO(#123): description\nCreate a GitHub issue first if one doesn't exist.\nThis prevents untracked technical debt from accumulating."}
EOF
fi

exit 0
