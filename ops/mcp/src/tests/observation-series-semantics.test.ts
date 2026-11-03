// ICN lifecycle semantics that the continuity fixture relies on, pinned against ICN's REAL runtime
// and REAL claim_files handler. Deliberately independent of the fixture generator: if the code
// these describe ever changes, these fail on their own, and the documentation that cites them
// (observation-series/HANDOFF.md) is known to be stale.

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { describe, expect, it } from "vitest";
import { initDb } from "../state/db.js";
import * as rt from "../runtime/session-runtime.js";
import { registerSessionTools } from "../tools/sessions.js";
import { mirrorClaim } from "./fixtures/observation-series/claim-mirror.js";

describe("what ICN's real runtime does NOT allow (so the fixture does not fake it)", () => {
  function fresh() {
    const db = initDb(":memory:");
    const lane = (name: string, gen: string | null) => ({ repo_id: "/s/r.git", repo_name: "r", worktree_id: `/s/r.git/worktrees/${name}`, worktree_path: `/s/w/${name}`, worktree_name: name, worktree_generation: gen });
    return { db, lane };
  }
  const conv = (n: number) => `5d1c2a4e-0000-4000-8000-${String(n).padStart(12, "0")}`;

  it("re-registering a known conversation with another provider, host or parent leaves the row unchanged", () => {
    const { db, lane } = fresh();
    const first = rt.registerSession(db, { repo: "r", identity: lane("x", "g1"), provider: "claude", host: "h1", provider_session_id: conv(1), parent_session_id: null });
    const again = rt.registerSession(db, { repo: "r", identity: lane("x", "g1"), provider: "OTHER", host: "h2", parent_session_id: "p", provider_session_id: conv(1) });
    expect(again).toMatchObject({ session_id: first.session_id, created: false, deduplicated: true });
    expect(db.prepare("SELECT provider, host, parent_session_id, started_at FROM sessions").all()).toHaveLength(1);
    expect(db.prepare("SELECT provider, host, parent_session_id FROM sessions").get()).toEqual({ provider: "claude", host: "h1", parent_session_id: null });
    db.close();
  });

  it("a DIFFERENT conversation id never mutates an existing row's conversation: it creates a new activation", () => {
    const { db, lane } = fresh();
    const a = rt.registerSession(db, { repo: "r", identity: lane("x", "g1"), provider_session_id: conv(1) });
    const b = rt.registerSession(db, { repo: "r", identity: lane("x", "g1"), provider_session_id: conv(2) });
    expect(b.created).toBe(true);
    expect(db.prepare("SELECT provider_session_id FROM sessions WHERE id = ?").get(a.session_id)).toEqual({ provider_session_id: conv(1) });
    db.close();
  });

  it("a session registered WITHOUT a conversation id never dedupes, so its lane never changes", () => {
    const { db, lane } = fresh();
    const a = rt.registerSession(db, { repo: "r", identity: lane("x", "g1") });
    const b = rt.registerSession(db, { repo: "r", identity: lane("y", "g9") });
    expect(a.session_id).not.toBe(b.session_id);
    expect(db.prepare("SELECT worktree_id FROM sessions WHERE id = ?").get(a.session_id)).toEqual({ worktree_id: "/s/r.git/worktrees/x" });
    db.close();
  });

  it("generation is overwritten with NULL when the new token cannot be minted (NULL = unknown, not 'same')", () => {
    const { db, lane } = fresh();
    const a = rt.registerSession(db, { repo: "r", identity: lane("x", "g1"), provider_session_id: conv(1) });
    rt.registerSession(db, { repo: "r", identity: lane("x", null), provider_session_id: conv(1) });
    expect(db.prepare("SELECT worktree_generation FROM sessions WHERE id = ?").get(a.session_id)).toEqual({ worktree_generation: null });
    db.close();
  });

  it("a resume never rewrites started_at or progress_count", () => {
    const { db, lane } = fresh();
    const a = rt.registerSession(db, { repo: "r", identity: lane("x", "g1"), provider_session_id: conv(1) });
    rt.recordProgress(db, a.session_id, { kind: "file_edit" });
    db.prepare("UPDATE sessions SET started_at = '2026-01-01 00:00:00'").run();
    rt.registerSession(db, { repo: "r", identity: lane("y", "g2"), provider_session_id: conv(1) });
    expect(db.prepare("SELECT started_at, progress_count FROM sessions").get()).toEqual({ started_at: "2026-01-01 00:00:00", progress_count: 1 });
    db.close();
  });

  it("release DELETES the row and its claims; a released id cannot be heartbeated, progressed or resurrected; the conversation returns as a NEW id", () => {
    const { db, lane } = fresh();
    const a = rt.registerSession(db, { repo: "r", identity: lane("x", "g1"), provider_session_id: conv(1) });
    db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES ('f.rs', ?)").run(a.session_id);
    const r = rt.releaseSession(db, a.session_id, { reason: "t" });
    expect(r).toMatchObject({ released: true, dropped: { file_claims: 1 } });
    expect(db.prepare("SELECT COUNT(*) AS n FROM sessions").get()).toEqual({ n: 0 });
    expect(db.prepare("SELECT COUNT(*) AS n FROM file_claims").get()).toEqual({ n: 0 });
    expect(rt.recordHeartbeat(db, a.session_id)).toBe(false);
    expect(rt.recordProgress(db, a.session_id, { kind: "file_edit" })).toBe(false);
    const again = rt.registerSession(db, { repo: "r", identity: lane("x", "g1"), provider_session_id: conv(1) });
    expect(again.created).toBe(true);
    expect(again.session_id).not.toBe(a.session_id);
    db.close();
  });

  it("there is no lifecycle-state column and no registry-instance identifier in the schema", () => {
    const { db } = fresh();
    const cols = (db.prepare("PRAGMA table_info(sessions)").all() as Array<{ name: string }>).map((c) => c.name);
    expect(cols.filter((c) => /state|status|ended|released|retired/i.test(c))).toEqual([]);
    const tables = (db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all() as Array<{ name: string }>).map((t) => t.name);
    expect(tables.filter((t) => /registry|instance|identity/i.test(t))).toEqual([]);
    expect(db.pragma("user_version", { simple: true })).toBe(0);
    expect(db.pragma("application_id", { simple: true })).toBe(0);
    db.close();
  });
});

