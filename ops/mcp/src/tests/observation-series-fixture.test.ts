// Multi-export continuity fixture: five REAL consecutive outputs of the restricted observation
// exporter (registry A x4, registry B x1), their contract, the offline verifier and the package.
//
// Four kinds of evidence live here:
//   1. the series is generated through the real producer and every export is a valid pair;
//   2. the lifecycle claims in the handoff are pinned against ICN's REAL runtime and REAL
//      claim_files handler (so the documentation cannot drift from the code it describes);
//   3. the expectations are hand-written literals, independent of the generator's own model;
//   4. the package rejects tampering, incomplete exports and unsafe archives.
//
// Nothing here opens a real registry. Generation runs in a child process with ICN_ROOT pointed at
// THIS checkout on purpose: the generator must redirect it, and a test proves the checkout's real
// ops/state/session-log.jsonl is untouched.

import { spawnSync } from "child_process";
import { createHash } from "crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, unlinkSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import Database from "better-sqlite3";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import * as rt from "../runtime/session-runtime.js";
import { verifySnapshotPair } from "../state/snapshot-export.js";
import { canariesInFile } from "./fixtures/snapshot-helpers.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const FX = join(HERE, "fixtures/observation-series");
const REPO = join(HERE, "../../../..");
const REF = join(FX, "reference/series-contract.json");
const LABELS = ["A1", "A2", "B1", "A3", "A4"] as const;

const [NODE_MAJOR, NODE_MINOR] = process.versions.node.split(".").map(Number) as [number, number];
const CAN_STRIP_TYPES = NODE_MAJOR > 22 || (NODE_MAJOR === 22 && NODE_MINOR >= 6);
const HAS_PY = spawnSync("python3", ["--version"]).status === 0;
// Visible skip: the generator needs Node >= 22.6 (type stripping) and the verifier needs python3.
const suite = describe.skipIf(!CAN_STRIP_TYPES || !HAS_PY);

const sha = (s: string | Buffer) => createHash("sha256").update(s).digest("hex");
const dig = (raw: string) => `sha256:${sha(raw)}`;

// Fictional world, mirrored by hand from series.ts (these are the independent literals).
const REPO_ID = "/synthetic/series-root/repos/icn-series.git";
const LANE_ALPHA = `${REPO_ID}/worktrees/lane-alpha`;
const LANE_BETA = `${REPO_ID}/worktrees/lane-beta`;
const id = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
const S = { a: id(1), b: id(2), e: id(3), l: id(4), c: id(5), b1: id(6), b2: id(7), d: id(8) };
const GEN = { alpha1: "00000000-0000-4000-8000-0000000000a1", alpha2: "00000000-0000-4000-8000-0000000000a2", beta1: "00000000-0000-4000-8000-0000000000b1" };
const CONV = { c1: "5d1c2a4e-0000-4000-8000-0000000051c1", c3: "5d1c2a4e-0000-4000-8000-0000000051c3", c5: "5d1c2a4e-0000-4000-8000-0000000051c5" };

let gen: string; // generator output
let pkgOut: string; // builder output
let pkgDir: string; // package directory
let archive: string;
let ext: string; // archive extracted into a fresh directory
let tmp: string;
let logBefore: string | null;
const sessionLog = join(REPO, "ops/state/session-log.jsonl");
const logFingerprint = () => (existsSync(sessionLog) ? `${statSync(sessionLog).size}:${statSync(sessionLog).mtimeMs}:${sha(readFileSync(sessionLog))}` : null);

function py(args: string[], opts: { cwd?: string } = {}) {
  return spawnSync("python3", ["-I", ...args], { encoding: "utf-8", timeout: 90_000, ...opts });
}
function runSeries(out: string) {
  return spawnSync(
    process.execPath,
    ["--experimental-strip-types", "--no-warnings", "--import", join(HERE, "fixtures/ts-resolve-register.mjs"), join(FX, "series.ts"), out],
    // HOSTILE ambient ICN_ROOT: the real checkout. The generator must redirect it.
    { encoding: "utf-8", timeout: 120_000, cwd: join(HERE, "../.."), env: { ...process.env, ICN_ROOT: REPO } }
  );
}
function build(out: string) {
  const r = py([join(FX, "build_package.py"), "--series-dir", gen, "--repo", REPO, "--out", out, "--use-working-tree"]);
  expect(r.status, r.stderr + r.stdout).toBe(0);
  return JSON.parse(r.stdout) as { package_dir: string; archive: string; sha256: string; bytes: number };
}
const verify = (dir: string) => py([join(dir, "verify_series.py")], { cwd: dir });

