// scripts/start-stdio.mjs is what the tracked .mcp.json and .cursor/mcp.json run. Opening a
// checkout must never compile into a shared (symlinked) dist or under a running server, and
// stdout must carry only the server's own output.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

const SCRIPT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../scripts/start-stdio.mjs");

// The fixture server prints its own argv[1] (so a test can see what a scanner would see) and exits.
const SERVER = `if (process.env.FX_SLEEP) setTimeout(() => {}, 30000);
else { process.stdout.write(JSON.stringify({ argv1: process.argv[1] }) + "\\n"); process.exit(Number(process.env.FX_EXIT || 0)); }`;

// Fake npm: `run --silent build` records the compile, writes compiler-style noise to STDOUT (a real
// tsc does), then emits the server. FX_BUILD_FAIL makes the build fail.
const FAKE_NPM = `import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
const log = process.env.FX_BUILD_LOG;
appendFileSync(log, process.cwd() + "\\n");
process.stdout.write("src/index.ts(1,1): tsc noise on stdout\\n");
if (process.env.FX_BUILD_FAIL) process.exit(2);
mkdirSync("dist", { recursive: true });
writeFileSync("dist/index.js", ${JSON.stringify(SERVER)});`;

let t: string;
let buildLog: string;
let fakeNpm: string;
const running: ChildProcess[] = [];

function pkg(name: string, built: boolean): string {
  const p = path.join(t, name, "ops", "mcp");
  mkdirSync(p, { recursive: true });
  if (built) {
    mkdirSync(path.join(p, "dist"));
    writeFileSync(path.join(p, "dist", "index.js"), SERVER);
  }
  return p;
}

function start(cwd: string, env: Record<string, string> = {}) {
  const r = spawnSync(process.execPath, [SCRIPT], {
    cwd,
    encoding: "utf8",
    env: { PATH: process.env.PATH ?? "", npm_execpath: fakeNpm, FX_BUILD_LOG: buildLog, ...env },
  });
  const compiles = existsSync(buildLog) ? readFileSync(buildLog, "utf8").split("\n").filter(Boolean) : [];
  return { rc: r.status, stdout: r.stdout, stderr: r.stderr, compiles };
}

function serve(cwd: string, argv: string): ChildProcess {
  const c = spawn(process.execPath, [argv], { cwd, env: { ...process.env, FX_SLEEP: "1" }, stdio: "ignore" });
  running.push(c);
  return c;
}

async function untilRunning(c: ChildProcess) {
  for (let i = 0; i < 50 && !existsSync(`/proc/${c.pid}/cmdline`); i++) await new Promise((r) => setTimeout(r, 20));
}

beforeEach(() => {
  t = realpathSync(mkdtempSync(path.join(tmpdir(), "start-stdio-")));
  buildLog = path.join(t, "builds.log");
  fakeNpm = path.join(t, "fake-npm.mjs");
  writeFileSync(fakeNpm, FAKE_NPM);
});

afterEach(() => {
  for (const c of running.splice(0)) c.kill("SIGKILL");
  rmSync(t, { recursive: true, force: true });
});

describe("start:stdio launcher", () => {
  it("starts an existing build without compiling, and stdout is only the server's output", () => {
    const p = pkg("a", true);
    const r = start(p);
    expect(r.rc).toBe(0);
    expect(r.compiles).toEqual([]);
    expect(r.stdout.trim().split("\n")).toHaveLength(1);
  });

  it("runs the server as a child whose argv names dist/index.js (scanners must see it)", () => {
    const p = pkg("a", true);
    const r = start(p);
    expect(JSON.parse(r.stdout).argv1).toBe(path.join(p, "dist", "index.js"));
  });

  it("propagates the server's exit code", () => {
    expect(start(pkg("a", true), { FX_EXIT: "3" }).rc).toBe(3);
  });

  it("compiles a fresh checkout once, with compiler output on stderr, never stdout", () => {
    const p = pkg("fresh", false);
    const r = start(p);
    expect(r.rc).toBe(0);
    expect(r.compiles).toEqual([p]);
    expect(r.stdout).not.toContain("tsc noise");
    expect(r.stderr).toContain("tsc noise");
    expect(r.stdout.trim().split("\n")).toHaveLength(1);
  });

  it("ICN_OPS_MCP_BUILD_ON_START=1 compiles the checkout's own unserved dist", () => {
    const p = pkg("a", true);
    expect(start(p, { ICN_OPS_MCP_BUILD_ON_START: "1" }).compiles).toEqual([p]);
  });

  it("a failed build stops before starting anything", () => {
    const r = start(pkg("fresh", false), { FX_BUILD_FAIL: "1" });
    expect(r.rc).toBe(1);
    expect(r.stderr).toContain("build failed");
    expect(r.stdout).not.toContain("argv1");
  });

  it("starts a symlinked (shared) build as-is, without compiling", () => {
    const shared = pkg("shared", true);
    const lane = pkg("lane", false);
    symlinkSync(path.join(shared, "dist"), path.join(lane, "dist"));
    const r = start(lane);
    expect(r.rc).toBe(0);
    expect(r.compiles).toEqual([]);
  });

  it("never compiles into a symlinked (shared) dist, even when forced", () => {
    const shared = pkg("shared", true);
    const lane = pkg("lane", false);
    symlinkSync(path.join(shared, "dist"), path.join(lane, "dist"));
    const r = start(lane, { ICN_OPS_MCP_BUILD_ON_START: "1" });
    expect(r.rc).toBe(1);
    expect(r.compiles).toEqual([]);
    expect(r.stderr).toContain("shared build");
  });

  it("never compiles through a dangling dist symlink", () => {
    const lane = pkg("lane", false);
    symlinkSync(path.join(t, "not-built", "dist"), path.join(lane, "dist"));
    const r = start(lane);
    expect(r.rc).toBe(1);
    expect(r.compiles).toEqual([]);
  });

  it("refuses a forced compile under a server started with a cwd-relative argv (npm run start)", async () => {
    const p = pkg("slot", true);
    const server = serve(p, "dist/index.js");
    await untilRunning(server);
    const r = start(p, { ICN_OPS_MCP_BUILD_ON_START: "1" });
    expect(r.rc).toBe(1);
    expect(r.compiles).toEqual([]);
    expect(r.stderr).toContain(String(server.pid));
  });

  it("refuses a forced compile under a server started with an absolute argv", async () => {
    const p = pkg("slot", true);
    const server = serve(t, path.join(p, "dist", "index.js"));
    await untilRunning(server);
    const r = start(p, { ICN_OPS_MCP_BUILD_ON_START: "1" });
    expect(r.rc).toBe(1);
    expect(r.compiles).toEqual([]);
    expect(r.stderr).toContain(String(server.pid));
  });
});
