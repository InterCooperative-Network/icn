#!/bin/bash
# Hook: PreToolUse (Edit|Write) guard for panics in protocol paths.
# ICN invariant: "No panics in protocol paths".
#
# Output contract (see HOOKS.md):
#   exit 2, reason on stderr           -> the tool call is BLOCKED
#   exit 0, nothing on stdout          -> allowed silently
#   exit 0, ONE JSON object on stdout  -> allowed with a warning ({"continue":true,"systemMessage":...})
# Any other exit code is a non-blocking error in Claude Code, so never use exit 1 to block.
#
# BLOCKS a Write whose full content has panic!() outside test-only code.
# WARNS on panic!() in an Edit snippet (the surrounding context is unknown) and on .unwrap()/.expect().
#
# Test-only code: files under /tests/, test_* paths, *_test.rs and tests.rs; and, inside any other
# file, the item that directly follows #[cfg(test)] or #[allow(clippy::panic)]. Only that item's span is
# exempt: a test module never exempts production code elsewhere in the same file.
#
# Malformed input (not a JSON object, or no string file_path) is not something Claude Code sends; it is
# allowed silently, with nothing on stdout. Missing jq degrades the same way (HOOKS.md, Prerequisites).

command -v jq >/dev/null 2>&1 || exit 0

INPUT=$(cat)
field() {
  printf '%s' "$INPUT" | jq -r --arg f "$1" \
    'if type == "object" and (.tool_input | type) == "object" then (.tool_input[$f] | strings) else empty end' \
    2>/dev/null
}
FILE_PATH=$(field file_path)

# Only Rust files.
if [[ -z "$FILE_PATH" || "$FILE_PATH" != *.rs ]]; then
  exit 0
fi

# Test files (matched against the whole path string, not line by line).
TEST_PATH_RE='/tests/|/test_|_test\.rs$|(^|/)tests\.rs$'
if [[ "$FILE_PATH" =~ $TEST_PATH_RE ]]; then
  exit 0
fi

NEW_STRING=$(field new_string)
FULL_CONTENT=$(field content)
NEW_CONTENT="${NEW_STRING}${FULL_CONTENT}"
if [[ -z "$NEW_CONTENT" ]]; then
  exit 0
fi

# Print the code that is NOT test-only: `//` comments are dropped, and the item following
# #[cfg(test)] or #[allow(clippy::panic)] is skipped up to its matching `}` (or a `;` before any `{`).
# Braces are counted lexically, so a lone brace in a string or char literal inside a skipped item can
# end or extend the skip early or late; that is a limitation of a text hook, not a parser.
non_test_code() {
  printf '%s\n' "$1" | awk '
    function skip_to_end(s,    i, c) {
      for (i = 1; i <= length(s); i++) {
        c = substr(s, i, 1)
        if (c == "{") { depth++; opened = 1 }
        else if (c == "}") { depth--; if (opened && depth <= 0) { skipping = 0; return i } }
        else if (c == ";" && !opened) { skipping = 0; return i }
      }
      return 0
    }
    {
      line = $0
      # A comment starts at `//` at line start or after whitespace, so "http://..." in code is kept.
      sub(/(^|[ \t])\/\/.*/, "", line)
      out = ""
      while (1) {
        if (skipping) {
          end = skip_to_end(line)
          if (skipping) { line = ""; break }
          line = substr(line, end + 1)
          continue
        }
        if (match(line, /#\[[ \t]*cfg[ \t]*\([ \t]*test[ \t]*\)[ \t]*\]|#\[[ \t]*allow[ \t]*\([^]]*clippy::panic[ \t]*[,)][^]]*\]/)) {
          out = out substr(line, 1, RSTART - 1)
          line = substr(line, RSTART + RLENGTH)
          skipping = 1; depth = 0; opened = 0
          continue
        }
        out = out line
        break
      }
      print out
    }'
}

PANIC_RE='(^|[^A-Za-z0-9_])panic![[:space:]]*[(\[{]'
MESSAGES=""
add_message() {
  MESSAGES="${MESSAGES}${MESSAGES:+
}$1"
}

# --- BLOCKING / WARNING: panic!() outside test-only code ---
if non_test_code "$NEW_CONTENT" | grep -qE "$PANIC_RE"; then
  if [[ -n "$FULL_CONTENT" ]]; then
    # Write: the full file is known, so this is a real violation. Block.
    {
      printf 'PANIC GUARD VIOLATION in %s:\n' "$FILE_PATH"
      printf '  panic!() found in non-test code.\n'
      printf "  ICN invariant: 'No panics in protocol paths'\n"
      printf '  Use Result<T, E> and propagate errors instead.\n'
      printf '  If this branch is intended to be unreachable, model it as a typed error\n'
      printf '  or, in non-protocol code only, a debug_assert! with a SAFETY comment.\n'
      printf '  Test-only code is exempt: put it under #[cfg(test)] or in a test file.\n'
    } >&2
    exit 2
  fi
  # Edit: only a snippet is known; it may sit inside an existing #[cfg(test)] item. Warn.
  add_message "  - panic!() in snippet: if this is inside a #[cfg(test)] block ignore this warning. Otherwise, use Result<T, E>. Never use unreachable!() as a substitute (it also panics)."
fi

# --- WARNING: .unwrap()/.expect() outside test modules ---
if printf '%s' "$NEW_CONTENT" | grep -qE '\.unwrap\(\)'; then
  if ! printf '%s' "$NEW_CONTENT" | grep -qE '#\[cfg\(test\)\]|#\[allow\(clippy::unwrap_used\)\]'; then
    add_message '  - .unwrap() found - prefer Result<T,E> or document why it cannot fail with .expect("reason")'
  fi
fi

if printf '%s' "$NEW_CONTENT" | grep -qE '\.expect\('; then
  if ! printf '%s' "$NEW_CONTENT" | grep -qE '#\[cfg\(test\)\]|#\[allow\(clippy::expect_used\)\]'; then
    add_message '  - .expect() found - ensure this truly cannot fail in production; add a // SAFETY: comment if so'
  fi
fi

if [[ -n "$MESSAGES" ]]; then
  # One object, serialized by jq: quotes, backslashes and control characters in the path or text are
  # escaped, never interpolated into hand-written JSON.
  jq -nc --arg path "$FILE_PATH" --arg body "$MESSAGES" '{
    continue: true,
    systemMessage: ("Panic guard warning in " + $path + ":\n" + $body
      + "\nSee AGENTS.md invariant: '"'"'No panics in protocol paths'"'"'. Use #[allow(clippy::unwrap_used)] with a SAFETY comment if the panic is truly unreachable.")
  }'
fi

exit 0
