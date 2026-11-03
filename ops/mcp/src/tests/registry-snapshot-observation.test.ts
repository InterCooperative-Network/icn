// Observation profile — the least-privilege export for an EXTERNAL consumer.
//
// The properties that matter here are negative ones ("this never appears"), so they are checked
// against the raw bytes of the published file, not by querying it: a query only sees live rows,
// while a byte scan also sees free pages, dropped tables and anything a redaction pass forgot.
// Every such scan is paired with the same scan on the FULL profile, which must find the
// canaries — otherwise "found nothing" could just mean the scanner or fixture is broken.

import { spawn, type ChildProcess } from "child_process";
import { createRequire } from "module";
import { closeSync, copyFileSync, openSync, readFileSync, readdirSync, statSync, unlinkSync, writeFileSync, writeSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import Database from "better-sqlite3";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { initDb } from "../state/db.js";
import { OBSERVATION_POLICY } from "../state/snapshot-profile.js";
import { exportRegistrySnapshot, verifySnapshotPair, type SnapshotOptions } from "../state/snapshot-export.js";
import {
  addSession,
  CANARY,
  canariesInFile,
  expectRefusal,
  makeRegistry,
  makeSensitiveRegistry,
  removeDir,
  sha256Hex,
  tempDir,
} from "./fixtures/snapshot-helpers.js";

let dir: string;
beforeEach(() => {
  dir = tempDir("icn-snapshot-obs-");
});
afterEach(() => removeDir(dir));

const observe = (o: Omit<SnapshotOptions, "profile">) => exportRegistrySnapshot({ profile: "observation", ...o });
const full = (o: Omit<SnapshotOptions, "profile">) => exportRegistrySnapshot({ profile: "full", ...o });
const ALL_CANARIES = Object.values(CANARY);

function rows(path: string, sql: string): Array<Record<string, unknown>> {
  const db = new Database(path, { readonly: true });
  try {
    return db.prepare(sql).all() as Array<Record<string, unknown>>;
  } finally {
    db.close();
  }
}

describe("the disclosure policy is exhaustive over the production schema", () => {
  // If a future migration adds a column or table, these fail until someone CLASSIFIES it. That
  // is the point of an allowlist: the default for anything unclassified is "not exported".
  it("classifies every column of sessions exactly once", () => {
    const db = initDb(join(dir, "fresh.db"));
    const actual = (db.prepare("PRAGMA table_info(sessions)").all() as Array<{ name: string }>).map((c) => c.name);
    db.close();
    const p = OBSERVATION_POLICY.sessions;
    const classified = [...p.verbatim, ...p.digested, ...p.withheld];
    expect([...classified].sort()).toEqual([...actual].sort());
    expect(new Set(classified).size).toBe(classified.length);
  });

  it("classifies every table: exported or withheld, nothing unaccounted for", () => {
    const db = initDb(join(dir, "fresh.db"));
    const actual = (
      db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").all() as Array<{
        name: string;
      }>
    ).map((t) => t.name);
    db.close();
    const classified = [...OBSERVATION_POLICY.tables, ...OBSERVATION_POLICY.withheldTables];
    expect([...classified].sort()).toEqual([...actual].sort());
  });

  it("classifies every column of file_claims and schema_version", () => {
    const db = initDb(join(dir, "fresh.db"));
    const cols = (t: string) =>
      (db.prepare(`PRAGMA table_info(${t})`).all() as Array<{ name: string }>).map((c) => c.name).sort();
    expect(cols("file_claims")).toEqual([...OBSERVATION_POLICY.file_claims.columns].sort());
    expect(cols("schema_version")).toEqual([...OBSERVATION_POLICY.schema_version.columns].sort());
    db.close();
  });
});

