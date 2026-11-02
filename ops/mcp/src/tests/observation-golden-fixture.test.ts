// Golden restricted-observation fixture. The REAL exporter is run (in a child process, via
// fixtures/observation-golden/generate.ts) against a fictional, WAL-backed registry, and the
// resulting pair is checked here. The database itself is NOT committed: the repository ignores
// `*.db`, and committing a sidecar whose hash points at an absent file would mislead. What IS
// committed, because it is byte-stable text, is reference/contract.json and
// reference/test-vectors.json. These tests pin the pair three ways:
//   1. against hand-written literals (so the fixture cannot merely agree with its generator),
//   2. against the committed reference/ files, byte for byte (so the contract cannot go stale),
//   3. against a second, independent generation, compared on logical content because the
//      database bytes are deliberately not reproducible.
//
// Nothing here opens a real registry; every string is fictional.

import { spawnSync } from "child_process";
import { createHash } from "crypto";
import { copyFileSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync, existsSync } from "fs";
import { hostname, tmpdir } from "os";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import Database from "better-sqlite3";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { verifySnapshotPair, SNAPSHOT_META_FORMAT } from "../state/snapshot-export.js";
import { claimToken, DIGEST_RE, digestOf, isDisclosablePath, OBSERVATION_POLICY, WITHHELD_PATH_RE } from "../state/snapshot-profile.js";
import { deriveContract, deriveVectors, PRIVACY_CANARIES, SYNTHETIC } from "./fixtures/observation-golden/contract.js";
import { canariesInFile } from "./fixtures/snapshot-helpers.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const GEN_DIR = join(HERE, "fixtures/observation-golden");
const REFERENCE = join(GEN_DIR, "reference"); // committed, byte-stable
// Set in beforeAll: the generated pair lives in a temp directory.
let GOLDEN: string;
let DB: string;
let META: string;

const [NODE_MAJOR, NODE_MINOR] = process.versions.node.split(".").map(Number) as [number, number];
const CAN_STRIP_TYPES = NODE_MAJOR > 22 || (NODE_MAJOR === 22 && NODE_MINOR >= 6);

// The generator runs under Node's type stripping (22.6+). `engines` still allows Node 20, where
// this whole file skips VISIBLY rather than failing or passing.
const suite = describe.skipIf(!CAN_STRIP_TYPES);

function runGenerator(out: string): void {
  const r = spawnSync(
    process.execPath,
    [
      "--experimental-strip-types",
      "--no-warnings",
      "--import",
      join(HERE, "fixtures/ts-resolve-register.mjs"),
      join(GEN_DIR, "generate.ts"),
      out,
    ],
    { encoding: "utf-8", timeout: 90_000, cwd: join(HERE, "../..") }
  );
  expect(r.status, r.stderr).toBe(0);
}

let genRoot: string;
beforeAll(() => {
  if (!CAN_STRIP_TYPES) return;
  genRoot = mkdtempSync(join(tmpdir(), "icn-golden-gen-"));
  GOLDEN = join(genRoot, "pair");
  runGenerator(GOLDEN);
  DB = join(GOLDEN, "observation.db");
  META = `${DB}.meta.json`;
}, 120_000);
afterAll(() => {
  if (genRoot) rmSync(genRoot, { recursive: true, force: true });
});

const sha = (s: string | Buffer) => createHash("sha256").update(s).digest("hex");
const open = (p = DB) => new Database(p, { readonly: true });
function rows(sql: string, p = DB): Array<Record<string, unknown>> {
  const d = open(p);
  try {
    return d.prepare(sql).all() as Array<Record<string, unknown>>;
  } finally {
    d.close();
  }
}

let tmp: string;
beforeEach(() => {
  tmp = mkdtempSync(join(tmpdir(), "icn-golden-test-"));
});
afterEach(() => rmSync(tmp, { recursive: true, force: true }));

