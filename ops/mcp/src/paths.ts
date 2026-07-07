// Centralized monorepo root resolution for the MCP server.
// Compiled output lives at ops/mcp/dist/<file>.js — resolve up to repo root.

import { readFileSync } from "node:fs";
import path from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

export function resolveMonorepoRoot(): string {
  if (process.env["ICN_ROOT"]) return process.env["ICN_ROOT"];
  // compiled file: ops/mcp/dist/<file>.js
  // up: dist -> mcp -> ops -> repo root
  const here = dirname(fileURLToPath(import.meta.url));
  return path.resolve(here, "..", "..", "..");
}

export function resolveOpsStatePath(...parts: string[]): string {
  return path.join(resolveMonorepoRoot(), "ops", "state", ...parts);
}

let warnedLegacyWtRoot = false;

// Worktree-root resolution order: ICN_WT_ROOT env → repo-map.json#worktrees.root
// (relative values resolve against the monorepo root; a leading ~ expands to $HOME)
// → legacy ../icn-wt sibling. The legacy layout is retired; the fallback exists only
// so pre-worktree-OS checkouts degrade gracefully, and it logs a diagnostic when hit.
export function resolveWorktreeRoot(): string {
  const env = process.env["ICN_WT_ROOT"];
  if (env) return env;
  const root = resolveMonorepoRoot();
  try {
    const raw = readFileSync(
      path.join(root, "ops", "state", "config", "repo-map.json"),
      "utf8"
    );
    const configured = (JSON.parse(raw) as { worktrees?: { root?: unknown } })
      .worktrees?.root;
    if (typeof configured === "string" && configured.length > 0) {
      const home = process.env["HOME"];
      const expanded =
        home && (configured === "~" || configured.startsWith("~/"))
          ? path.join(home, configured.slice(1))
          : configured;
      return path.resolve(root, expanded);
    }
  } catch {
    // unreadable/invalid repo-map — fall through to the legacy fallback
  }
  if (!warnedLegacyWtRoot) {
    warnedLegacyWtRoot = true;
    console.error(
      "icn-ops: worktree root not configured (repo-map.json#worktrees.root missing/invalid); falling back to legacy ../icn-wt"
    );
  }
  return path.resolve(root, "..", "icn-wt");
}
