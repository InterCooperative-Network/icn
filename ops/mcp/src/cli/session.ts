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
import { randomUUID } from "crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { hostname } from "os";
import { basename, dirname } from "path";
import { initDb } from "../state/db.js";
import {
  classifyWorktree,
  endSupervision,
  InvalidProviderSessionIdError,
  activeSessionsForWorktree,
  liveSupervisions,
  sessionsByWorktreeName,
  recordHeartbeat,
  recordInteraction,
  recordProgress,
  registerSession,
  releaseSession,
  superviseOperation,
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
function resolveHarnessKey(explicit?: string, identityFile?: string): string | null {
  if (explicit) return explicit;
  if (!identityFile) return null;
  try {
    if (existsSync(identityFile)) {
      const existing = readFileSync(identityFile, "utf-8").trim();
      if (existing) return existing;
    }
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
        "  supervise   --pid N [--harness-key K] [--label L]  -> prints supervision id",
        "              (without --harness-key: resolves the lane's sole session)",
        "  unsupervise --id N [--exit-code C]",
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
      const key = resolveHarnessKey(str(args, "harness-key"), str(args, "identity-file"));
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
          agent_pid: pidRaw ? Number(pidRaw) : null,
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

    case "supervise": {
      const pid = Number(str(args, "pid"));
      if (!Number.isInteger(pid) || pid <= 0) return 0;

      const key = resolveHarnessKey(str(args, "harness-key"), str(args, "identity-file"));
      let row = key ? findByProviderSession(db, key) : undefined;

      // No key supplied: resolve by lane. A Bash command has no access to the provider session
      // id (there is no CLAUDE_SESSION_ID env var — verified), so requiring one would make
      // `icn-wait --supervise` unusable from the place it is actually needed.
      //
      // Only unambiguous when ONE session occupies the lane. With several, guessing would
      // attach a build to the wrong session, so it declines and says why.
      if (!row) {
        const identity = discoverWorktree(str(args, "cwd") ?? process.cwd(), null);
        if (identity) {
          const rows = activeSessionsForWorktree(db, identity.worktree_id);
          if (rows.length === 1) {
            row = rows[0];
          } else if (rows.length > 1) {
            process.stderr.write(
              `icn-agent-session: ${rows.length} sessions occupy this lane; ` +
                "pass --harness-key to say which one owns this operation\n"
            );
            return 0;
          }
        }
      }
      if (!row) return 0;

      const id = superviseOperation(db, row.id, pid, str(args, "label") ?? "supervised operation");
      process.stdout.write(String(id) + "\n");
      return 0;
    }

    case "unsupervise": {
      const id = Number(str(args, "id"));
      if (!Number.isInteger(id)) return 0;
      const code = str(args, "exit-code");
      endSupervision(db, id, code === undefined ? undefined : Number(code));
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
      emit(JSON.stringify(res));
      return 0;
    }

    case "status": {
      const key = resolveHarnessKey(str(args, "harness-key"), str(args, "identity-file"));
      const row = key ? findByProviderSession(db, key) : undefined;
      process.stdout.write(
        JSON.stringify(
          row
            ? { registered: true, ...row, live_supervisions: liveSupervisions(db, row.id) }
            : { registered: false }
        ) + "\n"
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
          process.stderr.write("classify requires --worktree-id, --path, or --worktree\n");
          return 2;
        }
        const matches = sessionsByWorktreeName(db, name);
        const ids = [...new Set(matches.map((m) => m.worktree_id).filter(Boolean))];
        if (ids.length > 1) {
          process.stdout.write(
            JSON.stringify({
              state: "REGISTRY-UNAVAILABLE",
              retireable: false,
              retireable_with_approval: false,
              reason: `worktree name ${JSON.stringify(name)} is ambiguous across ${ids.length} lanes: ${ids.join(", ")}`,
            }) + "\n"
          );
          return 1;
        }
        worktreeId = ids[0] ?? name;
      }
      const worktree = worktreeId;
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
