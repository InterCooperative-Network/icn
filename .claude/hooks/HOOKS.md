# ICN Hook Patterns

## Input Protocol

All hooks receive tool data on stdin as JSON. Never use bare env vars as the
primary input path — they are not reliably set by Claude Code.

Correct pattern (used by all working hooks in this directory):

```bash
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null)
NEW_CONTENT=$(echo "$INPUT" | jq -r '.tool_input.new_string // .tool_input.content // empty' 2>/dev/null)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty' 2>/dev/null)
```

`CLAUDE_TOOL_INPUT_FILE_PATH` is available as an env var for simple single-field
path checks (see website worktree hooks), but stdin is preferred for consistency
and works regardless of Claude Code version.

## Hook Dependencies

Blocking hooks require `jq` to parse stdin JSON. If `jq` is missing, blocking hooks
silently pass — this is enforcement theater. The `hook-health.sh` check runs once per
session and exits 2 (BLOCK) when critical dependencies are absent.

| Tool | Required by | Severity if missing |
|------|------------|---------------------|
| jq | firewall-guard.sh, panic-guard.sh, + all advisory hooks | CRITICAL — blocking hooks silently pass; advisory hooks non-functional |
| git | scope-guard.sh, pre-bash-guard.py | CRITICAL — branch checks fail |
| cargo | build verification | WARN — can't verify compilation |
| rg | advisory hooks | WARN — advisory checks degrade |
| gh | PR workflows | WARN — PR workflows unavailable |

The kernel and domain crate lists used by `firewall-guard.sh` (the blocking
firewall hook) are centralized in `kernel-crates.conf`. Update that file when
crates are added or removed. Advisory hooks (`scope-guard.sh`, `pre-tool-guard.py`)
still use their own hardcoded lists — centralizing those is deferred.

## Hook Inventory

| File | Trigger | Effect |
|------|---------|--------|
| hook-health.sh | Any tool (once per session) | BLOCKS if jq or git missing; WARNS otherwise |
| session-orient.sh | SessionStart (startup) | Prints branch, sprint cadence (resolved via the registered `sprint_state` owner), skills, invariants. Never blocks |
| firewall-guard.sh | Edit/Write .rs in kernel crates | BLOCKS domain imports in kernel |
| panic-guard.sh | Edit/Write .rs in non-test files | BLOCKS (exit 2) a Write with panic!() outside test-only code; WARNS on panic!() in Edit snippets and on unwrap()/expect(). See [panic-guard.sh](#panic-guardsh) |
| scope-guard.sh | Edit/Write any file | WARNS if edit is outside branch scope |
| dep-guard.sh | Edit/Write Cargo.toml | WARNS on direct version pinning in crates |
| todo-guard.sh | Edit/Write .rs/.ts | WARNS on bare TODO without issue number |
| pre-tool-guard.py | Edit/Write protected crates | Checklist for high-risk crates |
| pre-bash-guard.py | Bash commands | WARNS on direct main branch operations |
| openapi-sync-guard.sh | Edit/Write gateway routes | REMINDS to regenerate OpenAPI + TS types |
| post-tool-guard.py | Edit/Write .rs | SUGGESTS cargo check after edit |

## panic-guard.sh

Output contract, which `tests/test_panic_guard.sh` checks:

- **Block:** exit `2`, with the reason on stderr and nothing on stdout. In Claude Code, only exit
  `2` blocks a tool call; any other non-zero status is a non-blocking error. The hook used `exit 1`
  until 2026-11, so its documented block never fired.
- **Warn:** exit `0` and **exactly one** JSON object on stdout,
  `{"continue": true, "systemMessage": "..."}`. It is serialized with `jq --arg`, so quotes,
  backslashes and control characters in the file path or message are escaped. Never build hook JSON
  by interpolating text into a heredoc.
- **Allow:** exit `0`, with nothing on stdout.
- **Malformed input:** a payload that is not a JSON object, or has no string `file_path`, is allowed
  silently and never produces invalid output. So is a missing `jq`, as described under Hook Dependencies.

What counts as test-only code:

- Files whose path contains `/tests/` or `/test_`, or ends in `_test.rs` or `tests.rs` as a whole
  file name. `src/actor/tests.rs` counts; `contests.rs` does not.
- In any other file, only the item directly after `#[cfg(test)]` or `#[allow(clippy::panic)]`.
  The exemption runs to that item's matching `}`, or to a `;` before any `{`. A test module never
  exempts production code elsewhere in the same file.
- `//` and `///` comments are ignored. A comment starts at `//` at the start of a line or after
  whitespace, so `"http://…"` in code is still checked.

The hook works on text; it does not parse Rust. Known limits:
- Braces are counted as text. A lone `{` or `}` inside a string or char literal within an exempt
  item (`"{"`, `'{'`, `r#"{"#`) can end the exemption early or late, so a later production panic can
  escape.
- A ` //` inside a string literal still cuts the rest of that line.
- `/* … */` block comments are not stripped: a `panic!` inside one counts, and a `#[cfg(test)]`
  inside one starts an exemption.
- Only `#[cfg(test)]` is recognised. `#[cfg(all(test, …))]`, `#[cfg(any(test, …))]` and an inner
  `#![cfg(test)]` are not, so a panic under them counts as production code.

A Write is blocked because the whole file is known. An Edit only warns, because the snippet may sit
inside an existing test item. A full-file Write of a file that already has a production `panic!()`
is blocked until that panic is removed or explicitly allowed.

Run the tests with `bash .claude/hooks/tests/test_panic_guard.sh [hook path]`. They use synthetic
stdin and temporary paths only.
