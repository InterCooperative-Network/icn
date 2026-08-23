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
import {
  branchChanged,
  readBranchState,
  type BranchState,
  type WorktreeIdentity,
} from "./worktree-identity.js";

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
  | "SUPERVISED-BUSY"
  | "PROGRESS-STALLED"
  | "REGISTERED-EXPIRED"
  | "UNREGISTERED-OBSERVED"
  | "REGISTRY-UNAVAILABLE";

/**
 * A harness key must be DURABLE: unique for the life of the harness session and not reusable
 * afterwards. Claude Code's hook payload `session_id` qualifies (verified stable across
 * --resume). `pid@host` does NOT: PIDs are recycled, so a future process could inherit a dead
 * session's identity and silently adopt its claims. Reject that shape outright rather than
 * documenting it as discouraged.
 */
const PID_AT_HOST = /^\d+@/;

export class InvalidProviderSessionIdError extends Error {}

export function assertDurableProviderSessionId(key: string): void {
  if (PID_AT_HOST.test(key)) {
    throw new InvalidProviderSessionIdError(
      `provider session id ${JSON.stringify(key)} looks like pid@host; PIDs are reusable and ` +
        "cannot be durable identity. Use the provider's own session id, or --identity-file."
    );
  }
}

/**
 * One RUNTIME ACTIVATION of an agent session.
 *
 *   `id`                  activation identity — unique per live activation
 *   `provider_session_id` conversation identity — stable across `--resume`, so ONE of these
 *                         may map to MANY activations over time (never two at once)
 *   `repo_id`/`worktree_id`  lane identity — Git-derived, immune to branch movement
 *   `branch_at_registration` a historical launch fact, NOT current branch state
 *   `agent_pid`/`host`    correlation only; PIDs are reusable
 */
export type SessionRow = {
  id: string;
  repo: string;
  repo_id: string | null;
  worktree: string | null;
  worktree_id: string | null;
  worktree_path: string | null;
  worktree_name: string | null;
  branch_at_registration: string | null;
  head_at_registration: string | null;
  task_description: string | null;
  task_ref: string | null;
  pr_ref: string | null;
  parent_session_id: string | null;
  provider: string | null;
  agent_pid: number | null;
  host: string | null;
  provider_session_id: string | null;
  transcript_path: string | null;
  current_activity: string | null;
  progress_count: number;
  started_at: string;
  last_heartbeat: string;
  last_progress: string | null;
};

export type RegisterInput = {
  repo: string;
  /** Git-derived lane identity. Supply via discoverWorktree(); never hand-built from a path. */
  identity?: WorktreeIdentity | null;
  /** Live branch/HEAD at registration — recorded as history, never re-read from here. */
  branch_state?: BranchState | null;
  worktree?: string | null;
  task_description?: string | null;
  task_ref?: string | null;
  pr_ref?: string | null;
  parent_session_id?: string | null;
  provider?: string | null;
  agent_pid?: number | null;
  host?: string | null;
  /**
   * The harness conversation id (Claude Code hook `session_id`). Registration is idempotent
   * on this WITHIN a live activation: a hook that fires twice must not create two rows. It is
   * NOT unique over time — a released conversation that later resumes gets a NEW activation.
   */
  provider_session_id?: string | null;
  transcript_path?: string | null;
};

export type RegisterResult = {
  /** The runtime activation id. */
  session_id: string;
  provider_session_id: string | null;
  worktree_id: string | null;
  created: boolean;
  /** True when an existing live activation was returned instead of a new one. */
  deduplicated: boolean;
  /** Other live activations already occupying this lane. Reported, never prevented. */
  co_occupants: string[];
};

// ── writes ───────────────────────────────────────────────────────────────────

