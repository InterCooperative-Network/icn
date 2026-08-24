// Canonical repo/worktree identity, derived from Git rather than invented.
//
// WHY NOT THE BASENAME
//   `worktree = "task-review"` is not an identity. Two repos on this VM can both have a
//   worktree by that name, and the disk/lifecycle tooling has already been bitten once by
//   basename collisions. A join key that can collide is a join key that will.
//
// WHY NOT THE BRANCH
//   A lane's branch moves constantly: commits change HEAD, rebases rewrite it, branches get
//   renamed, `git checkout --detach` drops it entirely. None of that makes it a different
//   worktree. Branch and HEAD are LIVE STATE about a lane, never the lane's identity.
//
// WHAT GIT ALREADY GUARANTEES
//   git rev-parse --absolute-git-dir   per-worktree admin dir; unique, stable across branch,
//                                      HEAD, rebase, rename and detach; identical when asked
//                                      from any subdirectory of the worktree.
//   git rev-parse --git-common-dir     the shared repository directory; the repo's identity.
//   git rev-parse --show-toplevel      the worktree's working directory.
//
//   For a linked worktree the admin dir is <repo>.git/worktrees/<name>; for the main worktree
//   it is the repo dir itself. Either way it is unique WITHIN and ACROSS repositories, so
//   `task-review` under icn.git and under nycn.git cannot be confused.
//
// Refs docs/architecture/AGENT_RUNTIME.md §2.

import { execFileSync } from "child_process";

/**
 * Environment for child `git` calls with the repo-selecting variables REMOVED.
 *
 * `GIT_DIR` overrides `-C`, and git EXPORTS it into hooks when running in a linked worktree —
 * which is the only kind ICN uses. Inherited, it made two different lanes resolve to the SAME
 * worktree_id and pointed the registry at an unrelated repository. This is the same lesson as
 * ICN_ROOT, one layer down: an unchecked environment variable must never be able to
 * misattribute a session to the wrong lane.
 */
const GIT_SANITISED_ENV: NodeJS.ProcessEnv = (() => {
  const e = { ...process.env };
  for (const k of [
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
  ]) {
    delete e[k];
  }
  return e;
})();
import { randomUUID } from "crypto";
import { existsSync, linkSync, readFileSync, realpathSync, unlinkSync, writeFileSync } from "fs";
import { basename, dirname, join, resolve } from "path";

/** Stable identity of a lane. Nothing here changes when the branch does. */
export type WorktreeIdentity = {
  /** AUTHORITATIVE STABLE: realpath of the shared repository directory. */
  repo_id: string;
  /** Display only. */
  repo_name: string;
  /** AUTHORITATIVE STABLE: realpath of this worktree's Git admin directory. */
  worktree_id: string;
  /** AUTHORITATIVE STABLE: realpath of the worktree's working directory. */
  worktree_path: string;
  /** Display only — may collide across repos, never used as a key. */
  worktree_name: string;
  /**
   * AUTHORITATIVE TEMPORAL: which GENERATION of this lane this is. Null when it cannot be
   * determined, which callers must treat as "unknown", never as "different". See laneGeneration().
   */
  worktree_generation: string | null;
};

/** AUTHORITATIVE LIVE: what the lane currently points at. Re-read, never cached. */
export type BranchState = {
  branch: string | null;
  head: string | null;
  detached: boolean;
};

function git(dir: string, args: string[]): string | null {
  try {
    return execFileSync("git", ["-C", dir, ...args], {
      encoding: "utf-8",
      stdio: ["ignore", "pipe", "ignore"],
      env: GIT_SANITISED_ENV,
    }).trim();
  } catch {
    return null;
  }
}

/** Human-readable repo name from a common-dir path, for both bare stores and plain clones. */
function repoNameFrom(commonDir: string): string {
  const base = basename(commonDir);
  if (base === ".git") return basename(dirname(commonDir));
  return base.replace(/\.git$/, "");
}

/**
 * Name of the runtime-owned generation token, stored INSIDE the Git admin directory.
 *
 * WHY A TOKEN AND NOT A PATH
 *   `repo + admin dir + worktree path` identifies a lane in SPACE but not in TIME. Git recycles
 *   `<repo>/.git/worktrees/<basename>` after `git worktree remove`, and a worktree can be
 *   recreated at the EXACT SAME pathname — at which point recorded path, admin dir and repo all
 *   match, and a brand-new generation silently inherits the previous one's unreleased rows and
 *   the authority attached to them. Verified: a fresh `gen2` worktree classified
 *   REGISTERED-ACTIVE holding `gen1`'s session row.
 *
 * WHY IT CANNOT OUTLIVE ITS GENERATION
 *   The token lives in the one container GIT ITSELF DELETES. `git worktree remove` and
 *   `git worktree prune` remove the whole admin directory, so the token goes with it and a
 *   recreated worktree necessarily mints a new one. It is deliberately NOT stored in the
 *   working tree (which a user may keep), nor in the repo root, nor in any global cache — every
 *   one of those would survive the removal and reintroduce exactly the aliasing it prevents.
 *
 * WHAT IT SURVIVES
 *   Commits, branch switches, branch renames, history-rewriting rebases and detached HEAD all
 *   leave the admin directory in place, so the generation is stable across every one of them —
 *   which is the whole point: none of those make it a different worktree.
 *
 * FAILURE IS "UNKNOWN", NOT "DIFFERENT"
 *   A read-only or unreadable admin directory yields null. Callers keep every row when either
 *   side is unknown, so an unmintable token can only ever make a lane look MORE occupied.
 */
