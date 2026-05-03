// Background K3s polling — 60s interval
// Pre-warms health_cache so cluster_health/service_endpoints tool calls return instantly.

import type Database from "better-sqlite3";
import { execSync } from "child_process";
import { safeJsonParse } from "../diagnostics/safe-json.js";

const INTERVAL_MS = 60_000;

function runCmd(cmd: string): { ok: boolean; output: string } {
  try {
    const output = execSync(cmd, { encoding: "utf-8", timeout: 10_000 }).trim();
    return { ok: true, output };
  } catch (err) {
    return { ok: false, output: err instanceof Error ? err.message : String(err) };
  }
}

function writeCache(db: Database.Database, key: string, value: unknown): void {
  db.prepare(
    "INSERT OR REPLACE INTO health_cache (key, value, polled_at) VALUES (?, ?, datetime('now'))"
  ).run(key, JSON.stringify(value));
}

function parsePodsJson(cmd: { ok: boolean; output: string }): unknown {
  if (!cmd.ok) {
    return { error: cmd.output };
  }
  const text = cmd.output;
  if (!text) {
    return { error: "empty kubectl/jq output (no cluster access or jq failed silently)" };
  }
  const parsed = safeJsonParse(text, "kubectl/jq pods");
  if (!parsed.ok) {
    return { error: parsed.error, preview: parsed.preview };
  }
  return parsed.value;
}

const SERVICE_ENDPOINTS = [
  { name: "ICN Gateway", url: "http://10.8.30.40:30080/v1/health" },
  { name: "Pilot UI", url: "http://10.8.30.40:30030" },
  { name: "Prometheus", url: "http://10.8.30.40:30090/-/healthy" },
  { name: "Grafana", url: "http://10.8.30.40:30300/" },
  { name: "Registry", url: "http://10.8.30.40:30500/v2/_catalog" },
];

function pollOnce(db: Database.Database): void {
  try {
    // Pod status
    const pods = runCmd(
      "kubectl get pods --all-namespaces -o json 2>/dev/null | " +
        "jq '[.items[] | {name: .metadata.name, namespace: .metadata.namespace, phase: .status.phase, ready: (.status.conditions // [] | map(select(.type == \"Ready\")) | first | .status // \"Unknown\")}]'"
    );
    const services: Record<string, boolean> = {};
    for (const ep of SERVICE_ENDPOINTS) {
      services[ep.name] = runCmd(`curl -sf --max-time 3 ${ep.url}`).ok;
    }
    writeCache(db, "k3s:pods", {
      pods: parsePodsJson(pods),
      services,
    });

    // Service endpoint reachability
    const reachability = SERVICE_ENDPOINTS.map((e) => ({
      name: e.name,
      url: e.url,
      reachable: runCmd(`curl -sf --max-time 3 ${e.url}`).ok,
    }));
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
  setImmediate(() => pollOnce(db));
  return setInterval(() => pollOnce(db), INTERVAL_MS);
}
