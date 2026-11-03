// Session-registry snapshot export — a consistent, offline-readable copy of an EXISTING
// icn-ops registry. Contract and limits: docs/architecture/AGENT_RUNTIME.md, "Registry snapshot
// export". What each profile discloses: ./snapshot-profile.ts.
//
// WHAT THIS IS NOT
//   - It is not a second registry, a sync channel, or an authority. ICN keeps session identity,
//     authorization and lifecycle; the output is an OBSERVATION of them at an instant.
//   - It never resolves a registry for the caller. There is no env var, no git-derived default,
//     no "current" database: the caller names the file. (`resolveDbPath` would silently pick the
//     live shared registry — exactly the wrong thing for a tool whose job is to avoid it.)
//   - It never opens the source through `initDb`, so it can neither create nor migrate it.
//   - It has no default PROFILE either. "full" (recovery, inside the trust boundary) and
//     "observation" (least-privilege, for an external reader) are different disclosure
//     decisions, and an omitted flag must not make one for the caller.
//
// HOW THE SNAPSHOT IS TAKEN
//   One READ TRANSACTION on a READ-ONLY connection is the consistency boundary:
//     full         a single `VACUUM INTO`, which SQLite runs inside one read transaction;
//     observation  an explicit BEGIN, then a streamed allowlisted projection, then COMMIT.
//   Either way every table reflects one committed point: in WAL mode that point includes
//   committed-but-uncheckpointed frames and excludes anything uncommitted, and concurrent
//   writers are neither blocked nor seen half-way. A raw file copy gives none of that.
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
//   not writable and no `-shm` exists the open fails and that failure is reported.
//
// WHAT THE EVIDENCE MEANS
//   Checks in the sidecar are run on the COPY. They establish that the copy is internally
//   consistent. They do not authenticate the producing host: `host.claimed` is self-asserted and
//   `host.authenticated` is null on purpose. The sha256 binds the sidecar to the file; it is
//   integrity evidence, not a signature.

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
  statfsSync,
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
import {
  checkObservationInvariants,
  OBSERVATION_POLICY,
  projectObservation,
  REQUIRED_COLUMNS,
  SNAPSHOT_PROFILES,
  type SnapshotProfile,
} from "./snapshot-profile.js";

export type { SnapshotProfile } from "./snapshot-profile.js";

export const SNAPSHOT_META_FORMAT = "icn-ops-registry-snapshot-meta/v1";
export const DEFAULT_BUSY_TIMEOUT_MS = 5000;
/** A registry is kilobytes to a few megabytes; anything near this is not one. */
export const DEFAULT_MAX_SOURCE_BYTES = 512 * 1024 * 1024;
export const DEFAULT_RESERVE_FREE_BYTES = 64 * 1024 * 1024;
/** Upper bound on a sidecar a verifier will read. Real ones are ~2 KiB. */
export const MAX_SIDECAR_BYTES = 1024 * 1024;
export const MAX_BUSY_TIMEOUT_MS = 10 * 60 * 1000;

const SQLITE_MAGIC = "SQLite format 3\0";
/** Tables the snapshot contract needs; a database without them is not an ops registry. */
const REQUIRED_TABLES = ["schema_version", "sessions", "file_claims"] as const;

/** Every refusal code the exporter can raise. The type below derives from this list. */
export const SNAPSHOT_ERROR_CODES = [
  "INVALID_OPTION",
  "SOURCE_MISSING",
  "SOURCE_NOT_FILE",
  "SOURCE_NOT_SQLITE",
  "SOURCE_UNREADABLE",
  "SOURCE_LOCKED",
  "SOURCE_CORRUPT",
  "SOURCE_CHANGED",
  "SOURCE_TOO_LARGE",
  "NOT_A_REGISTRY",
  "UNSUPPORTED_SCHEMA",
  "INSUFFICIENT_SPACE",
  "DEST_PARENT_MISSING",
  "DEST_EXISTS",
  "DEST_UNSAFE",
  "SNAPSHOT_VERIFY_FAILED",
  "PUBLISH_FAILED",
] as const;

export type SnapshotErrorCode = (typeof SNAPSHOT_ERROR_CODES)[number];

