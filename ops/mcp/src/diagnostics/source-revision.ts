/**
 * Provenance for repository-derived answers.
 *
 * The MCP server resolves "the repository" from whatever tree hosts the compiled binary (see
 * resolveMonorepoRoot in ../paths.ts), or from ICN_ROOT when it is set. That tree is not
 * necessarily current, and it is not necessarily clean: on the development host, three separate
 * checkouts answer to the name "main" at three different revisions, and orientation answers have
 * so far carried no way to tell which one produced them.
 *
 * This module does not choose a better source — that is a separate change. It makes the source
 * the server already uses describe itself, so that a stale or dirty answer says so instead of
 * looking identical to a current one.
 *
 * Three properties the implementation must hold, each of which is a way this could have
 * silently described the wrong thing:
 *
 *  1. FAILURE IS REPORTED, NEVER ASSUMED AWAY. When a probe cannot run, its field is null and
 *     `trustworthy` is false. A source whose cleanliness could not be determined must never be
 *     presented as clean.
 *
 *  2. THE ENVIRONMENT MUST NOT REDIRECT THE PROBE. `GIT_DIR` overrides `-C`, and git exports it
 *     into child processes in a linked worktree — the only kind ICN uses. Inherited, it would
 *     let this report `source_checkout: A` while describing the revision of repository B, which
 *     is precisely the misattribution the stamp exists to remove. The sanitised environment is
 *     shared with ../runtime/worktree-identity.ts rather than duplicated.
 *
 *  3. A READ-ONLY PROBE MUST NOT EXECUTE REPOSITORY-CONFIGURED CODE. `git status` honours
 *     `core.fsmonitor`, which may name an executable. Callers can supply the path being probed,
 *     so an unguarded probe would run that binary with the server's authority. Executable
 *     integrations are disabled per-invocation, and optional locks are dropped so the probe does
 *     not write to a tree it is only supposed to observe.
 */

import { createHash } from "node:crypto";
import { statSync } from "node:fs";
import { resolveMonorepoRoot } from "../paths.js";
import { GIT_SANITISED_ENV } from "../runtime/worktree-identity.js";
import { runCommand } from "../utils/commands.js";

/** How the repository root the answer came from was chosen. */
export type SourceResolution = "explicit_root" | "ICN_ROOT" | "server_location";

/**
 * Whether the checkout being described was chosen by the operator or by the caller.
 *
 * This is a trust boundary, not a configuration list. Inspecting a working tree makes git apply
 * repository-controlled behaviour to its contents — `.gitattributes` plus a `filter.<driver>.clean`
 * command is executed by `git status` (verified with git 2.43), and it is not the only such
 * mechanism, nor a bounded set. Reading refs and objects does not: `rev-parse` and `rev-list`
 * touch no working-tree content and run no filters.
 *
 * So a caller-selected checkout gets ref and object reads only. Its cleanliness is reported as
 * indeterminate rather than probed, because the probe is the exposure. Enumerating individual git
 * knobs was tried and abandoned as the wrong abstraction; removing the dependence on caller
 * working trees altogether is R1-B's job, and this is the bounded bridge until then.
 */
export type SourceProvenance = "controlled" | "caller_selected";

