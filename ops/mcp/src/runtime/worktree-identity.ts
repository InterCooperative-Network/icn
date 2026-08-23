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
import { existsSync, realpathSync } from "fs";
import { basename } from "path";

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
    }).trim();
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
  const candidates: string[] = [];
  if (startDir && existsSync(startDir)) candidates.push(startDir);
  if (envRoot && existsSync(envRoot)) candidates.push(envRoot);

  for (const dir of candidates) {
    const gitDir = canonical(git(dir, ["rev-parse", "--absolute-git-dir"]));
    if (!gitDir) continue;

    // --git-common-dir may be relative to the worktree; resolve it from there.
    const commonRaw = git(dir, ["rev-parse", "--git-common-dir"]);
    const common = canonical(
      commonRaw && !commonRaw.startsWith("/") ? `${git(dir, ["rev-parse", "--show-toplevel"])}/${commonRaw}` : commonRaw
    );
    const top = canonical(git(dir, ["rev-parse", "--show-toplevel"]));
    if (!common || !top) continue;

    // A bare repo has no working tree; it is not a lane.
    if (git(dir, ["rev-parse", "--is-bare-repository"]) === "true") continue;

    return {
      repo_id: common,
      repo_name: basename(common).replace(/\.git$/, ""),
      worktree_id: gitDir,
      worktree_path: top,
      worktree_name: basename(top),
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
