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
  assertDurableProviderSessionId,
  endSupervision,
  InvalidProviderSessionIdError,
  liveSupervisions,
  recordInteraction,
  superviseOperation,
  SupervisionRejected,
  ageMinutes,
  classify,
  classifyWorktree,
  getSession,
  recordHeartbeat,
  recordProgress,
  registerSession,
  releaseSession,
  type ClassifyObservation,
  type SessionRow,
} from "../runtime/session-runtime.js";
import { type WorktreeIdentity } from "../runtime/worktree-identity.js";

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

/** Synthetic lane identity mirroring the shape Git produces. */
function ident(name: string, repo = "icn"): WorktreeIdentity {
  return {
    repo_id: `/repos/${repo}.git`,
    repo_name: repo,
    worktree_id: `/repos/${repo}.git/worktrees/${name}`,
    worktree_path: `/wt/${repo}/${name}`,
    worktree_name: name,
  };
}
const wtid = (name: string, repo = "icn") => `/repos/${repo}.git/worktrees/${name}`;

describe("schema v3 migration", () => {
  it("adds runtime identity columns without breaking pre-v3 inserts", () => {
    // The exact INSERT shape used before v3 must still work: existing callers select s.*.
    const legacy = db.prepare(
      "INSERT INTO sessions (id, repo, worktree, task_description) VALUES (?, ?, ?, ?)"
    );
    expect(() => legacy.run("legacy-1", "icn", "wt", "old style")).not.toThrow();

    const row = db.prepare("SELECT * FROM sessions WHERE id = 'legacy-1'").get() as SessionRow;
    expect(row.progress_count).toBe(0);
    expect(row.provider_session_id).toBeNull();
    expect(row.last_progress).toBeNull();
  });

  it("is idempotent when applied twice", () => {
    // initDb runs migrate(); running it again on the same file must not throw on ADD COLUMN.
    const again = initDb(":memory:");
    expect(() =>
      again.prepare("SELECT provider_session_id, worktree_id FROM sessions").all()
    ).not.toThrow();
    again.close();
  });
});

describe("registration", () => {
  it("registers once and returns the same session for a repeated harness key", () => {
    const a = registerSession(db, { repo: "icn", identity: ident("wt-a"), provider_session_id: "harness-1" });
    const b = registerSession(db, { repo: "icn", identity: ident("wt-a"), provider_session_id: "harness-1" });

    expect(a.created).toBe(true);
    expect(b.created).toBe(false);
    expect(b.deduplicated).toBe(true);
    expect(b.session_id).toBe(a.session_id);

    const count = db
      .prepare("SELECT COUNT(*) c FROM sessions WHERE provider_session_id = 'harness-1'")
      .get() as { c: number };
    expect(count.c).toBe(1);
  });

  it("preserves progress history across a duplicate registration", () => {
    // A SessionStart hook that fires twice (resume, compact) must not reset the evidence of work.
    const a = registerSession(db, { repo: "icn", provider_session_id: "harness-2" });
    recordProgress(db, a.session_id, { kind: "file_edit" });
    recordProgress(db, a.session_id, { kind: "command" });

    registerSession(db, { repo: "icn", provider_session_id: "harness-2" });

    expect(getSession(db, a.session_id)!.progress_count).toBe(2);
  });

  it("creates distinct sessions for distinct harness keys in the same worktree", () => {
    const a = registerSession(db, { repo: "icn", identity: ident("wt"), provider_session_id: "k1" });
    const b = registerSession(db, { repo: "icn", identity: ident("wt"), provider_session_id: "k2" });
    expect(a.session_id).not.toBe(b.session_id);
    expect(activeSessionsForWorktree(db, wtid("wt"))).toHaveLength(2);
  });

  it("does not deduplicate when no harness key is supplied", () => {
    // A launcher with no stable session id gets independent rows rather than silently
    // colliding with an unrelated session.
    const a = registerSession(db, { repo: "icn", identity: ident("wt") });
    const b = registerSession(db, { repo: "icn", identity: ident("wt") });
    expect(a.session_id).not.toBe(b.session_id);
  });
});

