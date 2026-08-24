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
  assertProgressKind,
  classifyWorktree,
  parseObservedPids,
  InvalidProviderSessionIdError,
  sessionsByWorktreeName,
  recordHeartbeat,
  recordInteraction,
  recordProgress,
  registerSession,
  releaseSession,
  type Classification,
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

/**
 * The ONLY way this CLI emits a degraded classification.
 *
 * A degraded envelope has to be structurally identical to a healthy one, because a consumer
 * typed against `Classification` reads `.live_agent_pids.length` and throws outright on a
 * partial object. Four sites built these as bare object literals and TypeScript did not
 * enforce anything: a literal handed straight to `JSON.stringify` is checked against no type
 * at all, so deleting `live_agent_pids` and `contention` from every one of them left
 * `tsc --noEmit` clean and the whole suite green. One of the four — the `initDb` failure, i.e.
 * the registry missing, locked, or corrupt — shipped without them, and that is the MOST likely
 * degraded condition in production, not the rarest.
 *
 * The annotated return type is what makes the contract checkable; routing every site through
 * one constructor is what stops a fifth site from being added without it.
 */
function degradedClassification(reason: string): Classification {
  return {
    state: "REGISTRY-UNAVAILABLE",
    reason,
    session_id: null,
    heartbeat_age_min: null,
    progress_age_min: null,
    progress_count: null,
    contention: { count: 0, session_ids: [] },
    branch_changed: false,
    live_branch: null,
    live_agent_pids: [],
  };
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
        JSON.stringify(degradedClassification(`session registry could not be opened: ${msg}`)) +
          "\n"
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
      // An EXPLICIT `--root` is an operator stating intent, so it may still act as a fallback
      // when cwd resolves to nothing. ICN_ROOT MUST NOT, and no longer does. It is ambient, and
      // on icn-dev the shell profile pins it to the mcp-host worktree — so every session whose
      // cwd was not a worktree (a scratch path, a deleted directory, $HOME) was registered
      // under mcp-host's REAL lane: phantom occupancy on a lane nobody was in, while the lane
      // the agent was actually in reported UNREGISTERED. Whether a session was misfiled or
      // correctly refused came down to whether an environment variable happened to be set.
      const identity = discoverWorktree(cwd, str(args, "root") ?? null);
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
        // A CAST IS NOT A CHECK. `--kind` arrives as an arbitrary string from a hook payload,
        // and `as ProgressKind` asserted a fact TypeScript had no way to verify — so the CLI
        // accepted `--kind turn` and advanced progress_count, while the MCP tool rejected the
        // same word. Validate against the shared vocabulary so both surfaces agree.
        const kind = str(args, "kind") ?? "explicit";
        try {
          assertProgressKind(kind);
        } catch (e) {
          process.stderr.write(`icn-agent-session: ${(e as Error).message}\n`);
          return 2;
        }
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
            JSON.stringify(
              degradedClassification(
                "no lane could be resolved from the arguments given " +
                  "(need --worktree-id, --path, or --worktree)"
              )
            ) + "\n"
          );
          process.stderr.write("classify requires --worktree-id, --path, or --worktree\n");
          return 3;
        }
        const matches = sessionsByWorktreeName(db, name);
        const ids = [...new Set(matches.map((m) => m.worktree_id).filter(Boolean))];
        if (ids.length > 1) {
          process.stdout.write(
            JSON.stringify(
              degradedClassification(
                `worktree name ${JSON.stringify(name)} is ambiguous across ${ids.length} lanes: ${ids.join(", ")}`
              )
            ) + "\n"
          );
          // 3, not 1: the documented contract is 0 (facts produced) or 3 (none available).
          return 3;
        }
        worktreeId = ids[0] ?? name;
      }
      const worktree = worktreeId;
      const parsed = parseObservedPids(str(args, "pids"), args["observed-none"] === true);
      if (parsed.warning) process.stderr.write(`icn-agent-session: ${parsed.warning}\n`);
      const pids = parsed.observed_pids;
      let c;
      try {
        c = classifyWorktree(db, worktree, { observed_pids: pids });
      } catch (e) {
        // Any unexpected failure must still produce a parseable, fail-safe verdict. Letting the
        // exception escape produced EMPTY stdout — the precise failure the wrapper's degrade
        // path exists to prevent.
        process.stdout.write(
          JSON.stringify(
            degradedClassification(`classification failed: ${(e as Error).message}`)
          ) + "\n"
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

// `process.exitCode`, NOT `process.exit()`. When stdout is a PIPE, Node's writes are
// asynchronous, and process.exit() discards whatever is still queued — so a large envelope
// was TRUNCATED at the 64 KB pipe buffer while classify still exited 0, i.e. "facts produced"
// with unparseable JSON on the wire. Setting exitCode lets the event loop drain first.
process.exitCode = main();
