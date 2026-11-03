// Generates the multi-export CONTINUITY series with ICN's real code.
//
//   cd ops/mcp
//   node --experimental-strip-types --no-warnings \
//        --import ./src/tests/fixtures/ts-resolve-register.mjs \
//        src/tests/fixtures/observation-series/series.ts <empty-output-dir>
//
// Writes <output-dir>/snapshots/{A1,A2,A3,A4,B1}.db (+ .meta.json) and
// <output-dir>/series-contract.json. Node >= 22.6; no build; nothing under dist.
//
// WHAT IS REAL
//   * Every snapshot is a genuine output of exportRegistrySnapshot({profile:"observation"}).
//     No database or sidecar is edited after export, and no sidecar is re-sealed.
//   * Registry state is driven through ICN's own lifecycle functions: registerSession (incl.
//     its resume/dedupe, lane-move and generation-change paths), recordHeartbeat,
//     recordProgress and releaseSession.
//
// WHAT IS CONTROLLED (and therefore documented, not hidden)
//   * A VIRTUAL CLOCK. `Date` is replaced so the exporter's acquisition timestamps and the
//     lifecycle code's `new Date()` are coherent over a simulated hour. SQLite's own
//     `datetime('now')` cannot be replaced, so after each operation the row it touched is
//     re-stamped with the virtual instant — the same technique ICN's own tests use to age rows.
//   * crypto.randomUUID (a counter) so session ids are deterministic labels.
//   * os.hostname() per registry, so the claimed host is fictional.
//   * ICN_ROOT is redirected to a temp directory BEFORE the runtime is imported. The runtime
//     resolves its session log from ICN_ROOT at module load; without this, releaseSession would
//     append fabricated records to the real ops/state/session-log.jsonl of whatever checkout
//     ICN_ROOT points at.
//
// The claim rule is MIRRORED here (claim_files lives inside an MCP handler that cannot run on a
// virtual clock). observation-series-fixture.test.ts pins the mirror against the real handler.

import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from "fs";
import { syncBuiltinESMExports } from "module";
import cryptoMod from "node:crypto";
import os from "node:os";
import { tmpdir } from "os";
import { join, resolve } from "path";

if (!process.argv[2]) throw new Error("usage: series.ts <empty-output-dir>");
const outDir = resolve(process.argv[2]);
mkdirSync(join(outDir, "snapshots"), { recursive: true });
if (existsSync(join(outDir, "series-contract.json"))) throw new Error(`${outDir} already holds a series; refusing to overwrite`);

// ── isolation: must precede the runtime import ───────────────────────────────
const work = mkdtempSync(join(tmpdir(), "icn-series-"));
const icnRoot = join(work, "icn-root");
mkdirSync(join(icnRoot, "ops", "state"), { recursive: true });
process.env["ICN_ROOT"] = icnRoot;
const sessionLog = join(icnRoot, "ops", "state", "session-log.jsonl");

// ── virtual clock, deterministic ids, fictional host ─────────────────────────
const RealDate = Date;
const T0 = RealDate.parse("2026-01-01T10:00:00Z");
let nowMs = T0;
const at = (minute: number): void => {
  nowMs = T0 + minute * 60_000;
};
class VirtualDate extends RealDate {
  constructor(...args: unknown[]) {
    if (args.length === 0) super(nowMs);
    else super(...(args as [number]));
  }
  static now(): number {
    return nowMs;
  }
}
(globalThis as unknown as { Date: unknown }).Date = VirtualDate;
const vt = (ms: number): string => new RealDate(ms).toISOString().slice(0, 19).replace("T", " ");

let uuidN = 0;
(cryptoMod as unknown as { randomUUID: () => string }).randomUUID = () => `00000000-0000-4000-8000-${String(++uuidN).padStart(12, "0")}`;
let currentHost = "series-host-a";
(os as unknown as { hostname: () => string }).hostname = () => currentHost;
syncBuiltinESMExports();

const { default: Database } = await import("better-sqlite3");
const { initDb } = await import("../../../state/db.js");
const rt = await import("../../../runtime/session-runtime.js");
const { exportRegistrySnapshot, verifySnapshotPair } = await import("../../../state/snapshot-export.js");
const model = await import("./series-model.js");
const mirror = await import("./claim-mirror.js");

