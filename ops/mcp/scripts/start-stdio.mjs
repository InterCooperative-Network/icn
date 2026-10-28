#!/usr/bin/env node
// `npm run start:stdio`: the command the repository's tracked MCP configs (.mcp.json,
// .cursor/mcp.json) run. It used to be `npm run --silent build && node dist/index.js`, so any
// client that opened a checkout compiled it, including a checkout an MCP server was serving
// from, or a lane whose dist is a symlink into a shared build. Now:
//   - an existing build is started as-is;
//   - a checkout with no build is compiled first (a fresh clone still works);
//   - ICN_OPS_MCP_BUILD_ON_START=1 forces the compile, for a developer editing ops/mcp;
//   - a compile is refused when dist is shared (resolves outside this package) or when a running
//     process executes this dist/index.js;
//   - compiler output goes to stderr. stdout carries the MCP protocol and nothing else.
// Stdlib only: this must run before (and without) a build.

import { spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, readlinkSync, realpathSync } from "node:fs";
import path from "node:path";

// npm runs package scripts from the package directory. process.cwd() is the physical path, so a
// symlinked ops/mcp is not distinguishable here; the shared-build layout this guards is a
// symlinked dist (agent-runtime lanes).
const pkg = process.cwd();
const dist = path.join(pkg, "dist");
const entry = path.join(dist, "index.js");

function fail(message) {
  process.stderr.write(`icn-ops start:stdio: ${message}\n`);
  process.exit(1);
}

// The checkout's OWN output: false when dist resolves elsewhere (a shared build).
function ownDist() {
  if (existsSync(dist)) return realpathSync(dist) === path.join(pkg, "dist");
  try {
    readlinkSync(dist); // a dangling symlink is still somebody else's output
    return false;
  } catch {
    return true;
  }
}

// Pids (Linux /proc) whose argv names this entry file, resolving a relative argv against that
// process's own cwd. Empty where /proc is unavailable.
function pidsExecuting(file) {
  let target;
  try {
    target = realpathSync(file);
  } catch {
    return [];
  }
  let pids;
  try {
    pids = readdirSync("/proc").filter((p) => /^\d+$/.test(p) && Number(p) !== process.pid);
  } catch {
    return [];
  }
  const found = [];
  for (const pid of pids) {
    let args, cwd;
    try {
      args = readFileSync(`/proc/${pid}/cmdline`, "utf8").split("\0").filter((a) => a.endsWith("index.js"));
      if (args.length === 0) continue;
      cwd = readlinkSync(`/proc/${pid}/cwd`);
    } catch {
      continue;
    }
    for (const a of args) {
      let resolved;
      try {
        resolved = realpathSync(path.isAbsolute(a) ? a : path.join(cwd, a));
      } catch {
        continue;
      }
      if (resolved === target) {
        found.push(pid);
        break;
      }
    }
  }
  return found;
}

const forced = process.env.ICN_OPS_MCP_BUILD_ON_START === "1";
if (forced || !existsSync(entry)) {
  if (!ownDist()) {
    fail(`refusing to compile: ${dist} resolves outside this checkout (a shared build). Build it where it lives.`);
  }
  const serving = pidsExecuting(entry);
  if (serving.length > 0) {
    fail(`refusing to compile: ${entry} is executed by running process(es) ${serving.join(", ")}.`);
  }
  const npm = process.env.npm_execpath ? [process.execPath, process.env.npm_execpath] : ["npm"];
  const build = spawnSync(npm[0], [...npm.slice(1), "run", "--silent", "build"], {
    stdio: ["ignore", 2, 2],
  });
  if (build.status !== 0) fail(`build failed (exit ${build.status ?? build.signal}); see the compiler output above.`);
  if (!existsSync(entry)) fail(`build finished but ${entry} is missing.`);
}

// A child, not an in-process import: the server's argv must name dist/index.js, because that is
// how every served-checkout scanner (this script, the plugin launcher, icn-session) finds it.
const server = spawn(process.execPath, [entry], { stdio: "inherit" });
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(sig, () => server.kill(sig));
server.on("exit", (code, signal) => process.exit(code ?? (signal ? 128 : 1)));
