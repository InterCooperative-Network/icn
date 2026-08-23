#!/usr/bin/env node
// Session lifecycle CLI — the surface Claude Code hooks use.
//
// Hooks run as ordinary subprocesses with no MCP client, so they cannot call register_session.
// This CLI is the same runtime core (../runtime/session-runtime.ts) behind an argv interface.
//
// Every subcommand is designed to be safe to call from a hook:
//   - it never blocks,
//   - it never writes to stdout unless asked (hook stdout becomes agent context),
//   - it exits non-zero ONLY for `classify`, whose exit code is a verdict.
//
// Refs docs/architecture/AGENT_RUNTIME.md §3.

import { execFileSync } from "child_process";
import { hostname } from "os";
import { basename, dirname } from "path";
import { initDb } from "../state/db.js";
import {
  classifyWorktree,
  recordHeartbeat,
  recordProgress,
  registerSession,
  releaseSession,
  type ProgressKind,
  type SessionRow,
} from "../runtime/session-runtime.js";

type Args = Record<string, string | boolean>;

function parseArgs(argv: string[]): { cmd: string; args: Args } {
  const cmd = argv[0] ?? "help";
  const args: Args = {};
  for (let i = 1; i < argv.length; i++) {
    const tok = argv[i]!;
    if (!tok.startsWith("--")) continue;
    const key = tok.slice(2);
    const next = argv[i + 1];
    if (next === undefined || next.startsWith("--")) {
      args[key] = true;
    } else {
      args[key] = next;
      i++;
    }
  }
  return { cmd, args };
}

function str(args: Args, key: string): string | undefined {
  const v = args[key];
  return typeof v === "string" && v.length > 0 ? v : undefined;
}