export class SnapshotError extends Error {
  // Explicit field, not a constructor parameter property: the hard-kill tests run this file
  // under Node's type stripping, which only supports erasable syntax.
  readonly code: SnapshotErrorCode;
  constructor(code: SnapshotErrorCode, message: string) {
    super(`${code}: ${message}`);
    this.name = "SnapshotError";
    this.code = code;
  }
}

export interface SnapshotHooks {
  /** After the source path was last checked, immediately before it is opened. */
  beforeSourceOpen?: () => void;
  /** After the copy exists, before it is verified. Receives the temp path. */
  afterCopy?: (tmpPath: string) => void;
  /** After verification, before anything is published. */
  afterVerify?: (tmpPath: string) => void;
  /** After the snapshot has its final name, before the sidecar does. */
  afterPublishDb?: () => void;
}

export interface SnapshotOptions {
  /** Which disclosure policy to apply. Required; there is no default. */
  profile: SnapshotProfile;
  /** Path of an EXISTING registry. Required; there is no default. */
  source: string;
  /** Final path of the snapshot. Must not exist; its directory must. The sidecar is `<dest>.meta.json`. */
  dest: string;
  /** How long to wait on a locked source, in ms. Integer 0–600000. Default 5000. */
  busyTimeoutMs?: number;
  /** Refuse a source (main file + WAL) larger than this. Default 512 MiB. */
  maxSourceBytes?: number;
  /** Free space that must remain on the destination filesystem. Default 64 MiB. */
  reserveFreeBytes?: number;
  /** Test seams for interruption; unused in production. */
  hooks?: SnapshotHooks;
}

