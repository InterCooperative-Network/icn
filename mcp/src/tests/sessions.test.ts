import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { initDb } from "../state/db.js";
import type Database from "better-sqlite3";
import { randomUUID } from "crypto";

let db: Database.Database;

beforeEach(() => {
  db = initDb(":memory:");
});

afterEach(() => {
  db.close();
});

describe("sessions", () => {
  it("inserts a session and retrieves it", () => {
    const id = randomUUID();
    db.prepare(
      "INSERT INTO sessions (id, repo, worktree, task_description) VALUES (?, ?, ?, ?)"
    ).run(id, "icn", null, "testing");

    const row = db
      .prepare("SELECT * FROM sessions WHERE id = ?")
      .get(id) as { id: string; repo: string };
    expect(row.id).toBe(id);
    expect(row.repo).toBe("icn");
  });

  it("deletes session and cascades file_claims", () => {
    const id = randomUUID();
    db.prepare("INSERT INTO sessions (id, repo) VALUES (?, ?)").run(id, "icn");
    db.prepare(
      "INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)"
    ).run("crates/foo/src/lib.rs", id);

    db.prepare("DELETE FROM sessions WHERE id = ?").run(id);

    const claims = db
      .prepare("SELECT * FROM file_claims WHERE session_id = ?")
      .all(id);
    expect(claims).toHaveLength(0);
  });

  it("advisory file claim detects conflict from another session", () => {
    const id1 = randomUUID();
    const id2 = randomUUID();
    db.prepare("INSERT INTO sessions (id, repo) VALUES (?, ?)").run(id1, "icn");
    db.prepare("INSERT INTO sessions (id, repo) VALUES (?, ?)").run(id2, "icn");

    db.prepare(
      "INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)"
    ).run("crates/foo/src/lib.rs", id1);

    const existing = db
      .prepare(
        `SELECT session_id FROM file_claims
         WHERE file_path = ? AND session_id != ?`
      )
      .get("crates/foo/src/lib.rs", id2);

    expect(existing).toBeTruthy();
  });
});
