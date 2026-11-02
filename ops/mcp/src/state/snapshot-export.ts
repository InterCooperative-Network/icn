// Session-registry snapshot export — a consistent, offline-readable copy of an EXISTING
// icn-ops registry. Contract and limits: docs/architecture/AGENT_RUNTIME.md, "Registry snapshot
// export".
//
// WHAT THIS IS NOT
//   - It is not a second registry, a sync channel, or an authority. ICN keeps session identity,
//     authorization and lifecycle; the output is an OBSERVATION of them at an instant.
//   - It never resolves a registry for the caller. There is no env var, no git-derived default,
//     no "current" database: the caller names the file. (`resolveDbPath` would silently pick the
//     live shared registry — exactly the wrong thing for a tool whose job is to avoid it.)
//   - It never opens the source through `initDb`, so it can neither create nor migrate it.
//
// HOW THE SNAPSHOT IS TAKEN
//   `VACUUM INTO` on a READ-ONLY connection. SQLite runs the whole copy inside ONE read
//   transaction, so every table reflects a single committed point: in WAL mode that point
//   includes committed-but-uncheckpointed frames and excludes anything uncommitted, and
//   concurrent writers are neither blocked nor observed half-way. A raw file copy gives none of
//   that (the committed rows may still be in `-wal`).
//
//   The online-backup API was considered and rejected: it copies page ranges across steps, and
//   a write by another connection between steps restarts it, so its consistency depends on
//   step sizing rather than on a single transaction.
//
//   `immutable=1` is never used: it tells SQLite the file cannot change, which is false for a
//   live registry and would let it read torn pages.
//
//   A read-only connection does not mean "touches nothing on disk". Opening a WAL database
//   still attaches the `-shm` wal-index (and creates it if absent). That is locking metadata,
//   not registry content, and it is what lets a reader see the WAL at all. If the directory is
//   not writable and no `-shm` exists the open fails, and that failure is reported, never
//   papered over.
//
// WHAT THE EVIDENCE MEANS
//   Checks in the sidecar are run on the COPY. They establish that the copy is internally
//   consistent (integrity_check, foreign keys across sessions/file_claims, supported schema).
//   They do not authenticate the producing host: `host.claimed` is self-asserted and
//   `host.authenticated` is null on purpose.

