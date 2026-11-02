// Registry snapshot export — fixture-only tests.
//
// Every database here is a throwaway under a per-test temp dir. Nothing opens the real shared
// registry, and no test resolves a default path: the exporter takes an EXPLICIT source and has
// no env/default fallback, which one test below pins.

import { spawn, type ChildProcess } from "child_process";
import { createHash } from "crypto";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
  truncateSync,
  writeFileSync,
  openSync,
  writeSync,
  closeSync,
} from "fs";
import { createRequire } from "module";
import { hostname, tmpdir } from "os";
import { join } from "path";
import Database from "better-sqlite3";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { initDb, LATEST_SCHEMA_VERSION } from "../state/db.js";
import {
  exportRegistrySnapshot,
  SNAPSHOT_META_FORMAT,
  SnapshotError,
  type SnapshotErrorCode,
} from "../state/snapshot-export.js";

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "icn-snapshot-test-"));
});
afterEach(() => {
  // Restore perms a test may have removed, or rmSync cannot descend.
  try {
    chmodSync(dir, 0o700);
  } catch {
    /* gone */
  }
  rmSync(dir, { recursive: true, force: true });
});

/** A real, migrated schema-v5 registry — the production ladder, not a hand-rolled copy. */
function makeRegistry(name = "registry.db"): string {
  const p = join(dir, name);
  initDb(p).close();
  return p;
}

function addSession(db: Database.Database, id: string, claims: string[] = []): void {
  db.prepare("INSERT INTO sessions (id, repo, worktree_name) VALUES (?, 'icn', ?)").run(id, `wt-${id}`);
  for (const f of claims) {
    db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)").run(f, id);
  }
}

/** Canonical logical content of every user table, order-independent of physical layout. */
function logicalDump(path: string): string {
  const db = new Database(path, { readonly: true, fileMustExist: true });
  try {
    const tables = (
      db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").all() as Array<{ name: string }>
    ).map((t) => t.name);
    const out: Record<string, unknown[]> = {};
    for (const t of tables) out[t] = db.prepare(`SELECT * FROM "${t}" ORDER BY rowid`).all();
    return JSON.stringify(out);
  } finally {
    db.close();
  }
}

const sha256 = (p: string) => createHash("sha256").update(readFileSync(p)).digest("hex");

function expectRefusal(code: SnapshotErrorCode, fn: () => unknown): SnapshotError {
  try {
    fn();
  } catch (e) {
    expect(e).toBeInstanceOf(SnapshotError);
    expect((e as SnapshotError).code).toBe(code);
    return e as SnapshotError;
  }
  throw new Error(`expected SnapshotError ${code}, but the export succeeded`);
}

/** Names in the output dir — after a refusal this must be exactly the pre-existing set. */
const listing = (d: string) =>
  readdirSync(d)
    .filter((n) => !/\.db-(wal|shm)$/.test(n)) // SQLite wal-index metadata of a WAL source: see the sidecar test
    .sort();