export type SourceRevision = {
  /** Absolute path of the checkout this answer was produced from. */
  source_checkout: string;
  /** How that path was chosen. An explicitly supplied root outranks the environment. */
  resolved_from: SourceResolution;
  /**
   * Whether the operator or the caller chose this checkout. A `caller_selected` source is never
   * inspected through its working tree, so its cleanliness fields are null by design rather than
   * by failure — see SourceProvenance.
   */
  source_provenance: SourceProvenance;
  /** Full commit SHA of that checkout's HEAD; null when it is not a git tree. */
  source_revision: string | null;
  /** Branch name, or "HEAD" when detached; null when unknown. */
  source_ref: string | null;
  /** Upstream tracking ref (e.g. "origin/main"); null when there is none. */
  upstream: string | null;
  /** True/false when determined; null when the probe could not run. */
  dirty: boolean | null;
  /** Number of paths reported by `git status --porcelain`; null when unknown. */
  dirty_paths: number | null;
  /**
   * Tracked paths carrying `assume-unchanged` or `skip-worktree`. Git omits these from `status`
   * entirely, so a modified file marked either way leaves a checkout looking spotless. A
   * non-zero count means `dirty` is not a complete answer, however clean it looks.
   */
  index_hidden_paths: number | null;
  /**
   * Commits behind the LOCAL tracking ref. No fetch is performed, so this is a LOWER BOUND on
   * staleness and never an overestimate — a checkout that has not fetched since the remote moved
   * reports 0 while being arbitrarily far behind. Read it together with `remote_currency`.
   */
  behind_upstream: number | null;
  /**
   * Commits the checkout holds that its tracking ref does not. `HEAD..upstream` counts only one
   * direction, so a clean branch carrying an unpushed commit is "0 behind" while containing
   * work that exists nowhere else — not level, and not something an orientation answer should
   * present as canonical.
   */
  ahead_of_upstream: number | null;
  /**
   * ISO mtime of FETCH_HEAD: evidence that SOME fetch happened. It is repository-wide and
   * refspec-dependent — `git fetch origin somebranch` refreshes it without touching this
   * upstream — so it is an upper bound on this ref's freshness and never proof about it.
   */
  upstream_observed_at: string | null;
  /**
   * Always "unverified" here. Git records no per-ref "when was this last checked against the
   * remote" (remote-tracking refs carry no reflog in this configuration), so no local evidence
   * can establish currency. Only an actual fetch can, which is R1-B's job.
   */
  remote_currency: "unverified";
  /**
   * True when the revision is known, the tree is known-clean, and it is exactly level with its
   * tracking ref in BOTH directions. Any unknown makes it false. This is a statement about
   * local faithfulness only — it deliberately does not claim the remote has not moved, because
   * nothing available here could prove that.
   */
  trustworthy: boolean;
  /**
   * Conditions a reader must account for, including staleness risk. A non-empty list does not
   * by itself mean `trustworthy` is false: a checkout can be perfectly faithful to a tracking
   * ref that is itself old.
   */
  warnings: string[];
};

// Matches the git poller's budget (polling/git.ts). A cold `git status` on a checkout of a few
// thousand files whose index has not been touched for days has to re-stat the tree, and was
// observed exceeding a 5s budget on this repository — which fails closed to "unknown" and would
// report a perfectly clean reference checkout as untrusted.
const GIT_TIMEOUT_MS = 15_000;

/**
 * Age beyond which fetch evidence is reported as too old to reason about. `@{u}` is a LOCAL
 * ref: a host that never fetches shows 0 behind forever, which is the stale-but-confident state
 * this module exists to expose. This drives a WARNING, not the `trustworthy` flag — see
 * `remote_currency`: the available evidence (repository-wide FETCH_HEAD) cannot be tied to a
 * particular tracking ref, so it can raise suspicion but must not be used to certify.
 */
const UPSTREAM_STALE_AFTER_MS = 60 * 60 * 1000;

// Repository-configured executables are disabled per invocation rather than relying on the
// caller's git config being benign.
const GIT_SAFE_FLAGS = ["-c", "core.fsmonitor=", "-c", "core.hooksPath=/dev/null"] as const;

/**
 * `status.showUntrackedFiles=no` in the probed repository would hide untracked files, letting a
 * tree holding uncommitted content report clean. The mode is passed explicitly so repository
 * config cannot suppress the check — the same reasoning as disabling core.fsmonitor: a probe
 * must not let the thing it is inspecting decide how thoroughly it is inspected.
 */
const STATUS_ARGS = ["status", "--porcelain", "--untracked-files=normal"] as const;

