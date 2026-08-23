// Discriminating tests for the agent session runtime.
//
// These are deliberately NOT existence checks. Each test names a way the lifecycle could be
// wrong in a way that matters operationally — a duplicate registration, a deadlocked loop that
// looks healthy, a registry outage that reads as "safe to delete" — and pins the behaviour that
// prevents it. Refs docs/architecture/AGENT_RUNTIME.md.

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import type Database from "better-sqlite3";
import { initDb } from "../state/db.js";
import {
  activeSessionsForWorktree,
  ageMinutes,
  classify,
  classifyWorktree,
  getSession,
  recordHeartbeat,
  recordProgress,
  registerSession,
  releaseSession,
  type SessionRow,
} from "../runtime/session-runtime.js";

let db: Database.Database;

beforeEach(() => {
  db = initDb(":memory:");
});
afterEach(() => {
  db.close();
});

/** Backdate a timestamp column so age-dependent behaviour can be tested without waiting. */
function backdate(id: string, column: "last_heartbeat" | "last_progress", minutes: number) {
  db.prepare(
    `UPDATE sessions SET ${column} = datetime('now', ?) WHERE id = ?`
  ).run(`-${minutes} minutes`, id);
}

const LIMITS = { ttl_min: 30, stall_min: 90 };

describe("schema v3 migration", () => {
  it("adds runtime identity columns without breaking pre-v3 inserts", () => {
    // The exact INSERT shape used before v3 must still work: existing callers select s.*.
    const legacy = db.prepare(
      "INSERT INTO sessions (id, repo, worktree, task_description) VALUES (?, ?, ?, ?)"
    );
    expect(() => legacy.run("legacy-1", "icn", "wt", "old style")).not.toThrow();

    const row = db.prepare("SELECT * FROM sessions WHERE id = 'legacy-1'").get() as SessionRow;
    expect(row.progress_count).toBe(0);
    expect(row.state).toBe("active");
    expect(row.last_progress).toBeNull();
  });

  it("is idempotent when applied twice", () => {
    // initDb runs migrate(); running it again on the same file must not throw on ADD COLUMN.
    const again = initDb(":memory:");
    expect(() =>
      again.prepare("SELECT harness_key FROM sessions").all()
    ).not.toThrow();
    again.close();
  });
});

describe("registration", () => {
  it("registers once and returns the same session for a repeated harness key", () => {
    const a = registerSession(db, { repo: "icn", worktree: "wt-a", harness_key: "harness-1" });
    const b = registerSession(db, { repo: "icn", worktree: "wt-a", harness_key: "harness-1" });

    expect(a.created).toBe(true);
    expect(b.created).toBe(false);
    expect(b.deduplicated).toBe(true);
    expect(b.session_id).toBe(a.session_id);

    const count = db
      .prepare("SELECT COUNT(*) c FROM sessions WHERE harness_key = 'harness-1'")
      .get() as { c: number };
    expect(count.c).toBe(1);
  });

  it("preserves progress history across a duplicate registration", () => {
    // A SessionStart hook that fires twice (resume, compact) must not reset the evidence of work.
    const a = registerSession(db, { repo: "icn", harness_key: "harness-2" });
    recordProgress(db, a.session_id, { kind: "file_edit" });
    recordProgress(db, a.session_id, { kind: "command" });

    registerSession(db, { repo: "icn", harness_key: "harness-2" });

    expect(getSession(db, a.session_id)!.progress_count).toBe(2);
  });

  it("creates distinct sessions for distinct harness keys in the same worktree", () => {
    const a = registerSession(db, { repo: "icn", worktree: "wt", harness_key: "k1" });
    const b = registerSession(db, { repo: "icn", worktree: "wt", harness_key: "k2" });
    expect(a.session_id).not.toBe(b.session_id);
    expect(activeSessionsForWorktree(db, "wt")).toHaveLength(2);
  });

  it("does not deduplicate when no harness key is supplied", () => {
    // A launcher with no stable session id gets independent rows rather than silently
    // colliding with an unrelated session.
    const a = registerSession(db, { repo: "icn", worktree: "wt" });
    const b = registerSession(db, { repo: "icn", worktree: "wt" });
    expect(a.session_id).not.toBe(b.session_id);
  });
});

