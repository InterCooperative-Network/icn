// Derives the machine-readable producer contract and the published test vectors for the
// restricted OBSERVATION profile — from the real exporter's exports and from the generated
// fixture itself, never from a hand-maintained copy.
//
// The committed contract.json / test-vectors.json are compared against a fresh derivation by
// observation-golden-fixture.test.ts, so if the exporter changes and these are not regenerated,
// a test fails. That is what keeps this document honest.

import { readFileSync } from "fs";
import Database from "better-sqlite3";
import { LATEST_SCHEMA_VERSION } from "../../../state/db.js";
import {
  DEFAULT_BUSY_TIMEOUT_MS,
  DEFAULT_MAX_SOURCE_BYTES,
  DEFAULT_RESERVE_FREE_BYTES,
  MAX_BUSY_TIMEOUT_MS,
  MAX_SIDECAR_BYTES,
  SNAPSHOT_ERROR_CODES,
  SNAPSHOT_META_FORMAT,
} from "../../../state/snapshot-export.js";
import {
  claimToken,
  DIGEST_PREFIX,
  DIGEST_RE,
  digestOf,
  isDisclosablePath,
  MAX_PATH_BYTES,
  MAX_ROWS,
  OBSERVATION_POLICY,
  REQUIRED_COLUMNS,
  SAFE_SEGMENT,
  SAFE_VERBATIM,
  SNAPSHOT_PROFILES,
  WITHHELD_PATH_PREFIX,
  WITHHELD_PATH_RE,
} from "../../../state/snapshot-profile.js";

export const CONTRACT_VERSION = "icn-observation-producer-contract/v1";

/** Fictional inputs. Nothing here exists on any machine; digests of them are recomputable. */
export const SYNTHETIC = {
  root: "/synthetic/golden-root",
  repoId: "/synthetic/golden-root/repos/icn-golden.git",
  laneAlpha: "/synthetic/golden-root/repos/icn-golden.git/worktrees/lane-alpha",
  laneBeta: "/synthetic/golden-root/repos/icn-golden.git/worktrees/lane-beta",
  generationAlpha: "00000000-0000-4000-8000-0000000000a1",
  generationBeta: "00000000-0000-4000-8000-0000000000b1",
  conversationAlpha1: "5d1c2a4e-0000-4000-8000-00000000c0a1",
  conversationAlpha3: "5d1c2a4e-0000-4000-8000-00000000c0a3",
  conversationBeta1: "5d1c2a4e-0000-4000-8000-00000000c0b1",
  claimedHost: "golden-fixture-host",
} as const;

/** Strings that must never appear anywhere in the observation database or sidecar. */
export const PRIVACY_CANARIES: readonly string[] = [
  "GOLDEN-CANARY",
  SYNTHETIC.root, // any raw synthetic path: the digests must not leak their inputs
  SYNTHETIC.repoId,
  SYNTHETIC.laneAlpha,
  SYNTHETIC.laneBeta,
];

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

function flatten(v: unknown, prefix = "", out: Array<{ path: string; type: string }> = []) {
  if (v !== null && typeof v === "object" && !Array.isArray(v)) {
    for (const [k, x] of Object.entries(v as Record<string, unknown>)) flatten(x, prefix ? `${prefix}.${k}` : k, out);
  } else {
    out.push({ path: prefix, type: v === null ? "null" : Array.isArray(v) ? "array" : typeof v });
  }
  return out;
}

const hex = (b: Buffer, from: number, len: number) => b.subarray(from, from + len).toString("hex");

