import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";

export const CANONICAL_ICN_OPS_COMMAND = "npm";
export const CANONICAL_ICN_OPS_ARGS = ["--prefix", "./ops/mcp", "run", "start:stdio"] as const;

export type McpFileReport = {
  path: string;
  exists: boolean;
  usesCanonicalLaunch: boolean | null;
  /** True if args reference dist/index.js directly (discouraged). */
  invokesDistDirectly: boolean | null;
  parseError?: string;
};

function readMcpServer(
  repoRoot: string,
  relPath: string
): { ok: true; server: Record<string, unknown> } | { ok: false; error: string } {
  const full = join(repoRoot, relPath);
  if (!existsSync(full)) {
    return { ok: false, error: "file missing" };
  }
  try {
    const raw = readFileSync(full, "utf-8");
    const parsed = JSON.parse(raw) as { mcpServers?: Record<string, unknown> };
    const icn = parsed.mcpServers?.["icn-ops"];
    if (!icn || typeof icn !== "object") {
      return { ok: false, error: "mcpServers.icn-ops missing or not an object" };
    }
    return { ok: true, server: icn as Record<string, unknown> };
  } catch (e) {
    return {
      ok: false,
      error: e instanceof Error ? e.message : String(e),
    };
  }
}

function analyzeServer(server: Record<string, unknown>): {
  usesCanonicalLaunch: boolean;
  invokesDistDirectly: boolean;
} {
  const cmd = server["command"];
  const args = server["args"];
  const argList = Array.isArray(args) ? args.map(String) : [];
  const usesCanonicalLaunch =
    cmd === CANONICAL_ICN_OPS_COMMAND &&
    argList.length === CANONICAL_ICN_OPS_ARGS.length &&
    CANONICAL_ICN_OPS_ARGS.every((a, i) => argList[i] === a);
  const invokesDistDirectly = argList.some(
    (a) => a.includes("dist/index.js") || a.endsWith("/dist/index.js")
  );
  return { usesCanonicalLaunch, invokesDistDirectly };
}

export function inspectMcpConfigs(repoRoot: string): {
  files: McpFileReport[];
} {
  const paths = [".mcp.json", join(".cursor", "mcp.json")];
  const files: McpFileReport[] = [];
  for (const rel of paths) {
    const read = readMcpServer(repoRoot, rel);
    if (!read.ok) {
      files.push({
        path: rel,
        exists: existsSync(join(repoRoot, rel)),
        usesCanonicalLaunch: null,
        invokesDistDirectly: null,
        parseError: read.error,
      });
      continue;
    }
    const { usesCanonicalLaunch, invokesDistDirectly } = analyzeServer(read.server);
    files.push({
      path: rel,
      exists: true,
      usesCanonicalLaunch,
      invokesDistDirectly,
    });
  }
  return { files };
}