// ── fictional world ──────────────────────────────────────────────────────────
const ROOT = "/synthetic/series-root";
const REPO = `${ROOT}/repos/icn-series.git`;
const lane = (name: string, gen: string | null) => ({
  repo_id: REPO,
  repo_name: "icn-series",
  worktree_id: `${REPO}/worktrees/${name}`,
  worktree_path: `${ROOT}/worktrees/${name}`,
  worktree_name: name,
  worktree_generation: gen,
});
const G = { alpha1: "00000000-0000-4000-8000-0000000000a1", alpha2: "00000000-0000-4000-8000-0000000000a2", beta1: "00000000-0000-4000-8000-0000000000b1" };
const CONV = {
  c1: "5d1c2a4e-0000-4000-8000-0000000051c1",
  c3: "5d1c2a4e-0000-4000-8000-0000000051c3",
  c5: "5d1c2a4e-0000-4000-8000-0000000051c5",
};
const SECRET_PATH = `${ROOT}/SERIES-CANARY-secret.key`;
const CLAIM = { lib: "icn/crates/series-core/src/lib.rs", notes: "docs/series/notes.md", other: "icn/crates/series-core/src/other.rs" };

// ── registries ───────────────────────────────────────────────────────────────
interface Registry {
  name: string;
  host: string;
  path: string;
  db: InstanceType<typeof Database>;
  pin: InstanceType<typeof Database>;
}
const open: Registry[] = [];
function openRegistry(name: string, host: string, appliedAt: string): Registry {
  const path = join(work, `${name}.db`);
  const db = initDb(path);
  // Fixed per-registry stamps: the one thing in the file that is written once per registry.
  db.exec(`UPDATE schema_version SET applied_at = '${appliedAt}'`);
  db.pragma("wal_autocheckpoint = 0");
  const pin = new Database(path, { readonly: true });
  pin.exec("BEGIN");
  pin.prepare("SELECT COUNT(*) FROM sessions").get();
  const r = { name, host, path, db, pin };
  open.push(r);
  return r;
}

const operations: Array<Record<string, unknown>> = [];
const labels: Record<string, Record<string, string>> = {};
const minute = () => (nowMs - T0) / 60_000;

function stamp(r: Registry, id: string, cols: Record<string, string>): void {
  const keys = Object.keys(cols);
  for (const k of keys) if (!["started_at", "last_heartbeat", "last_progress"].includes(k)) throw new Error(`bad stamp column ${k}`);
  r.db.prepare(`UPDATE sessions SET ${keys.map((k) => `${k} = ?`).join(", ")} WHERE id = ?`).run(...keys.map((k) => cols[k]), id);
}

function register(r: Registry, label: string, o: { conv?: string; lane?: ReturnType<typeof lane>; pid: number | null; parent?: string }): string {
  const idt = o.lane ?? null;
  const res = rt.registerSession(r.db, {
    repo: "icn-series",
    identity: idt,
    branch_state: idt ? { branch: `SERIES-CANARY-branch-${idt.worktree_name}`, head: "SERIES-CANARY-0000000000000000000000000000", detached: false } : null,
    task_description: `SERIES-CANARY-task-${label}`,
    task_ref: `SERIES-CANARY-ref-${label}`,
    pr_ref: `SERIES-CANARY-pr-${label}`,
    parent_session_id: o.parent ?? null,
    provider: "claude",
    agent_pid: o.pid,
    host: r.host,
    provider_session_id: o.conv ?? null,
    transcript_path: `${ROOT}/SERIES-CANARY-transcript-${label}.jsonl`,
  });
  if (res.created) stamp(r, res.session_id, { started_at: vt(nowMs), last_heartbeat: vt(nowMs) });
  else stamp(r, res.session_id, { last_heartbeat: vt(nowMs) });
  labels[r.name] = { ...(labels[r.name] ?? {}), [label]: res.session_id };
  operations.push({
    t_min: minute(),
    registry: r.name,
    op: "register_session",
    as: label,
    session_id: res.session_id,
    created: res.created,
    deduplicated: res.deduplicated,
    lane_changed: res.lane_changed ? { from: res.lane_changed.from, to: res.lane_changed.to } : null,
  });
  return res.session_id;
}

function heartbeat(r: Registry, label: string, id: string): void {
  rt.recordHeartbeat(r.db, id);
  stamp(r, id, { last_heartbeat: vt(nowMs) });
  operations.push({ t_min: minute(), registry: r.name, op: "heartbeat", as: label, session_id: id });
}
function progress(r: Registry, label: string, id: string): void {
  rt.recordProgress(r.db, id, { kind: "file_edit", activity: `SERIES-CANARY-activity-${label}` });
  stamp(r, id, { last_heartbeat: vt(nowMs), last_progress: vt(nowMs) });
  operations.push({ t_min: minute(), registry: r.name, op: "session_progress", as: label, session_id: id });
}
function release(r: Registry, label: string, id: string): void {
  const res = rt.releaseSession(r.db, id, { reason: "synthetic-series" });
  operations.push({ t_min: minute(), registry: r.name, op: "release_session", as: label, session_id: id, released: res.released, dropped_file_claims: res.dropped.file_claims });
}

