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
const DIGEST_RE = /^sha256:[0-9a-f]{64}$/;
const WITHHELD_PATH_RE = /^withheld:[0-9a-f]{16}$/;
/** Bound on any free-text-capable verbatim column. Real values are short identifiers. */
const MAX_VERBATIM_BYTES = 256;
const MAX_ROWS = 2_000_000;

const hex = (s: string) => createHash("sha256").update(s, "utf8").digest("hex");

export const digestOf = (v: unknown): string => `${DIGEST_PREFIX}${hex(String(v))}`;

/**
 * May this file-claim path be shown to an external observer as written?
 *
 * Only a plain repo-relative path qualifies. Claims are free text supplied by an agent, so they
 * can be absolute (`/home/<user>/...`), home-relative, parent-escaping, or hold control
 * characters. A repo-relative path IS the occupancy fact the observer wants; it is therefore
 * disclosed, and the doc says so.
 */
export function isDisclosablePath(p: string): boolean {
  if (p.length === 0 || Buffer.byteLength(p, "utf8") > 512) return false;
  if (/[\u0000-\u001f\u007f\\]/.test(p)) return false;
  if (p.startsWith("/") || p.startsWith("~") || /^[A-Za-z]:/.test(p)) return false;
  return !p.split("/").includes("..");
}

const looksLikePathOrJunk = (s: string): boolean =>
  Buffer.byteLength(s, "utf8") > MAX_VERBATIM_BYTES || /[\u0000-\u001f\u007f]/.test(s) || s.startsWith("/") || s.startsWith("~");

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
  fail: (message: string) => never
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
        return v;
      });
      insertSession.run(...(vals as never[]));
      written.sessions++;
    }
    for (const r of src
      .prepare("SELECT file_path, session_id, claimed_at FROM file_claims ORDER BY rowid")
      .iterate() as Iterable<Record<string, unknown>>) {
      spend();
      const raw = String(r["file_path"]);
      let shown = raw;
      if (!isDisclosablePath(raw)) {
        // Keep the row: dropping it would turn "claimed, name withheld" into "not claimed". A
        // per-export salt makes equal paths equal within THIS snapshot (conflicts stay
        // visible) while tokens are not comparable across snapshots or reversible by lookup.
        shown = `${WITHHELD_PATH_PREFIX}${createHash("sha256").update(salt).update("\0").update(raw, "utf8").digest("hex").slice(0, 16)}`;
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

  const cols = (t: string) => (db.prepare(`PRAGMA table_info(${t})`).all() as Array<{ name: string }>).map((c) => c.name);
  const known = new Set<string>(REQUIRED_COLUMNS["sessions"]);
  for (const c of cols("sessions")) if (!known.has(c)) problems.push(`sessions has unclassified column "${c}"`);

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
  const textual = P.sessions.verbatim.filter((c) => !["agent_pid", "progress_count"].includes(c));
  for (const c of textual) {
    for (const r of db.prepare(`SELECT ${c} AS v FROM sessions WHERE ${c} IS NOT NULL`).all() as Array<{ v: unknown }>) {
      if (typeof r.v !== "string" || looksLikePathOrJunk(r.v)) {
        problems.push(`sessions.${c} holds a path-like, oversized or non-text value; refusing to disclose it`);
        break;
      }
    }
  }
  for (const c of ["agent_pid", "progress_count"]) {
    for (const r of db.prepare(`SELECT ${c} AS v FROM sessions WHERE ${c} IS NOT NULL`).all() as Array<{ v: unknown }>) {
      if (typeof r.v !== "number") {
        problems.push(`sessions.${c} holds a non-numeric value`);
        break;
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
