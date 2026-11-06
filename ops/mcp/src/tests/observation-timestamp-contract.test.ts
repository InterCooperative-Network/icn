// The observation profile publishes ONE timestamp form to consumers:
//
//   TEXT 'YYYY-MM-DD HH:MM:SS', UTC, as written by SQLite datetime('now')
//   (observation-golden/reference/contract.json -> database.timestamps)
//
// Every production writer of a registry time column uses datetime('now') or a column default, so
// no real registry holds any other spelling. The producer must therefore refuse anything else
// instead of copying it through under a contract that says it cannot occur: a strict reader
// (Conductor's v2 reader) would otherwise be handed a value the published contract excludes.
//
// Synthetic data only. Nothing here opens the real shared registry.

import { existsSync, readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import Database from "better-sqlite3";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { exportRegistrySnapshot, verifySnapshotPair } from "../state/snapshot-export.js";
import { initDb } from "../state/db.js";
import { observationValueProblem } from "../state/snapshot-profile.js";
import { addSession, expectRefusal, makeRegistry, removeDir, tempDir } from "./fixtures/snapshot-helpers.js";

const HERE = dirname(fileURLToPath(import.meta.url));

const COLUMNS: ReadonlyArray<readonly [string, string]> = [
  ["schema_version", "applied_at"],
  ["sessions", "started_at"],
  ["sessions", "last_heartbeat"],
  ["sessions", "last_progress"],
  ["file_claims", "claimed_at"],
];

const ACCEPTED = [
  "2026-01-01 10:00:00",
  "2026-12-31 23:59:59",
  "2024-02-29 00:00:00", // leap day
  "2000-02-29 12:00:00", // divisible by 400
  "0001-01-01 00:00:00", // earliest year Python's datetime (a consumer) can represent
];

const REFUSED: ReadonlyArray<readonly [string, string]> = [
  // Not the published form: a different separator, a zone, or a fraction.
  ["ISO-8601 'T' separator", "2026-01-01T10:00:00"],
  ["ISO-8601 with Z", "2026-01-01T10:00:00Z"],
  ["space form with Z", "2026-01-01 10:00:00Z"],
  ["fractional seconds", "2026-01-01 10:00:00.123"],
  ["ISO fractional with Z", "2026-01-01T10:05:00.123Z"],
  ["numeric offset", "2026-01-01 10:00:00+00:00"],
  // Surrounding bytes.
  ["leading space", " 2026-01-01 10:00:00"],
  ["trailing space", "2026-01-01 10:00:00 "],
  ["trailing newline", "2026-01-01 10:00:00\n"],
  ["empty string", ""],
  // Digits that are not ASCII 0-9.
  ["fullwidth digits", "２０２６-01-01 10:00:00"],
  ["Arabic-Indic digits", "٢٠٢٦-01-01 10:00:00"],
  // Right shape, but not a real UTC instant: a consumer that parses it rejects it.
  ["month 13", "2026-13-01 10:00:00"],
  ["month 00", "2026-00-10 10:00:00"],
  ["day 00", "2026-01-00 10:00:00"],
  ["day 32", "2026-01-32 10:00:00"],
  ["Feb 30", "2026-02-30 10:00:00"],
  ["Feb 29 in a non-leap year", "2023-02-29 10:00:00"],
  ["Feb 29 in 1900 (century, not leap)", "1900-02-29 10:00:00"],
  ["April 31", "2026-04-31 10:00:00"],
  ["hour 24", "2026-01-01 24:00:00"],
  ["minute 60", "2026-01-01 10:60:00"],
  ["second 60", "2026-01-01 10:00:60"],
  ["year 0000", "0000-01-01 00:00:00"],
];

describe("observation timestamps: one published form, nothing else", () => {
  it("the published contract still states the form this rule enforces", () => {
    const contract = JSON.parse(
      readFileSync(join(HERE, "fixtures/observation-golden/reference/contract.json"), "utf8")
    ) as { database?: { timestamps?: string } };
    expect(contract.database?.timestamps).toBe("TEXT 'YYYY-MM-DD HH:MM:SS', UTC, as written by SQLite datetime('now')");
  });

  describe.each(COLUMNS)("%s.%s", (table, column) => {
    it.each(ACCEPTED)("accepts %s", (v) => {
      expect(observationValueProblem(table, column, v)).toBeNull();
    });

    it("accepts NULL (not recorded)", () => {
      expect(observationValueProblem(table, column, null)).toBeNull();
    });

    it.each(REFUSED)("refuses %s", (_label, v) => {
      const problem = observationValueProblem(table, column, v);
      expect(problem).not.toBeNull();
      if (v.trim() !== "") expect(problem).not.toContain(v.trim()); // the reason never echoes the value
    });

    it("refuses a non-string", () => {
      expect(observationValueProblem(table, column, 20260101100000)).not.toBeNull();
    });
  });
});

describe("observation export refuses a time column outside the published form", () => {
  let work: string;
  let out: string;
  beforeEach(() => {
    work = tempDir("icn-ts-src-");
    out = tempDir("icn-ts-out-");
  });
  afterEach(() => {
    removeDir(work);
    removeDir(out);
  });

  function source(sql?: string): string {
    const p = makeRegistry(work);
    const db = initDb(p);
    addSession(db, "sess-1", ["src/a.rs"]);
    db.close();
    if (sql !== undefined) {
      const w = new Database(p);
      w.exec(sql);
      w.close();
    }
    return p;
  }

  it("control: a registry holding only datetime('now') values exports and verifies", () => {
    const dest = join(out, "ok.db");
    exportRegistrySnapshot({ profile: "observation", source: source(), dest });
    expect(verifySnapshotPair(dest).ok).toBe(true);
  });

  it.each([
    ["sessions.last_heartbeat ISO", "UPDATE sessions SET last_heartbeat = '2026-01-01T10:05:00Z'"],
    ["sessions.last_progress fractional ISO", "UPDATE sessions SET last_progress = '2026-01-01T10:05:00.123Z'"],
    ["sessions.started_at impossible date", "UPDATE sessions SET started_at = '2026-02-30 10:00:00'"],
    ["file_claims.claimed_at ISO", "UPDATE file_claims SET claimed_at = '2026-01-01T10:05:00Z'"],
    ["schema_version.applied_at ISO", "UPDATE schema_version SET applied_at = '2026-01-01T10:05:00Z'"],
  ])("%s: refused as SNAPSHOT_VERIFY_FAILED with nothing published", (_label, sql) => {
    const dest = join(out, "o.db");
    expectRefusal("SNAPSHOT_VERIFY_FAILED", () =>
      exportRegistrySnapshot({ profile: "observation", source: source(sql), dest })
    );
    expect(existsSync(dest)).toBe(false);
    expect(existsSync(`${dest}.meta.json`)).toBe(false);
  });
});