describe("happy path and output contract", () => {
  it("exports a valid registry to a verified, independently readable snapshot", () => {
    const src = makeRegistry();
    const w = initDb(src);
    addSession(w, "s1", ["a.ts", "b.ts"]);
    addSession(w, "s2", ["c.ts"]);
    w.close();

    const dest = join(dir, "out.db");
    const r = exportRegistrySnapshot({ source: src, dest });

    expect(r.dest).toBe(dest);
    expect(r.meta_path).toBe(`${dest}.meta.json`);
    expect(logicalDump(dest)).toBe(logicalDump(src));

    const snap = new Database(dest, { readonly: true });
    expect(snap.pragma("integrity_check", { simple: true })).toBe("ok");
    expect(snap.pragma("foreign_key_check")).toEqual([]);
    expect((snap.prepare("SELECT COUNT(*) c FROM file_claims").get() as { c: number }).c).toBe(3);
    snap.close();
  });

  it("publishes exactly the snapshot and its sidecar — no temp debris, mode 0600", () => {
    const src = makeRegistry();
    const dest = join(dir, "out.db");
    const before = listing(dir);
    exportRegistrySnapshot({ source: src, dest });

    expect(listing(dir)).toEqual([...before, "out.db", "out.db.meta.json"].sort());
    expect(statSync(dest).mode & 0o777).toBe(0o600);
    expect(statSync(`${dest}.meta.json`).mode & 0o777).toBe(0o600);
  });

  it("is repeatable offline: reading the snapshot never creates sidecars or changes it", () => {
    const src = makeRegistry();
    const w = initDb(src);
    addSession(w, "s1", ["a.ts"]);
    w.close();
    const dest = join(dir, "out.db");
    exportRegistrySnapshot({ source: src, dest });

    const h = sha256(dest);
    const d1 = logicalDump(dest);
    const d2 = logicalDump(dest);
    expect(d2).toBe(d1);
    expect(sha256(dest)).toBe(h);
    expect(existsSync(`${dest}-wal`)).toBe(false);
    expect(existsSync(`${dest}-shm`)).toBe(false);
    const snap = new Database(dest, { readonly: true });
    expect(snap.pragma("journal_mode", { simple: true })).toBe("delete");
    snap.close();
  });

  it("sidecar keeps source identity, acquisition, consistency evidence and host claims apart", () => {
    const src = makeRegistry();
    const w = initDb(src);
    addSession(w, "s1", ["a.ts"]);
    w.close();
    const dest = join(dir, "out.db");
    const { meta } = exportRegistrySnapshot({ source: src, dest });
    const onDisk = JSON.parse(readFileSync(`${dest}.meta.json`, "utf-8"));
    expect(onDisk).toEqual(meta);

    expect(meta.format).toBe(SNAPSHOT_META_FORMAT);
    // source identity
    expect(meta.source.realpath).toBe(src);
    expect(meta.source.schema_version_max).toBe(LATEST_SCHEMA_VERSION);
    // acquisition
    expect(meta.acquisition.method).toBe("sqlite-vacuum-into");
    expect(Date.parse(meta.acquisition.started_at)).not.toBeNaN();
    expect(Date.parse(meta.acquisition.finished_at)).toBeGreaterThanOrEqual(
      Date.parse(meta.acquisition.started_at)
    );
    // consistency evidence is about the COPY and is hash-bound to the published file
    expect(meta.snapshot.sha256).toBe(sha256(dest));
    expect(meta.snapshot.bytes).toBe(statSync(dest).size);
    expect(meta.consistency.integrity_check).toBe("ok");
    expect(meta.consistency.foreign_key_violations).toBe(0);
    expect(meta.consistency.table_row_counts["sessions"]).toBe(1);
    expect(meta.consistency.table_row_counts["file_claims"]).toBe(1);
    // host: claimed is self-asserted; nothing authenticates it, and the file says so
    expect(meta.host.claimed.hostname).toBe(hostname());
    expect(meta.host.authenticated).toBeNull();
    expect(meta.host.authenticity).toBe("none");
    // producer
    expect(meta.producer.name).toBe("@icn/ops-mcp");
    expect(meta.producer.sqlite_version).toMatch(/^\d+\.\d+\.\d+/);
  });

  it("has no env or default-path fallback: an unset source is a usage error, not a guess", () => {
    const prev = process.env["ICN_OPS_DB"];
    process.env["ICN_OPS_DB"] = makeRegistry("env.db");
    try {
      expectRefusal("SOURCE_MISSING", () =>
        exportRegistrySnapshot({ source: "", dest: join(dir, "out.db") })
      );
    } finally {
      if (prev === undefined) delete process.env["ICN_OPS_DB"];
      else process.env["ICN_OPS_DB"] = prev;
    }
    expect(existsSync(join(dir, "out.db"))).toBe(false);
  });
});

