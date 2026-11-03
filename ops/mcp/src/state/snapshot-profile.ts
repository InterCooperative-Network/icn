// Disclosure policy for registry snapshots — the ONE place that decides what leaves the
// registry. Contract: docs/architecture/AGENT_RUNTIME.md, "Registry snapshot export".
//
// Two profiles, two different purposes, deliberately not interchangeable:
//
//   full         A byte-faithful consistent copy for recovery and diagnostics INSIDE the ICN trust
//                boundary. Contains mailbox bodies, free-form agent text, transcript and worktree
//                paths. Never an input to an external consumer.
//
//   observation  A least-privilege projection for an external read-only observer. Built from an
//                ALLOWLIST: a column or table that is not named here is not exported, so a
//                future migration cannot widen the disclosure by accident. A test fails until
//                every new column/table is classified.
//
// The observation file is BUILT, not redacted. Rows are streamed out of one read transaction on
// the source into a fresh file, so withheld data never reaches disk — not in the output, not in
// free pages, not in a `.partial` left by a hard kill. (Copy-then-DELETE would leave all three.)

import { createHash, randomBytes } from "crypto";
import Database from "better-sqlite3";
import { migrate } from "./db.js";

export type SnapshotProfile = "full" | "observation";
export const SNAPSHOT_PROFILES: readonly SnapshotProfile[] = ["full", "observation"];

export const OBSERVATION_POLICY = {
  name: "observation-v1",
  tables: ["schema_version", "sessions", "file_claims"],
  /**
   * Every other production table. They are ABSENT from an observation file, not present-and-empty:
   * an empty `mailbox` would read as "looked, found no messages", which is a false observation.
   */
  withheldTables: ["decision_index", "events", "health_cache", "mailbox", "watchers_process"],
  schema_version: { columns: ["version", "applied_at"] },
  sessions: {
    /** Identity, lane association, lifecycle and liveness. Copied exactly. */
    verbatim: [
      "id",
      "repo",
      "worktree_name",
      "worktree_generation",
      "provider_session_id",
      "parent_session_id",
      "provider",
      "agent_pid",
      "host",
      "started_at",
      "last_heartbeat",
      "last_progress",
      "progress_count",
    ],
    /**
     * Path-valued identity. `repo_id` is the absolute git common dir and `worktree_id` the
     * absolute git dir, so the identity columns themselves carry filesystem layout. They are
     * replaced by a stable digest: still a usable join key, no longer a path. This is
     * pseudonymisation, not secrecy — whoever knows the original can recompute it.
     */
    digested: ["repo_id", "worktree_id"],
    /** Free-form agent text and filesystem/branch detail. NULL in the export. */
    withheld: [
      "worktree",
      "worktree_path",
      "branch_at_registration",
      "head_at_registration",
      "task_description",
      "task_ref",
      "pr_ref",
      "transcript_path",
      "current_activity",
    ],
  },
  file_claims: { columns: ["file_path", "session_id", "claimed_at"] },
} as const;

/** Every column a v5 registry must have for either profile to mean what it says. */
export const REQUIRED_COLUMNS: Readonly<Record<string, readonly string[]>> = {
  schema_version: OBSERVATION_POLICY.schema_version.columns,
  sessions: [
    ...OBSERVATION_POLICY.sessions.verbatim,
    ...OBSERVATION_POLICY.sessions.digested,
    ...OBSERVATION_POLICY.sessions.withheld,
  ],
  file_claims: OBSERVATION_POLICY.file_claims.columns,
};

export const DIGEST_PREFIX = "sha256:";
export const WITHHELD_PATH_PREFIX = "withheld:";
export const DIGEST_RE = /^sha256:[0-9a-f]{64}$/;
export const WITHHELD_PATH_RE = /^withheld:[0-9a-f]{16}$/;
export const MAX_ROWS = 2_000_000;
/** Longest claim path (bytes) shown as written. */
export const MAX_PATH_BYTES = 512;

const hex = (s: string) => createHash("sha256").update(s, "utf8").digest("hex");

export const digestOf = (v: unknown): string => `${DIGEST_PREFIX}${hex(String(v))}`;

