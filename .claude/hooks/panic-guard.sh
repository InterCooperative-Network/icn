#!/bin/bash
# Hook: PreToolUse guard for panics in protocol paths
# Warns (does not block) when unwrap/expect is added to non-test Rust code

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

# Only check Rust files
if [[ -z "$FILE_PATH" ]] || [[ "$FILE_PATH" != *.rs ]]; then
  exit 0
fi

# Skip test files
if echo "$FILE_PATH" | grep -qE '/tests/|/test_|_test\.rs$'; then
  exit 0
fi

NEW_CONTENT=$(echo "$INPUT" | jq -r '.tool_input.new_string // .tool_input.content // empty')

if [[ -z "$NEW_CONTENT" ]]; then
  exit 0
fi

# Check for panic-inducing patterns in non-test code
WARNINGS=""

# Check for .unwrap() - but not in test modules
if echo "$NEW_CONTENT" | grep -qE '\.unwrap\(\)'; then
  # Don't flag if inside #[cfg(test)] block (approximation)
  if ! echo "$NEW_CONTENT" | grep -qE '#\[cfg\(test\)\]'; then
    WARNINGS="${WARNINGS}\n  - .unwrap() found - use Result<T,E> or proper error handling instead"
  fi
fi

if echo "$NEW_CONTENT" | grep -qE '\.expect\('; then
  if ! echo "$NEW_CONTENT" | grep -qE '#\[cfg\(test\)\]'; then
    WARNINGS="${WARNINGS}\n  - .expect() found - use Result<T,E> or proper error handling instead"
  fi
fi

if echo "$NEW_CONTENT" | grep -qE 'panic!\('; then
  WARNINGS="${WARNINGS}\n  - panic!() found - never panic in protocol/actor/network paths"
fi

if [[ -n "$WARNINGS" ]]; then
  # Output as JSON with system message (warning, not blocking)
  cat <<EOF
{"continue": true, "systemMessage": "Warning: Potential panic in protocol code (${FILE_PATH}):${WARNINGS}\nConsider using Result<T, E> instead. See AGENTS.md invariant: 'No panics in protocol paths'."}
EOF
  exit 0
fi

exit 0