/** A private, writable copy of the committed pair for negative tests. */
function copyPair(): string {
  const dest = join(tmp, "observation.db");
  copyFileSync(DB, dest);
  copyFileSync(META, `${dest}.meta.json`);
  return dest;
}
/** Re-bind the sidecar to the (modified) database so only the invariant under test can fail. */
function rebind(dest: string, patch: (m: Record<string, any>) => void = () => {}): void {
  const m = JSON.parse(readFileSync(`${dest}.meta.json`, "utf-8"));
  m.snapshot.sha256 = sha(readFileSync(dest));
  m.snapshot.bytes = statSync(dest).size;
  patch(m);
  writeFileSync(`${dest}.meta.json`, JSON.stringify(m));
}

suite("the generated golden pair verifies and is what the committed contract says", () => {
  it("passes the producer's own consumer-side check", () => {
    expect(verifySnapshotPair(DB)).toEqual({ ok: true, profile: "observation", problems: [] });
  });

  it("the sidecar's sha256 and length cover exactly the database bytes (and nothing else)", () => {
    const m = JSON.parse(readFileSync(META, "utf-8"));
    expect(m.snapshot.sha256).toBe(sha(readFileSync(DB)));
    expect(m.snapshot.bytes).toBe(statSync(DB).size);
    expect(m.snapshot.file).toBe("observation.db");
    // The sidecar is not self-covered: editing it does not change the database hash.
    expect(JSON.stringify(m)).not.toContain(sha(readFileSync(META)));
  });

  it("contract.json equals a fresh derivation from source + fixture (it cannot go stale)", () => {
    const fresh = JSON.stringify(deriveContract(DB), null, 2) + "\n";
    expect(readFileSync(join(REFERENCE, "contract.json"), "utf-8")).toBe(fresh);
  });

  it("test-vectors.json equals a fresh derivation", () => {
    const fresh = JSON.stringify(deriveVectors(DB), null, 2) + "\n";
    expect(readFileSync(join(REFERENCE, "test-vectors.json"), "utf-8")).toBe(fresh);
  });

  it("is a delete-journal SQLite 3 file with the header the contract promises", () => {
    const h = readFileSync(DB).subarray(0, 100);
    expect(h.subarray(0, 16).toString("latin1")).toBe("SQLite format 3\0");
    expect(h.readUInt16BE(16)).toBe(4096); // page size
    expect([h[18], h[19]]).toEqual([1, 1]); // 1/1 = rollback journal; 2/2 would be WAL
    expect(h.readUInt32BE(56)).toBe(1); // UTF-8
    expect(h.readUInt32BE(68)).toBe(0); // application_id
    const before = readdirSync(GOLDEN).sort();
    expect(rows("PRAGMA journal_mode")).toEqual([{ journal_mode: "delete" }]);
    expect(readdirSync(GOLDEN).sort()).toEqual(before); // reading created no -wal/-shm
  });
});

suite("exact observation schema", () => {
  it("has exactly the three observation tables and no other schema object", () => {
    expect(rows("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")).toEqual([
      { name: "file_claims" },
      { name: "schema_version" },
      { name: "sessions" },
    ]);
    expect(rows("SELECT DISTINCT type FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type")).toEqual([
      { type: "index" },
      { type: "table" },
    ]);
    for (const t of ["mailbox", "events", "watchers_process", "health_cache", "decision_index"]) {
      expect(rows(`SELECT name FROM sqlite_master WHERE name = '${t}'`)).toEqual([]);
    }
  });

  it("sessions has the 24 production columns with their SQLite types and NOT NULL rules", () => {
    const cols = rows("PRAGMA table_info(sessions)").map((c) => [c["name"], c["type"], c["notnull"]]);
    expect(cols).toEqual(
      expect.arrayContaining([
        ["id", "TEXT", 0], // PRIMARY KEY: SQLite does not imply NOT NULL for TEXT keys
        ["repo", "TEXT", 1],
        ["repo_id", "TEXT", 0], // NULLABLE: added in v3 without NOT NULL
        ["worktree_id", "TEXT", 0],
        ["worktree_generation", "TEXT", 0],
        ["provider_session_id", "TEXT", 0],
        ["agent_pid", "INTEGER", 0],
        ["progress_count", "INTEGER", 1],
      ])
    );
    expect(cols).toHaveLength(24);
  });

  it("classifies columns as the policy says: verbatim / digested / withheld", () => {
    const c = JSON.parse(readFileSync(join(REFERENCE, "contract.json"), "utf-8"));
    const by = (d: string) => c.database.columns.sessions.filter((x: any) => x.disposition === d).map((x: any) => x.name).sort();
    expect(by("verbatim")).toEqual([...OBSERVATION_POLICY.sessions.verbatim].sort());
    expect(by("digest")).toEqual(["repo_id", "worktree_id"]);
    expect(by("withheld")).toEqual([...OBSERVATION_POLICY.sessions.withheld].sort());
  });
});

