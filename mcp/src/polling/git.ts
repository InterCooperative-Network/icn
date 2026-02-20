// Background git status polling — 30s interval
// Pre-warms health_cache so repo_status/worktree_status tool calls return instantly.

import type Database from "better-sqlite3";
import { execSync } from "child_process";

const INTERVAL_MS = 30_000;

function runCmd(cmd: string): string | null {
  try {
    return execSync(cmd, { encoding: "utf-8", timeout: 15_000 }).trim();
  } catch {
    return null;
  }
}

function writeCache(db: Database.Database, key: string, value: unknown): void {
  db.prepare(
    "INSERT OR REPLACE INTO health_cache (key, value, polled_at) VALUES (?, ?, datetime('now'))"
  ).run(key, JSON.stringify(value));
}

function pollOnce(db: Database.Database, icnRoot: string): void {
  const repos = ["icn", "icn-website", "icn-ops"];

  const statuses: Record<string, unknown> = {};
  for (const repo of repos) {
    const dir = `${icnRoot}/${repo}`;
    const branch = runCmd(`git -C ${dir} rev-parse --abbrev-ref HEAD 2>/dev/null`);
    if (!branch) {
      statuses[repo] = { error: "not a git repo or not found" };
      continue;
    }
    const dirty = runCmd(`git -C ${dir} status --porcelain 2>/dev/null`);
    const lastCommit = runCmd(
      `git -C ${dir} log -1 --format="%h %cr: %s" 2>/dev/null`
    );
    statuses[repo] = {
      branch,
      dirty: dirty !== null && dirty.length > 0,
      lastCommit: lastCommit ?? "unknown",
    };
  }
  writeCache(db, "git:repo_statuses", statuses);

  // Worktree lag vs origin/main for icn repo
  const wtRoot = `${icnRoot}/icn-wt`;
  const wtList = runCmd(`ls ${wtRoot} 2>/dev/null`);
  if (wtList) {
    const worktrees: Record<string, unknown> = {};
    for (const name of wtList.split("\n").filter(Boolean)) {
      const wtDir = `${wtRoot}/${name}/icn`;
      const branch = runCmd(`git -C ${wtDir} rev-parse --abbrev-ref HEAD 2>/dev/null`);
      if (!branch) continue;
      const behind = runCmd(
        `git -C ${wtDir} rev-list --count HEAD..origin/main 2>/dev/null`
      );
      worktrees[name] = {
        branch,
        behindMain: behind ? parseInt(behind, 10) : null,
      };
    }
    writeCache(db, "git:worktrees", worktrees);
  }
}

export function startGitPolling(
  db: Database.Database,
  icnRoot: string
): NodeJS.Timeout {
  // First poll immediately (fire and forget — don't block server startup)
  setImmediate(() => pollOnce(db, icnRoot));

  return setInterval(() => pollOnce(db, icnRoot), INTERVAL_MS);
}