/** Mirror of the claim_files handler's rule (claim-mirror.ts), against the virtual clock. */
const TTL_MIN = rt.ttlMinutes({} as NodeJS.ProcessEnv);
function claim(r: Registry, label: string, id: string, files: string[]): void {
  const { claimed, conflicts } = mirror.mirrorClaim(r.db, id, files, vt(nowMs), TTL_MIN);
  operations.push({
    t_min: minute(),
    registry: r.name,
    op: "claim_files",
    as: label,
    session_id: id,
    claimed: claimed.map((f) => (f === SECRET_PATH ? "<withheld-path>" : f)),
    refused_because_holder_is_live: conflicts.map((f) => (f === SECRET_PATH ? "<withheld-path>" : f)),
  });
}

function plantSensitiveRows(r: Registry): void {
  const d = r.db;
  d.prepare("INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES ('x','y','SERIES-CANARY-kind',?,1)").run(JSON.stringify({ b: "SERIES-CANARY-mailbox" }));
  d.prepare("INSERT INTO events (scope, type, payload, created_at) VALUES ('SERIES-CANARY-scope','t',?,1)").run(JSON.stringify({ b: "SERIES-CANARY-event" }));
  d.prepare("INSERT INTO watchers_process (session_id, pid, label, created_at) VALUES ('x', 9, 'SERIES-CANARY-watcher', 1)").run();
  d.prepare("INSERT INTO health_cache (key, value) VALUES ('svc', ?)").run(JSON.stringify({ u: "SERIES-CANARY-health" }));
  d.prepare("INSERT INTO decision_index (id, title, tags, file_path) VALUES ('d1','SERIES-CANARY-decision','t','docs/adr/none.md')").run();
}

// ── export ───────────────────────────────────────────────────────────────────
const exported: Array<{ label: string; registry: string }> = [];
function exportAt(r: Registry, label: string, atMinute: number): void {
  at(atMinute);
  currentHost = r.host;
  syncBuiltinESMExports();
  utimesSync(r.path, nowMs / 1000, nowMs / 1000);
  if (statSync(`${r.path}-wal`).size === 0) throw new Error("fixture invariant: committed rows must still be in the WAL");
  const dest = join(outDir, "snapshots", `${label}.db`);
  exportRegistrySnapshot({
    profile: "observation",
    source: r.path,
    dest,
    // Moves the virtual clock so finished_at > started_at, exactly as real elapsed time would.
    hooks: { afterCopy: () => void (nowMs += 1500) },
  });
  const v = verifySnapshotPair(dest);
  if (!v.ok) throw new Error(`${label} does not verify: ${v.problems.join("; ")}`);
  exported.push({ label, registry: r.name });
}

