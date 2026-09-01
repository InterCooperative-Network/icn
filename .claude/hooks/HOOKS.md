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

## Supported hook-command language (the checker's contract)

`scripts/check-agent-runtime-adoption.py` decides one question about each command in
`.claude/settings.json`: **does this run a repository file that must therefore exist and be
executable?** This is the exact domain it claims to decide correctly. A command inside this
domain that it classifies wrongly is a defect; a shell program outside it is not a classifier
defect, it is an unsupported program.

The checker does not own Bash and must never grow toward owning it.

### Supported forms

| form | proved |
|---|---|
| `"$CLAUDE_PROJECT_DIR"/<repo path>` | the file exists and is executable |
| `python3 "$CLAUDE_PROJECT_DIR"/<repo path>.py` | the script exists and is readable; the executable bit is **not** required, because it is `argv[1]` |
| `echo <text>` | nothing — an informational command that runs no repository file |
| `VAR=value` prefixes before a **repository path** | the same, and the assignment values are not analysed |

`VAR=value` before the **interpreter** form is deliberately NOT supported. A bare `python3` is
resolved through `PATH`, and an assignment can *be* the `PATH` — `PATH=/tmp:$PATH python3 <hook>`
runs whatever `/tmp/python3` happens to be. A name-based exemption requires that nothing
command-local could have changed what the name resolves to. A repository path is not looked up,
so an assignment in front of one is harmless.

### Path-bearing operands

Only two operands ever become a path this gate must prove: **`argv0`**, and a supported
interpreter's **script operand**. Path rules apply at those positions and nowhere else. An
expansion in a data argument is the shell's business.

Within a path-bearing operand, the project-directory expansion must appear **once**, **start
the word**, be **double-quoted**, and be **followed by `/`**. Each of those was a real defect:
unquoted expands are word-split on a checkout path containing spaces; a token not starting the
word is concatenated; a missing separator silently names a different file; a repeated token
produces a doubled absolute path. `..` is not part of the language, because `Path.resolve()`
collapses a traversal through a component that does not exist while bash cannot traverse it.

### Categorically unsupported — UNCLASSIFIED, which fails the gate

Command substitution `$(...)` and backticks; top-level composition (`;`, `&&`, `||`, `|`,
redirection, grouping); launchers and absolute external programs; interpreters other than a
bare `python3`/`python` with a repository `.py` operand; unresolved expansions; non-shell
whitespace such as `\r`; and anything else not listed as supported.

**Unsupported is not "probably fine".** It fails the gate, because the alternative is a
blocklist, and a substitution can execute a repository hook without naming one
(`echo "$(find . -name hook-health.sh -exec {} \;)"`). That case is why substitutions left the
language rather than acquiring an exception list.

### Out of contract

Deciding what an unsupported shell program *does*; reproducing Bash's parser; analysing
assignment values, data arguments, or the inside of a repository executable. Complex shell
belongs **behind** a repository-owned executable, where the gate proves the thing it can
actually prove: that `argv0` exists and is executable.

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
