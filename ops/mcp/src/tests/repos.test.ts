import { describe, it, expect } from "vitest";
import { resolveBranch } from "../tools/repos.js";

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
