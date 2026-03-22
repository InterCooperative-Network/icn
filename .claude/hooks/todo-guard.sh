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
BARE_TODOS=$(echo "$NEW_CONTENT" | grep -nE '(//|#)\s*(TODO|FIXME|HACK)\b' | grep -vE '(//|#)\s*(TODO|FIXME|HACK)\(#[0-9]+\)' | grep -vE '(//|#)\s*(TODO|FIXME|HACK)\([a-zA-Z0-9_-]+#[0-9]+\)' | head -5)

if [[ -n "$BARE_TODOS" ]]; then
  COUNT=$(echo "$BARE_TODOS" | wc -l | tr -d ' ')
  cat <<EOF
{"continue": true, "systemMessage": "TODO debt warning in ${FILE_PATH}: ${COUNT} TODO/FIXME/HACK comment(s) without an issue number.\nUse the format: // TODO(#123): description\nCreate a GitHub issue first if one doesn't exist.\nThis prevents untracked technical debt from accumulating."}
EOF
fi

exit 0
