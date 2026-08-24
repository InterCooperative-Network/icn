// THE THREE-STATE OBSERVATION CONTRACT (B2 — the Round 8 P0)
//
//   null = nobody looked        -> lane stays protected
//   []   = looked, found none   -> the ONLY affirmative form
//   [..] = found these holders
//
// The distinction is the whole point of this layer: a consumer applying a retirement policy
// treats `[]` as the strongest possible evidence that a lane is free. Garbage arriving from a
// shell must therefore never become `[]`.
//
// The shipped guard was `tokens.length > 0 && parsed.length === 0`, which let every
// whitespace/separator-only value through to the affirmative. Measured before the fix: with a
// live process holding the lane, `--pids ' , '` produced the byte-identical envelope to
// `--observed-none` — "an observation was performed and found no process holding the worktree".

import { describe, it, expect } from "vitest";
import { initDb } from "../state/db.js";
import {
  classify,
  classifyWorktree,
  DEFAULT_STALL_MINUTES,
  MIN_STALL_MINUTES,
  parseObservedPids,
  registerSession,
  stallMinutes,
  type SessionRow,
} from "../runtime/session-runtime.js";

const LIMITS = { ttl_min: 30, stall_min: 90 };

describe("parseObservedPids — garbage may never become an affirmative observation", () => {
  it("absent --pids and no flag means NOBODY LOOKED", () => {
    expect(parseObservedPids(undefined, false).observed_pids).toBeNull();
  });

  it("--observed-none is the one affirmative empty observation", () => {
    expect(parseObservedPids(undefined, true).observed_pids).toEqual([]);
  });

  it("a valid pid list is taken exactly as given", () => {
    expect(parseObservedPids("123", false).observed_pids).toEqual([123]);
    expect(parseObservedPids("123,456,789", false).observed_pids).toEqual([123, 456, 789]);
    expect(parseObservedPids(" 123 , 456 ", false).observed_pids).toEqual([123, 456]);
  });

  it.each([
    ["whitespace only", "  "],
    ["a single tab", "\t"],
    ["comma only", ","],
    ["comma with spaces", " , "],
    ["many empty separators", ", ,,"],
    ["trailing separator only", ","],
    ["word garbage", "abc"],
    ["an error message", "lsof: command not found"],
    ["zero", "0"],
    ["negative", "-1"],
    ["negative one (kill -1 signals everything)", "-1,-1"],
    ["float", "1.5"],
    ["hex", "0x10"],
    ["mixed valid and invalid", "123,abc"],
    ["valid then zero", "123,0"],
    ["valid then negative", "123,-4"],
  ])("%s yields NO OBSERVATION, never []", (_label, raw) => {
    const r = parseObservedPids(raw, false);
    expect(r.observed_pids).toBeNull();
    expect(r.warning).toBeTruthy();
  });

  it("--observed-none does not rescue a malformed --pids", () => {
    // An explicit list that could not be read is a FAILED observation even when the flag is
    // also present: the caller tried to look and could not.
    expect(parseObservedPids(" , ", true).observed_pids).toBeNull();
  });
});

