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

import { randomUUID } from "crypto";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "fs";
import { hostname } from "os";
import { dirname } from "path";
import { initDb } from "../state/db.js";
import {
  classifyWorktree,
  InvalidProviderSessionIdError,
  sessionsByWorktreeName,
  recordHeartbeat,
  recordInteraction,
  recordProgress,
  registerSession,
  releaseSession,
  type ProgressKind,
  type SessionRow,
} from "../runtime/session-runtime.js";
import { discoverWorktree, readBranchState } from "../runtime/worktree-identity.js";

/**
 * Resolve a DURABLE harness key.
 *
 * Preferred: the provider's own stable session id. For Claude Code that is `session_id` in the
 * hook payload — verified present on SessionStart/UserPromptSubmit/PostToolUse/Stop/SessionEnd
 * and verified unchanged across `--resume`. (There is no CLAUDE_SESSION_ID environment
 * variable; that was an assumption and it is false on this installation.)
 *
 * Fallback for launchers with no provider id: mint a UUID once and persist it in an identity
 * file whose lifetime is the harness's. `pid@host` is never synthesised — PIDs are recycled,
 * so it is correlation metadata at best and dangerous as identity.
 */
function resolveHarnessKey(
  explicit?: string,
  identityFile?: string,
  opts: { mint?: boolean } = {}
): string | null {
  if (explicit) return explicit;
  if (!identityFile) return null;
  try {
    if (existsSync(identityFile)) {
      const existing = readFileSync(identityFile, "utf-8").trim();
      if (existing) return existing;
    }
    // Only `register` may create an identity. Minting on read paths meant a second
    // `release --identity-file F` wrote a BRAND NEW uuid, missed the lookup, and returned
    // before the cleanup — a cleanup path that resurrected the artifact it exists to remove.
    if (!opts.mint) return null;
    mkdirSync(dirname(identityFile), { recursive: true });
    const minted = randomUUID();
    writeFileSync(identityFile, minted + "\n", { mode: 0o600 });
    return minted;
  } catch {
    return null;
  }
}

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