const GENERATION_FILE = "icn-lane-generation";

export function laneGeneration(
  adminDir: string,
  opts: { mint?: boolean } = {}
): string | null {
  // MINTING IS OPT-IN, and only the identity path opts in.
  //
  // This is called from two places with very different trust: discoverWorktree(), which has
  // just had the directory confirmed by `git rev-parse --absolute-git-dir`, and the lane filter
  // in activeSessionsForWorktree(), which receives a worktree_id straight from a caller —
  // `session_lifecycle`'s unvalidated `worktree_id`, or a CLI `--worktree <name>` that falls
  // back to a path relative to the process cwd. Minting there meant the documented READ-ONLY
  // classification path created a file at an attacker-chosen location. Verified: an unrelated
  // temp directory gained an `icn-lane-generation` file just by being named in a classify call.
  const file = join(adminDir, GENERATION_FILE);
  const read = (): string | null => {
    try {
      const v = readFileSync(file, "utf8").trim();
      return v || null;
    } catch {
      return null;
    }
  };
  const existing = read();
  if (existing) return existing;
  if (!opts.mint) return null;

  // ...and even then, only into something that actually IS a Git admin directory. Both a
  // linked worktree's admin dir and a main `.git` contain HEAD; nothing else we would ever be
  // handed does.
  if (!existsSync(join(adminDir, "HEAD"))) return null;

  // ATOMIC CREATE-WITH-CONTENT.
  //
  // `writeFileSync(file, uuid, { flag: "wx" })` is open(O_CREAT|O_EXCL) followed by a SEPARATE
  // write, so a loser that hit EEXIST in the window between them read a ZERO-LENGTH file and
  // returned null. Measured across 240 genuinely parallel minters: 6 (2.5%) came back null.
  // That is not cosmetic — a NULL-generation row is kept by the filter and therefore ADOPTED by
  // a later generation of the lane, which is precisely the aliasing this token exists to stop.
  //
  // Writing the content to a temp file first and then link(2)-ing it into place preserves the
  // single-winner guarantee (link fails EEXIST if the target exists) while guaranteeing the
  // target is COMPLETE the instant it becomes visible. rename(2) would be wrong here: it
  // replaces, so concurrent minters would each install their own token and disagree.
  const tmp = `${file}.tmp.${process.pid}.${randomUUID()}`;
  try {
    writeFileSync(tmp, `${randomUUID()}\n`, { mode: 0o644 });
    try {
      linkSync(tmp, file);
    } catch {
      // EEXIST — another minter won. Their token is complete; we read it below.
    }
  } catch {
    // Unwritable admin directory. Falls through to a final read, which returns null, and null
    // means UNKNOWN: the filter keeps every row it cannot judge.
  } finally {
    try {
      unlinkSync(tmp);
    } catch {
      // Nothing to clean up.
    }
  }
  return read();
}

/**
 * The path a LINKED worktree's admin directory currently points at, or null when the id is not
 * a linked worktree (a main worktree's `.git` has no `gitdir` file) or cannot be read.
 *
 * `worktree_id` is stable for the life of a worktree but NOT unique over time, because git
 * RECYCLES the admin directory name: after `git worktree remove old/mylane`, a later
 * `git worktree add new/mylane` — a different path, a different branch — is handed the SAME
 * `<repo>/.git/worktrees/mylane`, and therefore the same id. Verified directly.
 *
 * Left unhandled, the new lane silently adopts the removed lane's unreleased rows, and that
 * flips the verdict in the DANGEROUS direction: a fresh lane that should read
 * UNREGISTERED-OBSERVED (protected, because absence of a row is never evidence of absence)
 * instead reads REGISTERED-EXPIRED, with a reason string asserting facts about a directory
 * that no longer exists.
 *
 * Returning null on any doubt is deliberate — the caller keeps every row when it cannot tell,
 * so an unreadable admin dir can only ever make a lane look MORE occupied, never less.
 */