describe("nothing sensitive reaches the observation file", () => {
  it("raw-byte scan: no canary survives, including in free pages and the sidecar", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "obs.db");
    observe({ source: src, dest });

    expect(canariesInFile(dest, ALL_CANARIES)).toEqual([]);
    expect(canariesInFile(`${dest}.meta.json`, ALL_CANARIES)).toEqual([]);
  });

  it("is not vacuous: the FULL profile of the same source does contain them", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "full.db");
    full({ source: src, dest });
    const found = canariesInFile(dest, ALL_CANARIES);
    // Every canary class is detectable by this scanner.
    for (const c of [CANARY.task, CANARY.transcript, CANARY.mailbox, CANARY.eventPayload, CANARY.health, CANARY.futureColumn]) {
      expect(found).toContain(c);
    }
  });

  it("exports exactly the three observation tables and no other schema object", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "obs.db");
    observe({ source: src, dest });
    const objs = rows(dest, "SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'");
    const tables = objs.filter((o) => o["type"] === "table").map((o) => o["name"]);
    expect(tables.sort()).toEqual(["file_claims", "schema_version", "sessions"]);
    expect(objs.filter((o) => o["type"] !== "table" && o["type"] !== "index")).toEqual([]);
    // An ABSENT table is the honest encoding of "not exported"; an empty one would read as
    // "looked, found none".
    expect(rows(dest, "SELECT name FROM sqlite_master WHERE name IN ('mailbox','events','watchers_process')")).toEqual([]);
  });

  it("an unknown future column is withheld by default", () => {
    const src = makeSensitiveRegistry(dir); // has sessions.secret_note
    const dest = join(dir, "obs.db");
    observe({ source: src, dest });
    const cols = (rows(dest, "PRAGMA table_info(sessions)") as Array<{ name: string }>).map((c) => c.name);
    expect(cols).not.toContain("secret_note");
  });
});

describe("what the observation file does carry", () => {
  it("keeps identity, lifecycle and liveness columns verbatim, row for row", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "obs.db");
    observe({ source: src, dest });

    const keep = OBSERVATION_POLICY.sessions.verbatim.join(", ");
    expect(rows(dest, `SELECT ${keep} FROM sessions ORDER BY id`)).toEqual(
      rows(src, `SELECT ${keep} FROM sessions ORDER BY id`)
    );
    expect(rows(dest, "SELECT version, applied_at FROM schema_version ORDER BY version")).toEqual(
      rows(src, "SELECT version, applied_at FROM schema_version ORDER BY version")
    );
  });

  it("replaces path-valued identity with a stable digest, and says so", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "obs.db");
    const { meta } = observe({ source: src, dest });
    const [r] = rows(dest, "SELECT repo_id, worktree_id FROM sessions WHERE id = 'sess-1'");
    expect(r!["repo_id"]).toBe(`sha256:${sha256Hex(CANARY.repoId)}`);
    expect(r!["worktree_id"]).toBe(`sha256:${sha256Hex(CANARY.worktreeId)}`);
    expect(meta.profile).toBe("observation");
    if (meta.profile === "observation") {
      expect(meta.disclosure.sessions.digested_columns).toEqual(["repo_id", "worktree_id"]);
      expect(meta.disclosure.sessions.digest_transform).toMatch(/sha256/);
    }
  });

  it("keeps NULL as NULL when digesting (no value is invented)", () => {
    const src = makeRegistry(dir);
    const w = initDb(src);
    addSession(w, "bare"); // repo_id / worktree_id are NULL
    w.close();
    const dest = join(dir, "obs.db");
    observe({ source: src, dest });
    expect(rows(dest, "SELECT repo_id, worktree_id FROM sessions")).toEqual([{ repo_id: null, worktree_id: null }]);
  });

  it("withheld columns are NULL in every row", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "obs.db");
    observe({ source: src, dest });
    for (const col of OBSERVATION_POLICY.sessions.withheld) {
      expect(rows(dest, `SELECT COUNT(*) AS c FROM sessions WHERE ${col} IS NOT NULL`)).toEqual([{ c: 0 }]);
    }
  });
});

describe("file claims: occupancy survives, unsafe paths do not", () => {
  it("keeps safe repo-relative paths, tokenises the rest, and preserves row count and conflicts", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "obs.db");
    const { meta } = observe({ source: src, dest });
    const claims = rows(dest, "SELECT file_path, session_id FROM file_claims ORDER BY session_id, file_path");

    expect(claims).toHaveLength(7); // same as the source: dropping a row would fake "no claim"
    const paths = claims.map((c) => String(c["file_path"]));
    expect(paths).toContain("icn/crates/icn-core/src/lib.rs");
    expect(paths).toContain("docs/architecture/AGENT_RUNTIME.md");
    for (const p of paths) expect(p.startsWith("/") || p.startsWith("~") || p.includes("..")).toBe(false);

    const tokens = claims.filter((c) => String(c["file_path"]).startsWith("withheld:"));
    expect(tokens).toHaveLength(4);
    // The same unsafe path claimed by two sessions yields the SAME token, so a conflict is
    // still visible without the path being disclosed.
    const abs = tokens.filter((t) => t["session_id"] === "sess-1" || t["session_id"] === "sess-2").map((t) => t["file_path"]);
    expect(new Set(abs).size).toBe(3); // abs(shared), tilde, dotdot
    expect(meta.profile === "observation" && meta.disclosure.file_claims.rows_with_withheld_path).toBe(4);
  });

  it("uses a fresh salt per export: tokens are not comparable across snapshots", () => {
    const src = makeSensitiveRegistry(dir);
    observe({ source: src, dest: join(dir, "a.db") });
    observe({ source: src, dest: join(dir, "b.db") });
    const tok = (f: string) =>
      rows(join(dir, f), "SELECT file_path FROM file_claims WHERE file_path LIKE 'withheld:%' ORDER BY file_path").map(
        (r) => r["file_path"]
      );
    expect(tok("a.db")).not.toEqual(tok("b.db"));
  });
});

