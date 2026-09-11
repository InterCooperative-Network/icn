/**
 * Revision-addressed reads from controlled canonical storage.
 *
 * WHY THIS EXISTS
 *
 * Answering "what does the repository say at revision X" by inspecting a working tree makes the
 * answer depend on mutable state that the caller may control, and makes git apply
 * repository-defined behaviour while doing it. The previous slice (R1-A) established a safety
 * boundary for that: untrusted roots get ref resolution only, and cleanliness and divergence
 * become indeterminate where they cannot be established safely.
 *
 * That boundary is a mitigation, and mitigations of this kind are open-ended — six distinct
 * mechanisms were found by which a repository could redirect or execute during its own
 * inspection, and nothing suggests six is the total. This module removes the dependency instead
 * of defending it:
 *
 *   committed content is read from a BARE repository that ICN controls, addressed by an
 *   immutable revision, with no working tree involved at any point.
 *
 * Measured (git 2.43), and the reason this works rather than merely being tidier:
 *
 *   - `git --git-dir=<bare> cat-file blob <rev>:<path>` returns the object's canonical content
 *     and invokes no clean/smudge filter. The same content fetched through a working tree came
 *     back transformed by a filter, and the filter ran.
 *   - `git clone --bare` does not carry the source repository's local config, so filter driver
 *     definitions do not exist in canonical storage. A committed `.gitattributes` can NAME a
 *     filter, but a filter only runs when its driver is DEFINED in config, which is ours.
 *
 * The structural claim is therefore not "we disabled the dangerous knobs" but "there is no
 * working tree to inspect and the configuration is ours". Whole families of working-tree and
 * repository-configuration attacks become inapplicable rather than blocked.
 *
 * WHAT THIS MODULE DELIBERATELY DOES NOT DO
 *
 * It never accepts a repository location from a caller. `resolveCanonicalRepository()` reads
 * operator-controlled configuration and nothing else; there is no parameter through which a
 * request can point it somewhere. It also never falls back to a working tree: when canonical
 * storage cannot answer, the answer is an error.
 */

import { readFileSync } from "node:fs";
import path from "node:path";
import { resolveMonorepoRoot } from "../paths.js";
import { GIT_SANITISED_ENV } from "../runtime/worktree-identity.js";
import { runCommand } from "../utils/commands.js";

/**
 * A repository ICN controls, suitable for revision-addressed reads.
 *
 * Only `resolveCanonicalRepository()` constructs one, and only after verifying the store is
 * bare. The private brand is what stops a plain path string being passed where this is required
 * — the type is the capability, so "read committed content from somewhere the caller named"
 * cannot be expressed.
 */
export type CanonicalRepository = {
  readonly gitDir: string;
  readonly [CANONICAL_BRAND]: true;
};

declare const CANONICAL_BRAND: unique symbol;

/**
 * An immutable, fully-resolved commit id.
 *
 * Branded for the same reason: a branch name is not a revision. A ref can move between the
 * moment it is read and the moment it is used, so a ref is resolved to one of these ONCE and
 * this is what every subsequent read carries.
 */
export type Revision = string & { readonly [REVISION_BRAND]: true };

declare const REVISION_BRAND: unique symbol;

export type CanonicalError = {
  code:
    | "not_configured"
    | "not_a_git_dir"
    | "not_bare"
    | "unknown_revision"
    | "missing_path"
    | "read_failed";
  message: string;
};

export type CanonicalResult<T> =
  | { ok: true; value: T }
  | { ok: false; error: CanonicalError };

const GIT_TIMEOUT_MS = 15_000;

/**
 * Canonical storage is read with the same hardening as every other git call, plus
 * GIT_NO_LAZY_FETCH: a store configured with a promisor remote could otherwise turn a missing
 * object into a network call to a helper. Canonical storage is ours, so this should never fire
 * — which is exactly why it costs nothing to assert.
 */
async function canonicalGit(
  gitDir: string,
  args: readonly string[]
): Promise<{ ok: boolean; out: string; err: string }> {
  const r = await runCommand(
    "git",
    ["--git-dir", gitDir, "-c", "core.fsmonitor=", "-c", "core.hooksPath=/dev/null", ...args],
    {
      timeoutMs: GIT_TIMEOUT_MS,
      maxStdoutBytes: 8 * 1024 * 1024,
      maxStderrBytes: 16 * 1024,
      env: {
        ...GIT_SANITISED_ENV,
        GIT_OPTIONAL_LOCKS: "0",
        GIT_NO_LAZY_FETCH: "1",
      },
    }
  );
  return { ok: r.ok, out: r.stdout.trim(), err: r.stderr.trim() };
}