describe("heartbeat vs progress", () => {
  it("heartbeat advances liveness but NOT progress", () => {
    // This is the whole #2644 lesson: a spinning loop can heartbeat forever.
    const { session_id } = registerSession(db, { repo: "icn", provider_session_id: "k" });
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
    const { session_id } = registerSession(db, { repo: "icn", provider_session_id: "k" });
    const c0 = getSession(db, session_id)!.progress_count;
    recordProgress(db, session_id, { kind: "file_edit", activity: "editing lib.rs" });
    recordProgress(db, session_id, { kind: "test", activity: "cargo test" });
    const s = getSession(db, session_id)!;
    expect(s.progress_count).toBe(c0 + 2);
    expect(s.current_activity).toBe("cargo test");
  });

  it("neither heartbeat nor progress resurrects a released session", () => {
    const { session_id } = registerSession(db, { repo: "icn", provider_session_id: "k" });
    releaseSession(db, session_id, { reason: "completed" });
    expect(recordHeartbeat(db, session_id)).toBe(false);
    expect(recordProgress(db, session_id, { kind: "explicit" })).toBe(false);
  });
});

describe("release", () => {
  it("releases a session and drops its file claims", () => {
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("wt"), provider_session_id: "k" });
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
    const parent = registerSession(db, { repo: "icn", provider_session_id: "p" });
    const child = registerSession(db, {
      repo: "icn",
      provider_session_id: "c",
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
      id: "s1", repo: "icn", repo_id: "/repos/icn.git", worktree: "wt",
      worktree_id: wtid("wt"), worktree_path: "/wt/icn/wt", worktree_name: "wt",
      branch_at_registration: null, head_at_registration: null, task_description: null,
      task_ref: null, pr_ref: null, parent_session_id: null, provider: "claude-code",
      agent_pid: 1234, host: "icn-dev", provider_session_id: "k", transcript_path: null,
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
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("gone"), provider_session_id: "k" });
    backdate(session_id, "last_heartbeat", 120);
    backdate(session_id, "last_progress", 120);

    const c = classifyWorktree(db, wtid("gone"), { observed_pids: [] });
    expect(c.state).toBe("REGISTERED-EXPIRED");
    expect(c.retireable).toBe(true);
  });

  it("an idle deadlocked loop does not falsely advance progress", () => {
    // End-to-end version of the #2644 pathology, driven through the real write paths.
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("spin"), provider_session_id: "k" });
    recordProgress(db, session_id, { kind: "command", activity: "started mutation run" });
    backdate(session_id, "last_progress", 300);

    for (let i = 0; i < 50; i++) recordHeartbeat(db, session_id); // the spin

    const c = classifyWorktree(db, wtid("spin"), { observed_pids: [4242] });
    expect(c.state).toBe("PROGRESS-STALLED");
    expect(getSession(db, session_id)!.progress_count).toBe(1);
  });

  it("an orphaned child does not keep the parent looking healthy", () => {
    const parent = registerSession(db, { repo: "icn", identity: ident("p-wt"), provider_session_id: "p" });
    const child = registerSession(db, {
      repo: "icn", identity: ident("c-wt"), provider_session_id: "c", parent_session_id: parent.session_id,
    });
    // The child is busy; the parent has done nothing for hours.
    recordProgress(db, child.session_id, { kind: "command" });
    backdate(parent.session_id, "last_progress", 400);
    backdate(parent.session_id, "last_heartbeat", 400);

    expect(classifyWorktree(db, wtid("p-wt"), { observed_pids: [] }).state).toBe("REGISTERED-EXPIRED");
    expect(classifyWorktree(db, wtid("c-wt"), { observed_pids: [1] }).state).toBe("REGISTERED-ACTIVE");
  });
});


// ═══════════════════════════════════════════════════════════════════════════
// The four points resolved with evidence. Each block names the way the runtime
// could be wrong and pins the behaviour that prevents it.
// ═══════════════════════════════════════════════════════════════════════════