beforeAll(() => {
  if (!CAN_STRIP_TYPES || !HAS_PY) return;
  tmp = mkdtempSync(join(tmpdir(), "icn-series-test-"));
  logBefore = logFingerprint();
  gen = join(tmp, "gen");
  const r = runSeries(gen);
  expect(r.status, r.stderr + r.stdout).toBe(0);
  pkgOut = join(tmp, "pkg");
  const b = build(pkgOut);
  pkgDir = b.package_dir;
  archive = b.archive;
  ext = join(tmp, "extracted");
  mkdirSync(ext);
  const x = spawnSync("tar", ["-xzf", archive, "-C", ext, "--no-same-owner", "--keep-old-files"], { encoding: "utf-8" });
  expect(x.status, x.stderr).toBe(0);
}, 240_000);
afterAll(() => {
  if (tmp) rmSync(tmp, { recursive: true, force: true });
});

const snap = (l: string) => join(gen, "snapshots", `${l}.db`);
function rows(label: string, sql: string, p?: string): Array<Record<string, unknown>> {
  const d = new Database(p ?? snap(label), { readonly: true });
  try {
    return d.prepare(sql).all() as Array<Record<string, unknown>>;
  } finally {
    d.close();
  }
}
const sess = (label: string, sid: string) => rows(label, `SELECT * FROM sessions WHERE id = '${sid}'`)[0];
const ids = (label: string) => rows(label, "SELECT id FROM sessions ORDER BY id").map((r) => r["id"]);
const claimsOf = (label: string) => rows(label, "SELECT session_id, file_path FROM file_claims ORDER BY file_path, session_id").map((r) => `${r["session_id"]}|${r["file_path"]}`);

// ───────────────────────────────────────────────────────────────────────────────
suite("generation through the real producer", () => {
  it("every export is a valid completion-marker pair (the producer's own check)", () => {
    for (const l of LABELS) expect(verifySnapshotPair(snap(l)), l).toEqual({ ok: true, profile: "observation", problems: [] });
  });

  it("series-contract.json equals the committed reference byte for byte", () => {
    expect(readFileSync(join(gen, "series-contract.json"), "utf-8")).toBe(readFileSync(REF, "utf-8"));
  });

  it("each sidecar binds the exact length and SHA-256 of its own database", () => {
    for (const l of LABELS) {
      const m = JSON.parse(readFileSync(`${snap(l)}.meta.json`, "utf-8"));
      expect(m.snapshot.file, l).toBe(`${l}.db`);
      expect(m.snapshot.bytes, l).toBe(statSync(snap(l)).size);
      expect(m.snapshot.sha256, l).toBe(sha(readFileSync(snap(l))));
    }
  });

  it("records the acquisition times the exporter actually emitted (virtual clock), finished after started", () => {
    const want: Record<string, string> = {
      A1: "2026-01-01T10:10:00.000Z",
      A2: "2026-01-01T10:25:00.000Z",
      B1: "2026-01-01T10:30:00.000Z",
      A3: "2026-01-01T11:10:00.000Z",
      A4: "2026-01-01T10:50:00.000Z",
    };
    for (const l of LABELS) {
      const a = JSON.parse(readFileSync(`${snap(l)}.meta.json`, "utf-8")).acquisition;
      expect(a.started_at, l).toBe(want[l]);
      expect(Date.parse(a.finished_at) - Date.parse(a.started_at), l).toBe(1500);
      expect(a.method).toBe("sqlite-read-transaction-projection");
    }
  });

  it("registry A and B have different claimed hosts and different registry stamps; all A exports share both", () => {
    const meta = (l: string) => JSON.parse(readFileSync(`${snap(l)}.meta.json`, "utf-8"));
    for (const l of ["A1", "A2", "A3", "A4"]) {
      expect(meta(l).host.claimed.hostname).toBe("series-host-a");
      expect(rows(l, "SELECT DISTINCT applied_at FROM schema_version")).toEqual([{ applied_at: "2026-01-01 08:00:00" }]);
    }
    expect(meta("B1").host.claimed.hostname).toBe("series-host-b");
    expect(rows("B1", "SELECT DISTINCT applied_at FROM schema_version")).toEqual([{ applied_at: "2026-01-02 09:30:00" }]);
    for (const l of LABELS) expect(meta(l).host).toMatchObject({ authenticated: null, authenticity: "none" });
  });

  it("all five exports carry the same producer, format, profile and policy", () => {
    const tuples = new Set(
      LABELS.map((l) => {
        const m = JSON.parse(readFileSync(`${snap(l)}.meta.json`, "utf-8"));
        return JSON.stringify([m.format, m.profile, m.disclosure.policy, m.snapshot.schema_version_max, m.producer]);
      })
    );
    expect(tuples.size).toBe(1);
  });

  it("the ambient ICN_ROOT pointed at this checkout, yet its real session log was not touched", () => {
    expect(logFingerprint()).toBe(logBefore);
  });
});

