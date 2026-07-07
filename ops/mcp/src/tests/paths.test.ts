import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { resolveWorktreeRoot } from "../paths.js";

// resolveWorktreeRoot resolution order: ICN_WT_ROOT env → repo-map.json#worktrees.root
// (relative to the monorepo root) → legacy ../icn-wt sibling fallback.
describe("resolveWorktreeRoot", () => {
  const savedWtRoot = process.env["ICN_WT_ROOT"];
  const savedIcnRoot = process.env["ICN_ROOT"];
  let tempRoot: string;

  beforeEach(() => {
    delete process.env["ICN_WT_ROOT"];
    tempRoot = mkdtempSync(path.join(tmpdir(), "icn-paths-test-"));
    process.env["ICN_ROOT"] = tempRoot;
  });

  afterEach(() => {
    rmSync(tempRoot, { recursive: true, force: true });
    if (savedWtRoot === undefined) delete process.env["ICN_WT_ROOT"];
    else process.env["ICN_WT_ROOT"] = savedWtRoot;
    if (savedIcnRoot === undefined) delete process.env["ICN_ROOT"];
    else process.env["ICN_ROOT"] = savedIcnRoot;
  });

  function writeRepoMap(root: unknown): void {
    const dir = path.join(tempRoot, "ops", "state", "config");
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      path.join(dir, "repo-map.json"),
      JSON.stringify({ worktrees: { root } })
    );
  }

  it("prefers the ICN_WT_ROOT env override over everything", () => {
    writeRepoMap("../configured");
    process.env["ICN_WT_ROOT"] = "/custom/wt-root";
    expect(resolveWorktreeRoot()).toBe("/custom/wt-root");
  });

  it("resolves a relative repo-map root against the monorepo root", () => {
    // The worktree-OS layout keeps sibling worktrees one level up ("..").
    writeRepoMap("..");
    expect(resolveWorktreeRoot()).toBe(path.resolve(tempRoot, ".."));
  });

  it("expands a leading ~ in the configured root against HOME", () => {
    writeRepoMap("~/wt");
    const home = process.env["HOME"];
    expect(home).toBeTruthy();
    expect(resolveWorktreeRoot()).toBe(path.join(home as string, "wt"));
  });

  it("falls back to the legacy ../icn-wt sibling when repo-map is missing", () => {
    // No repo-map.json written — pre-worktree-OS checkouts degrade gracefully.
    expect(resolveWorktreeRoot()).toBe(path.resolve(tempRoot, "..", "icn-wt"));
  });

  it("falls back to the legacy sibling when the configured root is not a string", () => {
    writeRepoMap(42);
    expect(resolveWorktreeRoot()).toBe(path.resolve(tempRoot, "..", "icn-wt"));
  });
});