suite("repo_id: nullable in the source schema, so NULL is preserved and a value is never invented", () => {
  it("is NOT 'NOT NULL' in the original schema", () => {
    // The registry's own DDL, via the contract derived from the fixture.
    const c = JSON.parse(readFileSync(join(REFERENCE, "contract.json"), "utf-8"));
    const repoId = c.database.columns.sessions.find((x: any) => x.name === "repo_id");
    expect(repoId.not_null_in_schema).toBe(false);
    const repo = c.database.columns.sessions.find((x: any) => x.name === "repo");
    expect(repo.not_null_in_schema).toBe(true);
  });

  it("a non-NULL source repo_id always exports as a valid digest of the raw value; the path is gone", () => {
    const vals = rows("SELECT DISTINCT repo_id FROM sessions WHERE repo_id IS NOT NULL").map((r) => r["repo_id"]);
    expect(vals).toEqual([`sha256:${sha(SYNTHETIC.repoId)}`]); // independent recomputation
    for (const v of vals) expect(String(v)).toMatch(DIGEST_RE);
    expect(canariesInFile(DB, [SYNTHETIC.repoId, SYNTHETIC.root])).toEqual([]);
  });

  it("a NULL source repo_id (pre-identity row) stays NULL — and so does its worktree_id", () => {
    expect(rows("SELECT repo_id, worktree_id, worktree_generation, provider_session_id FROM sessions WHERE id = 'sess-legacy-1'")).toEqual([
      { repo_id: null, worktree_id: null, worktree_generation: null, provider_session_id: null },
    ]);
  });
});

