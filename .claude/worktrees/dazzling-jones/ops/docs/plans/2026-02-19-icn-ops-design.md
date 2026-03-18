# ICN-Ops Orchestration Plane — Design Document

**Date**: 2026-02-19
**Status**: Approved
**ADR**: [0001-orchestration-plane-architecture](../../state/decisions/0001-orchestration-plane-architecture.md)

## Overview

`icn-ops` is the orchestration plane for ICN development — a new repo that serves as the "nervous system" across all ICN repositories (`icn/`, `icn-website/`, `icn-wt/`). It coordinates multi-agent development, maintains persistent state across Claude Code sessions, and centralizes operational tooling.

### Design Principles

- **Hybrid state model**: MCP server owns live/ephemeral state (sessions, locks, health); git-tracked files own durable state (decisions, sprints, config)
- **Design for swarm**: Concurrent-safe by default — supports 4+ parallel agent sessions on a single VM
- **Each repo stays focused**: `icn/` = substrate code, `icn-website/` = content + presentation, `icn-ops/` = operations + coordination
- **Every phase independently useful**: No "wait for everything" gates

## Architecture

### Approach: Hybrid — MCP for Live State, Git for Durable State

```
┌──────────────────────────────────────────────────────┐
│  Claude Sessions (1..N)                              │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐              │
│  │Agent1│ │Agent2│ │Agent3│ │Agent4│              │
│  └──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘              │
│     └────────┴────────┴────────┘                   │
│          │ MCP              │ File I/O             │
│  ┌───────▼────────┐  ┌─────▼──────────────┐       │
│  │ MCP Server     │  │ icn-ops/ (git)     │       │
│  │ (Live State)   │  │ (Durable State)    │       │
│  │                │  │                    │       │
│  │ • Sessions     │  │ • decisions/       │       │
│  │ • Locks        │  │ • plans/           │       │
│  │ • Health cache │  │ • sprint/          │       │
│  │ • Task claims  │  │ • config/          │       │
│  │ • Build status │  │ • runbooks/        │       │
│  └───────┬────────┘  └────────────────────┘       │
│          │ polls                                   │
│  ┌───────▼────────────────────────────┐           │
│  │ Infrastructure                      │           │
│  │ K3s │ Docker │ Git │ Cargo        │           │
│  └─────────────────────────────────────┘           │
└──────────────────────────────────────────────────────┘
```

### Why Hybrid

Ephemeral and durable state have fundamentally different requirements:
- **Ephemeral state** needs atomic operations, locks, and real-time polling → MCP server with SQLite
- **Durable state** needs version history, auditability, and human readability → Git-tracked files

Building everything into the MCP server loses git history on decisions. Building everything into files means fighting file locking for concurrent agents.

## Section 1: Repository Structure

```
icn-ops/
├── mcp/                        # TypeScript MCP server
│   ├── src/
│   │   ├── index.ts            # Server entry point
│   │   ├── tools/              # MCP tool handlers
│   │   │   ├── sessions.ts     # Session registry + locks
│   │   │   ├── tasks.ts        # Task claims + status
│   │   │   ├── health.ts       # Infra health polling
│   │   │   ├── repos.ts        # Repo/branch/worktree status
│   │   │   └── decisions.ts    # Decision log tools
│   │   ├── state/              # State management
│   │   │   ├── db.ts           # SQLite connection + migrations
│   │   │   └── schemas.ts      # Zod schemas for validation
│   │   └── polling/            # Background health checks
│   │       ├── cluster.ts      # K3s pod/service health
│   │       ├── builds.ts       # sccache stats
│   │       └── git.ts          # Branch/PR/CI status
│   ├── package.json
│   ├── tsconfig.json
│   └── data/                   # SQLite DB (gitignored)
├── automation/                 # Cross-repo scripts, skills, hooks
│   ├── skills/                 # Shared skill definitions
│   ├── hooks/                  # Shared hook scripts
│   └── scripts/                # Orchestration shell scripts
├── ci/                         # Shared CI/CD
│   ├── workflows/              # Reusable GitHub Actions workflows
│   ├── templates/              # Job snippets, gate-ratchet helpers
│   └── runner/                 # Self-hosted runner config
├── monitoring/                 # Observability
│   ├── dashboards/             # Grafana dashboard JSON
│   ├── alerts/                 # Prometheus alert rules
│   └── runbooks/               # Operational runbooks
├── state/                      # Durable git-tracked state
│   ├── decisions/              # Architecture decision records (ADRs)
│   ├── sprint/                 # Sprint plans + retrospectives
│   └── config/                 # Cross-repo conventions + policy
├── docs/
│   └── plans/                  # Design documents
├── CLAUDE.md
└── README.md
```