describe("1. session identity is the provider's stable id, never pid@host", () => {
  // Evidence captured from this installation's real hook payloads:
  //   - `session_id` is present on SessionStart/UserPromptSubmit/PostToolUse/Stop/SessionEnd
  //   - it is IDENTICAL across all five events within a session
  //   - it is UNCHANGED across `claude --resume <id>` (SessionStart source: "resume")
  //   - there is NO CLAUDE_SESSION_ID environment variable
  const PROVIDER_ID = "8718f1fc-b864-488a-a10b-cd30a6f0b856"; // a real captured value

  it("rejects a pid@host key outright, because PIDs are recycled", () => {
    expect(() => assertDurableProviderSessionId("29383@icn-dev")).toThrow(InvalidProviderSessionIdError);
    expect(() => registerSession(db, { repo: "icn", provider_session_id: "29383@icn-dev" })).toThrow(
      InvalidProviderSessionIdError
    );
    expect(db.prepare("SELECT COUNT(*) c FROM sessions").get()).toEqual({ c: 0 });
  });

  it("accepts the provider session id", () => {
    expect(() => assertDurableProviderSessionId(PROVIDER_ID)).not.toThrow();
  });

  it("duplicate hook invocations with the real provider id register exactly once", () => {
    // The concrete failure this prevents: SessionStart firing for startup AND resume, or a
    // hook being retried, silently creating two rows that each look authoritative.
    const results = [];
    for (let i = 0; i < 5; i++) {
      results.push(registerSession(db, {
        repo: "icn", identity: ident("wt"), provider_session_id: PROVIDER_ID,
        transcript_path: "/home/ubuntu/.claude/projects/x/y.jsonl",
      }));
    }
    expect(results.filter((r) => r.created)).toHaveLength(1);
    expect(new Set(results.map((r) => r.session_id)).size).toBe(1);
    expect(db.prepare("SELECT COUNT(*) c FROM sessions").get()).toEqual({ c: 1 });
  });

  it("a resumed session reattaches to its existing row rather than forking a new lifecycle", () => {
    // `--resume` re-fires SessionStart with source="resume" and the SAME session_id.
    const first = registerSession(db, { repo: "icn", identity: ident("wt"), provider_session_id: PROVIDER_ID });
    recordProgress(db, first.session_id, { kind: "file_edit" });
    const resumed = registerSession(db, {
      repo: "icn", identity: ident("wt"), provider_session_id: PROVIDER_ID,
    });
    expect(resumed.session_id).toBe(first.session_id);
    const row = getSession(db, first.session_id)!;
    expect(row.progress_count).toBe(1);      // history survives
    // Identity is untouched by anything about the branch.
    expect(row.worktree_id).toBe(wtid("wt"));
  });

  it("release then resume with the same provider id starts a clean lifecycle, not a conflict", () => {
    // Observed for real: SessionEnd(reason=other) then SessionStart(source=resume), same id.
    const a = registerSession(db, { repo: "icn", provider_session_id: PROVIDER_ID });
    releaseSession(db, a.session_id, { reason: "other" });
    const b = registerSession(db, { repo: "icn", provider_session_id: PROVIDER_ID });
    expect(b.created).toBe(true);
    expect(b.session_id).not.toBe(a.session_id);
  });

  it("the unique index makes a duplicate active row impossible even by direct insert", () => {
    registerSession(db, { repo: "icn", provider_session_id: PROVIDER_ID });
    expect(() =>
      db.prepare("INSERT INTO sessions (id, repo, harness_key) VALUES (?, ?, ?)")
        .run("forced-dup", "icn", PROVIDER_ID)
    ).toThrow();
  });
});