suite("retained observable facts (hand-written literals, independent of the generator)", () => {
  const laneDigest = (raw: string) => `sha256:${sha(raw)}`;

  it("lane lookup BY DIGEST returns exactly that lane's activations", () => {
    const ids = (raw: string) =>
      rows(`SELECT id FROM sessions WHERE worktree_id = '${laneDigest(raw)}' ORDER BY id`).map((r) => r["id"]);
    expect(ids(SYNTHETIC.laneAlpha)).toEqual(["sess-alpha-1", "sess-alpha-2", "sess-alpha-3"]);
    expect(ids(SYNTHETIC.laneBeta)).toEqual(["sess-beta-1"]); // never selected by an alpha lookup
    expect(ids("/synthetic/golden-root/repos/icn-golden.git/worktrees/no-such-lane")).toEqual([]);
  });

  it("the raw lane path finds nothing — the file holds no raw path to match", () => {
    expect(rows(`SELECT id FROM sessions WHERE worktree_id = '${SYNTHETIC.laneAlpha}'`)).toEqual([]);
  });

  it("one full activation row is preserved verbatim, withheld fields are NULL", () => {
    expect(rows("SELECT * FROM sessions WHERE id = 'sess-alpha-1'")).toEqual([
      {
        id: "sess-alpha-1",
        repo: "icn-golden",
        worktree: null,
        task_description: null,
        started_at: "2026-01-01 10:00:00",
        last_heartbeat: "2026-01-01 10:20:00",
        repo_id: `sha256:${sha(SYNTHETIC.repoId)}`,
        worktree_id: laneDigest(SYNTHETIC.laneAlpha),
        worktree_path: null,
        worktree_name: "lane-alpha",
        provider_session_id: SYNTHETIC.conversationAlpha1,
        branch_at_registration: null,
        head_at_registration: null,
        task_ref: null,
        pr_ref: null,
        parent_session_id: null,
        provider: "claude",
        transcript_path: null,
        agent_pid: 41001,
        host: "golden-host-a",
        current_activity: null,
        last_progress: "2026-01-01 10:15:00",
        progress_count: 7,
        worktree_generation: SYNTHETIC.generationAlpha,
      },
    ]);
  });

  it("conversation and generation: present, NULL-unknown, and mixed within one lane", () => {
    expect(rows("SELECT id, provider_session_id AS c, worktree_generation AS g FROM sessions WHERE worktree_id = '" + laneDigest(SYNTHETIC.laneAlpha) + "' ORDER BY id")).toEqual([
      { id: "sess-alpha-1", c: SYNTHETIC.conversationAlpha1, g: SYNTHETIC.generationAlpha },
      { id: "sess-alpha-2", c: null, g: SYNTHETIC.generationAlpha }, // unknown conversation
      { id: "sess-alpha-3", c: SYNTHETIC.conversationAlpha3, g: null }, // missing generation
    ]);
    expect(rows("SELECT parent_session_id AS p FROM sessions WHERE id = 'sess-alpha-2'")).toEqual([{ p: "sess-alpha-1" }]);
  });

  it("every withheld column is NULL in every row", () => {
    for (const c of OBSERVATION_POLICY.sessions.withheld) {
      expect(rows(`SELECT COUNT(*) AS n FROM sessions WHERE ${c} IS NOT NULL`)).toEqual([{ n: 0 }]);
    }
  });

  it("schema_version rows survive with their (fixed) stamps", () => {
    expect(rows("SELECT version, applied_at FROM schema_version ORDER BY version")).toEqual(
      [1, 2, 3, 4, 5].map((version) => ({ version, applied_at: "2026-01-01 00:00:00" }))
    );
  });
});

suite("file claims: conflict information survives, withheld paths do not", () => {
  const claims = () => rows("SELECT file_path, session_id, claimed_at FROM file_claims ORDER BY session_id, claimed_at");

  it("keeps all 7 rows — no claim is dropped because its path is withheld", () => {
    expect(claims()).toHaveLength(7);
    expect(rows("SELECT COUNT(*) AS n FROM sessions")).toEqual([{ n: 5 }]);
    expect(rows("PRAGMA foreign_key_check")).toEqual([]);
  });

  it("a disclosable path claimed by two lanes is a visible, cleartext conflict", () => {
    expect(
      rows("SELECT session_id FROM file_claims WHERE file_path = 'icn/crates/golden-core/src/lib.rs' ORDER BY session_id")
    ).toEqual([{ session_id: "sess-alpha-1" }, { session_id: "sess-beta-1" }]);
    expect(rows("SELECT file_path FROM file_claims WHERE session_id = 'sess-alpha-2' AND file_path = 'docs/golden/notes.md'")).toHaveLength(1);
  });

  it("a withheld path claimed by two sessions is still a conflict: the SAME token on both rows", () => {
    const secret = claims().filter((c) => c["session_id"] === "sess-alpha-1" || c["session_id"] === "sess-beta-1").filter((c) => String(c["file_path"]).startsWith("withheld:"));
    expect(secret).toHaveLength(2);
    expect(secret[0]!["file_path"]).toBe(secret[1]!["file_path"]);
    expect(String(secret[0]!["file_path"])).toMatch(WITHHELD_PATH_RE);
  });

  it("distinct withheld paths get distinct tokens; every token is well-formed", () => {
    const tokens = claims().map((c) => String(c["file_path"])).filter((p) => p.startsWith("withheld:"));
    expect(tokens).toHaveLength(4);
    expect(new Set(tokens).size).toBe(3); // secret.key (x2), tilde, with-space
    for (const t of tokens) expect(t).toMatch(WITHHELD_PATH_RE);
  });

  it("only disclosable paths appear as written", () => {
    const shown = claims().map((c) => String(c["file_path"])).filter((p) => !p.startsWith("withheld:"));
    expect(new Set(shown)).toEqual(new Set(["icn/crates/golden-core/src/lib.rs", "docs/golden/notes.md"]));
    for (const p of shown) expect(isDisclosablePath(p)).toBe(true);
  });
});

