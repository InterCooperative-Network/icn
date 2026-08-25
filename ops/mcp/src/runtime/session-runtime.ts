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
  currentWorktreePathFor,
  laneGeneration,
  readBranchState,
  type BranchState,
  type WorktreeIdentity,
} from "./worktree-identity.js";

const SESSION_LOG = resolveOpsStatePath("session-log.jsonl");

/** Heartbeat age past which a session is no longer credibly live. */
export const DEFAULT_TTL_MINUTES = 30;
/** Progress age past which a heartbeating session is "moving but not progressing". */
export const DEFAULT_STALL_MINUTES = 90;

/** Floor on the TTL. Below this, ordinary gaps between hook events read as abandonment. */
export const MIN_TTL_MINUTES = 5;

/** Floor on the stall window. Below this, ordinary thinking time reads as failure to progress. */
export const MIN_STALL_MINUTES = 5;

export function ttlMinutes(env: NodeJS.ProcessEnv = process.env): number {
  // Clamped because this is read from the ambient environment of whoever runs classify:
  // ICN_SESSION_TTL_MINUTES=1 turned a two-minute-old heartbeat into a retirement candidate.
  return Math.max(
    MIN_TTL_MINUTES,
    positiveIntOr(env["ICN_SESSION_TTL_MINUTES"], DEFAULT_TTL_MINUTES)
  );
}
export function stallMinutes(env: NodeJS.ProcessEnv = process.env): number {
  // Floored for exactly the reason the TTL is, and it was missing here: this is read from the
  // ambient environment of whoever runs classify, so ICN_SESSION_STALL_MINUTES=1 made every
  // session older than a minute report PROGRESS-STALLED, and "1e-3" floored to 0 made every
  // session report it unconditionally.
  return Math.max(
    MIN_STALL_MINUTES,
    positiveIntOr(env["ICN_SESSION_STALL_MINUTES"], DEFAULT_STALL_MINUTES)
  );
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
  /** Which GENERATION of the lane this row belongs to. Null = unknown (pre-upgrade row). */
  worktree_generation: string | null;
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
  /**
   * Set when a deduplicated activation was found occupying a DIFFERENT lane than the caller.
   * The lane columns are updated to where the session actually is, and this reports the move
   * so a caller is never silently told it registered somewhere it did not.
   */
  lane_changed?: { from: string | null; to: string | null };
  /** Other live activations already occupying this lane. Reported, never prevented. */
  co_occupants: string[];
};

// ── writes ───────────────────────────────────────────────────────────────────

/**
 * Register (or re-attach to) an activation.
 *
 * The whole body runs in ONE transaction: the SELECT-then-INSERT was racing its own unique
 * index on provider_session_id. Two hook subprocesses firing concurrently for the same
 * conversation both saw no row, both inserted, and the loser died with
 * SQLITE_CONSTRAINT_UNIQUE — leaving that session unregistered, which is precisely the
 * outcome idempotency exists to prevent. Several MCP server processes share one database file
 * on this VM, so this is not theoretical.
 */
export function registerSession(
  db: Database.Database,
  input: RegisterInput
): RegisterResult {
  // .immediate() takes the write lock at BEGIN. A plain (deferred) transaction takes a WAL read
  // snapshot first, so a concurrent commit between the SELECT and the write raises
  // SQLITE_BUSY_SNAPSHOT — which busy_timeout does NOT cover — and the losing hook dies
  // unregistered. That is the exact outcome idempotency exists to prevent, so the fix has to be
  // the lock mode, not merely "a transaction".
  return db.transaction(() => registerSessionInner(db, input)).immediate();
}

