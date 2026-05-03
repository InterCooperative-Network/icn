// Background sccache stats polling — 120s interval
// Pre-warms health_cache so build_cache_status tool calls return instantly.

import type Database from "better-sqlite3";
import { runCommand } from "../utils/commands.js";

const INTERVAL_MS = 120_000;

function writeCache(db: Database.Database, key: string, value: unknown): void {
  db.prepare(
    "INSERT OR REPLACE INTO health_cache (key, value, polled_at) VALUES (?, ?, datetime('now'))"
  ).run(key, JSON.stringify(value));
}

async function pollOnce(db: Database.Database): Promise<void> {
  try {
    const r = await runCommand("sccache", ["--show-stats"], {
      timeoutMs: 10_000,
      maxStdoutBytes: 256 * 1024,
      maxStderrBytes: 16 * 1024,
    });
    if (r.ok) {
      writeCache(db, "sccache:stats", { ok: true, output: r.stdout });
    } else {
      writeCache(db, "sccache:stats", {
        ok: false,
        output: r.stderr || r.stdout || "sccache failed",
        exitCode: r.exitCode,
        timedOut: r.timedOut,
      });
    }
  } catch (err) {
    writeCache(db, "sccache:stats", {
      ok: false,
      output: err instanceof Error ? err.message : String(err),
    });
  }
}

export function startBuildsPolling(db: Database.Database): NodeJS.Timeout {
  setImmediate(() => {
    void pollOnce(db).catch((e) => console.error("builds poll async error:", e));
  });
  return setInterval(() => {
    void pollOnce(db).catch((e) => console.error("builds poll async error:", e));
  }, INTERVAL_MS);
}
