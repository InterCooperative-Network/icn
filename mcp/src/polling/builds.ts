// Background sccache stats polling — 120s interval
// Pre-warms health_cache so build_cache_status tool calls return instantly.

import type Database from "better-sqlite3";
import { execSync } from "child_process";

const INTERVAL_MS = 120_000;

function writeCache(db: Database.Database, key: string, value: unknown): void {
  db.prepare(
    "INSERT OR REPLACE INTO health_cache (key, value, polled_at) VALUES (?, ?, datetime('now'))"
  ).run(key, JSON.stringify(value));
}

function pollOnce(db: Database.Database): void {
  try {
    const output = execSync("sccache --show-stats", {
      encoding: "utf-8",
      timeout: 10_000,
    }).trim();
    writeCache(db, "sccache:stats", { ok: true, output });
  } catch (err) {
    writeCache(db, "sccache:stats", {
      ok: false,
      output: err instanceof Error ? err.message : String(err),
    });
  }
}

export function startBuildsPolling(db: Database.Database): NodeJS.Timeout {
  setImmediate(() => pollOnce(db));
  return setInterval(() => pollOnce(db), INTERVAL_MS);
}
