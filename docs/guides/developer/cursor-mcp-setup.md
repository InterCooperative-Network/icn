# Cursor MCP Setup

This repo ships a local `icn-ops` MCP server in `ops/mcp`.

The Cursor-specific MCP registration lives in `.cursor/mcp.json` at the repo root.

## Config Surfaces

- `./.mcp.json`
  - Repo-local MCP registration used by Claude-oriented workflows in this repo.
  - Do not repurpose this file for Cursor-specific worktree setup.
- `./.claude/settings.json`
  - Claude-specific permissions, hooks, and session behavior.
  - This file is not the place to register Cursor MCP servers.
- `./.cursor/mcp.json`
  - Project-local Cursor MCP registration for the currently opened workspace.
  - This is the correct place to wire Cursor to the repo-local `icn-ops` server.
- `~/.cursor/mcp.json`
  - User-level Cursor MCP config.
  - Leave this empty or use it only for tools you want in every workspace.

## Build The Server

The Cursor config points at the built runtime entrypoint:

```bash
cd ops/mcp
npm ci
npm run build
```

Expected output path:

```text
ops/mcp/dist/index.js
```

The checked-in Cursor config uses a repo-relative path (`./ops/mcp/dist/index.js`).
Cursor resolves this relative to the workspace root (the directory you open in Cursor),
so it works for any local checkout without modification.

## Validate The Runtime

You can verify the server manually from the repo root:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"cursor-mcp-check","version":"1.0"}}}' \
  | node ./ops/mcp/dist/index.js
```

Expected result: a JSON-RPC response containing `"serverInfo":{"name":"icn-ops","version":"0.1.0"}`.

## Cursor Workflow

1. Open the repo root in Cursor (any local checkout path works).
2. Ensure `ops/mcp/dist/index.js` exists. If not, build it.
3. Reload the Cursor window so it re-reads `.cursor/mcp.json`
4. Confirm the `icn-ops` server appears in Cursor's MCP UI/tools list

## Claude Coexistence

- Claude-side repo MCP wiring is preserved in `./.mcp.json`
- Claude lifecycle and hook configuration stays in `./.claude/settings.json`
- Cursor-side worktree MCP wiring is isolated in `./.cursor/mcp.json`
- This keeps Cursor setup work from mutating user-global config or breaking existing Claude flows
