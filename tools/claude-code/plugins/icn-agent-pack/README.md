# icn-agent-pack

A portable, **plugin-ready** bundle of ICN's Claude Code agent tooling. It packages the
highest-value skills, specialist subagents, an advisory hook layer, and the canonical `icn-ops`
MCP launch into a single self-contained plugin directory so the tooling can travel with any
checkout — without changing how the repository's project-local `.claude/` config behaves today.

This is a **first packaging pass**, not a migration. It is **not** production-ready, and it does
**not** remove or replace any existing project-local configuration.

> ICN is a coordination substrate / digital public infrastructure / constraint engine — not a
> blockchain, token system, payment system, or retail crypto network. The skills and agents here
> keep that vocabulary disciplined.

### Scope and non-claims

- This plugin does **not** replace the project-local `.claude/` config (you opt in per session with `--plugin-dir`).
- It is **not** production-readiness evidence.
- It does **not** alter ICN's claims, deployment posture, or live/pilot state — it is developer tooling only.

## What it bundles

```
tools/claude-code/plugins/icn-agent-pack/
├── .claude-plugin/plugin.json   # manifest (name, version, author, repo)
├── .mcp.json                    # icn-ops MCP server (portable wrapper launch)
├── .lsp.json                    # documented placeholder (see "LSP" below)
├── README.md
├── skills/
│   ├── preflight/               # session env + canonical-doc preflight
│   ├── truth-sync/              # classify public/docs/API claims vs. truth
│   ├── authority-spine/         # plan entity-aware auth / coop_id<->EntityId / treasury work
│   ├── route-impact/            # downstream impact of a route/API change
│   ├── navigator/               # repo knowledge graph / source-to-claim / impact maps
│   └── doctor/                  # diagnose plugin load + ICN root + MCP reachability
├── agents/
│   ├── icn-architect.md         # crate boundaries, kernel/app, threat modeling
│   ├── icn-economist.md         # ledger, CCL, mutual credit invariants, terminology
│   ├── icn-code-reviewer.md     # invariants-lens PR review (read-only)
│   ├── icn-ops.md               # CI/demo/release readiness (read-mostly, MCP-first)
│   ├── icn-docs-truth-auditor.md# docs/public-claim proof levels + stale-state (read-only)
│   └── icn-navigator.md         # knowledge graph / impact maps (read-mostly)
├── hooks/hooks.json             # advisory only: SessionStart context + PostToolUse hints
└── bin/
    ├── icn-find-root            # resolve the ICN repo root (ICN_ROOT / git / walk-up / dev layouts)
    ├── icn-ops-mcp              # portable icn-ops MCP launcher (resolves root, then runs npm)
    ├── icn-session-context      # SessionStart context pointer
    ├── icn-route-impact-hint    # PostToolUse route/API hint
    └── icn-truth-sync-hint      # PostToolUse public-claim hint
```

User-invocable skills are namespaced under the plugin, e.g. `/icn-agent-pack:preflight`. Every skill
sets `disable-model-invocation: true`, so the model never auto-runs them — they fire only when you
explicitly invoke them.

## How this plugin is loaded

**Development / test loading (current).** Load the plugin directly from the repo — no marketplace
install, no change to project config:

```bash
claude --plugin-dir ./tools/claude-code/plugins/icn-agent-pack   # from the repo root
/reload-plugins                                                   # after editing plugin files
/icn-agent-pack:doctor                                            # diagnose plugin + root + MCP
/icn-agent-pack:preflight                                         # session preflight
/icn-agent-pack:route-impact icn/crates/icn-gateway/src/routes/governance.rs
```

**Launching from a higher-level / parent directory.** The plugin's `icn-ops` MCP launch is portable
(see "Portable MCP launch" below), so it also works when Claude Code is started above the repo. If root
discovery is ambiguous (e.g. several ICN worktrees under one parent), set `ICN_ROOT` explicitly:

```bash
cd ..
ICN_ROOT=/path/to/icn claude --plugin-dir /path/to/icn/tools/claude-code/plugins/icn-agent-pack
```

**Not yet automatic.** The plugin is **not** installed as a project-scoped plugin and is **not**
auto-discovered. It does not replace the existing project-local `.claude/` config — you opt in with
`--plugin-dir` per session.

**Future option.** Later we may mirror or move the pack into `.claude/skills/icn-agent-pack/` (or wire
it through a marketplace) for project-scoped auto-discovery after workspace trust. This first pass is
deliberately plugin-*packaging* infrastructure, not a cutover.