describe("the classifier reads those three states differently", () => {
  function row(over: Partial<SessionRow> = {}): SessionRow {
    return {
      id: "s1", repo: "icn", repo_id: "/r", worktree: "wt",
      worktree_id: "/r/worktrees/wt", worktree_path: "/wt", worktree_name: "wt",
      worktree_generation: null,
      branch_at_registration: null, head_at_registration: null, task_description: null,
      task_ref: null, pr_ref: null, parent_session_id: null, provider: null,
      agent_pid: null, host: null, provider_session_id: "k", transcript_path: null,
      current_activity: null, progress_count: 0,
      started_at: "2026-01-01 00:00:00", last_heartbeat: "2026-01-01 00:00:00",
      last_progress: null,
      ...over,
    };
  }
  const expired = { heartbeat_age_min: 120, progress_age_min: null, session_age_min: 200 };

  it("distinguishes 'nobody looked' from 'looked and found nothing' IN THE REASON", () => {
    const nobody = classify([row()], { observed_pids: null }, expired, LIMITS);
    const looked = classify([row()], { observed_pids: [] }, expired, LIMITS);

    expect(nobody.reason).toMatch(/no process observation was supplied|absence of observation/i);
    expect(looked.reason).toMatch(/an observation was performed/i);
    // The two must not be describable by the same sentence — that collapse IS the defect.
    expect(nobody.reason).not.toBe(looked.reason);
  });

  it("a garbage --pids classifies IDENTICALLY to no observation at all", () => {
    const viaGarbage = classify(
      [row()],
      { observed_pids: parseObservedPids(" , ", false).observed_pids },
      expired,
      LIMITS
    );
    const viaAbsence = classify([row()], { observed_pids: null }, expired, LIMITS);
    expect(viaGarbage.reason).toBe(viaAbsence.reason);
    expect(viaGarbage.state).toBe(viaAbsence.state);
    // ...and NOT like the affirmative one.
    const affirmative = classify([row()], { observed_pids: [] }, expired, LIMITS);
    expect(viaGarbage.reason).not.toBe(affirmative.reason);
  });

  it("every classification envelope carries the full required field set", () => {
    // A consumer typed against Classification reads `.live_agent_pids.length` and throws on a
    // partial object, so a degraded answer that omits fields is unparseable, not safer.
    const REQUIRED = [
      "state", "reason", "session_id", "heartbeat_age_min", "progress_age_min",
      "progress_count", "contention", "branch_changed", "live_branch", "live_agent_pids",
    ];
    const db = initDb(":memory:");
    const envelopes = [
      classify([], { observed_pids: null }, expired, LIMITS),
      classify([], { observed_pids: [] }, expired, LIMITS),
      classify([row()], { observed_pids: null }, expired, LIMITS),
      classify([row()], { observed_pids: [] }, expired, LIMITS),
      classify([row()], { registry_unavailable: true, observed_pids: null }, expired, LIMITS),
      classifyWorktree(db, "/nonexistent/worktree/id", { observed_pids: null }),
      classifyWorktree(db, "/nonexistent/worktree/id", { observed_pids: [] }),
    ];
    for (const e of envelopes) {
      for (const key of REQUIRED) {
        expect(Object.keys(e)).toContain(key);
      }
      expect(Array.isArray(e.live_agent_pids)).toBe(true);
      expect(typeof e.contention.count).toBe("number");
      expect(Array.isArray(e.contention.session_ids)).toBe(true);
    }
  });
});

describe("provider session id — the shapes that reached the unique index", () => {
  it("an EMPTY id is stored as NULL, so it can never poison the unique index", () => {
    const db = initDb(":memory:");
    const first = registerSession(db, { repo: "icn", provider_session_id: "" });
    expect(first.created).toBe(true);
    expect(first.provider_session_id).toBeNull();
    const stored = db.prepare("SELECT provider_session_id AS p FROM sessions").all() as Array<{
      p: string | null;
    }>;
    expect(stored.every((r) => r.p === null)).toBe(true);

    // Before the fix this threw SQLITE_CONSTRAINT_UNIQUE: "" was stored NON-NULL (so the
    // partial unique index covered it) but was FALSY (so the dedupe SELECT was skipped), and
    // every later session carrying "" died unregistered.
    const second = registerSession(db, { repo: "icn", provider_session_id: "" });
    expect(second.created).toBe(true);
    expect(second.session_id).not.toBe(first.session_id);
  });

  it("an absent id is also NULL, and two of them do not collide", () => {
    const db = initDb(":memory:");
    const a = registerSession(db, { repo: "icn" });
    const b = registerSession(db, { repo: "icn" });
    expect(a.session_id).not.toBe(b.session_id);
  });

  it("a REAL id still dedupes to one live activation", () => {
    const db = initDb(":memory:");
    const a = registerSession(db, { repo: "icn", provider_session_id: "conv-1" });
    const b = registerSession(db, { repo: "icn", provider_session_id: "conv-1" });
    expect(b.deduplicated).toBe(true);
    expect(b.session_id).toBe(a.session_id);
    expect(
      (db.prepare("SELECT COUNT(*) AS c FROM sessions").get() as { c: number }).c
    ).toBe(1);
  });
});