describe("2. a completed turn is interaction, not progress", () => {
  it("repeated nonproductive turns cannot defeat PROGRESS-STALLED", () => {
    // The precise regression: an agent that keeps answering but never advances task state.
    // If Stop counted as progress, this lane would read healthy forever.
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("chatty"), provider_session_id: "k" });
    recordProgress(db, session_id, { kind: "command", activity: "kicked off work" });
    backdate(session_id, "last_progress", 300);

    for (let i = 0; i < 200; i++) recordInteraction(db, session_id); // 200 completed turns

    const row = getSession(db, session_id)!;
    expect(row.progress_count).toBe(1);                       // unchanged by turns
    expect(ageMinutes(db, row.last_heartbeat)!).toBeLessThan(1); // but demonstrably live
    expect(ageMinutes(db, row.last_progress)!).toBeGreaterThan(290);

    const c = classifyWorktree(db, wtid("chatty"), { observed_pids: [4242] });
    expect(c.state).toBe("PROGRESS-STALLED");
  });

  it("interaction alone never creates the appearance of progress on a fresh session", () => {
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("w"), provider_session_id: "k" });
    for (let i = 0; i < 20; i++) recordInteraction(db, session_id);
    const row = getSession(db, session_id)!;
    expect(row.progress_count).toBe(0);
    expect(row.last_progress).toBeNull();
  });

  it("'turn' is not a spellable progress kind", () => {
    // Type-level in TS; asserted at runtime so the tool schema cannot quietly re-add it.
    const kinds = ["file_edit", "command", "test", "task_state", "explicit"];
    expect(kinds).not.toContain("turn");
  });

  it("real work still advances progress, so the signal is not simply dead", () => {
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("w"), provider_session_id: "k" });
    recordInteraction(db, session_id);
    recordProgress(db, session_id, { kind: "file_edit" });
    recordInteraction(db, session_id);
    expect(getSession(db, session_id)!.progress_count).toBe(1);
  });
});

describe("3. release surrenders every session-scoped authority", () => {
  function seedAuthority(sessionId: string) {
    db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)")
      .run("icn/crates/a/src/lib.rs", sessionId);
    superviseOperation(db, sessionId, process.pid, "cargo test");
    db.prepare(
      "INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES (?, ?, 'text', '{}', ?)"
    ).run(sessionId, "someone-else", Date.now());
  }

  it("drops file claims, invalidates watchers, and clears the undelivered inbox", () => {
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("wt"), provider_session_id: "k" });
    seedAuthority(session_id);

    const res = releaseSession(db, session_id, { reason: "completed" });

    expect(res.dropped).toEqual({ file_claims: 1, watchers: 1, undelivered_messages: 1 });
    expect(db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(session_id))
      .toEqual({ c: 0 });
    // The watcher row survives as history but is no longer 'running', so the background
    // poller stops supervising on behalf of a session that no longer exists.
    const w = db.prepare("SELECT status FROM watchers_process WHERE session_id = ?").get(session_id);
    expect(w).toEqual({ status: "released" });
    const pending = db
      .prepare("SELECT COUNT(*) c FROM mailbox WHERE to_session = ? AND read_at IS NULL")
      .get(session_id);
    expect(pending).toEqual({ c: 0 });
  });

  it("a released session cannot be resurrected into holding authority again", () => {
    const { session_id } = registerSession(db, { repo: "icn", provider_session_id: "k" });
    seedAuthority(session_id);
    releaseSession(db, session_id);

    expect(getSession(db, session_id)).toBeUndefined();
    expect(recordHeartbeat(db, session_id)).toBe(false);
    expect(liveSupervisions(db, session_id)).toEqual([]);
  });

  it("keeps history that carries no authority: sender attribution and the event log", () => {
    const { session_id } = registerSession(db, { repo: "icn", provider_session_id: "k" });
    db.prepare(
      "INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES ('other', ?, 'text', '{}', ?)"
    ).run(session_id, Date.now());
    db.prepare("INSERT INTO events (scope, type, payload, created_at) VALUES (?, 'x', '{}', ?)")
      .run(`session:${session_id}`, Date.now());

    releaseSession(db, session_id);

    expect(db.prepare("SELECT COUNT(*) c FROM mailbox WHERE from_session = ?").get(session_id))
      .toEqual({ c: 1 });
    expect(db.prepare("SELECT COUNT(*) c FROM events WHERE scope = ?").get(`session:${session_id}`))
      .toEqual({ c: 1 });
  });

  it("cascade semantics for file_claims are unchanged from before this runtime", () => {
    // Guard against a future "keep history" refactor silently leaving claims behind.
    const id = "legacy-cascade";
    db.prepare("INSERT INTO sessions (id, repo) VALUES (?, 'icn')").run(id);
    db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES ('f.rs', ?)").run(id);
    db.prepare("DELETE FROM sessions WHERE id = ?").run(id);
    expect(db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(id))
      .toEqual({ c: 0 });
  });
});