/**
 * The token that stands in for a withheld claim path:
 * `withheld:` + first 16 hex of SHA-256( salt || 0x00 || UTF-8(path) ).
 *
 * The salt is random PER EXPORT and is never written anywhere, so a token can be compared only
 * with other tokens in the same snapshot. It cannot be recomputed, or confirmed against a
 * guessed path, by anyone holding just the snapshot. Exported so the formula has a published
 * test vector; the production call site is below.
 */
export function claimToken(salt: Uint8Array, rawPath: string): string {
  return `${WITHHELD_PATH_PREFIX}${createHash("sha256").update(salt).update("\0").update(rawPath, "utf8").digest("hex").slice(0, 16)}`;
}

/** One path segment. Deliberately narrow: no ':' (drive letters, URI schemes), no '~', no '\\', no spaces. */
export const SAFE_SEGMENT = /^[A-Za-z0-9._@+=,-]+$/;
/** Verbatim identifier text: letters/digits and a few joiners. No '/', so it can never be a path. */
export const SAFE_VERBATIM = /^[\p{L}\p{N}._:@+ -]{1,256}$/u;

/**
 * May this file-claim path be shown to an external observer as written?
 *
 * A POSITIVE allowlist, not a list of known-bad shapes: claims are free text supplied by an
 * agent, and every "reject absolute / ~ / .. / drive letter" rule leaves some spelling that
 * still parses as a path elsewhere (`file:///x`, `a:b/c`). Only plain repo-relative paths made
 * of conservative segments qualify. Anything else — including legitimate paths with spaces or
 * non-ASCII — is TOKENISED, not dropped, so the failure direction is safe: the occupancy fact
 * survives and only the spelling is withheld. A repo-relative path IS the occupancy fact the
 * observer wants, so it is disclosed, and the doc says so.
 */
export function isDisclosablePath(p: string): boolean {
  if (p.length === 0 || Buffer.byteLength(p, "utf8") > MAX_PATH_BYTES) return false;
  return p.split("/").every((seg) => seg !== ".." && SAFE_SEGMENT.test(seg));
}

const isSafeVerbatim = (s: string): boolean => SAFE_VERBATIM.test(s);

/**
 * A timestamp as ICN writes it (SQLite `datetime('now')`: `YYYY-MM-DD HH:MM:SS`) or as ISO-8601
 * (`T` separator, optional fraction and `Z`). Anything else in a time column is free text, and
 * free text is exactly what the observation profile exists to keep out.
 */
export const TIMESTAMP_RE = /^\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(\.\d{1,9})?Z?$/;

/** Which kind of value a copied (non-digested) observation column may hold. */
type ValueKind = "text" | "timestamp" | "integer";
const COLUMN_KIND: Readonly<Record<string, Readonly<Record<string, ValueKind>>>> = {
  schema_version: { version: "integer", applied_at: "timestamp" },
  sessions: {
    id: "text",
    repo: "text",
    worktree_name: "text",
    worktree_generation: "text",
    provider_session_id: "text",
    parent_session_id: "text",
    provider: "text",
    agent_pid: "integer",
    host: "text",
    started_at: "timestamp",
    last_heartbeat: "timestamp",
    last_progress: "timestamp",
    progress_count: "integer",
  },
  file_claims: { session_id: "text", claimed_at: "timestamp" },
};

/**
 * The ONE value rule for a copied observation column, shared by the producer (before a value is
 * written) and by every consumer-side check, so the two can never disagree. NULL is allowed
 * everywhere: it is "not recorded", never a value. Returns a reason, or null when the value may
 * be disclosed. The reason never contains the value itself.
 */
export function observationValueProblem(table: string, column: string, v: unknown): string | null {
  if (v === null || v === undefined) return null;
  const kind = COLUMN_KIND[table]?.[column];
  if (kind === undefined) return `${table}.${column} is not a copied observation column`;
  switch (kind) {
    case "integer":
      return typeof v === "number" && Number.isFinite(v) ? null : `${table}.${column} holds a non-numeric value`;
    case "timestamp":
      return typeof v === "string" && TIMESTAMP_RE.test(v)
        ? null
        : `${table}.${column} holds a value that is not a timestamp; refusing to disclose it`;
    case "text":
      return typeof v === "string" && isSafeVerbatim(v)
        ? null
        : `${table}.${column} holds a path-like, oversized or non-text value; refusing to disclose it`;
  }
}

