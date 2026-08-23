// Upgrade-path tests against ON-DISK databases built to the shapes that actually shipped.
//
// A fresh :memory: database proves the migration ladder runs top-to-bottom on an empty file.
// It proves nothing about UPGRADING a database that already carries rows and a version stamp,
// which is the only case that exists in production. These tests build the real prior shapes on
// disk, run the current migration over them, and check both schema and data.

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import Database from "better-sqlite3";
import { mkdtempSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { initDb } from "../state/db.js";
import { registerSession } from "../runtime/session-runtime.js";

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "icn-schema-"));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

function columns(db: Database.Database, table: string): string[] {
  return (db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map(
    (c) => c.name
  );
}

/**
 * The schema exactly as it existed at v2 — the shape observed on icn-dev's only persistent ops
 * database (worktrees/icn/mcp-host/ops/mcp/data/icn-ops.db reported schema_version {1,2} and
 * these six session columns). Reproduced literally so this test breaks if the real prior shape
 * is ever misremembered.
 */
function buildShippedV2(path: string): void {
  const db = new Database(path);
  db.pragma("foreign_keys = ON");
  db.exec(`
    CREATE TABLE sessions (
      id TEXT PRIMARY KEY,
      repo TEXT NOT NULL,
      worktree TEXT,
      task_description TEXT,
      started_at TEXT DEFAULT (datetime('now')),
      last_heartbeat TEXT DEFAULT (datetime('now'))
    );
    CREATE TABLE file_claims (
      file_path TEXT NOT NULL,
      session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
      claimed_at TEXT DEFAULT (datetime('now')),
      PRIMARY KEY (file_path, session_id)
    );
    CREATE TABLE health_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL, polled_at TEXT);
    CREATE TABLE decision_index (id TEXT PRIMARY KEY, title TEXT NOT NULL, tags TEXT, file_path TEXT NOT NULL, created_at TEXT);
    CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TEXT DEFAULT (datetime('now')));
    CREATE TABLE events (id INTEGER PRIMARY KEY, scope TEXT NOT NULL, type TEXT NOT NULL, payload TEXT NOT NULL DEFAULT '{}', created_at INTEGER NOT NULL);
    CREATE TABLE mailbox (id INTEGER PRIMARY KEY, to_session TEXT NOT NULL, from_session TEXT, kind TEXT NOT NULL DEFAULT 'text', payload TEXT NOT NULL DEFAULT '{}', created_at INTEGER NOT NULL, read_at INTEGER);
    CREATE TABLE watchers_process (id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, pid INTEGER NOT NULL, label TEXT NOT NULL, created_at INTEGER NOT NULL, completed_at INTEGER, exit_code INTEGER, status TEXT NOT NULL DEFAULT 'running');
    INSERT INTO schema_version (version) VALUES (1), (2);
  `);
  // Pre-existing production rows, written the pre-v3 way.
  db.prepare("INSERT INTO sessions (id, repo, worktree, task_description) VALUES (?,?,?,?)")
    .run("legacy-session-1", "icn", "task-2640-respelling-replay", "pre-integration lane");
  db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES (?,?)")
    .run("icn/crates/x/src/lib.rs", "legacy-session-1");
  db.close();
}

describe("upgrade from the shipped v2 shape", () => {
  it("adds v3 columns to a populated on-disk database without losing rows", () => {
    const path = join(dir, "ops.db");
    buildShippedV2(path);

    const db = initDb(path); // runs the real migration ladder over the real prior shape

    const versions = (
      db.prepare("SELECT version FROM schema_version ORDER BY version").all() as Array<{
        version: number;
      }>
    ).map((r) => r.version);
    expect(versions).toEqual([1, 2, 3, 4]);

    const cols = columns(db, "sessions");
    for (const c of [
      "repo_id", "worktree_id", "worktree_path", "worktree_name", "provider_session_id",
      "branch_at_registration", "head_at_registration", "transcript_path",
      "progress_count", "last_progress",
    ]) {
      expect(cols).toContain(c);
    }
    expect(cols).not.toContain("state"); // the discarded experimental column

    // v4 lane-scopes supervisions so they cannot follow a session out of the worktree.
    const wcols = columns(db, "watchers_process");
    expect(wcols).toContain("worktree_id");

    // Pre-existing data survives, with safe defaults for the new columns.
    const row = db.prepare("SELECT * FROM sessions WHERE id = 'legacy-session-1'").get() as
      Record<string, unknown>;
    expect(row["worktree"]).toBe("task-2640-respelling-replay");
    expect(row["progress_count"]).toBe(0);
    expect(row["worktree_id"]).toBeNull(); // legacy rows have no canonical identity...
    db.close();
  });

  it("leaves legacy rows unregistered-but-protected rather than inventing an identity", () => {
    // A pre-integration row has no worktree_id, so it cannot be joined to a lane. It must not
    // be silently adopted into one — see the fail-safe classification rules.
    const path = join(dir, "ops.db");
    buildShippedV2(path);
    const db = initDb(path);
    const found = db
      .prepare("SELECT COUNT(*) c FROM sessions WHERE worktree_id = ?")
      .get("/repos/icn.git/worktrees/task-2640-respelling-replay") as { c: number };
    expect(found.c).toBe(0);
    db.close();
  });

  it("is idempotent: re-running the migration over an upgraded file changes nothing", () => {
    const path = join(dir, "ops.db");
    buildShippedV2(path);
    initDb(path).close();
    const before = (() => {
      const d = new Database(path, { readonly: true });
      const c = columns(d, "sessions");
      d.close();
      return c;
    })();

    const db = initDb(path); // second open, same file
    expect(columns(db, "sessions")).toEqual(before);
    expect(
      db.prepare("SELECT COUNT(*) c FROM schema_version WHERE version = 3").get()
    ).toEqual({ c: 1 });
    expect(
      db.prepare("SELECT COUNT(*) c FROM schema_version WHERE version = 4").get()
    ).toEqual({ c: 1 });
    db.close();
  });

  it("an upgraded database accepts new-style registration immediately", () => {
    const path = join(dir, "ops.db");
    buildShippedV2(path);
    const db = initDb(path);
    const r = registerSession(db, {
      repo: "icn",
      identity: {
        repo_id: "/repos/icn.git", repo_name: "icn",
        worktree_id: "/repos/icn.git/worktrees/wt", worktree_path: "/wt", worktree_name: "wt",
      },
      provider_session_id: "conv-after-upgrade",
    });
    expect(r.created).toBe(true);
    // Legacy row and new row coexist.
    expect(db.prepare("SELECT COUNT(*) c FROM sessions").get()).toEqual({ c: 2 });
    db.close();
  });
});

describe("migration-number discipline", () => {
  it("a database already stamped v3 is SKIPPED, which is why v3 must never be edited again", () => {
    // This pins the rule rather than a behaviour we want. The migration ladder gates on the
    // version stamp, so editing a stamped migration silently leaves that database on the old
    // shape. Evidence recorded when v3 was finalised: icn-dev's only persistent ops database
    // was at schema_version {1,2} and had never run any v3, so editing it in place was safe.
    // If that is ever untrue again, the fix is a NEW numbered migration, not another edit.
    const path = join(dir, "stamped.db");
    buildShippedV2(path);
    const pre = new Database(path);
    pre.exec("ALTER TABLE sessions ADD COLUMN state TEXT NOT NULL DEFAULT 'active'");
    pre.prepare("INSERT INTO schema_version (version) VALUES (3)").run();
    pre.close();

    const db = initDb(path);
    const cols = columns(db, "sessions");
    // The stamp wins: the experimental column is still there, the new ones never arrived.
    expect(cols).toContain("state");
    expect(cols).not.toContain("worktree_id");
    // ...and this is exactly why the lane-scoping change was shipped as v4 rather than as an
    // edit to v3: a v3-stamped database already existed on the development VM.
    expect(
      db.prepare("SELECT COUNT(*) c FROM schema_version WHERE version = 4").get()
    ).toEqual({ c: 1 });
    expect(columns(db, "watchers_process")).toContain("worktree_id");
    db.close();
  });
});