// ── B3: progress is a property of the LANE, not of the freshest heartbeat ───
describe("lane progress aggregates across every occupant", () => {
  function lane(db: ReturnType<typeof initDb>, key: string, wtId: string) {
    return registerSession(db, {
      repo: "icn",
      provider_session_id: key,
      identity: {
        repo_id: "/r", repo_name: "icn",
        worktree_id: wtId, worktree_path: "/wt", worktree_name: "wt",
        worktree_generation: "gen-1",
      },
    });
  }
  const WT = "/r/worktrees/shared";

  function setup() {
    const db = initDb(":memory:");
    const a = lane(db, "occupant-a", WT);
    const b = lane(db, "occupant-b", WT);
    // Both registered long ago, so "no progress" would otherwise read as stalled.
    db.prepare("UPDATE sessions SET started_at = datetime('now','-200 minutes')").run();
    return { db, a: a.session_id, b: b.session_id };
  }

  /**
   * Age ONE occupant's column. The aggregation tests below need occupants at DIFFERENT
   * timestamps: an earlier version of this fixture ran a single UPDATE with no WHERE clause, so
   * every occupant got the same value — which meant min and max were indistinguishable and
   * mutants swapping them survived the whole suite.
   */
  const ageOne = (
    db: ReturnType<typeof initDb>,
    id: string,
    col: "started_at" | "last_progress" | "last_heartbeat",
    minutes: number
  ) =>
    db
      .prepare(`UPDATE sessions SET ${col} = datetime('now', ?) WHERE id = ?`)
      .run(`-${minutes} minutes`, id);
  const progress = (db: ReturnType<typeof initDb>, id: string) =>
    db.prepare(
      "UPDATE sessions SET last_progress = datetime('now'), progress_count = progress_count + 1, last_heartbeat = datetime('now') WHERE id = ?"
    ).run(id);
  const beat = (db: ReturnType<typeof initDb>, id: string) =>
    db.prepare("UPDATE sessions SET last_heartbeat = datetime('now') WHERE id = ?").run(id);

  it("ONLY the first occupant progresses — the lane is still progressing", () => {
    const { db, a, b } = setup();
    progress(db, a);
    beat(db, b); // b is fresher by heartbeat, so it becomes `primary`
    const c = classifyWorktree(db, WT, { observed_pids: [] });
    expect(c.progress_count).toBe(1);
    expect(c.progress_age_min).not.toBeNull();
    expect(c.state).toBe("REGISTERED-ACTIVE");
    expect(c.reason).not.toMatch(/NO progress has ever been recorded/);
  });

  it("ONLY the second occupant progresses — same answer", () => {
    const { db, a, b } = setup();
    progress(db, b);
    beat(db, a);
    const c = classifyWorktree(db, WT, { observed_pids: [] });
    expect(c.progress_count).toBe(1);
    expect(c.state).toBe("REGISTERED-ACTIVE");
  });

  it("BOTH progress — counts are summed for the lane", () => {
    const { db, a, b } = setup();
    progress(db, a);
    progress(db, b);
    progress(db, b);
    expect(classifyWorktree(db, WT, { observed_pids: [] }).progress_count).toBe(3);
  });

  it("interaction-only occupancy NEVER becomes progress", () => {
    const { db, a, b } = setup();
    for (let i = 0; i < 50; i++) {
      beat(db, a);
      beat(db, b);
    }
    const c = classifyWorktree(db, WT, { observed_pids: [] });
    expect(c.progress_count).toBe(0);
    expect(c.progress_age_min).toBeNull();
    expect(c.state).toBe("PROGRESS-STALLED");
  });

  it("the working occupant's progress survives the other one being RELEASED", () => {
    const { db, a, b } = setup();
    progress(db, a);
    db.prepare("DELETE FROM sessions WHERE id = ?").run(b);
    const c = classifyWorktree(db, WT, { observed_pids: [] });
    expect(c.contention.count).toBe(1);
    expect(c.progress_count).toBe(1);
  });

  it("the FRESHEST progress on the lane wins (min, not max)", () => {
    const { db, a, b } = setup();
    progress(db, a);
    progress(db, b);
    ageOne(db, a, "last_progress", 300); // one occupant went quiet long ago...
    // ...the other progressed just now, so the LANE progressed just now.
    const c = classifyWorktree(db, WT, { observed_pids: [] });
    expect(c.progress_age_min).not.toBeNull();
    expect(c.progress_age_min!).toBeLessThan(5);
    expect(c.state).toBe("REGISTERED-ACTIVE");
  });

  it("the stall window is measured from the OLDEST registration (max, not min)", () => {
    const { db, b } = setup();
    // A newcomer joined a minute ago; the lane has still been occupied for 200 minutes.
    ageOne(db, b, "started_at", 1);
    const c = classifyWorktree(db, WT, { observed_pids: [] });
    expect(c.state).toBe("PROGRESS-STALLED");
    expect(c.reason).toMatch(/200\.\d min after registration/);
  });

  it("the lane heartbeat follows the MOST LIVE occupant, not the deadest", () => {
    // classifyWorktree's own contract: "a busy reviewer must not let a dead editor look
    // retireable". Liveness and progress were both aggregated; the heartbeat dimension was
    // pinned by nothing, so reporting the OLDEST heartbeat instead of the freshest left the
    // whole suite green while turning an actively-working lane into REGISTERED-EXPIRED.
    const { db, a, b } = setup();
    progress(db, b); // b is working right now
    ageOne(db, a, "last_heartbeat", 400); // a is long dead
    const c = classifyWorktree(db, WT, { observed_pids: [] });
    expect(c.heartbeat_age_min).not.toBeNull();
    expect(c.heartbeat_age_min!).toBeLessThan(5);
    expect(c.state).toBe("REGISTERED-ACTIVE");
    expect(c.reason).not.toMatch(/an observation was performed and found no process/);
  });

});