describe("what reading a WAL source leaves behind", () => {
  it("may attach SQLite -wal/-shm metadata to the source, but never changes its main file or content", () => {
    const src = makeRegistry();
    const w = initDb(src);
    addSession(w, "s1", ["a.ts"]);
    w.close(); // last connection closed: SQLite checkpoints and removes -wal/-shm
    expect(existsSync(`${src}-wal`)).toBe(false);
    const mainBefore = sha256(src);
    const dumpBefore = logicalDump(src);
    // logicalDump itself opened the source read-only; clear its sidecars' effect on the count.
    exportRegistrySnapshot({ source: src, dest: join(dir, "out.db") });

    // Locking metadata for a reader of a WAL database. Not registry content; documented limit.
    const extra = readdirSync(dir).filter((n) => n.startsWith("registry.db") && n !== "registry.db");
    for (const n of extra) expect(["registry.db-wal", "registry.db-shm"]).toContain(n);
    expect(sha256(src)).toBe(mainBefore);
    expect(logicalDump(src)).toBe(dumpBefore);
  });
});

describe("WAL and transaction semantics", () => {
  it("includes committed-but-uncheckpointed WAL rows that a raw file copy would lose", () => {
    const src = makeRegistry();
    const w = initDb(src);
    w.pragma("wal_autocheckpoint = 0"); // writer never checkpoints
    // A long-lived reader pins the WAL so nothing can be checkpointed away.
    const pin = new Database(src, { readonly: true });
    pin.exec("BEGIN");
    pin.prepare("SELECT COUNT(*) FROM sessions").get();
    addSession(w, "wal-only", ["w.ts"]);
    expect(statSync(`${src}-wal`).size).toBeGreaterThan(0);

    // Control: the main file alone does NOT contain the committed row.
    const raw = join(dir, "raw-copy.db");
    writeFileSync(raw, readFileSync(src));
    const rawDb = new Database(raw, { readonly: true });
    expect((rawDb.prepare("SELECT COUNT(*) c FROM sessions").get() as { c: number }).c).toBe(0);
    rawDb.close();

    const dest = join(dir, "out.db");
    exportRegistrySnapshot({ source: src, dest });
    const snap = new Database(dest, { readonly: true });
    expect(snap.prepare("SELECT id FROM sessions").all()).toEqual([{ id: "wal-only" }]);
    expect((snap.prepare("SELECT COUNT(*) c FROM file_claims").get() as { c: number }).c).toBe(1);
    snap.close();
    pin.exec("COMMIT");
    pin.close();
    w.close();
  });

  it("never shows an open, uncommitted transaction — and shows it once committed", () => {
    const src = makeRegistry();
    const w = initDb(src);
    addSession(w, "base", ["base.ts"]);

    w.exec("BEGIN IMMEDIATE");
    addSession(w, "pending", ["p1.ts", "p2.ts"]);

    const d1 = join(dir, "during.db");
    exportRegistrySnapshot({ source: src, dest: d1 });
    const during = new Database(d1, { readonly: true });
    expect(during.prepare("SELECT id FROM sessions ORDER BY id").all()).toEqual([{ id: "base" }]);
    expect((during.prepare("SELECT COUNT(*) c FROM file_claims").get() as { c: number }).c).toBe(1);
    during.close();

    w.exec("COMMIT");
    const d2 = join(dir, "after.db");
    exportRegistrySnapshot({ source: src, dest: d2 });
    const after = new Database(d2, { readonly: true });
    expect(after.prepare("SELECT id FROM sessions ORDER BY id").all()).toEqual([
      { id: "base" },
      { id: "pending" },
    ]);
    expect((after.prepare("SELECT COUNT(*) c FROM file_claims").get() as { c: number }).c).toBe(3);
    after.close();
    w.close();

    // A rolled-back transaction leaves no trace either.
    const w2 = initDb(src);
    w2.exec("BEGIN IMMEDIATE");
    addSession(w2, "rolled-back", ["r.ts"]);
    w2.exec("ROLLBACK");
    w2.close();
    const d3 = join(dir, "rolledback.db");
    exportRegistrySnapshot({ source: src, dest: d3 });
    expect(logicalDump(d3)).toBe(logicalDump(d2));
  });

  it("is atomic across tables while a separate process commits multi-table transactions", async () => {
    const src = makeRegistry();
    const sqlitePath = createRequire(import.meta.url).resolve("better-sqlite3");

    // Each writer transaction inserts ONE sessions row, ONE file_claims row and ONE events row,
    // with a pause BETWEEN the statements so a non-snapshot reader would reliably catch it
    // mid-flight. The invariant a consistent snapshot must satisfy: all three counts are equal.
    const writerSrc = `
      const Database = require(process.argv[1]);
      const db = new Database(process.argv[2]);
      db.pragma("busy_timeout = 5000");
      db.pragma("foreign_keys = ON");
      const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
      const s = db.prepare("INSERT INTO sessions (id, repo) VALUES (?, 'icn')");
      const c = db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)");
      const e = db.prepare("INSERT INTO events (scope, type, payload, created_at) VALUES ('t','w','{}',?)");
      process.stdout.write("ready\\n");
      for (let k = 0; ; k++) {
        db.exec("BEGIN IMMEDIATE");
        s.run("s" + k); sleep(1);
        c.run("f" + k, "s" + k); sleep(1);
        e.run(k);
        db.exec("COMMIT");
      }
    `;
    const child: ChildProcess = spawn(process.execPath, ["-e", writerSrc, sqlitePath, src], {
      stdio: ["ignore", "pipe", "inherit"],
    });
    try {
      await new Promise<void>((resolve, reject) => {
        child.once("error", reject);
        child.stdout!.once("data", () => resolve());
      });

      const counts: number[] = [];
      for (let i = 0; i < 25; i++) {
        const dest = join(dir, `snap-${i}.db`);
        exportRegistrySnapshot({ source: src, dest });
        const snap = new Database(dest, { readonly: true });
        const n = (t: string) =>
          (snap.prepare(`SELECT COUNT(*) c FROM ${t}`).get() as { c: number }).c;
        const [s, c, e] = [n("sessions"), n("file_claims"), n("events")];
        expect({ s, c, e }, `snapshot ${i} is torn across tables`).toEqual({ s, c: s, e: s });
        expect(snap.pragma("foreign_key_check")).toEqual([]);
        snap.close();
        counts.push(s);
      }
      // Non-vacuity: the writer really was committing while we exported, and snapshots
      // really did observe different points in its history.
      expect(counts[counts.length - 1]).toBeGreaterThan(counts[0]!);
      expect(new Set(counts).size).toBeGreaterThan(1);
      for (let i = 1; i < counts.length; i++) expect(counts[i]).toBeGreaterThanOrEqual(counts[i - 1]!);
    } finally {
      child.kill("SIGKILL");
    }
  }, 60_000);

  it("leaves the source registry's logical content and file set untouched", () => {
    const src = makeRegistry();
    const w = initDb(src);
    addSession(w, "s1", ["a.ts"]);
    w.close();
    const beforeDump = logicalDump(src);
    const beforeFiles = listing(dir);

    exportRegistrySnapshot({ source: src, dest: join(dir, "out.db") });

    expect(logicalDump(src)).toBe(beforeDump);
    // Only the two published files are new; the source gained no stray files of its own.
    expect(listing(dir).filter((f) => !beforeFiles.includes(f))).toEqual(["out.db", "out.db.meta.json"]);
  });
});