try {
  // ═════════════════ REGISTRY A (host a) ═════════════════
  const A = openRegistry("A", "series-host-a", "2026-01-01 08:00:00");
  plantSensitiveRows(A);

  // --- up to export A1 (+10 min) ---
  at(1);
  const sa = register(A, "s-a", { conv: CONV.c1, lane: lane("lane-alpha", G.alpha1), pid: 41001 });
  at(2);
  const sb = register(A, "s-b", { lane: lane("lane-alpha", G.alpha1), pid: 41002 }); // NULL conversation id
  at(3);
  const se = register(A, "s-e", { conv: CONV.c5, lane: lane("lane-alpha", G.alpha1), pid: 41005 });
  register(A, "s-l", { pid: null }); // no identity at all: NULL repo_id/worktree_id/generation/conversation
  for (const m of [4, 6, 8]) {
    at(m);
    progress(A, "s-a", sa);
  }
  at(5);
  heartbeat(A, "s-b", sb);
  at(6);
  claim(A, "s-a", sa, [CLAIM.lib]);
  claim(A, "s-b", sb, [CLAIM.notes, SECRET_PATH]);
  at(9);
  heartbeat(A, "s-e", se);
  exportAt(A, "A1", 10);

  // --- up to export A2 (+25 min) ---
  at(12);
  const sc = register(A, "s-c", { conv: CONV.c3, lane: lane("lane-beta", G.beta1), pid: 41003 }); // a NEW activation appears
  at(15);
  claim(A, "s-c", sc, [CLAIM.other]);
  for (const m of [16, 18]) {
    at(m);
    progress(A, "s-a", sa);
  }
  at(20);
  heartbeat(A, "s-e", se);
  exportAt(A, "A2", 25);

  // --- registry B (a different instance, host and clock reading) ---
  const B = openRegistry("B", "series-host-b", "2026-01-02 09:30:00");
  plantSensitiveRows(B);
  at(26);
  // The SAME conversation id and the SAME lane path string as registry A, on another host.
  const b1 = register(B, "b-1", { conv: CONV.c1, lane: lane("lane-alpha", G.alpha1), pid: 52001 });
  at(27);
  const b2 = register(B, "b-2", { lane: lane("lane-alpha", G.alpha1), pid: 52002 });
  at(28);
  claim(B, "b-1", b1, [CLAIM.lib]);
  claim(B, "b-2", b2, [SECRET_PATH]);
  at(29);
  progress(B, "b-1", b1);
  exportAt(B, "B1", 30);

  // --- up to export A3 (+70 min) ---
  // s-b and s-l are never heard from again: they go STALE (age > TTL) but ICN keeps their rows.
  at(45);
  heartbeat(A, "s-a", sa); // keeps s-a live (age < TTL) when s-d tries to claim its file
  at(50);
  release(A, "s-c", sc); // row deleted; its claim on other.rs is dropped with it
  at(52);
  const sd = register(A, "s-d", { conv: CONV.c3, lane: lane("lane-beta", G.beta1), pid: 41006 }); // same conversation, NEW activation
  at(54);
  claim(A, "s-d", sd, [CLAIM.lib]); // refused: s-a is live
  at(55);
  claim(A, "s-d", sd, [CLAIM.notes, SECRET_PATH]); // granted: holder s-b is stale -> a real ownership conflict
  at(58);
  register(A, "s-e", { conv: CONV.c5, lane: lane("lane-beta", G.beta1), pid: 41005 }); // resumed from ANOTHER lane: row moves
  at(62);
  register(A, "s-a", { conv: CONV.c1, lane: lane("lane-alpha", G.alpha2), pid: 41001 }); // lane recreated at the same path: new generation
  at(68);
  progress(A, "s-d", sd);
  exportAt(A, "A3", 70);

  // --- adversarial: the clock steps BACK, then the unchanged registry is exported again ---
  exportAt(A, "A4", 50);

  // ═════════════════ derive the contract from the REAL outputs ═════════════════
  const history = existsSync(sessionLog) ? readFileSync(sessionLog, "utf-8").split("\n").filter(Boolean) : [];
  const releasedIds = history.map((l) => String((JSON.parse(l) as { id: string }).id)).sort();
  const contract = model.deriveSeriesContract(join(outDir, "snapshots"), {
    exports: exported,
    pairs: [
      ["A1", "A2"],
      ["A2", "A3"],
      ["A3", "A4"],
      ["A2", "B1"],
      ["B1", "A3"],
    ],
    operations: operations as never,
    session_labels: labels,
    registries: {
      A: { claimed_host: "series-host-a", schema_version_applied_at: "2026-01-01 08:00:00" },
      B: { claimed_host: "series-host-b", schema_version_applied_at: "2026-01-02 09:30:00" },
    },
    release_history: { lines: history.length, released_ids: releasedIds },
    claim_semantics_evidence: {
      refused_when_holder_is_live: "s-d claiming lib.rs while s-a (age 25 min < TTL) held it was refused; see operations",
      granted_when_holder_is_stale: "s-d claiming notes.md and a withheld path while s-b (age 50 min >= TTL) held them was granted, so both rows coexist",
      removal: "no tool removes a single claim; claims leave the registry only when their session is released",
    } as never,
  });
  writeFileSync(join(outDir, "series-contract.json"), JSON.stringify(contract, null, 2) + "\n");
  process.stdout.write(JSON.stringify({ ok: true, outDir, exports: exported.map((e) => e.label), release_log_lines: history.length }) + "\n");
} finally {
  for (const r of open) {
    try {
      r.pin.exec("COMMIT");
      r.pin.close();
    } catch {
      /* best effort */
    }
    try {
      r.db.close();
    } catch {
      /* best effort */
    }
  }
  rmSync(work, { recursive: true, force: true });
}