export interface ProjectionCounts {
  schema_version: number;
  sessions: number;
  file_claims: number;
}

export interface ProjectionResult {
  /** Rows written to the observation file. */
  written: ProjectionCounts;
  /** COUNT(*) of the same tables read inside the same source transaction. */
  source: ProjectionCounts;
  file_claims_withheld_path: number;
}

/**
 * Stream the allowlisted projection of `src` into a NEW file at `outPath`.
 *
 * The caller must already hold a read transaction on `src` (BEGIN issued, schema checked): that
 * transaction is the consistency boundary, exactly as the single `VACUUM INTO` is for the full
 * profile. Only allowlisted columns are ever SELECTed, so withheld values never enter this
 * process — a bug here cannot leak what it never read.
 */
export function projectObservation(
  src: Database.Database,
  outPath: string,
  fail: (message: string) => never,
  /** A value that may not be disclosed. Called BEFORE the value is passed to any INSERT. */
  reject: (message: string) => never = fail
): ProjectionResult {
  const out = new Database(outPath);
  try {
    out.pragma("foreign_keys = OFF"); // an orphan claim is reported by foreign_key_check, uniformly
    // The production ladder, so the DDL cannot drift from what ICN actually creates.
    out.transaction(() => migrate(out)).immediate();
    for (const t of OBSERVATION_POLICY.withheldTables) out.exec(`DROP TABLE "${t}"`);
    out.exec("DELETE FROM schema_version");

    const salt = randomBytes(16);
    const P = OBSERVATION_POLICY.sessions;
    const srcCols = [...P.verbatim, ...P.digested];
    const insertSession = out.prepare(
      `INSERT INTO sessions (${srcCols.join(", ")}) VALUES (${srcCols.map(() => "?").join(", ")})`
    );
    const insertVersion = out.prepare("INSERT INTO schema_version (version, applied_at) VALUES (?, ?)");
    const insertClaim = out.prepare("INSERT INTO file_claims (file_path, session_id, claimed_at) VALUES (?, ?, ?)");

    const written: ProjectionCounts = { schema_version: 0, sessions: 0, file_claims: 0 };
    let withheldPaths = 0;
    let budget = MAX_ROWS;
    const spend = () => {
      if (--budget < 0) fail(`more than ${MAX_ROWS} rows; refusing an unbounded projection`);
    };

    out.exec("BEGIN");
    for (const r of src.prepare("SELECT version, applied_at FROM schema_version ORDER BY version").iterate() as Iterable<
      Record<string, unknown>
    >) {
      spend();
      for (const c of OBSERVATION_POLICY.schema_version.columns) {
        const why = observationValueProblem("schema_version", c, r[c]);
        if (why) reject(why);
      }
      insertVersion.run(r["version"], r["applied_at"]);
      written.schema_version++;
    }
    for (const r of src
      .prepare(`SELECT ${srcCols.join(", ")} FROM sessions ORDER BY rowid`)
      .iterate() as Iterable<Record<string, unknown>>) {
      spend();
      const vals = srcCols.map((c) => {
        const v = r[c];
        if (v === null || v === undefined) return null;
        if ((P.digested as readonly string[]).includes(c)) return digestOf(v);
        // Validated BEFORE the INSERT: a value that may not be disclosed must never reach the
        // output file, including the `.partial` a hard kill leaves behind.
        const why = observationValueProblem("sessions", c, v);
        if (why) reject(why);
        return v;
      });
      insertSession.run(...(vals as never[]));
      written.sessions++;
    }
    for (const r of src
      .prepare("SELECT file_path, session_id, claimed_at FROM file_claims ORDER BY rowid")
      .iterate() as Iterable<Record<string, unknown>>) {
      spend();
      for (const c of ["session_id", "claimed_at"]) {
        const why = observationValueProblem("file_claims", c, r[c]);
        if (why) reject(why);
      }
      const raw = String(r["file_path"]);
      let shown = raw;
      if (!isDisclosablePath(raw)) {
        // Keep the row: dropping it would turn "claimed, name withheld" into "not claimed". A
        // per-export salt makes equal paths equal within THIS snapshot (conflicts stay
        // visible) while tokens are not comparable across snapshots or reversible by lookup.
        shown = claimToken(salt, raw);
        withheldPaths++;
      }
      insertClaim.run(shown, r["session_id"], r["claimed_at"]);
      written.file_claims++;
    }
    out.exec("COMMIT");

    const n = (t: string) => (src.prepare(`SELECT COUNT(*) AS c FROM ${t}`).get() as { c: number }).c;
    return {
      written,
      source: { schema_version: n("schema_version"), sessions: n("sessions"), file_claims: n("file_claims") },
      file_claims_withheld_path: withheldPaths,
    };
  } finally {
    try {
      out.close();
    } catch {
      /* the real error, if any, is already propagating */
    }
  }
}

