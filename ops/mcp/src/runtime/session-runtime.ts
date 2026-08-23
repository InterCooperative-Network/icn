// Agent session runtime — the single implementation of ICN session lifecycle.
//
// Both surfaces call this module and nothing else:
//   - the MCP tools in ../tools/sessions.ts (used by a model that decides to call them)
//   - the CLI in ../cli/session.ts        (used by Claude Code hooks, which cannot call MCP)
//
// There is deliberately no second implementation: a hook and a tool that disagree about what
// "registered" means is the drift this layer exists to remove.
//
// Refs docs/architecture/AGENT_RUNTIME.md.

import type Database from "better-sqlite3";
import { randomUUID } from "crypto";
import { appendFileSync } from "fs";
import { resolveOpsStatePath } from "../paths.js";

const SESSION_LOG = resolveOpsStatePath("session-log.jsonl");

/** Heartbeat age past which a session is no longer credibly live. */
export const DEFAULT_TTL_MINUTES = 30;
/** Progress age past which a heartbeating session is "moving but not progressing". */
export const DEFAULT_STALL_MINUTES = 90;

export function ttlMinutes(env: NodeJS.ProcessEnv = process.env): number {
  return positiveIntOr(env["ICN_SESSION_TTL_MINUTES"], DEFAULT_TTL_MINUTES);
}
export function stallMinutes(env: NodeJS.ProcessEnv = process.env): number {
  return positiveIntOr(env["ICN_SESSION_STALL_MINUTES"], DEFAULT_STALL_MINUTES);
}
function positiveIntOr(raw: string | undefined, fallback: number): number {
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : fallback;
}

/**
 * Lifecycle verdict for a worktree.
 *
 * The vocabulary is deliberately explicit about *why* a lane is protected, because "no row"
 * and "row says idle" are different facts with different safe actions.
 */
export type LifecycleState =
  | "REGISTERED-ACTIVE"
  | "PROGRESS-STALLED"
  | "REGISTERED-EXPIRED"
  | "UNREGISTERED-OBSERVED"
  | "REGISTRY-UNAVAILABLE";

export type SessionRow = {
  id: string;
  repo: string;
  worktree: string | null;
  branch: string | null;
  task_description: string | null;
  task_ref: string | null;
  pr_ref: string | null;
  parent_session_id: string | null;
  provider: string | null;
  agent_pid: number | null;
  host: string | null;
  harness_key: string | null;
  state: string;
  current_activity: string | null;
  progress_count: number;
  started_at: string;
  last_heartbeat: string;
  last_progress: string | null;
};

export type RegisterInput = {
  repo: string;
  worktree?: string | null;
  branch?: string | null;
  task_description?: string | null;
  task_ref?: string | null;
  pr_ref?: string | null;
  parent_session_id?: string | null;
  provider?: string | null;
  agent_pid?: number | null;
  host?: string | null;
  /**
   * Stable per-harness-session key (Claude Code's session id, else `pid@host`).
   * Registration is idempotent on this: a hook that fires twice must not create two rows.
   */
  harness_key?: string | null;
};

export type RegisterResult = {
  session_id: string;
  created: boolean;
  /** True when an existing row was returned instead of a new one. */
  deduplicated: boolean;
};

// ── writes ───────────────────────────────────────────────────────────────────

export function registerSession(
  db: Database.Database,
  input: RegisterInput
): RegisterResult {
  const key = input.harness_key ?? null;

  if (key) {
    const existing = db
      .prepare(
        "SELECT id FROM sessions WHERE harness_key = ? AND state = 'active' LIMIT 1"
      )
      .get(key) as { id: string } | undefined;
    if (existing) {
      // Idempotent path. Refresh the mutable launch facts (a resumed session may have moved
      // branch or learned its PR) but keep the identity and the progress history.
      db.prepare(
        `UPDATE sessions
            SET branch = COALESCE(?, branch),
                task_ref = COALESCE(?, task_ref),
                pr_ref = COALESCE(?, pr_ref),
                task_description = COALESCE(?, task_description),
                agent_pid = COALESCE(?, agent_pid),
                last_heartbeat = datetime('now')
          WHERE id = ?`
      ).run(
        input.branch ?? null,
        input.task_ref ?? null,
        input.pr_ref ?? null,
        input.task_description ?? null,
        input.agent_pid ?? null,
        existing.id
      );
      return { session_id: existing.id, created: false, deduplicated: true };
    }
  }

  const id = randomUUID();
  db.prepare(
    `INSERT INTO sessions
       (id, repo, worktree, branch, task_description, task_ref, pr_ref,
        parent_session_id, provider, agent_pid, host, harness_key, state)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active')`
  ).run(
    id,
    input.repo,
    input.worktree ?? null,
    input.branch ?? null,
    input.task_description ?? null,
    input.task_ref ?? null,
    input.pr_ref ?? null,
    input.parent_session_id ?? null,
    input.provider ?? null,
    input.agent_pid ?? null,
    input.host ?? null,
    key
  );
  return { session_id: id, created: true, deduplicated: false };
}

