// The four invariants that Round 8 found documented, fixed, and COMPLETELY UNTESTED.
//
// Each one had a comment in the source explaining a defect it had already caused in
// production. Reviewer A applied a valid mutant restoring each defect and watched the entire
// 184-test suite stay green. A fix nobody can break in a test is a fix that comes back.

import { describe, it, expect, beforeAll, afterAll, vi } from "vitest";
import { execFileSync } from "child_process";
import { mkdtempSync, rmSync, symlinkSync, writeFileSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { resolveDbPath, resolveDefaultDbPath } from "../state/db.js";
import { discoverWorktree } from "../runtime/worktree-identity.js";

let root: string;
let repo: string;
let wt: string;
let otherRepo: string;

function git(dir: string, ...args: string[]): string {
  return execFileSync("git", ["-C", dir, ...args], {
    encoding: "utf-8",
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@e",
           GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@e" },
  }).trim();
}

function makeRepo(path: string): void {
  mkdirSync(path, { recursive: true });
  git(path, "init", "-q", "-b", "main", ".");
  writeFileSync(join(path, "f.txt"), "one\n");
  git(path, "add", "f.txt");
  git(path, "commit", "-qm", "init");
}

beforeAll(() => {
  root = mkdtempSync(join(tmpdir(), "icn-plumbing-"));
  repo = join(root, "repo");
  otherRepo = join(root, "other");
  makeRepo(repo);
  makeRepo(otherRepo);
  wt = join(root, "lane");
  git(repo, "worktree", "add", "-q", "-b", "lane", wt);
});
afterAll(() => rmSync(root, { recursive: true, force: true }));

// ── A5-3: the registry is ONE database per REPOSITORY ────────────────────────
describe("default registry path is derived from the repository, not from the executing JS", () => {
  it("resolves to the git common dir for a worktree, a subdirectory, and the repo itself", () => {
    const common = git(wt, "rev-parse", "--path-format=absolute", "--git-common-dir");
    const expected = join(common, "icn-ops.db");

    expect(resolveDefaultDbPath(wt)).toBe(expected);
    // A subdirectory must resolve to the SAME registry — `--git-common-dir` can be relative,
    // and resolving it against the toplevel instead of the git cwd produced a different path
    // per directory depth.
    const sub = join(wt, "deep", "deeper");
    mkdirSync(sub, { recursive: true });
    expect(resolveDefaultDbPath(sub)).toBe(expected);
    expect(resolveDefaultDbPath(repo)).toBe(expected);
  });

  it("normalises symlinks, so one repository cannot get two registries", () => {
    // `realpathSync` here is load-bearing and was untested: dropping it left `tsc` clean and
    // all 250 tests green, while a repo reached through a symlinked parent resolved to a
    // DIFFERENT database file than the same repo reached directly. That is the exact
    // per-worktree split this function exists to close, reintroduced one path component up.
    const linkedParent = join(root, "link-to-root");
    symlinkSync(root, linkedParent);
    const viaLink = join(linkedParent, "repo");
    expect(resolveDefaultDbPath(viaLink)).toBe(resolveDefaultDbPath(repo));

    // ...and through a symlinked worktree as well.
    const wtLink = join(root, "link-to-lane");
    symlinkSync(wt, wtLink);
    expect(resolveDefaultDbPath(wtLink)).toBe(resolveDefaultDbPath(wt));
    // The resolved path must contain no symlink component at all.
    expect(resolveDefaultDbPath(viaLink)).not.toContain("link-to-root");
  });

  it("gives DIFFERENT repositories different registries", () => {
    expect(resolveDefaultDbPath(repo)).not.toBe(resolveDefaultDbPath(otherRepo));
  });

  it("does not silently return the per-worktree legacy path from inside a repo", () => {
    // The legacy path sits beside the executing JS (dist/state/../../data). Returning it is the
    // exact split this function exists to close: a hook-registered session invisible to every
    // MCP tool because they were writing to two different files.
    expect(resolveDefaultDbPath(wt)).not.toMatch(/\/data\/icn-ops\.db$/);
  });
});

// ── A5-4: an empty ICN_OPS_DB is not a path ─────────────────────────────────
describe("ICN_OPS_DB is only honoured when it names something", () => {
  const saved = process.env["ICN_OPS_DB"];
  afterAll(() => {
    if (saved === undefined) delete process.env["ICN_OPS_DB"];
    else process.env["ICN_OPS_DB"] = saved;
  });

  it("treats empty and whitespace-only as UNSET, never as a filename", () => {
    for (const bogus of ["", " ", "\t", "\n", "   "]) {
      process.env["ICN_OPS_DB"] = bogus;
      const got = resolveDbPath();
      // "" makes better-sqlite3 open an anonymous TEMPORARY database: the row is written, the
      // process exits, and the very next `status` reports not-registered.
      expect(got).not.toBe(bogus);
      expect(got.trim()).not.toBe("");
      expect(got).toBe(resolveDefaultDbPath());
    }
  });

  it("honours a real value, and an explicit argument outranks the environment", () => {
    process.env["ICN_OPS_DB"] = "/tmp/some-explicit.db";
    expect(resolveDbPath()).toBe("/tmp/some-explicit.db");
    expect(resolveDbPath("/tmp/argument-wins.db")).toBe("/tmp/argument-wins.db");
  });
});

// ── A5-2: inherited Git environment must not redirect lane identity ─────────
describe("inherited GIT_* variables cannot misattribute a lane", () => {
  const GIT_VARS = [
    "GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_CEILING_DIRECTORIES",
  ];
  const saved: Record<string, string | undefined> = {};
  beforeAll(() => { for (const k of GIT_VARS) saved[k] = process.env[k]; });
  afterAll(() => {
    for (const k of GIT_VARS) {
      if (saved[k] === undefined) delete process.env[k];
      else process.env[k] = saved[k]!;
    }
  });

  /**
   * Load a FRESH copy of the module with the contamination already in the environment.
   *
   * THIS IS THE WHOLE POINT. `GIT_SANITISED_ENV` is an import-time IIFE snapshot of
   * `process.env`, so setting `process.env.GIT_DIR` in a test BODY changes nothing the module
   * ever reads — the earlier version of these tests did exactly that, and deleting the entire
   * `delete e[k]` sanitisation loop passed 13/13. (It caught only a different mutant, one that
   * read the ambient environment at call time.)
   *
   * Production hits the contaminated case at process START: git EXPORTS `GIT_DIR` into hooks
   * when running in a linked worktree, which is the only kind ICN uses. So the variable must be
   * set BEFORE the module is imported, which is what `resetModules` + dynamic import buys.
   */
  async function freshDiscover(env: Record<string, string>) {
    for (const [k, v] of Object.entries(env)) process.env[k] = v;
    vi.resetModules();
    const mod = await import("../runtime/worktree-identity.js");
    return mod.discoverWorktree;
  }

  it("resolves the SAME lane when GIT_DIR was already set at module load", async () => {
    const clean = discoverWorktree(wt);
    expect(clean).not.toBeNull();

    const discover = await freshDiscover({
      GIT_DIR: join(otherRepo, ".git"),
      GIT_COMMON_DIR: join(otherRepo, ".git"),
      GIT_WORK_TREE: otherRepo,
    });

    const dirty = discover(wt);
    expect(dirty).not.toBeNull();
    expect(dirty!.repo_id).toBe(clean!.repo_id);
    expect(dirty!.worktree_id).toBe(clean!.worktree_id);
    expect(dirty!.repo_name).toBe(clean!.repo_name);
    // ...and specifically NOT the repository the environment points at.
    expect(dirty!.repo_id).not.toContain("other");
  });

  it("keeps two lanes DISTINCT under a contaminated environment", async () => {
    // The observed defect was worse than a wrong name: two different lanes collapsed onto ONE
    // worktree_id pointing at an unrelated repository, so a session in lane A and a session in
    // lane B looked like co-occupants of the same lane.
    const second = join(root, "lane2");
    git(repo, "worktree", "add", "-q", "-b", "lane2", second);

    const discover = await freshDiscover({
      GIT_DIR: join(otherRepo, ".git"),
      GIT_COMMON_DIR: join(otherRepo, ".git"),
      GIT_WORK_TREE: otherRepo,
    });

    const a = discover(wt);
    const b = discover(second);
    expect(a).not.toBeNull();
    expect(b).not.toBeNull();
    expect(a!.worktree_id).not.toBe(b!.worktree_id);
    // Each must still name its OWN lane, not the unrelated repo the env advertises.
    expect(a!.worktree_id).toContain("lane");
    expect(b!.worktree_id).toContain("lane2");
  });
});

// ── B6: lane resolution, and what may NEVER decide a lane ───────────────────
describe("lane resolution", () => {
  const saved = process.env["ICN_ROOT"];
  afterAll(() => {
    if (saved === undefined) delete process.env["ICN_ROOT"];
    else process.env["ICN_ROOT"] = saved;
  });

  it("resolves from the worktree itself and from a deep subdirectory", () => {
    const sub = join(wt, "a", "b", "c");
    mkdirSync(sub, { recursive: true });
    expect(discoverWorktree(sub)!.worktree_id).toBe(discoverWorktree(wt)!.worktree_id);
  });

  it("resolves identically through a symlink to the worktree", () => {
    const link = join(root, "wt-symlink");
    symlinkSync(wt, link);
    expect(discoverWorktree(link)!.worktree_id).toBe(discoverWorktree(wt)!.worktree_id);
  });

  it("REFUSES a non-Git cwd, and an ambient ICN_ROOT cannot rescue it", () => {
    const notGit = join(root, "not-a-repo", "deep");
    mkdirSync(notGit, { recursive: true });

    delete process.env["ICN_ROOT"];
    expect(discoverWorktree(notGit)).toBeNull();

    // THE DEFECT: on icn-dev the shell profile pins ICN_ROOT to the mcp-host worktree, so a
    // session whose cwd was not a worktree got filed under mcp-host's REAL lane — phantom
    // occupancy on a lane nobody was in. Whether that happened was decided purely by whether
    // an environment variable happened to be set. Callers pass only an explicit --root now.
    process.env["ICN_ROOT"] = wt;
    expect(discoverWorktree(notGit)).toBeNull();
  });

  it("an EXPLICIT root is still honoured — operator intent, not ambient state", () => {
    const notGit = join(root, "not-a-repo-2");
    mkdirSync(notGit, { recursive: true });
    expect(discoverWorktree(notGit, wt)!.worktree_id).toBe(discoverWorktree(wt)!.worktree_id);
  });

  it("cwd WINS over an explicit root when cwd is itself a worktree", () => {
    const other = join(root, "lane-other");
    git(repo, "worktree", "add", "-q", "-b", "lane-other", other);
    // A valid answer to the wrong question is still wrong: the session is in `wt`.
    expect(discoverWorktree(wt, other)!.worktree_id).toBe(discoverWorktree(wt)!.worktree_id);
  });
});
