# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working in this repository.

## What This Directory Is

`ops/` is the orchestration plane for ICN development. It lives inside the ICN monorepo and
coordinates work across all subdirectories and worktrees. It is **not** substrate code — it's
the operational layer that observes, coordinates, and automates.

## Directory Map

| Directory | What lives there |
|-----------|----------------|
| `mcp/` | TypeScript MCP server — build with `npm run build`, runs via stdio transport |
| `automation/skills/` | Cross-repo skill definitions (SKILL.md format) |
| `automation/hooks/` | Shared hook scripts used by root `.claude/settings.json` |
| `automation/scripts/` | Orchestration shell scripts |
| `ci/workflows/` | Reusable GitHub Actions workflow fragments |
| `monitoring/dashboards/` | Grafana JSON dashboards |
| `monitoring/alerts/` | Prometheus alert rules YAML |
| `monitoring/runbooks/` | Operational runbooks (Markdown) |
| `state/decisions/` | Architecture Decision Records (ADRs) — numbered, use `template.md` |
| `state/sprint/` | Sprint plans: `current.json` is the active sprint |
| `state/config/` | `repo-map.json` (repo locations/relationships), `conventions.md` |
| `docs/plans/` | Design documents (dated: `YYYY-MM-DD-<topic>-design.md`) |

## MCP Server Commands

```bash
cd mcp && npm install          # Install dependencies
cd mcp && npm run build        # Compile TypeScript → dist/
cd mcp && npm run dev          # Watch mode for development
cd mcp && npm test             # Run tests
```

The MCP server uses **stdio transport** — Claude Code starts and manages its lifecycle via the `mcpServers` config in the root `.claude/settings.json`. The SQLite DB lives at `mcp/data/icn-ops.db` (gitignored, auto-created on first run).

## State Management Rules

### Ephemeral state (MCP server / SQLite)
- Sessions, file claims, health cache, task claims
- Safe to delete `mcp/data/icn-ops.db` — it rebuilds from scratch
- Do not commit `mcp/data/` to git

### Durable state (git-tracked files)
- ADRs in `state/decisions/` — never delete, use `superseded` status
- Sprint state in `state/sprint/current.json` — update in place, archive to `history/` on close
- Repo map in `state/config/repo-map.json` — keep in sync with actual repo locations

## Writing ADRs

Copy `state/decisions/template.md`, increment the number from the last ADR in the directory.
Use the `log_decision` MCP tool if available — it writes the file and updates the SQLite index atomically.

Format: `NNNN-kebab-case-title.md`

## What Stays Out of ops/

- Rust/Cargo code → `icn/` (Cargo workspace root)
- Website content or Astro components → `website/`
- K8s deployment manifests → `deploy/k8s/`
- Homelab infrastructure mutations → external `homelab-inventory` repo

## Monorepo Layout

See `state/config/repo-map.json` for the authoritative map. Key relationships:
- `docs/` at repo root is the canonical documentation source
- `website/` reads docs directly via path.resolve (no sync script)
- `icn/` is the Cargo workspace root (not repo root)
- Worktrees live in `../icn-wt/` on the dev VM
- K3s cluster: control at `10.8.10.40`, workers at `.41`/`.42`
