// Regressions for the GitHub review of PR #2856. Each block was written to FAIL against the
// previous implementation first (see the PR thread for the recorded failures), then made to pass.
//
//   A  unsafe values must never reach the output file, not even a `.partial` left by a hard kill
//   B  the consumer-side check must enforce the exact column layout of ALL three tables
//   C  verifySnapshotPair must return a bounded verdict for hostile file types, never throw/hang
//   D  --expect-profile / expectProfile: a consumer can require the profile it expects
//
// Synthetic data only. Nothing here opens the real shared registry.

import { copyFileSync, chmodSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "fs";
import { execFileSync, spawnSync } from "child_process";
import { basename, dirname, join } from "path";
import { fileURLToPath } from "url";
import Database from "better-sqlite3";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { exportRegistrySnapshot, verifySnapshotPair, type SnapshotProfile } from "../state/snapshot-export.js";
import { initDb } from "../state/db.js";
import { OBSERVATION_POLICY, checkObservationInvariants, observationValueProblem } from "../state/snapshot-profile.js";
import { addSession, expectRefusal, makeRegistry, removeDir, sha256File, tempDir } from "./fixtures/snapshot-helpers.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const [NODE_MAJOR, NODE_MINOR] = process.versions.node.split(".").map(Number) as [number, number];
const CAN_STRIP_TYPES = NODE_MAJOR > 22 || (NODE_MAJOR === 22 && NODE_MINOR >= 6);
const itChild = it.skipIf(!CAN_STRIP_TYPES);

let work: string; // sources live here
let out: string; // destinations live here (separate, so a scan of `out` sees only export output)
beforeEach(() => {
  work = tempDir("icn-review-src-");
  out = tempDir("icn-review-out-");
});
afterEach(() => {
  removeDir(work);
  removeDir(out);
});

/** Every file under `d` whose raw bytes contain `needle` (a scan of the FILES, not of queries). */
function filesContaining(d: string, needle: string): string[] {
  return readdirSync(d)
    .filter((n) => statSync(join(d, n)).isFile() && readFileSync(join(d, n)).includes(Buffer.from(needle, "utf8")))
    .sort();
}

function benignSource(): string {
  const p = makeRegistry(work);
  const db = initDb(p);
  addSession(db, "sess-1", ["src/a.rs"]);
  addSession(db, "sess-2", ["docs/b.md"]);
  db.close();
  return p;
}

function exportPair(profile: SnapshotProfile, source: string, name = "o.db", dir = out): string {
  const dest = join(dir, name);
  exportRegistrySnapshot({ profile, source, dest });
  return dest;
}

// ── A. nothing unsafe is written ────────────────────────────────────────────────

const NEEDLE = "CANARY";
interface UnsafeCase {
  label: string;
  sql: string;
}
const UNSAFE: UnsafeCase[] = [
  { label: "sessions.host (absolute path)", sql: "UPDATE sessions SET host = '/home/CANARYUSER/host-canary'" },
  { label: "sessions.repo (absolute path)", sql: "UPDATE sessions SET repo = '/home/CANARYUSER/repo-canary'" },
  { label: "sessions.worktree_name (traversal)", sql: "UPDATE sessions SET worktree_name = 'wt/../CANARY-traversal'" },
  { label: "sessions.provider_session_id (drive path)", sql: "UPDATE sessions SET provider_session_id = 'C:\\CANARY\\conv-' || id" },
  { label: "sessions.worktree_generation (URI)", sql: "UPDATE sessions SET worktree_generation = 'file:///CANARY-gen'" },
  { label: "sessions.agent_pid (text in a numeric column)", sql: "UPDATE sessions SET agent_pid = '/CANARY-pid'" },
  // Free text that passes the identifier alphabet but is not a timestamp.
  { label: "sessions.started_at (free text, not a time)", sql: "UPDATE sessions SET started_at = 'CANARY free text not a time'" },
  { label: "sessions.last_heartbeat (free text, not a time)", sql: "UPDATE sessions SET last_heartbeat = 'CANARY free text not a time'" },
  { label: "sessions.last_progress (free text, not a time)", sql: "UPDATE sessions SET last_progress = 'CANARY free text not a time'" },
  // These two columns were copied with NO check at all.
  { label: "file_claims.claimed_at (absolute path)", sql: "UPDATE file_claims SET claimed_at = '/home/CANARYUSER/claimed-canary'" },
  { label: "file_claims.claimed_at (free text)", sql: "UPDATE file_claims SET claimed_at = 'CANARY claimed note'" },
  { label: "schema_version.applied_at (absolute path)", sql: "UPDATE schema_version SET applied_at = '/home/CANARYUSER/applied-canary'" },
  { label: "schema_version.applied_at (free text)", sql: "UPDATE schema_version SET applied_at = 'CANARY applied note'" },
];

function unsafeSource(c: UnsafeCase): string {
  const p = benignSource();
  const db = new Database(p);
  db.exec(c.sql);
  db.close();
  return p;
}

describe("A. an unsafe value never reaches the output file (observation)", () => {
  it("control: the benign source exports, so the refusals below are not vacuous", () => {
    expect(verifySnapshotPair(exportPair("observation", benignSource())).ok).toBe(true);
  });

  it("control: SQLite-format and ISO-8601 timestamps are accepted", () => {
    const p = benignSource();
    const db = new Database(p);
    db.exec(
      "UPDATE sessions SET started_at = '2026-01-01 10:00:00', last_heartbeat = '2026-01-01T10:05:00Z', last_progress = '2026-01-01T10:05:00.123Z'"
    );
    db.close();
    expect(verifySnapshotPair(exportPair("observation", p)).ok).toBe(true);
  });

  it.each(UNSAFE)("$label: refused as SNAPSHOT_VERIFY_FAILED BEFORE any row is written", (c) => {
    const src = unsafeSource(c);
    let hookCalled = false;
    let leakedAtHook: string[] = [];
    const dest = join(out, "o.db");
    const e = expectRefusal("SNAPSHOT_VERIFY_FAILED", () =>
      exportRegistrySnapshot({
        profile: "observation",
        source: src,
        dest,
        hooks: {
          // The state a SIGKILL here would leave behind.
          afterCopy: (tmp) => {
            hookCalled = true;
            leakedAtHook = filesContaining(dirname(tmp), NEEDLE);
          },
        },
      })
    );
    // The check happens during projection, so the "copy finished" stage is never reached with
    // an unsafe value on disk.
    expect(hookCalled).toBe(false);
    expect(leakedAtHook).toEqual([]);
    expect(e.message).not.toContain(NEEDLE); // the refusal never echoes the value
    expect(filesContaining(out, NEEDLE)).toEqual([]); // nothing in the destination directory
    expect(existsSync(dest)).toBe(false);
    expect(existsSync(`${dest}.meta.json`)).toBe(false);
  });

  itChild("a REAL SIGKILL after projection leaves no unsafe bytes behind", () => {
    const src = unsafeSource(UNSAFE[0]!);
    const r = spawnSync(
      process.execPath,
      [
        "--experimental-strip-types",
        "--import",
        join(HERE, "fixtures/ts-resolve-register.mjs"),
        "--no-warnings",
        join(HERE, "fixtures/snapshot-child.ts"),
        "observation",
        src,
        join(out, "o.db"),
        "afterCopy",
      ],
      { encoding: "utf-8", timeout: 60_000 }
    );
    // Previously: killed with the path-like host sitting in the 0600 `.partial`.
    // Now: refused during projection, so the kill point is never reached.
    expect(r.signal).not.toBe("SIGKILL");
    expect(r.status).not.toBe(0);
    expect(filesContaining(out, NEEDLE)).toEqual([]);
    expect(readdirSync(out).filter((n) => !n.endsWith(".partial"))).toEqual([]); // no published name
  });

  it("the FULL profile is unaffected: it is a restricted-fidelity copy and keeps verbatim text", () => {
    const src = unsafeSource(UNSAFE[0]!);
    const dest = exportPair("full", src);
    expect(verifySnapshotPair(dest).ok).toBe(true);
    expect(filesContaining(out, NEEDLE)).toEqual([dest.replace(`${out}/`, "")]);
  });
});

// ── B. exact layout on every table ─────────────────────────────────────────────

/** Copy a finished pair, run `mutate` on the copy's database, then RESEAL the sidecar (hash + length). */
function tamperedCopy(fromDest: string, mutate: (db: Database.Database) => void, patchMeta?: (m: Record<string, any>) => void): string {
  const dir = tempDir("icn-review-tamper-");
  const dest = join(dir, basename(fromDest)); // the sidecar names its file, so keep the name
  copyFileSync(fromDest, dest);
  copyFileSync(`${fromDest}.meta.json`, `${dest}.meta.json`);
  const db = new Database(dest);
  mutate(db);
  db.close();
  const m = JSON.parse(readFileSync(`${dest}.meta.json`, "utf-8"));
  m.snapshot.sha256 = sha256File(dest);
  m.snapshot.bytes = statSync(dest).size;
  patchMeta?.(m);
  writeFileSync(`${dest}.meta.json`, JSON.stringify(m, null, 2));
  return dest;
}

describe("B. the consumer refuses any pair whose columns are not exactly the observation layout", () => {
  const LEAK = "CANARY-extra-column-value";
  const cases: Array<{ table: string; alter: string; update?: string }> = [
    { table: "sessions", alter: "ALTER TABLE sessions ADD COLUMN leak TEXT", update: `UPDATE sessions SET leak = '${LEAK}'` },
    { table: "file_claims", alter: "ALTER TABLE file_claims ADD COLUMN leak TEXT", update: `UPDATE file_claims SET leak = '${LEAK}'` },
    { table: "schema_version", alter: "ALTER TABLE schema_version ADD COLUMN leak TEXT", update: `UPDATE schema_version SET leak = '${LEAK}'` },
    // A generated column is invisible to PRAGMA table_info; only table_xinfo shows it.
    { table: "file_claims", alter: "ALTER TABLE file_claims ADD COLUMN leak TEXT GENERATED ALWAYS AS ('CANARY-generated') VIRTUAL" },
    { table: "schema_version", alter: "ALTER TABLE schema_version ADD COLUMN leak TEXT GENERATED ALWAYS AS ('CANARY-generated') VIRTUAL" },
  ];

  it("control: an untouched, resealed copy verifies, so the helper is not what causes the refusals", () => {
    const dest = tamperedCopy(exportPair("observation", benignSource()), () => {});
    expect(verifySnapshotPair(dest)).toEqual({ ok: true, profile: "observation", problems: [] });
  });

  it.each(cases)("extra column on $table ($alter)", (c) => {
    const good = exportPair("observation", benignSource());
    const dest = tamperedCopy(good, (db) => {
      db.exec(c.alter);
      if (c.update) db.exec(c.update);
    });
    const v = verifySnapshotPair(dest);
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(new RegExp(`${c.table} has unclassified column "leak"`));
    expect(v.problems.join(" ")).not.toContain(LEAK); // names the column, never the value
  });

  it("a missing required column is a refusal, not an exception", () => {
    const good = exportPair("observation", benignSource());
    for (const t of ["file_claims", "schema_version"]) {
      const col = t === "file_claims" ? "claimed_at" : "applied_at";
      const dest = tamperedCopy(good, (db) => db.exec(`ALTER TABLE ${t} DROP COLUMN ${col}`));
      const v = verifySnapshotPair(dest);
      expect(v.ok, t).toBe(false);
      expect(v.problems.join(" "), t).toMatch(new RegExp(`${t} lacks column.*${col}`));
    }
  });

  it.each([
    ["a table named sqlite + one character (LIKE's '_' wildcard)", "CREATE TABLE sqliteXleak (v TEXT); INSERT INTO sqliteXleak VALUES ('/home/CANARYUSER/secret')", /unexpected table "sqliteXleak"/],
    ["an index with a free-form name", 'CREATE INDEX "/home/CANARYUSER/idx-canary" ON file_claims (claimed_at)', /unexpected index "\/home\/CANARYUSER\/idx-canary"/],
    ["a view named sqlite + one character", "CREATE VIEW sqliteXview AS SELECT 1 AS v", /sqliteXview/],
    ["a trigger named sqlite + one character", "CREATE TRIGGER sqliteXtrg AFTER INSERT ON file_claims BEGIN SELECT 1; END", /sqliteXtrg/],
  ])("%s is refused", (_n, sql, re) => {
    const good = exportPair("observation", benignSource());
    const dest = tamperedCopy(good, (db) => db.exec(sql));
    const v = verifySnapshotPair(dest);
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(re);
  });

  // The schema TEXT (sqlite_master.sql) is free text too: matching object NAMES is not enough.
  it.each([
    [
      "an index EXPRESSION carrying a path (same name as a real index)",
      (db: Database.Database) =>
        db.exec(
          "DROP INDEX idx_sessions_parent; CREATE INDEX idx_sessions_parent ON sessions (parent_session_id || '/home/CANARYUSER/private-key')"
        ),
    ],
    [
      "a partial-index PREDICATE carrying a path",
      (db: Database.Database) =>
        db.exec(
          "DROP INDEX idx_sessions_parent; CREATE INDEX idx_sessions_parent ON sessions (parent_session_id) WHERE host <> '/home/CANARYUSER/predicate'"
        ),
    ],
    [
      "a column DEFAULT in a table's CREATE statement (columns and rows unchanged)",
      (db: Database.Database) =>
        db.exec(
          "CREATE TABLE fc2 (file_path TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, claimed_at TEXT DEFAULT '/home/CANARYUSER/default-canary', PRIMARY KEY (file_path, session_id));" +
            "INSERT INTO fc2 SELECT file_path, session_id, claimed_at FROM file_claims;" +
            "DROP TABLE file_claims; ALTER TABLE fc2 RENAME TO file_claims;"
        ),
    ],
  ])("%s is refused (definitions, not just names, must match observation-v1)", (_n, mutate) => {
    const good = exportPair("observation", benignSource());
    const dest = tamperedCopy(good, mutate as (db: Database.Database) => void);
    const v = verifySnapshotPair(dest, { expectProfile: "observation" });
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(/definition differs from observation-v1/);
    expect(v.problems.join(" ")).not.toContain("CANARYUSER"); // names the object, never the text
  });

  it("checkObservationInvariants alone (it is exported for consumers) also refuses lookalike names", () => {
    // verifySnapshotPair's shape check overlaps with this one; test the function directly so
    // neither layer can quietly rely on the other.
    const good = exportPair("observation", benignSource());
    for (const sql of ["CREATE TABLE sqliteXleak (v TEXT)", 'CREATE INDEX "/home/CANARYUSER/i" ON file_claims (claimed_at)']) {
      const dest = tamperedCopy(good, (db) => db.exec(sql));
      const db = new Database(dest, { readonly: true });
      try {
        expect(checkObservationInvariants(db).length, sql).toBeGreaterThan(0);
      } finally {
        db.close();
      }
    }
  });

  it("the producer's own sqlite_autoindex_* objects are tolerated (the control for the internal-object rule)", () => {
    const good = exportPair("observation", benignSource());
    const db = new Database(good, { readonly: true });
    const internal = (db.prepare("SELECT name FROM sqlite_master WHERE substr(name, 1, 7) = 'sqlite_'").all() as Array<{ name: string }>).map((r) => r.name);
    db.close();
    expect(internal.length).toBeGreaterThan(0); // not vacuous: there ARE internal objects, and the pair verifies
    expect(internal.every((n) => n.startsWith("sqlite_autoindex_"))).toBe(true);
    expect(verifySnapshotPair(good, { expectProfile: "observation" }).ok).toBe(true);
  });

  it.each([
    ["ANALYZE alone (sqlite_stat1 exists)", (db: Database.Database) => db.exec("ANALYZE")],
    [
      "sqlite_stat1 rows carrying free text",
      (db: Database.Database) => db.exec("ANALYZE; INSERT INTO sqlite_stat1 (tbl, idx, stat) VALUES ('/home/CANARYUSER/stat-canary', 'x', 'free text')"),
    ],
  ])("%s is refused: the producer never emits sqlite_stat*, and its rows are free text", (_n, mutate) => {
    const good = exportPair("observation", benignSource());
    const dest = tamperedCopy(good, mutate as (db: Database.Database) => void);
    const v = verifySnapshotPair(dest, { expectProfile: "observation" });
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(/unexpected internal object "sqlite_stat1"/);
    expect(v.problems.join(" ")).not.toContain("CANARYUSER");
  });

  it("a SOURCE registry with a lookalike-named view is refused, not exported", () => {
    const src = benignSource();
    const db = new Database(src);
    db.exec("CREATE VIEW sqliteXview AS SELECT 1 AS v");
    db.close();
    for (const profile of ["full", "observation"] as const) {
      let refused = false;
      try {
        exportRegistrySnapshot({ profile, source: src, dest: join(out, `${profile}.db`) });
      } catch (e) {
        refused = (e as Error).name === "SnapshotError";
      }
      expect(refused, profile).toBe(true);
      expect(existsSync(join(out, `${profile}.db`))).toBe(false);
    }
  });

  it("a FULL snapshot cannot masquerade as an observation, even with a resealed, relabelled sidecar", () => {
    const sensitive = join(work, "full-src.db");
    const db0 = initDb(sensitive);
    addSession(db0, "sess-1", ["src/a.rs"]);
    db0.prepare("INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES ('sess-1','sess-1','k',?,1)").run(
      "CANARY-mailbox-body"
    );
    db0.close();
    const full = exportPair("full", sensitive, "full.db");
    expect(verifySnapshotPair(full).profile).toBe("full");
    const forged = tamperedCopy(full, () => {}, (m) => {
      m.profile = "observation";
      m.disclosure.classification = "least-privilege-observation";
      m.disclosure.policy = "observation-v1";
    });
    const v = verifySnapshotPair(forged);
    expect(v.ok).toBe(false);
    expect(v.profile).toBe("observation");
    expect(v.problems.join(" ")).toMatch(/unexpected table "mailbox"/);
  });
});

// ── C. bounded verdicts for hostile file types ─────────────────────────────────

interface ChildResult {
  timedOut: boolean;
  status: number | null;
  verdict: { ok: boolean; problems: string[] } | null;
  stderr: string;
}
function verifyInChild(dest: string, expectProfile?: string): ChildResult {
  const r = spawnSync(
    process.execPath,
    [
      "--experimental-strip-types",
      "--import",
      join(HERE, "fixtures/ts-resolve-register.mjs"),
      "--no-warnings",
      join(HERE, "fixtures/verify-child.ts"),
      dest,
      ...(expectProfile ? [expectProfile] : []),
    ],
    { encoding: "utf-8", timeout: 15_000, killSignal: "SIGKILL" }
  );
  const timedOut = r.error !== undefined && (r.error as NodeJS.ErrnoException).code === "ETIMEDOUT";
  let verdict: ChildResult["verdict"] = null;
  try {
    verdict = JSON.parse(r.stdout.trim().split("\n").pop() ?? "");
  } catch {
    /* leave null */
  }
  return { timedOut, status: r.status, verdict, stderr: r.stderr };
}

describe("C. verifySnapshotPair returns a bounded negative verdict for hostile inputs", () => {
  const goodPair = () => exportPair("observation", benignSource());
  const expectBoundedRefusal = (r: ChildResult, why: string) => {
    expect(r.timedOut, `${why}: verifier hung`).toBe(false);
    expect(r.status, `${why}: verifier threw: ${r.stderr.slice(0, 300)}`).toBe(0);
    expect(r.verdict?.ok, why).toBe(false);
    expect(r.verdict?.problems.length, why).toBeGreaterThan(0);
  };

  itChild("control: the child runner verifies a good pair", () => {
    const r = verifyInChild(goodPair());
    expect(r.status, r.stderr).toBe(0);
    expect(r.verdict).toMatchObject({ ok: true });
  });

  itChild("a directory where the database should be (sidecar intact)", () => {
    const dest = goodPair();
    rmSync(dest);
    mkdirSync(dest);
    expectBoundedRefusal(verifyInChild(dest), "directory");
  });

  itChild("a FIFO where the database should be (sidecar intact)", () => {
    const dest = goodPair();
    rmSync(dest);
    execFileSync("mkfifo", [dest]);
    expectBoundedRefusal(verifyInChild(dest), "fifo database");
  });

  itChild("a FIFO where the sidecar should be", () => {
    const dest = goodPair();
    rmSync(`${dest}.meta.json`);
    execFileSync("mkfifo", [`${dest}.meta.json`]);
    expectBoundedRefusal(verifyInChild(dest), "fifo sidecar");
  });

  itChild("a symlink to a directory where the database should be", () => {
    const dest = goodPair();
    const target = join(out, "somedir");
    mkdirSync(target);
    rmSync(dest);
    execFileSync("ln", ["-s", target, dest]);
    expectBoundedRefusal(verifyInChild(dest), "symlink to directory");
  });

  itChild("a symlink to a real file where the database should be", () => {
    const dest = goodPair();
    const real = join(out, "real.bin");
    copyFileSync(dest, real);
    rmSync(dest);
    execFileSync("ln", ["-s", real, dest]);
    expectBoundedRefusal(verifyInChild(dest), "symlink to file");
  });

  itChild("an unreadable database (mode 000)", () => {
    const dest = goodPair();
    chmodSync(dest, 0o000);
    try {
      expectBoundedRefusal(verifyInChild(dest), "unreadable");
    } finally {
      chmodSync(dest, 0o600);
    }
  });

  itChild("a directory where the sidecar should be", () => {
    const dest = goodPair();
    rmSync(`${dest}.meta.json`);
    mkdirSync(`${dest}.meta.json`);
    expectBoundedRefusal(verifyInChild(dest), "sidecar directory");
  });
});

// ── D. a consumer can require the profile it expects ───────────────────────────

describe("D. expectProfile", () => {
  it("a valid FULL pair is refused when an observation is expected, and vice versa", () => {
    const src = benignSource();
    const full = exportPair("full", src, "full.db");
    const obs = exportPair("observation", src, "obs.db");
    expect(verifySnapshotPair(full, { expectProfile: "observation" })).toMatchObject({ ok: false, profile: "full" });
    expect(verifySnapshotPair(full, { expectProfile: "observation" }).problems.join(" ")).toMatch(/expected profile observation/);
    expect(verifySnapshotPair(obs, { expectProfile: "full" }).ok).toBe(false);
    expect(verifySnapshotPair(obs, { expectProfile: "observation" }).ok).toBe(true);
    expect(verifySnapshotPair(full, { expectProfile: "full" }).ok).toBe(true);
  });

  it("omitting it keeps the previous behaviour (no compatibility break)", () => {
    const src = benignSource();
    expect(verifySnapshotPair(exportPair("full", src, "full.db")).ok).toBe(true);
    expect(verifySnapshotPair(exportPair("observation", src, "obs.db")).ok).toBe(true);
  });
});

// ── E. no copied column can be missing a value rule ────────────────────────────

describe("E. every column the policy copies has a value rule", () => {
  const copied: Array<[string, string]> = [
    ...OBSERVATION_POLICY.schema_version.columns.map((c): [string, string] => ["schema_version", c]),
    ...OBSERVATION_POLICY.sessions.verbatim.map((c): [string, string] => ["sessions", c]),
    ...OBSERVATION_POLICY.file_claims.columns.filter((c) => c !== "file_path").map((c): [string, string] => ["file_claims", c]),
  ];
  it.each(copied)("%s.%s is governed (a new copied column without a rule fails here)", (t, c) => {
    // NULL is always allowed, so probe with a value that no rule can accept: a path.
    const why = observationValueProblem(t, c, "/definitely/not/disclosable");
    expect(why, `${t}.${c} has no value rule`).not.toMatch(/is not a copied observation column/);
    expect(why, `${t}.${c} accepted a filesystem path`).not.toBeNull();
    expect(why).not.toContain("/definitely"); // the reason never echoes the value
  });
  it("an unknown column is reported as such, not silently allowed", () => {
    expect(observationValueProblem("sessions", "brand_new_column", "x")).toMatch(/not a copied observation column/);
  });
});

// ── F. the CLI exposes the same check ──────────────────────────────────────────

describe("F. snapshot-export --verify --expect-profile", () => {
  const cli = (...args: string[]) =>
    spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--import", join(HERE, "fixtures/ts-resolve-register.mjs"), "--no-warnings", join(HERE, "../cli/snapshot-export.ts"), ...args],
      { encoding: "utf-8", timeout: 30_000 }
    );

  itChild("exit 0 when the profile matches, 1 with the verdict on stdout when it does not", () => {
    const src = benignSource();
    const full = exportPair("full", src, "full.db");
    const obs = exportPair("observation", src, "obs.db");
    expect(cli("--verify", obs, "--expect-profile", "observation").status).toBe(0);
    expect(cli("--verify", full).status).toBe(0); // unchanged default
    const bad = cli("--verify", full, "--expect-profile", "observation");
    expect(bad.status).toBe(1);
    expect(JSON.parse(bad.stdout)).toMatchObject({ ok: false, profile: "full" });
  });

  itChild("usage errors (exit 2): bad value, and the flag outside --verify", () => {
    const src = benignSource();
    const obs = exportPair("observation", src, "obs.db");
    expect(cli("--verify", obs, "--expect-profile", "everything").status).toBe(2);
    expect(cli("--profile", "observation", "--source", src, "--dest", join(out, "x.db"), "--expect-profile", "observation").status).toBe(2);
    expect(existsSync(join(out, "x.db"))).toBe(false); // a usage error exports nothing
  });
});