type GitProbe =
  | { ok: true; out: string }
  | { ok: false; timedOut: boolean; truncated?: boolean };

/**
 * Output budget for a single probe. `ls-files -v` scales with the size of the repository, so a
 * small budget is a correctness problem and not merely a display one.
 */
const GIT_MAX_OUTPUT_BYTES = 8 * 1024 * 1024;

/**
 * runCommand truncates oversized output and still reports success, appending a marker. For a
 * probe that COUNTS things, a silently shortened list reads as "fewer findings" rather than
 * "incomplete answer" — a flagged file past the cutoff would simply not be seen. Truncation is
 * therefore a probe failure here, which fails closed like any other unknown.
 */
export function wasTruncated(out: string): boolean {
  return /\n… \[truncated \d+ chars\]$/.test(out);
}

async function gitProbe(
  cwd: string,
  args: readonly string[]
): Promise<GitProbe> {
  // `--work-tree` pins the probe to the directory being described. `core.worktree` in the
  // probed repository would otherwise redirect it: a modified checkout whose config points at a
  // clean copy reports an empty status, so locally modified payload data would be stamped
  // clean. Verified with git 2.43. This completes the family the sanitised environment,
  // `core.fsmonitor` and `--untracked-files` belong to — a repository must not get to decide
  // how, or where, it is inspected.
  const r = await runCommand("git", ["--work-tree", cwd, ...GIT_SAFE_FLAGS, ...args], {
    cwd,
    timeoutMs: GIT_TIMEOUT_MS,
    maxStdoutBytes: GIT_MAX_OUTPUT_BYTES,
    maxStderrBytes: 16 * 1024,
    // GIT_OPTIONAL_LOCKS=0 keeps `git status` from refreshing/writing the index of a tree this
    // probe is only observing — a diagnostic must not mutate another lane's working state.
    env: { ...GIT_SANITISED_ENV, GIT_OPTIONAL_LOCKS: "0" },
  });
  if (!r.ok) return { ok: false, timedOut: r.timedOut };
  if (wasTruncated(r.stdout)) return { ok: false, timedOut: false, truncated: true };
  return { ok: true, out: r.stdout.trim() };
}

async function git(cwd: string, args: readonly string[]): Promise<string | null> {
  const r = await gitProbe(cwd, args);
  return r.ok ? r.out : null;
}

/** Resolve the current HEAD of a checkout, or null when it is not a readable git tree. */
export async function headRevision(checkout: string): Promise<string | null> {
  return git(checkout, ["rev-parse", "HEAD"]);
}

/**
 * A snapshot of everything a repository-derived payload could have been read from: the commit
 * AND the working tree on top of it.
 *
 * Guarding a read with HEAD alone is not enough. A dirty tree can shape a payload and then be
 * cleaned — `git restore`, `git reset --hard HEAD` — without HEAD ever moving, so the two
 * revision probes agree while the payload describes content that no longer exists. Comparing
 * this across the read closes that whole class rather than the commit-shaped instance of it.
 *
 * `null` means the state could not be determined, which never compares equal to anything.
 */
export async function worktreeFingerprint(
  checkout: string,
  provenance: SourceProvenance = "controlled"
): Promise<string | null> {
  const head = await headRevision(checkout);
  if (head === null) return null;
  if (provenance === "caller_selected") {
    // The working tree of a caller-selected checkout is never read, so the guard degrades to the
    // commit alone. Nothing is certified from such a source anyway — `trustworthy` is already
    // false — so this weakens no claim that was being made.
    return `${head}:worktree-not-inspected`;
  }
  const status = await gitProbe(checkout, STATUS_ARGS);
  if (!status.ok) return null;
  return `${head}:${createHash("sha256").update(status.out).digest("hex")}`;
}

