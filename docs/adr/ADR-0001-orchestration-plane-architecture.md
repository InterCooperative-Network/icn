# ADR-0001: Orchestration Plane Architecture (icn-ops)

**Date**: 2026-02-19
**Status**: accepted
**Tags**: orchestration, mcp, multi-agent, cross-repo, state-management

## Context

ICN development spans multiple repositories (`icn/`, `icn-website/`, `icn-wt/`) and increasingly involves parallel Claude Code agent sessions working across git worktrees. Three persistent problems emerged:

1. **Lost context between sessions**: Work-in-progress state, architecture decisions, and environment status must be re-explained to every new Claude session.
2. **No coordination primitive**: Multiple agents editing the same crate simultaneously with no awareness of each other.
3. **Operational tooling scattered**: Monitoring configs, CI templates, workflow automation, and sprint tracking live "everywhere and nowhere" across the repos.

The `icn/` repo had strong substrate-level Claude Code automation (7 agents, 10 skills, 4 hooks) but nothing coordinated across repos or sessions. The `icn-website/` repo had zero Claude Code support.

## Decision

Create a dedicated `icn-ops` repository as the **orchestration plane** — the "nervous system" across all ICN development:

- **Hybrid state model**: MCP server (TypeScript + SQLite) for live/ephemeral state (sessions, advisory locks, health caches); git-tracked files for durable state (ADRs, sprint plans, cross-repo config).
- **TypeScript MCP server** with stdio transport, SQLite in WAL mode for concurrent reads, background health polling via child processes (`kubectl`, `gh`, `git`, `sccache`).
- **Directory layout**: `mcp/` (server), `automation/` (skills/hooks/scripts), `ci/` (shared workflows), `monitoring/` (dashboards/alerts), `state/` (durable state), `docs/` (design docs).
- **Root-level `.claude/`** at `/home/ubuntu/projects/` registers the MCP server and hosts cross-repo skills (`/status`, `/sync-and-build`, `/worktree`) and an orchestrator agent.
- **icn-website gets first-class Claude support**: CLAUDE.md, Prettier hook, synced-content guard hook, Astro conventions rule.

The design is documented in full at [`docs/plans/2026-02-19-icn-ops-design.md`](../../docs/plans/2026-02-19-icn-ops-design.md).

## Consequences

**Becomes easier:**
- Any Claude session (new or resumed) gets full project context via MCP tools in the first turn
- Parallel agents can avoid stepping on each other via advisory file claims
- Infrastructure health, CI status, and sprint state are always one tool call away
- Architecture decisions accumulate in a searchable, auditable ADR log
- Operational tooling (monitoring configs, CI templates, scripts) has a clear home

**Becomes harder:**
- icn-ops MCP server must be running for rich orchestration features (graceful degradation: tools fail informatively when server is down)
- Moving monitoring configs from `icn/deploy/` creates a migration step
- One more repo to maintain

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| **MCP server as central brain** (all state in SQLite) | Loses git history on decisions; durable state can't be diffed or audited |
| **File-based state only** (no MCP server) | File locking for 4+ concurrent agents is fragile; no live polling of external state |
| **Enhance existing root `.claude/`** (no new repo) | Operational tooling has different lifecycle than project code; mixing them creates confusion; no version-controlled home for monitoring/CI configs |
| **Embed in `icn/`** | `icn/` should stay focused on the substrate; operational concerns violate the kernel/app separation principle |
