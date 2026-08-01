import { describe, it, expect } from "vitest";
import { resolveBranch, readWorktreeDirs } from "../tools/repos.js";

describe("resolveBranch", () => {
  it("falls back to main when HEAD is detached", () => {
    // `git rev-parse --abbrev-ref HEAD` prints "HEAD" on a detached checkout
    expect(resolveBranch("HEAD")).toBe("main");
  });

  it("falls back to main when branch lookup yields nothing", () => {
    // gitLine returns "" when the git call fails
    expect(resolveBranch("")).toBe("main");
    expect(resolveBranch("   ")).toBe("main");
  });

  it("preserves a real branch name", () => {
    expect(resolveBranch("feat/foo")).toBe("feat/foo");
    expect(resolveBranch("main")).toBe("main");
  });

  it("trims surrounding whitespace", () => {
    expect(resolveBranch("  release/1.2 \n")).toBe("release/1.2");
  });
});

describe("readWorktreeDirs", () => {
  it("lists worktrees and hides dotfiles", () => {
    const r = readWorktreeDirs("/wt", () => ["alpha", ".git", "beta", ".DS_Store"]);
    expect(r.dirs).toEqual(["alpha", "beta"]);
    expect(r.error).toBeNull();
  });

  it("distinguishes a genuinely empty root from an unreadable one", () => {
    const empty = readWorktreeDirs("/wt", () => []);
    expect(empty.dirs).toEqual([]);
    expect(empty.error).toBeNull();
  });

  it("reports an unreadable worktree root instead of returning an empty list", () => {
    // Regression: a misconfigured/absent root used to be swallowed by `catch {
    // dirs = [] }`, which is indistinguishable from "no worktrees exist" and
    // reads as success. That is how a wrong root stayed invisible.
    const r = readWorktreeDirs("/nope", () => {
      throw new Error("ENOENT: no such file or directory");
    });
    expect(r.dirs).toEqual([]);
    expect(r.error).toContain("/nope");
    expect(r.error).toContain("ICN_WT_ROOT");
  });
});
