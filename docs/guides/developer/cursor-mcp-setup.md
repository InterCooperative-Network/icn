# Cursor and Claude MCP Setup

This repo ships a local `icn-ops` MCP server in `ops/mcp`.

Both Claude Code and Cursor should launch that server from the current checkout, not from a developer-specific absolute path. The project configs therefore use repo-relative paths / commands so the same checkout works on any contributor machine.

## Config Surfaces

- `./.mcp.json`
  - Repo-local MCP registration used by Claude-oriented workflows in this repo.
  - Starts `icn-ops` through `npm --prefix ./ops/mcp run start:stdio`.
  - Do not hard-code `/home/...`, `/Users/...`, WSL paths, VM names, or homelab host paths here.
- `./.claude/settings.json`
  - Claude-specific permissions, hooks, and session behavior.
  - This file is not the place to register MCP servers.
- `./.cursor/mcp.json`
  - Project-local Cursor MCP registration for the currently opened workspace.
  - This is the correct place to wire Cursor to the repo-local `icn-ops` server.
- `~/.mcp.json`
  - User-level Claude MCP config.
  - Use this only for personal/global servers you want in every workspace.
- `~/.cursor/mcp.json`
  - User-level Cursor MCP config.
  - Leave this empty or use it only for tools you want in every workspace.

## Install Dependencies

The checked-in Claude config builds and starts the MCP server automatically, but dependencies must be installed once per checkout:

```bash
cd ops/mcp
npm ci
```

## Build The Server Manually

The runtime entrypoint is:

```text
ops/mcp/dist/index.js
```

Build it manually with:

```bash
cd ops/mcp
npm run build
```

Claude's `./.mcp.json` uses `npm --prefix ./ops/mcp run start:stdio`, which runs the build before starting the server. Cursor currently points at the built runtime entrypoint directly, so Cursor users should ensure the build exists before reloading the Cursor MCP config.

## Validate The Runtime

You can verify the server manually from the repo root:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"icn-mcp-check","version":"1.0"}}}' \
  | node ./ops/mcp/dist/index.js
```

Expected result: a JSON-RPC response containing `"serverInfo":{"name":"icn-ops","version":"0.1.0"}`.

## Claude Workflow

1. Clone the repo anywhere.
2. Install dependencies once:

   ```bash
   cd ops/mcp
   npm ci
   ```

3. Start Claude Code from the repo root or a parent workspace that loads this repo's `.mcp.json`.
4. Confirm the `icn-ops` server appears in Claude's MCP UI/tools list.

## Cursor Workflow

1. Open the repo root in Cursor. Any local checkout path works.
2. Ensure `ops/mcp/dist/index.js` exists. If not, build it.
3. Reload the Cursor window so it re-reads `.cursor/mcp.json`.
4. Confirm the `icn-ops` server appears in Cursor's MCP UI/tools list.

## Claude / Cursor Coexistence

- Claude-side repo MCP wiring lives in `./.mcp.json`.
- Claude lifecycle and hook configuration stays in `./.claude/settings.json`.
- Cursor-side worktree MCP wiring is isolated in `./.cursor/mcp.json`.
- User-global MCP files should remain for personal/global tools only.

This keeps contributor setup portable and prevents one developer's machine path from becoming everyone else's broken default.