function findByProviderSession(
  db: ReturnType<typeof initDb>,
  key: string
): SessionRow | undefined {
  return db
    .prepare("SELECT * FROM sessions WHERE provider_session_id = ? LIMIT 1")
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
        "  register    --harness-key K|--identity-file F --cwd DIR [--task-ref R] [--pr-ref P]",
        "            [--parent-session ID] [--provider NAME] [--pid N] [--task-description T]",
        "  progress    --harness-key K --kind file_edit|command|test|task_state|explicit",
        "              [--activity TEXT]",
        "  interaction --harness-key K   (turn boundary: liveness only, NOT progress)",
        "  heartbeat   --harness-key K",
              "  release   --harness-key K [--reason completed|cancelled|error|shutdown]",
        "  status    --harness-key K",
        "  classify    --worktree-id ID | --path DIR | --worktree NAME",
        "              [--pids 1,2,3 | --observed-none]",
        "              (omitting both means 'no observation performed' -> stays protected)",
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
      const key = resolveHarnessKey(str(args, "harness-key"), str(args, "identity-file"), {
        mint: true,
      });
      const cwd = str(args, "cwd") ?? process.cwd();
      // ICN_ROOT (or --root) is a CANDIDATE, validated by Git — never authoritative on its own,
      // and hook cwd may be a subdirectory or scratch path, so Git resolves the real owner.
      const identity = discoverWorktree(cwd, str(args, "root") ?? process.env["ICN_ROOT"] ?? null);
      if (!identity) {
        process.stderr.write(
          `icn-agent-session: ${cwd} does not resolve to a Git worktree; not registering\n`
        );
        return 0;
      }
      const branchState = readBranchState(identity.worktree_path);
      const pidRaw = str(args, "pid");
      let result;
      try {
        result = registerSession(db, {
          repo: str(args, "repo") ?? identity.repo_name,
          identity,
          branch_state: branchState,
          task_description: str(args, "task-description") ?? null,
          task_ref: str(args, "task-ref") ?? null,
          pr_ref: str(args, "pr-ref") ?? null,
          parent_session_id: str(args, "parent-session") ?? null,
          provider: str(args, "provider") ?? "claude-code",
          // Validated like --pids is. `process.kill(0,0)` and `kill(-1,0)` SUCCEED (they
        // signal a process group), so --pid 0 made the lane report live forever.
        agent_pid: (() => {
          const n = pidRaw ? Number(pidRaw) : NaN;
          return Number.isInteger(n) && n > 1 ? n : null;
        })(),
          host: hostname(),
          provider_session_id: key ?? null,
          transcript_path: str(args, "transcript-path") ?? null,
        });
      } catch (e) {
        if (e instanceof InvalidProviderSessionIdError) {
          process.stderr.write(`icn-agent-session: ${(e as Error).message}\n`);
          return 2;
        }
        throw e;
      }
      emit(
        JSON.stringify({
          ...result,
          repo: identity.repo_name,
          worktree_name: identity.worktree_name,
          worktree_path: identity.worktree_path,
          branch: branchState.branch,
          detached: branchState.detached,
        })
      );
      return 0;
    }

    case "progress":
    case "interaction":
    case "heartbeat": {
      const key = resolveHarnessKey(str(args, "harness-key"), str(args, "identity-file"));
      if (!key) return 0; // nothing to attribute the signal to; never block a hook
      const row = findByProviderSession(db, key);
      if (!row) return 0; // unregistered session: silently a no-op, not an error
      if (cmd === "heartbeat") recordHeartbeat(db, row.id);
      else if (cmd === "interaction") recordInteraction(db, row.id);
      else {
        const kind = (str(args, "kind") ?? "explicit") as ProgressKind;
        recordProgress(db, row.id, { kind, activity: str(args, "activity") ?? null });
      }
      return 0;
    }



    case "release": {
      const key = resolveHarnessKey(str(args, "harness-key"), str(args, "identity-file"));
      if (!key) return 0;
      const row = findByProviderSession(db, key);
      if (!row) return 0;
      const res = releaseSession(db, row.id, {
        reason: str(args, "reason") ?? "completed",
      });
      // The identity file is a session-scoped resource too. Left behind, a SIGKILLed session's
      // key was re-read by the next launch, which then DEDUPED INTO THE DEAD SESSION'S ROW and
      // inherited its progress history and file claims — exactly the identity inheritance that
      // rejecting pid@host exists to prevent, reached through the sanctioned fallback.
      const identityFile = str(args, "identity-file");
      if (identityFile) {
        try {
          rmSync(identityFile, { force: true });
        } catch {
          process.stderr.write(
            `icn-agent-session: could not remove identity file ${identityFile}; ` +
              "a later launch could inherit this session's identity\n"
          );
        }
      }
      emit(JSON.stringify(res));
      return 0;
    }

    case "status": {
      const key = resolveHarnessKey(str(args, "harness-key"), str(args, "identity-file"));
      const row = key ? findByProviderSession(db, key) : undefined;
      process.stdout.write(
        JSON.stringify(row ? { registered: true, ...row } : { registered: false }) + "\n"
      );
      return 0;
    }

    case "classify": {
      // Prefer the Git-derived id. --worktree (a display name) is accepted for humans and
      // resolved through Git when a path is available, because a bare basename is ambiguous.
      let worktreeId = str(args, "worktree-id") ?? null;
      const path = str(args, "path");
      if (!worktreeId && path) {
        worktreeId = discoverWorktree(path, null)?.worktree_id ?? null;
      }
      if (!worktreeId) {
        const name = str(args, "worktree");
        if (!name) {
          // Only 0 and 3 are defined. An earlier contract used 2 for "retirement candidate";
          // that verdict no longer exists, and reusing the code for a usage error told a
          // consumer an unresolvable path was permission to retire.
          process.stdout.write(
            JSON.stringify({
              state: "REGISTRY-UNAVAILABLE",
              // Present on every envelope: the type declares it required, and a consumer
              // typed against the interface throws on `.live_agent_pids.length`.
              live_agent_pids: [],
              contention: { count: 0, session_ids: [] },
              reason:
                "no lane could be resolved from the arguments given " +
                "(need --worktree-id, --path, or --worktree)",
            }) + "\n"
          );
          process.stderr.write("classify requires --worktree-id, --path, or --worktree\n");
          return 3;
        }
        const matches = sessionsByWorktreeName(db, name);
        const ids = [...new Set(matches.map((m) => m.worktree_id).filter(Boolean))];
        if (ids.length > 1) {
          process.stdout.write(
            JSON.stringify({
              state: "REGISTRY-UNAVAILABLE",
              // Present on every envelope: the type declares it required, and a consumer
              // typed against the interface throws on `.live_agent_pids.length`.
              live_agent_pids: [],
              contention: { count: 0, session_ids: [] },
              reason: `worktree name ${JSON.stringify(name)} is ambiguous across ${ids.length} lanes: ${ids.join(", ")}`,
            }) + "\n"
          );
          // 3, not 1: the documented contract is 0 (facts produced) or 3 (none available).
          return 3;
        }
        worktreeId = ids[0] ?? name;
      }
      const worktree = worktreeId;
      // Absent --pids means NOBODY LOOKED and must stay protected. --observed-none is the
      // affirmative "I looked and found nothing", which is the only form that can support
      // retirement. Passing [] for both was the bug: it let a lane with a live agent process
      // classify as retireable.
      const pidsRaw = str(args, "pids");
      const observedNone = args["observed-none"] === true;
      let pids: number[] | null;
      if (pidsRaw !== undefined) {
        const tokens = pidsRaw.split(",").map((s) => s.trim()).filter((s) => s.length > 0);
        const parsed = tokens
          .map((s) => Number(s))
          .filter((n) => Number.isInteger(n) && n > 0);
        // Garbage in must not become the strongest possible claim. `--pids "$(lsof -t … )"`
        // where lsof errored used to collapse to [] — an affirmative "I looked and found
        // nothing" — reintroducing the very bug the three-state observation closed.
        if (tokens.length > 0 && parsed.length === 0) {
          process.stderr.write(
            `icn-agent-session: --pids ${JSON.stringify(pidsRaw)} contained no valid pids; ` +
              "treating as NO observation performed (lane stays protected)\n"
          );
          pids = null;
        } else {
          pids = parsed;
        }
      } else {
        pids = observedNone ? [] : null;
      }
      let c;
      try {
        c = classifyWorktree(db, worktree, { observed_pids: pids });
      } catch (e) {
        // Any unexpected failure must still produce a parseable, fail-safe verdict. Letting the
        // exception escape produced EMPTY stdout — the precise failure the wrapper's degrade
        // path exists to prevent.
        process.stdout.write(
          JSON.stringify({
            state: "REGISTRY-UNAVAILABLE",
            live_agent_pids: [],
            contention: { count: 0, session_ids: [] },
            reason: `classification failed: ${(e as Error).message}`,
          }) + "\n"
        );
        return 3;
      }
      process.stdout.write(JSON.stringify(c) + "\n");
      // The exit code reports whether FACTS COULD BE PRODUCED, not whether a lane may be
      // retired. There is no "retirement candidate" code: this command observes, and the
      // consumer applies policy. 0 = facts returned, 3 = nothing authoritative available.
      return c.state === "REGISTRY-UNAVAILABLE" ? 3 : 0;
    }

    default:
      process.stderr.write(`unknown command: ${cmd}\n`);
      return 2;
  }
}

process.exit(main());