export function currentWorktreePathFor(worktreeId: string): string | null {
  try {
    const raw = readFileSync(join(worktreeId, "gitdir"), "utf8").trim();
    if (!raw) return null;
    // `gitdir` holds the path of the worktree's own `.git` FILE; the lane is its parent.
    return canonical(dirname(raw));
  } catch {
    return null;
  }
}

function canonical(p: string | null): string | null {
  if (!p) return null;
  try {
    return realpathSync(p);
  } catch {
    return p;
  }
}

/**
 * Resolve the owning worktree of `startDir`.
 *
 * `envRoot` (ICN_ROOT or a launcher-supplied root) is treated as a CANDIDATE, not as truth:
 * it is only used when Git confirms it resolves to a real worktree. An unchecked environment
 * variable must never be able to misattribute a session to the wrong lane.
 *
 * Hook `cwd` is likewise only a starting point — tool execution can happen from a subdirectory
 * or a scratch path, so the answer always comes from Git's own resolution, not from the path.
 */
export function discoverWorktree(
  startDir: string,
  envRoot?: string | null
): WorktreeIdentity | null {
  // PRECEDENCE MATTERS. `startDir` is where the session actually is; `envRoot` is a launcher
  // hint that is frequently stale — on icn-dev, ICN_ROOT is pinned to the mcp-host worktree by
  // the shell profile, so letting it win would file every session under the wrong lane
  // regardless of which worktree the agent is working in. (This is not hypothetical: it was
  // caught by an end-to-end smoke test that registered a task lane as `mcp-host`.)
  //
  // Validating that envRoot is a real worktree is therefore NOT enough — a valid answer to the
  // wrong question is still wrong. envRoot is consulted only when cwd resolves to nothing.
  //
  // AND IT MUST NEVER BE AMBIENT. Callers now pass only an EXPLICIT `--root` here; the
  // `ICN_ROOT` reads were removed from every call site. The fallback below is reachable
  // whenever cwd yields no gitDir — which includes the ordinary case of a cwd that simply is
  // not in a repository — so while it was fed from the environment, a session started in a
  // scratch path or a deleted directory was filed under mcp-host's real lane instead of being
  // refused. An operator typing `--root` is stating intent; a shell profile is not.
  const candidates: string[] = [];
  if (startDir && existsSync(startDir)) candidates.push(startDir);
  if (envRoot && existsSync(envRoot)) candidates.push(envRoot);

  for (const dir of candidates) {
    const gitDir = canonical(git(dir, ["rev-parse", "--absolute-git-dir"]));
    if (!gitDir) continue;

    // `--git-common-dir` may be relative — and it is relative to the directory git ran in
    // (our `-C dir`), NOT to the worktree toplevel. Resolving it against the toplevel produced
    // a different repo_id for every subdirectory depth (`<top>/../../.git` from two levels
    // down), which escapes the repo, fails realpath, and silently returns un-normalised. On
    // linked worktrees git prints an absolute path so this was invisible here; it breaks on
    // any plain clone. resolve() handles both the absolute and relative cases.
    const commonRaw = git(dir, ["rev-parse", "--git-common-dir"]);
    const common = canonical(commonRaw ? resolve(dir, commonRaw) : null);
    const top = canonical(git(dir, ["rev-parse", "--show-toplevel"]));
    if (!common || !top) continue;

    // A bare repo has no working tree; it is not a lane.
    if (git(dir, ["rev-parse", "--is-bare-repository"]) === "true") continue;

    return {
      repo_id: common,
      // A bare store is `<name>.git`; a plain clone's common dir is `<repo>/.git`, whose
      // basename is literally ".git" and would strip to the empty string. Step up in that case.
      repo_name: repoNameFrom(common),
      worktree_id: gitDir,
      worktree_path: top,
      worktree_name: basename(top),
      worktree_generation: laneGeneration(gitDir, { mint: true }),
    };
  }
  return null;
}

/**
 * Read the CURRENT branch/HEAD of a lane.
 *
 * Deliberately separate from identity, and deliberately not cached: the branch captured at
 * registration is a historical launch fact, and using it later as current state is exactly
 * the mistake that makes a rebased or renamed lane look like a different one.
 */
export function readBranchState(worktreePath: string): BranchState {
  const branch = git(worktreePath, ["branch", "--show-current"]);
  const head = git(worktreePath, ["rev-parse", "HEAD"]);
  return {
    branch: branch ? branch : null,
    head: head ?? null,
    // `git branch --show-current` prints nothing in detached HEAD.
    detached: !branch && head !== null,
  };
}

/** Whether a lane has moved off the branch it was registered on. Advisory, never fatal. */
export function branchChanged(
  branchAtRegistration: string | null,
  live: BranchState
): boolean {
  if (!branchAtRegistration) return false;
  if (live.detached) return true;
  return live.branch !== null && live.branch !== branchAtRegistration;
}