suite("test vectors: what can and cannot be recomputed", () => {
  const V = JSON.parse(readFileSync(join(REFERENCE, "test-vectors.json"), "utf-8"));

  it("digest vectors recompute from the raw value with nothing private (independent SHA-256)", () => {
    for (const d of V.digest_vectors) {
      expect(d.digest).toBe(`sha256:${sha(d.raw)}`);
      expect(d.digest).toBe(digestOf(d.raw));
      expect(d.recomputable).toBe(true);
    }
  });

  it("the token FORMULA reproduces with the published test salt (independent computation)", () => {
    const salt = Buffer.from(V.claim_token_formula.salt_hex, "hex");
    for (const v of V.claim_token_formula.vectors) {
      const expected = "withheld:" + createHash("sha256").update(salt).update(Buffer.from([0])).update(v.path, "utf8").digest("hex").slice(0, 16);
      expect(v.token).toBe(expected);
      expect(claimToken(salt, v.path)).toBe(expected);
    }
  });

  it("the fixture's OWN tokens are not recomputable: no published salt reproduces them", () => {
    const salt = Buffer.from(V.claim_token_formula.salt_hex, "hex");
    const mine = claims();
    const secret = claimToken(salt, `${SYNTHETIC.root}/GOLDEN-CANARY-secret.key`);
    expect(mine.map((c) => c["file_path"])).not.toContain(secret);
    function claims() {
      return rows("SELECT file_path FROM file_claims");
    }
  });

  it("path-disclosure vectors match the real predicate", () => {
    for (const p of V.claim_path_disclosure) expect(isDisclosablePath(p.path)).toBe(p.disclosed_as_written);
    const ok = new Set(V.claim_path_disclosure.filter((p: any) => p.disclosed_as_written).map((p: any) => p.path));
    expect(ok).toEqual(new Set(["icn/crates/golden-core/src/lib.rs", "docs/golden/notes.md", "./local/file.ts", "Cargo.lock"]));
  });
});

suite("privacy: nothing withheld survives in ANY byte of the pair", () => {
  it("scanner self-check: it does find a canary when one is present", () => {
    const p = join(tmp, "positive.bin");
    writeFileSync(p, Buffer.concat([Buffer.from([0, 1, 2]), Buffer.from("xxGOLDEN-CANARY-zz"), Buffer.from([255])]));
    expect(canariesInFile(p, PRIVACY_CANARIES)).toEqual(["GOLDEN-CANARY"]);
  });

  it("no synthetic canary or raw synthetic path appears in the database or the sidecar", () => {
    expect(canariesInFile(DB, PRIVACY_CANARIES)).toEqual([]);
    expect(canariesInFile(META, PRIVACY_CANARIES)).toEqual([]);
  });

  it("nothing from the generating machine leaked: no real hostname, user, home, temp path or private IP", () => {
    const needles = ["/home/", "/tmp/", "icn-golden-src", "10.8.", "icn-dev", "ubuntu"];
    const h = hostname();
    if (h.length >= 4) needles.push(h);
    // The fictional claimed host is allowed to differ from the real one; if they coincide the
    // check above would be meaningless, so assert they do not.
    expect(h).not.toBe("golden-fixture-host");
    expect(canariesInFile(DB, needles)).toEqual([]);
    expect(canariesInFile(META, needles)).toEqual([]);
  });

  it("the claimed host is the fictional one, explicitly unauthenticated", () => {
    const m = JSON.parse(readFileSync(META, "utf-8"));
    expect(m.host).toEqual({ claimed: { hostname: "golden-fixture-host" }, authenticated: null, authenticity: "none" });
  });
});