describe("path spellings that only LOOK relative are tokenised (positive allowlist)", () => {
  it.each([
    ["a URI scheme", "file:///etc/shadow"],
    ["a drive-letter prefix", "C:/Users/someone/key"],
    ["a colon in a segment", "a:b/c"],
    ["a backslash path", "dir\\file"],
    ["home-relative", "~/x"],
    ["a mid-path parent escape", "ok/../../etc/passwd"],
    ["a leading slash", "/etc/passwd"],
    ["a double slash", "a//b"],
    ["a trailing slash", "dir/"],
    ["an embedded newline", "a\nb"],
    ["a space (conservative: tokenised, not dropped)", "docs/my file.md"],
    ["an over-long path", `${"a/".repeat(300)}z`],
  ])("%s", (_n, claimed) => {
    const src = makeRegistry(dir);
    const w = initDb(src);
    addSession(w, "s1", [claimed]);
    w.close();
    const dest = join(dir, "o.db");
    observe({ source: src, dest });
    const [r] = rows(dest, "SELECT file_path FROM file_claims");
    expect(String(r!["file_path"])).toMatch(/^withheld:[0-9a-f]{16}$/);
    expect(canariesInFile(dest, [claimed])).toEqual([]);
  });

  it.each(["icn/crates/icn-core/src/lib.rs", "docs/a-b_c.d/e+f=g,h@i.md", "./local/file.ts", "Cargo.lock"])(
    "an ordinary repo-relative path is still shown: %s",
    (ok) => {
      const src = makeRegistry(dir);
      const w = initDb(src);
      addSession(w, "s1", [ok]);
      w.close();
      const dest = join(dir, "o.db");
      observe({ source: src, dest });
      expect(rows(dest, "SELECT file_path FROM file_claims")).toEqual([{ file_path: ok }]);
    }
  );

  it.each([
    ["a slash anywhere in host", "UPDATE sessions SET host = 'box/../../etc'"],
    ["a URI in repo", "UPDATE sessions SET repo = 'https://example.invalid/x'"],
    ["a backslash in worktree_name", "UPDATE sessions SET worktree_name = 'a\\b'"],
    ["an embedded path in provider", "UPDATE sessions SET provider = 'claude /home/x'"],
  ])("verbatim columns reject %s", (_n, sql) => {
    const src = makeSensitiveRegistry(dir);
    const w = new Database(src);
    w.exec(sql);
    w.close();
    expectRefusal("SNAPSHOT_VERIFY_FAILED", () => observe({ source: src, dest: join(dir, "o.db") }));
  });

  it("accepts the identifier shapes the registry really produces, including non-ASCII lane names", () => {
    const src = makeSensitiveRegistry(dir);
    const w = new Database(src);
    w.exec("UPDATE sessions SET worktree_name = 'espace-de-travail-été'");
    w.close();
    observe({ source: src, dest: join(dir, "o.db") });
    expect(rows(join(dir, "o.db"), "SELECT DISTINCT worktree_name FROM sessions")).toEqual([
      { worktree_name: "espace-de-travail-été" },
    ]);
  });
});