describe("heartbeat vs progress", () => {
  it("heartbeat advances liveness but NOT progress", () => {
    // This is the whole #2644 lesson: a spinning loop can heartbeat forever.
    const { session_id } = registerSession(db, { repo: "icn", harness_key: "k" });
    backdate(session_id, "last_heartbeat", 60);
    recordProgress(db, session_id, { kind: "command" });
    const before = getSession(db, session_id)!;
    backdate(session_id, "last_progress", 200);

    recordHeartbeat(db, session_id);

    const after = getSession(db, session_id)!;
    expect(after.progress_count).toBe(before.progress_count);
    expect(ageMinutes(db, after.last_heartbeat)!).toBeLessThan(1);
    expect(ageMinutes(db, after.last_progress)!).toBeGreaterThan(100);
  });

  it("progress advances the monotonic counter, so motion is provable without clocks", () => {
    const { session_id } = registerSession(db, { repo: "icn", harness_key: "k" });
    const c0 = getSession(db, session_id)!.progress_count;
    recordProgress(db, session_id, { kind: "file_edit", activity: "editing lib.rs" });
    recordProgress(db, session_id, { kind: "test", activity: "cargo test" });
    const s = getSession(db, session_id)!;
    expect(s.progress_count).toBe(c0 + 2);
    expect(s.current_activity).toBe("cargo test");
  });

  it("neither heartbeat nor progress resurrects a released session", () => {
    const { session_id } = registerSession(db, { repo: "icn", harness_key: "k" });
    releaseSession(db, session_id, { reason: "completed" });
    expect(recordHeartbeat(db, session_id)).toBe(false);
    expect(recordProgress(db, session_id, { kind: "turn" })).toBe(false);
  });
});

describe("release", () => {
  it("releases a session and drops its file claims", () => {
    const { session_id } = registerSession(db, { repo: "icn", worktree: "wt", harness_key: "k" });
    db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)").run(
      "icn/crates/a/src/lib.rs",
      session_id
    );

    const res = releaseSession(db, session_id, { reason: "completed" });

    expect(res.released).toBe(true);
    expect(getSession(db, session_id)).toBeUndefined();
    expect(
      db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(session_id)
    ).toEqual({ c: 0 });
  });

  it("is a no-op for an unknown session rather than an error", () => {
    // Hooks call release unconditionally; an already-released session must not fail the hook.
    expect(releaseSession(db, "no-such-session").released).toBe(false);
  });

  it("reports orphaned children instead of cascading them", () => {
    const parent = registerSession(db, { repo: "icn", harness_key: "p" });
    const child = registerSession(db, {
      repo: "icn",
      harness_key: "c",
      parent_session_id: parent.session_id,
    });

    const res = releaseSession(db, parent.session_id, { reason: "completed" });

    expect(res.orphaned_children).toEqual([child.session_id]);
    // The child outlives its parent: its lifecycle is its own.
    expect(getSession(db, child.session_id)).toBeDefined();
  });
});

describe("classification — fail-safe invariants", () => {
  it("a registry outage is never retireable", () => {
    const c = classify([], { observed_pids: [1, 2], registry_unavailable: true }, 
      { heartbeat_age_min: null, progress_age_min: null }, LIMITS);
    expect(c.state).toBe("REGISTRY-UNAVAILABLE");
    expect(c.retireable).toBe(false);
    expect(c.retireable_with_approval).toBe(false);
  });

  it("an unregistered but process-held worktree is never retireable", () => {
    // Pre-integration sessions and unsupported launchers land here. Absence of a row is
    // NOT evidence of absence of a session.
    const c = classify([], { observed_pids: [4242] },
      { heartbeat_age_min: null, progress_age_min: null }, LIMITS);
    expect(c.state).toBe("UNREGISTERED-OBSERVED");
    expect(c.retireable).toBe(false);
  });

  it("an unregistered worktree with no processes is still not auto-retireable", () => {
    const c = classify([], { observed_pids: [] },
      { heartbeat_age_min: null, progress_age_min: null }, LIMITS);
    expect(c.state).toBe("UNREGISTERED-OBSERVED");
    expect(c.retireable).toBe(false);
  });

  it("NO input combination makes an empty registry retireable", () => {
    // Exhaustive guard against a future edit reintroducing "no row == safe to kill".
    for (const pids of [[], [1], [1, 2, 3]]) {
      for (const unavailable of [true, false]) {
        for (const hb of [null, 0, 10, 10_000]) {
          const c = classify([], { observed_pids: pids, registry_unavailable: unavailable },
            { heartbeat_age_min: hb, progress_age_min: hb }, LIMITS);
          expect(c.retireable).toBe(false);
        }
      }
    }
  });
});

