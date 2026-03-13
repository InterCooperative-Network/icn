import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type Database from "better-sqlite3";
import { exec } from "child_process";
import { promisify } from "util";

const execAsync = promisify(exec);
const CACHE_TTL_SECS = 90;
const MAX_BUFFER = 10 * 1024 * 1024; // 10 MB

async function cached<T>(
  db: Database.Database,
  key: string,
  ttlSecs: number,
  fetch: () => Promise<T>
): Promise<T> {
  const row = db
    .prepare(
      `SELECT value, polled_at FROM health_cache
       WHERE key = ? AND datetime(polled_at, '+' || ? || ' seconds') > datetime('now')`
    )
    .get(key, ttlSecs) as { value: string; polled_at: string } | undefined;

  if (row) return JSON.parse(row.value) as T;

  const value = await fetch();
  db.prepare("INSERT OR REPLACE INTO health_cache (key, value) VALUES (?, ?)").run(
    key,
    JSON.stringify(value)
  );
  return value;
}

async function runCmd(cmd: string): Promise<{ ok: boolean; output: string }> {
  try {
    const { stdout } = await execAsync(cmd, { timeout: 10000, maxBuffer: MAX_BUFFER });
    return { ok: true, output: stdout.trim() };
  } catch (err) {
    return {
      ok: false,
      output: err instanceof Error ? err.message : String(err),
    };
  }
}

const SERVICE_ENDPOINTS = [
  { name: "ICN Gateway", url: "http://10.8.30.40:30080/v1/health" },
  { name: "Pilot UI", url: "http://10.8.30.40:30030" },
  { name: "Prometheus", url: "http://10.8.30.40:30090/-/healthy" },
  { name: "Grafana", url: "http://10.8.30.40:30300/" },
  { name: "Registry", url: "http://10.8.30.40:30500/v2/_catalog" },
];

export function registerHealthTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "cluster_health",
    "Get K3s cluster pod status and service endpoint reachability (cached 90s).",
    {},
    async () => {
      const result = await cached(db, "k3s:pods", CACHE_TTL_SECS, async () => {
        const pods = await runCmd(
          "kubectl get pods --all-namespaces -o json 2>/dev/null | " +
          "jq '[.items[] | {name: .metadata.name, namespace: .metadata.namespace, phase: .status.phase, ready: (.status.conditions // [] | map(select(.type == \"Ready\")) | first | .status // \"Unknown\")}]'"
        );
        const serviceResults = await Promise.all(
          SERVICE_ENDPOINTS.map(async (ep) => [
            ep.name,
            (await runCmd(`curl -sf --max-time 3 ${ep.url}`)).ok,
          ] as [string, boolean])
        );
        return {
          pods: pods.ok && pods.output ? (JSON.parse(pods.output) as unknown) : { error: pods.output },
          services: Object.fromEntries(serviceResults),
        };
      });
      return {
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
      };
    }
  );

  server.tool(
    "build_cache_status",
    "Get sccache hit rates and disk usage (cached 120s).",
    {},
    async () => {
      const result = await cached(db, "sccache:stats", 120, async () => {
        const stats = await runCmd("sccache --show-stats");
        return { ok: stats.ok, output: stats.output };
      });
      return {
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
      };
    }
  );

  server.tool(
    "service_endpoints",
    "List all known service endpoints and whether they're reachable.",
    {},
    async () => {
      const results = await Promise.all(
        SERVICE_ENDPOINTS.map(async (e) => ({
          name: e.name,
          url: e.url,
          reachable: (await runCmd(`curl -sf --max-time 3 ${e.url}`)).ok,
        }))
      );
      return {
        content: [{ type: "text", text: JSON.stringify(results, null, 2) }],
      };
    }
  );
}