describe("4. a supervised long-running operation is not abandonment", () => {
  it("stale heartbeat + live supervised build is NOT expired and NOT retireable", () => {
    // The 45-minute cargo build against a 30-minute TTL. No hook fires for the whole run.
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("build"), provider_session_id: "k" });
    superviseOperation(db, session_id, process.pid, "cargo test --workspace");
    backdate(session_id, "last_heartbeat", 45);
    backdate(session_id, "last_progress", 45);

    const c = classifyWorktree(db, wtid("build"), { observed_pids: [process.pid] });

    expect(c.state).toBe("SUPERVISED-BUSY");
    expect(c.retireable).toBe(false);
    // Protected from AUTOMATIC retirement, but an operator may still retire a lane holding a
    // runaway build. Reporting this as untouchable put supervision above every verdict the
    // runtime produces, so one call with any long-lived pid exempted a lane permanently.
    expect(c.retireable_with_approval).toBe(true);
    expect(c.reason).toContain("cargo test --workspace");
  });

  it("refuses to supervise a process that is not running", () => {
    // Without this, `supervise --pid 1` (or any long-lived pid) permanently exempted a lane:
    // the reap only fires when the process dies, and init never does.
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("build"), provider_session_id: "k" });
    expect(() => superviseOperation(db, session_id, 999_999_999, "cargo build")).toThrow(
      SupervisionRejected
    );
    expect(() => superviseOperation(db, session_id, 1, "cargo build")).toThrow(SupervisionRejected);
    expect(() => superviseOperation(db, "no-such-session", process.pid, "x")).toThrow(
      SupervisionRejected
    );
    expect(db.prepare("SELECT COUNT(*) c FROM watchers_process").get()).toEqual({ c: 0 });
  });

  it("reaps a supervision whose process has since died", () => {
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("build"), provider_session_id: "k" });
    superviseOperation(db, session_id, process.pid, "cargo build");
    backdate(session_id, "last_heartbeat", 120);
    backdate(session_id, "last_progress", 120);

    // Simulate the process dying after declaration.
    const c = classifyWorktree(db, wtid("build"), {
      observed_pids: [],
      live_supervisions: [],
    });
    expect(c.state).toBe("REGISTERED-EXPIRED");
    expect(c.retireable).toBe(true);
  });

  it("expires a supervision that outlives its bound even if the pid never dies", () => {
    // A daemon, a wedged process, or a mistakenly-supervised long-lived pid must not protect a
    // lane indefinitely just because it refuses to exit.
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("build"), provider_session_id: "k" });
    const sup = superviseOperation(db, session_id, process.pid, "endless");
    db.prepare("UPDATE watchers_process SET created_at = ? WHERE id = ?")
      .run(Date.now() - 999 * 60_000, sup);

    expect(liveSupervisions(db, session_id)).toHaveLength(0);
    expect(
      db.prepare("SELECT status FROM watchers_process WHERE id = ?").get(sup)
    ).toEqual({ status: "expired" });
  });

  it("supervision expires when explicitly ended, restoring normal judgement", () => {
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("build"), provider_session_id: "k" });
    const sup = superviseOperation(db, session_id, process.pid, "long build");
    backdate(session_id, "last_heartbeat", 90);
    backdate(session_id, "last_progress", 90);
    expect(classifyWorktree(db, wtid("build"), { observed_pids: [] }).state).toBe("SUPERVISED-BUSY");

    endSupervision(db, sup, 0);

    expect(classifyWorktree(db, wtid("build"), { observed_pids: [] }).state).toBe("REGISTERED-EXPIRED");
  });

  it("supervision is not a back door to faking progress", () => {
    // A supervised lane is PROTECTED, but its progress evidence must be untouched — otherwise
    // "declare a supervision" would become the fake-heartbeat we banned.
    const { session_id } = registerSession(db, { repo: "icn", identity: ident("build"), provider_session_id: "k" });
    superviseOperation(db, session_id, process.pid, "sleep");
    backdate(session_id, "last_progress", 400);
    const c = classifyWorktree(db, wtid("build"), { observed_pids: [] });
    expect(c.progress_count).toBe(0);
    expect(c.progress_age_min!).toBeGreaterThan(390);
  });

  it("an unregistered lane gains nothing from supervision (still fail-safe)", () => {
    const c = classify([], { observed_pids: [1], live_supervisions: [
      { id: 1, pid: process.pid, label: "x", created_at: Date.now(), worktree_id: null },
    ] }, { heartbeat_age_min: null, progress_age_min: null }, LIMITS);
    expect(c.state).toBe("UNREGISTERED-OBSERVED");
    expect(c.retireable).toBe(false);
  });
});


