// CONCURRENCY, WITH REAL OPERATING-SYSTEM PROCESSES.
//
// Nothing here can be demonstrated in-process. The defects these pin were all cross-process:
// several MCP server processes share one database file on this VM, and the hook CLI is a
// THIRD writer that runs out of the agent's own worktree.
//
//   - 9 of 40 parallel initDb calls on a fresh database threw `duplicate column name`, and
//     the server's main().catch(exit 1) turns that into "the server does not start".
//   - two hook subprocesses firing for the same conversation both saw no row, both inserted,
//     and the loser died with SQLITE_CONSTRAINT_UNIQUE — unregistered, which is the exact
//     outcome idempotency exists to prevent.
//
// The suite compiles the runtime once into a temp directory rather than depending on `dist/`,
// so it neither requires a prior build nor silently skips when one is missing.

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { execFileSync, spawn, spawnSync } from "child_process";
import { mkdtempSync, readdirSync, rmSync, symlinkSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";

let out: string;
let work: string;

beforeAll(() => {
  work = mkdtempSync(join(tmpdir(), "icn-conc-"));
  out = join(work, "build");
  // A build failure must FAIL this suite, never skip it — a skipped concurrency test reads as
  // "concurrency is fine".
  execFileSync("npx", ["tsc", "--outDir", out], { cwd: process.cwd(), stdio: "pipe" });
  // Node resolves dependencies by walking UP from the importing file, so a build parked in
  // /tmp cannot see ops/mcp/node_modules. One symlink at the temp root is enough.
  symlinkSync(join(process.cwd(), "node_modules"), join(work, "node_modules"));
}, 120_000);
afterAll(() => rmSync(work, { recursive: true, force: true }));

/**
 * Run N copies of a script that are GENUINELY simultaneous.
 *
 * This used `spawnSync` in a loop, which BLOCKS until each child exits — so the "stampede" ran
 * strictly one process after another and contended with nothing. Both mutants that should have
 * been caught (dropping the ladder transaction, dropping `.immediate()`) sailed through it.
 * `spawn` + Promise.all is what actually overlaps them; the shared wall-clock start is what
 * makes them collide rather than merely overlap.
 */
async function stampede(
  n: number,
  body: string
): Promise<Array<{ code: number | null; stdout: string; stderr: string }>> {
  const script = join(work, `job-${Math.random().toString(36).slice(2)}.mjs`);
  writeFileSync(script, body);
  const startAt = Date.now() + 1500;
  return Promise.all(
    Array.from({ length: n }, (_, i) => {
      return new Promise<{ code: number | null; stdout: string; stderr: string }>((resolve) => {
        const p = spawn(process.execPath, [script, String(startAt), String(i)], {
          stdio: ["ignore", "pipe", "pipe"],
        });
        let stdout = "";
        let stderr = "";
        p.stdout.on("data", (d) => (stdout += d));
        p.stderr.on("data", (d) => (stderr += d));
        p.on("close", (code) => resolve({ code, stdout, stderr }));
      });
    })
  );
}

const PRELUDE = (outDir: string, db: string) => `
import { initDb } from ${JSON.stringify(join(outDir, "state/db.js"))};
import { registerSession, releaseSession } from ${JSON.stringify(join(outDir, "runtime/session-runtime.js"))};
const startAt = Number(process.argv[2]);
const idx = process.argv[3];
while (Date.now() < startAt) { /* spin to a common start */ }
const DB = ${JSON.stringify(db)};
`;

describe("concurrent cold start", () => {
  it("survives many simultaneous first-opens of the SAME new database", async () => {
    const db = join(work, "cold.db");
    const results = await stampede(
      16,
      PRELUDE(out, db) +
        `
try { const d = initDb(DB); d.close(); console.log("OK"); }
catch (e) { console.error("FAIL " + (e && e.code) + " " + (e && e.message)); process.exit(1); }
`
    );
    const failures = results.filter((r) => r.code !== 0);
    expect(failures.map((f) => f.stderr.trim())).toEqual([]);
    expect(results.filter((r) => r.stdout.includes("OK"))).toHaveLength(16);
  }, 120_000);

  it("leaves exactly one stamp per schema version, and a coherent schema", async () => {
    const db = join(work, "cold2.db");
    // ASSERT ON THE STAMPEDE. This discarded its return value, so the concurrent opens could
    // all be dying and the test still passed — the probe below calls initDb() itself, which
    // re-establishes every property it then checks.
    const opens = await stampede(
      12,
      PRELUDE(out, db) + `const d = initDb(DB); d.close(); console.log("OK");`
    );
    expect(opens.filter((r) => r.code !== 0).map((f) => f.stderr.trim())).toEqual([]);
    expect(opens.filter((r) => r.stdout.includes("OK"))).toHaveLength(12);
    const probe = join(work, "probe.mjs");
    writeFileSync(
      probe,
      `
import { initDb } from ${JSON.stringify(join(out, "state/db.js"))};
const d = initDb(${JSON.stringify(db)});
const rows = d.prepare("SELECT version, COUNT(*) c FROM schema_version GROUP BY version").all();
const dupes = rows.filter((r) => r.c !== 1);
const cols = d.prepare("PRAGMA table_info(sessions)").all().map((c) => c.name);
console.log(JSON.stringify({ dupes, hasWorktreeId: cols.includes("worktree_id"),
  hasGeneration: cols.includes("worktree_generation"),
  integrity: d.pragma("integrity_check", { simple: true }) }));
`
    );
    const r = spawnSync(process.execPath, [probe], { encoding: "utf-8", timeout: 60_000 });
    expect(r.status).toBe(0);
    const info = JSON.parse(r.stdout.trim());
    expect(info.dupes).toEqual([]);
    expect(info.hasWorktreeId).toBe(true);
    expect(info.hasGeneration).toBe(true);
    expect(info.integrity).toBe("ok");
  }, 120_000);
});

describe("concurrent registration", () => {
  it("gives ONE row and exactly one creator for one conversation id", async () => {
    const db = join(work, "reg.db");
    // Migrate first so this test isolates the registration race from the ladder race.
    spawnSync(process.execPath, ["-e",
      `import(${JSON.stringify(join(out, "state/db.js"))}).then(m => m.initDb(${JSON.stringify(db)}).close())`,
    ], { encoding: "utf-8" });

    const results = await stampede(
      16,
      PRELUDE(out, db) +
        `
try {
  const d = initDb(DB);
  const r = registerSession(d, { repo: "icn", provider_session_id: "one-conversation" });
  console.log(JSON.stringify({ created: r.created, id: r.session_id }));
  d.close();
} catch (e) { console.error("FAIL " + (e && e.code) + " " + (e && e.message)); process.exit(1); }
`
    );
    expect(results.filter((r) => r.code !== 0).map((f) => f.stderr.trim())).toEqual([]);
    const parsed = results.map((r) => JSON.parse(r.stdout.trim()));
    expect(parsed.filter((p) => p.created)).toHaveLength(1);
    expect(new Set(parsed.map((p) => p.id)).size).toBe(1);
  }, 120_000);

  it("register racing release never leaves a caller believing a deleted row is theirs", async () => {
    const db = join(work, "relrace.db");
    spawnSync(process.execPath, ["-e",
      `import(${JSON.stringify(join(out, "state/db.js"))}).then(m => m.initDb(${JSON.stringify(db)}).close())`,
    ], { encoding: "utf-8" });

    // Half register, half release, all at once, repeatedly on the same conversation id.
    const results = await stampede(
      16,
      PRELUDE(out, db) +
        `
try {
  const d = initDb(DB);
  if (Number(idx) % 2 === 0) {
    const r = registerSession(d, { repo: "icn", provider_session_id: "raced" });
    console.log(JSON.stringify({ op: "register", id: r.session_id, created: r.created }));
  } else {
    const row = d.prepare("SELECT id FROM sessions WHERE provider_session_id = ?").get("raced");
    const res = row ? releaseSession(d, row.id) : { released: false };
    console.log(JSON.stringify({ op: "release", released: res.released }));
  }
  d.close();
} catch (e) { console.error("FAIL " + (e && e.code) + " " + (e && e.message)); process.exit(1); }
`
    );
    // The invariant is NOT "a particular interleaving happened" — it is that nobody crashed and
    // the table never ends up with two live activations for one conversation.
    expect(results.filter((r) => r.code !== 0).map((f) => f.stderr.trim())).toEqual([]);
    const probe = join(work, "count.mjs");
    writeFileSync(
      probe,
      `
import { initDb } from ${JSON.stringify(join(out, "state/db.js"))};
const d = initDb(${JSON.stringify(db)});
console.log(String(d.prepare("SELECT COUNT(*) c FROM sessions WHERE provider_session_id = ?").get("raced").c));
`
    );
    const r = spawnSync(process.execPath, [probe], { encoding: "utf-8", timeout: 60_000 });
    expect(Number(r.stdout.trim())).toBeLessThanOrEqual(1);
  }, 120_000);
});

// ── A1: AVAILABILITY under contention (a different property from atomicity) ──
//
// A5 proves the migration ladder is ATOMIC. This proves something else entirely: that opening
// an ALREADY-CURRENT registry does not require a write lock at all, and that a genuine cold
// start waits out transient contention instead of dying instantly.
//
// The shipped defect was `db.pragma("journal_mode = WAL")` before `busy_timeout`, plus
// `db.transaction(migrate).immediate()` on EVERY open. SQLite does not run the busy handler for
// a journal-mode change, so initDb raised SQLITE_BUSY in ~27 ms whatever was going on — and
// because the migration transaction ran unconditionally, even a read-only `classify` took an
// EXCLUSIVE write lock on a database several MCP processes share. The CLI turned that into
// "registry unavailable", exit 1, session never registered.
//
// NOTE ON MUTANT SELECTION: the migration transaction's ATOMICITY is proven deterministically
// by the rollback test in schema-upgrade.test.ts, not by racing for it. These tests cover the
// other half — that concurrent cold opens neither crash nor corrupt — and they do detect a
// missing transaction now that the stampede's results are actually asserted (`duplicate column
// name` from the losers). One test is not made responsible for two properties merely because
// both involve SQLite.
describe("registry availability under genuine contention", () => {
  /** Hold a real write lock in a SEPARATE process; resolves once the lock is actually held. */
  function holdWriteLock(db: string, holdMs: number): { ready: Promise<void>; done: Promise<void> } {
    const script = join(work, `hold-${Math.random().toString(36).slice(2)}.mjs`);
    writeFileSync(
      script,
      `
import Database from "better-sqlite3";
const d = new Database(${JSON.stringify(db)});
d.pragma("busy_timeout = 30000");
d.exec("CREATE TABLE IF NOT EXISTS _lockprobe (x)");
d.exec("BEGIN IMMEDIATE");
d.prepare("INSERT INTO _lockprobe VALUES (1)").run();
console.log("HELD");
setTimeout(() => { d.exec("COMMIT"); process.exit(0); }, ${holdMs});
`
    );
    const p = spawn(process.execPath, [script], { stdio: ["ignore", "pipe", "pipe"] });
    let ready!: () => void;
    let done!: () => void;
    const readyP = new Promise<void>((r) => (ready = r));
    const doneP = new Promise<void>((r) => (done = r));
    p.stdout.on("data", (d) => {
      if (String(d).includes("HELD")) ready();
    });
    p.on("close", () => done());
    return { ready: readyP, done: doneP };
  }

  /** Open via the REAL initDb in a separate process; report success and elapsed ms. */
  function openOnce(db: string): Promise<{ ok: boolean; ms: number; err: string }> {
    const script = join(work, `open-${Math.random().toString(36).slice(2)}.mjs`);
    writeFileSync(
      script,
      `
import { initDb } from ${JSON.stringify(join(out, "state/db.js"))};
const t0 = Date.now();
try { const d = initDb(${JSON.stringify(db)}); d.close();
      console.log(JSON.stringify({ ok: true, ms: Date.now() - t0, err: "" })); }
catch (e) { console.log(JSON.stringify({ ok: false, ms: Date.now() - t0,
      err: (e && e.code) + " " + (e && e.message) })); }
`
    );
    return new Promise((resolve) => {
      const p = spawn(process.execPath, [script], { stdio: ["ignore", "pipe", "pipe"] });
      let o = "";
      p.stdout.on("data", (d) => (o += d));
      p.on("close", () => resolve(JSON.parse(o.trim() || '{"ok":false,"ms":-1,"err":"no output"}')));
    });
  }

  it("an ALREADY-CURRENT registry opens while another process holds the write lock", async () => {
    const db = join(work, "current-under-lock.db");
    const r0 = await openOnce(db); // migrate once, uncontended
    expect(r0.ok).toBe(true);

    // Held for LONGER than busy_timeout (5s), so this is pass/fail rather than a timing race:
    // an open that needs the write lock cannot get it and must fail; one that does not, opens.
    const holder = holdWriteLock(db, 9000);
    await holder.ready;
    const r = await openOnce(db);
    expect(r.err).toBe("");
    expect(r.ok).toBe(true);
    await holder.done;
  }, 120_000);

  it("a COLD start waits out transient contention instead of failing instantly", async () => {
    const db = join(work, "cold-under-lock.db");
    // Seed the file so a competing writer can hold a lock on it before any migration exists.
    // 1200 ms against a 5 s busy_timeout leaves ~3.8 s of headroom. At 2500 ms the margin was
    // thin enough that heavy parallel load on the machine could eat it, which would make this
    // test flaky in the one direction that matters least — a false failure.
    const holder = holdWriteLock(db, 1200);
    await holder.ready;
    const r = await openOnce(db);
    expect(r.err).toBe("");
    expect(r.ok).toBe(true);
    // It must have WAITED rather than sailed through — otherwise this proves nothing about
    // contention handling at all.
    expect(r.ms).toBeGreaterThan(300);
    await holder.done;
  }, 120_000);
});

// ── kept P2: a large envelope must not be truncated by process.exit() ───────
//
// `process.exit(main())` discards whatever `process.stdout.write` still has queued when stdout
// is a PIPE, because pipe writes are asynchronous in Node. classify then exited 0 — "facts
// produced" — having emitted a truncated, unparseable envelope. Session rows are only ever
// removed by release, so co-occupants accumulate on a lane indefinitely and the 64 KB pipe
// buffer is reachable in the ordinary course of things.
// ── the generation token must be minted atomically ──────────────────────────
//
// `writeFileSync(file, uuid, { flag: "wx" })` is open(O_CREAT|O_EXCL) followed by a SEPARATE
// write, so a loser that hit EEXIST in the window between them read a ZERO-LENGTH file and got
// null. Measured across 240 genuinely parallel minters: 6 came back null — and a
// NULL-generation row is kept by the lane filter and therefore ADOPTED by a later generation,
// which is the aliasing the token exists to prevent.
describe("lane generation minting under real concurrency", () => {
  it("every concurrent minter gets the SAME non-null token", async () => {
    const repo = join(work, "mintrepo");
    execFileSync("git", ["init", "-q", "-b", "main", repo], { stdio: "pipe" });
    execFileSync("git", ["-C", repo, "-c", "user.email=t@e", "-c", "user.name=t",
                         "commit", "-q", "--allow-empty", "-m", "base"], { stdio: "pipe" });
    const lane = join(work, "mintlane");
    execFileSync("git", ["-C", repo, "worktree", "add", "-q", "-b", "mint", lane], { stdio: "pipe" });

    const results = await stampede(
      24,
      `
import { discoverWorktree } from ${JSON.stringify(join(out, "runtime/worktree-identity.js"))};
const startAt = Number(process.argv[2]);
while (Date.now() < startAt) { /* spin to a common start */ }
const id = discoverWorktree(${JSON.stringify(lane)});
console.log(JSON.stringify({ gen: id ? id.worktree_generation : null }));
`
    );
    expect(results.filter((r) => r.code !== 0).map((f) => f.stderr.trim())).toEqual([]);
    const gens = results.map((r) => JSON.parse(r.stdout.trim()).gen);

    // NOT ONE may be null: a null generation is exactly the row a recreated lane adopts.
    expect(gens.filter((g) => g === null)).toEqual([]);
    // ...and they must all agree, or the "generation" would change per reader and orphan rows.
    expect(new Set(gens).size).toBe(1);
    // No torn or partial token.
    expect(gens[0]).toMatch(/^[0-9a-f-]{36}$/);
    // No temp files left behind in the admin directory.
    const admin = execFileSync("git", ["-C", lane, "rev-parse", "--absolute-git-dir"], {
      encoding: "utf-8",
    }).trim();
    expect(readdirSync(admin).filter((f) => f.includes("icn-lane-generation.tmp"))).toEqual([]);
  }, 120_000);
});

// ── the CLI's --pid must be validated exactly like --pids ──────────────────
//
// It claimed to be, and was not: `Number()` silently reinterprets "0x10" as 16 and "1e3" as
// 1000, so a token that is not a pid at all became some OTHER pid and was published in
// `live_agent_pids` — the field a retirement consumer is told to act on. (pid 16 on this
// machine is a kernel thread.) Only a spawned process can exercise the CLI's own parsing.
describe("CLI --pid validation", () => {
  it("stores only plain decimal pids greater than 1, and NULL for anything else", () => {
    const lane = join(work, "pidlane");
    execFileSync("git", ["init", "-q", "-b", "main", lane], { stdio: "pipe" });
    const cli = join(out, "cli/session.js");

    const check = (raw: string): number | null => {
      const db = join(work, `pid-${Buffer.from(raw).toString("hex")}.db`);
      execFileSync(process.execPath, [cli, "register", "--harness-key", `k-${raw}`,
                                      "--cwd", lane, "--pid", raw, "--quiet"],
                   { env: { ...process.env, ICN_OPS_DB: db }, stdio: "pipe" });
      const probe = join(work, "readpid.mjs");
      writeFileSync(
        probe,
        `import Database from "better-sqlite3";
const d = new Database(process.argv[2]);
const r = d.prepare("SELECT agent_pid AS p FROM sessions").get();
console.log(JSON.stringify(r ? r.p : "NOROW"));`
      );
      return JSON.parse(
        execFileSync(process.execPath, [probe, db], { encoding: "utf-8" }).trim()
      );
    };

    // Silently reinterpreted before the fix: "1e3" -> 1000, "0x10" -> 16.
    expect(check("1e3")).toBeNull();
    expect(check("0x10")).toBeNull();
    expect(check("0")).toBeNull();      // kill(0) signals the caller's process group
    expect(check("-1")).toBeNull();     // kill(-1) signals everything the uid may signal
    expect(check("1")).toBeNull();      // pid 1 is init; it can never be this agent
    expect(check("1.5")).toBeNull();
    expect(check("99999999999999999999")).toBeNull(); // Number() yields a float, not a pid
    // ...and a real pid still survives, or the guard would be indistinguishable from a
    // register that simply never records anything.
    expect(check("12345")).toBe(12345);
  }, 120_000);
});

// ── the CLI classify exit-code contract ────────────────────────────────────
//
// The contract is 0 (facts produced) or 3 (none available), and NOTHING tested it: the CLI is
// executed by exactly one other test, inside a shell pipeline whose status is `cat`'s, so
// classify's own exit code was never observed. Three separate mutants survived — including
// turning an unresolvable lane into exit 0, which the source itself calls "telling a consumer
// an unresolvable path was permission to retire".
describe("CLI classify exit codes", () => {
  const runClassify = (args: string[], db: string): { code: number; out: string } => {
    const cli = join(out, "cli/session.js");
    const r = spawnSync(process.execPath, [cli, "classify", ...args], {
      encoding: "utf-8",
      env: { ...process.env, ICN_OPS_DB: db },
      timeout: 30_000,
    });
    return { code: r.status ?? -1, out: r.stdout ?? "" };
  };

  it("exits 3 with a full envelope when no lane can be resolved", () => {
    const db = join(work, "exit3.db");
    const r = runClassify([], db);
    expect(r.code).toBe(3);
    const parsed = JSON.parse(r.out);
    expect(parsed.state).toBe("REGISTRY-UNAVAILABLE");
    expect(Array.isArray(parsed.live_agent_pids)).toBe(true);
  });

  it("exits 3, never 0, when the registry itself cannot be opened", () => {
    const corrupt = join(work, "corrupt-exit.db");
    writeFileSync(corrupt, Buffer.from([0xde, 0xad, 0xbe, 0xef, 0x00, 0x01, 0x02, 0x03]));
    const r = runClassify(["--worktree-id", "/some/lane"], corrupt);
    expect(r.code).toBe(3);
    const parsed = JSON.parse(r.out); // must still be parseable, not empty stdout
    expect(parsed.state).toBe("REGISTRY-UNAVAILABLE");
    expect(Object.keys(parsed)).toContain("contention");
  });

  it("exits 0 when facts ARE produced, so 3 is not simply always returned", () => {
    const db = join(work, "exit0.db");
    const r = runClassify(["--worktree-id", "/a/lane/that/has/no/rows", "--observed-none"], db);
    expect(r.code).toBe(0);
    expect(JSON.parse(r.out).state).toBe("UNREGISTERED-OBSERVED");
  });
});

describe("classify output survives a pipe", () => {
  it("emits a COMPLETE, parseable envelope larger than the pipe buffer", async () => {
    const db = join(work, "bigenvelope.db");
    const seed = join(work, "seed-big.mjs");
    const WT = "/repo/.git/worktrees/crowded";
    writeFileSync(
      seed,
      `
import { initDb } from ${JSON.stringify(join(out, "state/db.js"))};
const d = initDb(${JSON.stringify(db)});
const ins = d.prepare("INSERT INTO sessions (id, repo, worktree_id, worktree_path, started_at, last_heartbeat) VALUES (?, 'icn', ?, '/w', datetime('now'), datetime('now'))");
const many = d.transaction(() => {
  for (let i = 0; i < 2500; i++) ins.run("session-id-padding-" + String(i).padStart(12, "0"), ${JSON.stringify(WT)});
});
many();
console.log("SEEDED");
`
    );
    const seeded = execFileSync(process.execPath, [seed], { encoding: "utf-8" });
    expect(seeded).toContain("SEEDED");

    // A SHELL pipeline with a genuinely slow reader. Node's own spawn plumbing is the wrong
    // instrument here — the reader has to be slow enough that the 64 KB kernel pipe buffer
    // fills and the writer blocks, which is the only state in which `process.exit()` drops
    // queued output. Measured: 85386 bytes with the fix, 65536 without it.
    const cli = join(out, "cli/session.js");
    const piped = execFileSync(
      "bash",
      [
        "-c",
        `"${process.execPath}" "${cli}" classify --worktree-id "${WT}" --observed-none | { sleep 2; cat; }`,
      ],
      { encoding: "utf-8", env: { ...process.env, ICN_OPS_DB: db }, maxBuffer: 32 * 1024 * 1024 }
    );

    expect(piped.length).toBeGreaterThan(65536);
    const parsed = JSON.parse(piped); // throws if the tail was dropped
    expect(parsed.contention.count).toBe(2500);
    expect(Array.isArray(parsed.live_agent_pids)).toBe(true);
  }, 120_000);
});