interface MetaCommon {
  format: typeof SNAPSHOT_META_FORMAT;
  /** How and when the copy was made. */
  acquisition: {
    method: "sqlite-vacuum-into" | "sqlite-read-transaction-projection";
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

interface SourceCommon {
  /** Observed when the export started; NOT bound to the snapshot instant. */
  size_bytes_at_start: number;
  mtime_ms_at_start: number;
  journal_mode: string;
  schema_version_max: number;
  user_version: number;
}

export interface FullMeta extends MetaCommon {
  profile: "full";
  /** Identity of the database that was read — including its filesystem location. */
  source: SourceCommon & { path_given: string; realpath: string; dev: number; ino: number };
  disclosure: {
    classification: "restricted-full-fidelity";
    policy: "full-v1";
    contains: string[];
    transfer: string;
  };
}

export interface ObservationMeta extends MetaCommon {
  profile: "observation";
  /**
   * No path, device or inode, and deliberately NO derived source identity either: a hash of
   * guessable filesystem metadata is a confirmable fingerprint, not an opaque id, and nothing
   * needs one.
   */
  source: SourceCommon;
  disclosure: {
    classification: "least-privilege-observation";
    policy: typeof OBSERVATION_POLICY.name;
    tables_included: string[];
    tables_withheld: string[];
    sessions: {
      verbatim_columns: string[];
      digested_columns: string[];
      digest_transform: string;
      withheld_columns: string[];
    };
    file_claims: { path_policy: string; withheld_path_encoding: string; rows_with_withheld_path: number };
    null_semantics: string;
    absent_table_semantics: string;
    transfer: string;
  };
}

export type SnapshotMeta = FullMeta | ObservationMeta;

export interface SnapshotResult<M extends SnapshotMeta = SnapshotMeta> {
  dest: string;
  meta_path: string;
  meta: M;
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

function nonNegInt(name: string, v: unknown, max: number): number {
  if (typeof v !== "number" || !Number.isInteger(v) || v < 0 || v > max) {
    throw new SnapshotError("INVALID_OPTION", `${name} must be an integer between 0 and ${max}`);
  }
  return v;
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

/**
 * The path must still name the file we inspected. The stat that gave us dev/ino and the open
 * that reads the data are two operations on a NAME, so a rotation between them would make the
 * recorded identity describe a different file than the one copied. Not eliminated (there is no
 * fd to fstat through better-sqlite3), but narrowed to "replaced and restored", and every
 * check here fails closed.
 */
function assertSameFile(sourceReal: string, st: { dev: number; ino: number }): void {
  let now;
  try {
    now = statSync(sourceReal);
  } catch (e) {
    throw new SnapshotError("SOURCE_CHANGED", `${sourceReal} disappeared during the export (${errCode(e) ?? e})`);
  }
  if (now.dev !== st.dev || now.ino !== st.ino) {
    throw new SnapshotError(
      "SOURCE_CHANGED",
      `${sourceReal} was replaced during the export; what was read no longer matches the path`
    );
  }
}

function walBytes(sourceReal: string): number {
  try {
    return statSync(`${sourceReal}-wal`).size;
  } catch {
    return 0;
  }
}

function schemaVersions(db: Database.Database): number[] {
  return (db.prepare("SELECT version FROM schema_version ORDER BY version").all() as Array<{ version: number }>).map(
    (r) => r.version
  );
}

/**
 * Is this really a v5 ops registry? "Stamped v5" is not enough — the stamp is a row, not a
 * schema — so the objects and columns the contract depends on are checked too. Runs on the
 * source INSIDE the snapshot transaction (observation) and again on the finished copy (both
 * profiles), which is the authoritative check.
 */
function assertRegistryShape(db: Database.Database, label: string): number {
  const objects = db
    .prepare("SELECT type, name, sql FROM sqlite_master WHERE substr(name, 1, 7) != 'sqlite_'")
    .all() as Array<{ type: string; name: string; sql: string | null }>;
  for (const o of objects) {
    const virtual = o.type === "table" && /^\s*CREATE\s+VIRTUAL\b/i.test(o.sql ?? "");
    if (virtual || (o.type !== "table" && o.type !== "index")) {
      // A registry is tables and indexes. A view, trigger or virtual table is code the
      // consumer would inherit; refuse rather than carry it across a trust boundary.
      throw new SnapshotError(
        "NOT_A_REGISTRY",
        `${label} contains a ${virtual ? "virtual table" : o.type} "${o.name}"; a registry holds only tables and indexes`
      );
    }
  }
  const have = new Set(objects.filter((o) => o.type === "table").map((o) => o.name));
  const missing = REQUIRED_TABLES.filter((t) => !have.has(t));
  if (missing.length) {
    throw new SnapshotError("NOT_A_REGISTRY", `${label} lacks required table(s): ${missing.join(", ")}`);
  }
  const versions = schemaVersions(db);
  const max = versions.length ? Math.max(...versions) : 0;
  if (max !== LATEST_SCHEMA_VERSION) {
    throw new SnapshotError(
      "UNSUPPORTED_SCHEMA",
      `${label} schema is v${max}; this exporter supports only v${LATEST_SCHEMA_VERSION}. It will not migrate the source`
    );
  }
  for (const [table, expected] of Object.entries(REQUIRED_COLUMNS)) {
    const cols = new Set((db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map((c) => c.name));
    const lacking = expected.filter((c) => !cols.has(c));
    if (lacking.length) {
      throw new SnapshotError(
        "UNSUPPORTED_SCHEMA",
        `${label} is stamped v${max} but ${table} lacks column(s): ${lacking.join(", ")}`
      );
    }
  }
  return max;
}

// ── the export ───────────────────────────────────────────────────────────────

export function exportRegistrySnapshot(opts: SnapshotOptions & { profile: "full" }): SnapshotResult<FullMeta>;
export function exportRegistrySnapshot(opts: SnapshotOptions & { profile: "observation" }): SnapshotResult<ObservationMeta>;
export function exportRegistrySnapshot(opts: SnapshotOptions): SnapshotResult;
export function exportRegistrySnapshot(opts: SnapshotOptions): SnapshotResult {
  const hooks = opts.hooks ?? {};

  // 0. Options. Validated before anything touches the filesystem, and strictly: a NaN timeout
  //    used to be accepted and then recorded in the sidecar as if it were a real setting.
  const profile = opts.profile;
  if (!SNAPSHOT_PROFILES.includes(profile)) {
    throw new SnapshotError("INVALID_OPTION", `profile must be one of ${SNAPSHOT_PROFILES.join(", ")}; there is no default`);
  }
  const busy = nonNegInt("busyTimeoutMs", opts.busyTimeoutMs ?? DEFAULT_BUSY_TIMEOUT_MS, MAX_BUSY_TIMEOUT_MS);
  const maxSource = nonNegInt("maxSourceBytes", opts.maxSourceBytes ?? DEFAULT_MAX_SOURCE_BYTES, Number.MAX_SAFE_INTEGER);
  const reserve = nonNegInt("reserveFreeBytes", opts.reserveFreeBytes ?? DEFAULT_RESERVE_FREE_BYTES, Number.MAX_SAFE_INTEGER);

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

  // The WAL counts: a small main file with a large WAL is a large database.
  const sourceBytes = st.size + walBytes(sourceReal);
  if (sourceBytes > maxSource) {
    throw new SnapshotError(
      "SOURCE_TOO_LARGE",
      `source is ${sourceBytes} bytes including its WAL, over the ${maxSource} byte limit; raise maxSourceBytes deliberately if this is intended`
    );
  }

  // 2. Destination. Fully decided before we create a single byte.
  const plan = planDestination(opts.dest, sourceReal);
  try {
    const fs = statfsSync(plan.dir);
    const free = fs.bavail * fs.bsize;
    // The full copy is about as large as the source; the observation projection is a small
    // subset, so it only needs the reserve.
    const need = reserve + (profile === "full" ? sourceBytes : 0);
    if (free < need) {
      throw new SnapshotError("INSUFFICIENT_SPACE", `${plan.dir} has ${free} bytes free; this export needs ${need}`);
    }
  } catch (e) {
    if (e instanceof SnapshotError) throw e;
    throw new SnapshotError("INSUFFICIENT_SPACE", `cannot determine free space on ${plan.dir} (${errCode(e) ?? e}); failing closed`);
  }

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
    let sourceCommon: SourceCommon;
    let sqliteVersion: string;
    let projection: ReturnType<typeof projectObservation> | null = null;
    try {
      hooks.beforeSourceOpen?.();
      assertSameFile(sourceReal, st);
      src = new Database(sourceReal, { readonly: true, fileMustExist: true });
      src.pragma(`busy_timeout = ${busy}`);
      // A registry file is untrusted input as far as its stored SQL goes: never let schema
      // objects run functions on our behalf.
      src.pragma("trusted_schema = OFF");

      if (profile === "full") {
        // Preflight only — the authoritative check is on the finished copy.
        const max = assertRegistryShape(src, sourceReal);
        sqliteVersion = (src.prepare("SELECT sqlite_version() AS v").get() as { v: string }).v;
        sourceCommon = {
          size_bytes_at_start: st.size,
          mtime_ms_at_start: st.mtimeMs,
          journal_mode: String(src.pragma("journal_mode", { simple: true })),
          schema_version_max: max,
          user_version: Number(src.pragma("user_version", { simple: true })),
        };
        // ONE read transaction for the entire copy — this statement is the consistency boundary.
        src.prepare("VACUUM INTO ?").run(tmpDb);
      } else {
        // The explicit transaction IS the consistency boundary here: the first read below
        // pins the snapshot, and every later read — shape, identity, rows, counts — sees it.
        src.exec("BEGIN");
        try {
          const max = assertRegistryShape(src, sourceReal);
          sqliteVersion = (src.prepare("SELECT sqlite_version() AS v").get() as { v: string }).v;
          sourceCommon = {
            size_bytes_at_start: st.size,
            mtime_ms_at_start: st.mtimeMs,
            journal_mode: String(src.pragma("journal_mode", { simple: true })),
            schema_version_max: max,
            user_version: Number(src.pragma("user_version", { simple: true })),
          };
          projection = projectObservation(
            src,
            tmpDb,
            (m) => {
              throw new SnapshotError("SOURCE_TOO_LARGE", m);
            },
            // Same classification the post-copy check always used, but raised BEFORE the value
            // is written, so it never reaches the output file or a `.partial`.
            (m) => {
              throw new SnapshotError("SNAPSHOT_VERIFY_FAILED", `the copy was discarded: ${m}`);
            }
          );
        } finally {
          try {
            src.exec("ROLLBACK"); // read-only: nothing to commit, and ROLLBACK releases the snapshot
          } catch {
            /* closing the connection below releases it regardless */
          }
        }
      }
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
    // The path must still name what we read — checked AFTER the copy so that a rotation at any
    // point in the window, including inside SQLite's own copy, refuses instead of mislabelling.
    assertSameFile(sourceReal, st);

    // 4. Verify the COPY. Anything wrong here discards it.
    const v = verifyCopy(tmpDb, profile);
    if (projection) {
      for (const t of OBSERVATION_POLICY.tables) {
        const a = projection.source[t as keyof typeof projection.source];
        const b = v.table_row_counts[t];
        if (a !== b) {
          throw new SnapshotError(
            "SNAPSHOT_VERIFY_FAILED",
            `the copy was discarded: ${t} has ${b} row(s) but the source had ${a} in the same transaction`
          );
        }
      }
    }
    hooks.afterVerify?.(tmpDb);
    chmodSync(tmpDb, 0o600);
    fsyncPath(tmpDb, constants.O_RDWR);
    const sha = sha256File(tmpDb);
    const bytes = statSync(tmpDb).size;
    const finished = new Date();

    const prod = producerVersion();
    const common = {
      format: SNAPSHOT_META_FORMAT as typeof SNAPSHOT_META_FORMAT,
      acquisition: {
        method: profile === "full" ? ("sqlite-vacuum-into" as const) : ("sqlite-read-transaction-projection" as const),
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
      host: { claimed: { hostname: hostname() }, authenticated: null, authenticity: "none" as const },
      producer: {
        name: prod.name,
        version: prod.version,
        node: process.version,
        better_sqlite3: prod.better_sqlite3,
        sqlite_version: sqliteVersion,
        supported_schema_version: LATEST_SCHEMA_VERSION,
      },
    };

    let meta: SnapshotMeta;
    if (profile === "full") {
      meta = {
        ...common,
        profile: "full",
        source: { path_given: sourceGiven, realpath: sourceReal, dev: st.dev, ino: st.ino, ...sourceCommon },
        disclosure: {
          classification: "restricted-full-fidelity",
          policy: "full-v1",
          contains: [
            "mailbox message bodies and kinds",
            "event payloads and scopes",
            "free-form agent text (task_description, current_activity, task_ref, pr_ref)",
            "filesystem paths (worktree_path, transcript_path, repo_id, worktree_id)",
            "branch names and commit ids at registration",
            "process watcher labels",
            "health cache (service endpoints, cluster state)",
            "decision index",
          ],
          transfer:
            "For recovery and diagnostics inside the ICN trust boundary. Do not transfer to an external " +
            "consumer; use the observation profile for that.",
        },
      };
    } else {
      const P = OBSERVATION_POLICY;
      meta = {
        ...common,
        profile: "observation",
        source: sourceCommon,
        disclosure: {
          classification: "least-privilege-observation",
          policy: P.name,
          tables_included: [...P.tables],
          tables_withheld: [...P.withheldTables],
          sessions: {
            verbatim_columns: [...P.sessions.verbatim],
            digested_columns: [...P.sessions.digested],
            digest_transform: "value is replaced by 'sha256:' + hex(SHA-256 of its UTF-8 bytes); NULL stays NULL",
            withheld_columns: [...P.sessions.withheld],
          },
          file_claims: {
            path_policy:
              "a plain repo-relative path is shown as written; anything absolute, home-relative, " +
              "parent-escaping or containing control characters is replaced",
            withheld_path_encoding:
              "'withheld:' + 16 hex of SHA-256(per-export random salt, path). Equal paths share a token " +
              "within this snapshot only",
            rows_with_withheld_path: projection!.file_claims_withheld_path,
          },
          null_semantics: "A NULL in a withheld column means WITHHELD. It never means absent, none, or unknown-to-ICN.",
          absent_table_semantics:
            "A table listed in tables_withheld was not exported. Its absence is not evidence that it holds no rows.",
          transfer:
            "Designed for an external read-only consumer. It still discloses session ids, host names, " +
            "lane names and repo-relative file paths; see AGENT_RUNTIME.md, Registry snapshot export.",
        },
      };
    }

    createExclusive(tmpMeta);
    tmpMetaLive = true;
    writeFileSync(tmpMeta, JSON.stringify(meta, null, 2) + "\n", { mode: 0o600 });
    fsyncPath(tmpMeta, constants.O_RDWR);

    // 5. Publish. link(2) fails with EEXIST instead of replacing, so a destination that
    //    appeared after our check is still never overwritten. The sidecar goes last: a database
    //    without it is incomplete, and `verifySnapshotPair` says so.
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
    // half-pair is never left behind (a hard kill can still leave one; verifySnapshotPair
    // detects it).
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

/** Verify a finished snapshot file. Throws a SnapshotError describing the first problem. */
function verifyCopy(path: string, profile: SnapshotProfile): CopyVerdict {
  let db: Database.Database | null = null;
  const fail = (why: string): never => {
    throw new SnapshotError("SNAPSHOT_VERIFY_FAILED", `the copy was discarded: ${why}`);
  };
  try {
    db = new Database(path, { readonly: true, fileMustExist: true });
    db.pragma("trusted_schema = OFF");
    const integrity = String(db.pragma("integrity_check", { simple: true }));
    if (integrity !== "ok") fail(`integrity_check reported ${integrity}`);
    const fk = db.pragma("foreign_key_check") as unknown[];
    if (fk.length !== 0) fail(`${fk.length} foreign-key violation(s)`);

    const max = assertRegistryShape(db, "the copy");

    const journal = String(db.pragma("journal_mode", { simple: true }));
    if (journal !== "delete") fail(`journal_mode is ${journal}; a snapshot must be readable without sidecar files`);

    if (profile === "observation") {
      const problems = checkObservationInvariants(db);
      if (problems.length) fail(`observation invariants violated: ${problems.join("; ")}`);
    }

    const counts: Record<string, number> = {};
    const tables = (
      db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND substr(name, 1, 7) != 'sqlite_' ORDER BY name").all() as Array<{
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

// ── consumer-side check ──────────────────────────────────────────────────────

export interface PairVerdict {
  ok: boolean;
  profile: SnapshotProfile | null;
  problems: string[];
}

export interface VerifyPairOptions {
  /**
   * Refuse a pair whose declared profile differs. A consumer that expects the restricted
   * `observation` profile should always pass this: without it a perfectly valid `full` pair
   * (mailbox bodies, transcript paths) is reported `ok: true` with `profile: "full"`.
   */
  expectProfile?: SnapshotProfile;
}

/** lstat that never follows a link: true only for a plain regular file. */
function isPlainFile(p: string): boolean {
  try {
    return lstatSync(p).isFile();
  } catch {
    return false;
  }
}

/**
 * Is `<dest>` + `<dest>.meta.json` a complete, untampered snapshot pair? This is the check a
 * consumer must make before reading, and the one the tests use to prove that a hard kill leaves
 * debris that cannot pass for a snapshot. Never throws and never blocks on a special file: every
 * failure, including an unreadable file, a directory, a FIFO or a link in either position, is a
 * problem in the returned verdict.
 *
 * It establishes INTEGRITY (the pair is complete, hash-bound, and obeys its profile). It does
 * not establish authenticity: anyone who can write both files can write a matching pair.
 */
export function verifySnapshotPair(destArg: string, options: VerifyPairOptions = {}): PairVerdict {
  const problems: string[] = [];
  const state: { profile: SnapshotProfile | null } = { profile: null };
  const bail = (): PairVerdict => ({ ok: false, profile: state.profile, problems });
  try {
    return verifyPairUnguarded(destArg, options, problems, state, bail);
  } catch (e) {
    // Backstop for anything unforeseen (a file replaced mid-check, EIO, EACCES): a verdict, not a throw.
    problems.push(`the pair could not be verified (${errCode(e) ?? (e as Error).message})`);
    return bail();
  }
}

function verifyPairUnguarded(
  destArg: string,
  options: VerifyPairOptions,
  problems: string[],
  state: { profile: SnapshotProfile | null },
  bail: () => PairVerdict
): PairVerdict {
  const dest = resolve(destArg);
  const metaPath = `${dest}.meta.json`;

  // Decide what each path IS before opening anything: opening a FIFO blocks, and reading a
  // directory throws. Neither is a snapshot, so neither is ever opened.
  try {
    lstatSync(dest);
  } catch (e) {
    problems.push(`snapshot file is not readable (${errCode(e) ?? e})`);
    return bail();
  }
  if (!isPlainFile(dest)) {
    problems.push("snapshot is not a regular file");
    return bail();
  }

  let meta: Record<string, unknown>;
  try {
    lstatSync(metaPath);
  } catch (e) {
    problems.push(
      errCode(e) === "ENOENT"
        ? "sidecar is missing: this snapshot is incomplete or was interrupted"
        : `sidecar is unreadable or not JSON (${errCode(e) ?? (e as Error).message})`
    );
    return bail();
  }
  if (!isPlainFile(metaPath)) {
    problems.push("sidecar is not a regular file");
    return bail();
  }
  try {
    const size = statSync(metaPath).size;
    if (size > MAX_SIDECAR_BYTES) {
      problems.push(`sidecar is ${size} bytes, over the ${MAX_SIDECAR_BYTES} byte limit`);
      return bail();
    }
    const parsed: unknown = JSON.parse(readFileSync(metaPath, "utf-8"));
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      problems.push("sidecar is not a JSON object");
      return bail();
    }
    meta = parsed as Record<string, unknown>;
  } catch (e) {
    problems.push(`sidecar is unreadable or not JSON (${errCode(e) ?? (e as Error).message})`);
    return bail();
  }

  if (meta["format"] !== SNAPSHOT_META_FORMAT) problems.push(`unknown sidecar format ${JSON.stringify(meta["format"])}`);
  if (SNAPSHOT_PROFILES.includes(meta["profile"] as SnapshotProfile)) {
    state.profile = meta["profile"] as SnapshotProfile;
  } else problems.push("sidecar names no known profile");
  const profile = state.profile;
  if (options.expectProfile !== undefined && profile !== null && profile !== options.expectProfile) {
    problems.push(`the pair declares profile ${profile} but expected profile ${options.expectProfile}`);
  }

  const snap = meta["snapshot"] as Record<string, unknown> | undefined;
  if (!snap || typeof snap !== "object") {
    problems.push("sidecar has no snapshot block");
    return bail();
  }
  if (snap["file"] !== basename(dest)) problems.push("sidecar describes a different file name");
  let digest: string | null = null;
  let length: number | null = null;
  try {
    digest = sha256File(dest);
    length = statSync(dest).size;
  } catch (e) {
    problems.push(`snapshot file could not be read for hashing (${errCode(e) ?? (e as Error).message})`);
    return bail();
  }
  if (typeof snap["sha256"] !== "string" || !/^[0-9a-f]{64}$/.test(snap["sha256"])) {
    problems.push("sidecar sha256 is not a 64-hex digest");
  } else if (snap["sha256"] !== digest) {
    problems.push("sha256 does not match the snapshot file");
  }
  if (snap["bytes"] !== length) problems.push("byte length does not match the snapshot file");
  // The sidecar must agree with ITSELF about which disclosure policy it describes. A mislabelled
  // pair (observation label over a full-profile body, or a policy string from another version)
  // is exactly what a consumer selecting a profile explicitly needs us to refuse.
  if (profile !== null) {
    const disc = meta["disclosure"] as Record<string, unknown> | undefined;
    const wantClass = profile === "full" ? "restricted-full-fidelity" : "least-privilege-observation";
    const wantPolicy = profile === "full" ? "full-v1" : OBSERVATION_POLICY.name;
    if (disc?.["classification"] !== wantClass) {
      problems.push(`disclosure.classification is ${JSON.stringify(disc?.["classification"])}, expected ${wantClass} for profile ${profile}`);
    }
    if (disc?.["policy"] !== wantPolicy) {
      problems.push(`disclosure.policy is ${JSON.stringify(disc?.["policy"])}, expected ${wantPolicy} for profile ${profile}`);
    }
  }
  if (problems.length || profile === null) return bail();

  try {
    const v = verifyCopy(dest, profile);
    if (snap["schema_version_max"] !== v.schema_version_max) {
      problems.push("sidecar schema_version_max does not match the snapshot");
    }
    const claimed = (meta["consistency"] as { table_row_counts?: unknown } | undefined)?.table_row_counts;
    if (JSON.stringify(claimed) !== JSON.stringify(v.table_row_counts)) {
      problems.push("sidecar row counts do not match the snapshot");
    }
  } catch (e) {
    problems.push((e as Error).message);
  }
  return { ok: problems.length === 0, profile, problems };
}
