/**
 * Provenance for repository-derived answers.
 *
 * The MCP server resolves "the repository" from whatever tree hosts the compiled
 * binary (see resolveMonorepoRoot in ../paths.ts), or from ICN_ROOT when it is
 * set. That tree is not necessarily current, and it is not necessarily clean:
 * on the development host, three separate checkouts answer to the name "main" at
 * three different revisions, and orientation answers have so far carried no way
 * to tell which one produced them.
 *
 * This module does not choose a better source — that is a separate change. It
 * makes the source the server already uses describe itself, so that a stale or
 * dirty answer says so instead of looking identical to a current one.
 *
 * Failure is reported, never assumed away: when a probe cannot run, the
 * corresponding field is null and `trustworthy` is false. A source whose
 * cleanliness could not be determined must never be presented as clean.
 */

import { resolveMonorepoRoot } from "../paths.js";
import { runCommand } from "../utils/commands.js";

/** How the repository root the answer came from was chosen. */
export type SourceResolution = "ICN_ROOT" | "server_location";

export type SourceRevision = {
  /** Absolute path of the checkout this answer was produced from. */
  source_checkout: string;
  /** How that path was chosen. */
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
  /** Commits the checkout is behind its upstream; null when unknown/no upstream. */
  behind_upstream: number | null;
  /**
   * True only when the revision is known, the tree is known-clean, and it is
   * known to be level with its upstream. Any unknown makes this false.
   */
  trustworthy: boolean;
  /** Human-readable reasons `trustworthy` is false. Empty when it is true. */
  warnings: string[];
};

// Matches the git poller's budget (polling/git.ts). A cold `git status` on a checkout of a
// few thousand files whose index has not been touched for days has to re-stat the tree, and
// was observed exceeding a 5s budget on this repository — which fails closed to "unknown"
// and would report a perfectly clean reference checkout as untrusted.
const GIT_TIMEOUT_MS = 15_000;

type GitProbe =
  | { ok: true; out: string }
  | { ok: false; timedOut: boolean };

async function gitProbe(
  cwd: string,
  args: readonly string[]
): Promise<GitProbe> {
  const r = await runCommand("git", args, {
    cwd,
    timeoutMs: GIT_TIMEOUT_MS,
    maxStdoutBytes: 256 * 1024,
    maxStderrBytes: 16 * 1024,
  });
  return r.ok ? { ok: true, out: r.stdout.trim() } : { ok: false, timedOut: r.timedOut };
}

async function git(cwd: string, args: readonly string[]): Promise<string | null> {
  const r = await gitProbe(cwd, args);
  return r.ok ? r.out : null;
}

/**
 * Describe the checkout a repository-derived answer was produced from.
 *
 * `root` defaults to the same root the diagnostics themselves read, so the
 * stamp always describes the tree that actually produced the payload rather
 * than a separately resolved one.
 */
export async function describeSourceRevision(
  root?: string
): Promise<SourceRevision> {
  const resolvedFrom: SourceResolution = process.env["ICN_ROOT"]
    ? "ICN_ROOT"
    : "server_location";
  const checkout = root ?? resolveMonorepoRoot();
  const warnings: string[] = [];

  const revision = await git(checkout, ["rev-parse", "HEAD"]);
  if (!revision) {
    // Not a git tree, or git is unavailable. Everything below is unknowable.
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

  // `git status --porcelain` prints one line per changed path and nothing when
  // clean. A failed probe is "unknown", never "clean" — reporting an
  // undetermined tree as clean is the exact failure this stamp exists to remove.
  const status = await gitProbe(checkout, ["status", "--porcelain"]);
  let dirty: boolean | null = null;
  let dirtyPaths: number | null = null;
  if (!status.ok) {
    // Why it failed is reported, so an operator can tell a corrupt index from a slow one.
    const cause = status.timedOut
      ? ` (git status exceeded ${GIT_TIMEOUT_MS}ms)`
      : "";
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
  if (upstream) {
    const raw = await git(checkout, [
      "rev-list",
      "--count",
      `HEAD..${upstream}`,
    ]);
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
  } else {
    warnings.push(
      "source checkout has no upstream tracking ref; its staleness cannot be measured"
    );
  }

  const trustworthy = dirty === false && behind === 0;

  return {
    source_checkout: checkout,
    resolved_from: resolvedFrom,
    source_revision: revision,
    source_ref: ref,
    upstream,
    dirty,
    dirty_paths: dirtyPaths,
    behind_upstream: behind,
    trustworthy,
    warnings,
  };
}
