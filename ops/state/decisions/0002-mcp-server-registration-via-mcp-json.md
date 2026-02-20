# ADR-0002: MCP Server Registration via ~/.mcp.json

**Date**: 2026-02-19
**Status**: accepted
**Tags**: mcp, claude-code, setup

## Context

Claude Code's ~/.claude/settings.json schema does not accept a top-level mcpServers field. MCP servers must be registered in ~/.mcp.json (user scope) or .mcp.json (project scope), with enableAllProjectMcpServers: true to auto-approve them.

## Decision

Register icn-ops MCP server in ~/.mcp.json rather than ~/.claude/settings.json. Set enableAllProjectMcpServers: true in ~/.claude/settings.json to auto-approve without prompting each session.

## Consequences

Server is available in all Claude Code sessions regardless of working directory. No manual approval prompt on session start. dist/ must exist (run npm run build in icn-ops/mcp/ after each clone/reset).

## Alternatives Considered

Project-level .mcp.json: works but only loads when starting Claude Code from that exact directory. settings.json mcpServers: rejected by schema validator in newer Claude Code versions.