// ───────────────────────────────────────────────────────────────────────────────
suite("sensitive fields are restricted identically across the whole sequence", () => {
  it("scanner self-check: it finds a canary when one is present", () => {
    const f = join(tmp, "positive.bin");
    writeFileSync(f, Buffer.concat([Buffer.from([0, 1]), Buffer.from("xxSERIES-CANARY-yy"), Buffer.from([255])]));
    expect(canariesInFile(f, ["SERIES-CANARY"])).toEqual(["SERIES-CANARY"]);
  });

  it.each(LABELS)("%s: only the three observation tables, withheld columns NULL, no canary or synthetic path in any byte", (l) => {
    expect(rows(l, "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")).toEqual([{ name: "file_claims" }, { name: "schema_version" }, { name: "sessions" }]);
    for (const c of ["worktree", "worktree_path", "branch_at_registration", "head_at_registration", "task_description", "task_ref", "pr_ref", "transcript_path", "current_activity"]) {
      expect(rows(l, `SELECT COUNT(*) AS n FROM sessions WHERE ${c} IS NOT NULL`), `${l}.${c}`).toEqual([{ n: 0 }]);
    }
    const needles = ["SERIES-CANARY", "/synthetic/series-root", "/home/", "/tmp/", "icn-dev", "10.8."];
    expect(canariesInFile(snap(l), needles)).toEqual([]);
    expect(canariesInFile(`${snap(l)}.meta.json`, needles)).toEqual([]);
  });

  it("every non-NULL repo_id / worktree_id is the digest of a fictional path, never a path", () => {
    for (const l of LABELS) {
      const ok = new Set([dig(REPO_ID), dig(LANE_ALPHA), dig(LANE_BETA)]);
      for (const r of rows(l, "SELECT repo_id AS v FROM sessions WHERE repo_id IS NOT NULL UNION ALL SELECT worktree_id FROM sessions WHERE worktree_id IS NOT NULL")) {
        expect(ok.has(String(r["v"])), `${l} ${r["v"]}`).toBe(true);
      }
    }
  });

  it("the source registries held mailbox, event, watcher, health and decision rows that never appear", () => {
    // The generator planted SERIES-CANARY in all of them; their absence is already shown by the
    // byte scans above. This pins that the planted tables are really ABSENT, not empty.
    for (const l of LABELS) {
      expect(rows(l, "SELECT name FROM sqlite_master WHERE name IN ('mailbox','events','watchers_process','health_cache','decision_index')")).toEqual([]);
    }
  });
});

