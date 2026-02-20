import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type Database from "better-sqlite3";
import { execSync } from "child_process";

const CACHE_TTL_SECS = 90;

function cached<T>(
  db: Database.Database,
  key: string,
  ttlSecs: number,
  fetch: () => T
): T {
  const row = db
    .prepare(
      `SELECT value, polled_at FROM health_cache
       WHERE key = ? AND datetime(polled_at, '+' || ? || ' seconds') > datetime('now')`
    )
    .get(key, ttlSecs) as { value: string; polled_at: string } | undefined;

  if (row) return JSON.parse(row.value) as T;

  const value = fetch();
  db.prepare("INSERT OR REPLACE INTO health_cache (key, value) VALUES (?, ?)").run(
    key,
    JSON.stringify(value)
  );
  return value;
}

function runCmd(cmd: string): { ok: boolean; output: string } {
  try {
    const output = execSync(cmd, {
      encoding: "utf-8",
      timeout: 10000,
    }).trim();
    return { ok: true, output };
  } catch (err) {
    return {
      ok: false,
      output: err instanceof Error ? err.message : String(err),
    };
  }
}

export function registerHealthTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "cluster_health",
    "Get K3s cluster pod status and service endpoint reachability (cached 90s).",
    {},
    async () => {
      const result = cached(db, "k3s:pods", CACHE_TTL_SECS, () => {
        const pods = runCmd(
          "kubectl get pods --all-namespaces -o json 2>/dev/null | " +
          "jq '[.items[] | {name: .metadata.name, namespace: .metadata.namespace, phase: .status.phase, ready: (.status.conditions // [] | map(select(.type == \"Ready\")) | first | .status // \"Unknown\")}]'"
        );
        const services = {
          gateway: runCmd("curl -sf --max-time 3 http://10.8.10.40:30080/health").ok,
          pilot_ui: runCmd("curl -sf --max-time 3 http://10.8.10.40:30030").ok,
          metrics: runCmd("curl -sf --max-time 3 http://10.8.10.40:30091/-/healthy").ok,
        };
        return {
          pods: pods.ok ? JSON.parse(pods.output) : { error: pods.output },
          services,
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
      const result = cached(db, "sccache:stats", 120, () => {
        const stats = runCmd("sccache --show-stats");
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
      const endpoints = [
        { name: "ICN Gateway", url: "http://10.8.10.40:30080/health" },
        { name: "Pilot UI", url: "http://10.8.10.40:30030" },
        { name: "Prometheus", url: "http://10.8.10.40:30091/-/healthy" },
      ];
      const results = endpoints.map((e) => ({
        name: e.name,
        url: e.url,
        reachable: runCmd(`curl -sf --max-time 3 ${e.url}`).ok,
      }));
      return {
        content: [{ type: "text", text: JSON.stringify(results, null, 2) }],
      };
    }
  );
}