// ═══════════════════════════════════════════════════════════════════════════
// Fail-safe observation semantics.
//
// The defect these pin: every caller defaulted observed_pids to [], so "nobody looked" was
// consumed as "nothing is there". A lane whose registered agent process was demonstrably
// ALIVE classified as REGISTERED-EXPIRED / retireable:true, with a reason string asserting
// "no process holds the worktree" that nothing had ever checked.
// ═══════════════════════════════════════════════════════════════════════════

describe("absence of observation is not evidence of absence", () => {
  function row(over: Partial<SessionRow> = {}): SessionRow {
    return {
      id: "s1", repo: "icn", repo_id: "/repos/icn.git", worktree: "wt",
      worktree_id: wtid("wt"), worktree_path: "/wt/icn/wt", worktree_name: "wt",
      branch_at_registration: null, head_at_registration: null, task_description: null,
      task_ref: null, pr_ref: null, parent_session_id: null, provider: "claude-code",
      agent_pid: null, host: "icn-dev", provider_session_id: "k", transcript_path: null,
      current_activity: null, progress_count: 3, started_at: "2026-08-23 00:00:00",
      last_heartbeat: "2026-08-23 00:00:00", last_progress: "2026-08-23 00:00:00",
      ...over,
    };
  }
  const EXPIRED = { heartbeat_age_min: 120, progress_age_min: 120 };

  it("an expired heartbeat with NO observation supplied is not retireable", () => {
    const c = classify([row()], {}, EXPIRED, LIMITS);
    expect(c.state).toBe("REGISTERED-EXPIRED");
    expect(c.retireable).toBe(false);
    expect(c.retireable_with_approval).toBe(true);
    expect(c.reason).toContain("no process observation");
  });

  it("null observed_pids is treated the same as omitting it", () => {
    const c = classify([row()], { observed_pids: null }, EXPIRED, LIMITS);
    expect(c.retireable).toBe(false);
  });

  it("an AFFIRMATIVE empty observation is what unlocks retirement", () => {
    const c = classify([row()], { observed_pids: [] }, EXPIRED, LIMITS);
    expect(c.state).toBe("REGISTERED-EXPIRED");
    expect(c.retireable).toBe(true);
    expect(c.reason).toContain("an observation was performed");
  });

  it("a live recorded agent_pid protects the lane even with an affirmative empty observation", () => {
    // The registry corroborates itself: it recorded the pid, so it can check it without any
    // caller cooperation.
    const c = classify([row({ agent_pid: 4242 })], { observed_pids: [], agent_pid_alive: true },
      EXPIRED, LIMITS);
    expect(c.state).toBe("PROGRESS-STALLED");
    expect(c.retireable).toBe(false);
    expect(c.reason).toContain("agent process");
  });

  it("end to end: a live agent process is never retireable, however the caller asks", () => {
    const { session_id } = registerSession(db, {
      repo: "icn", identity: ident("live"), provider_session_id: "k", agent_pid: process.pid,
    });
    backdate(session_id, "last_heartbeat", 120);
    backdate(session_id, "last_progress", 120);

    for (const obs of [{}, { observed_pids: null }, { observed_pids: [] }] as ClassifyObservation[]) {
      const c = classifyWorktree(db, wtid("live"), obs);
      expect(c.retireable).toBe(false);
    }
  });
});