// ───────────────────────────────────────────────────────────────────────────────
suite("legitimate lifecycle transitions, as literals", () => {
  it("A1: four registered activations, NULL identities present, one claim withheld", () => {
    expect(ids("A1")).toEqual([S.a, S.b, S.e, S.l]);
    expect(sess("A1", S.a)).toMatchObject({ provider_session_id: CONV.c1, worktree_id: dig(LANE_ALPHA), worktree_generation: GEN.alpha1, progress_count: 3, provider: "claude", host: "series-host-a" });
    expect(sess("A1", S.b)).toMatchObject({ provider_session_id: null, worktree_id: dig(LANE_ALPHA) }); // NULL conversation id
    expect(sess("A1", S.l)).toMatchObject({ repo_id: null, worktree_id: null, worktree_generation: null, provider_session_id: null }); // NULL identity
    expect(claimsOf("A1").filter((c) => c.includes("withheld:"))).toHaveLength(1);
  });

  it("A1 -> A2: a new activation appears; heartbeat and progress advance on the same id", () => {
    expect(ids("A2")).toEqual([S.a, S.b, S.e, S.l, S.c]);
    expect(sess("A2", S.c)).toMatchObject({ provider_session_id: CONV.c3, worktree_id: dig(LANE_BETA), worktree_generation: GEN.beta1, progress_count: 0 });
    expect(sess("A2", S.a)).toMatchObject({ progress_count: 5, last_progress: "2026-01-01 10:18:00", last_heartbeat: "2026-01-01 10:18:00" });
    expect(sess("A2", S.e)).toMatchObject({ last_heartbeat: "2026-01-01 10:20:00", progress_count: 0 });
    expect(claimsOf("A2").filter((c) => c.endsWith("other.rs"))).toEqual([`${S.c}|icn/crates/series-core/src/other.rs`]);
  });

  it("A2 -> A3: a released session is ABSENT, its claim gone, and its conversation returns under a NEW id", () => {
    expect(ids("A3")).toEqual([S.a, S.b, S.e, S.l, S.d]);
    expect(ids("A3")).not.toContain(S.c);
    expect(sess("A3", S.d)).toMatchObject({ provider_session_id: CONV.c3, progress_count: 1, worktree_id: dig(LANE_BETA) });
    expect(S.d).not.toBe(S.c);
    expect(claimsOf("A3").some((c) => c.startsWith(`${S.c}|`))).toBe(false);
  });

  it("A2 -> A3: a lane recreated at the same path changes ONLY worktree_generation, on the same id", () => {
    const a2 = sess("A2", S.a)!;
    const a3 = sess("A3", S.a)!;
    expect(a3["worktree_generation"]).toBe(GEN.alpha2);
    expect(a2["worktree_generation"]).toBe(GEN.alpha1);
    for (const f of ["repo_id", "worktree_id", "worktree_name", "provider", "provider_session_id", "host", "started_at"]) expect(a3[f], f).toBe(a2[f]);
  });

  it("A2 -> A3: a resumed conversation from another lane MOVES the row (lane columns change together, same id)", () => {
    const a2 = sess("A2", S.e)!;
    const a3 = sess("A3", S.e)!;
    expect([a2["worktree_id"], a2["worktree_name"], a2["worktree_generation"]]).toEqual([dig(LANE_ALPHA), "lane-alpha", GEN.alpha1]);
    expect([a3["worktree_id"], a3["worktree_name"], a3["worktree_generation"]]).toEqual([dig(LANE_BETA), "lane-beta", GEN.beta1]);
    expect(a3["repo_id"]).toBe(a2["repo_id"]); // same repository
    for (const f of ["provider", "provider_session_id", "host", "started_at"]) expect(a3[f], f).toBe(a2[f]);
  });

  it("A3: stale rows persist - a stale session is NOT an ended session", () => {
    expect(ids("A3")).toEqual(expect.arrayContaining([S.b, S.l]));
    const a3 = JSON.parse(readFileSync(`${snap("A3")}.meta.json`, "utf-8")).acquisition.started_at as string;
    const age = (sid: string) => Math.floor((Date.parse(a3) - Date.parse(`${(sess("A3", sid)!["last_heartbeat"] as string).replace(" ", "T")}Z`)) / 60000);
    expect(age(S.b)).toBe(65);
    expect(age(S.l)).toBe(67);
    expect(age(S.b)).toBeGreaterThanOrEqual(rt.DEFAULT_TTL_MINUTES);
  });

  it("A3: a real ownership conflict coexists - the stale holder keeps its row beside the live claimant", () => {
    const notes = claimsOf("A3").filter((c) => c.endsWith("|docs/series/notes.md"));
    expect(notes.sort()).toEqual([`${S.b}|docs/series/notes.md`, `${S.d}|docs/series/notes.md`].sort());
    const tok = rows("A3", "SELECT session_id, file_path FROM file_claims WHERE file_path LIKE 'withheld:%'");
    expect(tok.map((r) => r["session_id"]).sort()).toEqual([S.b, S.d].sort());
    expect(tok[0]!["file_path"]).toBe(tok[1]!["file_path"]); // same path => same token WITHIN this snapshot
  });

  it("A3: the live holder's claim was refused to the newcomer (it is still the only holder of lib.rs)", () => {
    expect(claimsOf("A3").filter((c) => c.endsWith("|icn/crates/series-core/src/lib.rs"))).toEqual([`${S.a}|icn/crates/series-core/src/lib.rs`]);
  });

  it("claim removals happen only with a release: every A1/A2 claim still exists in A3 except the released session's", () => {
    const a2 = claimsOf("A2").filter((c) => !c.includes("withheld:"));
    const a3 = new Set(claimsOf("A3"));
    expect(a2.filter((c) => !a3.has(c))).toEqual([`${S.c}|icn/crates/series-core/src/other.rs`]);
  });

  it("A4 is an unchanged re-export of A3 taken with the clock stepped back", () => {
    expect(ids("A4")).toEqual(ids("A3"));
    for (const sid of ids("A3") as string[]) expect(sess("A4", sid)).toEqual(sess("A3", sid));
    const t = (l: string) => Date.parse(JSON.parse(readFileSync(`${snap(l)}.meta.json`, "utf-8")).acquisition.started_at);
    expect(t("A4")).toBeLessThan(t("A3"));
    const late = rows("A4", "SELECT id FROM sessions WHERE last_heartbeat > '2026-01-01 10:50:00'").map((r) => r["id"]);
    expect(late.sort()).toEqual([S.a, S.e, S.d].sort()); // rows stamped AFTER the export's own acquisition
  });

  it("NULL identities are never equal: the three NULL-conversation rows and the NULL-lane row share nothing", () => {
    const nulls = rows("A3", "SELECT id FROM sessions WHERE provider_session_id IS NULL ORDER BY id").map((r) => r["id"]);
    expect(nulls).toEqual([S.b, S.l]);
    expect(rows("A3", "SELECT COUNT(*) AS n FROM sessions WHERE worktree_id IS NULL")).toEqual([{ n: 1 }]);
    // A consumer joining on provider_session_id must not pair these two NULLs.
    expect(rows("A3", "SELECT COUNT(*) AS n FROM sessions x JOIN sessions y ON x.provider_session_id = y.provider_session_id AND x.id < y.id")).toEqual([{ n: 0 }]);
  });
});

