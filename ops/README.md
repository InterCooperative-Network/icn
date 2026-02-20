# icn-ops

The orchestration plane for ICN development. This repo is the "nervous system" that coordinates work across all ICN repositories.

## What lives here

| Directory | Purpose |
|-----------|---------|
| `mcp/` | TypeScript MCP server — live state coordination for Claude Code sessions |
| `automation/` | Cross-repo skills, hook scripts, orchestration shell scripts |
| `ci/` | Shared GitHub Actions workflows and CI templates |
| `monitoring/` | Grafana dashboards, Prometheus alert rules, operational runbooks |
| `state/` | Durable git-tracked state: ADRs, sprint plans, cross-repo config |
| `docs/plans/` | Design documents |

## Quick Start

### Start the MCP server (for Claude Code orchestration)

```bash
cd mcp && npm install && npm run build
# Claude Code picks it up via the root .claude/settings.json MCP registration
```

### Check current sprint state

```bash
cat state/sprint/current.json
```

### View repo relationships

```bash
cat state/config/repo-map.json
```

## Architecture

icn-ops uses a **hybrid state model**:
- **MCP server (ephemeral state)**: Active agent sessions, advisory file locks, cached infrastructure health, task claims. Stored in SQLite (WAL mode, auto-resets on restart without data loss).
- **Git-tracked files (durable state)**: Architecture decision records, sprint history, cross-repo configuration. Version-controlled and auditable.

See [`docs/plans/2026-02-19-icn-ops-design.md`](docs/plans/2026-02-19-icn-ops-design.md) for the full design, and [`state/decisions/0001-orchestration-plane-architecture.md`](state/decisions/0001-orchestration-plane-architecture.md) for the ADR.

## Repos This Coordinates

- **[icn](https://github.com/InterCooperative-Network/icn)** — Main ICN daemon (Rust, 39 crates)
- **[icn-website](https://github.com/InterCooperative-Network/icn-website)** — Public website (Astro 5)
- **icn-wt/** — Git worktrees for parallel feature development (lives at `../icn-wt/` on dev VM)
- **[homelab-inventory](https://github.com/fahertym/homelab-inventory)** — Infrastructure (read-only observation)
