// What a consumer binding actually depends on, pinned at the producer.
//
//   1. PROVIDER. Production hooks register `provider = "claude-code"`. The registry stores whatever
//      the caller supplied (it is advisory launch metadata, unvalidated, immutable after INSERT),
//      so the stable value is a convention between the hook and the CLI default. A consumer that
//      maps `claude-code` -> its own runtime name needs that convention not to drift silently.
//   2. RAW LANE IDENTITY. The stored `worktree_id` is the realpath of
//      `git rev-parse --absolute-git-dir` (a per-worktree admin directory), and `repo_id` is the
//      realpath of the common dir. The observation profile publishes only `sha256:` + hex of the
//      UTF-8 bytes of those raw values (unsalted), so a consumer can digest a path it already knows
//      and compare, but can never read a path out of the file.
//
// Real Git repositories in a temp directory; a synthetic registry. Nothing here opens the real
// shared registry or any live ICN worktree.

import { execFileSync } from "child_process";
import { createHash } from "crypto";
import { mkdirSync, readFileSync, realpathSync, symlinkSync, writeFileSync } from "fs";
import { dirname, join, resolve } from "path";
import { fileURLToPath } from "url";
import Database from "better-sqlite3";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { discoverWorktree } from "../runtime/worktree-identity.js";
import { registerSession } from "../runtime/session-runtime.js";
import { initDb } from "../state/db.js";
import { exportRegistrySnapshot } from "../state/snapshot-export.js";
import { makeRegistry, removeDir, tempDir } from "./fixtures/snapshot-helpers.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(HERE, "..", "..", "..", "..");

const sha256 = (s: string) => `sha256:${createHash("sha256").update(s, "utf8").digest("hex")}`;

function git(dir: string, ...args: string[]): string {
  return execFileSync("git", ["-C", dir, ...args], {
    encoding: "utf-8",
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      // Ambient git config (commit signing, hooks, templates) must not decide whether this test runs.
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_AUTHOR_NAME: "t",
      GIT_AUTHOR_EMAIL: "t@e",
      GIT_COMMITTER_NAME: "t",
      GIT_COMMITTER_EMAIL: "t@e",
    },
  }).trim();
}

describe("provider: the hook and the CLI default agree on the production value", () => {
  it("the session-lifecycle hook registers --provider claude-code", () => {
    const hook = readFileSync(join(REPO_ROOT, ".claude", "hooks", "session-lifecycle.sh"), "utf8");
    expect(hook).toMatch(/register\b[^\n]*\\\n[^\n]*--provider claude-code\b/);
  });

  it("the icn-agent-session CLI defaults provider to claude-code", () => {
    const cli = readFileSync(join(HERE, "..", "cli", "session.ts"), "utf8");
    expect(cli).toContain('provider: str(args, "provider") ?? "claude-code"');
  });
});

describe("lane identity: raw in the registry, digest in the observation file", () => {
  let root: string;
  let out: string;
  let lane: string;
  let alias: string;

  beforeAll(() => {
    root = tempDir("icn-prodid-");
    out = tempDir("icn-prodid-out-");
    const repo = join(root, "repo");
    mkdirSync(repo, { recursive: true });
    git(repo, "init", "-q", "-b", "main", ".");
    writeFileSync(join(repo, "f.txt"), "one\n");
    git(repo, "add", "f.txt");
    git(repo, "commit", "-qm", "init");
    lane = join(root, "wt", "lane-one");
    git(repo, "worktree", "add", "-q", "-b", "feat/one", lane);
    alias = join(root, "alias-to-lane");
    symlinkSync(lane, alias);
  });
  afterAll(() => {
    removeDir(root);
    removeDir(out);
  });

  it("stores realpath(git --absolute-git-dir) and realpath(--git-common-dir), reached through a symlink", () => {
    const id = discoverWorktree(alias);
    expect(id).not.toBeNull();
    const adminDir = realpathSync(git(alias, "rev-parse", "--absolute-git-dir"));
    const commonDir = realpathSync(resolve(alias, git(alias, "rev-parse", "--git-common-dir")));
    expect(id!.worktree_id).toBe(adminDir);
    expect(id!.repo_id).toBe(commonDir);
    // A linked worktree's admin dir is `<common>/worktrees/<name>`: per-worktree, not the checkout path.
    expect(adminDir).toContain(join("worktrees", "lane-one"));
    expect(id!.worktree_id).not.toBe(realpathSync(lane));
  });

  it("the registry keeps the raw id; the observation file carries only sha256:<hex of those exact bytes>", () => {
    const id = discoverWorktree(alias)!;
    const src = makeRegistry(root, "registry.db");
    const db = initDb(src);
    registerSession(db, {
      repo: id.repo_name,
      identity: id,
      provider: "claude-code",
      provider_session_id: "5d1c2a4e-0000-4000-8000-000000000001",
    });
    const raw = db.prepare("SELECT worktree_id, repo_id, provider FROM sessions").get() as {
      worktree_id: string;
      repo_id: string;
      provider: string;
    };
    db.close();
    expect(raw).toEqual({ worktree_id: id.worktree_id, repo_id: id.repo_id, provider: "claude-code" });

    const dest = join(out, "obs.db");
    exportRegistrySnapshot({ profile: "observation", source: src, dest });

    const o = new Database(dest, { readonly: true });
    const row = o.prepare("SELECT worktree_id, repo_id, provider FROM sessions").get() as typeof raw;
    o.close();

    expect(row.provider).toBe("claude-code"); // copied verbatim, not mapped
    expect(row.worktree_id).toBe(sha256(raw.worktree_id));
    expect(row.repo_id).toBe(sha256(raw.repo_id));
    expect(row.worktree_id).not.toBe(raw.worktree_id);

    for (const f of [dest, `${dest}.meta.json`]) {
      const bytes = readFileSync(f);
      expect(bytes.includes(Buffer.from(raw.worktree_id, "utf8"))).toBe(false);
      expect(bytes.includes(Buffer.from(raw.repo_id, "utf8"))).toBe(false);
      expect(bytes.includes(Buffer.from(lane, "utf8"))).toBe(false);
    }
  });
});