// ───────────────────────────────────────────────────────────────────────────────
suite("ICN's invariants hold across every pair of real consecutive exports", () => {
  const consecutive: Array<[string, string]> = [["A1", "A2"], ["A2", "A3"]];
  const IMMUTABLE = ["repo", "provider", "provider_session_id", "parent_session_id", "host", "started_at"];

  it.each(consecutive)("%s -> %s: immutable fields never change for an id present in both; progress_count never decreases", (x, y) => {
    for (const sid of ids(x) as string[]) {
      if (!(ids(y) as string[]).includes(sid)) continue;
      const a = sess(x, sid)!;
      const b = sess(y, sid)!;
      for (const f of IMMUTABLE) expect(b[f], `${sid}.${f}`).toBe(a[f]);
      expect(Number(b["progress_count"])).toBeGreaterThanOrEqual(Number(a["progress_count"]));
    }
  });

  it.each(consecutive)("%s -> %s: lane fields change only as a lane move (all together) or as a generation-only change", (x, y) => {
    for (const sid of ids(x) as string[]) {
      if (!(ids(y) as string[]).includes(sid)) continue;
      const a = sess(x, sid)!;
      const b = sess(y, sid)!;
      if (a["worktree_id"] !== b["worktree_id"]) {
        expect(b["worktree_name"], "a lane move changes the name too").not.toBe(a["worktree_name"]);
        expect(b["worktree_generation"], "and the generation").not.toBe(a["worktree_generation"]);
      } else {
        expect(b["worktree_name"]).toBe(a["worktree_name"]);
        expect(b["repo_id"]).toBe(a["repo_id"]);
      }
    }
  });

  it("a conversation id is on at most one row in every export", () => {
    for (const l of LABELS) {
      expect(rows(l, "SELECT provider_session_id AS c, COUNT(*) AS n FROM sessions WHERE provider_session_id IS NOT NULL GROUP BY c HAVING n > 1")).toEqual([]);
    }
  });

  it("session ids are unique across the two registries' histories except by design (no id is reused for a new activation)", () => {
    const a = new Set([...ids("A1"), ...ids("A2"), ...ids("A3")] as string[]);
    const b = new Set(ids("B1") as string[]);
    expect([...a].filter((x) => b.has(x))).toEqual([]);
    expect(S.c).not.toBe(S.d);
  });
});

// ───────────────────────────────────────────────────────────────────────────────
suite("no identity is inferred across registries, clocks or claim tokens", () => {
  it("registry A and B share a conversation id AND lane digests (coincidence a naive joiner would call identity)", () => {
    const conv = (l: string) => new Set(rows(l, "SELECT provider_session_id AS c FROM sessions WHERE provider_session_id IS NOT NULL").map((r) => r["c"]));
    const lanes = (l: string) => new Set(rows(l, "SELECT worktree_id AS w FROM sessions WHERE worktree_id IS NOT NULL").map((r) => r["w"]));
    expect([...conv("A2")].filter((c) => conv("B1").has(c))).toEqual([CONV.c1]);
    expect([...lanes("A2")].filter((w) => lanes("B1").has(w))).toEqual([dig(LANE_ALPHA)]);
    // ... yet the ONLY registry-scoped value differs.
    expect(rows("A2", "SELECT applied_at FROM schema_version WHERE version = 1")).not.toEqual(rows("B1", "SELECT applied_at FROM schema_version WHERE version = 1"));
  });

  it("the committed contract refuses A<->B as different registries and A3->A4 as non-successor", () => {
    const c = JSON.parse(readFileSync(REF, "utf-8")) as { pairs: Array<{ from: string; to: string; verdict: string; shared_with_other_registry: { note: string } | null }> };
    const v = (f: string, t: string) => c.pairs.find((p) => p.from === f && p.to === t)!;
    expect(v("A2", "B1").verdict).toBe("refused_different_registry");
    expect(v("B1", "A3").verdict).toBe("refused_different_registry");
    expect(v("A3", "A4").verdict).toBe("refused_not_later");
    expect(v("A1", "A2").verdict).toBe("comparable_if_same_registry_asserted");
    expect(v("A2", "B1").shared_with_other_registry!.note).toMatch(/COINCIDENCE/);
  });

  it("no sidecar field is, or looks like, a registry/instance identifier", () => {
    for (const l of LABELS) {
      const m = JSON.parse(readFileSync(`${snap(l)}.meta.json`, "utf-8"));
      expect(Object.keys(m.source).sort(), l).toEqual(["journal_mode", "mtime_ms_at_start", "schema_version_max", "size_bytes_at_start", "user_version"]);
      expect(JSON.stringify(m)).not.toMatch(/instance|registry_id|uuid|identity_sha256/i);
    }
  });

  it("the SAME withheld path is claimed in A1..A4, yet its token differs in every export", () => {
    const tok = (l: string) => rows(l, "SELECT DISTINCT file_path AS t FROM file_claims WHERE file_path LIKE 'withheld:%'").map((r) => String(r["t"]));
    const per = ["A1", "A2", "A3", "A4"].map(tok);
    expect(per.map((t) => t.length)).toEqual([1, 1, 1, 1]); // exactly one distinct withheld path each time
    expect(new Set(per.flat()).size).toBe(4); // four exports, four different tokens
  });

  it("no token value appears in more than one snapshot (so none can be a stable path identifier)", () => {
    const seen = new Map<string, string[]>();
    for (const l of LABELS) for (const r of rows(l, "SELECT DISTINCT file_path AS t FROM file_claims WHERE file_path LIKE 'withheld:%'")) seen.set(String(r["t"]), [...(seen.get(String(r["t"])) ?? []), l]);
    expect([...seen.values()].filter((v) => v.length > 1)).toEqual([]);
  });

  it("acquisition timestamps are a sampled clock: A4 is later in the series but EARLIER in time than A3", () => {
    const t = (l: string) => Date.parse(JSON.parse(readFileSync(`${snap(l)}.meta.json`, "utf-8")).acquisition.started_at);
    expect(t("A4")).toBeLessThan(t("A3"));
    expect(t("B1")).toBeGreaterThan(t("A2"));
  });
});