/**
 * Liveness only. Advances `last_heartbeat` and nothing else.
 *
 * This is intentionally weak evidence: a wait loop can call it forever. Progress is a
 * separate, stronger claim — see recordProgress.
 */
export function recordHeartbeat(db: Database.Database, sessionId: string): boolean {
  const r = db
    .prepare(
      "UPDATE sessions SET last_heartbeat = datetime('now') WHERE id = ? AND state = 'active'"
    )
    .run(sessionId);
  return r.changes > 0;
}

/** Runtime events that count as evidence that work actually happened. */
export type ProgressKind =
  | "file_edit"
  | "command"
  | "turn"
  | "test"
  | "task_state"
  | "explicit";

/**
 * Progress. Advances `last_progress` AND the monotonic `progress_count`, so a consumer can
 * sample the counter twice and prove motion without trusting wall-clock timestamps.
 */
export function recordProgress(
  db: Database.Database,
  sessionId: string,
  opts: { kind: ProgressKind; activity?: string | null } 
): boolean {
  const r = db
    .prepare(
      `UPDATE sessions
          SET last_heartbeat = datetime('now'),
              last_progress  = datetime('now'),
              progress_count = progress_count + 1,
              current_activity = COALESCE(?, current_activity)
        WHERE id = ? AND state = 'active'`
    )
    .run(opts.activity ?? null, sessionId);
  if (r.changes > 0) {
    void opts.kind; // kind is recorded on the event bus by callers that want an audit trail
    return true;
  }
  return false;
}

export type ReleaseResult = {
  released: boolean;
  /** Child sessions still active at release time; they are NOT cascaded (see AGENT_RUNTIME §6). */
  orphaned_children: string[];
};

export function releaseSession(
  db: Database.Database,
  sessionId: string,
  opts: { reason?: string } = {}
): ReleaseResult {
  const session = db
    .prepare("SELECT * FROM sessions WHERE id = ?")
    .get(sessionId) as SessionRow | undefined;

  if (!session) return { released: false, orphaned_children: [] };

  const children = (
    db
      .prepare(
        "SELECT id FROM sessions WHERE parent_session_id = ? AND state = 'active'"
      )
      .all(sessionId) as Array<{ id: string }>
  ).map((r) => r.id);

  const files = (
    db
      .prepare("SELECT file_path FROM file_claims WHERE session_id = ?")
      .all(sessionId) as Array<{ file_path: string }>
  ).map((r) => r.file_path);

  try {
    appendFileSync(
      SESSION_LOG,
      JSON.stringify({
        ...session,
        released_at: new Date().toISOString(),
        release_reason: opts.reason ?? "unspecified",
        files_touched: files,
        orphaned_children: children,
        duration_minutes: Math.round(
          (Date.now() - new Date(session.started_at + "Z").getTime()) / 60000
        ),
      }) + "\n"
    );
  } catch {
    // Non-fatal: an unwritable log must not block releasing the session.
  }

  db.prepare("DELETE FROM file_claims WHERE session_id = ?").run(sessionId);
  db.prepare("DELETE FROM sessions WHERE id = ?").run(sessionId);
  return { released: true, orphaned_children: children };
}

// ── reads ────────────────────────────────────────────────────────────────────

export function getSession(
  db: Database.Database,
  sessionId: string
): SessionRow | undefined {
  return db.prepare("SELECT * FROM sessions WHERE id = ?").get(sessionId) as
    | SessionRow
    | undefined;
}

export function activeSessionsForWorktree(
  db: Database.Database,
  worktree: string
): SessionRow[] {
  return db
    .prepare(
      "SELECT * FROM sessions WHERE worktree = ? AND state = 'active' ORDER BY started_at DESC"
    )
    .all(worktree) as SessionRow[];
}

/** Age in minutes of an SQLite `datetime('now')` timestamp, or null when absent. */
export function ageMinutes(
  db: Database.Database,
  ts: string | null | undefined
): number | null {
  if (!ts) return null;
  const row = db
    .prepare("SELECT (julianday('now') - julianday(?)) * 1440.0 AS m")
    .get(ts) as { m: number | null };
  return row.m == null ? null : row.m;
}

// ── classification ───────────────────────────────────────────────────────────

export type ClassifyObservation = {
  /** PIDs observed holding the worktree. Corroborating evidence only. */
  observed_pids: number[];
  /** True when the registry could not be opened/read at all. */
  registry_unavailable?: boolean;
};

