import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { readFileSync } from "fs";
import { resolveOpsStatePath } from "../paths.js";
import { discoverWorktree, readBranchState } from "../runtime/worktree-identity.js";
import {
  activeSessionsForWorktree,
  ageMinutes,
  classifyWorktree,
  getSession,
  recordHeartbeat,
  recordInteraction,
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
      branch: z
        .string()
        .optional()
        .describe(
          "DEPRECATED and ignored: branch is live state read from Git, not a launch input. " +
            "The branch at registration is recorded automatically as history."
        ),
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
      agent_pid: z
        .number()
        .int()
        .positive()
        .optional()
        .describe(
          "Your process id, if you know it. Recorded as correlation metadata and used by " +
            "lifecycle classification to corroborate liveness without any caller cooperation. " +
            "Omitting it removes that safety net for this session."
        ),
      provider_session_id: z
        .string()
        .optional()
        .describe(
          "The harness conversation id (Claude Code's hook session_id). Registration is " +
            "idempotent on this within a live activation. It is stable across --resume, so " +
            "one conversation may have several activations over time — never two at once."
        ),
      cwd: z
        .string()
        .optional()
        .describe(
          "Directory to resolve the owning Git worktree from. Resolved through Git, so a " +
            "subdirectory works and a wrong path cannot misattribute the lane."
        ),
    },
    async (input) => {
      const identity = discoverWorktree(
        input.cwd ?? process.cwd(),
        process.env["ICN_ROOT"] ?? null
      );
      if (!identity) {
        return json({
          error: "no_worktree",
          reason: `${input.cwd ?? process.cwd()} does not resolve to a Git worktree`,
        });
      }
      try {
        return json(
          registerSession(db, {
            ...input,
            identity,
            branch_state: readBranchState(identity.worktree_path),
          })
        );
      } catch (e) {
        // A durable-id violation is an expected answer, not a protocol error: throwing here
        // surfaced it as a transport failure instead of a usable message.
        return json({ error: "registration_rejected", reason: (e as Error).message });
      }
    }
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

      // NOTE: no `state` predicate. v3 deliberately has no `state` column — release DELETEs
      // the row, so a row's EXISTENCE is liveness. An earlier draft of this query carried a
      // `state = 'active'` clause left over from a discarded design; it made every call throw
      // `no such column: state`, silently disabling advisory locking altogether.
      const checkStmt = db.prepare(
        `SELECT session_id FROM file_claims
         WHERE file_path = ? AND session_id != ?
         AND session_id IN (
           SELECT id FROM sessions
           WHERE julianday('now') - julianday(last_heartbeat) < ? / 1440.0
         )`
      );
      const insertStmt = db.prepare(
        "INSERT OR REPLACE INTO file_claims (file_path, session_id) VALUES (?, ?)"
      );

      for (const file of files) {
        if (checkStmt.get(file, session_id, ttl)) {
          conflicts.push(file);
        } else {
          try {
            insertStmt.run(file, session_id);
            claimed.push(file);
          } catch (e) {
            // A released/unknown session_id trips the foreign key. Every sibling tool answers
            // with structured JSON; leaking a raw SqliteError here made an ordinary
            // "your session is gone" indistinguishable from a server fault.
            return json({
              error: "unknown_session",
              session_id,
              reason:
                `cannot claim files for session ${session_id}: it is not registered ` +
                "(it may have been released). Re-register before claiming. " +
                `[${(e as Error).message}]`,
              claimed,
              conflicts,
            });
          }
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
        .enum(["file_edit", "command", "test", "task_state", "explicit"])
        .describe(
          "What kind of runtime event this progress represents. Note there is no 'turn': a " +
            "completed agent turn is interaction, not progress — use session_interaction."
        ),
      activity: z
        .string()
        .optional()
        .describe("Short human-readable current activity, e.g. 'cargo test -p icn-gateway'"),
    },
    async ({ session_id, kind, activity }) =>
      json({ ok: recordProgress(db, session_id, { kind, activity }), signal: "progress" })
  );

  server.tool(
    "session_interaction",
    "Report INTERACTION — a turn completed, a prompt arrived. Advances liveness only. A turn " +
      "boundary proves the harness responded, not that the work moved, so this must never " +
      "advance progress or a stalled agent could hide behind repeated empty turns.",
    { session_id: z.string() },
    async ({ session_id }) =>
      json({ ok: recordInteraction(db, session_id), signal: "interaction" })
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
      worktree_id: z
        .string()
        .optional()
        .describe("Canonical lane id: realpath of `git rev-parse --absolute-git-dir`."),
      path: z
        .string()
        .optional()
        .describe("Any path inside the worktree; the lane id is resolved from it via Git."),
      observed_pids: z
        .array(z.number().int().positive())
        .optional()
        .describe(
          "PIDs you observed holding the worktree. OMIT this if you did not actually look — " +
            "an omitted value means 'no observation performed' and keeps the lane protected. " +
            "Pass [] only as an affirmative 'I looked and found nothing'."
        ),
    },
    async ({ worktree_id, path, observed_pids }) => {
      const id = worktree_id ?? (path ? discoverWorktree(path, null)?.worktree_id : undefined);
      if (!id) {
        return json({
          state: "REGISTRY-UNAVAILABLE",
          live_agent_pids: [],
          contention: { count: 0, session_ids: [] },
          reason:
            "no canonical worktree id could be resolved, so no facts can be reported for this lane",
        });
      }
      return json(classifyWorktree(db, id, { observed_pids: observed_pids ?? null }));
    }
  );

  server.tool(
    "session_info",
    "Full record for one session, including its children.",
    { session_id: z.string() },
    async ({ session_id }) => {
      const session = getSession(db, session_id);
      if (!session) return json({ error: "not_found", session_id });
      const children = db
        .prepare(
          "SELECT id, worktree, worktree_id FROM sessions WHERE parent_session_id = ?"
        )
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