## Validate

```bash
python3 scripts/check-claude-plugin.py                              # structure + wrapper-launch + bin syntax
python3 scripts/check-claude-plugin-root-resolution.py              # icn-find-root branch tests
claude plugin validate ./tools/claude-code/plugins/icn-agent-pack   # Anthropic's official validator
```

## Interactive smoke test (manual acceptance)

The checks above are static. `claude plugin validate` proves the plugin's **schema and loadability**,
but it does **not** prove that an interactive session can actually start `icn-ops` through the plugin
wrapper. The manual acceptance test is `/icn-agent-pack:doctor` in a live session.

Launch from a higher-level directory (e.g. one level above the repo). Set `ICN_ROOT` so root discovery
is unambiguous when several ICN worktrees share a parent:

```bash
cd ~/icn-dev
ICN_ROOT=/home/ubuntu/icn-dev/worktrees/icn/task-claude-agent-pack \
  claude --plugin-dir /home/ubuntu/icn-dev/worktrees/icn/task-claude-agent-pack/tools/claude-code/plugins/icn-agent-pack
```

Then, inside Claude Code:

```text
/reload-plugins            # pick up the plugin
/icn-agent-pack:doctor      # plugin loaded? root resolved? icn-ops MCP reachable?
/icn-agent-pack:preflight   # session env preflight
```

Acceptance checklist:

- [ ] `/icn-agent-pack:doctor` reports the plugin is loaded and the ICN root resolves.
- [ ] `doctor` finds `ops/mcp/package.json` and reports `node`/`npm` present (and warns if `node_modules` is absent — run `npm ci` in `ops/mcp` first).
- [ ] `icn-ops` MCP tools are reachable in the session (e.g. the navigator/ops skills can call `icn_ops_*`).
- [ ] `/icn-agent-pack:preflight` runs its read-only checks.
- [ ] Root `.mcp.json` / `.cursor/mcp.json` are unchanged (the plugin uses its own wrapper-based `.mcp.json`).

Notes: when launching from a parent/higher-level directory, set `ICN_ROOT` if discovery is ambiguous.
Root `.mcp.json` and `.cursor/mcp.json` stay repo-root-relative for normal repo-root sessions; the
plugin's `.mcp.json` is wrapper-based and portable.

## Portable MCP launch

Root `.mcp.json` and `.cursor/mcp.json` remain **canonical and repo-root-relative** for normal
repo-root sessions (`npm --prefix ./ops/mcp run start:stdio`, enforced by
`scripts/check-mcp-portability.py`). This pass does **not** change them.

The *plugin's* `.mcp.json` is **wrapper-based and portable**:

```json
{ "mcpServers": { "icn-ops": { "command": "${CLAUDE_PLUGIN_ROOT}/bin/icn-ops-mcp" } } }
```

`bin/icn-ops-mcp` resolves the ICN repo root (via `bin/icn-find-root`), exports `ICN_ROOT`, then execs
`npm --prefix "$ICN_ROOT/ops/mcp" run start:stdio` — so startup no longer depends on Claude Code's
launch directory, and it never points at `ops/mcp/dist/index.js`.

`bin/icn-find-root` resolves the root in this order, accepting only a directory that contains both
`ops/mcp/package.json` and `docs/STATE.md`:

1. **`ICN_ROOT`** if set (decisive — must be valid, else it fails clearly).
2. **git toplevel** from the current dir and `$CLAUDE_PROJECT_DIR`.
3. **upward directory walk** from the current dir and `$CLAUDE_PROJECT_DIR`.
4. **a small set of common ICN dev layouts** (e.g. `./icn`, `./worktrees/icn/*`,
   `$HOME/icn-dev/worktrees/icn/*`, `$HOME/icn-dev/icn`).

If nothing is found, or multiple distinct roots are found (ambiguous), it fails with a clear message
and asks you to set `ICN_ROOT=/path/to/icn`. Run `/icn-agent-pack:doctor` to diagnose resolution and
MCP reachability.

## Relationship to the existing `.claude/` config

The repository's project-local config under `.claude/` (`settings.json`, `hooks/*`, `agents/*`,
`skills/*`) remains the **active, canonical** configuration for this repo. This plugin is an
*additional* packaging path, not a replacement.