/** Resolve repo/worktree/branch from a working directory, without trusting the caller. */
function resolveLocation(cwd: string): {
  repo: string;
  worktree: string | null;
  branch: string | null;
} {
  let top: string | null = null;
  let branch: string | null = null;
  try {
    top = execFileSync("git", ["-C", cwd, "rev-parse", "--show-toplevel"], {
      encoding: "utf-8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    top = null;
  }
  if (top) {
    try {
      branch =
        execFileSync("git", ["-C", top, "branch", "--show-current"], {
          encoding: "utf-8",
          stdio: ["ignore", "pipe", "ignore"],
        }).trim() || null;
    } catch {
      branch = null;
    }
  }
  // Canonical layout is <root>/worktrees/<repo>/<worktree>; derive both from the path rather
  // than hardcoding a repo name, so other repos on this VM work unchanged.
  const path = top ?? cwd;
  const worktree = basename(path);
  const parent = basename(dirname(path));
  const repo = parent && parent !== "worktrees" ? parent : "icn";
  return { repo, worktree: worktree || null, branch };
}

function findByHarnessKey(db: ReturnType<typeof initDb>, key: string): SessionRow | undefined {
  return db
    .prepare("SELECT * FROM sessions WHERE harness_key = ? AND state = 'active' LIMIT 1")
    .get(key) as SessionRow | undefined;
}

function main(): number {
  const { cmd, args } = parseArgs(process.argv.slice(2));
  const quiet = args["quiet"] === true;
  const emit = (s: string) => {
    if (!quiet) process.stdout.write(s + "\n");
  };

  if (cmd === "help" || args["help"] === true) {
    process.stdout.write(
      [
        "icn-agent-session <command> [--flags]",
        "",
        "  register  --harness-key K --cwd DIR [--task-ref R] [--pr-ref P]",
        "            [--parent-session ID] [--provider NAME] [--pid N] [--task-description T]",
        "  progress  --harness-key K --kind file_edit|command|turn|test|task_state|explicit",
        "            [--activity TEXT]",
        "  heartbeat --harness-key K",
        "  release   --harness-key K [--reason completed|cancelled|error|shutdown]",
        "  status    --harness-key K",
        "  classify  --worktree W [--pids 1,2,3]",
        "",
      ].join("\n")
    );
    return 0;
  }

  let db: ReturnType<typeof initDb>;
  try {
    db = initDb();
  } catch (e) {
    // Registry unavailable. For `classify` this is a VERDICT (fail safe, exit 3); for the
    // write paths it is a degraded-but-non-blocking condition the caller must surface.
    const msg = e instanceof Error ? e.message : String(e);
    if (cmd === "classify") {
      process.stdout.write(
        JSON.stringify({
          state: "REGISTRY-UNAVAILABLE",
          retireable: false,
          retireable_with_approval: false,
          reason: `session registry could not be opened: ${msg}`,
        }) + "\n"
      );
      return 3;
    }
    process.stderr.write(`icn-agent-session: registry unavailable: ${msg}\n`);
    return 1;
  }

  switch (cmd) {
    case "register": {
      const key = str(args, "harness-key");
      const cwd = str(args, "cwd") ?? process.cwd();
      const loc = resolveLocation(cwd);
      const pidRaw = str(args, "pid");
      const result = registerSession(db, {
        repo: str(args, "repo") ?? loc.repo,
        worktree: str(args, "worktree") ?? loc.worktree,
        branch: str(args, "branch") ?? loc.branch,
        task_description: str(args, "task-description") ?? null,
        task_ref: str(args, "task-ref") ?? null,
        pr_ref: str(args, "pr-ref") ?? null,
        parent_session_id: str(args, "parent-session") ?? null,
        provider: str(args, "provider") ?? "claude-code",
        agent_pid: pidRaw ? Number(pidRaw) : null,
        host: hostname(),
        harness_key: key ?? null,
      });
      emit(
        JSON.stringify({
          ...result,
          repo: str(args, "repo") ?? loc.repo,
          worktree: loc.worktree,
          branch: loc.branch,
        })
      );
      return 0;
    }

    case "progress":
    case "heartbeat": {
      const key = str(args, "harness-key");
      if (!key) return 0; // nothing to attribute progress to; never block a hook
      const row = findByHarnessKey(db, key);
      if (!row) return 0; // unregistered session: silently a no-op, not an error
      if (cmd === "heartbeat") {
        recordHeartbeat(db, row.id);
      } else {
        const kind = (str(args, "kind") ?? "explicit") as ProgressKind;
        recordProgress(db, row.id, { kind, activity: str(args, "activity") ?? null });
      }
      return 0;
    }

    case "release": {
      const key = str(args, "harness-key");
      if (!key) return 0;
      const row = findByHarnessKey(db, key);
      if (!row) return 0;
      const res = releaseSession(db, row.id, {
        reason: str(args, "reason") ?? "completed",
      });
      emit(JSON.stringify(res));
      return 0;
    }

    case "status": {
      const key = str(args, "harness-key");
      const row = key ? findByHarnessKey(db, key) : undefined;
      process.stdout.write(
        JSON.stringify(row ? { registered: true, ...row } : { registered: false }) + "\n"
      );
      return 0;
    }

    case "classify": {
      const worktree = str(args, "worktree");
      if (!worktree) {
        process.stderr.write("classify requires --worktree\n");
        return 2;
      }
      const pids = (str(args, "pids") ?? "")
        .split(",")
        .map((s) => Number(s.trim()))
        .filter((n) => Number.isInteger(n) && n > 0);
      const c = classifyWorktree(db, worktree, { observed_pids: pids });
      process.stdout.write(JSON.stringify(c) + "\n");
      // Exit code is the verdict: 0 protected-and-healthy, 1 protected, 2 candidate.
      if (c.state === "REGISTERED-ACTIVE") return 0;
      return c.retireable ? 2 : 1;
    }

    default:
      process.stderr.write(`unknown command: ${cmd}\n`);
      return 2;
  }
}

process.exit(main());
