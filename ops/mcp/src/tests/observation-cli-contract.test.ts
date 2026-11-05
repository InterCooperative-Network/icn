// The source-level exporter CLI, as an operator (or Conductor's attended trial) runs it:
//
//   node --experimental-strip-types --no-warnings --import ./src/tests/fixtures/ts-resolve-register.mjs \
//        src/cli/snapshot-export.ts <args>
//
// This path never touches `dist/` (the shared compiled runtime can lag the committed source), so
// the exit-code and output conventions an operator scripts against are pinned here against the
// TypeScript itself:
//
//   --help / -h          exit 2, usage on stderr, nothing on stdout (a usage convention, not a failure to load)
//   export succeeds      exit 0, ONE JSON line on stdout, nothing on stderr
//   export is refused    exit 1, ONE JSON line on stderr {ok:false,code,message}, nothing published
//
// Synthetic registries in a temp directory only. Nothing here opens the real shared registry.

import { spawnSync } from "child_process";
import { existsSync, statSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import Database from "better-sqlite3";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { initDb } from "../state/db.js";
import { addSession, makeRegistry, removeDir, sha256File, tempDir } from "./fixtures/snapshot-helpers.js";

const HERE = dirname(fileURLToPath(import.meta.url));

function cli(...args: string[]) {
  return spawnSync(
    process.execPath,
    [
      "--experimental-strip-types",
      "--no-warnings",
      "--import",
      join(HERE, "fixtures/ts-resolve-register.mjs"),
      join(HERE, "../cli/snapshot-export.ts"),
      ...args,
    ],
    { encoding: "utf-8", timeout: 30_000 }
  );
}

describe("snapshot-export CLI contract (source-level invocation)", () => {
  let work: string;
  let out: string;
  beforeEach(() => {
    work = tempDir("icn-cli-src-");
    out = tempDir("icn-cli-out-");
  });
  afterEach(() => {
    removeDir(work);
    removeDir(out);
  });

  function source(): string {
    const p = makeRegistry(work);
    const db = initDb(p);
    addSession(db, "sess-1", ["src/a.rs"]);
    db.close();
    return p;
  }

  it.each(["--help", "-h"])("%s exits 2 with usage on stderr and nothing on stdout, and exports nothing", (flag) => {
    const r = cli(flag);
    expect(r.status).toBe(2);
    expect(r.stdout).toBe("");
    expect(r.stderr).toMatch(/^usage: snapshot-export --profile <full\|observation> --source <registry\.db> --dest <snapshot\.db>/);
    expect(r.stderr).toContain("--verify <snapshot.db> [--expect-profile <full|observation>]");
  });

  it("a successful observation export exits 0 with one JSON line, and the pair then verifies", () => {
    const dest = join(out, "obs.db");
    const r = cli("--profile", "observation", "--source", source(), "--dest", dest);
    expect(r.status).toBe(0);
    expect(r.stderr).toBe("");

    const lines = r.stdout.split("\n").filter((l) => l !== "");
    expect(lines).toHaveLength(1);
    const doc = JSON.parse(lines[0]!) as Record<string, unknown>;
    expect(Object.keys(doc).sort()).toEqual(["bytes", "dest", "meta_path", "ok", "profile", "schema_version", "sha256"]);
    expect(doc).toMatchObject({ ok: true, profile: "observation", dest, meta_path: `${dest}.meta.json`, schema_version: 5 });
    expect(doc.sha256).toBe(sha256File(dest));
    expect(doc.bytes).toBe(statSync(dest).size);

    // The native SQLite addon really loaded: the published file is a readable observation database.
    const o = new Database(dest, { readonly: true });
    const n = (o.prepare("SELECT count(*) AS n FROM sessions").get() as { n: number }).n;
    o.close();
    expect(n).toBe(1);

    const v = cli("--verify", dest, "--expect-profile", "observation");
    expect(v.status).toBe(0);
    expect(JSON.parse(v.stdout)).toEqual({ ok: true, profile: "observation", problems: [] });
  });

  it("a refused export (timestamp outside the published form) exits 1 with the reason on stderr and publishes nothing", () => {
    const src = source();
    const w = new Database(src);
    w.exec("UPDATE sessions SET last_heartbeat = '2026-01-01T10:05:00Z'");
    w.close();

    const dest = join(out, "obs.db");
    const r = cli("--profile", "observation", "--source", src, "--dest", dest);
    expect(r.status).toBe(1);
    expect(r.stdout).toBe("");
    const lines = r.stderr.split("\n").filter((l) => l !== "");
    expect(lines).toHaveLength(1);
    expect(JSON.parse(lines[0]!)).toMatchObject({ ok: false, code: "SNAPSHOT_VERIFY_FAILED" });
    expect(r.stderr).not.toContain("2026-01-01T10:05:00Z"); // the reason never echoes the value
    expect(existsSync(dest)).toBe(false);
    expect(existsSync(`${dest}.meta.json`)).toBe(false);
  });

  it("verifying a pair against the wrong expected profile exits 1 and says why", () => {
    const dest = join(out, "obs.db");
    expect(cli("--profile", "observation", "--source", source(), "--dest", dest).status).toBe(0);
    const v = cli("--verify", dest, "--expect-profile", "full");
    expect(v.status).toBe(1);
    expect(JSON.parse(v.stdout)).toMatchObject({ ok: false, profile: "observation" });
  });
});