/**
 * Invariants an observation file must satisfy. Run by the producer on its own output AND by any
 * consumer-side verifier, so a mislabelled or tampered file is caught the same way both times.
 * Returns problems (empty = fine); never throws on bad data.
 */
export function checkObservationInvariants(db: Database.Database): string[] {
  const problems: string[] = [];
  const P = OBSERVATION_POLICY;

  const objects = db
    .prepare("SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'")
    .all() as Array<{ type: string; name: string }>;
  const tables = objects.filter((o) => o.type === "table").map((o) => o.name);
  for (const t of tables) {
    if (!(P.tables as readonly string[]).includes(t)) problems.push(`observation file contains unexpected table "${t}"`);
  }
  for (const t of P.tables) if (!tables.includes(t)) problems.push(`observation file lacks table "${t}"`);
  for (const o of objects) {
    if (o.type !== "table" && o.type !== "index") problems.push(`observation file contains a ${o.type} "${o.name}"`);
  }
  if (problems.length) return problems; // shape is wrong; column-level checks would only add noise

  // EXACT layout for every observation table, not only `sessions`. table_xinfo (unlike
  // table_info) also lists generated columns, so a hidden/virtual column cannot slip past.
  const cols = (t: string) =>
    (db.prepare(`PRAGMA table_xinfo(${t})`).all() as Array<{ name: string }>).map((c) => c.name);
  for (const t of P.tables) {
    const have = cols(t);
    const want = new Set<string>(REQUIRED_COLUMNS[t]);
    for (const c of have) if (!want.has(c)) problems.push(`${t} has unclassified column "${c}"`);
    for (const c of want) if (!have.includes(c)) problems.push(`${t} lacks column "${c}"`);
  }
  if (problems.length) return problems; // a wrong layout makes the per-column reads below meaningless

  for (const c of P.sessions.withheld) {
    const n = (db.prepare(`SELECT COUNT(*) AS c FROM sessions WHERE ${c} IS NOT NULL`).get() as { c: number }).c;
    if (n) problems.push(`withheld column sessions.${c} is non-NULL in ${n} row(s)`);
  }
  for (const c of P.sessions.digested) {
    for (const r of db.prepare(`SELECT ${c} AS v FROM sessions WHERE ${c} IS NOT NULL`).all() as Array<{ v: unknown }>) {
      if (typeof r.v !== "string" || !DIGEST_RE.test(r.v)) {
        problems.push(`sessions.${c} holds a value that is not a sha256 digest`);
        break;
      }
    }
  }
  const valueCols: Array<[string, readonly string[]]> = [
    ["schema_version", P.schema_version.columns],
    ["sessions", P.sessions.verbatim],
    ["file_claims", ["session_id", "claimed_at"]],
  ];
  for (const [t, columns] of valueCols) {
    for (const c of columns) {
      for (const r of db.prepare(`SELECT ${c} AS v FROM ${t} WHERE ${c} IS NOT NULL`).all() as Array<{ v: unknown }>) {
        const why = observationValueProblem(t, c, r.v);
        if (why) {
          problems.push(why);
          break; // one finding per column is enough; never list (or echo) every offending row
        }
      }
    }
  }
  for (const r of db.prepare("SELECT file_path AS v FROM file_claims").all() as Array<{ v: unknown }>) {
    if (typeof r.v !== "string" || !(WITHHELD_PATH_RE.test(r.v) || isDisclosablePath(r.v))) {
      problems.push("file_claims.file_path holds a path that is neither repo-relative nor a withheld token");
      break;
    }
  }
  return problems;
}
