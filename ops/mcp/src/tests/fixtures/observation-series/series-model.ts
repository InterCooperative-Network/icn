// Derives the machine-readable SERIES contract (reference/series-contract.json) from a set of
// REAL exporter outputs. Nothing here edits a database or sidecar: it only READS them.
//
// Everything written to the committed contract must be byte-stable across regenerations, so it
// contains no sha256, no byte length, no claim-token VALUE and no real-time quantity — only
// things the generator controls (virtual acquisition times, deterministic session ids) or that
// are structural (counts, id sets, equality classes of tokens WITHIN one snapshot).
//
// verify_series.py recomputes the same facts and diffs independently, in another language, so
// this file cannot be the only witness to its own output.

import { readFileSync } from "fs";
import { join } from "path";
import Database from "better-sqlite3";
import { LATEST_SCHEMA_VERSION } from "../../../state/db.js";
import { DEFAULT_TTL_MINUTES, MIN_TTL_MINUTES } from "../../../runtime/session-runtime.js";
import { SNAPSHOT_META_FORMAT } from "../../../state/snapshot-export.js";
import { OBSERVATION_POLICY } from "../../../state/snapshot-profile.js";

export const SERIES_CONTRACT_VERSION = "icn-observation-series-contract/v1";

type Row = Record<string, unknown>;
type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

/** Session columns compared between exports (everything the observation profile keeps). */
export const COMPARED_FIELDS = [
  "repo",
  "repo_id",
  "worktree_id",
  "worktree_name",
  "worktree_generation",
  "provider",
  "provider_session_id",
  "parent_session_id",
  "host",
  "agent_pid",
  "started_at",
  "last_heartbeat",
  "last_progress",
  "progress_count",
] as const;

/** Fields that ICN's real code NEVER changes for an existing session id (see source facts). */
export const IMMUTABLE_FIELDS = ["repo", "provider", "provider_session_id", "parent_session_id", "host", "started_at"] as const;
/** Fields that legitimately change, and the operations that change them. */
export const LANE_FIELDS = ["repo_id", "worktree_id", "worktree_name", "worktree_generation"] as const;

export interface ExportFacts {
  label: string;
  registry: string;
  started_at: string;
  finished_at: string;
  claimed_host: string;
  applied_at: Array<[number, string]>;
  sessions: Record<string, Row>;
  claims: Array<{ session_id: string; file_path: string }>;
}

export function readExport(dir: string, label: string, registry: string): ExportFacts {
  const db = join(dir, `${label}.db`);
  const meta = JSON.parse(readFileSync(`${db}.meta.json`, "utf-8"));
  const d = new Database(db, { readonly: true });
  try {
    const sessions: Record<string, Row> = {};
    for (const r of d.prepare("SELECT * FROM sessions ORDER BY id").all() as Row[]) sessions[String(r["id"])] = r;
    return {
      label,
      registry,
      started_at: meta.acquisition.started_at,
      finished_at: meta.acquisition.finished_at,
      claimed_host: meta.host.claimed.hostname,
      applied_at: (d.prepare("SELECT version, applied_at FROM schema_version ORDER BY version").all() as Row[]).map((r) => [
        Number(r["version"]),
        String(r["applied_at"]),
      ]),
      sessions,
      claims: d.prepare("SELECT session_id, file_path FROM file_claims ORDER BY file_path, session_id").all() as Array<{
        session_id: string;
        file_path: string;
      }>,
    };
  } finally {
    d.close();
  }
}

const isToken = (p: string) => p.startsWith("withheld:");

/** UTC seconds-resolution string of an ISO instant, comparable to SQLite datetime('now'). */
export const sqliteTime = (iso: string): string => iso.slice(0, 19).replace("T", " ");

function ageMinutes(acquisition: string, stamp: string | null): number | null {
  if (!stamp) return null;
  return Math.floor((Date.parse(`${sqliteTime(acquisition)}Z`.replace(" ", "T")) - Date.parse(`${stamp.replace(" ", "T")}Z`)) / 60000);
}