describe("the observation sidecar", () => {
  it("does not disclose the source path or home directory", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "obs.db");
    observe({ source: src, dest });
    const text = readFileSync(`${dest}.meta.json`, "utf-8");
    expect(text).not.toContain(dir);
    expect(text).not.toContain(homedir());
    expect(text).not.toContain("path_given");
    expect(text).not.toContain("realpath");
  });

  it("carries no source identity at all — not a path, not a hash of one — plus the null/absence rules a reader needs", () => {
    const a = makeSensitiveRegistry(dir, "a.db");
    const m1 = observe({ source: a, dest: join(dir, "1.db") }).meta;
    if (m1.profile !== "observation") throw new Error("profile");
    // A hash of dev:ino:realpath is a confirmable fingerprint of guessable metadata, not an
    // opaque id; nothing needs one, so there is none.
    expect(Object.keys(m1.source).sort()).toEqual(
      ["journal_mode", "mtime_ms_at_start", "schema_version_max", "size_bytes_at_start", "user_version"].sort()
    );
    expect(JSON.stringify(m1)).not.toMatch(/identity_sha256/);
    expect(m1.disclosure.null_semantics).toMatch(/WITHHELD/);
    expect(m1.disclosure.absent_table_semantics).toMatch(/not (an )?(evidence|observation)/i);
    expect(m1.disclosure.tables_withheld).toEqual([...OBSERVATION_POLICY.withheldTables]);
    expect(m1.consistency.table_row_counts).toEqual({ file_claims: 7, schema_version: 5, sessions: 2 });
  });

  it("marks the FULL profile as restricted so the two cannot be confused", () => {
    const src = makeSensitiveRegistry(dir);
    const { meta } = full({ source: src, dest: join(dir, "f.db") });
    expect(meta.profile).toBe("full");
    expect(meta.disclosure.classification).toBe("restricted-full-fidelity");
    expect(meta.disclosure.transfer).toMatch(/do not/i);
  });
});

describe("verbatim columns are guarded too", () => {
  it.each([
    ["a path in host", "UPDATE sessions SET host = '/home/CANARYUSER/box'"],
    ["a home-relative repo", "UPDATE sessions SET repo = '~/secret-checkout'"],
    ["an oversized worktree_name", `UPDATE sessions SET worktree_name = '${"x".repeat(400)}'`],
  ])("observation refuses %s instead of disclosing it; full does not care", (_n, sql) => {
    const src = makeSensitiveRegistry(dir);
    const w = new Database(src);
    w.exec(sql);
    w.close();
    const before = readdirSync(dir).sort();
    const e = expectRefusal("SNAPSHOT_VERIFY_FAILED", () => observe({ source: src, dest: join(dir, "o.db") }));
    expect(e.message).toMatch(/refusing to disclose/);
    expect(readdirSync(dir).filter((n) => n.startsWith("o.db") || n.endsWith(".partial"))).toEqual([]);
    expect(before.length).toBeGreaterThan(0);
    full({ source: src, dest: join(dir, "f.db") }); // the restricted profile is the strict one
  });
});

describe("profile is a required, explicit choice", () => {
  it.each([undefined, "", "FULL", "redacted", "observation ", null, 1])("rejects profile %j", (p) => {
    const src = makeRegistry(dir);
    expectRefusal("INVALID_OPTION", () =>
      exportRegistrySnapshot({ source: src, dest: join(dir, "o.db"), profile: p as never })
    );
    expect(readdirSync(dir).filter((n) => n.startsWith("o.db"))).toEqual([]);
  });
});

describe("consistency is preserved by the streaming projection", () => {
  it("includes committed-but-uncheckpointed rows and excludes an open transaction", () => {
    const src = makeRegistry(dir);
    const w = initDb(src);
    w.pragma("wal_autocheckpoint = 0");
    const pin = new Database(src, { readonly: true });
    pin.exec("BEGIN");
    pin.prepare("SELECT COUNT(*) FROM sessions").get();
    addSession(w, "committed", ["c.ts"]);
    w.exec("BEGIN IMMEDIATE");
    addSession(w, "open", ["o.ts"]);

    const dest = join(dir, "obs.db");
    observe({ source: src, dest });
    expect(rows(dest, "SELECT id FROM sessions")).toEqual([{ id: "committed" }]);
    expect(rows(dest, "SELECT file_path FROM file_claims")).toEqual([{ file_path: "c.ts" }]);
    w.exec("ROLLBACK");
    pin.exec("COMMIT");
    pin.close();
    w.close();
  });

  it("is atomic across sessions and file_claims while another process commits", async () => {
    const src = makeRegistry(dir);
    const sqlitePath = createRequire(import.meta.url).resolve("better-sqlite3");
    const writer = `
      const Database = require(process.argv[1]);
      const db = new Database(process.argv[2]);
      db.pragma("busy_timeout = 5000"); db.pragma("foreign_keys = ON");
      const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
      const s = db.prepare("INSERT INTO sessions (id, repo) VALUES (?, 'icn')");
      const c = db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)");
      process.stdout.write("ready\\n");
      for (let k = 0; ; k++) { db.exec("BEGIN IMMEDIATE"); s.run("s"+k); sleep(1); c.run("f"+k, "s"+k); db.exec("COMMIT"); }
    `;
    const child: ChildProcess = spawn(process.execPath, ["-e", writer, sqlitePath, src], {
      stdio: ["ignore", "pipe", "inherit"],
    });
    try {
      await new Promise<void>((res, rej) => {
        child.once("error", rej);
        child.stdout!.once("data", () => res());
      });
      const seen: number[] = [];
      for (let i = 0; i < 15; i++) {
        const dest = join(dir, `o${i}.db`);
        observe({ source: src, dest });
        const [{ s, c }] = rows(
          dest,
          "SELECT (SELECT COUNT(*) FROM sessions) AS s, (SELECT COUNT(*) FROM file_claims) AS c"
        ) as Array<{ s: number; c: number }>;
        expect({ s, c }, `snapshot ${i} torn`).toEqual({ s, c: s });
        seen.push(s);
      }
      expect(new Set(seen).size).toBeGreaterThan(1);
    } finally {
      child.kill("SIGKILL");
    }
  }, 60_000);
});