import { createHash, randomBytes } from "crypto";
import {
  chmodSync,
  closeSync,
  constants,
  fsyncSync,
  linkSync,
  lstatSync,
  openSync,
  readFileSync,
  readSync,
  realpathSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "fs";
import { createRequire } from "module";
import { hostname } from "os";
import { basename, dirname, join, resolve } from "path";
import { fileURLToPath } from "url";
import Database from "better-sqlite3";
import { LATEST_SCHEMA_VERSION } from "./db.js";

export const SNAPSHOT_META_FORMAT = "icn-ops-registry-snapshot-meta/v1";
export const DEFAULT_BUSY_TIMEOUT_MS = 5000;

const SQLITE_MAGIC = "SQLite format 3\0";
/** Tables the snapshot contract needs; a database without them is not an ops registry. */
const REQUIRED_TABLES = ["schema_version", "sessions", "file_claims"] as const;

export type SnapshotErrorCode =
  | "SOURCE_MISSING"
  | "SOURCE_NOT_FILE"
  | "SOURCE_NOT_SQLITE"
  | "SOURCE_UNREADABLE"
  | "SOURCE_LOCKED"
  | "SOURCE_CORRUPT"
  | "NOT_A_REGISTRY"
  | "UNSUPPORTED_SCHEMA"
  | "DEST_PARENT_MISSING"
  | "DEST_EXISTS"
  | "DEST_UNSAFE"
  | "SNAPSHOT_VERIFY_FAILED"
  | "PUBLISH_FAILED";

export class SnapshotError extends Error {
  constructor(
    readonly code: SnapshotErrorCode,
    message: string
  ) {
    super(`${code}: ${message}`);
    this.name = "SnapshotError";
  }
}

export interface SnapshotHooks {
  /** After the copy exists, before it is verified. Receives the temp path. */
  afterCopy?: (tmpPath: string) => void;
  /** After verification, before anything is published. */
  afterVerify?: (tmpPath: string) => void;
  /** After the snapshot has its final name, before the sidecar does. */
  afterPublishDb?: () => void;
}

export interface SnapshotOptions {
  /** Path of an EXISTING registry. Required; there is no default. */
  source: string;
  /** Final path of the snapshot. Must not exist; its directory must. The sidecar is `<dest>.meta.json`. */
  dest: string;
  /** How long to wait on a locked source. Default 5000. */
  busyTimeoutMs?: number;
  /** Test seams for interruption; unused in production. */
  hooks?: SnapshotHooks;
}

export interface SnapshotMeta {
  format: typeof SNAPSHOT_META_FORMAT;
  /** Identity of the database that was read. */
  source: {
    path_given: string;
    realpath: string;
    dev: number;
    ino: number;
    /** Observed when the export started; NOT bound to the snapshot instant. */
    size_bytes_at_start: number;
    mtime_ms_at_start: number;
    journal_mode: string;
    schema_version_max: number;
    user_version: number;
  };
  /** How and when the copy was made. */
  acquisition: {
    method: "sqlite-vacuum-into";
    started_at: string;
    finished_at: string;
    busy_timeout_ms: number;
  };
  /** The published file, hash-bound. A consumer must recompute sha256 before trusting the pair. */
  snapshot: {
    file: string;
    bytes: number;
    sha256: string;
    page_size: number;
    page_count: number;
    journal_mode: string;
    schema_version_max: number;
  };
  /** Checks run on the COPY. See the header comment for what this does and does not prove. */
  consistency: {
    integrity_check: string;
    foreign_key_violations: number;
    table_row_counts: Record<string, number>;
    note: string;
  };
  host: {
    /** Self-asserted by the producing process. */
    claimed: { hostname: string };
    /** Null unless an independent mechanism established it. None does today. */
    authenticated: null;
    authenticity: "none";
  };
  producer: {
    name: string;
    version: string;
    node: string;
    better_sqlite3: string;
    sqlite_version: string;
    supported_schema_version: number;
  };
}

export interface SnapshotResult {
  dest: string;
  meta_path: string;
  meta: SnapshotMeta;
}

// ── helpers ──────────────────────────────────────────────────────────────────

const errCode = (e: unknown): string | undefined => (e as NodeJS.ErrnoException | undefined)?.code;

/** Map a better-sqlite3 error raised while reading the SOURCE to a refusal. */
function mapSourceSqliteError(e: unknown, src: string): SnapshotError {
  const code = errCode(e) ?? "";
  const msg = (e as Error)?.message ?? String(e);
  if (code.startsWith("SQLITE_BUSY") || code.startsWith("SQLITE_LOCKED")) {
    return new SnapshotError("SOURCE_LOCKED", `${src} is locked by another connection (${msg})`);
  }
  if (code.startsWith("SQLITE_CORRUPT") || code.startsWith("SQLITE_NOTADB")) {
    return new SnapshotError("SOURCE_CORRUPT", `${src} is corrupt or not a database (${msg})`);
  }
  if (
    code.startsWith("SQLITE_CANTOPEN") ||
    code.startsWith("SQLITE_PERM") ||
    code.startsWith("SQLITE_READONLY") ||
    code.startsWith("SQLITE_IOERR") ||
    code.startsWith("SQLITE_AUTH")
  ) {
    return new SnapshotError("SOURCE_UNREADABLE", `${src} could not be read (${msg})`);
  }
  return new SnapshotError("SOURCE_UNREADABLE", `${src}: unexpected SQLite error (${code || "no code"}: ${msg})`);
}

function readHeader(path: string): Buffer {
  let fd: number;
  try {
    fd = openSync(path, constants.O_RDONLY);
  } catch (e) {
    if (errCode(e) === "EACCES" || errCode(e) === "EPERM") {
      throw new SnapshotError("SOURCE_UNREADABLE", `${path} is not readable (${errCode(e)})`);
    }
    throw new SnapshotError("SOURCE_UNREADABLE", `${path} could not be opened (${errCode(e) ?? e})`);
  }
  try {
    const buf = Buffer.alloc(100);
    const n = readSync(fd, buf, 0, 100, 0);
    return buf.subarray(0, n);
  } finally {
    closeSync(fd);
  }
}

function sha256File(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function fsyncPath(path: string, flags: number): void {
  const fd = openSync(path, flags);
  try {
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
}

function producerVersion(): { name: string; version: string; better_sqlite3: string } {
  const here = dirname(fileURLToPath(import.meta.url));
  let name = "@icn/ops-mcp";
  let version = "unknown";
  try {
    const pkg = JSON.parse(readFileSync(join(here, "../../package.json"), "utf-8")) as {
      name?: string;
      version?: string;
    };
    name = pkg.name ?? name;
    version = pkg.version ?? version;
  } catch {
    /* version stays "unknown" — recorded as such, never invented */
  }
  let better = "unknown";
  try {
    const p = createRequire(import.meta.url).resolve("better-sqlite3/package.json");
    better = (JSON.parse(readFileSync(p, "utf-8")) as { version?: string }).version ?? better;
  } catch {
    /* as above */
  }
  return { name, version, better_sqlite3: better };
}

interface DestPlan {
  dest: string;
  metaPath: string;
  dir: string;
}

function planDestination(destArg: string, sourceReal: string): DestPlan {
  const abs = resolve(destArg);
  const parent = dirname(abs);
  let parentReal: string;
  try {
    parentReal = realpathSync(parent);
    if (!statSync(parentReal).isDirectory()) throw Object.assign(new Error("not a dir"), { code: "ENOTDIR" });
  } catch (e) {
    throw new SnapshotError(
      "DEST_PARENT_MISSING",
      `destination directory ${parent} does not exist or is not a directory (${errCode(e) ?? e}); it is never created`
    );
  }
  const dest = join(parentReal, basename(abs));
  const metaPath = `${dest}.meta.json`;

  // Collision with the source or its SQLite sidecars comes BEFORE the exists-check, because
  // the source itself exists and would otherwise be reported as a mere "already there".
  const forbidden = new Set([sourceReal, `${sourceReal}-wal`, `${sourceReal}-shm`, `${sourceReal}-journal`]);
  for (const p of [dest, metaPath]) {
    if (forbidden.has(p)) {
      throw new SnapshotError("DEST_UNSAFE", `${p} is the source registry or one of its SQLite sidecar files`);
    }
  }
  for (const p of [dest, metaPath]) {
    try {
      lstatSync(p);
    } catch (e) {
      if (errCode(e) === "ENOENT") continue;
      throw new SnapshotError("DEST_UNSAFE", `cannot inspect ${p} (${errCode(e) ?? e})`);
    }
    // lstat, not stat: a dangling or aliasing symlink is "something there" too.
    throw new SnapshotError("DEST_EXISTS", `${p} already exists; refusing to overwrite`);
  }
  return { dest, metaPath, dir: parentReal };
}

/** Create an empty, exclusively-owned 0600 file. SQLite accepts a pre-existing EMPTY target. */
function createExclusive(path: string): void {
  try {
    closeSync(openSync(path, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL, 0o600));
  } catch (e) {
    throw new SnapshotError(
      "DEST_UNSAFE",
      `cannot create a temporary file in ${dirname(path)} (${errCode(e) ?? e})`
    );
  }
}

function tryUnlink(path: string | null): void {
  if (!path) return;
  try {
    unlinkSync(path);
  } catch {
    /* already gone, or not ours to worry about */
  }
}

function schemaVersions(db: Database.Database): number[] {
  return (db.prepare("SELECT version FROM schema_version ORDER BY version").all() as Array<{ version: number }>).map(
    (r) => r.version
  );
}

function hasTables(db: Database.Database): string[] {
  const have = new Set(
    (db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all() as Array<{ name: string }>).map(
      (r) => r.name
    )
  );
  return REQUIRED_TABLES.filter((t) => !have.has(t));
}

// ── the export ───────────────────────────────────────────────────────────────

export function exportRegistrySnapshot(opts: SnapshotOptions): SnapshotResult {
  const hooks = opts.hooks ?? {};
  const busy = opts.busyTimeoutMs ?? DEFAULT_BUSY_TIMEOUT_MS;

  // 1. Source identity. Nothing here can create anything.
  if (typeof opts.source !== "string" || opts.source.trim() === "") {
    throw new SnapshotError("SOURCE_MISSING", "no source registry given; there is deliberately no default");
  }
  const sourceGiven = opts.source;
  const sourceAbs = resolve(sourceGiven);
  let st;
  try {
    st = statSync(sourceAbs);
  } catch (e) {
    if (errCode(e) === "EACCES") {
      throw new SnapshotError("SOURCE_UNREADABLE", `${sourceAbs} is not accessible (EACCES)`);
    }
    throw new SnapshotError("SOURCE_MISSING", `${sourceAbs} does not exist (${errCode(e) ?? e}); it is never created`);
  }
  if (!st.isFile()) throw new SnapshotError("SOURCE_NOT_FILE", `${sourceAbs} is not a regular file`);
  const sourceReal = realpathSync(sourceAbs);

  const header = readHeader(sourceReal);
  if (header.length < 100 || header.subarray(0, 16).toString("latin1") !== SQLITE_MAGIC) {
    // Deliberately BEFORE any `new Database()`: SQLite would happily treat an empty file as a
    // brand-new database and we would have "initialised" something.
    throw new SnapshotError("SOURCE_NOT_SQLITE", `${sourceReal} does not carry a SQLite header`);
  }

  // 2. Destination. Fully decided before we create a single byte.
  const plan = planDestination(opts.dest, sourceReal);

  const tag = `${process.pid}.${randomBytes(6).toString("hex")}`;
  const tmpDb = join(plan.dir, `.${basename(plan.dest)}.${tag}.partial`);
  const tmpMeta = join(plan.dir, `.${basename(plan.metaPath)}.${tag}.partial`);
  let tmpDbLive = false;
  let tmpMetaLive = false;
  let publishedDb = false;

  try {
    createExclusive(tmpDb);
    tmpDbLive = true;

    // 3. Read-only open of the source, and the copy itself.
    const started = new Date();
    let src: Database.Database | null = null;
    let sourceMeta: SnapshotMeta["source"];
    let sqliteVersion: string;
    try {
      src = new Database(sourceReal, { readonly: true, fileMustExist: true });
      src.pragma(`busy_timeout = ${Math.max(0, Math.trunc(busy))}`);

      const missing = hasTables(src);
      if (missing.length) {
        throw new SnapshotError("NOT_A_REGISTRY", `${sourceReal} lacks required table(s): ${missing.join(", ")}`);
      }
      const versions = schemaVersions(src);
      const max = versions.length ? Math.max(...versions) : 0;
      if (max !== LATEST_SCHEMA_VERSION) {
        throw new SnapshotError(
          "UNSUPPORTED_SCHEMA",
          `registry schema is v${max}; this exporter supports only v${LATEST_SCHEMA_VERSION}. It will not migrate the source`
        );
      }
      sqliteVersion = (src.prepare("SELECT sqlite_version() AS v").get() as { v: string }).v;
      sourceMeta = {
        path_given: sourceGiven,
        realpath: sourceReal,
        dev: st.dev,
        ino: st.ino,
        size_bytes_at_start: st.size,
        mtime_ms_at_start: st.mtimeMs,
        journal_mode: String(src.pragma("journal_mode", { simple: true })),
        schema_version_max: max,
        user_version: Number(src.pragma("user_version", { simple: true })),
      };
      // ONE read transaction for the entire copy — this statement is the consistency boundary.
      src.prepare("VACUUM INTO ?").run(tmpDb);
    } catch (e) {
      if (e instanceof SnapshotError) throw e;
      throw mapSourceSqliteError(e, sourceReal);
    } finally {
      try {
        src?.close();
      } catch {
        /* a failed close must not mask the real error */
      }
    }

    hooks.afterCopy?.(tmpDb);

    // 4. Verify the COPY. Anything wrong here discards it.
    const v = verifyCopy(tmpDb);
    hooks.afterVerify?.(tmpDb);
    chmodSync(tmpDb, 0o600);
    fsyncPath(tmpDb, constants.O_RDWR);
    const sha = sha256File(tmpDb);
    const bytes = statSync(tmpDb).size;
    const finished = new Date();

    const prod = producerVersion();
    const meta: SnapshotMeta = {
      format: SNAPSHOT_META_FORMAT,
      source: sourceMeta,
      acquisition: {
        method: "sqlite-vacuum-into",
        started_at: started.toISOString(),
        finished_at: finished.toISOString(),
        busy_timeout_ms: busy,
      },
      snapshot: {
        file: basename(plan.dest),
        bytes,
        sha256: sha,
        page_size: v.page_size,
        page_count: v.page_count,
        journal_mode: v.journal_mode,
        schema_version_max: v.schema_version_max,
      },
      consistency: {
        integrity_check: v.integrity_check,
        foreign_key_violations: v.foreign_key_violations,
        table_row_counts: v.table_row_counts,
        note:
          "Checks were run on the copy. They show the copy is internally consistent; they do not " +
          "authenticate the producing host or prove the registry's contents were true.",
      },
      host: { claimed: { hostname: hostname() }, authenticated: null, authenticity: "none" },
      producer: {
        name: prod.name,
        version: prod.version,
        node: process.version,
        better_sqlite3: prod.better_sqlite3,
        sqlite_version: sqliteVersion,
        supported_schema_version: LATEST_SCHEMA_VERSION,
      },
    };

    createExclusive(tmpMeta);
    tmpMetaLive = true;
    writeFileSync(tmpMeta, JSON.stringify(meta, null, 2) + "\n", { mode: 0o600 });
    fsyncPath(tmpMeta, constants.O_RDWR);

    // 5. Publish. link(2) fails with EEXIST instead of replacing, so a destination that
    //    appeared after our check is still never overwritten. The sidecar goes last: it is the
    //    commit marker, and consumers must require it AND a matching sha256.
    try {
      linkSync(tmpDb, plan.dest);
      publishedDb = true;
    } catch (e) {
      if (errCode(e) === "EEXIST") {
        throw new SnapshotError("DEST_EXISTS", `${plan.dest} appeared during export; refusing to overwrite`);
      }
      throw new SnapshotError("PUBLISH_FAILED", `cannot publish ${plan.dest} (${errCode(e) ?? e})`);
    }
    hooks.afterPublishDb?.();
    try {
      linkSync(tmpMeta, plan.metaPath);
    } catch (e) {
      if (errCode(e) === "EEXIST") {
        throw new SnapshotError("DEST_EXISTS", `${plan.metaPath} appeared during export; refusing to overwrite`);
      }
      throw new SnapshotError("PUBLISH_FAILED", `cannot publish ${plan.metaPath} (${errCode(e) ?? e})`);
    }
    publishedDb = false; // pair complete: nothing left to roll back
    try {
      fsyncPath(plan.dir, constants.O_RDONLY);
    } catch {
      /* directory fsync is best-effort durability, not correctness */
    }

    return { dest: plan.dest, meta_path: plan.metaPath, meta };
  } catch (e) {
    // Roll back whatever THIS attempt made. A published db without its sidecar is removed so a
    // half-pair is never left behind.
    if (publishedDb) tryUnlink(plan.dest);
    throw e;
  } finally {
    if (tmpDbLive) tryUnlink(tmpDb);
    if (tmpMetaLive) tryUnlink(tmpMeta);
  }
}

interface CopyVerdict {
  integrity_check: string;
  foreign_key_violations: number;
  table_row_counts: Record<string, number>;
  page_size: number;
  page_count: number;
  journal_mode: string;
  schema_version_max: number;
}

function verifyCopy(path: string): CopyVerdict {
  let db: Database.Database | null = null;
  const fail = (why: string): never => {
    throw new SnapshotError("SNAPSHOT_VERIFY_FAILED", `the copy was discarded: ${why}`);
  };
  try {
    db = new Database(path, { readonly: true, fileMustExist: true });
    const integrity = String(db.pragma("integrity_check", { simple: true }));
    if (integrity !== "ok") fail(`integrity_check reported ${integrity}`);
    const fk = db.pragma("foreign_key_check") as unknown[];
    if (fk.length !== 0) fail(`${fk.length} foreign-key violation(s)`);

    const missing = hasTables(db);
    if (missing.length) fail(`required table(s) missing: ${missing.join(", ")}`);
    const versions = schemaVersions(db);
    const max = versions.length ? Math.max(...versions) : 0;
    if (max !== LATEST_SCHEMA_VERSION) {
      throw new SnapshotError("UNSUPPORTED_SCHEMA", `the copy is schema v${max}, expected v${LATEST_SCHEMA_VERSION}`);
    }

    const journal = String(db.pragma("journal_mode", { simple: true }));
    if (journal !== "delete") fail(`journal_mode is ${journal}; a snapshot must be readable without sidecar files`);

    const counts: Record<string, number> = {};
    const tables = (
      db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name").all() as Array<{
        name: string;
      }>
    ).map((t) => t.name);
    for (const t of tables) {
      counts[t] = (db.prepare(`SELECT COUNT(*) AS c FROM "${t.replace(/"/g, '""')}"`).get() as { c: number }).c;
    }
    return {
      integrity_check: integrity,
      foreign_key_violations: 0,
      table_row_counts: counts,
      page_size: Number(db.pragma("page_size", { simple: true })),
      page_count: Number(db.pragma("page_count", { simple: true })),
      journal_mode: journal,
      schema_version_max: max,
    };
  } catch (e) {
    if (e instanceof SnapshotError) throw e;
    throw new SnapshotError(
      "SNAPSHOT_VERIFY_FAILED",
      `the copy could not be verified (${errCode(e) ?? "error"}: ${(e as Error).message})`
    );
  } finally {
    try {
      db?.close();
    } catch {
      /* ignore */
    }
  }
}