export function registerSession(
  db: Database.Database,
  input: RegisterInput
): RegisterResult {
  const key = input.provider_session_id ?? null;
  if (key) assertDurableProviderSessionId(key);

  const idt = input.identity ?? null;
  const bs = input.branch_state ?? null;
  const worktreeId = idt?.worktree_id ?? null;

  const coOccupants = (): string[] =>
    worktreeId
      ? (
          db
            .prepare("SELECT id FROM sessions WHERE worktree_id = ?")
            .all(worktreeId) as Array<{ id: string }>
        ).map((r) => r.id)
      : [];

  if (key) {
    // Dedupe against the LIVE activation only. Released rows are deleted, so a conversation
    // resumed after release falls through to a genuinely new activation below.
    const existing = db
      .prepare("SELECT id FROM sessions WHERE provider_session_id = ? LIMIT 1")
      .get(key) as { id: string } | undefined;
    if (existing) {
      // Refresh mutable launch facts; never touch identity or progress history.
      db.prepare(
        `UPDATE sessions
            SET task_ref = COALESCE(?, task_ref),
                pr_ref = COALESCE(?, pr_ref),
                task_description = COALESCE(?, task_description),
                agent_pid = COALESCE(?, agent_pid),
                transcript_path = COALESCE(?, transcript_path),
                last_heartbeat = datetime('now')
          WHERE id = ?`
      ).run(
        input.task_ref ?? null,
        input.pr_ref ?? null,
        input.task_description ?? null,
        input.agent_pid ?? null,
        input.transcript_path ?? null,
        existing.id
      );
      return {
        session_id: existing.id,
        provider_session_id: key,
        worktree_id: worktreeId,
        created: false,
        deduplicated: true,
        co_occupants: coOccupants().filter((id) => id !== existing.id),
      };
    }
  }

  const id = randomUUID();
  db.prepare(
    `INSERT INTO sessions
       (id, repo, repo_id, worktree, worktree_id, worktree_path, worktree_name,
        branch_at_registration, head_at_registration, task_description, task_ref, pr_ref,
        parent_session_id, provider, agent_pid, host, provider_session_id, transcript_path)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
  ).run(
    id,
    input.repo,
    idt?.repo_id ?? null,
    input.worktree ?? idt?.worktree_name ?? null,
    worktreeId,
    idt?.worktree_path ?? null,
    idt?.worktree_name ?? null,
    bs?.branch ?? null,
    bs?.head ?? null,
    input.task_description ?? null,
    input.task_ref ?? null,
    input.pr_ref ?? null,
    input.parent_session_id ?? null,
    input.provider ?? null,
    input.agent_pid ?? null,
    input.host ?? null,
    key,
    input.transcript_path ?? null
  );
  return {
    session_id: id,
    provider_session_id: key,
    worktree_id: worktreeId,
    created: true,
    deduplicated: false,
    co_occupants: coOccupants().filter((x) => x !== id),
  };
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
      "UPDATE sessions SET last_heartbeat = datetime('now') WHERE id = ?"
    )
    .run(sessionId);
  return r.changes > 0;
}

/**
 * INTERACTION: the session is being driven, but nothing is asserted about task state.
 *
 * A completed agent turn belongs here, NOT in progress. A turn boundary proves the harness
 * produced a response; it does not prove the work moved. An agent that answers "still waiting"
 * fifty times has completed fifty turns and progressed zero. Counting turns as progress would
 * let exactly that defeat PROGRESS-STALLED, which is the signal this runtime exists to provide.
 */
export function recordInteraction(db: Database.Database, sessionId: string): boolean {
  return recordHeartbeat(db, sessionId);
}

/** Runtime events that count as evidence that work actually happened. */
export type ProgressKind =
  | "file_edit"
  | "command"
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
        WHERE id = ?`
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
  /** Ephemeral authority surrendered, by resource kind. */
  dropped: { file_claims: number; watchers: number; undelivered_messages: number };
};

/**
 * Session-scoped resources — the complete inventory, kept next to the code that must clear it.
 *
 *   file_claims.session_id       AUTHORITY  advisory edit lock  -> deleted (FK ON DELETE CASCADE)
 *   watchers_process.session_id  AUTHORITY  live process monitor-> invalidated (no FK; would
 *                                                                  otherwise stay 'running'
 *                                                                  and keep emitting events to
 *                                                                  a dead session)
 *   mailbox.to_session           AUTHORITY  undelivered inbox   -> invalidated (no FK)
 *   mailbox.from_session         HISTORY    sender attribution  -> kept
 *   events.scope = session:<id>  HISTORY    event log           -> kept
 *
 * The rule: a released session retains HISTORY and surrenders every AUTHORITY. Because the
 * session row itself is deleted (unchanged pre-existing semantics), there is no "released but
 * still holding claims" state to get wrong.
 */

export function releaseSession(
  db: Database.Database,
  sessionId: string,
  opts: { reason?: string } = {}
): ReleaseResult {
  const session = db
    .prepare("SELECT * FROM sessions WHERE id = ?")
    .get(sessionId) as SessionRow | undefined;

  if (!session)
    return {
      released: false,
      orphaned_children: [],
      dropped: { file_claims: 0, watchers: 0, undelivered_messages: 0 },
    };

  const children = (
    db
      .prepare(
        "SELECT id FROM sessions WHERE parent_session_id = ?"
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

  // Drop ephemeral authority BEFORE deleting the row, so nothing is orphaned by the cascade.
  const claims = db
    .prepare("DELETE FROM file_claims WHERE session_id = ?")
    .run(sessionId).changes;

  // Watchers have no foreign key: without this they stay status='running' forever and the
  // background poller keeps supervising on behalf of a session that no longer exists.
  const watchers = db
    .prepare(
      "UPDATE watchers_process SET status = 'released', completed_at = ? WHERE session_id = ? AND status = 'running'"
    )
    .run(Date.now(), sessionId).changes;

  // Undelivered inbox messages: keep the row (history) but stop them being pending for a
  // session that can never read them.
  const undelivered = db
    .prepare(
      "UPDATE mailbox SET read_at = ? WHERE to_session = ? AND read_at IS NULL"
    )
    .run(Date.now(), sessionId).changes;

  db.prepare("DELETE FROM sessions WHERE id = ?").run(sessionId);
  return {
    released: true,
    orphaned_children: children,
    dropped: {
      file_claims: claims,
      watchers,
      undelivered_messages: undelivered,
    },
  };
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

/**
 * Every live activation occupying a lane, newest first.
 *
 * Keyed on the Git-derived worktree_id, never the display name. Returns a LIST because
 * "one worktree == one session" is not an invariant: an interactive agent plus a review
 * session, or an overlapping resume, are legitimate and must stay individually addressable.
 */
export function activeSessionsForWorktree(
  db: Database.Database,
  worktreeId: string
): SessionRow[] {
  return db
    .prepare("SELECT * FROM sessions WHERE worktree_id = ? ORDER BY started_at DESC")
    .all(worktreeId) as SessionRow[];
}

/** Compatibility lookup by display name. Ambiguous by construction — reports every match. */
export function sessionsByWorktreeName(
  db: Database.Database,
  name: string
): SessionRow[] {
  return db
    .prepare(
      "SELECT * FROM sessions WHERE worktree_name = ? OR worktree = ? ORDER BY started_at DESC"
    )
    .all(name, name) as SessionRow[];
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

// ── supervised long-running operations ───────────────────────────────────────
//
// A legitimate 45-minute build emits NO hook events while it runs, so its heartbeat ages past
// the 30-minute TTL and the lane would look abandoned. That must not make it retireable.
//
// The fix is not a fake heartbeat — it is declaring the operation. `watchers_process` already
// models exactly this (session, pid, label, status), so supervision reuses it rather than
// adding a table. A supervision is only credible while its PID is actually alive, so a
// supervised operation cannot outlive the process it names.

export type Supervision = { id: number; pid: number; label: string; created_at: number };

export function superviseOperation(
  db: Database.Database,
  sessionId: string,
  pid: number,
  label: string
): number {
  const r = db
    .prepare(
      "INSERT INTO watchers_process (session_id, pid, label, created_at, status) VALUES (?, ?, ?, ?, 'running')"
    )
    .run(sessionId, pid, label, Date.now());
  return Number(r.lastInsertRowid);
}

export function endSupervision(db: Database.Database, id: number, exitCode?: number): boolean {
  return (
    db
      .prepare(
        "UPDATE watchers_process SET status = 'completed', completed_at = ?, exit_code = ? WHERE id = ? AND status = 'running'"
      )
      .run(Date.now(), exitCode ?? null, id).changes > 0
  );
}

/** Whether a PID is alive. Signal 0 performs the permission/existence check only. */
export function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    // EPERM means the process exists but belongs to another user — still alive.
    return (e as NodeJS.ErrnoException).code === "EPERM";
  }
}

/**
 * Supervisions whose process is STILL RUNNING. A row whose PID has died is reaped here rather
 * than trusted, so a crashed build cannot keep a lane protected forever.
 */
export function liveSupervisions(
  db: Database.Database,
  sessionId: string,
  isAlive: (pid: number) => boolean = pidAlive
): Supervision[] {
  const rows = db
    .prepare(
      "SELECT id, pid, label, created_at FROM watchers_process WHERE session_id = ? AND status = 'running'"
    )
    .all(sessionId) as Supervision[];
  const live: Supervision[] = [];
  for (const r of rows) {
    if (isAlive(r.pid)) live.push(r);
    else endSupervision(db, r.id);
  }
  return live;
}

// ── classification ───────────────────────────────────────────────────────────

export type ClassifyObservation = {
  /** PIDs observed holding the worktree. Corroborating evidence only. */
  observed_pids: number[];
  /** True when the registry could not be opened/read at all. */
  registry_unavailable?: boolean;
  /**
   * Declared long-running operations whose process is still alive. These are the difference
   * between "no hook fired because the session is dead" and "no hook fired because a known,
   * supervised 45-minute build is still running".
   */
  live_supervisions?: Supervision[];
  /** Current branch/HEAD, re-read at classification time. Never the registration value. */
  live_branch?: BranchState | null;
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
  /** Declared long-running operations still alive at classification time. */
  supervised: Supervision[];
  /**
   * Concurrent occupancy. Surfaced, never prevented: several sessions on one lane is a real
   * situation the runtime must be able to REPORT (an agent plus a reviewer, an overlapping
   * resume, an accidental double-launch), not a constraint to enforce.
   */
  contention: { count: number; session_ids: string[] };
  /**
   * Advisory: the lane is no longer on the branch it registered with (rename, switch, detach).
   * Identity is unaffected — this is a warning about state, not a different lane.
   */
  branch_changed: boolean;
  live_branch: BranchState | null;
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
    supervised: [] as Supervision[],
    contention: { count: 0, session_ids: [] as string[] },
    branch_changed: false,
    live_branch: null as BranchState | null,
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
  const supervised = obs.live_supervisions ?? [];
  const live = obs.live_branch ?? null;
  const shared = {
    session_id: s.id,
    contention: {
      count: sessions.length,
      session_ids: sessions.map((x) => x.id),
    },
    branch_changed: live ? branchChanged(s.branch_at_registration, live) : false,
    live_branch: live,
    heartbeat_age_min: ages.heartbeat_age_min,
    progress_age_min: ages.progress_age_min,
    progress_count: s.progress_count,
    supervised,
  };

  const hb = ages.heartbeat_age_min;
  const heartbeatExpired = hb != null && hb > limits.ttl_min;

  // A declared, still-running operation outranks heartbeat age. This is the ONE way a stale
  // heartbeat is legitimately explained, and it requires a live PID — not a timer, not a claim.
  if (supervised.length > 0) {
    return {
      ...shared,
      state: "SUPERVISED-BUSY",
      retireable: false,
      retireable_with_approval: false,
      reason:
        `heartbeat is ${fmt(hb)} min old, but ${supervised.length} supervised operation(s) ` +
        `are still running: ${supervised.map((x) => `${x.label} (pid ${x.pid})`).join(", ")}`,
    };
  }

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

/**
 * Classify a lane straight from the database.
 *
 * `worktreeId` is the Git-derived identity, not a basename. Branch state is re-read live from
 * the worktree path so a rebase or rename shows up as current state rather than corrupting
 * identity. When several sessions occupy the lane the verdict follows the MOST LIVE one —
 * a busy reviewer must not let a dead editor look retireable, and vice versa — while
 * `contention` reports every occupant.
 */
export function classifyWorktree(
  db: Database.Database,
  worktreeId: string,
  obs: ClassifyObservation,
  env: NodeJS.ProcessEnv = process.env
): Classification {
  const sessions = activeSessionsForWorktree(db, worktreeId);
  const s = sessions[0];

  // Freshest heartbeat wins: order the lane's sessions by liveness before judging it.
  const ranked = sessions
    .map((x) => ({ row: x, hb: ageMinutes(db, x.last_heartbeat) }))
    .sort((a, b) => (a.hb ?? Infinity) - (b.hb ?? Infinity));
  const primary = ranked[0]?.row ?? s;

  const supervised =
    obs.live_supervisions ??
    sessions.flatMap((x) => liveSupervisions(db, x.id));

  const liveBranch =
    obs.live_branch ??
    (primary?.worktree_path ? readBranchState(primary.worktree_path) : null);

  return classify(
    primary ? [primary, ...sessions.filter((x) => x.id !== primary.id)] : [],
    { ...obs, live_supervisions: supervised, live_branch: liveBranch },
    {
      heartbeat_age_min: primary ? ageMinutes(db, primary.last_heartbeat) : null,
      progress_age_min: primary ? ageMinutes(db, primary.last_progress) : null,
    },
    { ttl_min: ttlMinutes(env), stall_min: stallMinutes(env) }
  );
}