describe("source refusal — fail closed, create nothing", () => {
  it("a missing database is not created, nor is its directory", () => {
    const missingDir = join(dir, "nope");
    const src = join(missingDir, "icn-ops.db");
    const before = listing(dir);
    expectRefusal("SOURCE_MISSING", () =>
      exportRegistrySnapshot({ source: src, dest: join(dir, "out.db") })
    );
    expect(existsSync(src)).toBe(false);
    expect(existsSync(missingDir)).toBe(false);
    expect(listing(dir)).toEqual(before);
  });

  it("refuses a directory", () => {
    const d = join(dir, "adir");
    mkdirSync(d);
    expectRefusal("SOURCE_NOT_FILE", () =>
      exportRegistrySnapshot({ source: d, dest: join(dir, "out.db") })
    );
  });

  it.each([
    ["an empty file", Buffer.alloc(0)],
    ["a text file", Buffer.from("this is not a database\n".repeat(200))],
  ])("refuses %s as not SQLite, without turning it into a database", (_n, bytes) => {
    const src = join(dir, "bogus.db");
    writeFileSync(src, bytes);
    const before = sha256(src);
    expectRefusal("SOURCE_NOT_SQLITE", () =>
      exportRegistrySnapshot({ source: src, dest: join(dir, "out.db") })
    );
    expect(sha256(src)).toBe(before);
    expect(listing(dir)).toEqual(["bogus.db"]);
  });

  it("refuses a valid SQLite file that is not a session registry", () => {
    const src = join(dir, "other.db");
    const o = new Database(src);
    o.exec("CREATE TABLE foo (a); INSERT INTO foo VALUES (1);");
    o.close();
    expectRefusal("NOT_A_REGISTRY", () =>
      exportRegistrySnapshot({ source: src, dest: join(dir, "out.db") })
    );
    expect(listing(dir)).toEqual(["other.db"]);
  });

  it.each([
    ["an older schema (v4)", (db: Database.Database) => db.exec("DELETE FROM schema_version WHERE version = 5")],
    ["a newer schema (v6)", (db: Database.Database) => db.exec("INSERT INTO schema_version (version) VALUES (6)")],
  ])("refuses %s rather than exporting a shape the consumer was not built for", (_n, mutate) => {
    const src = makeRegistry();
    const w = new Database(src);
    mutate(w);
    w.close();
    const e = expectRefusal("UNSUPPORTED_SCHEMA", () =>
      exportRegistrySnapshot({ source: src, dest: join(dir, "out.db") })
    );
    expect(e.message).toContain(String(LATEST_SCHEMA_VERSION));
    expect(existsSync(join(dir, "out.db"))).toBe(false);
  });

  it("refuses a corrupt database and leaves no artifact that could pass for a snapshot", () => {
    const src = makeRegistry();
    const w = initDb(src);
    for (let i = 0; i < 400; i++) addSession(w, `s${i}`, [`f${i}-${"x".repeat(200)}`]);
    w.pragma("wal_checkpoint(TRUNCATE)");
    w.close();
    // Scribble over the middle of the file, leaving the 100-byte header (and magic) intact.
    const size = statSync(src).size;
    const fd = openSync(src, "r+");
    writeSync(fd, Buffer.alloc(8192, 0xab), 0, 8192, Math.floor(size / 2));
    closeSync(fd);

    const before = listing(dir);
    let err: unknown;
    try {
      exportRegistrySnapshot({ source: src, dest: join(dir, "out.db") });
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(SnapshotError);
    expect(["SOURCE_CORRUPT", "SNAPSHOT_VERIFY_FAILED"]).toContain((err as SnapshotError).code);
    expect(listing(dir)).toEqual(before);
  });

  it("reports a truncated database as an error, not a smaller snapshot", () => {
    const src = makeRegistry();
    const w = initDb(src);
    for (let i = 0; i < 200; i++) addSession(w, `s${i}`, [`f${i}-${"y".repeat(200)}`]);
    w.pragma("wal_checkpoint(TRUNCATE)");
    w.close();
    truncateSync(src, Math.floor(statSync(src).size / 2));
    const before = listing(dir);
    let err: unknown;
    try {
      exportRegistrySnapshot({ source: src, dest: join(dir, "out.db") });
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(SnapshotError);
    expect(listing(dir)).toEqual(before);
  });

  it.skipIf(process.getuid?.() === 0)("an unreadable source is refused, not reported as empty or missing", () => {
    const src = makeRegistry();
    chmodSync(src, 0o000);
    const e = expectRefusal("SOURCE_UNREADABLE", () =>
      exportRegistrySnapshot({ source: src, dest: join(dir, "out.db") })
    );
    expect(e.message).not.toMatch(/missing|does not exist/i);
    chmodSync(src, 0o600);
    expect(existsSync(join(dir, "out.db"))).toBe(false);
  });

  it("a source locked by an exclusive writer is reported as LOCKED, bounded by the busy timeout", () => {
    // Rollback-journal mode: an EXCLUSIVE lock really does block readers (WAL readers never wait).
    const src = join(dir, "locked.db");
    initDb(src).close();
    const j = new Database(src);
    j.pragma("journal_mode = DELETE");
    j.exec("BEGIN EXCLUSIVE");

    const t0 = Date.now();
    expectRefusal("SOURCE_LOCKED", () =>
      exportRegistrySnapshot({ source: src, dest: join(dir, "out.db"), busyTimeoutMs: 100 })
    );
    expect(Date.now() - t0).toBeLessThan(5_000);
    j.exec("ROLLBACK");
    j.close();
    expect(existsSync(join(dir, "out.db"))).toBe(false);
  });
});

describe("destination refusal — no overwrite, no side effects", () => {
  it("refuses an existing destination and leaves it byte-identical", () => {
    const src = makeRegistry();
    const dest = join(dir, "out.db");
    writeFileSync(dest, "precious");
    expectRefusal("DEST_EXISTS", () => exportRegistrySnapshot({ source: src, dest }));
    expect(readFileSync(dest, "utf-8")).toBe("precious");
    expect(existsSync(`${dest}.meta.json`)).toBe(false);
  });

  it("refuses when only the sidecar already exists", () => {
    const src = makeRegistry();
    const dest = join(dir, "out.db");
    writeFileSync(`${dest}.meta.json`, "{}");
    expectRefusal("DEST_EXISTS", () => exportRegistrySnapshot({ source: src, dest }));
    expect(existsSync(dest)).toBe(false);
  });

  it("refuses a dangling symlink and a directory at the destination", () => {
    const src = makeRegistry();
    const link = join(dir, "link.db");
    symlinkSync(join(dir, "elsewhere.db"), link);
    expectRefusal("DEST_EXISTS", () => exportRegistrySnapshot({ source: src, dest: link }));
    expect(existsSync(join(dir, "elsewhere.db"))).toBe(false);

    const d = join(dir, "adir");
    mkdirSync(d);
    expectRefusal("DEST_EXISTS", () => exportRegistrySnapshot({ source: src, dest: d }));
  });

  it("refuses a destination that collides with the source or its WAL/SHM/journal files", () => {
    const src = makeRegistry();
    const before = sha256(src);
    for (const dest of [src, `${src}-wal`, `${src}-shm`, `${src}-journal`]) {
      expectRefusal("DEST_UNSAFE", () => exportRegistrySnapshot({ source: src, dest }));
    }
    expect(sha256(src)).toBe(before);
  });

  it("refuses a destination reached through a symlink to the source", () => {
    const src = makeRegistry();
    const alias = join(dir, "alias.db");
    symlinkSync(src, alias);
    // `alias` exists (it is a link), so it is refused either as existing or as the source.
    let code: string | undefined;
    try {
      exportRegistrySnapshot({ source: src, dest: alias });
    } catch (e) {
      code = (e as SnapshotError).code;
    }
    expect(["DEST_EXISTS", "DEST_UNSAFE"]).toContain(code);
  });

  it("does not create a missing destination directory", () => {
    const src = makeRegistry();
    const parent = join(dir, "not-here");
    expectRefusal("DEST_PARENT_MISSING", () =>
      exportRegistrySnapshot({ source: src, dest: join(parent, "out.db") })
    );
    expect(existsSync(parent)).toBe(false);
  });

  it("refuses a destination whose parent is a file", () => {
    const src = makeRegistry();
    const f = join(dir, "afile");
    writeFileSync(f, "x");
    expectRefusal("DEST_PARENT_MISSING", () =>
      exportRegistrySnapshot({ source: src, dest: join(f, "out.db") })
    );
  });

  it.skipIf(process.getuid?.() === 0)("an unwritable destination directory fails closed", () => {
    const src = makeRegistry();
    const ro = join(dir, "ro");
    mkdirSync(ro);
    chmodSync(ro, 0o500);
    let err: unknown;
    try {
      exportRegistrySnapshot({ source: src, dest: join(ro, "out.db") });
    } catch (e) {
      err = e;
    }
    chmodSync(ro, 0o700);
    expect(err).toBeInstanceOf(SnapshotError);
    expect(readdirSync(ro)).toEqual([]);
  });
});

describe("interruption and cleanup", () => {
  it("at every stage before publication, nothing with a final name exists", () => {
    const src = makeRegistry();
    const dest = join(dir, "out.db");
    const seen: Array<{ stage: string; dest: boolean; meta: boolean; partials: number }> = [];
    const probe = (stage: string) => () => {
      seen.push({
        stage,
        dest: existsSync(dest),
        meta: existsSync(`${dest}.meta.json`),
        partials: readdirSync(dir).filter((n) => n.endsWith(".partial")).length,
      });
    };
    exportRegistrySnapshot({
      source: src,
      dest,
      hooks: { afterCopy: probe("afterCopy"), afterVerify: probe("afterVerify") },
    });
    // What a SIGKILL at these points would leave: a clearly-named .partial, and no final file.
    expect(seen.map((s) => s.stage)).toEqual(["afterCopy", "afterVerify"]);
    for (const s of seen) {
      expect(s.dest).toBe(false);
      expect(s.meta).toBe(false);
      expect(s.partials).toBeGreaterThan(0);
    }
    expect(readdirSync(dir).some((n) => n.endsWith(".partial"))).toBe(false);
  });

  it.each(["afterCopy", "afterVerify", "afterPublishDb"] as const)(
    "a failure at %s removes every artifact of the attempt",
    (stage) => {
      const src = makeRegistry();
      const dest = join(dir, "out.db");
      const before = listing(dir);
      let err: unknown;
      try {
        exportRegistrySnapshot({
          source: src,
          dest,
          hooks: {
            [stage]: () => {
              throw new Error("injected interruption");
            },
          },
        });
      } catch (e) {
        err = e;
      }
      expect(err).toBeInstanceOf(Error);
      expect((err as Error).message).toContain("injected interruption");
      expect(listing(dir)).toEqual(before);
    }
  );

  it("a snapshot that fails verification is discarded, never published", () => {
    const src = makeRegistry();
    const dest = join(dir, "out.db");
    const before = listing(dir);
    expectRefusal("SNAPSHOT_VERIFY_FAILED", () =>
      exportRegistrySnapshot({
        source: src,
        dest,
        hooks: {
          // Damage the finished copy: the verifier must catch it.
          afterCopy: (tmp) => {
            const fd = openSync(tmp, "r+");
            writeSync(fd, Buffer.from("garbage-garbage-garbage"), 0, 23, 0);
            closeSync(fd);
          },
        },
      })
    );
    expect(listing(dir)).toEqual(before);
  });

  it("a destination that appears AFTER the up-front check is still never overwritten", () => {
    const src = makeRegistry();
    const dest = join(dir, "out.db");
    expectRefusal("DEST_EXISTS", () =>
      exportRegistrySnapshot({
        source: src,
        dest,
        hooks: { afterVerify: () => writeFileSync(dest, "raced-in") },
      })
    );
    expect(readFileSync(dest, "utf-8")).toBe("raced-in");
    expect(existsSync(`${dest}.meta.json`)).toBe(false);
  });

  it("a sidecar that appears late is not overwritten, and the half-pair is rolled back", () => {
    const src = makeRegistry();
    const dest = join(dir, "out.db");
    const meta = `${dest}.meta.json`;
    expectRefusal("DEST_EXISTS", () =>
      exportRegistrySnapshot({
        source: src,
        dest,
        hooks: { afterPublishDb: () => writeFileSync(meta, "raced-in") },
      })
    );
    expect(readFileSync(meta, "utf-8")).toBe("raced-in");
    expect(existsSync(dest)).toBe(false); // our db was withdrawn: no snapshot without its sidecar
  });

  it("a failed attempt does not poison a retry", () => {
    const src = makeRegistry();
    const dest = join(dir, "out.db");
    expect(() =>
      exportRegistrySnapshot({
        source: src,
        dest,
        hooks: {
          afterCopy: () => {
            throw new Error("boom");
          },
        },
      })
    ).toThrow("boom");
    const r = exportRegistrySnapshot({ source: src, dest });
    expect(r.meta.snapshot.sha256).toBe(sha256(dest));
  });
});