// ───────────────────────────────────────────────────────────────────────────────
suite("the package: independent verification, tampering and incomplete exports", () => {
  it("the unpacked archive passes the offline verifier (every check)", () => {
    const r = verify(join(ext, readdirSync(ext)[0]!));
    expect(r.status, r.stdout.split("\n").filter((l) => l.startsWith("FAIL")).join("\n")).toBe(0);
    expect(r.stdout).toMatch(/SERIES VERIFIED: profile=observation policy=observation-v1/);
    expect(r.stdout.split("\n").filter((l) => l.startsWith("PASS")).length).toBeGreaterThan(150);
  });

  it("the archive is audited clean, and the manifest records an unpushed commit with a truthful tree state", () => {
    const top = readdirSync(ext)[0]!;
    const a = py([join(FX, "audit_archive.py"), archive, top]);
    expect(a.status, a.stdout).toBe(0);
    const man = JSON.parse(readFileSync(join(pkgDir, "MANIFEST.json"), "utf-8"));
    expect(man.producer).toMatchObject({ pushed: false });
    expect(typeof man.producer.git_tree_clean).toBe("boolean");
    expect(man.not_included).toEqual(expect.arrayContaining(["the full recovery profile", "any source registry", "the release session-log"]));
  });

  it("rebuilding from the same snapshots yields a byte-identical archive", () => {
    const again = build(join(tmp, "pkg-again"));
    expect(again.sha256).toBe(sha(readFileSync(archive)));
  });

  it("the package carries no full-fidelity data: no mailbox/event/session-log content anywhere", () => {
    const walk = (d: string): string[] => readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(d, e.name)) : [join(d, e.name)]));
    // verify_series.py names the canary in its own scan list; every DATA file must be free of it.
    for (const f of walk(pkgDir).filter((x) => !x.endsWith("verify_series.py"))) expect(canariesInFile(f, ["SERIES-CANARY"]), f).toEqual([]);
  });

  // --- tampering ----------------------------------------------------------------
  function refresh(dir: string) {
    for (const l of LABELS) {
      const db = join(dir, "snapshots", `${l}.db`);
      const mp = `${db}.meta.json`;
      if (!existsSync(db) || !existsSync(mp)) continue;
      const m = JSON.parse(readFileSync(mp, "utf-8"));
      m.snapshot.sha256 = sha(readFileSync(db));
      m.snapshot.bytes = statSync(db).size;
      writeFileSync(mp, JSON.stringify(m, null, 2) + "\n");
    }
    const man = JSON.parse(readFileSync(join(dir, "MANIFEST.json"), "utf-8"));
    for (const f of man.files) {
      const p = join(dir, ...f.path.split("/"));
      if (existsSync(p) && !statSync(p).isSymbolicLink()) {
        f.sha256 = sha(readFileSync(p));
        f.bytes = statSync(p).size;
      }
    }
    writeFileSync(join(dir, "MANIFEST.json"), JSON.stringify(man, null, 2) + "\n");
    const all: string[] = [];
    const walk = (d: string, pre: string) => {
      for (const e of readdirSync(d, { withFileTypes: true })) {
        if (e.isDirectory()) walk(join(d, e.name), `${pre}${e.name}/`);
        else if (e.isFile() && `${pre}${e.name}` !== "SHA256SUMS") all.push(`${pre}${e.name}`);
      }
    };
    walk(dir, "");
    writeFileSync(join(dir, "SHA256SUMS"), all.sort().map((r) => `${sha(readFileSync(join(dir, ...r.split("/"))))}  ${r}`).join("\n") + "\n");
  }
  const cases: Array<[string, (d: string) => void, RegExp | null]> = [
    ["a flipped byte in A2.db (hashes NOT refreshed)", (d) => { const p = join(d, "snapshots/A2.db"); const b = readFileSync(p); b[b.length - 1] = b[b.length - 1]! ^ 0xff; writeFileSync(p, b); }, /SHA256SUMS: every hash matches/],
    ["an INCOMPLETE export: A3.db without its completion marker", (d) => unlinkSync(join(d, "snapshots/A3.db.meta.json")), /exactly the expected files/],
    ["a marker with no database: A3.db removed", (d) => unlinkSync(join(d, "snapshots/A3.db")), /exactly the expected files/],
    ["a stray -wal sibling", (d) => writeFileSync(join(d, "snapshots/A1.db-wal"), "x"), /exactly the expected files/],
    ["a symlink in place of a reference file", (d) => { unlinkSync(join(d, "reference/contract.json")); symlinkSync("/etc/hostname", join(d, "reference/contract.json")); }, /regular file|exactly the expected files/],
    ["A2.db and A3.db swapped (everything re-hashed)", (d) => { const a = readFileSync(join(d, "snapshots/A2.db")); const b = readFileSync(join(d, "snapshots/A3.db")); writeFileSync(join(d, "snapshots/A2.db"), b); writeFileSync(join(d, "snapshots/A3.db"), a); refresh(d); }, /A2:|A2 \(re-derived\)/],
    ["a withheld column made non-NULL in A3 (re-hashed)", (d) => { const w = new Database(join(d, "snapshots/A3.db")); w.exec("UPDATE sessions SET task_description = 'x' WHERE rowid = 1"); w.close(); refresh(d); }, /withheld columns are NULL/],
    ["an immutable field changed between exports: s-a's provider in A3 (re-hashed)", (d) => { const w = new Database(join(d, "snapshots/A3.db")); w.exec(`UPDATE sessions SET provider = 'other' WHERE id = '${S.a}'`); w.close(); refresh(d); }, /re-derived/],
    ["B1 forged to carry registry A's stamps (re-hashed)", (d) => { const w = new Database(join(d, "snapshots/B1.db")); w.exec("UPDATE schema_version SET applied_at = '2026-01-01 08:00:00'"); w.close(); refresh(d); }, /re-derived|verdict/],
    ["A4's acquisition time edited to look later (re-hashed)", (d) => { const p = join(d, "snapshots/A4.db.meta.json"); const m = JSON.parse(readFileSync(p, "utf-8")); m.acquisition.started_at = "2026-01-01T12:00:00.000Z"; writeFileSync(p, JSON.stringify(m)); refresh(d); }, /acquisition times|re-derived/],
    ["the contract's verdict edited to say A3->A4 is comparable (re-hashed)", (d) => { const p = join(d, "reference/series-contract.json"); const c = JSON.parse(readFileSync(p, "utf-8")); c.pairs[2].verdict = "comparable_if_same_registry_asserted"; writeFileSync(p, JSON.stringify(c, null, 2) + "\n"); refresh(d); }, /verdict is/],
    ["the withheld-path conflict broken: one token altered (re-hashed)", (d) => { const w = new Database(join(d, "snapshots/A3.db")); w.exec(`UPDATE file_claims SET file_path = 'withheld:0000000000000000' WHERE session_id = '${S.d}' AND file_path LIKE 'withheld:%'`); w.close(); refresh(d); }, /equality classes|same|conflict/i],
    ["a CONSISTENTLY forged package: s-a's provider changed in A3 AND the contract edited to expect it, still labelled comparable (re-hashed)", (d) => {
      const w = new Database(join(d, "snapshots/A3.db"));
      w.exec(`UPDATE sessions SET provider = 'other' WHERE id = '${S.a}'`);
      w.close();
      const cp = join(d, "reference/series-contract.json");
      const c = JSON.parse(readFileSync(cp, "utf-8"));
      c.pairs[1].sessions.changed[S.a].provider = ["claude", "other"]; // make the diff agree...
      writeFileSync(cp, JSON.stringify(c, null, 2) + "\n"); // ...while leaving verdict "comparable" and anomalies []
      refresh(d);
    }, /verdict is comparable_if_same_registry_asserted/],
    ["a claim token VALUE reused across two exports, as if it were a stable path id (re-hashed)", (d) => {
      const a1 = new Database(join(d, "snapshots/A1.db"), { readonly: true });
      const t = (a1.prepare("SELECT file_path FROM file_claims WHERE file_path LIKE 'withheld:%'").get() as { file_path: string }).file_path;
      a1.close();
      const w = new Database(join(d, "snapshots/A2.db"));
      w.prepare("UPDATE file_claims SET file_path = ? WHERE file_path LIKE 'withheld:%'").run(t);
      w.close();
      refresh(d);
    }, /no token VALUE recurs/],
    ["a canary planted in a kept column (re-hashed)", (d) => { const w = new Database(join(d, "snapshots/A2.db")); w.exec("UPDATE sessions SET host = 'SERIES-CANARY-x' WHERE rowid = 1"); w.close(); refresh(d); }, /canary/],
    ["a raw path where a digest belongs (re-hashed)", (d) => { const w = new Database(join(d, "snapshots/A2.db")); w.exec(`UPDATE sessions SET worktree_id = '${LANE_ALPHA}' WHERE id = '${S.a}'`); w.close(); refresh(d); }, /digest|re-derived|canary/],
  ];

  it("control: the untouched copy still verifies (so each rejection below is caused by its tamper)", () => {
    const d = join(tmp, "ctl");
    cpSync(pkgDir, d, { recursive: true });
    expect(verify(d).status).toBe(0);
    refresh(d);
    expect(verify(d).status, "re-hashing alone must not break a good package").toBe(0);
  });

  it.each(cases)("rejects %s", (_name, mutate, why) => {
    const d = join(tmp, `t-${sha(_name).slice(0, 8)}`);
    cpSync(pkgDir, d, { recursive: true });
    mutate(d);
    const r = verify(d);
    expect(r.status, `accepted: ${_name}`).not.toBe(0);
    // Only FAIL lines count: a PASS line names the same checks, so matching all of stdout proves nothing.
    const failed = r.stdout.split("\n").filter((l) => l.startsWith("FAIL")).join("\n");
    if (why) expect(failed, `no FAIL line for: ${_name}`).toMatch(why);
  });

  it("the producer's own check also calls a database without its marker incomplete", () => {
    const d = join(tmp, "incomplete");
    mkdirSync(d);
    writeFileSync(join(d, "A2.db"), readFileSync(snap("A2")));
    const v = verifySnapshotPair(join(d, "A2.db"));
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(/sidecar is missing/);
  });

  // --- unsafe archives ----------------------------------------------------------
  const evil = (name: string, body: string) => {
    const out = join(tmp, `${name}.tar.gz`);
    const code = `import tarfile,io,sys\nt=tarfile.open(sys.argv[1],'w:gz')\n${body}\nt.close()`;
    const r = py(["-c", code, out]);
    expect(r.status, r.stderr).toBe(0);
    return out;
  };
  it.each([
    ["an absolute path", "i=tarfile.TarInfo('/etc/pwn');i.size=1;t.addfile(i,io.BytesIO(b'x'))"],
    ["a parent-directory entry", "i=tarfile.TarInfo('top/../evil');i.size=1;t.addfile(i,io.BytesIO(b'x'))"],
    ["a symlink", "i=tarfile.TarInfo('top/l');i.type=tarfile.SYMTYPE;i.linkname='/etc/passwd';t.addfile(i)"],
    ["a hard link", "i=tarfile.TarInfo('top/h');i.type=tarfile.LNKTYPE;i.linkname='top/x';t.addfile(i)"],
    ["a setuid file", "i=tarfile.TarInfo('top/s');i.size=1;i.mode=0o4755;t.addfile(i,io.BytesIO(b'x'))"],
    ["a world-writable file", "i=tarfile.TarInfo('top/w');i.size=1;i.mode=0o666;t.addfile(i,io.BytesIO(b'x'))"],
    ["a second top-level directory", "i=tarfile.TarInfo('other/f');i.size=1;t.addfile(i,io.BytesIO(b'x'))"],
    ["a non-neutral owner", "i=tarfile.TarInfo('top/o');i.size=1;i.uid=1000;t.addfile(i,io.BytesIO(b'x'))"],
    ["a duplicate entry", "[t.addfile(tarfile.TarInfo('top/d'),io.BytesIO(b'')) for _ in range(2)]"],
  ])("the archive auditor refuses %s", (n, body) => {
    const a = evil(n.replace(/\W+/g, "-"), body);
    expect(py([join(FX, "audit_archive.py"), a, "top"]).status).not.toBe(0);
  });

  it("extraction with --keep-old-files will not overwrite an existing file", () => {
    const d = join(tmp, "noclobber");
    mkdirSync(d);
    expect(spawnSync("tar", ["-xzf", archive, "-C", d, "--no-same-owner", "--keep-old-files"]).status).toBe(0);
    writeFileSync(join(d, readdirSync(d)[0]!, "MANIFEST.json"), "precious");
    const again = spawnSync("tar", ["-xzf", archive, "-C", d, "--no-same-owner", "--keep-old-files"], { encoding: "utf-8" });
    expect(again.status).not.toBe(0);
    expect(readFileSync(join(d, readdirSync(d)[0]!, "MANIFEST.json"), "utf-8")).toBe("precious");
  });
});
