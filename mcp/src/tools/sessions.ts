import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { randomUUID } from "crypto";
import { appendFileSync, readFileSync } from "fs";
import { join } from "path";

const ICN_ROOT = process.env["ICN_ROOT"] ?? "/home/ubuntu/projects";
const SESSION_LOG = join(ICN_ROOT, "icn-ops/state/session-log.jsonl");

export function registerSessionTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "register_session",
    "Register an agent session to track what it's working on. Call at session start.",
    {
      repo: z
        .string()
        .describe(
          "Repo name: icn, icn-website, icn-ops, homelab-inventory"
        ),
      worktree: z
        .string()
        .optional()
        .describe("Worktree name if working in icn-wt/ (e.g. 1084-names-gateway-a)"),
      task_description: z
        .string()
        .optional()
        .describe("Brief description of what this session is working on"),
    },
    async ({ repo, worktree, task_description }) => {
      const id = randomUUID();
      db.prepare(
        "INSERT INTO sessions (id, repo, worktree, task_description) VALUES (?, ?, ?, ?)"
      ).run(id, repo, worktree ?? null, task_description ?? null);
      return {
        content: [
          {
            type: "text",
            text: JSON.stringify({ session_id: id, repo, worktree }),
          },
        ],
      };
    }
  );

  server.tool(
    "list_sessions",
    "List all active agent sessions and their file claims.",
    {},
    async () => {
      const sessions = db
        .prepare(
          `SELECT s.*, GROUP_CONCAT(fc.file_path) as claimed_files
           FROM sessions s
           LEFT JOIN file_claims fc ON fc.session_id = s.id
           WHERE datetime(s.last_heartbeat) > datetime('now', '-30 minutes')
           GROUP BY s.id
           ORDER BY s.started_at DESC`
        )
        .all();
      return {
        content: [{ type: "text", text: JSON.stringify(sessions, null, 2) }],
      };
    }
  );

  server.tool(
    "claim_files",
    "Advisory lock on files to signal intent to edit. Prevents concurrent edits across agents.",
    {
      session_id: z.string().describe("Your session ID from register_session"),
      files: z
        .array(z.string())
        .describe("File paths to claim (relative to repo root)"),
    },
    async ({ session_id, files }) => {
      const conflicts: string[] = [];
      const claimed: string[] = [];

      const checkStmt = db.prepare(
        `SELECT session_id FROM file_claims
         WHERE file_path = ? AND session_id != ?
         AND session_id IN (
           SELECT id FROM sessions
           WHERE datetime(last_heartbeat) > datetime('now', '-30 minutes')
         )`
      );
      const insertStmt = db.prepare(
        "INSERT OR REPLACE INTO file_claims (file_path, session_id) VALUES (?, ?)"
      );

      for (const file of files) {
        const existing = checkStmt.get(file, session_id);
        if (existing) {
          conflicts.push(file);
        } else {
          insertStmt.run(file, session_id);
          claimed.push(file);
        }
      }

      return {
        content: [
          { type: "text", text: JSON.stringify({ claimed, conflicts }) },
        ],
      };
    }
  );

  server.tool(
    "heartbeat",
    "Update session last_heartbeat to keep it active. Call periodically during long operations.",
    {
      session_id: z.string(),
    },
    async ({ session_id }) => {
      db.prepare(
        "UPDATE sessions SET last_heartbeat = datetime('now') WHERE id = ?"
      ).run(session_id);
      return {
        content: [{ type: "text", text: "ok" }],
      };
    }
  );

  server.tool(
    "release_session",
    "Sign off and release all file claims. Call at session end.",
    {
      session_id: z.string(),
    },
    async ({ session_id }) => {
      // Capture before deletion — file_claims cascade-delete with the session row
      const session = db
        .prepare("SELECT * FROM sessions WHERE id = ?")
        .get(session_id) as Record<string, unknown> | undefined;

      if (session) {
        const files = (
          db
            .prepare("SELECT file_path FROM file_claims WHERE session_id = ?")
            .all(session_id) as Array<{ file_path: string }>
        ).map((r) => r.file_path);

        const entry = {
          ...session,
          released_at: new Date().toISOString(),
          files_touched: files,
          duration_minutes: Math.round(
            (Date.now() - new Date(session["started_at"] as string).getTime()) / 60000
          ),
        };
        try {
          appendFileSync(SESSION_LOG, JSON.stringify(entry) + "\n");
        } catch {
          // Non-fatal — log failure shouldn't block session release
        }
      }

      db.prepare("DELETE FROM file_claims WHERE session_id = ?").run(session_id);
      db.prepare("DELETE FROM sessions WHERE id = ?").run(session_id);
      return {
        content: [{ type: "text", text: "session released" }],
      };
    }
  );

  server.tool(
    "recent_sessions",
    "Show recently completed sessions with what they worked on.",
    {
      count: z
        .number()
        .optional()
        .default(10)
        .describe("Number of recent sessions to return"),
    },
    async ({ count }) => {
      try {
        const content = readFileSync(SESSION_LOG, "utf-8");
        const lines = content.trim().split("\n").filter(Boolean);
        const recent = lines
          .slice(-count)
          .reverse()
          .map((l) => JSON.parse(l) as unknown);
        return {
          content: [{ type: "text", text: JSON.stringify(recent, null, 2) }],
        };
      } catch {
        return {
          content: [{ type: "text", text: "No session history yet." }],
        };
      }
    }
  );
}
