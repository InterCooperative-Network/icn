import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { initDb } from "../state/db.js";
import type Database from "better-sqlite3";

let db: Database.Database;

beforeEach(() => {
  db = initDb(":memory:");
});

afterEach(() => {
  db.close();
});

describe("decision_index", () => {
  it("inserts and searches decisions by title", () => {
    db.prepare(
      "INSERT INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
    ).run(
      "0001",
      "Orchestration Plane Architecture",
      "orchestration,mcp",
      "state/decisions/0001-test.md",
      "2026-02-19"
    );

    const results = db
      .prepare("SELECT * FROM decision_index WHERE title LIKE ?")
      .all("%Orchestration%") as Array<{ id: string; title: string }>;

    expect(results).toHaveLength(1);
    expect(results[0].id).toBe("0001");
  });

  it("INSERT OR REPLACE re-points stale file_path on re-sync", () => {
    // Simulate a pre-migration row pointing at the retired location.
    db.prepare(
      "INSERT INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
    ).run(
      "0001",
      "Orchestration Plane Architecture",
      "orchestration,mcp",
      "ops/state/decisions/0001-orchestration-plane-architecture.md", // stale
      "2026-02-19"
    );

    // syncDecisionIndex now uses INSERT OR REPLACE; this is the same
    // statement and the same input shape, with the canonical path.
    db.prepare(
      "INSERT OR REPLACE INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
    ).run(
      "0001",
      "Orchestration Plane Architecture",
      "orchestration,mcp",
      "docs/adr/ADR-0001-orchestration-plane-architecture.md", // canonical
      "2026-02-19"
    );

    const row = db
      .prepare("SELECT file_path FROM decision_index WHERE id = ?")
      .get("0001") as { file_path: string };
    expect(row.file_path).toBe(
      "docs/adr/ADR-0001-orchestration-plane-architecture.md"
    );
  });

  it("tag search returns only matching decisions", () => {
    db.prepare(
      "INSERT INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
    ).run(
      "0001",
      "ADR One",
      "networking,kernel",
      "state/decisions/0001.md",
      "2026-02-19"
    );
    db.prepare(
      "INSERT INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
    ).run(
      "0002",
      "ADR Two",
      "deployment",
      "state/decisions/0002.md",
      "2026-02-19"
    );

    const kernelResults = db
      .prepare("SELECT * FROM decision_index WHERE tags LIKE ?")
      .all("%kernel%") as Array<{ id: string }>;

    expect(kernelResults).toHaveLength(1);
    expect(kernelResults[0].id).toBe("0001");
  });
});
