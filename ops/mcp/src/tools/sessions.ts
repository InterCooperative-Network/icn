import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { readFileSync } from "fs";
import { resolveOpsStatePath } from "../paths.js";
import {
  activeSessionsForWorktree,
  ageMinutes,
  classifyWorktree,
  getSession,
  recordHeartbeat,
  recordProgress,
  registerSession,
  releaseSession,
  stallMinutes,
  ttlMinutes,
} from "../runtime/session-runtime.js";

const SESSION_LOG = resolveOpsStatePath("session-log.jsonl");

function json(value: unknown) {
  return { content: [{ type: "text" as const, text: JSON.stringify(value, null, 2) }] };
}

export function registerSessionTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "register_session",
    "Register an agent session. Normally called automatically by the SessionStart hook " +
      "(ops/scripts/icn-agent-session); call it manually only from a launcher that has no hooks.",
    {
      repo: z.string().describe("Repo name: icn, homelab-inventory"),
      worktree: z
        .string()
        .optional()
        .describe(
          "Worktree name under the configured worktree root (repo-map.json#worktrees.root; e.g. task-preflight-hardening)"
        ),
      task_description: z
        .string()
        .optional()
        .describe("Brief description of what this session is working on"),
      branch: z.string().optional().describe("Branch at registration time"),
      task_ref: z.string().optional().describe("Issue reference, e.g. icn#2653"),
      pr_ref: z.string().optional().describe("PR reference, e.g. icn#2660"),
      parent_session_id: z
        .string()
        .optional()
        .describe("Parent session ID when this is a child/review session"),
      provider: z
        .string()
        .optional()
        .describe("Agent provider/harness, e.g. claude-code, codex, cursor"),
      harness_key: z
        .string()
        .optional()
        .describe(
          "Stable per-harness session key. Registration is idempotent on this value: " +
            "re-registering returns the existing session instead of creating a duplicate."
        ),
    },
    async (input) => json(registerSession(db, input)),
  );

  server.tool(
    "list_sessions",
    "List agent sessions with lifecycle classification and file claims.",
    {
      include_expired: z
        .boolean()
        .optional()
        .default(false)
        .describe("Include sessions whose heartbeat is older than the TTL"),
    },
    async ({ include_expired }) => {
      const ttl = ttlMinutes();
      const rows = db
        .prepare(
          `SELECT s.*, GROUP_CONCAT(fc.file_path) as claimed_files
             FROM sessions s
             LEFT JOIN file_claims fc ON fc.session_id = s.id
            WHERE s.state = 'active'
            GROUP BY s.id
            ORDER BY s.started_at DESC`
        )
        .all() as Array<Record<string, unknown>>;

      const enriched = rows
        .map((r) => {
          const hb = ageMinutes(db, r["last_heartbeat"] as string);
          const pa = ageMinutes(db, r["last_progress"] as string | null);
          return {
            ...r,
            heartbeat_age_min: hb == null ? null : Number(hb.toFixed(1)),
            progress_age_min: pa == null ? null : Number(pa.toFixed(1)),
            expired: hb != null && hb > ttl,
          };
        })
        .filter((r) => include_expired || !r.expired);

      return json({
        ttl_minutes: ttl,
        stall_minutes: stallMinutes(),
        sessions: enriched,
      });
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
      const ttl = ttlMinutes();

      const checkStmt = db.prepare(
        `SELECT session_id FROM file_claims
         WHERE file_path = ? AND session_id != ?
         AND session_id IN (
           SELECT id FROM sessions
           WHERE state = 'active'
             AND julianday('now') - julianday(last_heartbeat) < ? / 1440.0
         )`
      );
      const insertStmt = db.prepare(
        "INSERT OR REPLACE INTO file_claims (file_path, session_id) VALUES (?, ?)"
      );

      for (const file of files) {
        if (checkStmt.get(file, session_id, ttl)) {
          conflicts.push(file);
        } else {
          insertStmt.run(file, session_id);
          claimed.push(file);
        }
      }

      return json({ claimed, conflicts });
    }
  );

  server.tool(
    "heartbeat",
    "Report LIVENESS only — 'the harness is still running'. This is deliberately weak " +
      "evidence and never implies progress. To report that work happened, call session_progress.",
    { session_id: z.string() },
    async ({ session_id }) =>
      json({ ok: recordHeartbeat(db, session_id), signal: "liveness" })
  );

  server.tool(
    "session_progress",
    "Report that meaningful work happened (file edit, completed command, test run, turn " +
      "boundary). Advances last_progress and the monotonic progress_count, which is what " +
      "distinguishes a working session from a spinning one.",
    {
      session_id: z.string(),
      kind: z
        .enum(["file_edit", "command", "turn", "test", "task_state", "explicit"])
        .describe("What kind of runtime event this progress represents"),
      activity: z
        .string()
        .optional()
        .describe("Short human-readable current activity, e.g. 'cargo test -p icn-gateway'"),
    },
    async ({ session_id, kind, activity }) =>
      json({ ok: recordProgress(db, session_id, { kind, activity }), signal: "progress" })
  );

  server.tool(
    "release_session",
    "Sign off and release all file claims. Normally called automatically by the SessionEnd hook.",
    {
      session_id: z.string(),
      reason: z
        .string()
        .optional()
        .describe("Why the session ended: completed, cancelled, error, shutdown"),
    },
    async ({ session_id, reason }) => json(releaseSession(db, session_id, { reason }))
  );

  server.tool(
    "session_lifecycle",
    "Authoritative lifecycle classification for a worktree, combining the session registry " +
      "with process observation. Absence of a registry row NEVER means 'safe to terminate'.",
    {
      worktree: z.string().describe("Worktree directory name"),
      observed_pids: z
        .array(z.number())
        .optional()
        .default([])
        .describe("PIDs observed holding the worktree (corroborating evidence)"),
    },
    async ({ worktree, observed_pids }) =>
      json(classifyWorktree(db, worktree, { observed_pids }))
  );

  server.tool(
    "session_info",
    "Full record for one session, including its children.",
    { session_id: z.string() },
    async ({ session_id }) => {
      const session = getSession(db, session_id);
      if (!session) return json({ error: "not_found", session_id });
      const children = db
        .prepare("SELECT id, worktree, state FROM sessions WHERE parent_session_id = ?")
        .all(session_id);
      return json({
        ...session,
        heartbeat_age_min: ageMinutes(db, session.last_heartbeat),
        progress_age_min: ageMinutes(db, session.last_progress),
        children,
      });
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
        const lines = readFileSync(SESSION_LOG, "utf-8")
          .trim()
          .split("\n")
          .filter(Boolean);
        return json(lines.slice(-count).reverse().map((l) => JSON.parse(l) as unknown));
      } catch {
        return { content: [{ type: "text" as const, text: "No session history yet." }] };
      }
    }
  );
}

export { activeSessionsForWorktree };
