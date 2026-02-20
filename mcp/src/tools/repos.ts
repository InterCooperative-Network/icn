import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { execSync } from "child_process";
import { readFileSync } from "fs";
import { join } from "path";

const ICN_ROOT = process.env["ICN_ROOT"] ?? "/home/ubuntu/projects";

function runGit(cwd: string, args: string): string {
  try {
    return execSync(`git ${args}`, { cwd, encoding: "utf-8", timeout: 10000 }).trim();
  } catch {
    return "";
  }
}

function repoStatus(repoPath: string, name: string) {
  const branch = runGit(repoPath, "rev-parse --abbrev-ref HEAD");
  const dirty = runGit(repoPath, "status --porcelain");
  const ahead = runGit(repoPath, "rev-list @{u}..HEAD --count 2>/dev/null || echo 0");
  const behind = runGit(repoPath, "rev-list HEAD..@{u} --count 2>/dev/null || echo 0");
  const lastCommit = runGit(repoPath, "log -1 --format=%s");
  return {
    name,
    branch,
    dirty: dirty !== "",
    dirtyFiles: dirty.split("\n").filter(Boolean).length,
    ahead: parseInt(ahead) || 0,
    behind: parseInt(behind) || 0,
    lastCommit,
  };
}

export function registerRepoTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "repo_status",
    "Get current branch, dirty files, and sync status for all ICN repos.",
    {},
    async () => {
      const repos = [
        { name: "icn", path: join(ICN_ROOT, "icn") },
        { name: "icn-website", path: join(ICN_ROOT, "icn-website") },
        { name: "icn-ops", path: join(ICN_ROOT, "icn-ops") },
        { name: "homelab-inventory", path: join(ICN_ROOT, "homelab-inventory") },
      ];
      const results = repos.map((r) => repoStatus(r.path, r.name));
      return {
        content: [{ type: "text", text: JSON.stringify(results, null, 2) }],
      };
    }
  );

  server.tool(
    "worktree_status",
    "List all git worktrees with branch, last commit, staleness vs main.",
    {},
    async () => {
      const wtRoot = join(ICN_ROOT, "icn-wt");
      const icnPath = join(ICN_ROOT, "icn");
      const mainHash = runGit(icnPath, "rev-parse origin/main");

      let dirs: string[] = [];
      try {
        const { readdirSync } = await import("fs");
        dirs = readdirSync(wtRoot).filter(
          (d) => !d.startsWith(".")
        );
      } catch {
        dirs = [];
      }

      const results = dirs.map((dir) => {
        const wtPath = join(wtRoot, dir, "icn");
        const branch = runGit(wtPath, "rev-parse --abbrev-ref HEAD");
        const hash = runGit(wtPath, "rev-parse HEAD");
        const lastCommit = runGit(wtPath, "log -1 --format=%s");
        const behind = runGit(
          wtPath,
          `rev-list HEAD..${mainHash} --count`
        );
        return {
          name: dir,
          branch,
          lastCommit,
          behindMain: parseInt(behind) || 0,
          stale: parseInt(behind) > 10,
        };
      });

      // Also check if the worktree has any claimed sessions
      const activeSessions = db
        .prepare(
          `SELECT worktree, task_description FROM sessions
           WHERE worktree IS NOT NULL
           AND datetime(last_heartbeat) > datetime('now', '-30 minutes')`
        )
        .all() as Array<{ worktree: string; task_description: string }>;

      const sessionMap: Record<string, string> = {};
      for (const s of activeSessions) {
        sessionMap[s.worktree] = s.task_description;
      }

      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(
              results.map((r) => ({
                ...r,
                claimedBy: sessionMap[r.name] ?? null,
              })),
              null,
              2
            ),
          },
        ],
      };
    }
  );

  server.tool(
    "ci_status",
    "Get latest CI run results for active branches (uses gh CLI).",
    {
      repo: z
        .string()
        .optional()
        .default("icn")
        .describe("Repo name: icn, icn-website, icn-ops"),
      branch: z.string().optional().describe("Branch name, defaults to current"),
    },
    async ({ repo, branch }) => {
      try {
        const repoPath = join(ICN_ROOT, repo);
        const targetBranch =
          branch ?? runGit(repoPath, "rev-parse --abbrev-ref HEAD");
        const result = execSync(
          `gh run list --repo InterCooperative-Network/${repo} --branch ${targetBranch} --limit 3 --json status,conclusion,name,createdAt`,
          { encoding: "utf-8", timeout: 15000 }
        ).trim();
        return {
          content: [{ type: "text", text: result }],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text",
              text: `gh CLI error: ${err instanceof Error ? err.message : String(err)}`,
            },
          ],
          isError: true,
        };
      }
    }
  );

  server.tool(
    "pr_status",
    "List open PRs with review status (uses gh CLI).",
    {
      repo: z.string().optional().default("icn"),
    },
    async ({ repo }) => {
      try {
        const result = execSync(
          `gh pr list --repo InterCooperative-Network/${repo} --json number,title,state,reviewDecision,headRefName,createdAt --limit 10`,
          { encoding: "utf-8", timeout: 15000 }
        ).trim();
        return {
          content: [{ type: "text", text: result }],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text",
              text: `gh CLI error: ${err instanceof Error ? err.message : String(err)}`,
            },
          ],
          isError: true,
        };
      }
    }
  );
}
