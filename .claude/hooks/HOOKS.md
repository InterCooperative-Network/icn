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

## Why the shell lives in files

A hook command in `settings.json` must be a **single simple command** naming a repository
executable. `scripts/check-agent-runtime-adoption.py` proves that command exists and has its
executable bit, and it deliberately does not interpret shell.

Command substitutions — `$(...)` and backticks — are therefore **not part of the supported
hook-command language**. A substitution can execute a repository hook without naming one
(`echo "$(find . -name hook-health.sh -exec {} \;)"`), and the only alternative was a
blocklist of `-exec`/`xargs`/`sh -c`/`eval`/git aliases, which the sibling registry work spent
nine review rounds establishing does not terminate (icn#2691, icn#2632).

So when a hook needs shell, it goes in a file here and `settings.json` invokes the path.
`report-branch.sh` is the worked example: its body was previously inline.

## Hook Inventory

| File | Trigger | Effect |
|------|---------|--------|
| hook-health.sh | Any tool (once per session) | BLOCKS if jq or git missing; WARNS otherwise |
| session-orient.sh | SessionStart (startup) | Prints branch, sprint cadence (resolved via the registered `sprint_state` owner), skills, invariants. Never blocks |
| firewall-guard.sh | Edit/Write .rs in kernel crates | BLOCKS domain imports in kernel |
| panic-guard.sh | Edit/Write .rs in non-test files | BLOCKS panic!(), WARNS unwrap() |
| report-branch.sh | Edit/Write any file | Prints the current branch. Never blocks |
| scope-guard.sh | Edit/Write any file | WARNS if edit is outside branch scope |
| dep-guard.sh | Edit/Write Cargo.toml | WARNS on direct version pinning in crates |
| todo-guard.sh | Edit/Write .rs/.ts | WARNS on bare TODO without issue number |
| pre-tool-guard.py | Edit/Write protected crates | Checklist for high-risk crates |
| pre-bash-guard.py | Bash commands | WARNS on direct main branch operations |
| openapi-sync-guard.sh | Edit/Write gateway routes | REMINDS to regenerate OpenAPI + TS types |
| post-tool-guard.py | Edit/Write .rs | SUGGESTS cargo check after edit |
