# ADR-0002: MCP Server Registration via ~/.mcp.json

**Date**: 2026-02-19
**Status**: amended
**Amended by**: ADR-0017
**Tags**: mcp, claude-code, setup

> **Amendment (2026-04-26):** the registration mechanism described
> here remains correct — MCP servers are still registered in
> `~/.mcp.json` (user scope) or `.mcp.json` (project scope), with
> `enableAllProjectMcpServers: true` in `~/.claude/settings.json`.
>
> What changed is the path. MCP source is no longer in a separate
> `icn-ops/` repo; it lives in the main ICN repo under `ops/mcp/`.
> The `dist/` build path that `~/.mcp.json` references has moved
> accordingly. ADRs are canonical under `docs/adr/`, not under
> `ops/state/decisions/` (see [ADR-0018](ADR-0018-adr-lifecycle-and-canonical-decision-index.md)).
> The decision recorded here is unchanged; its addresses are
> updated. See [ADR-0017](ADR-0017-monorepo-consolidation-with-explicit-internal-boundaries.md)
> for the canonical layout.

## Context

Claude Code's ~/.claude/settings.json schema does not accept a top-level mcpServers field. MCP servers must be registered in ~/.mcp.json (user scope) or .mcp.json (project scope), with enableAllProjectMcpServers: true to auto-approve them.

## Decision

Register icn-ops MCP server in ~/.mcp.json rather than ~/.claude/settings.json. Set enableAllProjectMcpServers: true in ~/.claude/settings.json to auto-approve without prompting each session.

## Consequences

Server is available in all Claude Code sessions regardless of working directory. No manual approval prompt on session start. dist/ must exist (run npm run build in icn-ops/mcp/ after each clone/reset).

## Alternatives Considered

Project-level .mcp.json: works but only loads when starting Claude Code from that exact directory. settings.json mcpServers: rejected by schema validator in newer Claude Code versions.
