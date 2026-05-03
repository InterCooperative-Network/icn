// Background K3s polling — 60s interval
// Pre-warms health_cache so cluster_health/service_endpoints tool calls return instantly.

import type Database from "better-sqlite3";
import { runCommand } from "../utils/commands.js";
import { summarizePodsFromKubectlJsonString } from "../utils/kubectl-pods.js";

const INTERVAL_MS = 60_000;
const KUBECTL_JSON_MAX = 16 * 1024 * 1024;

function writeCache(db: Database.Database, key: string, value: unknown): void {
  db.prepare(
    "INSERT OR REPLACE INTO health_cache (key, value, polled_at) VALUES (?, ?, datetime('now'))"
  ).run(key, JSON.stringify(value));
}

const SERVICE_ENDPOINTS = [
  { name: "ICN Gateway", url: "http://10.8.30.40:30080/v1/health" },
  { name: "Pilot UI", url: "http://10.8.30.40:30030" },
  { name: "Prometheus", url: "http://10.8.30.40:30090/-/healthy" },
  { name: "Grafana", url: "http://10.8.30.40:30300/" },
  { name: "Registry", url: "http://10.8.30.40:30500/v2/_catalog" },
];

async function pollOnce(db: Database.Database): Promise<void> {
  try {
    // Pod status
    const kr = await runCommand(
      "kubectl",
      ["get", "pods", "--all-namespaces", "-o", "json"],
      { timeoutMs: 12_000, maxStdoutBytes: KUBECTL_JSON_MAX, maxStderrBytes: 32 * 1024 }
    );
    const pods = kr.ok
      ? summarizePodsFromKubectlJsonString(kr.stdout)
      : {
          error: kr.stderr || "kubectl failed",
          exitCode: kr.exitCode,
          timedOut: kr.timedOut,
        };

    const services: Record<string, boolean> = {};
    for (const ep of SERVICE_ENDPOINTS) {
      const cr = await runCommand("curl", ["-sf", "--max-time", "3", ep.url], {
        timeoutMs: 5000,
        maxStdoutBytes: 4096,
        maxStderrBytes: 1024,
      });
      services[ep.name] = cr.ok;
    }
    writeCache(db, "k3s:pods", { pods, services });

    // Service endpoint reachability
    const reachability = await Promise.all(
      SERVICE_ENDPOINTS.map(async (e) => {
        const cr = await runCommand("curl", ["-sf", "--max-time", "3", e.url], {
          timeoutMs: 5000,
          maxStdoutBytes: 4096,
          maxStderrBytes: 1024,
        });
        return { name: e.name, url: e.url, reachable: cr.ok };
      })
    );
    writeCache(db, "k3s:endpoints", reachability);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.error("cluster poll failed:", message);
    try {
      writeCache(db, "k3s:pods", { error: message, services: {} });
    } catch {
      // ignore secondary failures
    }
  }
}

export function startClusterPolling(db: Database.Database): NodeJS.Timeout {
  setImmediate(() => {
    void pollOnce(db).catch((e) => console.error("cluster poll async error:", e));
  });
  return setInterval(() => {
    void pollOnce(db).catch((e) => console.error("cluster poll async error:", e));
  }, INTERVAL_MS);
}
