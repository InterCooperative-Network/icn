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

import { statSync } from "node:fs";
import { resolveMonorepoRoot } from "../paths.js";
import { GIT_SANITISED_ENV } from "../runtime/worktree-identity.js";
import { runCommand } from "../utils/commands.js";

/** How the repository root the answer came from was chosen. */
export type SourceResolution = "explicit_root" | "ICN_ROOT" | "server_location";

export type SourceRevision = {
  /** Absolute path of the checkout this answer was produced from. */
  source_checkout: string;
  /** How that path was chosen. An explicitly supplied root outranks the environment. */
  resolved_from: SourceResolution;
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
   * Commits behind the LOCAL tracking ref. No fetch is performed, so this is a LOWER BOUND on
   * staleness and never an overestimate — a checkout that has not fetched since the remote moved
   * reports 0 while being arbitrarily far behind. Read it together with `upstream_observed_at`.
   */
  behind_upstream: number | null;
  /** ISO time the tracking refs were last updated from the remote; null when never/unknown. */
  upstream_observed_at: string | null;
  /**
   * True only when the revision is known, the tree is known-clean, it is level with its tracking
   * ref, AND that tracking ref was refreshed recently enough to mean anything. Any unknown makes
   * this false.
   */
  trustworthy: boolean;
  /** Human-readable reasons `trustworthy` is false. Empty when it is true. */
  warnings: string[];
};

// Matches the git poller's budget (polling/git.ts). A cold `git status` on a checkout of a few
// thousand files whose index has not been touched for days has to re-stat the tree, and was
// observed exceeding a 5s budget on this repository — which fails closed to "unknown" and would
// report a perfectly clean reference checkout as untrusted.
const GIT_TIMEOUT_MS = 15_000;

/**
 * How recently the tracking refs must have been refreshed for `behind_upstream == 0` to support
 * a claim of currency. `@{u}` is a LOCAL ref: a host that never fetches shows 0 behind forever,
 * which is exactly the stale-but-confident state this module exists to expose. An hour is short
 * enough that a host syncing on any normal cadence stays certified, and long enough that the
 * flag is not permanently false on a healthy machine.
 */
const UPSTREAM_FRESHNESS_WINDOW_MS = 60 * 60 * 1000;

// Repository-configured executables are disabled per invocation rather than relying on the
// caller's git config being benign.
const GIT_SAFE_FLAGS = ["-c", "core.fsmonitor=", "-c", "core.hooksPath=/dev/null"] as const;

type GitProbe = { ok: true; out: string } | { ok: false; timedOut: boolean };

async function gitProbe(
  cwd: string,
  args: readonly string[]
): Promise<GitProbe> {
  const r = await runCommand("git", [...GIT_SAFE_FLAGS, ...args], {
    cwd,
    timeoutMs: GIT_TIMEOUT_MS,
    maxStdoutBytes: 256 * 1024,
    maxStderrBytes: 16 * 1024,
    // GIT_OPTIONAL_LOCKS=0 keeps `git status` from refreshing/writing the index of a tree this
    // probe is only observing — a diagnostic must not mutate another lane's working state.
    env: { ...GIT_SANITISED_ENV, GIT_OPTIONAL_LOCKS: "0" },
  });
  return r.ok ? { ok: true, out: r.stdout.trim() } : { ok: false, timedOut: r.timedOut };
}

async function git(cwd: string, args: readonly string[]): Promise<string | null> {
  const r = await gitProbe(cwd, args);
  return r.ok ? r.out : null;
}

/** Resolve the current HEAD of a checkout, or null when it is not a readable git tree. */
export async function headRevision(checkout: string): Promise<string | null> {
  return git(checkout, ["rev-parse", "HEAD"]);
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
  root?: string
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
      source_revision: null,
      source_ref: null,
      upstream: null,
      dirty: null,
      dirty_paths: null,
      behind_upstream: null,
      upstream_observed_at: null,
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
  const status = await gitProbe(checkout, ["status", "--porcelain"]);
  let dirty: boolean | null = null;
  let dirtyPaths: number | null = null;
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

  let behind: number | null = null;
  let observedAt: Date | null = null;
  if (upstream) {
    const raw = await git(checkout, ["rev-list", "--count", `HEAD..${upstream}`]);
    const n = raw === null ? NaN : Number.parseInt(raw, 10);
    behind = Number.isFinite(n) ? n : null;
    if (behind === null) {
      warnings.push(
        `could not measure distance from ${upstream}; treating staleness as unknown`
      );
    } else if (behind > 0) {
      warnings.push(
        `source checkout is ${behind} commit(s) behind ${upstream}; this answer describes an older revision`
      );
    }

    observedAt = await upstreamObservedAt(checkout);
    const age = observedAt === null ? null : Date.now() - observedAt.getTime();
    if (age === null) {
      warnings.push(
        `no record of ${upstream} ever being fetched, so distance from the remote is a lower bound only and currency cannot be established`
      );
    } else if (age > UPSTREAM_FRESHNESS_WINDOW_MS) {
      const hours = Math.floor(age / 3_600_000);
      warnings.push(
        `${upstream} was last fetched ${hours}h ago, so "${behind ?? "?"} behind" is a lower bound; the remote may have moved since`
      );
    }
  } else {
    warnings.push(
      "source checkout has no upstream tracking ref; its staleness cannot be measured"
    );
  }

  const upstreamFresh =
    observedAt !== null &&
    Date.now() - observedAt.getTime() <= UPSTREAM_FRESHNESS_WINDOW_MS;
  const trustworthy = dirty === false && behind === 0 && upstreamFresh;

  return {
    source_checkout: checkout,
    resolved_from: resolvedFrom,
    source_revision: revision,
    source_ref: ref,
    upstream,
    dirty,
    dirty_paths: dirtyPaths,
    behind_upstream: behind,
    upstream_observed_at: observedAt === null ? null : observedAt.toISOString(),
    trustworthy,
    warnings,
  };
}
