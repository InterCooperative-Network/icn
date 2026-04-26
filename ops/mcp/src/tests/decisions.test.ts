import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { initDb } from "../state/db.js";
import { parseAdrMetadata } from "../tools/decisions.js";
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

describe("parseAdrMetadata — classic markdown", () => {
  it("extracts title, status, date, tags from `**Field**:` style", () => {
    const content = `# ADR-0001: Orchestration Plane Architecture

**Date**: 2026-02-19
**Status**: superseded
**Superseded by**: ADR-0017
**Tags**: orchestration, mcp, multi-agent

## Context
…
`;
    const m = parseAdrMetadata(content);
    expect(m.title).toBe("Orchestration Plane Architecture");
    expect(m.status).toBe("superseded");
    expect(m.date).toBe("2026-02-19");
    expect(m.tags).toBe("orchestration, mcp, multi-agent");
    expect(m.superseded_by).toEqual(["ADR-0017"]);
  });

  it("treats `**Tags**: —` as empty tags", () => {
    const content = `# ADR-0099: Test

**Date**: 2026-04-26
**Status**: accepted
**Tags**: —
`;
    const m = parseAdrMetadata(content);
    expect(m.tags).toBe("");
    expect(m.status).toBe("accepted");
  });
});

describe("parseAdrMetadata — YAML frontmatter", () => {
  it("extracts metadata from a frontmatter block", () => {
    const content = `---
id: "0014"
title: "Constitutional Object Model"
status: "proposed"
date: "2026-04-19"
tags: ["governance", "architecture", "authority"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "partially implemented"
---

# ADR 0014: Constitutional Object Model

## Status
…
`;
    const m = parseAdrMetadata(content);
    expect(m.title).toBe("Constitutional Object Model");
    expect(m.status).toBe("proposed");
    expect(m.date).toBe("2026-04-19");
    expect(m.tags).toBe("governance, architecture, authority");
    expect(m.implementation_status).toBe("partially implemented");
    expect(m.supersedes).toEqual([]);
    expect(m.superseded_by).toEqual([]);
  });

  it("handles supersedes as a populated YAML inline list", () => {
    const content = `---
id: "0017"
title: "Monorepo Consolidation"
status: "accepted"
date: "2026-04-26"
supersedes: ["ADR-0001", "ADR-0010"]
amends: ["ADR-0002"]
---

# ADR 0017
`;
    const m = parseAdrMetadata(content);
    expect(m.supersedes).toEqual(["ADR-0001", "ADR-0010"]);
    expect(m.amends).toEqual(["ADR-0002"]);
  });

  it("handles supersedes as a YAML block-style list", () => {
    const content = `---
id: "0017"
title: "Test"
status: "accepted"
date: "2026-04-26"
supersedes:
  - "ADR-0001"
  - "ADR-0010"
---

# ADR
`;
    const m = parseAdrMetadata(content);
    expect(m.supersedes).toEqual(["ADR-0001", "ADR-0010"]);
  });
});

describe("parseAdrMetadata — bullet metadata", () => {
  it("extracts status and date from `- Status:` / `- Date:` lines", () => {
    const content = `# ADR-0010: App topology and canonical app roots

- Status: Superseded
- Superseded by: ADR-0017
- Date: 2026-02-09
- Decision drivers:
  - Reduce contributor confusion

## Context
…
`;
    const m = parseAdrMetadata(content);
    expect(m.title).toBe("App topology and canonical app roots");
    expect(m.status).toBe("superseded");
    expect(m.date).toBe("2026-02-09");
    expect(m.superseded_by).toEqual(["ADR-0017"]);
  });
});

describe("parseAdrMetadata — fallbacks", () => {
  it("returns empty metadata for an empty body", () => {
    const m = parseAdrMetadata("");
    expect(m.title).toBe("");
    expect(m.status).toBeNull();
    expect(m.date).toBeNull();
    expect(m.tags).toBe("");
    expect(m.supersedes).toEqual([]);
    expect(m.superseded_by).toEqual([]);
    expect(m.amends).toEqual([]);
    expect(m.implementation_status).toBeNull();
  });

  it("derives title from `# ADR-NNNN: …` heading when frontmatter omits it", () => {
    const content = `---
id: "0099"
date: "2026-04-26"
---

# ADR-0099: Heading-Derived Title
`;
    const m = parseAdrMetadata(content);
    expect(m.title).toBe("Heading-Derived Title");
  });
});
