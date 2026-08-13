import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { join } from "path";
import { resolveMonorepoRoot, resolveWorktreeRoot } from "../paths.js";
import { runCommand } from "../utils/commands.js";

const ICN_ROOT = resolveMonorepoRoot();

async function gitLine(cwd: string, args: readonly string[]): Promise<string> {
  const r = await runCommand("git", [...args], {
    cwd,
    timeoutMs: 10_000,
    maxStdoutBytes: 256 * 1024,
    maxStderrBytes: 32 * 1024,
  });
  return r.ok ? r.stdout.trim() : "";
}

/**
 * Resolve which branch ci_status should query. `git rev-parse --abbrev-ref HEAD`
 * prints "HEAD" on a detached checkout, and gitLine returns "" when git fails —
 * in both cases fall back to "main" so ci_status still returns useful runs
 * instead of an empty list.
 */
export function resolveBranch(rawBranch: string): string {
  const branch = rawBranch.trim();
  return branch === "" || branch === "HEAD" ? "main" : branch;
}

/**
 * Enumerate worktree directories under `wtRoot`.
 *
 * Returns the failure instead of swallowing it: an unreadable worktree root
 * previously collapsed to `[]`, which is indistinguishable from "no worktrees
 * exist" and reads as success. That is how a misconfigured root stayed
 * invisible while dozens of worktrees went unreported.
 */
export function readWorktreeDirs(
  wtRoot: string,
  readdir: (p: string) => string[]
): { dirs: string[]; error: string | null } {
  try {
    return { dirs: readdir(wtRoot).filter((d) => !d.startsWith(".")), error: null };
  } catch (e) {
    return {
      dirs: [],
      error: `Worktree root unreadable at ${wtRoot}: ${
        e instanceof Error ? e.message : String(e)
      }. Set ICN_WT_ROOT or fix ops/state/config/repo-map.json#worktrees.root.`,
    };
  }
}

async function repoStatus(repoPath: string, name: string) {
  const branch = await gitLine(repoPath, ["rev-parse", "--abbrev-ref", "HEAD"]);
  // An unresolvable path yields "" here and 0 for every counter below, which
  // renders as a clean, up-to-date repo. Report it as unresolved instead —
  // a repo we cannot see is not a repo that is fine.
  if (branch === "") {
    return {
      name,
      resolved: false,
      // Say what was observed, not a diagnosis. `gitLine` returns "" for any
      // failure — missing git, timeout, permissions, or genuinely not a repo —
      // so naming one cause would be asserting more than we checked, which is
      // the habit this whole change exists to remove.
      error: `Could not resolve a git branch at ${repoPath} (not a repository, or git failed there); treat this repo's status as unknown, not clean`,
      branch: null,
      dirty: null,
      dirtyFiles: null,
      ahead: null,
      behind: null,
      lastCommit: null,
    };
  }
  const dirty = await gitLine(repoPath, ["status", "--porcelain"]);
  let ahead = await gitLine(repoPath, ["rev-list", "@{u}..HEAD", "--count"]);
  if (!ahead) ahead = "0";
  let behind = await gitLine(repoPath, ["rev-list", "HEAD..@{u}", "--count"]);
  if (!behind) behind = "0";
  const lastCommit = await gitLine(repoPath, ["log", "-1", "--format=%s"]);
  const aheadN = parseInt(ahead, 10);
  const behindN = parseInt(behind, 10);
  return {
    name,
    resolved: true,
    error: null,
    branch,
    dirty: dirty !== "",
    dirtyFiles: dirty.split("\n").filter(Boolean).length,
    ahead: Number.isFinite(aheadN) ? aheadN : 0,
    behind: Number.isFinite(behindN) ? behindN : 0,
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
        { name: "icn", path: ICN_ROOT },
        { name: "homelab-inventory", path: join(ICN_ROOT, "..", "homelab-inventory") },
      ];
      const results = await Promise.all(
        repos.map((r) => repoStatus(r.path, r.name))
      );
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
      const wtRoot = resolveWorktreeRoot();
      const icnPath = join(ICN_ROOT, "icn");
      const mainHash = await gitLine(icnPath, ["rev-parse", "origin/main"]);

      const { readdirSync } = await import("fs");
      const { dirs, error: wtRootError } = readWorktreeDirs(wtRoot, (p) =>
        readdirSync(p)
      );

      const results = await Promise.all(
        dirs.map(async (dir) => {
          const wtPath = join(wtRoot, dir, "icn");
          const branch = await gitLine(wtPath, ["rev-parse", "--abbrev-ref", "HEAD"]);
          const lastCommit = await gitLine(wtPath, ["log", "-1", "--format=%s"]);
          const behind =
            mainHash && branch
              ? await gitLine(wtPath, ["rev-list", "--count", `HEAD..${mainHash}`])
              : "";
          const behindN = behind ? parseInt(behind, 10) : 0;
          return {
            name: dir,
            branch,
            lastCommit,
            behindMain: Number.isFinite(behindN) ? behindN : 0,
            stale: Number.isFinite(behindN) && behindN > 10,
          };
        })
      );

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
              {
                worktreeRoot: wtRoot,
                error: wtRootError,
                worktrees: results.map((r) => ({
                  ...r,
                  claimedBy: sessionMap[r.name] ?? null,
                })),
              },
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
        .describe("Repo name: icn, homelab-inventory"),
      branch: z.string().optional().describe("Branch name, defaults to current"),
    },
    async ({ repo, branch }) => {
      try {
        const repoPath = join(ICN_ROOT, repo);
        const targetBranch =
          branch ?? resolveBranch(await gitLine(repoPath, ["rev-parse", "--abbrev-ref", "HEAD"]));
        const r = await runCommand(
          "gh",
          [
            "run",
            "list",
            "--repo",
            `InterCooperative-Network/${repo}`,
            "--branch",
            targetBranch,
            "--limit",
            "3",
            "--json",
            "status,conclusion,name,createdAt",
          ],
          { cwd: ICN_ROOT, timeoutMs: 15_000, maxStdoutBytes: 512 * 1024 }
        );
        if (!r.ok) {
          return {
            content: [
              {
                type: "text",
                text: JSON.stringify({
                  error: "gh_failed",
                  stderr: r.stderr,
                  exitCode: r.exitCode,
                  timedOut: r.timedOut,
                }),
              },
            ],
            isError: true,
          };
        }
        return {
          content: [{ type: "text", text: r.stdout }],
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
        const r = await runCommand(
          "gh",
          [
            "pr",
            "list",
            "--repo",
            `InterCooperative-Network/${repo}`,
            "--json",
            "number,title,state,reviewDecision,headRefName,createdAt",
            "--limit",
            "10",
          ],
          { cwd: ICN_ROOT, timeoutMs: 15_000, maxStdoutBytes: 512 * 1024 }
        );
        if (!r.ok) {
          return {
            content: [
              {
                type: "text",
                text: JSON.stringify({
                  error: "gh_failed",
                  stderr: r.stderr,
                  exitCode: r.exitCode,
                  timedOut: r.timedOut,
                }),
              },
            ],
            isError: true,
          };
        }
        return {
          content: [{ type: "text", text: r.stdout }],
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