suite("sidecar contract", () => {
  let m: any;
  beforeAll(() => {
    m = JSON.parse(readFileSync(META, "utf-8"));
  });

  it("names the format, profile, policy and schema; carries no source path or identity", () => {
    expect(m.format).toBe(SNAPSHOT_META_FORMAT);
    expect(m.profile).toBe("observation");
    expect(m.disclosure.policy).toBe("observation-v1");
    expect(m.disclosure.classification).toBe("least-privilege-observation");
    expect(m.snapshot.schema_version_max).toBe(5);
    expect(m.acquisition.method).toBe("sqlite-read-transaction-projection");
    for (const k of ["path_given", "realpath", "dev", "ino", "identity_sha256"]) expect(m.source).not.toHaveProperty(k);
    expect(m.snapshot).toMatchObject({ journal_mode: "delete", page_size: 4096 });
  });

  it("binds row counts and states the integrity checks it ran on the copy", () => {
    expect(m.consistency.table_row_counts).toEqual({ file_claims: 7, schema_version: 5, sessions: 5 });
    expect(m.consistency.integrity_check).toBe("ok");
    expect(m.consistency.foreign_key_violations).toBe(0);
  });

  it("discloses exactly what was withheld, in its own words", () => {
    expect(m.disclosure.tables_withheld).toEqual(["decision_index", "events", "health_cache", "mailbox", "watchers_process"]);
    expect(m.disclosure.sessions.digested_columns).toEqual(["repo_id", "worktree_id"]);
    expect(m.disclosure.sessions.withheld_columns).toEqual([...OBSERVATION_POLICY.sessions.withheld]);
    expect(m.disclosure.file_claims.rows_with_withheld_path).toBe(4);
    expect(m.disclosure.null_semantics).toMatch(/WITHHELD/);
  });
});

