# Agent MCP tooling (icn-ops)

The `ops/mcp` server is a **portable agent operations layer**: one TypeScript stdio MCP that Cursor, Claude Code, Codex-style hosts, and future MCP clients can share. It is not editor-specific glue; it centralizes read-mostly diagnostics, cached cluster/git health, and safe discovery so agents can orient before changing the ICN monorepo.

## Launch (all clients)

From the **repository root**, register `icn-ops` with:

- **Command:** `npm`
- **Args:** `["--prefix", "./ops/mcp", "run", "start:stdio"]`

Repo files:

- `.mcp.json` — Claude-compatible project MCP
- `.cursor/mcp.json` — Cursor project MCP

These must stay **identical** for `icn-ops` (enforced by `python3 scripts/check-mcp-portability.py`). Do not point MCP configs at `ops/mcp/dist/index.js` directly; `start:stdio` runs `tsc` then `node` under one Node.

Install once per checkout (or after changing Node major):

```bash
cd ops/mcp && npm ci
```

Native module note: `better-sqlite3` is rebuilt in `postinstall`. If the MCP host uses a different Node than the one used for `npm ci`, run `npm rebuild better-sqlite3` (or `npm ci` again) under the host’s Node.

## Tools (agent-facing)

| Tool | Purpose |
|------|---------|
| `icn_ops_environment_report` | JSON snapshot: repo root, git branch/commit/dirty, Node ABI, npm/rust/python versions, optional `gh`/`kubectl`, MCP config inspection, `node_modules`/`dist` presence, `better-sqlite3` load probe. Missing optional CLIs are **warnings**, not hard failures. |
| `icn_ops_doctor` | Read-only diagnosis: severity (`ok` / `warn` / `error`), per-check results, suggested **shell repair commands** (not executed). Covers MCP parity script, native module, dirty tree, optional tools, key `ops/state` files, and lightweight **CLI runner probes** (`git` / `npm` / `python3`). |
| `icn_ops_agent_brief` | Compact structured briefing: docs to read first, safe vs forbidden vocabulary, verification commands by area, PR hygiene, completeness warning, MCP troubleshooting bullets. |
| `icn_ops_command_catalog` | **Catalog only** — grouped commands with `working_directory`, `safety` (`read_only` / `modifies_local` / `destructive` / `external_side_effect`), `runtime` hint, and `when_to_use`. Never runs commands. |
| `icn_ops_state_index` | Lists canonical state/architecture paths with `present: true/false` (filesystem stat); does not invent missing files. Optional arg `include_absent` (default true). |

Existing tools (`cluster_health`, sessions, tasks, decisions, etc.) remain available. Poller and health paths use **`execFile`-style argv** (via `runCommand` in `ops/mcp/src/utils/commands.ts`): no shell, bounded stdout/stderr, timeouts, and structured `{ ok, exitCode, stderr, timedOut }` results. `kubectl get pods -o json` is parsed in-process (no `jq` pipeline). External JSON goes through `safeJsonParse` so malformed output becomes `{ error, preview }` instead of throwing through the MCP boundary.

### Safe command execution policy

- **Default:** `runCommand` / `runCommandQuick` / `runCommandJson` — argv only, no `/bin/sh -c`, predictable quoting, output truncation.
- **Why avoid shell pipelines:** they inject quoting bugs, hide exit codes, and make “optional tool missing” look like opaque script failures.
- **Failures:** represented as `ok: false` plus stderr/exit/timedOut; pollers write those into `health_cache` or doctor checks — **never** `process.exit` from optional probes.
- **Warnings vs errors:** `icn_ops_doctor` uses `warn` for optional CLIs and dirty trees; `error` for portability failures, native module load, or missing `node_modules`. Suggested repairs are **strings only** — not executed by MCP.

## Common failure modes

| Symptom | Likely cause | What to do |
|---------|----------------|------------|
| MCP host shows native module / `NODE_MODULE_VERSION` | Node ABI mismatch vs `npm ci` | `cd ops/mcp && npm rebuild better-sqlite3` or reinstall with the host’s Node |
| `icn_ops_doctor` → portability error | `.mcp.json` ≠ `.cursor/mcp.json` | Align args to canonical `npm --prefix ./ops/mcp run start:stdio` |
| Doctor warns on missing `ops/state/...` | Fresh clone or ops state not checked in | Confirm paths; some files are environment-specific |
| `kubectl` / `gh` warnings | Optional tooling absent | Expected on dev laptops; not required for MCP core |

## Validation before edits

1. Call `icn_ops_doctor` (or `icn_ops_environment_report`) after switching branches or Node versions.
2. Follow `icn_ops_agent_brief` + `AGENTS.md` change routing for the area you touch.
3. Use `icn_ops_command_catalog` to pick verification commands; run them in your terminal (MCP does not auto-run them).

## Related docs

- [cursor-mcp-setup.md](./cursor-mcp-setup.md) — Cursor vs Claude wiring and smoke-test commands
- `AGENTS.md` (repo root) — invariants and verification matrix
- `ops/CLAUDE.md` — orchestration plane layout
