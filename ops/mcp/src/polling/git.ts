// Background git status polling — 30s interval
// Pre-warms health_cache so repo_status/worktree_status tool calls return instantly.

import type Database from "better-sqlite3";
import { readdirSync } from "node:fs";
import { join } from "node:path";
import { runCommand } from "../utils/commands.js";

const INTERVAL_MS = 30_000;

function writeCache(db: Database.Database, key: string, value: unknown): void {
  db.prepare(
    "INSERT OR REPLACE INTO health_cache (key, value, polled_at) VALUES (?, ?, datetime('now'))"
  ).run(key, JSON.stringify(value));
}

async function gitOut(repoPath: string, args: readonly string[]): Promise<string> {
  const r = await runCommand("git", [...args], {
    cwd: repoPath,
    timeoutMs: 15_000,
    maxStdoutBytes: 256 * 1024,
    maxStderrBytes: 32 * 1024,
  });
  return r.ok ? r.stdout.trim() : "";
}

async function pollOnce(db: Database.Database, icnRoot: string): Promise<void> {
  try {
    const repos = ["icn"] as const;

    const statuses: Record<string, unknown> = {};
    for (const repo of repos) {
      const dir =
        repo === "icn" ? icnRoot : join(icnRoot, "..", repo);
      const branch = await gitOut(dir, ["rev-parse", "--abbrev-ref", "HEAD"]);
      if (!branch) {
        statuses[repo] = { error: "not a git repo or not found" };
        continue;
      }
      const dirty = await gitOut(dir, ["status", "--porcelain"]);
      const lastCommit = await gitOut(dir, ["log", "-1", '--format=%h %cr: %s']);
      let ahead = await gitOut(dir, ["rev-list", "@{u}..HEAD", "--count"]);
      if (!ahead) ahead = "0";
      let behind = await gitOut(dir, ["rev-list", "HEAD..@{u}", "--count"]);
      if (!behind) behind = "0";
      const aheadN = parseInt(ahead, 10);
      const behindN = parseInt(behind, 10);
      statuses[repo] = {
        branch,
        dirty: dirty.length > 0,
        lastCommit: lastCommit || "unknown",
        ahead: Number.isFinite(aheadN) ? aheadN : 0,
        behind: Number.isFinite(behindN) ? behindN : 0,
      };
    }
    writeCache(db, "git:repo_statuses", statuses);

    const wtRoot = join(icnRoot, "..", "icn-wt");
    let dirs: string[] = [];
    try {
      dirs = readdirSync(wtRoot).filter((d) => !d.startsWith("."));
    } catch {
      dirs = [];
    }
    if (dirs.length > 0) {
      const worktrees: Record<string, unknown> = {};
      for (const name of dirs) {
        const wtDir = join(wtRoot, name, "icn");
        const branch = await gitOut(wtDir, ["rev-parse", "--abbrev-ref", "HEAD"]);
        if (!branch) continue;
        const behind = await gitOut(wtDir, [
          "rev-list",
          "--count",
          "HEAD..origin/main",
        ]);
        const n = behind ? parseInt(behind, 10) : NaN;
        worktrees[name] = {
          branch,
          behindMain: Number.isFinite(n) ? n : null,
        };
      }
      writeCache(db, "git:worktrees", worktrees);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.error("git poll failed:", message);
    try {
      writeCache(db, "git:repo_statuses", { error: message });
    } catch {
      // ignore secondary failures
    }
  }
}

export function startGitPolling(
  db: Database.Database,
  icnRoot: string
): NodeJS.Timeout {
  setImmediate(() => {
    void pollOnce(db, icnRoot).catch((e) => console.error("git poll async error:", e));
  });
  return setInterval(() => {
    void pollOnce(db, icnRoot).catch((e) => console.error("git poll async error:", e));
  }, INTERVAL_MS);
}