describe("verifySnapshotPair — the consumer-side completeness check", () => {
  it.each(["full", "observation"] as const)("accepts a good %s export", (profile) => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "x.db");
    exportRegistrySnapshot({ profile, source: src, dest });
    expect(verifySnapshotPair(dest)).toEqual({ ok: true, profile, problems: [] });
  });

  it("rejects a database with no sidecar", () => {
    const src = makeRegistry(dir);
    const dest = join(dir, "x.db");
    full({ source: src, dest });
    unlinkSync(`${dest}.meta.json`);
    const v = verifySnapshotPair(dest);
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(/sidecar/i);
  });

  it("rejects a tampered database (hash and size are bound)", () => {
    const src = makeRegistry(dir);
    const dest = join(dir, "x.db");
    full({ source: src, dest });
    const fd = openSync(dest, "r+");
    writeSync(fd, Buffer.from([0xff]), 0, 1, statSync(dest).size - 1);
    closeSync(fd);
    const v = verifySnapshotPair(dest);
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(/sha256|integrity|bytes/i);
  });

  it("rejects a sidecar that belongs to a different snapshot", () => {
    const a = makeRegistry(dir, "a.db");
    const w = initDb(a);
    addSession(w, "only-in-a");
    w.close();
    const b = makeRegistry(dir, "b.db");
    full({ source: a, dest: join(dir, "a.snap") });
    full({ source: b, dest: join(dir, "b.snap") });
    writeFileSync(`${join(dir, "b.snap")}.meta.json`, readFileSync(`${join(dir, "a.snap")}.meta.json`));
    expect(verifySnapshotPair(join(dir, "b.snap")).ok).toBe(false);
  });

  it("rejects malformed, oversized and unknown-format sidecars without throwing", () => {
    const src = makeRegistry(dir);
    const dest = join(dir, "x.db");
    full({ source: src, dest });
    const meta = `${dest}.meta.json`;
    for (const body of ["{not json", "[]", "null", JSON.stringify({ format: "other/v9" }), "x".repeat(2_000_000)]) {
      writeFileSync(meta, body);
      expect(verifySnapshotPair(dest).ok).toBe(false);
    }
  });

  it("rejects an observation sidecar over a database that still holds withheld data", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "x.db");
    observe({ source: src, dest });
    // Swap in the FULL database but keep the observation sidecar, re-pointing the hash so the
    // only thing left to catch it is the profile invariant.
    const fullDest = join(dir, "full.db");
    full({ source: src, dest: fullDest });
    const meta = JSON.parse(readFileSync(`${dest}.meta.json`, "utf-8"));
    copyFileSync(fullDest, dest);
    meta.snapshot.sha256 = sha256Hex(readFileSync(dest));
    meta.snapshot.bytes = statSync(dest).size;
    writeFileSync(`${dest}.meta.json`, JSON.stringify(meta));
    const v = verifySnapshotPair(dest);
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(/observation|table|withheld/i);
  });

  it("rejects an observation database carrying a raw absolute path", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "x.db");
    observe({ source: src, dest });
    const db = new Database(dest);
    db.prepare("UPDATE file_claims SET file_path = '/etc/passwd' WHERE rowid = 1").run();
    db.close();
    const meta = JSON.parse(readFileSync(`${dest}.meta.json`, "utf-8"));
    meta.snapshot.sha256 = sha256Hex(readFileSync(dest));
    meta.snapshot.bytes = statSync(dest).size;
    writeFileSync(`${dest}.meta.json`, JSON.stringify(meta));
    const v = verifySnapshotPair(dest);
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(/path/i);
  });
});