describe("a session that has never progressed can still stall", () => {
  it("falls back to age since registration, not heartbeat staleness", () => {
    // Previously unreachable: the fallback used heartbeat AGE, and control only reaches the
    // stall check when that age is under the TTL — which can never exceed the stall window.
    const { session_id } = registerSession(db, {
      repo: "icn", identity: ident("idle"), provider_session_id: "k", agent_pid: process.pid,
    });
    // Registered hours ago, heartbeating constantly, never once progressed.
    db.prepare("UPDATE sessions SET started_at = datetime('now','-400 minutes') WHERE id = ?")
      .run(session_id);
    for (let i = 0; i < 20; i++) recordInteraction(db, session_id);

    const c = classifyWorktree(db, wtid("idle"), { observed_pids: [process.pid] });
    expect(c.state).toBe("PROGRESS-STALLED");
    expect(c.progress_count).toBe(0);
    expect(c.reason).toContain("NO progress has ever been recorded");
  });

  it("does not stall a young session that simply has not progressed yet", () => {
    const { session_id } = registerSession(db, {
      repo: "icn", identity: ident("young"), provider_session_id: "k",
    });
    recordInteraction(db, session_id);
    expect(classifyWorktree(db, wtid("young"), { observed_pids: [1] }).state)
      .toBe("REGISTERED-ACTIVE");
  });
});