- **This pass removes nothing.** `.claude/skills/icn-preflight/`, the project agents, and the
  blocking safety hooks (firewall / panic / scope / dep / todo guards) are untouched and keep
  working exactly as before.
- **No name collisions.** Plugin skills are namespaced (`/icn-agent-pack:preflight`), so they
  coexist with the project-local `/icn-preflight`.
- **Safety hooks are not duplicated or weakened.** This plugin's hooks are advisory and
  non-blocking; the repo's blocking guards in `.claude/` are the enforcement layer.
- **Root MCP configs are untouched and canonical.** The repo's `.mcp.json` and `.cursor/mcp.json`
  keep the canonical repo-root-relative launch (`npm --prefix ./ops/mcp run start:stdio`, enforced by
  `scripts/check-mcp-portability.py`). The plugin's own `.mcp.json` instead uses the portable
  `${CLAUDE_PLUGIN_ROOT}/bin/icn-ops-mcp` wrapper so it works from any launch directory — see
  "Portable MCP launch". The plugin does not modify the root configs.

If the project later decides to consolidate on the plugin, see
`docs/guides/developer/claude-code-plugin.md` for the migration considerations.

## Safety model

- **Advisory by default.** The plugin's hooks only print hints and a context pointer. They never
  block a tool call and never mutate source. Enforcement stays with the project-local `.claude/`
  guards.
- **Hooks read stdin JSON.** Helper scripts parse the Claude Code hook input from stdin (preferring
  `jq`, with a POSIX fallback) rather than trusting environment variables. They make no network
  calls, perform no source mutation, and are safe to run outside the repo.
- **Read-mostly agents.** Review/audit agents (`icn-code-reviewer`, `icn-docs-truth-auditor`) carry
  an explicit read-only `tools` allow-list (`Read`, `Grep`, `Glob`, `Bash`). `icn-ops` and
  `icn-navigator` keep broad tool access **specifically** so they can reach the `icn-ops` MCP tools
  (a narrow allow-list would silently exclude MCP), and are constrained to read-mostly behavior by
  their prompts. None of the agents may assert live cluster/runtime state from static prompt text —
  they must consult the `icn-ops` MCP server or live evidence.
- **No autonomous source mutation.** Nothing in this plugin edits code automatically.

## LSP

`.lsp.json` ships as an intentional empty placeholder (`{}` — a no-op). This first pass configures no
language servers, and the plugin never installs server binaries (populating it is deferred — out of
scope here).

When populated later, a standalone `.lsp.json` is a direct map of *server name → config*, where each
entry requires `command` and `extensionToLanguage` (per the
[plugins reference](https://code.claude.com/docs/en/plugins-reference)). The validator accepts either
`{}` or this shape:

```json
{
  "rust":       { "command": "rust-analyzer", "extensionToLanguage": { ".rs": "rust" } },
  "typescript": { "command": "typescript-language-server", "args": ["--stdio"],
                  "extensionToLanguage": { ".ts": "typescript", ".tsx": "typescriptreact" } },
  "python":     { "command": "pyright-langserver", "args": ["--stdio"],
                  "extensionToLanguage": { ".py": "python" } }
}
```

These servers are conventional and **non-installing** — the binaries must already be present on the
user's machine:

| Server | Install (user-provided) |
|--------|--------------------------|
| `rust-analyzer` | `rustup component add rust-analyzer` |
| `typescript-language-server` | `npm i -g typescript-language-server typescript` |
| `pyright` | `npm i -g pyright` |

## Known limitations

- First pass: skills are concise starting points; expect iteration.
- `.lsp.json` is a placeholder (see above); no LSP servers are configured yet.
- `navigator` reads the generated **Agent Context Spine v0**
  (`docs/reference/project-index/generated/agent-context-spine.json`, via the `icn-ops` MCP tool
  `icn_ops_agent_context_spine`) and otherwise produces maps inline. The spine is v0 — it does not
  yet parse the Rust module graph or enumerate per-route nodes. See
  [`docs/guides/developer/agent-context-spine.md`](../../../../docs/guides/developer/agent-context-spine.md).
- Hooks load at session start — edits to `hooks/hooks.json` or `bin/*` require a restart (or
  `/reload-plugins`) to take effect.
- No CI wiring for plugin validation yet (run `scripts/check-claude-plugin.py` and
  `claude plugin validate ...` manually).
- The plugin's advisory hooks fire in addition to the project-local hooks; in a normal repo session
  you get both layers.