suite("negative: a reader following the contract must reject each of these", () => {
  it("accepts an untouched copy (control)", () => {
    expect(verifySnapshotPair(copyPair()).ok).toBe(true);
  });

  it("a flipped byte in the database", () => {
    const d = copyPair();
    const b = readFileSync(d);
    b[b.length - 1] = b[b.length - 1]! ^ 0xff;
    writeFileSync(d, b);
    expect(verifySnapshotPair(d).ok).toBe(false);
  });

  it("a missing sidecar", () => {
    const d = copyPair();
    rmSync(`${d}.meta.json`);
    const v = verifySnapshotPair(d);
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(/sidecar is missing/);
  });

  it("a database that still holds a withheld table", () => {
    const d = copyPair();
    const w = new Database(d);
    w.exec("CREATE TABLE mailbox (id INTEGER PRIMARY KEY, payload TEXT)");
    w.close();
    rebind(d);
    expect(verifySnapshotPair(d).problems.join(" ")).toMatch(/unexpected table "mailbox"/);
  });

  it("a withheld column that is no longer NULL", () => {
    const d = copyPair();
    const w = new Database(d);
    w.exec("UPDATE sessions SET task_description = 'leak' WHERE id = 'sess-alpha-1'");
    w.close();
    rebind(d);
    expect(verifySnapshotPair(d).problems.join(" ")).toMatch(/withheld column sessions\.task_description/);
  });

  it("a raw path where a digest belongs", () => {
    const d = copyPair();
    const w = new Database(d);
    w.exec(`UPDATE sessions SET worktree_id = '${SYNTHETIC.laneAlpha}' WHERE id = 'sess-alpha-1'`);
    w.close();
    rebind(d);
    expect(verifySnapshotPair(d).problems.join(" ")).toMatch(/not a sha256 digest/);
  });

  it("a raw absolute path in a claim", () => {
    const d = copyPair();
    const w = new Database(d);
    w.exec("UPDATE file_claims SET file_path = '/etc/shadow' WHERE rowid = 1");
    w.close();
    rebind(d);
    expect(verifySnapshotPair(d).problems.join(" ")).toMatch(/neither repo-relative nor a withheld token/);
  });

  it("an unsupported schema version", () => {
    const d = copyPair();
    const w = new Database(d);
    w.exec("DELETE FROM schema_version WHERE version = 5");
    w.close();
    rebind(d);
    expect(verifySnapshotPair(d).ok).toBe(false);
  });

  it("a sidecar whose row counts do not describe this database", () => {
    const d = copyPair();
    rebind(d, (m) => (m.consistency.table_row_counts.sessions = 99));
    expect(verifySnapshotPair(d).problems.join(" ")).toMatch(/row counts/);
  });

  it("a sidecar describing a different file name", () => {
    const d = copyPair();
    rebind(d, (m) => (m.snapshot.file = "other.db"));
    expect(verifySnapshotPair(d).ok).toBe(false);
  });

  it("a wrong policy version, a wrong profile label, or a classification that contradicts the profile", () => {
    for (const patch of [
      (m: any) => (m.disclosure.policy = "observation-v0"),
      (m: any) => (m.profile = "full"),
      (m: any) => (m.disclosure.classification = "restricted-full-fidelity"),
      (m: any) => (m.format = "icn-ops-registry-snapshot-meta/v2"),
    ]) {
      const d = copyPair();
      rebind(d, patch);
      expect(verifySnapshotPair(d).ok, JSON.stringify(patch.toString())).toBe(false);
    }
  });
});

suite("regeneration: an independent second generation agrees with the first and the reference", () => {
  function regenerate(): string {
    const out = join(tmp, "regen");
    runGenerator(out);
    return out;
  }

  /** Logical content with token VALUES replaced by first-appearance labels (T1, T2, ...). */
  function normalised(db: string): unknown {
    const labels = new Map<string, string>();
    const claims = rows("SELECT file_path, session_id, claimed_at FROM file_claims ORDER BY session_id, claimed_at, rowid", db).map((c) => {
      const p = String(c["file_path"]);
      if (!p.startsWith("withheld:")) return c;
      if (!labels.has(p)) labels.set(p, `T${labels.size + 1}`);
      return { ...c, file_path: labels.get(p) };
    });
    return {
      sessions: rows("SELECT * FROM sessions ORDER BY id", db),
      schema_version: rows("SELECT * FROM schema_version ORDER BY version", db),
      claims,
    };
  }

  it("a fresh generation verifies, has identical contract/vectors bytes and identical logical content", () => {
    const out = regenerate();
    const fresh = join(out, "observation.db");
    expect(verifySnapshotPair(fresh).ok).toBe(true);
    for (const f of ["contract.json", "test-vectors.json"]) {
      expect(readFileSync(join(out, f), "utf-8"), f).toBe(readFileSync(join(REFERENCE, f), "utf-8"));
    }
    expect(normalised(fresh)).toEqual(normalised(DB));
    expect(canariesInFile(fresh, PRIVACY_CANARIES)).toEqual([]);
  }, 120_000);

  it("is NOT byte-reproducible, as documented: the random token salt and timestamps change the hash", () => {
    const out = regenerate();
    expect(sha(readFileSync(join(out, "observation.db")))).not.toBe(sha(readFileSync(DB)));
    expect(existsSync(`${join(out, "observation.db")}.meta.json`)).toBe(true);
  }, 120_000);
});