/** Where canonical storage is declared, in the config ICN already owns. */
const REPO_MAP_REL = path.join("ops", "state", "config", "repo-map.json");
const CANONICAL_STORE_FIELD = "local_store";

function expandHome(p: string): string {
  const home = process.env["HOME"];
  if (home && (p === "~" || p.startsWith("~/"))) return path.join(home, p.slice(1));
  return p;
}

/**
 * Resolve the canonical repository for the ICN monorepo from operator-controlled config.
 *
 * Takes no arguments on purpose. A caller cannot influence which repository is read, because
 * there is nothing to pass.
 */
export async function resolveCanonicalRepository(): Promise<
  CanonicalResult<CanonicalRepository>
> {
  const mapPath = path.join(resolveMonorepoRoot(), REPO_MAP_REL);
  let declared: unknown;
  try {
    const raw = readFileSync(mapPath, "utf-8");
    declared = (
      JSON.parse(raw) as {
        repos?: Record<string, Record<string, unknown>>;
      }
    ).repos?.["icn"]?.[CANONICAL_STORE_FIELD];
  } catch (e) {
    return {
      ok: false,
      error: {
        code: "not_configured",
        message: `could not read ${REPO_MAP_REL} (${e instanceof Error ? e.message : String(e)})`,
      },
    };
  }
  if (typeof declared !== "string" || declared.length === 0) {
    return {
      ok: false,
      error: {
        code: "not_configured",
        message:
          `no canonical store declared: set repos.icn.${CANONICAL_STORE_FIELD} in ${REPO_MAP_REL} ` +
          "to a bare repository ICN controls",
      },
    };
  }

  const gitDir = path.resolve(expandHome(declared));

  // The bare check is the invariant, not a sanity check. A bare repository has no working tree,
  // so the entire class of "inspecting the tree ran something" is inapplicable by construction
  // rather than by mitigation. A non-bare store would silently re-open it.
  const bare = await canonicalGit(gitDir, ["rev-parse", "--is-bare-repository"]);
  if (!bare.ok) {
    return {
      ok: false,
      error: {
        code: "not_a_git_dir",
        message: `declared canonical store is not a readable git directory: ${gitDir}`,
      },
    };
  }
  if (bare.out !== "true") {
    return {
      ok: false,
      error: {
        code: "not_bare",
        message:
          `declared canonical store ${gitDir} is not bare; canonical reads require a store with ` +
          "no working tree, so that no working-tree state can affect a revision-addressed answer",
      },
    };
  }

  return { ok: true, value: { gitDir } as CanonicalRepository };
}

/**
 * Resolve a ref to an immutable revision, once.
 *
 * Callers hold the result for the rest of the operation instead of re-reading the ref. A branch
 * read twice is two different questions; a Revision read twice is the same answer.
 */
export async function resolveRevision(
  repo: CanonicalRepository,
  ref: string
): Promise<CanonicalResult<Revision>> {
  const r = await canonicalGit(repo.gitDir, ["rev-parse", "--verify", "--end-of-options", `${ref}^{commit}`]);
  if (!r.ok || !/^[0-9a-f]{40}$/.test(r.out)) {
    return {
      ok: false,
      error: {
        code: "unknown_revision",
        message: `canonical store cannot resolve '${ref}' to a commit`,
      },
    };
  }
  return { ok: true, value: r.out as Revision };
}

/**
 * Read one committed file's content at an exact revision.
 *
 * No working tree is consulted, so no working-tree state — content, index, cleanliness, or
 * configuration — can influence the result. If the revision or path is not present in canonical
 * storage this fails; it does not look anywhere else. Falling back to a workspace would
 * reintroduce precisely the dependency this module exists to remove.
 */
export async function readFileAtRevision(
  repo: CanonicalRepository,
  revision: Revision,
  repoRelativePath: string
): Promise<CanonicalResult<string>> {
  // Normalised and rejected rather than resolved: a path escaping the repository root is not a
  // repository-relative path, and `..` has no meaning inside a tree object.
  const normalised = path.posix.normalize(repoRelativePath.replace(/\\/g, "/"));
  if (
    normalised.startsWith("/") ||
    normalised === ".." ||
    normalised.startsWith("../")
  ) {
    return {
      ok: false,
      error: {
        code: "missing_path",
        message: `not a repository-relative path: ${repoRelativePath}`,
      },
    };
  }

  const r = await canonicalGit(repo.gitDir, [
    "cat-file",
    "blob",
    `${revision}:${normalised}`,
  ]);
  if (!r.ok) {
    return {
      ok: false,
      error: {
        code: "missing_path",
        message: `${normalised} is not present at ${revision.slice(0, 12)} in canonical storage`,
      },
    };
  }
  return { ok: true, value: r.out };
}