// ── kept P2s, each with a compact regression ────────────────────────────────
describe("environment-tunable windows are floored", () => {
  it("the stall window cannot be driven to zero from the ambient environment", () => {
    // Same rationale as the TTL clamp, which already had a floor: this is read from the
    // environment of whoever runs classify. "1" made every session older than a minute report
    // PROGRESS-STALLED and "1e-3" floored to 0 made every session report it unconditionally.
    // A small POSITIVE value is clamped up to the floor...
    expect(stallMinutes({ ICN_SESSION_STALL_MINUTES: "1" })).toBe(MIN_STALL_MINUTES);
    expect(stallMinutes({ ICN_SESSION_STALL_MINUTES: "1e-3" })).toBe(MIN_STALL_MINUTES);
    // ...while a value that is not a positive number at all falls back to the DEFAULT, which
    // is a different (and larger) safe answer. Both directions end up >= the floor, which is
    // the property that matters; conflating them would have hidden which branch ran.
    expect(stallMinutes({ ICN_SESSION_STALL_MINUTES: "0" })).toBe(DEFAULT_STALL_MINUTES);
    expect(stallMinutes({ ICN_SESSION_STALL_MINUTES: "-5" })).toBe(DEFAULT_STALL_MINUTES);
    expect(stallMinutes({ ICN_SESSION_STALL_MINUTES: "abc" })).toBe(DEFAULT_STALL_MINUTES);
    for (const v of ["1", "1e-3", "0", "-5", "abc", "240"]) {
      expect(stallMinutes({ ICN_SESSION_STALL_MINUTES: v })).toBeGreaterThanOrEqual(
        MIN_STALL_MINUTES
      );
    }
    // ...and a legitimate value is still honoured, or the floor would just be a constant.
    expect(stallMinutes({ ICN_SESSION_STALL_MINUTES: "240" })).toBe(240);
    expect(stallMinutes({})).toBe(DEFAULT_STALL_MINUTES);
  });
});
