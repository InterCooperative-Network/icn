// Centralized monorepo root resolution for the MCP server.
// Compiled output lives at ops/mcp/dist/<file>.js — resolve up to repo root.

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
