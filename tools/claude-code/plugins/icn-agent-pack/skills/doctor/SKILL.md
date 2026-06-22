---
name: doctor
description: ICN Agent Pack self-diagnosis. This skill should be used when the user explicitly invokes "/icn-agent-pack:doctor", or asks to "diagnose the plugin", "is the icn-agent-pack plugin loaded", "why can't the plugin reach the repo/MCP", or "check icn-ops MCP startability". Runs read-only checks on plugin load, ICN root resolution, the icn-ops MCP launcher, and the repo/Node prerequisites.
disable-model-invocation: true
user-invocable: true
allowed-tools: "Bash, Read"
---

Diagnose whether the ICN Agent Pack plugin is correctly loaded and can reach the repository and the
`icn-ops` MCP layer. **Read-only** — do not install, build, edit, or start anything. Report a short
PASS/WARN/FAIL line per check plus a one-line overall verdict.

## Locate the plugin and the ICN root (read-only)

Resolve the plugin's bin dir and the ICN root without assuming the current directory is the repo root:

```sh
# Plugin root: prefer the env var Claude Code sets; fall back to discovery only if unset.
PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:-}"
[ -n "$PLUGIN_ROOT" ] && echo "plugin root: $PLUGIN_ROOT" || echo "CLAUDE_PLUGIN_ROOT not set (plugin may not be loaded as a plugin)"

# ICN root: use the plugin resolver if reachable, else git, else ICN_ROOT.
ICN_ROOT_RESOLVED="$([ -n "$PLUGIN_ROOT" ] && "$PLUGIN_ROOT"/bin/icn-find-root 2>/dev/null || git rev-parse --show-toplevel 2>/dev/null)"
echo "icn root: ${ICN_ROOT_RESOLVED:-UNRESOLVED}"
```

If the root is `UNRESOLVED`, tell the user to launch from the ICN repo root or set `ICN_ROOT=/path/to/icn`.

## Checks (run from the resolved ICN root `R="$ICN_ROOT_RESOLVED"`)

1. **Plugin loaded** — `CLAUDE_PLUGIN_ROOT` set, and `"$PLUGIN_ROOT"/bin/icn-ops-mcp` exists and is executable.
2. **ICN root resolution** — `"$PLUGIN_ROOT"/bin/icn-find-root` prints a path and exits 0 (re-run; report any ambiguity message verbatim).
3. **icn-ops-mcp startability (dry, no launch)** — confirm the wrapper, `npm`, and `node` are present; do **not** actually start the server. `command -v npm`, `command -v node`, `node --version`, `npm --version`.
4. **ops/mcp project** — `test -f "$R/ops/mcp/package.json"`.
5. **ops/mcp deps** — `test -d "$R/ops/mcp/node_modules"` (WARN if missing: `npm ci` in `ops/mcp` is needed before first use).
6. **better-sqlite3** — if `node_modules` exists, WARN unless `test -d "$R/ops/mcp/node_modules/better-sqlite3"`; note it is a native module rebuilt in postinstall and must match the host Node.
7. **Root MCP parity intact** — `python3 "$R/scripts/check-mcp-portability.py"` (confirms the canonical root `.mcp.json` / `.cursor/mcp.json` are unchanged; this plugin must not alter them).
8. **Truth docs present** — `test -f "$R/docs/STATE.md"` and `test -f "$R/docs/PHASE_PROGRESS.md"`.
9. **gh auth (optional)** — if `command -v gh`, run `gh auth status` and report; otherwise note "gh not installed".

## Output

One line per check (`PASS`/`WARN`/`FAIL` + the evidence), then a final verdict line. If anything is
FAIL, give the single most useful next step (commonly: set `ICN_ROOT`, run `npm ci` in `ops/mcp`, or
launch from the repo root). Change nothing.