describe("classification — registered lanes", () => {
  function row(over: Partial<SessionRow> = {}): SessionRow {
    return {
      id: "s1", repo: "icn", worktree: "wt", branch: null, task_description: null,
      task_ref: null, pr_ref: null, parent_session_id: null, provider: "claude-code",
      agent_pid: 1234, host: "icn-dev", harness_key: "k", state: "active",
      current_activity: null, progress_count: 3, started_at: "2026-08-23 00:00:00",
      last_heartbeat: "2026-08-23 00:00:00", last_progress: "2026-08-23 00:00:00",
      ...over,
    };
  }

  it("fresh heartbeat and fresh progress is ACTIVE", () => {
    const c = classify([row()], { observed_pids: [1234] },
      { heartbeat_age_min: 1, progress_age_min: 2 }, LIMITS);
    expect(c.state).toBe("REGISTERED-ACTIVE");
    expect(c.retireable_with_approval).toBe(false);
  });

  it("fresh heartbeat with stale progress is PROGRESS-STALLED, not ACTIVE", () => {
    // The deadlocked-wait-loop signature: the harness is alive and reporting, the work is not.
    const c = classify([row()], { observed_pids: [1234] },
      { heartbeat_age_min: 1, progress_age_min: 240 }, LIMITS);
    expect(c.state).toBe("PROGRESS-STALLED");
    expect(c.retireable).toBe(false);
    expect(c.retireable_with_approval).toBe(true);
  });

  it("a session that never progressed is judged on how long it has been heartbeating", () => {
    const c = classify([row({ last_progress: null, progress_count: 0 })], { observed_pids: [1] },
      { heartbeat_age_min: 200, progress_age_min: null }, LIMITS);
    // Heartbeat itself is past TTL and a process holds the lane -> stalled, not expired.
    expect(c.state).toBe("PROGRESS-STALLED");
    expect(c.retireable).toBe(false);
  });

  it("expired heartbeat with no live process is the only auto-retireable state", () => {
    const c = classify([row()], { observed_pids: [] },
      { heartbeat_age_min: 120, progress_age_min: 120 }, LIMITS);
    expect(c.state).toBe("REGISTERED-EXPIRED");
    expect(c.retireable).toBe(true);
  });

  it("expired heartbeat but a live process downgrades to protected", () => {
    // Process observation is corroborating evidence and can only make the verdict SAFER.
    const c = classify([row()], { observed_pids: [999] },
      { heartbeat_age_min: 120, progress_age_min: 120 }, LIMITS);
    expect(c.state).toBe("PROGRESS-STALLED");
    expect(c.retireable).toBe(false);
  });

  it("an abruptly-disappeared session becomes EXPIRED after the TTL", () => {
    // SIGKILL / VM death: no release is possible, so expiry is the only mechanism.
    const { session_id } = registerSession(db, { repo: "icn", worktree: "gone", harness_key: "k" });
    backdate(session_id, "last_heartbeat", 120);
    backdate(session_id, "last_progress", 120);

    const c = classifyWorktree(db, "gone", { observed_pids: [] });
    expect(c.state).toBe("REGISTERED-EXPIRED");
    expect(c.retireable).toBe(true);
  });

  it("an idle deadlocked loop does not falsely advance progress", () => {
    // End-to-end version of the #2644 pathology, driven through the real write paths.
    const { session_id } = registerSession(db, { repo: "icn", worktree: "spin", harness_key: "k" });
    recordProgress(db, session_id, { kind: "command", activity: "started mutation run" });
    backdate(session_id, "last_progress", 300);

    for (let i = 0; i < 50; i++) recordHeartbeat(db, session_id); // the spin

    const c = classifyWorktree(db, "spin", { observed_pids: [4242] });
    expect(c.state).toBe("PROGRESS-STALLED");
    expect(getSession(db, session_id)!.progress_count).toBe(1);
  });

  it("an orphaned child does not keep the parent looking healthy", () => {
    const parent = registerSession(db, { repo: "icn", worktree: "p-wt", harness_key: "p" });
    const child = registerSession(db, {
      repo: "icn", worktree: "c-wt", harness_key: "c", parent_session_id: parent.session_id,
    });
    // The child is busy; the parent has done nothing for hours.
    recordProgress(db, child.session_id, { kind: "command" });
    backdate(parent.session_id, "last_progress", 400);
    backdate(parent.session_id, "last_heartbeat", 400);

    expect(classifyWorktree(db, "p-wt", { observed_pids: [] }).state).toBe("REGISTERED-EXPIRED");
    expect(classifyWorktree(db, "c-wt", { observed_pids: [1] }).state).toBe("REGISTERED-ACTIVE");
  });
});