export type Classification = {
  state: LifecycleState;
  /** Whether a consumer may consider retiring this lane WITHOUT operator approval. */
  retireable: boolean;
  /** Whether retirement is defensible but must be operator-approved. */
  retireable_with_approval: boolean;
  reason: string;
  session_id: string | null;
  heartbeat_age_min: number | null;
  progress_age_min: number | null;
  progress_count: number | null;
};

/**
 * Classify a worktree from registry state plus process observation.
 *
 * FAIL-SAFE INVARIANT (asserted by tests, not by convention):
 *   absence of a registry row NEVER yields retireable=true.
 * A missing row is indistinguishable from a pre-integration session, an unsupported
 * launcher, or a registry failure — all of which must stay protected.
 */
export function classify(
  sessions: SessionRow[],
  obs: ClassifyObservation,
  ages: { heartbeat_age_min: number | null; progress_age_min: number | null },
  limits: { ttl_min: number; stall_min: number }
): Classification {
  const base = {
    retireable: false,
    retireable_with_approval: false,
    session_id: null as string | null,
    heartbeat_age_min: null as number | null,
    progress_age_min: null as number | null,
    progress_count: null as number | null,
  };

  if (obs.registry_unavailable) {
    return {
      ...base,
      state: "REGISTRY-UNAVAILABLE",
      reason:
        "session registry could not be read; lifecycle state is unknown, so the lane is protected",
    };
  }

  if (sessions.length === 0) {
    return {
      ...base,
      state: "UNREGISTERED-OBSERVED",
      reason:
        obs.observed_pids.length > 0
          ? `no registry row, but ${obs.observed_pids.length} process(es) hold the worktree; ` +
            "may be a pre-integration session or an unsupported launcher — protected"
          : "no registry row and no observed process; nothing authoritative to act on — protected",
    };
  }

  const s = sessions[0]!;
  const shared = {
    session_id: s.id,
    heartbeat_age_min: ages.heartbeat_age_min,
    progress_age_min: ages.progress_age_min,
    progress_count: s.progress_count,
  };

  const hb = ages.heartbeat_age_min;
  const heartbeatExpired = hb != null && hb > limits.ttl_min;

  if (heartbeatExpired) {
    // Expired heartbeat + a live process is still a *process-pinned* lane. The process is the
    // stronger fact here, so it downgrades the verdict rather than being ignored.
    if (obs.observed_pids.length > 0) {
      return {
        ...shared,
        state: "PROGRESS-STALLED",
        retireable: false,
        retireable_with_approval: true,
        reason:
          `heartbeat is ${fmt(hb)} min old (TTL ${limits.ttl_min}) but ` +
          `${obs.observed_pids.length} process(es) still hold the worktree`,
      };
    }
    return {
      ...shared,
      state: "REGISTERED-EXPIRED",
      retireable: true,
      retireable_with_approval: true,
      reason: `heartbeat is ${fmt(hb)} min old (TTL ${limits.ttl_min}) and no process holds the worktree`,
    };
  }

  const pa = ages.progress_age_min;
  // Never progressed at all: fall back to how long the session has been heartbeating.
  const effectiveProgressAge = pa ?? hb;
  if (
    effectiveProgressAge != null &&
    effectiveProgressAge > limits.stall_min
  ) {
    return {
      ...shared,
      state: "PROGRESS-STALLED",
      retireable: false,
      retireable_with_approval: true,
      reason:
        `heartbeat is fresh (${fmt(hb)} min) but no progress for ${fmt(effectiveProgressAge)} min ` +
        `(stall window ${limits.stall_min}); activity without progress`,
    };
  }

  return {
    ...shared,
    state: "REGISTERED-ACTIVE",
    retireable: false,
    retireable_with_approval: false,
    reason: `heartbeat ${fmt(hb)} min, progress ${fmt(effectiveProgressAge)} min, count ${s.progress_count}`,
  };
}

function fmt(n: number | null): string {
  return n == null ? "n/a" : n.toFixed(1);
}

/** Convenience: classify straight from the database. */
export function classifyWorktree(
  db: Database.Database,
  worktree: string,
  obs: ClassifyObservation,
  env: NodeJS.ProcessEnv = process.env
): Classification {
  const sessions = activeSessionsForWorktree(db, worktree);
  const s = sessions[0];
  return classify(
    sessions,
    obs,
    {
      heartbeat_age_min: s ? ageMinutes(db, s.last_heartbeat) : null,
      progress_age_min: s ? ageMinutes(db, s.last_progress) : null,
    },
    { ttl_min: ttlMinutes(env), stall_min: stallMinutes(env) }
  );
}