export function deriveContract(snapshotDb: string): Json {
  const P = OBSERVATION_POLICY;
  const header = readFileSync(snapshotDb).subarray(0, 100);
  const sidecar = JSON.parse(readFileSync(`${snapshotDb}.meta.json`, "utf-8")) as Record<string, unknown>;

  const db = new Database(snapshotDb, { readonly: true });
  const tables: Record<string, Json> = {};
  try {
    for (const t of P.tables) {
      const cols = db.prepare(`PRAGMA table_info(${t})`).all() as Array<{
        name: string;
        type: string;
        notnull: number;
        dflt_value: string | null;
        pk: number;
      }>;
      const disposition = (c: string) =>
        t !== "sessions"
          ? "verbatim"
          : (P.sessions.digested as readonly string[]).includes(c)
            ? "digest"
            : (P.sessions.withheld as readonly string[]).includes(c)
              ? "withheld"
              : "verbatim";
      tables[t] = cols.map((c) => ({
        name: c.name,
        declared_type: c.type,
        not_null_in_schema: c.notnull === 1,
        primary_key_position: c.pk,
        default: c.dflt_value,
        disposition: disposition(c.name),
        // What a reader may rely on in an observation file.
        null_in_export:
          disposition(c.name) === "withheld"
            ? "always"
            : c.notnull === 1 || c.pk > 0
              ? "never"
              : "when the source value was NULL",
      }));
    }
    const indexes = (
      db.prepare("SELECT name, tbl_name, sql FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%' ORDER BY name").all() as Array<{
        name: string;
        tbl_name: string;
        sql: string | null;
      }>
    ).map((i) => ({ name: i.name, table: i.tbl_name, sql: i.sql }));
    const present = (
      db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name").all() as Array<{
        name: string;
      }>
    ).map((r) => r.name);

    return {
      contract_version: CONTRACT_VERSION,
      scope:
        "Producer contract of ICN's RESTRICTED OBSERVATION registry snapshot. Not the full recovery profile, " +
        "which is a different disclosure policy and must never be substituted for it.",
      derived_from: [
        "ops/mcp/src/state/snapshot-profile.ts",
        "ops/mcp/src/state/snapshot-export.ts",
        "ops/mcp/src/state/db.ts",
        "ops/mcp/src/cli/snapshot-export.ts",
        "the generated observation.db and its sidecar (types, header, field paths)",
      ],
      profile: {
        selector: { sidecar_field: "profile", value: "observation", all_profiles: [...SNAPSHOT_PROFILES] },
        policy: { sidecar_field: "disclosure.policy", value: P.name },
        sidecar_format: { sidecar_field: "format", value: SNAPSHOT_META_FORMAT },
        registry_schema_version: LATEST_SCHEMA_VERSION,
        matching_rule:
          "A reader must require ALL of profile, disclosure.policy, format and schema version to equal what it was " +
          "built for. A new policy is a new value, never a silent change to this one.",
      },
      completeness: {
        marker: { filename_rule: "<snapshot file name> + '.meta.json', in the same directory", required: true },
        publication_order: "snapshot file first, sidecar last",
        consequence:
          "A snapshot without its sidecar is INCOMPLETE (a hard kill between the two publishes leaves exactly " +
          "that). It is not corrupt and not empty; it must be treated as 'no observation'.",
        checksum: {
          algorithm: "sha256",
          field: "snapshot.sha256",
          covers: "every byte of the snapshot file",
          does_not_cover: "the sidecar itself",
          also_bound: ["snapshot.bytes", "snapshot.file (basename)", "consistency.table_row_counts"],
          is_authenticity: false,
        },
        reference_check: "snapshot-export --verify <snapshot>  (library: verifySnapshotPair)",
        sidecar_max_bytes: MAX_SIDECAR_BYTES,
      },
      sidecar: {
        file_suffix: ".meta.json",
        encoding: "UTF-8 JSON object",
        fields: flatten(sidecar),
        environment_dependent_fields: [
          "acquisition.started_at",
          "acquisition.finished_at",
          "source.mtime_ms_at_start",
          "source.size_bytes_at_start",
          "producer.version",
          "producer.node",
          "producer.better_sqlite3",
          "producer.sqlite_version",
          "snapshot.sha256",
          "snapshot.bytes",
        ],
        absent_by_design: [
          "source.path_given",
          "source.realpath",
          "source.dev",
          "source.ino",
          "source.identity_sha256 (removed: a hash of guessable path metadata is a fingerprint, not an opaque id)",
        ],
        host: {
          claimed: "host.claimed.hostname is a SELF-ASSERTED string from the producing process",
          authenticated: "host.authenticated is null and host.authenticity is 'none': nothing attests the producer",
        },
      },
      database: {
        header: {
          magic: header.subarray(0, 16).toString("latin1").replace(/\0/g, "\\0"),
          page_size: header.readUInt16BE(16) === 1 ? 65536 : header.readUInt16BE(16),
          file_format_write_version: header[18],
          file_format_read_version: header[19],
          journal_mode: "delete (versions 1/1: no -wal/-shm sidecar files exist or are created by reading)",
          text_encoding: header.readUInt32BE(56) === 1 ? "UTF-8" : `code ${header.readUInt32BE(56)}`,
          application_id: header.readUInt32BE(68),
          user_version: header.readUInt32BE(60),
          raw_header_hex_bytes_16_to_24: hex(header, 16, 8),
        },
        tables_present: present,
        tables_absent_by_design: [...P.withheldTables],
        absent_table_semantics:
          "An absent table means NOT EXPORTED. It is never evidence that the table holds no rows, and a reader " +
          "must not synthesise an empty one.",
        columns: tables,
        indexes,
        no_other_objects: "only tables and indexes: no views, triggers or virtual tables",
        schema_version_rows: "copied from the source: one row per ladder step 1.." + LATEST_SCHEMA_VERSION,
        timestamps: "TEXT 'YYYY-MM-DD HH:MM:SS', UTC, as written by SQLite datetime('now')",
      },
      null_rules: {
        withheld: "A NULL in a column whose disposition is 'withheld' means WITHHELD. It never means absent, none or unknown-to-ICN.",
        digest: "NULL stays NULL. A non-NULL value becomes 'sha256:<64 hex>'. No value is ever invented.",
        verbatim: "NULL only where the source had NULL; semantics are the registry's own.",
        repo_id:
          "NOT 'NOT NULL': it was added in schema v3 as a nullable TEXT, and registration stores NULL when " +
          "lane discovery fails or the row predates v3. The export preserves NULL as NULL; it never fabricates a " +
          "digest and never exposes the original path. Non-null source values ALWAYS export as a valid digest.",
        worktree_generation: "NULL means the generation is unknown (a pre-v5 row), not 'none'.",
        provider_session_id: "NULL means the harness conversation id is unknown for that activation.",
      },
      identifiers: {
        digest: {
          applies_to: [...P.sessions.digested],
          algorithm: "SHA-256",
          input: "the UTF-8 bytes of the stored column value, exactly as stored (no normalisation, no trailing newline)",
          output: `'${DIGEST_PREFIX}' + lowercase hex`,
          pattern: DIGEST_RE.source,
          keyed: false,
          secret: false,
          authenticated: false,
          unguessable: false,
          recomputable_without_private_inputs: false,
          recomputable_from_the_raw_value: true,
          note:
            "UNKEYED. Anyone who knows or can enumerate the raw value can recompute it, and can therefore CONFIRM a " +
            "guess. It removes filesystem layout from the file; it is pseudonymisation, not secrecy and not " +
            "authentication. Stored values are the canonical git paths ICN produced on the registering host " +
            "(repo_id = git common dir, worktree_id = git dir), so equality needs the same canonical string.",
        },
        claim_path_token: {
          applies_to: ["file_claims.file_path (only when the path is not disclosable)"],
          formula: "'withheld:' + first 16 lowercase hex of SHA-256( salt || 0x00 || UTF-8(path) )",
          pattern: WITHHELD_PATH_RE.source,
          salt: "16 random bytes, fresh for every export, never written anywhere",
          stability_scope: "within ONE snapshot only: equal paths share a token, tokens are not comparable across snapshots",
          recomputable_from_snapshot: false,
          conflict_semantics:
            "Two rows with the same token are claims on the same withheld path. The ROW is always kept; a withheld " +
            "path never turns a claim into a non-claim.",
        },
        disclosable_claim_path: {
          rule: "every '/'-separated segment matches SAFE_SEGMENT and none is '..'; total length <= max_bytes",
          segment_pattern: SAFE_SEGMENT.source,
          max_bytes: MAX_PATH_BYTES,
          otherwise: "replaced by a token; conservative: spaces and non-ASCII are tokenised",
        },
        verbatim_text: {
          applies_to: [...P.sessions.verbatim.filter((c) => c !== "agent_pid" && c !== "progress_count")],
          pattern: SAFE_VERBATIM.source,
          flags: SAFE_VERBATIM.flags,
          on_violation: "the producer REFUSES the export (SNAPSHOT_VERIFY_FAILED); it never alters the value",
        },
        numeric_verbatim: ["agent_pid", "progress_count"],
      },
      field_policy: {
        sessions: {
          verbatim: [...P.sessions.verbatim],
          digested: [...P.sessions.digested],
          withheld: [...P.sessions.withheld],
        },
        required_source_columns: Object.fromEntries(Object.entries(REQUIRED_COLUMNS).map(([k, v]) => [k, [...v]])),
        unclassified_columns: "never exported (allowlist). A new source column is withheld until classified.",
        file_claims: [...P.file_claims.columns],
      },
      consistency: {
        boundary: "one read transaction on a read-only connection to the source",
        includes: "committed-but-uncheckpointed WAL frames",
        excludes: "uncommitted and rolled-back work",
        atomic_across_tables: true,
        row_counts_cross_checked_against_source_in_same_transaction: true,
        evidence_scope: "internal consistency of the COPY; not authenticity, not equality with any instant beyond SQLite's guarantee",
      },
      failure_semantics: {
        refusal_writes_nothing: true,
        exit_codes: { exported_or_verified: 0, refused_or_not_verified: 1, usage: 2 },
        error_codes: [...SNAPSHOT_ERROR_CODES],
        stderr_on_refusal: "one JSON line {ok:false, code, message}",
        stdout_on_verify: "one JSON line {ok, profile, problems[]}",
        hard_kill_leaves: [
          "owner-only '.<name>.<pid>.<rand>.partial' temp files (an observation partial holds only allowlisted data)",
          "possibly a snapshot with no sidecar (incomplete; detected by --verify)",
        ],
        retry_never_overwrites: "an existing snapshot or sidecar is refused as DEST_EXISTS",
      },
      limits: {
        max_source_bytes_default: DEFAULT_MAX_SOURCE_BYTES,
        max_source_counts: "main file + WAL",
        reserve_free_bytes_default: DEFAULT_RESERVE_FREE_BYTES,
        max_rows_total: MAX_ROWS,
        busy_timeout_ms: { default: DEFAULT_BUSY_TIMEOUT_MS, max: MAX_BUSY_TIMEOUT_MS },
        sidecar_max_bytes: MAX_SIDECAR_BYTES,
        file_mode: "0600",
      },
      reader_obligations: [
        "Select the observation profile explicitly; never accept 'whatever profile the sidecar names'.",
        "Require the sidecar and verify sha256, byte length and row counts before opening the database.",
        "Open read-only. The file is in delete journal mode, so reading creates no sidecar files.",
        "Match profile, disclosure.policy, format and schema version exactly; refuse on any difference.",
        "Treat NULL in a withheld column as WITHHELD and an absent table as NOT EXPORTED.",
        "Look lanes up by digest of an out-of-band raw worktree id; never expect a raw path in the file.",
        "Never present host.claimed.hostname or any sidecar field as authenticated.",
      ],
    };
  } finally {
    db.close();
  }
}

