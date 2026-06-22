# Claude Code plugin: icn-agent-pack

This guide explains the `icn-agent-pack` Claude Code plugin: what it is, how to test it, how it
relates to the repository's project-local `.claude/` configuration, and what would be involved in
migrating to it later.

> **Status:** first packaging pass, not production-ready. This pass **adds** a plugin; it removes
> nothing. The project-local `.claude/` config remains the active, canonical configuration.

## Two configuration paths (and why both exist)

| Path | Location | Role today |
|------|----------|------------|
| **Project-local config** | `.claude/` (`settings.json`, `hooks/*`, `agents/*`, `skills/*`), `.mcp.json`, `.cursor/mcp.json` | **Canonical and active.** This is what runs in a normal repo session — including the blocking safety hooks. |
| **Plugin packaging** | `tools/claude-code/plugins/icn-agent-pack/` | **New, additive.** A self-contained, portable bundle of the highest-value skills/agents/hooks plus the `icn-ops` MCP launch. |

The plugin path exists so the tooling can be loaded from any checkout (or eventually shared/installed)
without being entangled with one machine's setup. It is intentionally a *superset-friendly* second
path, not a replacement.

## What the plugin contains

- **Skills** (`skills/`, all `disable-model-invocation: true`, user-invoked via `/icn-agent-pack:<name>`):
  `preflight`, `truth-sync`, `authority-spine`, `route-impact`, `navigator`, and `doctor`
  (plugin/root/MCP self-diagnosis).
- **Agents** (`agents/`): `icn-architect`, `icn-economist`, `icn-code-reviewer`, `icn-ops`
  (de-staled, MCP-first), plus new `icn-docs-truth-auditor` and `icn-navigator`.
- **Hooks** (`hooks/hooks.json`): advisory only — a SessionStart context pointer and PostToolUse
  route/claim hints. Non-blocking; they do **not** duplicate or weaken the project-local safety hooks.
- **MCP** (`.mcp.json`): the `icn-ops` server launched through the portable wrapper
  `${CLAUDE_PLUGIN_ROOT}/bin/icn-ops-mcp` (see "Portable MCP launch" below) — **not** identical to the
  root configs, which stay repo-root-relative.
- **LSP** (`.lsp.json`): a documented `{}` placeholder; see the plugin README for the intended,
  non-installing Rust/TS/Python servers.
