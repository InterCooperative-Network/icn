#!/usr/bin/env bash
# PreToolUse hook: Warns when editing files outside the scope implied by the current branch name.
# Non-blocking — advisory only.
#
# Branch naming convention: feat/<scope>-*, fix/<scope>-*, etc.
# Scope is inferred from the first path segment after the prefix.
# Example: fix/ledger-overflow → scope=ledger → warn if editing icn-trust/

set -euo pipefail

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi

# Get current branch
BRANCH=$(git branch --show-current 2>/dev/null || echo "")

# Skip on branches where scope enforcement doesn't apply
if [[ -z "$BRANCH" ]] || [[ "$BRANCH" == "main" ]] || [[ "$BRANCH" == "develop" ]]; then
  exit 0
fi

# Only check branches with a type prefix (feat/, fix/, refactor/, etc.)
if ! echo "$BRANCH" | grep -qE '^(feat|fix|refactor|docs|chore|test|ci)/'; then
  exit 0
fi

# Extract full scope segment from branch name (e.g. fix/http-kit-refactor → http-kit-refactor)
# Using the full segment (not just up to the first '-') allows matching hyphenated scopes.
SCOPE_SEGMENT=$(echo "$BRANCH" | sed -E 's|^[^/]+/([^/]+).*|\1|')

if [[ -z "$SCOPE_SEGMENT" ]] || [[ ${#SCOPE_SEGMENT} -lt 3 ]]; then
  exit 0
fi

# Build list of known ICN crate scopes
KNOWN_SCOPES="core identity trust net gossip ledger ccl store rpc obs gateway governance compute security time snapshot crypto steward zkp community coop entity encoding api naming authz federation privacy protocol services http-kit"

# Find the longest matching known scope (prefix match against full segment).
# Longest match handles hyphenated scopes correctly: "http-kit-refactor" matches "http-kit", not "http".
MATCHED_SCOPE=""
LONGEST_MATCH_LEN=0
for scope in $KNOWN_SCOPES; do
  if [[ "$SCOPE_SEGMENT" == "$scope"* ]]; then
    if (( ${#scope} > LONGEST_MATCH_LEN )); then
      MATCHED_SCOPE="$scope"
      LONGEST_MATCH_LEN=${#scope}
    fi
  fi
done

if [[ -z "$MATCHED_SCOPE" ]]; then
  exit 0
fi

# Check if the file being edited is in a different crate
# Extract crate name from file path (e.g. icn/crates/icn-trust/src/... → trust)
FILE_CRATE=""
if echo "$FILE_PATH" | grep -qE 'crates/icn-([^/]+)/'; then
  FILE_CRATE=$(echo "$FILE_PATH" | sed 's|.*crates/icn-\([^/]*\)/.*|\1|')
fi

if [[ -z "$FILE_CRATE" ]]; then
  exit 0
fi

# Warn if editing a crate that doesn't match scope
if [[ "$FILE_CRATE" != "$MATCHED_SCOPE"* ]] && [[ "$MATCHED_SCOPE" != "$FILE_CRATE"* ]]; then
  cat <<EOF
{"continue": true, "systemMessage": "Scope warning: branch '${BRANCH}' suggests scope '${MATCHED_SCOPE}' but you are editing 'icn-${FILE_CRATE}'. If this is intentional (e.g. fixing a downstream consumer), ignore this warning. If not, confirm you are on the correct branch."}
EOF
fi

exit 0