// ───────────────────────────────────────────────────────────────────────────────
describe("the generator's claim mirror is pinned to the REAL claim_files handler", () => {
  async function realClaims(setup: (db: ReturnType<typeof initDb>) => string[]) {
    const db = initDb(":memory:");
    const server = new McpServer({ name: "icn-ops-test", version: "0.0.0" });
    registerSessionTools(server, db);
    const [ct, st] = InMemoryTransport.createLinkedPair();
    const client = new Client({ name: "test", version: "0.0.0" });
    await Promise.all([server.connect(st), client.connect(ct)]);
    const sids = setup(db);
    return { db, client, sids };
  }
  const call = async (client: Client, session_id: string, files: string[]) => {
    const res = (await client.callTool({ name: "claim_files", arguments: { session_id, files } })) as { content: Array<{ text: string }> };
    return JSON.parse(res.content[0]!.text) as { claimed: string[]; conflicts: string[] };
  };
  const mk = (db: ReturnType<typeof initDb>, ageMin: number) => {
    const holder = rt.registerSession(db, { repo: "r" }).session_id;
    const other = rt.registerSession(db, { repo: "r" }).session_id;
    db.prepare("UPDATE sessions SET last_heartbeat = datetime('now', ?) WHERE id = ?").run(`-${ageMin} minutes`, holder);
    db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES ('f.rs', ?)").run(holder);
    return [holder, other];
  };

  it.each([
    ["a live holder (5 min) refuses the newcomer", 5, { claimed: [], conflicts: ["f.rs"] }],
    ["a stale holder (45 min) does not: both rows then coexist", 45, { claimed: ["f.rs"], conflicts: [] }],
  ])("%s", async (_n, age, expected) => {
    const real = await realClaims((db) => mk(db, age as number));
    const mirrorDb = initDb(":memory:");
    const [mh, mo] = mk(mirrorDb, age as number);
    const r = await call(real.client, real.sids[1]!, ["f.rs"]);
    const m = mirrorClaim(mirrorDb, mo!, ["f.rs"], null, rt.ttlMinutes({} as NodeJS.ProcessEnv));
    expect({ claimed: r.claimed, conflicts: r.conflicts }).toEqual(expected);
    expect({ claimed: m.claimed, conflicts: m.conflicts }).toEqual(expected);
    const holders = (d: ReturnType<typeof initDb>) => (d.prepare("SELECT COUNT(*) AS n FROM file_claims WHERE file_path = 'f.rs'").get() as { n: number }).n;
    expect(holders(mirrorDb)).toBe(holders(real.db));
    expect(mh).toBeTruthy();
    real.db.close();
    mirrorDb.close();
  });

  it("re-claiming a path you already hold REPLACES the row and resets claimed_at - in the real handler and the mirror alike", async () => {
    const real = await realClaims((db) => {
      const s = rt.registerSession(db, { repo: "r" }).session_id;
      db.prepare("INSERT INTO file_claims (file_path, session_id, claimed_at) VALUES ('f.rs', ?, '2000-01-01 00:00:00')").run(s);
      return [s];
    });
    const r = await call(real.client, real.sids[0]!, ["f.rs"]);
    expect(r.claimed).toEqual(["f.rs"]);
    const after = real.db.prepare("SELECT claimed_at FROM file_claims").get() as { claimed_at: string };
    expect(after.claimed_at > "2000-01-01 00:00:00").toBe(true);

    const mdb = initDb(":memory:");
    const s = rt.registerSession(mdb, { repo: "r" }).session_id;
    mdb.prepare("INSERT INTO file_claims (file_path, session_id, claimed_at) VALUES ('f.rs', ?, '2000-01-01 00:00:00')").run(s);
    mirrorClaim(mdb, s, ["f.rs"], null, 30);
    expect((mdb.prepare("SELECT claimed_at FROM file_claims").get() as { claimed_at: string }).claimed_at > "2000-01-01 00:00:00").toBe(true);
    real.db.close();
    mdb.close();
  });
});
