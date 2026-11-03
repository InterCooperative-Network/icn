// Adversarial / failure-path tests for the snapshot exporter.
//
// Each block starts from a hypothesis about how the exporter could be made to lie, leak or
// destroy something, and checks it against a real failure — a real SIGKILL, a real file swapped
// under it, a real hostile schema — rather than asserting on implementation text.

import { spawnSync } from "child_process";
import { chmodSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, statSync, symlinkSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import Database from "better-sqlite3";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { initDb } from "../state/db.js";
import { exportRegistrySnapshot, verifySnapshotPair, type SnapshotOptions } from "../state/snapshot-export.js";
import {
  addSession,
  CANARY,
  canariesInFile,
  expectRefusal,
  listing,
  logicalDump,
  makeRegistry,
  makeSensitiveRegistry,
  removeDir,
  sha256File,
  tempDir,
} from "./fixtures/snapshot-helpers.js";

const HERE = dirname(fileURLToPath(import.meta.url));
// The hard-kill tests run the TypeScript sources in a child via Node's type stripping (22.6+).
// `engines` still permits Node 20, where these cannot run: skip VISIBLY there, never fail or pass.
const [NODE_MAJOR, NODE_MINOR] = process.versions.node.split(".").map(Number) as [number, number];
const CAN_STRIP_TYPES = NODE_MAJOR > 22 || (NODE_MAJOR === 22 && NODE_MINOR >= 6);
const itKill = it.skipIf(!CAN_STRIP_TYPES);
const profiles = ["full", "observation"] as const;

let dir: string;
beforeEach(() => {
  dir = tempDir("icn-snapshot-hard-");
});
afterEach(() => {
  try {
    chmodSync(dir, 0o700);
  } catch {
    /* gone */
  }
  removeDir(dir);
});

const run = (profile: (typeof profiles)[number], o: Omit<SnapshotOptions, "profile">) =>
  exportRegistrySnapshot({ profile, ...o });

describe.each(profiles)("option validation (%s)", (profile) => {
  it.each([
    ["NaN", Number.NaN],
    ["negative", -1],
    ["fractional", 1.5],
    ["Infinity", Number.POSITIVE_INFINITY],
    ["absurdly large", 1e20],
    ["a string", "5" as unknown as number],
  ])("rejects busyTimeoutMs = %s as an option error, not a SQLite error", (_n, v) => {
    const src = makeRegistry(dir);
    const before = listing(dir);
    expectRefusal("INVALID_OPTION", () => run(profile, { source: src, dest: join(dir, "o.db"), busyTimeoutMs: v }));
    expect(listing(dir)).toEqual(before);
  });

  it.each([
    ["maxSourceBytes", { maxSourceBytes: -5 }],
    ["maxSourceBytes", { maxSourceBytes: Number.NaN }],
    ["reserveFreeBytes", { reserveFreeBytes: -1 }],
  ])("rejects a bad %s", (_n, extra) => {
    const src = makeRegistry(dir);
    expectRefusal("INVALID_OPTION", () => run(profile, { source: src, dest: join(dir, "o.db"), ...extra }));
  });
});

describe.each(profiles)("a registry that is not what its version stamp claims (%s)", (profile) => {
  it("refuses a v5-stamped registry that is missing a column the contract needs", () => {
    const src = makeRegistry(dir);
    const w = new Database(src);
    w.exec("ALTER TABLE sessions DROP COLUMN agent_pid");
    w.close();
    const before = listing(dir);
    const e = expectRefusal("UNSUPPORTED_SCHEMA", () => run(profile, { source: src, dest: join(dir, "o.db") }));
    expect(e.message).toContain("agent_pid");
    expect(listing(dir)).toEqual(before);
  });

  it("refuses a registry carrying a trigger, a view or a virtual table", () => {
    for (const [label, ddl] of [
      ["trigger", "CREATE TRIGGER t_evil AFTER INSERT ON sessions BEGIN SELECT 1; END"],
      ["view", "CREATE VIEW v_evil AS SELECT * FROM sessions"],
      ["virtual", "CREATE VIRTUAL TABLE fts_evil USING fts5(body)"],
    ] as const) {
      const src = makeRegistry(dir, `${label}.db`);
      const w = new Database(src);
      try {
        w.exec(ddl);
      } catch {
        w.close();
        continue; // this SQLite build lacks the module; the other cases still run
      }
      w.close();
      const e = expectRefusal("NOT_A_REGISTRY", () => run(profile, { source: src, dest: join(dir, `${label}.out`) }));
      expect(e.message).toMatch(/trigger|view|virtual/i);
      expect(existsSync(join(dir, `${label}.out`))).toBe(false);
    }
  });
});

describe.each(profiles)("the source is replaced while the export runs (%s)", (profile) => {
  it("refuses if the file is swapped between inspection and open", () => {
    const src = makeRegistry(dir);
    const other = makeRegistry(dir, "other.db");
    const before = listing(dir);
    expectRefusal("SOURCE_CHANGED", () =>
      run(profile, {
        source: src,
        dest: join(dir, "o.db"),
        hooks: { beforeSourceOpen: () => renameSync(other, src) },
      })
    );
    expect(listing(dir).filter((n) => n.startsWith("o.db"))).toEqual([]);
    expect(before.length).toBeGreaterThan(0);
  });

  it("refuses if the file is swapped during the copy, so provenance never describes another file", () => {
    const src = makeRegistry(dir);
    const other = makeRegistry(dir, "other.db");
    expectRefusal("SOURCE_CHANGED", () =>
      run(profile, {
        source: src,
        dest: join(dir, "o.db"),
        hooks: { afterCopy: () => renameSync(other, src) },
      })
    );
    expect(readdirSync(dir).filter((n) => n.startsWith("o.db") || n.endsWith(".partial"))).toEqual([]);
  });
});

describe.each(profiles)("resource limits (%s)", (profile) => {
  it("refuses a source over maxSourceBytes", () => {
    const src = makeRegistry(dir);
    const before = sha256File(src);
    expectRefusal("SOURCE_TOO_LARGE", () => run(profile, { source: src, dest: join(dir, "o.db"), maxSourceBytes: 1024 }));
    expect(sha256File(src)).toBe(before);
    expect(readdirSync(dir).some((n) => n.startsWith("o.db") || n.endsWith(".partial"))).toBe(false);
  });

  it("counts the WAL: a small main file with a large WAL is still large", () => {
    const src = makeRegistry(dir);
    const w = initDb(src);
    w.pragma("wal_autocheckpoint = 0");
    const pin = new Database(src, { readonly: true });
    pin.exec("BEGIN");
    pin.prepare("SELECT COUNT(*) FROM sessions").get();
    for (let i = 0; i < 300; i++) addSession(w, `s${i}`, [`f${i}-${"x".repeat(300)}`]);
    const main = statSync(src).size;
    const wal = statSync(`${src}-wal`).size;
    expect(wal).toBeGreaterThan(main);
    // Cap sits between the two: main alone passes, main + WAL must not.
    expectRefusal("SOURCE_TOO_LARGE", () =>
      run(profile, { source: src, dest: join(dir, "o.db"), maxSourceBytes: main + Math.floor(wal / 2) })
    );
    pin.exec("COMMIT");
    pin.close();
    w.close();
  });

  it("refuses when the destination filesystem cannot hold the work, before writing anything", () => {
    const src = makeRegistry(dir);
    expectRefusal("INSUFFICIENT_SPACE", () =>
      run(profile, { source: src, dest: join(dir, "o.db"), reserveFreeBytes: Number.MAX_SAFE_INTEGER })
    );
    expect(readdirSync(dir).some((n) => n.startsWith("o.db") || n.endsWith(".partial"))).toBe(false);
  });
});

describe.each(profiles)("hostile path spellings (%s)", (profile) => {
  const spellings = [
    "it's.db",
    'say "hi".db',
    "semi;colon.db",
    "back`tick.db",
    "dash--comment.db",
    "pct%20and?query#frag.db",
    "ünï©ödé-名前.db",
    "with space.db",
    "new\nline.db",
    "$(touch pwned).db",
  ];

  it.each(spellings)("exports to and from %j and the result verifies", (name) => {
    const srcDir = join(dir, name.replace(/\.db$/, ".d"));
    mkdirSync(srcDir);
    const src = makeRegistry(srcDir, name);
    const w = initDb(src);
    addSession(w, "s1", ["a.ts"]);
    w.close();
    const dest = join(dir, `out-${name}`);
    run(profile, { source: src, dest });
    expect(verifySnapshotPair(dest)).toMatchObject({ ok: true });
    expect(existsSync(join(dir, "pwned"))).toBe(false);
  });

  it("fails closed, not with a raw OS error, when the temp name would exceed NAME_MAX", () => {
    const src = makeRegistry(dir);
    const name = "a".repeat(250);
    let err: unknown;
    try {
      run(profile, { source: src, dest: join(dir, name) });
    } catch (e) {
      err = e;
    }
    // Either it fits, or it is refused as a SnapshotError — never an uncaught ENAMETOOLONG.
    if (err) expect((err as Error).name).toBe("SnapshotError");
    expect(readdirSync(dir).some((n) => n.endsWith(".partial"))).toBe(false);
  });
});

describe.each(profiles)("filesystem placement (%s)", (profile) => {
  it("follows a symlinked source but never writes through a symlinked destination file", () => {
    const real = makeRegistry(dir, "real.db");
    const link = join(dir, "link.db");
    symlinkSync(real, link);
    const dest = join(dir, "o.db");
    run(profile, { source: link, dest });
    expect(verifySnapshotPair(dest).ok).toBe(true);
    expectRefusal("DEST_EXISTS", () => run(profile, { source: real, dest: link }));
  });

  it.skipIf(!existsSync("/dev/shm"))("works across filesystems (temp files live beside the destination)", () => {
    const shm = tempDirIn("/dev/shm");
    try {
      const src = makeRegistry(dir);
      if (statSync(shm).dev === statSync(dir).dev) return; // not actually cross-device here
      const dest = join(shm, "o.db");
      run(profile, { source: src, dest });
      expect(verifySnapshotPair(dest).ok).toBe(true);
    } finally {
      removeDir(shm);
    }
  });
});

function tempDirIn(base: string): string {
  const p = join(base, `icn-snapshot-xdev-${process.pid}-${Date.now()}`);
  mkdirSync(p, { mode: 0o700 });
  return p;
}

describe.each(profiles)("a real SIGKILL mid-export (%s)", (profile) => {
  const child = (src: string, dest: string, killAt?: string) =>
    spawnSync(
      process.execPath,
      [
        "--experimental-strip-types",
        "--import",
        join(HERE, "fixtures/ts-resolve-register.mjs"),
        "--no-warnings",
        join(HERE, "fixtures/snapshot-child.ts"),
        profile,
        src,
        dest,
        ...(killAt ? [killAt] : []),
      ],
      { encoding: "utf-8", timeout: 60_000 }
    );
  const partials = () => readdirSync(dir).filter((n) => n.endsWith(".partial"));

  itKill("the child runner itself works (control) and publishes 0600 files even with umask 0", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "ok.db");
    const r = child(src, dest);
    expect(r.status, r.stderr).toBe(0);
    expect(statSync(dest).mode & 0o777).toBe(0o600);
    expect(statSync(`${dest}.meta.json`).mode & 0o777).toBe(0o600);
    expect(verifySnapshotPair(dest).ok).toBe(true);
  });

  itKill.each(["afterCopy", "afterVerify"])("killed at %s: no final name exists, leftovers are owner-only", (stage) => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "o.db");
    const r = child(src, dest, stage);
    expect(r.signal).toBe("SIGKILL");

    expect(existsSync(dest)).toBe(false);
    expect(existsSync(`${dest}.meta.json`)).toBe(false);
    expect(verifySnapshotPair(dest).ok).toBe(false);
    const left = partials();
    expect(left.length).toBeGreaterThan(0);
    for (const n of left) expect(statSync(join(dir, n)).mode & 0o777).toBe(0o600); // umask was 0

    if (profile === "observation") {
      // The streaming projection means withheld data never reached disk, even here.
      for (const n of left) expect(canariesInFile(join(dir, n), Object.values(CANARY))).toEqual([]);
    }
    // A retry is not poisoned by the debris.
    const again = run(profile, { source: src, dest });
    expect(verifySnapshotPair(again.dest).ok).toBe(true);
  });

  itKill("killed between the two publishes: a lone database is DETECTED as incomplete and never overwritten", () => {
    const src = makeSensitiveRegistry(dir);
    const dest = join(dir, "o.db");
    const r = child(src, dest, "afterPublishDb");
    expect(r.signal).toBe("SIGKILL");

    expect(existsSync(dest)).toBe(true);
    expect(existsSync(`${dest}.meta.json`)).toBe(false);
    const v = verifySnapshotPair(dest);
    expect(v.ok).toBe(false);
    expect(v.problems.join(" ")).toMatch(/sidecar is missing.*(incomplete|interrupted)/i);

    const bytes = readFileSync(dest);
    expectRefusal("DEST_EXISTS", () => run(profile, { source: src, dest })); // fail closed, don't "repair"
    expect(readFileSync(dest).equals(bytes)).toBe(true);
  });
});

