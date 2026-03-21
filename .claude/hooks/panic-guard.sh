#!/bin/bash
# Hook: PreToolUse guard for panics in protocol paths
# BLOCKS on panic!() in non-test Rust code (ICN invariant: No panics in protocol paths)
# WARNS on .unwrap()/.expect() in non-test Rust code

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

# --- BLOCKING: panic!() is never allowed in non-test protocol paths ---
# Skip if the panic is inside a #[cfg(test)] block or #[allow(clippy::panic)] annotation
if echo "$NEW_CONTENT" | grep -qE 'panic!\('; then
  if ! echo "$NEW_CONTENT" | grep -qE '#\[cfg\(test\)\]|#\[allow\(clippy::panic\)\]'; then
    echo "PANIC GUARD VIOLATION in ${FILE_PATH}:" >&2
    echo "  panic!() found in non-test code." >&2
    echo "  ICN invariant: 'No panics in protocol paths'" >&2
    echo "  Use Result<T, E> and propagate errors instead." >&2
    echo "  If this is intentionally unreachable, use unreachable!() with a comment." >&2
    exit 1
  fi
fi

# --- WARNING: .unwrap()/.expect() outside test modules ---
WARNINGS=""

if echo "$NEW_CONTENT" | grep -qE '\.unwrap\(\)'; then
  if ! echo "$NEW_CONTENT" | grep -qE '#\[cfg\(test\)\]|#\[allow\(clippy::unwrap_used\)\]'; then
    WARNINGS="${WARNINGS}\n  - .unwrap() found - prefer Result<T,E> or document why it cannot fail with .expect(\"reason\")"
  fi
fi

if echo "$NEW_CONTENT" | grep -qE '\.expect\('; then
  if ! echo "$NEW_CONTENT" | grep -qE '#\[cfg\(test\)\]|#\[allow\(clippy::expect_used\)\]'; then
    WARNINGS="${WARNINGS}\n  - .expect() found - ensure this truly cannot fail in production; add a // SAFETY: comment if so"
  fi
fi

if [[ -n "$WARNINGS" ]]; then
  cat <<EOF
{"continue": true, "systemMessage": "Panic guard warning in ${FILE_PATH}:${WARNINGS}\nSee AGENTS.md invariant: 'No panics in protocol paths'. Use #[allow(clippy::unwrap_used)] with a SAFETY comment if the panic is truly unreachable."}
EOF
fi

exit 0
