#!/usr/bin/env bash
# PreToolUse hook: Warns when adding dependencies to crate Cargo.toml
# that might not be in [workspace.dependencies]
#
# Triggered by: Edit/Write on **/Cargo.toml

set -euo pipefail

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

# Only check crate-level Cargo.toml (not workspace root)
if [[ "$FILE_PATH" != *"Cargo.toml" ]]; then
    exit 0
fi

# Skip the workspace root Cargo.toml
if [[ "$FILE_PATH" == *"icn/Cargo.toml" ]] && [[ "$FILE_PATH" != *"crates/"* ]] && [[ "$FILE_PATH" != *"bins/"* ]]; then
    exit 0
fi

# Check if the edit adds a new dependency line (version = "x.y").
# Only inspect new_string (the edited snippet); do not fall back to content
# (the full file for Write operations) — that triggers false positives on
# every crate Cargo.toml write because [package] version = "..." always matches.
NEW_CONTENT=$(echo "$INPUT" | jq -r '.tool_input.new_string // empty' 2>/dev/null)
if [[ -z "$NEW_CONTENT" ]]; then
    exit 0
fi
if echo "$NEW_CONTENT" | grep -qE 'version\s*=\s*"[0-9]'; then
    echo "Direct version specified in crate Cargo.toml. Prefer workspace dependencies:"
    echo "  1. Add to icn/Cargo.toml [workspace.dependencies]"
    echo "  2. Use dep.workspace = true in crate Cargo.toml"
    # Warning only, don't block
    exit 0
fi

exit 0
