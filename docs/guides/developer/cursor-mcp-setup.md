# Cursor and Claude MCP Setup

This repo ships a local `icn-ops` MCP server in `ops/mcp`.

**Clients:** Cursor (`.cursor/mcp.json`), Claude Code and other tools that honor the repo-root `.mcp.json`, and any generic MCP host that can run a `command` + `args` from the workspace root should all launch the server the same way. Paths are repo-relative so a clone works on any machine; each host still uses **its own** `npm`/`node` from PATH, so keep Node majors aligned between “where you ran `npm ci`” and “where the MCP host spawns processes” (see Node ABI note below).

For **agent-facing MCP tools** (`icn_ops_doctor`, environment report, command catalog, etc.), see [agent-mcp-tooling.md](./agent-mcp-tooling.md).

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
  - Uses the **same** `icn-ops` launch stanza as `./.mcp.json` (portability script enforces this).
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

Both `./.mcp.json` and `./.cursor/mcp.json` use `npm --prefix ./ops/mcp run start:stdio`, which runs `tsc` then `node dist/index.js` under **one** `node` resolved for that `npm` invocation. After `npm ci` / `npm install`, `postinstall` runs `npm rebuild better-sqlite3` so native bindings match **that** Node.

**Node ABI:** `better-sqlite3` is native. If you install dependencies with Node 22 but the MCP host starts the server with Node 20 (or the reverse), the addon fails to load until you run `npm ci` or `npm rebuild better-sqlite3` using the same Node the host will use. The unified `npm … start:stdio` entrypoint does not remove that OS-level constraint; it only ensures `tsc` and `node dist/index.js` agree within a single spawn.

**DevDependency noise:** Vitest pulls Vite 7, which may print `EBADENGINE` on Node versions older than **20.19**; that warning is about the test runner stack, not the MCP runtime. Use Node **20.19+** or **22.12+** if you want a clean `npm ci` with no engine warnings.

## Validate The Runtime

From the **repo root**, confirm dependencies install, TypeScript builds, and the stdio server stays up briefly without exiting (same path MCP clients use):

```bash
npm --prefix ./ops/mcp ci
npm --prefix ./ops/mcp run build
python3 scripts/check-mcp-portability.py
timeout 5 npm --prefix ./ops/mcp run start:stdio
```

Smoke test: you should **not** see a Node stack trace right after `node dist/index.js` starts. With GNU **coreutils** `timeout`, exit status **124** means the timer fired while the server was still running (expected). If the server exits immediately, investigate stderr (common causes: missing `ops/mcp` deps, Node/native ABI mismatch, or DB path issues). On macOS, use `gtimeout` from Homebrew **coreutils** if `timeout` is missing. After `npm ci`, run `npm test` inside `ops/mcp` when changing server code.

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
2. Run `cd ops/mcp && npm ci` once per checkout (or after changing Node major versions).
3. Reload the Cursor window so it re-reads `.cursor/mcp.json`.
4. Confirm the `icn-ops` server appears in Cursor's MCP UI/tools list.

## Claude / Cursor / other MCP hosts

- Repo-local MCP wiring for Claude-compatible hosts: `./.mcp.json` (same `icn-ops` stanza as Cursor).
- Claude lifecycle and hooks: `./.claude/settings.json` (not used for MCP server registration in this repo).
- Cursor project MCP: `./.cursor/mcp.json` (must stay identical to `./.mcp.json` for `icn-ops`; enforced by `scripts/check-mcp-portability.py`).
- User-global `~/.mcp.json` / `~/.cursor/mcp.json`: optional; do not duplicate `icn-ops` there unless you intentionally want a second registration.

This keeps contributor setup portable and prevents one developer's machine path from becoming everyone else's broken default.