export function exportSummary(f: ExportFacts): Json {
  const ids = Object.keys(f.sessions).sort();
  const clear = new Map<string, string[]>();
  const groups = new Map<string, string[]>();
  for (const c of f.claims) {
    const m = isToken(c.file_path) ? groups : clear;
    m.set(c.file_path, [...(m.get(c.file_path) ?? []), c.session_id]);
  }
  const ages: Record<string, number | null> = {};
  for (const id of ids) ages[id] = ageMinutes(f.started_at, f.sessions[id]!["last_heartbeat"] as string | null);
  return {
    label: f.label,
    registry: f.registry,
    claimed_host: f.claimed_host,
    acquisition: { started_at: f.started_at, finished_at: f.finished_at },
    schema_version_applied_at: f.applied_at as unknown as Json,
    counts: {
      sessions: ids.length,
      file_claims: f.claims.length,
      withheld_claim_rows: f.claims.filter((c) => isToken(c.file_path)).length,
    },
    session_ids: ids,
    heartbeat_age_minutes_at_acquisition: ages,
    cleartext_claims: [...clear].map(([path, sessions]) => ({ path, sessions: sessions.sort() })).sort((a, b) => a.path.localeCompare(b.path)),
    // Equality classes WITHIN this snapshot only. Token values are deliberately absent.
    withheld_claim_groups: [...groups.values()].map((s) => s.sort()).sort((a, b) => a.join().localeCompare(b.join())),
    row_timestamps_after_acquisition: ids.filter((id) =>
      ["started_at", "last_heartbeat", "last_progress"].some((c) => {
        const v = f.sessions[id]![c] as string | null;
        return v !== null && v > sqliteTime(f.started_at);
      })
    ),
  };
}

export type Verdict = "comparable_if_same_registry_asserted" | "refused_different_registry" | "refused_not_later" | "refused_anomaly";

export function comparePair(a: ExportFacts, b: ExportFacts): Json {
  const sameStamps = JSON.stringify(a.applied_at) === JSON.stringify(b.applied_at);
  const reasons: string[] = [];
  let verdict: Verdict = "comparable_if_same_registry_asserted";

  if (!sameStamps) {
    verdict = "refused_different_registry";
    reasons.push("schema_version.applied_at differs: these cannot be exports of one continuous registry (stamps are written once per registry)");
  } else if (!(Date.parse(b.started_at) > Date.parse(a.started_at))) {
    verdict = "refused_not_later";
    reasons.push(`acquisition.started_at is not later (${b.started_at} <= ${a.started_at}): the clock is sampled, not monotonic, so order is unestablished`);
  }

  const ia = new Set(Object.keys(a.sessions));
  const ib = new Set(Object.keys(b.sessions));
  const newly = [...ib].filter((x) => !ia.has(x)).sort();
  const absent = [...ia].filter((x) => !ib.has(x)).sort();
  const changed: Record<string, Record<string, [unknown, unknown]>> = {};
  const unchanged: string[] = [];
  const anomalies: string[] = [];
  for (const id of [...ia].filter((x) => ib.has(x)).sort()) {
    const diff: Record<string, [unknown, unknown]> = {};
    for (const f of COMPARED_FIELDS) {
      const x = a.sessions[id]![f] ?? null;
      const y = b.sessions[id]![f] ?? null;
      if (x !== y) diff[f] = [x, y];
    }
    if (Object.keys(diff).length) changed[id] = diff;
    else unchanged.push(id);
    for (const f of IMMUTABLE_FIELDS) if (diff[f]) anomalies.push(`${id}: ${f} changed, which ICN never does to an existing session id`);
    if (diff["progress_count"] && Number(diff["progress_count"][1]) < Number(diff["progress_count"][0])) {
      anomalies.push(`${id}: progress_count decreased`);
    }
  }
  if (verdict === "comparable_if_same_registry_asserted" && anomalies.length) {
    verdict = "refused_anomaly";
    reasons.push("an invariant ICN's code maintains was violated between the exports");
  }

  const key = (c: { session_id: string; file_path: string }) => `${c.file_path}\u0000${c.session_id}`;
  const ca = new Map(a.claims.filter((c) => !isToken(c.file_path)).map((c) => [key(c), c]));
  const cb = new Map(b.claims.filter((c) => !isToken(c.file_path)).map((c) => [key(c), c]));
  const fmt = (c: { session_id: string; file_path: string }) => ({ path: c.file_path, session_id: c.session_id });

  return {
    from: a.label,
    to: b.label,
    verdict,
    reasons,
    same_applied_at_stamps: sameStamps,
    sessions: {
      newly_present: newly,
      absent_in_later_export: absent,
      changed: changed as unknown as Json,
      unchanged,
    },
    claims: {
      cleartext_added: [...cb].filter(([k]) => !ca.has(k)).map(([, c]) => fmt(c)),
      cleartext_removed: [...ca].filter(([k]) => !cb.has(k)).map(([, c]) => fmt(c)),
      withheld: "not_comparable_across_exports",
    },
    anomalies,
    absence_meaning:
      "ROW ABSENT. In ICN the only deletion path is releaseSession, but nothing in either export states that the two are the same " +
      "registry, in what order they were taken, or that no restore/rotation happened. 'Released' may be inferred only under an " +
      "out-of-band same-registry assertion; otherwise absence is not evidence of the session's end.",
    shared_with_other_registry:
      sameStamps
        ? null
        : {
            conversation_ids: [...new Set(Object.values(a.sessions).map((s) => s["provider_session_id"]).filter((v): v is string => typeof v === "string"))]
              .filter((c) => Object.values(b.sessions).some((s) => s["provider_session_id"] === c))
              .sort(),
            lane_digests: [...new Set(Object.values(a.sessions).map((s) => s["worktree_id"]).filter((v): v is string => typeof v === "string"))]
              .filter((c) => Object.values(b.sessions).some((s) => s["worktree_id"] === c))
              .sort(),
            note: "Equal values here are COINCIDENCE of unkeyed digests / conversation ids across hosts, never identity.",
          },
  };
}

