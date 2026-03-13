// Process watcher tools — register interest in PID completion, check status.
// Replaces agent-side polling loops with server-side monitoring.

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";

export function registerWatcherTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "watch_process",
    "Register a process watcher. The server monitors the PID and emits an event when it exits. Call this instead of polling loops.",
    {
      session_id: z.string().describe("Your session ID"),
      pid: z.number().describe("Process ID to watch"),
      label: z.string().describe("Human-readable label (e.g. 'docker build icnd')"),
    },
    async ({ session_id, pid, label }) => {
      // Verify PID exists before registering
      try {
        process.kill(pid, 0);
      } catch {
        return {
          content: [{ type: "text", text: `Error: PID ${pid} not found` }],
          isError: true,
        };
      }

      db.prepare(
        "INSERT INTO watchers_process (session_id, pid, label, created_at, status) VALUES (?, ?, ?, ?, 'running')"
      ).run(session_id, pid, label, Date.now());

      return {
        content: [
          {
            type: "text",
            text: `Watching PID ${pid} (${label}). Check events for 'process.completed'.`,
          },
        ],
      };
    }
  );

  server.tool(
    "list_watchers",
    "List process watchers, optionally filtered by session.",
    {
      session_id: z.string().optional().describe("Filter by session ID"),
      status: z
        .enum(["running", "completed", "missing", "all"])
        .optional()
        .default("all")
        .describe("Filter by status"),
    },
    async ({ session_id, status }) => {
      const conditions: string[] = [];
      const params: unknown[] = [];

      if (session_id) {
        conditions.push("session_id = ?");
        params.push(session_id);
      }
      if (status !== "all") {
        conditions.push("status = ?");
        params.push(status);
      }

      const where =
        conditions.length > 0 ? `WHERE ${conditions.join(" AND ")}` : "";
      const watchers = db
        .prepare(`SELECT * FROM watchers_process ${where} ORDER BY id DESC`)
        .all(...params);

      return {
        content: [{ type: "text", text: JSON.stringify(watchers, null, 2) }],
      };
    }
  );
}