function registerSessionInner(
  db: Database.Database,
  input: RegisterInput
): RegisterResult {
  // `|| null`, not `?? null`. An EMPTY STRING is not nullish, so `??` preserved it: the row
  // stored a NON-NULL "" (covered by the partial unique index) while every `if (key)` guard
  // treated it as absent, so the dedupe SELECT was skipped. The first such register stored a
  // key that could never be looked up, and every later register carrying "" — the same
  // conversation OR an unrelated session — died on SQLITE_CONSTRAINT_UNIQUE, unregistered.
  // The CLI's `str()` already drops zero-length values; the MCP surface (z.string().optional(),
  // which accepts "") did not, so the guard has to live here, in the shared core.
  const key = input.provider_session_id || null;
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
      // A resumed conversation can legitimately reappear in a DIFFERENT worktree (`--resume`
      // run from another lane, or a `resume` SessionStart after the previous process died
      // without SessionEnd). Previously the lane columns were left pointing at the old lane
      // while the RESULT reported the new one — so lane A looked permanently occupied by a
      // session that had left, lane B reported UNREGISTERED-OBSERVED, and the returned
      // worktree_id contradicted the stored row. Move the row to where the session actually is
      // and say so.
      const prior = db
        .prepare("SELECT worktree_id, worktree_generation FROM sessions WHERE id = ?")
        .get(existing.id) as { worktree_id: string | null; worktree_generation: string | null };
      const moved = worktreeId !== null && prior.worktree_id !== worktreeId;
      if (moved) {
        db.prepare(
          `UPDATE sessions
              SET repo_id = ?, worktree_id = ?, worktree_path = ?, worktree_name = ?,
                  worktree = COALESCE(?, worktree),
                  worktree_generation = ?,
                  branch_at_registration = ?, head_at_registration = ?
            WHERE id = ?`
        ).run(
          idt?.repo_id ?? null,
          worktreeId,
          idt?.worktree_path ?? null,
          idt?.worktree_name ?? null,
          idt?.worktree_name ?? null,
          // The generation moves with the lane for the same reason the branch does: this row
          // now belongs to the destination lane's CURRENT generation, not the origin's.
          idt?.worktree_generation ?? null,
          // Move these WITH the lane. Left behind, `branch_changed` was permanently and
          // wrongly true after a supported lane move — it compared lane B's live branch
          // against a branch recorded for lane A.
          bs?.branch ?? null,
          bs?.head ?? null,
          existing.id
        );
      } else if (idt !== null && prior.worktree_generation !== (idt.worktree_generation ?? null)) {
        // SAME worktree_id, DIFFERENT GENERATION — and this is not an exotic case.
        //
        // Git recycles `<repo>/.git/worktrees/<basename>`, so a lane removed and recreated at
        // the SAME pathname keeps its worktree_id. `moved` is therefore false, and a resumed
        // conversation that deduped into its old row kept the DEAD generation. The lane filter
        // drops rows whose generation is known-different, so it then discarded the row of a
        // LIVE, just-registered, heartbeating session — emptying that session's own lane.
        //
        // Measured before this fix: register reported success (deduplicated), the row still
        // held gen1 while the lane held gen2, and classify answered UNREGISTERED-OBSERVED with
        // contention 0. Worse, a DEAD co-occupant carrying the fresh generation survived the
        // filter, so the lane reported REGISTERED-EXPIRED naming the dead pid while the live
        // occupant had been filtered away — an inversion of the exact invariant the lane-level
        // aggregation exists to guarantee.
        //
        // Overwrite unconditionally, including with NULL when the token could not be minted:
        // NULL means "unknown", which the filter KEEPS. Preserving a stale generation would
        // keep the row filtered, which is the unsafe direction.
        db.prepare("UPDATE sessions SET worktree_generation = ? WHERE id = ?").run(
          idt.worktree_generation ?? null,
          existing.id
        );
      }
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
      // Report the lane actually stored on the row, never the caller's guess.
      const stored = db
        .prepare("SELECT worktree_id FROM sessions WHERE id = ?")
        .get(existing.id) as { worktree_id: string | null };
      return {
        session_id: existing.id,
        provider_session_id: key,
        worktree_id: stored.worktree_id,
        created: false,
        deduplicated: true,
        co_occupants: coOccupants().filter((id) => id !== existing.id),
        ...(moved ? { lane_changed: { from: prior.worktree_id, to: stored.worktree_id } } : {}),
      };
    }
  }

  const id = randomUUID();
  db.prepare(
    `INSERT INTO sessions
       (id, repo, repo_id, worktree, worktree_id, worktree_path, worktree_name,
        worktree_generation,
        branch_at_registration, head_at_registration, task_description, task_ref, pr_ref,
        parent_session_id, provider, agent_pid, host, provider_session_id, transcript_path)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
  ).run(
    id,
    input.repo,
    idt?.repo_id ?? null,
    input.worktree ?? idt?.worktree_name ?? null,
    worktreeId,
    idt?.worktree_path ?? null,
    idt?.worktree_name ?? null,
    idt?.worktree_generation ?? null,
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

/**
 * Runtime events that count as evidence that work actually happened.
 *
 * ONE list, exported, because this vocabulary was written out three times — this union, the
 * zod enum in tools/sessions.ts, and an unchecked cast in cli/session.ts. Three copies is
 * three chances to disagree, and they already did: the CLI accepted `--kind turn` silently
 * while the MCP tool rejected it, and adding "turn" back to BOTH the union and the enum left
 * the entire suite green. The MCP schema and the CLI now derive from this array.
 *
 * THERE IS NO "turn". A completed agent turn proves the harness answered, not that work moved;
 * an agent looping on a failing edit completes turns forever. Counting turns as progress would
 * defeat PROGRESS-STALLED, which is the one signal this runtime exists to provide.
 */
export const PROGRESS_KINDS = [
  "file_edit",
  "command",
  "test",
  "task_state",
  "explicit",
] as const;

export type ProgressKind = (typeof PROGRESS_KINDS)[number];

export class InvalidProgressKindError extends Error {}

/**
 * Enforced in the SHARED CORE, not per surface. TypeScript cannot help here: the CLI receives
 * `--kind` as an arbitrary string from a hook payload, and a cast is not a check.
 */
export function assertProgressKind(kind: string): asserts kind is ProgressKind {
  if (!(PROGRESS_KINDS as readonly string[]).includes(kind)) {
    throw new InvalidProgressKindError(
      `${JSON.stringify(kind)} is not a progress kind (allowed: ${PROGRESS_KINDS.join(", ")}). ` +
        "A completed agent turn is INTERACTION, not progress — record it as an interaction."
    );
  }
}

/**
 * Progress. Advances `last_progress` AND the monotonic `progress_count`, so a consumer can
 * sample the counter twice and prove motion without trusting wall-clock timestamps.
 */
export function recordProgress(
  db: Database.Database,
  sessionId: string,
  opts: { kind: ProgressKind; activity?: string | null }
): boolean {
  // Validated here so EVERY caller is covered, including the ones TypeScript cannot see.
  assertProgressKind(opts.kind);
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
  dropped: { file_claims: number; undelivered_messages: number };
};

/**
 * Session-scoped resources — the complete inventory, kept next to the code that must clear it.
 *
 *   file_claims.session_id       AUTHORITY  advisory edit lock  -> deleted (FK ON DELETE CASCADE)
 *   watchers_process.session_id  NOT TOUCHED. That table belongs to the pre-existing
 *                                watch_process feature and its background poller. Supervision
 *                                (which also used it) left with ops/agent-supervision-lifecycle,
 *                                and cleaning up watcher rows goes with it. A row whose session
 *                                is deleted therefore survives as 'running' until its pid exits
 *                                — a pre-existing leak on main, not one this branch introduces.
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
  // ATOMIC, for the same reason registerSession is. Release ran a SELECT, four dependent
  // statements and the row DELETE as SEVEN separate implicit transactions. A registration that
  // deduped into this row between the SELECT and the DELETE was silently voided: the arriving
  // session was told `created:false, deduplicated:true` about a row release then deleted,
  // leaving an agent that believes it is registered with no row at all — the precise outcome
  // idempotency exists to prevent. `.immediate()` takes the write lock at BEGIN because a
  // deferred transaction raises SQLITE_BUSY_SNAPSHOT on a concurrent commit, which
  // busy_timeout does not cover.
  //
  // The session-log append is best-effort and now happens AFTER the commit, so a rolled-back
  // release can no longer leave a history line for a release that did not happen. The cost is
  // that a process dying between commit and append loses the line — a log is not authority,
  // and that is the cheaper failure.
  // The history line is written AFTER the transaction commits.
  //
  // Wrapping release in a transaction put an `appendFileSync` inside the exclusive registry
  // write lock, so any stalled write to ops/state/session-log.jsonl held that lock for as long
  // as the filesystem took. Measured with a FIFO standing in for a stalled disk: a concurrent
  // `register` waited out its full 5 s busy_timeout and then died SQLITE_BUSY, exit 1,
  // unregistered — the exact outcome registerSession's own transaction exists to prevent.
  // Blocking I/O has no business inside a lock that other processes are queuing on.
  let historyLine: string | null = null;
  const result = db
    .transaction(() => releaseSessionInner(db, sessionId, opts, (line) => (historyLine = line)))
    .immediate();
  if (historyLine !== null) {
    try {
      appendFileSync(SESSION_LOG, historyLine);
    } catch {
      // Best-effort: a log is not authority, and release has already committed.
    }
  }
  return result;
}

function releaseSessionInner(
  db: Database.Database,
  sessionId: string,
  opts: { reason?: string } = {},
  emitHistory: (line: string) => void = () => {}
): ReleaseResult {
  const session = db
    .prepare("SELECT * FROM sessions WHERE id = ?")
    .get(sessionId) as SessionRow | undefined;

  if (!session)
    return {
      released: false,
      orphaned_children: [],
      dropped: { file_claims: 0, undelivered_messages: 0 },
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

  // Handed OUT of the transaction, written after it commits. See releaseSession().
  emitHistory(
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

  // Drop ephemeral authority BEFORE deleting the row, so nothing is orphaned by the cascade.
  const claims = db
    .prepare("DELETE FROM file_claims WHERE session_id = ?")
    .run(sessionId).changes;

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
  const rows = db
    .prepare("SELECT * FROM sessions WHERE worktree_id = ? ORDER BY started_at DESC")
    .all(worktreeId) as SessionRow[];

  // Drop rows left behind by a REMOVED worktree that happened to share this admin directory.
  // git recycles `<repo>/.git/worktrees/<basename>`, so a new lane at a DIFFERENT path can
  // inherit a deleted lane's id and adopt its unreleased rows — see currentWorktreePathFor().
  //
  // Fail-safe by construction: when the live path cannot be determined (a main worktree, an
  // unreadable admin dir) every row is kept, and a row that never recorded a path is kept too.
  // Filtering can therefore only ever make a lane look LESS occupied when git itself says the
  // recorded path is not this lane — and an emptied lane classifies UNREGISTERED-OBSERVED,
  // which is protected, not actionable.
  // GENERATION FIRST. `worktree_id` identifies a lane in SPACE; the generation identifies it in
  // TIME. Recreating a worktree at the EXACT SAME pathname leaves repo, admin dir and recorded
  // path all matching while the lane is genuinely a different one — verified: a fresh `gen2`
  // worktree classified REGISTERED-ACTIVE holding `gen1`'s row. A path comparison cannot see
  // that; only a token minted per generation can.
  //
  // Both comparisons drop a row ONLY when the two sides are KNOWN AND DIFFERENT. Unknown on
  // either side keeps it, so an unmintable token, an unreadable admin dir or a pre-upgrade NULL
  // can only ever make a lane look MORE occupied. A lane emptied by this filter classifies
  // UNREGISTERED-OBSERVED, which is protected rather than actionable.
  const liveGeneration = laneGeneration(worktreeId);
  const livePath = currentWorktreePathFor(worktreeId);
  return rows.filter((r) => {
    if (liveGeneration != null && r.worktree_generation != null) {
      return r.worktree_generation === liveGeneration;
    }
    // Nothing to compare temporally (a row written before v5, or an admin dir we cannot mint
    // into). Fall back to the path, which still catches recreation at a DIFFERENT path.
    if (livePath != null && r.worktree_path != null) return r.worktree_path === livePath;
    return true;
  });
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

// ── process liveness ─────────────────────────────────────────────────────────
//
// SUPERVISION OF LONG-RUNNING OPERATIONS IS NOT PART OF THIS MODULE.
//
// It lived here and was removed deliberately: five of the six P0 defects found across three
// independent review rounds were in that surface, and each repair satisfied its own test and
// comment while breaking the invariant one layer out. The root cause is that a lane's
// protection had three competing sources of truth — the supervision row's lane, the owning
// session's lane, and the live pid — and every fix re-derived the wrong one.
//
// It is preserved on branch ops/agent-supervision-lifecycle for a separate PR, where it can be
// redesigned around a single source of truth and reviewed on its own.
//
// What remains here reports OBSERVATIONS. Nothing in this module decides that a lane may be
// retired; see the Classification type.

/**
 * Turn a `--pids` string and an `--observed-none` flag into the THREE-STATE observation the
 * classifier requires: `null` = nobody looked, `[]` = looked and found nothing, `[…]` = found
 * these. Lives in the shared core, not in the CLI, so the invariant is testable in-process —
 * the CLI module runs `main()` on import and cannot be imported by a test.
 *
 * THE RULE IS ABOUT THE RESULT, NOT THE SPELLING OF THE INPUT. Any `--pids` value that yields
 * no usable pid means the observation FAILED, and a failed observation is `null`. The
 * affirmative "I looked and found nothing" has exactly one spelling: `--observed-none`.
 *
 * The previous guard was `tokens.length > 0 && parsed.length === 0`, which left a hole exactly
 * where the shell puts one: `tokens` is already filtered to non-empty strings, so a value made
 * only of whitespace and separators produced `tokens.length === 0`, skipped the guard, and
 * landed on the affirmative `[]`. Measured — `' , '`, `','`, `'  '` and `', ,,'` each produced
 * a byte-identical envelope to `--observed-none` on a lane a live process was holding. Those
 * values are not exotic: `"$(a | paste -sd, -),$(b | paste -sd, -)"` yields `","` when both
 * commands find nothing, and so does `printf '%s,' "${EMPTY[@]}"`.
 */
export function parseObservedPids(
  raw: string | undefined,
  observedNone: boolean
): { observed_pids: number[] | null; warning?: string } {
  if (raw === undefined) {
    return { observed_pids: observedNone ? [] : null };
  }
  const tokens = raw
    .split(",")
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
  const parsed: number[] = [];
  const rejected: string[] = [];
  for (const t of tokens) {
    // PLAIN DECIMAL DIGITS ONLY. `Number()` silently reinterprets "0x10" as 16 and "1e3" as
    // 1000, so a token that is not a pid at all would be accepted as some OTHER pid — an
    // observation about a process nobody looked at.
    const n = /^[0-9]+$/.test(t) ? Number(t) : NaN;
    // Non-positive is not a pid either: POSIX gives `kill(0)` and `kill(-1)` process-GROUP
    // meanings, so they always "succeed" and would report a lane alive forever.
    if (Number.isInteger(n) && n > 0) parsed.push(n);
    else rejected.push(t);
  }

  // A PARTIAL observation is not a safe observation. Silently dropping an unparseable token
  // would report FEWER holders than the caller actually saw — the less-protective direction —
  // so anything unreadable invalidates the whole thing rather than shrinking it.
  if (rejected.length > 0) {
    return {
      observed_pids: null,
      warning:
        `--pids ${JSON.stringify(raw)} contained unusable entries (${rejected
          .map((t) => JSON.stringify(t))
          .join(", ")}); treating as NO observation performed (lane stays protected)`,
    };
  }
  if (parsed.length === 0) {
    return {
      observed_pids: null,
      warning:
        `--pids ${JSON.stringify(raw)} contained no pids; treating as NO observation performed ` +
        "(lane stays protected). The affirmative 'I looked and found nothing' is --observed-none",
    };
  }
  return { observed_pids: parsed };
}

/**
 * The ONLY way any surface emits a degraded classification.
 *
 * A degraded envelope must be structurally identical to a healthy one, because a consumer
 * typed against `Classification` reads `.live_agent_pids.length` and throws outright on a
 * partial object.
 *
 * It lives in the shared core rather than in one surface because the repair that introduced it
 * covered ONLY the CLI. The MCP tool and the shell wrapper kept hand-written literals, and a
 * literal handed to `JSON.stringify` (or to a helper typed `unknown`) is checked against no
 * type at all — deleting `contention` and `live_agent_pids` from the MCP tool's copy left
 * `tsc --noEmit` clean and all 250 tests green, while the same deletion inside this annotated
 * constructor is a compile error. One emitter was enforced; the others were decoration.
 */
export function degradedClassification(reason: string): Classification {
  return {
    state: "REGISTRY-UNAVAILABLE",
    reason,
    session_id: null,
    heartbeat_age_min: null,
    progress_age_min: null,
    progress_count: null,
    contention: { count: 0, session_ids: [] },
    branch_changed: false,
    live_branch: null,
    live_agent_pids: [],
  };
}

/** Whether a PID is alive. Signal 0 performs the permission/existence check only. */
export function pidAlive(pid: number): boolean {
  // NON-POSITIVE PIDS ARE NOT PROCESSES. POSIX gives them a completely different meaning:
  // `kill(0, sig)` signals the CALLER'S OWN PROCESS GROUP and `kill(-1, sig)` signals every
  // process the uid may signal, so both SUCCEED and reported "alive" forever. The CLI already
  // rejected 0/negative on its own `--pid`/`--pids` inputs, but the shared primitive did not —
  // and `watchers_process.pid` reaches it unvalidated, so a watcher on pid 0 never completed.
  // A liveness primitive must answer "no" to a question that is not about a process at all.
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    // EPERM means the process exists but belongs to another user — still alive.
    return (e as NodeJS.ErrnoException).code === "EPERM";
  }
}

// ── classification ───────────────────────────────────────────────────────────

export type ClassifyObservation = {
  /**
   * PIDs observed holding the worktree.
   *
   * `null` or absent means NOBODY LOOKED — which is NOT the same as "nothing is there", and
   * must never be consumed as evidence of absence. An empty ARRAY is an affirmative claim that
   * an observation was performed and found nothing; only that can support retirement.
   *
   * This distinction is the whole finding: every caller defaulted this to `[]`, so a lane whose
   * agent process was demonstrably alive classified as REGISTERED-EXPIRED / retireable, with a
   * reason string asserting "no process holds the worktree" that nothing had checked.
   */
  observed_pids?: number[] | null;
  /** True when the registry could not be opened/read at all. */
  registry_unavailable?: boolean;
  /** Current branch/HEAD, re-read at classification time. Never the registration value. */
  live_branch?: BranchState | null;
  /**
   * Result of checking the lane's recorded `agent_pid`s. The registry can corroborate itself
   * without any caller cooperation, so a missing external observation is not fatal.
   */
  agent_pid_alive?: boolean | null;
  /**
   * WHICH recorded pids were found alive. Carried separately so the reason string can name the
   * pid that is actually running rather than whichever row happened to be ranked first — an
   * aggregated verdict with a per-row explanation produced "agent pid 999991 is still alive"
   * about the one pid known to be dead.
   */
  live_agent_pids?: number[];
};

/**
 * A read-only description of what the runtime OBSERVES about a lane.
 *
 * There is deliberately no `retireable` field. Deciding that a lane may be reclaimed is a
 * policy question with real consequences — it can mean killing a live agent or destroying an
 * in-flight build — and every attempt to answer it inside this module produced a defect that
 * passed its own tests. Consumers (icn-lane-audit) apply their own policy to these facts, and
 * retirement stays read-only and operator-approved.
 */
export type Classification = {
  /** What the registry and process observation say. NOT a retirement verdict. */
  state: LifecycleState;
  reason: string;
  session_id: string | null;
  heartbeat_age_min: number | null;
  progress_age_min: number | null;
  progress_count: number | null;
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
  /**
   * Recorded agent pids found ALIVE. Emitted because it is the decisive fact for any consumer
   * applying a retirement policy — the reduction's whole rationale is that consumers own that
   * policy, so the evidence has to be on the wire, not just interpolated into `reason`.
   */
  live_agent_pids: number[];
};

/**
 * Classify a worktree from registry state plus process observation.
 *
 * FAIL-SAFE INVARIANT (asserted by tests, not by convention):
 *   absence of a registry row NEVER yields a state implying the lane is free.
 * A missing row is indistinguishable from a pre-integration session, an unsupported
 * launcher, or a registry failure — all of which must stay protected.
 */
export function classify(
  sessions: SessionRow[],
  obs: ClassifyObservation,
  ages: {
    heartbeat_age_min: number | null;
    progress_age_min: number | null;
    /** Age since started_at. Used when a session has NEVER progressed — see below. */
    session_age_min?: number | null;
    /**
     * LANE-level progress count, when the caller has one.
     *
     * `classifyWorktree` aggregates progress across every occupant for the same reason it
     * aggregates liveness, so it must be able to override the primary row's own counter —
     * otherwise the lane reports the primary's `0` while a co-occupant is working.
     */
    progress_count?: number | null;
  },
  limits: { ttl_min: number; stall_min: number }
): Classification {
  const base = {
    session_id: null as string | null,
    heartbeat_age_min: null as number | null,
    progress_age_min: null as number | null,
    progress_count: null as number | null,
    contention: { count: 0, session_ids: [] as string[] },
    branch_changed: false,
    live_branch: null as BranchState | null,
    live_agent_pids: [] as number[],
  };

  // "Nobody looked" vs "looked and found nothing" — see ClassifyObservation.observed_pids.
  const observationPerformed = Array.isArray(obs.observed_pids);
  const observedPids = obs.observed_pids ?? [];

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
        observedPids.length > 0
          ? `no registry row, but ${observedPids.length} process(es) hold the worktree; ` +
            "may be a pre-integration session or an unsupported launcher — protected"
          : observationPerformed
            ? "no registry row, and an observation found no process; still protected because a " +
              "missing row may be a pre-integration session, an unsupported launcher, or a registry failure"
            : "no registry row, and no process observation was performed; nothing authoritative " +
              "to act on — protected",
    };
  }

  const s = sessions[0]!;
  const live = obs.live_branch ?? null;
  // Lane-level when the caller supplied it, else this row's own. See `ages.progress_count`.
  const effectiveProgressCount = ages.progress_count ?? s.progress_count;

  const shared = {
    session_id: s.id,
    contention: {
      count: sessions.length,
      session_ids: sessions.map((x) => x.id),
    },
    branch_changed: live ? branchChanged(s.branch_at_registration, live) : false,
    live_branch: live,
    live_agent_pids: obs.live_agent_pids ?? [],
    heartbeat_age_min: ages.heartbeat_age_min,
    progress_age_min: ages.progress_age_min,
    progress_count: effectiveProgressCount,
  };

  const hb = ages.heartbeat_age_min;
  const heartbeatExpired = hb != null && hb > limits.ttl_min;

  if (heartbeatExpired) {
    // Expired heartbeat + a live process is still a *process-pinned* lane. The process is the
    // stronger fact here, so it downgrades the verdict rather than being ignored.
    if (observedPids.length > 0) {
      return {
        ...shared,
        state: "PROGRESS-STALLED",
        reason:
          `heartbeat is ${fmt(hb)} min old (TTL ${limits.ttl_min}) but ` +
          `${observedPids.length} process(es) still hold the worktree`,
      };
    }

    // The registry corroborates itself: the session recorded its own pid, so a live agent
    // process is decisive regardless of what any caller did or did not observe.
    if (obs.agent_pid_alive) {
      return {
        ...shared,
        state: "PROGRESS-STALLED",
        reason:
          `heartbeat is ${fmt(hb)} min old (TTL ${limits.ttl_min}) but a registered agent ` +
          `process on this lane is still alive` +
          (obs.live_agent_pids && obs.live_agent_pids.length > 0
            ? ` (pid ${obs.live_agent_pids.join(", ")})`
            : ""),
      };
    }

    // Retirement requires an AFFIRMATIVE observation. Without one the only honest verdict is
    // "the heartbeat expired and nobody checked whether anything is running".
    if (!observationPerformed) {
      return {
        ...shared,
        state: "REGISTERED-EXPIRED",
        reason:
          `heartbeat is ${fmt(hb)} min old (TTL ${limits.ttl_min}), but no process observation ` +
          "was supplied — absence of observation is not evidence of absence, so the lane is protected",
      };
    }

    return {
      ...shared,
      state: "REGISTERED-EXPIRED",
      reason:
        `heartbeat is ${fmt(hb)} min old (TTL ${limits.ttl_min}); an observation was performed ` +
        `and found no process holding the worktree` +
        // Only assert the pid is gone when that was actually determined. Gating this on the
        // pid's mere presence produced "its recorded agent pid N is gone" for a pid nobody
        // had checked — and, in the multi-session case, for one that was alive.
        (obs.agent_pid_alive === false && s.agent_pid
          ? `, and its recorded agent pid ${s.agent_pid} is gone`
          : ""),
    };
  }

  const pa = ages.progress_age_min;
  // A session that has NEVER progressed must still be able to stall. Falling back to the
  // HEARTBEAT AGE was wrong twice over: it is staleness, not elapsed work time, and the branch
  // was unreachable — control only reaches here when hb <= ttl (30), which can never exceed
  // the stall window (90). So a lane that registered and then only ever emitted interactions
  // — a review or analysis session using Read/Grep, or one blocked waiting on a human —
  // reported REGISTERED-ACTIVE forever. Fall back to age since registration, which is the
  // quantity the comment always meant.
  const effectiveProgressAge = pa ?? ages.session_age_min ?? null;
  if (
    effectiveProgressAge != null &&
    effectiveProgressAge > limits.stall_min
  ) {
    return {
      ...shared,
      state: "PROGRESS-STALLED",
      reason:
        (effectiveProgressCount === 0
          ? `heartbeat is fresh (${fmt(hb)} min) but NO progress has ever been recorded, ` +
            `${fmt(effectiveProgressAge)} min after registration `
          : `heartbeat is fresh (${fmt(hb)} min) but no progress for ${fmt(effectiveProgressAge)} min `) +
        `(stall window ${limits.stall_min}); activity without progress`,
    };
  }

  return {
    ...shared,
    state: "REGISTERED-ACTIVE",
    reason:
      effectiveProgressCount === 0
        ? `heartbeat ${fmt(hb)} min; no progress recorded yet (registered ${fmt(ages.session_age_min ?? null)} min ago)`
        : `heartbeat ${fmt(hb)} min, progress ${fmt(effectiveProgressAge)} min, count ${effectiveProgressCount}`,
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

  // Rank by heartbeat freshness for AGE REPORTING only.
  const ranked = sessions
    .map((x) => ({ row: x, hb: ageMinutes(db, x.last_heartbeat) }))
    .sort((a, b) => (a.hb ?? Infinity) - (b.hb ?? Infinity));
  const primary = ranked[0]?.row;

  // PROTECTION IS A PROPERTY OF THE LANE, NOT OF ONE ROW.
  //
  // Selecting a single "primary" by freshest heartbeat and then judging the lane from it was
  // wrong in the dangerous direction: a crashed co-occupant that happened to heartbeat more
  // recently became primary, its dead pid was the only one checked, and a live agent's recorded
  // pid was never looked at — producing retireable:true on a lane with a running agent. The
  // documented invariant is explicit that "a dead editor must not make a busy reviewer's lane
  // look abandoned", so liveness is aggregated across every occupant.
  const recordedPids = sessions
    .map((s) => s.agent_pid)
    .filter((x): x is number => x != null);
  const liveAgentPids = obs.live_agent_pids ?? recordedPids.filter((pid) => pidAlive(pid));
  const agentPidAlive =
    obs.agent_pid_alive ?? (recordedPids.length > 0 ? liveAgentPids.length > 0 : null);

  // PROGRESS IS A PROPERTY OF THE LANE TOO — the same lesson as liveness, one field over.
  //
  // Progress was read from `primary` alone, and `primary` is whichever row has the freshest
  // HEARTBEAT. So a co-occupant that only heartbeats (a reviewer completing turns, which is
  // interaction, not progress) outranked one that was actually working, and the lane reported
  // "NO progress has ever been recorded" one second after a co-occupant recorded progress —
  // with progress_count 0 and progress_age_min null emitted as LANE facts. Aggregate instead.
  const progressAges = sessions
    .map((x) => ageMinutes(db, x.last_progress))
    .filter((x): x is number => x != null);
  const laneProgressAge = progressAges.length > 0 ? Math.min(...progressAges) : null;
  const laneProgressCount = sessions.reduce((n, x) => n + (x.progress_count ?? 0), 0);
  // When NOBODY has progressed, "how long has this lane failed to progress" is measured from
  // the OLDEST registration on it. A lane occupied for 200 minutes is not made fresh by a
  // session that joined a minute ago.
  const sessionAges = sessions
    .map((x) => ageMinutes(db, x.started_at))
    .filter((x): x is number => x != null);
  const laneSessionAge = sessionAges.length > 0 ? Math.max(...sessionAges) : null;

  const liveBranch =
    obs.live_branch ??
    (primary?.worktree_path ? readBranchState(primary.worktree_path) : null);

  return classify(
    primary ? [primary, ...sessions.filter((x) => x.id !== primary.id)] : [],
    {
      ...obs,
      live_branch: liveBranch,
      agent_pid_alive: agentPidAlive,
      live_agent_pids: liveAgentPids,
    },
    {
      heartbeat_age_min: primary ? ageMinutes(db, primary.last_heartbeat) : null,
      progress_age_min: laneProgressAge,
      session_age_min: laneSessionAge,
      progress_count: laneProgressCount,
    },
    { ttl_min: ttlMinutes(env), stall_min: stallMinutes(env) }
  );
}