## Section 2: MCP Server — Tools and State Model

### Tool Categories

#### Session Management — who's working on what, prevent collisions

| Tool | Purpose | Example |
|------|---------|---------|
| `register_session` | Agent checks in with repo, worktree, task | `{repo: "icn", worktree: "1084-names-gateway-a", task: "implementing naming service"}` |
| `list_sessions` | See all active agents and what they're touching | Returns active sessions with file-level claims |
| `claim_files` | Advisory lock on files to prevent concurrent edits | `{files: ["crates/icn-naming/src/lib.rs"], session_id: "..."}` |
| `release_session` | Agent signs off, releases all claims | Auto-releases on timeout too |

#### Task Board — cross-session work coordination

| Tool | Purpose |
|------|---------|
| `get_tasks` | List tasks with status, assignee, blockers (reads from `state/sprint/`) |
| `claim_task` | Atomically assign a task to the calling session |
| `update_task` | Mark progress, add notes, flag blockers |
| `create_task` | Agent discovers new work needed |

#### Repository Status — live view of all repos

| Tool | Purpose |
|------|---------|
| `repo_status` | Branch, dirty files, ahead/behind for all repos |
| `worktree_status` | All worktrees with branch, last commit, staleness |
| `ci_status` | Latest CI run results per branch (polls GitHub API) |
| `pr_status` | Open PRs with review status |

#### Infrastructure Health — what's deployed and running

| Tool | Purpose |
|------|---------|
| `cluster_health` | K3s pod status, resource usage, service endpoints |
| `build_cache_status` | sccache hit rates, disk usage |
| `service_endpoints` | Gateway, Pilot UI, Grafana, Prometheus URLs and reachability |

#### Decision Log — durable decisions with live indexing

| Tool | Purpose |
|------|---------|
| `log_decision` | Write an ADR to `state/decisions/`, index in SQLite for search |
| `search_decisions` | Full-text search over past decisions |
| `get_decision` | Retrieve a specific ADR by ID or topic |

### SQLite Schema (Ephemeral State)

```sql
-- Active agent sessions
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    repo TEXT NOT NULL,
    worktree TEXT,
    task_description TEXT,
    started_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    last_heartbeat TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Advisory file locks
CREATE TABLE file_claims (
    file_path TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    claimed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (file_path, session_id)
);

-- Cached infrastructure health (polled periodically)
CREATE TABLE health_cache (
    key TEXT PRIMARY KEY,       -- e.g. "k3s:pods", "ci:icn:main"
    value JSON NOT NULL,
    polled_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Decision index (full text lives in git files)
CREATE TABLE decision_index (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    tags TEXT,                  -- comma-separated
    file_path TEXT NOT NULL,
    created_at TIMESTAMP
);
```

The MCP server uses **stdio transport** (Claude Code manages the process lifecycle). SQLite runs in **WAL mode** for concurrent reads.

### Polling Strategy

Background threads poll external state into `health_cache`:

| Source | Frequency | Method |
|--------|-----------|--------|
| Git repos | 30s | `git status`, `git rev-list --count` |
| GitHub API | 60s | `gh run list`, `gh pr list` |
| K3s cluster | 60s | `kubectl get pods`, `kubectl top nodes` |
| sccache | 120s | `sccache --show-stats` |
| Worktrees | 30s | Scan `icn-wt/` directory |

All polling uses **child processes** (`kubectl`, `gh`, `git`, `sccache`) — never SSH from the MCP server. Results cached in SQLite; tools read from cache, never block on live queries.

## Section 3: Durable State — Git-Tracked Files

### Architecture Decision Records (`state/decisions/`)

```
state/decisions/
├── 0001-orchestration-plane-architecture.md
├── template.md
└── index.json    # Auto-generated index for quick lookup
```

ADR template:
```markdown
# ADR-NNNN: Title

**Date**: YYYY-MM-DD
**Status**: proposed | accepted | superseded | deprecated
**Tags**: kernel, networking, deployment, ...
**Supersedes**: ADR-NNNN (if applicable)

## Context
What's the situation? What forces are at play?

## Decision
What did we decide?

## Consequences
What are the trade-offs? What becomes easier/harder?

## Alternatives Considered
What else did we evaluate and why not?
```

### Sprint State (`state/sprint/`)

```
state/sprint/
├── current.json           # Active sprint: goals, tasks, owners
├── history/               # Closed sprints with outcomes
└── backlog.json           # Items not yet in a sprint
```

### Cross-Repo Config (`state/config/`)

```
state/config/
├── conventions.md         # Shared coding conventions
├── repo-map.json          # Repo locations, branches, relationships
└── agent-profiles.json    # Agent capabilities, preferred assignments
```