/** When the tracking refs were last updated from the remote, via FETCH_HEAD's mtime. */
async function upstreamObservedAt(checkout: string): Promise<Date | null> {
  const p = await git(checkout, ["rev-parse", "--git-path", "FETCH_HEAD"]);
  if (!p) return null;
  try {
    return statSync(p.startsWith("/") ? p : `${checkout}/${p}`).mtime;
  } catch {
    return null;
  }
}

/**
 * Describe the checkout a repository-derived answer was produced from.
 *
 * `root` defaults to the same root the diagnostics themselves read, so the stamp always
 * describes the tree that actually produced the payload rather than a separately resolved one.
 */
export async function describeSourceRevision(
  root?: string,
  provenance: SourceProvenance = "controlled"
): Promise<SourceRevision> {
  // An explicitly supplied root is how it was chosen, regardless of what the environment says.
  // icn_ops_agent_runtime deliberately overrides ICN_ROOT with the caller's lane; reporting
  // "ICN_ROOT" there would contradict the path in source_checkout.
  const resolvedFrom: SourceResolution = root
    ? "explicit_root"
    : process.env["ICN_ROOT"]
      ? "ICN_ROOT"
      : "server_location";
  const checkout = root ?? resolveMonorepoRoot();
  const warnings: string[] = [];

  const revision = await headRevision(checkout);
  if (!revision) {
    warnings.push(
      `source checkout is not a readable git tree (${checkout}); this answer cannot be tied to a revision`
    );
    return {
      source_checkout: checkout,
      resolved_from: resolvedFrom,
      source_provenance: provenance,
      source_revision: null,
      source_ref: null,
      upstream: null,
      dirty: null,
      dirty_paths: null,
      index_hidden_paths: null,
      behind_upstream: null,
      ahead_of_upstream: null,
      upstream_observed_at: null,
      remote_currency: "unverified",
      trustworthy: false,
      warnings,
    };
  }

  const ref = await git(checkout, ["rev-parse", "--abbrev-ref", "HEAD"]);
  const upstream = await git(checkout, [
    "rev-parse",
    "--abbrev-ref",
    "--symbolic-full-name",
    "@{u}",
  ]);

  // `git status --porcelain` prints one line per changed path and nothing when clean. A failed
  // probe is "unknown", never "clean" — reporting an undetermined tree as clean is the exact
  // failure this stamp exists to remove.
  // THE BRIDGE. A caller-selected checkout is never inspected through its working tree: doing so
  // is what lets the repository run code (see SourceProvenance). Cleanliness is therefore
  // reported as indeterminate — null, not false — and nothing is certified from it. The revision
  // identity established above came from ref reads and remains valid.
  const inspectWorkingTree = provenance === "controlled";

  let dirty: boolean | null = null;
  let dirtyPaths: number | null = null;
  let indexHidden: number | null = null;

  if (!inspectWorkingTree) {
    warnings.push(
      "cleanliness was not probed: this checkout was selected by the caller, and inspecting a " +
        "working tree can execute repository-defined behaviour. Revision identity is reported; " +
        "cleanliness is indeterminate and this source cannot be certified"
    );
  } else {
  const status = await gitProbe(checkout, STATUS_ARGS);
  if (!status.ok) {
    const cause = status.timedOut ? ` (git status exceeded ${GIT_TIMEOUT_MS}ms)` : "";
    warnings.push(
      `could not determine whether the source checkout is clean${cause}; treating it as untrusted`
    );
  } else {
    dirtyPaths = status.out.length === 0 ? 0 : status.out.split("\n").length;
    dirty = dirtyPaths > 0;
    if (dirty) {
      warnings.push(
        `source checkout has ${dirtyPaths} uncommitted path(s); this answer may describe unreviewed local edits`
      );
    }
  }

  // `status` is not the whole story: `git update-index --assume-unchanged` and `--skip-worktree`
  // make git omit a tracked file from status even when it is modified. `ls-files -v` reports a
  // per-path status letter, where a lowercase letter means assume-unchanged and "S" means
  // skip-worktree. This is the fifth mechanism by which the probed repository could hide its
  // own state from the probe; like the others, the answer is to look anyway.
  const lsFiles = await gitProbe(checkout, ["ls-files", "-v"]);
  if (!lsFiles.ok) {
    const why = lsFiles.truncated
      ? " (the index listing was too large to read in full)"
      : "";
    warnings.push(
      `could not check for index flags that hide tracked files from status${why}; treating cleanliness as incomplete`
    );
  } else {
    indexHidden = lsFiles.out
      .split("\n")
      .filter((l) => l.length > 0)
      .filter((l) => {
        const c = l[0] as string;
        return c === "S" || (c >= "a" && c <= "z");
      }).length;
    if (indexHidden > 0) {
      warnings.push(
        `${indexHidden} tracked path(s) carry assume-unchanged or skip-worktree, so git omits them from status; this checkout may hold modifications that cannot be seen`
      );
    }
  }
  }

  let behind: number | null = null;
  let ahead: number | null = null;
  let observedAt: Date | null = null;
  if (upstream) {
    // Symmetric difference. `HEAD..upstream` counts one direction only, so a clean branch with
    // an unpushed commit would read as "0 behind" and pass for level while holding work that
    // exists nowhere else.
    const raw = await git(checkout, [
      "rev-list",
      "--left-right",
      "--count",
      `HEAD...${upstream}`,
    ]);
    const parts = raw === null ? [] : raw.split(/\s+/).filter(Boolean);
    const a = parts.length === 2 ? Number.parseInt(parts[0] as string, 10) : NaN;
    const b = parts.length === 2 ? Number.parseInt(parts[1] as string, 10) : NaN;
    ahead = Number.isFinite(a) ? a : null;
    behind = Number.isFinite(b) ? b : null;
    if (ahead === null || behind === null) {
      warnings.push(
        `could not measure divergence from ${upstream}; treating staleness as unknown`
      );
    } else {
      if (behind > 0) {
        warnings.push(
          `source checkout is ${behind} commit(s) behind ${upstream}; this answer describes an older revision`
        );
      }
      if (ahead > 0) {
        warnings.push(
          `source checkout holds ${ahead} commit(s) not in ${upstream}; this answer includes work that exists nowhere else`
        );
      }
    }

    observedAt = await upstreamObservedAt(checkout);
    const age = observedAt === null ? null : Date.now() - observedAt.getTime();
    if (age === null) {
      warnings.push(
        `no record of any fetch in this checkout, so distance from ${upstream} is a lower bound and the remote may have moved`
      );
    } else if (age > UPSTREAM_STALE_AFTER_MS) {
      const hours = Math.floor(age / 3_600_000);
      warnings.push(
        `last fetch in this checkout was ${hours}h ago, so distance from ${upstream} is a lower bound; the remote may have moved since`
      );
    }
  } else {
    warnings.push(
      "source checkout has no upstream tracking ref; its staleness cannot be measured"
    );
  }

  // Local faithfulness only. Remote currency is deliberately excluded: see `remote_currency`.
  // `inspectWorkingTree` is not redundant with the null checks: it states the intent directly, so
  // a future change that gives these fields a non-null default cannot quietly re-certify an
  // uninspected source.
  const trustworthy =
    inspectWorkingTree &&
    dirty === false &&
    indexHidden === 0 &&
    behind === 0 &&
    ahead === 0;

  return {
    source_checkout: checkout,
    resolved_from: resolvedFrom,
    source_provenance: provenance,
    source_revision: revision,
    source_ref: ref,
    upstream,
    dirty,
    dirty_paths: dirtyPaths,
    index_hidden_paths: indexHidden,
    behind_upstream: behind,
    ahead_of_upstream: ahead,
    upstream_observed_at: observedAt === null ? null : observedAt.toISOString(),
    remote_currency: "unverified",
    trustworthy,
    warnings,
  };
}
