# ops/CLAUDE.md

Claude-specific scoped guidance for work under `ops/`.

Read root `AGENTS.md` first. This file owns only stable **ops-directory mechanics**. Current sprint, repository topology, agent/skill routing, merge policy, infrastructure roles, and other mutable facts are owned by the machine-readable sources under `ops/state/`; do not restate them here.

## What `ops/` is

`ops/` is ICN's development-orchestration and operational coordination plane. It observes, coordinates, validates, and automates work around the monorepo; it is not Rust substrate/runtime code.

Useful areas:

| Path | Role |
|---|---|
| `mcp/` | TypeScript MCP server and tests |
| `automation/skills/` | Canonical shared operational skills where `skills.json` assigns ownership here |
| `automation/hooks/` | Shared hook implementations |
| `automation/scripts/` | Shared orchestration scripts |
| `monitoring/` | Dashboards, alerts, runbooks |
| `state/sprint/` | Machine-readable sprint state/history |
| `state/config/` | Repository/worktree/infrastructure-role metadata |
| `state/truth/` | Truth-owner, merge-policy, agent, and skill registries |
| `coordination/` | Cross-repo coordination manifests/protocols |
| `scripts/` | Operational validation/orientation scripts |

If this table and the current tree disagree, the tree plus the relevant registry wins. Update this routing table rather than treating it as a hidden second inventory.

## MCP server

From `ops/mcp/`:

```bash
npm install
npm run build
npm run dev
npm test
```

The MCP server uses stdio transport. Runtime/cache/session databases are ephemeral unless a registered source says otherwise; do not infer durable project truth from an MCP cache.

## Truth and state

Resolve mutable facts from their owners:

- fact/domain owner: `state/truth/sources.json`
- merge/CI policy: `state/truth/policy.json` plus live branch protection
- agent routing: `state/truth/agents.json`
- skill ownership/routing: `state/truth/skills.json`
- sprint/task state: `state/sprint/current.json`
- repository/worktree/infrastructure-role metadata: `state/config/repo-map.json`
- live branch/PR/review/CI state: Git/GitHub

Do not make `what-matters-now.sh`, an MCP response, a handoff, or this file into a replacement owner. Use synthesis scripts when their summary is useful for the current ops task, then verify consequential claims against their sources.

## Skill ownership

Before editing a skill, resolve its canonical source through `state/truth/skills.json`.

Provider-facing copies/symlinks are compatibility surfaces. Do not repair drift by independently editing both copies unless the registry explicitly defines them as separate owners.

Run the repository's drift tooling after agent/skill/truth-registry changes:

```bash
bash ops/scripts/drift-check.sh
bash scripts/check-preflight-consistency.sh
```

## Repository topology

Never rely on a memorized local worktree path. Resolve the current checkout with Git and consult `state/config/repo-map.json` when canonical worktree/repository relationships matter.

Concrete private provider addresses and secrets do not belong in this public file. Public topology metadata should use roles/boundaries; resolve private operational values from the registered private source when authorized and needed.

## ADRs and durable decisions

Architecture decisions live under root `docs/adr/`. Use the registered ADR lifecycle/template and available decision tooling rather than creating an ops-local competing decision store.

A session note, MCP record, or ops plan does not become an architectural decision merely because it is durable in Git.

## Scope boundary

Keep out of `ops/` unless a registered architecture says otherwise:

- Rust/Cargo runtime implementation -> `icn/`
- website implementation -> `website/`
- generic documentation truth -> the appropriate root `docs/` owner
- institution-local/private meaning -> the institution/private source
- concrete provider secrets/topology -> the authorized private ops source

## Verification

Derive checks from the files changed and their owners. Typical ops work may require MCP TypeScript tests, JSON/schema validation, drift checks, shell lint/smoke tests, or workflow inspection. Do not run unrelated whole-workspace Rust verification just because the task lives in the monorepo.

## Conflict rule

If this scoped adapter conflicts with root `AGENTS.md`, `state/truth/sources.json`, current code/tool behavior, or live Git/GitHub state, this adapter is stale. Fix the scoped guidance rather than propagating its old assumption.