describe("a projection that loses rows is discarded, not published", () => {
  it("observation: a row missing from the copy is caught against the source's same-transaction count", () => {
    const src = makeSensitiveRegistry(dir);
    const before = listing(dir);
    const e = expectRefusal("SNAPSHOT_VERIFY_FAILED", () =>
      run("observation", {
        source: src,
        dest: join(dir, "o.db"),
        hooks: {
          afterCopy: (tmp) => {
            const d = new Database(tmp);
            d.prepare("DELETE FROM file_claims WHERE rowid = (SELECT MIN(rowid) FROM file_claims)").run();
            d.close();
          },
        },
      })
    );
    expect(e.message).toMatch(/file_claims.*source had/);
    expect(listing(dir)).toEqual(before);
  });
});

describe("the source is never altered, whatever happens", () => {
  it.each(profiles)("%s: logical content and main-file bytes survive refusals and injected failures", (profile) => {
    const src = makeSensitiveRegistry(dir);
    const main = sha256File(src);
    const dump = logicalDump(src);
    for (const hooks of [
      { afterCopy: () => { throw new Error("x"); } },
      { afterVerify: () => { throw new Error("x"); } },
    ]) {
      expect(() => run(profile, { source: src, dest: join(dir, "o.db"), hooks })).toThrow("x");
    }
    expectRefusal("DEST_PARENT_MISSING", () => run(profile, { source: src, dest: join(dir, "nope", "o.db") }));
    expect(sha256File(src)).toBe(main);
    expect(logicalDump(src)).toBe(dump);
  });
});