export interface FixtureFacts {
  lane_alpha_sessions: string[];
  lane_beta_sessions: string[];
  legacy_sessions: string[];
  sessions_with_null_conversation: string[];
  sessions_with_null_generation: string[];
  claims_cleartext_conflicts: Array<{ file_path: string; sessions: string[] }>;
  claims_withheld_token_groups: Array<{ sessions: string[] }>;
  claims_total: number;
  claims_withheld: number;
}

/** Facts read back from the generated database (not hand-asserted). Token VALUES are excluded. */
export function deriveFacts(snapshotDb: string): FixtureFacts {
  const db = new Database(snapshotDb, { readonly: true });
  try {
    const ids = (sql: string, ...a: unknown[]) =>
      (db.prepare(sql).all(...a) as Array<{ id: string }>).map((r) => r.id);
    const claims = db.prepare("SELECT file_path, session_id FROM file_claims ORDER BY file_path, session_id").all() as Array<{
      file_path: string;
      session_id: string;
    }>;
    const byPath = new Map<string, string[]>();
    for (const c of claims) byPath.set(c.file_path, [...(byPath.get(c.file_path) ?? []), c.session_id]);
    const clear = [...byPath].filter(([p, s]) => !p.startsWith(WITHHELD_PATH_PREFIX) && s.length > 1);
    const tok = [...byPath].filter(([p, s]) => p.startsWith(WITHHELD_PATH_PREFIX) && s.length > 1);
    return {
      lane_alpha_sessions: ids("SELECT id FROM sessions WHERE worktree_id = ? ORDER BY id", digestOf(SYNTHETIC.laneAlpha)),
      lane_beta_sessions: ids("SELECT id FROM sessions WHERE worktree_id = ? ORDER BY id", digestOf(SYNTHETIC.laneBeta)),
      legacy_sessions: ids("SELECT id FROM sessions WHERE worktree_id IS NULL ORDER BY id"),
      sessions_with_null_conversation: ids("SELECT id FROM sessions WHERE provider_session_id IS NULL ORDER BY id"),
      sessions_with_null_generation: ids("SELECT id FROM sessions WHERE worktree_generation IS NULL ORDER BY id"),
      claims_cleartext_conflicts: clear.map(([file_path, sessions]) => ({ file_path, sessions })),
      claims_withheld_token_groups: tok.map(([, sessions]) => ({ sessions })),
      claims_total: claims.length,
      claims_withheld: claims.filter((c) => c.file_path.startsWith(WITHHELD_PATH_PREFIX)).length,
    };
  } finally {
    db.close();
  }
}