export interface SeriesContext {
  /** label of each export, in generation order, with its registry */
  exports: Array<{ label: string; registry: string }>;
  pairs: Array<[string, string]>;
  operations: Json[];
  session_labels: Record<string, Record<string, string>>;
  registries: Record<string, { claimed_host: string; schema_version_applied_at: string }>;
  release_history: { lines: number; released_ids: string[] };
  claim_semantics_evidence: Json;
}

export function deriveSeriesContract(snapDir: string, ctx: SeriesContext): Json {
  const facts = new Map<string, ExportFacts>();
  for (const e of ctx.exports) facts.set(e.label, readExport(snapDir, e.label, e.registry));
  const get = (l: string) => facts.get(l)!;

  return {
    contract_version: SERIES_CONTRACT_VERSION,
    synthetic: true,
    scope:
      "A series of REAL consecutive outputs of ICN's restricted observation exporter, taken from disposable synthetic registries " +
      "whose state was driven through ICN's own lifecycle code. No database or sidecar here was edited after export.",
    tuple: {
      profile: "observation",
      policy: OBSERVATION_POLICY.name,
      format: SNAPSHOT_META_FORMAT,
      registry_schema_version: LATEST_SCHEMA_VERSION,
    },
    clock:
      "Acquisition times and registry row timestamps come from a generator-controlled VIRTUAL clock (the exporter and ICN's lifecycle " +
      "functions run unmodified; only Date and the claimed hostname are substituted, and rows touched by an operation are re-stamped " +
      "with the virtual instant). They are internally coherent but are NOT wall-clock time at generation.",
    ttl: {
      default_minutes: DEFAULT_TTL_MINUTES,
      minimum_minutes: MIN_TTL_MINUTES,
      override_env: "ICN_SESSION_TTL_MINUTES",
      recorded_in_snapshot: false,
      note: "Liveness/staleness is a consumer judgement against a TTL the snapshot does not carry. Without an agreed TTL, staleness is UNKNOWN.",
    },
    registries: ctx.registries as unknown as Json,
    session_labels: ctx.session_labels as unknown as Json,
    operations: ctx.operations,
    release_history: {
      lines_in_session_log: ctx.release_history.lines,
      released_ids: ctx.release_history.released_ids,
      note: "History of a release lives in ops/state/session-log.jsonl OUTSIDE the registry. It is never exported, and the generator did not ship it.",
    },
    claim_semantics_evidence: ctx.claim_semantics_evidence,
    exports: ctx.exports.map((e) => exportSummary(get(e.label))),
    pairs: ctx.pairs.map(([x, y]) => comparePair(get(x), get(y))),
    rules: [
      { id: "R1", rule: "Registry sameness is never derivable from a snapshot. Differing schema_version.applied_at proves 'different'; equal stamps prove nothing. Same-registry must be asserted out of band." },
      { id: "R2", rule: "A later acquisition.started_at is not authenticated freshness. Refuse an export as 'newer' unless its started_at is strictly later; flag any row timestamp after the export's own started_at." },
      { id: "R3", rule: "NULL repo_id / worktree_id / worktree_generation / provider_session_id are UNKNOWN. Never equal to each other, never equal to a value." },
      { id: "R4", rule: "withheld:<hex> tokens carry a per-export random salt. Equality is meaningful only within one snapshot; never compare token values across snapshots." },
      { id: "R5", rule: `For one session id: ${IMMUTABLE_FIELDS.join(", ")} never change. A change means different registry or corruption: refuse.` },
      { id: "R6", rule: "Lane fields (repo_id, worktree_id, worktree_name, worktree_generation) may change on an existing id, only through re-registration of a known conversation: a lane move changes them together; a recreated lane at the same path changes only worktree_generation." },
      { id: "R7", rule: "progress_count never decreases for one id. A new id (release + re-register) restarts at 0." },
      { id: "R8", rule: "A session absent from a later export of the same registry has had its row deleted (release). Absence is not staleness, and 'ended' must not be concluded from absence alone." },
      { id: "R9", rule: "A stale row is not an ended session: ICN never reaps stale rows. Only an explicit release removes them." },
      { id: "R10", rule: "A conversation id appears on at most one row within one export. Across exports of one registry it may map to a NEW session id after release." },
    ],
  };
}
