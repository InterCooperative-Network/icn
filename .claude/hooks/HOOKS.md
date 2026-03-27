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

## Hook Inventory

| File | Trigger | Effect |
|------|---------|--------|
| firewall-guard.sh | Edit/Write .rs in kernel crates | BLOCKS domain imports in kernel |
| panic-guard.sh | Edit/Write .rs in non-test files | BLOCKS panic!(), WARNS unwrap() |
| scope-guard.sh | Edit/Write any file | WARNS if edit is outside branch scope |
| dep-guard.sh | Edit/Write Cargo.toml | WARNS on direct version pinning in crates |
| todo-guard.sh | Edit/Write .rs/.ts | WARNS on bare TODO without issue number |
| pre-tool-guard.py | Edit/Write protected crates | Checklist for high-risk crates |
| pre-bash-guard.py | Bash commands | WARNS on direct main branch operations |
| openapi-sync-guard.sh | Edit/Write gateway routes | REMINDS to regenerate OpenAPI + TS types |
| post-tool-guard.py | Edit/Write .rs | SUGGESTS cargo check after edit |