const TEST_SALT_HEX = "000102030405060708090a0b0c0d0e0f";

export function deriveVectors(snapshotDb: string): Json {
  const salt = Buffer.from(TEST_SALT_HEX, "hex");
  const claimPaths = [
    "icn/crates/golden-core/src/lib.rs",
    "docs/golden/notes.md",
    "docs/golden/with space.md",
    `${SYNTHETIC.root}/GOLDEN-CANARY-secret.key`,
    "~/GOLDEN-CANARY-tilde-claim",
    "file:///etc/hosts",
    "a:b/c",
    "ok/../../etc/passwd",
    "./local/file.ts",
    "Cargo.lock",
    "dir/",
    "a//b",
  ];
  const verbatim = ["icn-golden", "lane-alpha", "golden-fixture-host", "2026-01-01 10:00:00", "espace-été", "box/../x", "https://x.invalid/y", "a\\b", ""];
  return {
    note:
      "Every value here is fictional or fixed. 'recomputable' vectors can be reproduced from the input shown with " +
      "nothing private. The fixture's own claim tokens are NOT recomputable (their salt was discarded); they are " +
      "covered by 'claim_token_formula' (a published salt) and by the structural facts below.",
    privacy_canaries: [...PRIVACY_CANARIES],
    synthetic_inputs: SYNTHETIC,
    digest_vectors: [
      { column: "repo_id", raw: SYNTHETIC.repoId, digest: digestOf(SYNTHETIC.repoId), recomputable: true },
      { column: "worktree_id", raw: SYNTHETIC.laneAlpha, digest: digestOf(SYNTHETIC.laneAlpha), recomputable: true },
      { column: "worktree_id", raw: SYNTHETIC.laneBeta, digest: digestOf(SYNTHETIC.laneBeta), recomputable: true },
      { column: "(any)", raw: "", digest: digestOf(""), recomputable: true, note: "empty string is non-NULL, so it digests" },
    ],
    claim_token_formula: {
      salt_hex: TEST_SALT_HEX,
      note: "A TEST salt, only to pin the formula. Real exports use a random, unrecorded salt.",
      vectors: [
        { path: `${SYNTHETIC.root}/GOLDEN-CANARY-secret.key`, token: claimToken(salt, `${SYNTHETIC.root}/GOLDEN-CANARY-secret.key`) },
        { path: "~/GOLDEN-CANARY-tilde-claim", token: claimToken(salt, "~/GOLDEN-CANARY-tilde-claim") },
        { path: "docs/golden/with space.md", token: claimToken(salt, "docs/golden/with space.md") },
      ],
    },
    claim_path_disclosure: claimPaths.map((path) => ({ path, disclosed_as_written: isDisclosablePath(path) })),
    verbatim_text_acceptance: verbatim.map((value) => ({ value, accepted: SAFE_VERBATIM.test(value) })),
    fixture_facts: deriveFacts(snapshotDb) as unknown as Json,
  };
}
