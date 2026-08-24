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
import { mkdtempSync, rmSync, symlinkSync, writeFileSync } from "fs";
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
    await stampede(12, PRELUDE(out, db) + `const d = initDb(DB); d.close(); console.log("OK");`);
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
// NOTE ON MUTANT SELECTION: removing the migration transaction does NOT fail these tests, and
// that is correct rather than a weakness here — with the pragma order fixed and every ALTER
// already guarded by a column check, the ladder is idempotent enough that concurrency alone no
// longer exposes it. Its necessity is proven where it belongs, by the deterministic rollback
// test in schema-upgrade.test.ts.
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
    const holder = holdWriteLock(db, 2500); // releases well inside the 5s busy_timeout
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