## Section 4: Root-Level Claude Config

Skills and agents at `/home/ubuntu/projects/.claude/`:

### Skills

**`/status`** — Dashboard command. Reads MCP live state + git sprint state.
```
Output:
  Active Sessions: 3 (Agent1→icn:naming, Agent2→icn-wt:auth, Agent3→icn-website:blog)
  Sprint: Pilot Vertical Slice Hardening (4/7 tasks done)
  Cluster: 3 pods healthy, gateway responding
  CI: main ✓, feat/1084 ✓, feat/1120 ⏳
  Worktrees: 4 active, 1 stale (1051 — 3 days behind main)
```

**`/sync-and-build`** — Cross-repo content pipeline.
```
1. Check icn/docs/ for changes since last sync
2. Run icn-website/scripts/sync-from-icn.sh
3. Build website (npm run build)
4. Report: files synced, build status, broken links
```

**`/worktree`** — Worktree lifecycle management.
```
/worktree create <name>     → Create new worktree + branch
/worktree status            → All worktrees with freshness, branch, claimed-by
/worktree cleanup           → Remove stale worktrees, prune branches
/worktree rebase <name>     → Rebase worktree on latest main
```

### Agents

**`orchestrator.md`** — Meta-agent that knows about all repos, their relationships, and current state.

### MCP Registration

Root `.claude/settings.json`:
```json
{
  "mcpServers": {
    "icn-ops": {
      "command": "node",
      "args": ["/home/ubuntu/projects/icn-ops/mcp/dist/index.js"],
      "env": {
        "ICN_OPS_DB": "/home/ubuntu/projects/icn-ops/mcp/data/icn-ops.db",
        "ICN_ROOT": "/home/ubuntu/projects"
      }
    }
  }
}
```

## Section 5: Website Claude Config

Give `icn-website/` first-class Claude support:

### CLAUDE.md
Covers Astro conventions, content sync architecture, design system tokens, file-based routing, gitignored content in `src/content/docs/`.

### Hooks
1. **Prettier auto-format** on Edit/Write (PostToolUse)
2. **Block synced content edits** — prevents editing `src/content/docs/` directly (PreToolUse), redirects to source in `icn/docs/`

### Rules
1. **`astro-conventions.md`** — Use content collections, no inline styles, CSS custom properties from `global.css`, responsive-first

## Section 6: Migration Path

What moves out of `icn/` into `icn-ops/`:

| Current Location | New Location | Notes |
|-----------------|-------------|-------|
| `icn/deploy/prometheus/` | `icn-ops/monitoring/` | Prometheus configs |
| `icn/deploy/grafana/` | `icn-ops/monitoring/dashboards/` | Grafana dashboards |
| `icn/.agent/workflows/` | `icn-ops/automation/` | Workflow definitions |
| `icn/scripts/worktrees.sh` | `icn-ops/automation/scripts/worktrees.sh` | Worktree helper |
| Sprint trackers in `icn/docs/` | `icn-ops/state/sprint/` | Sprint state |

Things that **stay** in `icn/`:
- All Rust code, Cargo config, crate-specific Claude agents/hooks/rules
- `icn/deploy/k8s/` manifests (deployment artifacts of the daemon)
- `icn/deploy/devnet/` Docker Compose (dev tool for the daemon)
- `icn/docs/` architecture and API docs (documentation of the substrate)

**Migration principle**: If something is *about* the ICN substrate, it stays in `icn/`. If something is *about operating, coordinating, or observing* the substrate (and its ecosystem), it moves to `icn-ops/`.

## Cross-Repo Relationships

These relationships are encoded in the orchestrator agent and MCP server:

- `icn/docs/` → `icn-website/src/content/docs/` (content sync, one-way)
- `icn/deploy/k8s/` manifests deploy to K3s cluster at 10.8.10.40-42
- `icn/icn/` is the Cargo workspace root (NOT `icn/`)
- `homelab-inventory` manages the infrastructure ICN runs on — read-only observation from `icn-ops`
- Worktrees live in `icn-wt/` as siblings, managed via `scripts/worktrees.sh`

## Implementation Phases

See the implementation plan (separate document) for ordered phases. Each phase is independently useful.

## Tech Stack

| Component | Technology |
|-----------|------------|
| MCP Server | TypeScript, Node.js v22 |
| MCP SDK | `@modelcontextprotocol/sdk` (stdio transport) |
| Database | SQLite via `better-sqlite3` (WAL mode) |
| Validation | Zod schemas |
| State Files | JSON (machine-readable) + Markdown (human-readable) |
| CI | GitHub Actions (reusable workflows) |
| Monitoring | Prometheus + Grafana |