describe("a resumed conversation that reappears in another lane", () => {
  it("moves the row to where the session actually is, and says so", () => {
    const a = registerSession(db, {
      repo: "icn", identity: ident("laneA"), provider_session_id: "conv",
    });
    const b = registerSession(db, {
      repo: "icn", identity: ident("laneB"), provider_session_id: "conv",
    });

    expect(b.session_id).toBe(a.session_id);
    expect(b.deduplicated).toBe(true);
    // The returned lane must match the STORED lane — previously it reported the caller's lane
    // while the row still pointed at the old one.
    expect(b.worktree_id).toBe(wtid("laneB"));
    expect(b.lane_changed).toEqual({ from: wtid("laneA"), to: wtid("laneB") });

    expect(activeSessionsForWorktree(db, wtid("laneA"))).toHaveLength(0);
    expect(activeSessionsForWorktree(db, wtid("laneB"))).toHaveLength(1);
  });

  it("does not report a lane change when the lane is unchanged", () => {
    registerSession(db, { repo: "icn", identity: ident("same"), provider_session_id: "conv" });
    const again = registerSession(db, {
      repo: "icn", identity: ident("same"), provider_session_id: "conv",
    });
    expect(again.lane_changed).toBeUndefined();
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// Round-2 independent review: protection is a property of the LANE.
//
// The defect these pin: classifyWorktree selected one "primary" session by freshest heartbeat
// — the very signal this module argues is not liveness — and judged the whole lane from it.
// A crashed co-occupant that heartbeated more recently became primary, its dead pid was the
// only one checked, and a live agent's recorded pid was never looked at.
// ═══════════════════════════════════════════════════════════════════════════

describe("a lane is protected by ANY live occupant", () => {
  it("a live agent protects the lane even when a crashed peer has a fresher heartbeat", () => {
    const live = registerSession(db, {
      repo: "icn", identity: ident("shared"), provider_session_id: "live", agent_pid: process.pid,
    });
    const dead = registerSession(db, {
      repo: "icn", identity: ident("shared"), provider_session_id: "dead", agent_pid: 999_999_999,
    });
    backdate(live.session_id, "last_heartbeat", 40);
    backdate(live.session_id, "last_progress", 40);
    backdate(dead.session_id, "last_heartbeat", 35); // fresher -> becomes "primary"
    backdate(dead.session_id, "last_progress", 35);

    const c = classifyWorktree(db, wtid("shared"), { observed_pids: [] });

    expect(c.retireable).toBe(false);
    expect(c.contention.count).toBe(2);
    // ...and the reason must name the pid that is actually alive, not primary's dead one.
    expect(c.reason).toContain(String(process.pid));
    expect(c.reason).not.toContain("999999999");
  });

  it("only retires when EVERY recorded pid on the lane is gone", () => {
    const a = registerSession(db, {
      repo: "icn", identity: ident("gone"), provider_session_id: "a", agent_pid: 999_999_998,
    });
    const b = registerSession(db, {
      repo: "icn", identity: ident("gone"), provider_session_id: "b", agent_pid: 999_999_999,
    });
    for (const s of [a.session_id, b.session_id]) {
      backdate(s, "last_heartbeat", 120);
      backdate(s, "last_progress", 120);
    }
    const c = classifyWorktree(db, wtid("gone"), { observed_pids: [] });
    expect(c.state).toBe("REGISTERED-EXPIRED");
    expect(c.retireable).toBe(true);
  });
});

describe("a supervision belongs to the lane, not to the session that declared it", () => {
  it("protects the lane where the build runs after the session moves away", () => {
    // A build runs in a DIRECTORY. When a resumed conversation re-registered from another
    // worktree the row moved and took the supervision with it, leaving the lane with the
    // running build reading retireable:true.
    const a = registerSession(db, {
      repo: "icn", identity: ident("laneA"), provider_session_id: "conv",
    });
    superviseOperation(db, a.session_id, process.pid, "cargo test --workspace");
    const peer = registerSession(db, {
      repo: "icn", identity: ident("laneA"), provider_session_id: "peer", agent_pid: 999_999_999,
    });
    backdate(a.session_id, "last_heartbeat", 90);
    backdate(peer.session_id, "last_heartbeat", 90);
    backdate(peer.session_id, "last_progress", 90);

    registerSession(db, { repo: "icn", identity: ident("laneB"), provider_session_id: "conv" });

    const c = classifyWorktree(db, wtid("laneA"), { observed_pids: [] });
    expect(c.state).toBe("SUPERVISED-BUSY");
    expect(c.retireable).toBe(false);
    expect(c.reason).toContain("cargo test --workspace");
  });

  it("does not reap plain watch_process rows, which belong to the background poller", () => {
    // watch_process writes to the same table with a NULL lane. Reaping those here stole the
    // poller's completion events and mailbox alerts from a pre-existing feature.
    const s = registerSession(db, { repo: "icn", identity: ident("w"), provider_session_id: "k" });
    db.prepare(
      "INSERT INTO watchers_process (session_id, pid, label, created_at, status) VALUES (?, ?, ?, ?, 'running')"
    ).run(s.session_id, 999_999_999, "docker build", Date.now());

    liveSupervisions(db, s.session_id);
    classifyWorktree(db, wtid("w"), { observed_pids: [] });

    expect(
      db.prepare("SELECT status FROM watchers_process WHERE label = 'docker build'").get()
    ).toEqual({ status: "running" });
  });
});

describe("supervision explains staleness; it never makes a healthy lane less protected", () => {
  it("a healthy lane stays REGISTERED-ACTIVE while a build is declared", () => {
    const s = registerSession(db, { repo: "icn", identity: ident("busy"), provider_session_id: "k" });
    recordProgress(db, s.session_id, { kind: "command" });
    superviseOperation(db, s.session_id, process.pid, "cargo build");

    const c = classifyWorktree(db, wtid("busy"), { observed_pids: [] });
    expect(c.state).toBe("REGISTERED-ACTIVE");
    expect(c.retireable_with_approval).toBe(false);
  });
});

describe("the TTL cannot be driven low enough to make everything retireable", () => {
  it("clamps an absurd ICN_SESSION_TTL_MINUTES to the floor", () => {
    const s = registerSession(db, { repo: "icn", identity: ident("ttl"), provider_session_id: "k" });
    backdate(s.session_id, "last_heartbeat", 2);
    backdate(s.session_id, "last_progress", 2);
    const c = classifyWorktree(db, wtid("ttl"), { observed_pids: [] },
      { ICN_SESSION_TTL_MINUTES: "1" } as NodeJS.ProcessEnv);
    expect(c.state).toBe("REGISTERED-ACTIVE");
    expect(c.retireable).toBe(false);
  });
});