- **bin/**: the MCP launcher (`icn-ops-mcp`) and root resolver (`icn-find-root`) in Python, plus the
  POSIX-sh hook helpers (`icn-session-context`, `icn-route-impact-hint`, `icn-truth-sync-hint`) that
  read hook input JSON from stdin. None make network calls or mutate anything.

See `tools/claude-code/plugins/icn-agent-pack/README.md` for the full inventory and safety model.

## How this plugin is loaded

### Development / test loading (current)

Load the plugin directly from the repo — no marketplace install, no change to project config:

```bash
claude --plugin-dir ./tools/claude-code/plugins/icn-agent-pack   # from the repo root
```

Inside the session:

```text
/reload-plugins                 # after editing plugin files (hooks need a reload/restart)
/icn-agent-pack:doctor           # diagnose plugin load + ICN root + MCP reachability
/icn-agent-pack:preflight        # session preflight
/icn-agent-pack:route-impact icn/crates/icn-gateway/src/routes/governance.rs
```

### Launching from a higher-level / parent directory

The `icn-ops` MCP launch is portable (see "Portable MCP launch" below), so it also works when Claude
Code is started above the repo. If root discovery is ambiguous (e.g. several ICN worktrees under one
parent), set `ICN_ROOT` explicitly:

```bash
cd ..
ICN_ROOT=/path/to/icn claude --plugin-dir /path/to/icn/tools/claude-code/plugins/icn-agent-pack
```

### Not yet automatic

The plugin is **not** installed as a project-scoped plugin and is **not** auto-discovered. It does not
replace the project-local `.claude/` config; you opt in per session with `--plugin-dir`. This first
pass is plugin-*packaging* infrastructure, not a cutover.

### Future option

Later we may mirror or move the pack into `.claude/skills/icn-agent-pack/` (or publish it via a
marketplace) for project-scoped auto-discovery after workspace trust. That is a deliberate, separate
decision — see "Migrating later".

## Portable MCP launch

Root `.mcp.json` and `.cursor/mcp.json` stay **canonical and repo-root-relative** for normal repo-root
sessions (`npm --prefix ./ops/mcp run start:stdio`, enforced by `scripts/check-mcp-portability.py`).
This pass does **not** change them.

The plugin's own `.mcp.json` is **wrapper-based and portable**:

```json
{ "mcpServers": { "icn-ops": { "command": "${CLAUDE_PLUGIN_ROOT}/bin/icn-ops-mcp" } } }
```

`bin/icn-ops-mcp` resolves the ICN root via `bin/icn-find-root`, exports `ICN_ROOT`, then execs
`npm --prefix "$ICN_ROOT/ops/mcp" run start:stdio` (never `ops/mcp/dist/index.js`), so MCP startup no
longer depends on Claude Code's launch directory. `bin/icn-find-root` resolves the root, in order, by
`ICN_ROOT` → git toplevel → upward walk → a small set of common dev layouts, accepting only a directory
containing both `ops/mcp/package.json` and `docs/STATE.md`; a not-found or multiple-distinct-roots
situation fails clearly and asks you to set `ICN_ROOT`. Diagnose with `/icn-agent-pack:doctor`.

## Validate

```bash
python3 scripts/check-claude-plugin.py                              # structure + wrapper-launch pre-check
python3 scripts/check-claude-plugin-root-resolution.py              # icn-find-root branch tests
claude plugin validate ./tools/claude-code/plugins/icn-agent-pack   # Anthropic's official validator
```

`scripts/check-claude-plugin.py` (standard library only) verifies: the manifest parses and is named
`icn-agent-pack`; `.claude-plugin/` contains only `plugin.json`; the `skills/`, `agents/`, `hooks/`,
and `bin/` directories exist; `bin/icn-find-root` and `bin/icn-ops-mcp` exist and are executable; all
required skills (incl. `doctor`) and agents exist; every skill has a `description` and the heavy
workflows (and any user-invocable skill) set `disable-model-invocation: true`; every agent has `name` +
`description`; `hooks.json` has a top-level `hooks` object and no stray top-level `description`; hook
commands quote `${CLAUDE_PLUGIN_ROOT}` and point at existing, executable scripts; the plugin `.mcp.json`
launches `icn-ops` via the `${CLAUDE_PLUGIN_ROOT}/bin/icn-ops-mcp` wrapper and contains none of
`./ops/mcp`, `ops/mcp/dist/index.js`, or `node dist/index.js`; `.lsp.json` is `{}` or a valid LSP server
map; the README and this guide exist; and every file in `bin/` passes a shebang-aware syntax check
(builtin `compile()` for Python — no `.pyc` cache written — and `sh -n` for shell), iterating files
only so directories like `__pycache__` are ignored. `check-claude-plugin-root-resolution.py` unit-tests
the resolver branches (repo root, subdir, parent + `ICN_ROOT`, spaces, invalid `ICN_ROOT`, ambiguous,
higher-level unique `./icn` layout, git, no-root) against temp fixtures. Both are fast pre-checks that
complement `claude plugin validate`.

## Interactive smoke test (manual acceptance)

The checks above are static. `claude plugin validate` proves the plugin's **schema and loadability**,
but it does **not** prove an interactive session can actually start `icn-ops` through the plugin
wrapper. The manual acceptance test is `/icn-agent-pack:doctor` in a live session.

Launch from a higher-level directory, setting `ICN_ROOT` so discovery is unambiguous when several ICN
worktrees share a parent:

```bash
cd ~/icn-dev
ICN_ROOT=/home/ubuntu/icn-dev/worktrees/icn/task-claude-agent-pack \
  claude --plugin-dir /home/ubuntu/icn-dev/worktrees/icn/task-claude-agent-pack/tools/claude-code/plugins/icn-agent-pack
```

Inside Claude Code:

```text
/reload-plugins            # pick up the plugin
/icn-agent-pack:doctor      # plugin loaded? root resolved? icn-ops MCP reachable?
/icn-agent-pack:preflight   # session env preflight
```

- `/icn-agent-pack:doctor` is the manual smoke test for plugin load + ICN root resolution + MCP reachability.
- When launching from a parent/higher-level directory, set `ICN_ROOT` if discovery is ambiguous.
- Root `.mcp.json` and `.cursor/mcp.json` stay repo-root-relative for normal repo-root sessions; the
  plugin's `.mcp.json` is wrapper-based and portable.

## What must stay canonical in project-local config (for now)

Until a deliberate migration, treat these as authoritative in `.claude/` — the plugin does not own them:

- **Blocking safety hooks** — `firewall-guard.sh` (meaning firewall), `panic-guard.sh`,
  `scope-guard.sh`, `dep-guard.sh`, `todo-guard.sh`, and the `pre-*`/`post-*` guards in
  `.claude/hooks/`. The plugin's hooks are advisory only and do not replace these.
- **Permissions** — the `allow`/`deny`/`ask` lists in `.claude/settings.json`.
- **The project-local `/icn-preflight` skill** and the existing `.claude/agents/*` set.
- **Root MCP configs** — `.mcp.json` and `.cursor/mcp.json` (kept identical, enforced by
  `scripts/check-mcp-portability.py`).

The plugin's namespaced skills (`/icn-agent-pack:*`) coexist with the project-local ones without
collision.

## Migrating later (if we choose)

This pass deliberately stops short of migration. If the project later decides to consolidate on the
plugin, the considerations are:

1. **Decide ownership per component.** For each skill/agent/hook, decide whether the plugin or
   `.claude/` is the single source. Avoid maintaining two copies that drift.
2. **Port the blocking hooks carefully.** The safety guards are blocking and depend on
   `$CLAUDE_PROJECT_DIR`. Moving them into a plugin means re-validating they still block (and still
   read stdin JSON) under `${CLAUDE_PLUGIN_ROOT}`. Do this behind a test, one hook at a time.
3. **Keep the MCP launch identical** across whichever configs survive; keep the portability check green.
4. **Add CI** for `scripts/check-claude-plugin.py` so the plugin can't rot.
5. **Only then** remove the duplicated project-local copies — never before the plugin equivalents are
   proven in real sessions.

Follow-ups tracked for later phases: whether to move project-local config into the plugin; whether to
populate `.lsp.json`; whether to add monitors; whether to expose MCP resources for the truth maps and
the repo graph; whether to add CI for plugin validation; and whether to build `repo_knowledge_graph.py`.

## Related docs

- `docs/guides/developer/agent-mcp-tooling.md` — the `icn-ops` MCP server and its launch doctrine.
- `docs/guides/developer/agent-context-spine.md` — the Agent Context Spine v0 that the `navigator`
  skill/agent read (generated, non-canonical, evidence-grounded; exposed via the
  `icn_ops_agent_context_spine` MCP tool).
- `tools/claude-code/plugins/icn-agent-pack/README.md` — plugin inventory, safety model, and LSP notes.
