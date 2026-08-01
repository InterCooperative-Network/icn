import { describe, it, expect } from "vitest";
import {
  classifyTreeFreshness,
  MAIN_STALE_ERROR_COMMITS,
  FEATURE_BASE_STALE_WARN_COMMITS,
} from "../diagnostics/doctor.js";

describe("classifyTreeFreshness", () => {
  it("is ok when level with origin/main", () => {
    expect(classifyTreeFreshness("main", 0).severity).toBe("ok");
    expect(classifyTreeFreshness("feat/x", 0).severity).toBe("ok");
  });

  it("treats an unresolvable origin/main as skipped, not healthy-by-default", () => {
    const r = classifyTreeFreshness("main", Number.NaN);
    expect(r.severity).toBe("ok");
    expect(r.message).toMatch(/skipped/);
  });

  describe("a checkout claiming to be main", () => {
    it("warns as soon as it is behind at all", () => {
      const r = classifyTreeFreshness("main", 1);
      expect(r.severity).toBe("warn");
      expect(r.repair).toMatch(/ff-only/);
    });

    it("escalates to error past the staleness threshold", () => {
      expect(classifyTreeFreshness("main", MAIN_STALE_ERROR_COMMITS).severity).toBe("error");
      expect(classifyTreeFreshness("main", MAIN_STALE_ERROR_COMMITS - 1).severity).toBe("warn");
    });

    it("errors on the real regression this check exists for", () => {
      // The icn-ops MCP served ops/state + canonical docs from a `main`
      // checkout 75 commits behind origin/main while icn_ops_doctor reported
      // severity "ok" — repo_status already knew the number and nothing
      // consulted it. Any tree this far behind must not read as healthy.
      const r = classifyTreeFreshness("main", 75);
      expect(r.severity).toBe("error");
      expect(r.message).toContain("75");
      expect(r.message).toMatch(/stale/);
    });
  });

  describe("a feature branch", () => {
    it("does not complain about the ordinary lag of active work", () => {
      expect(classifyTreeFreshness("feat/x", 5).severity).toBe("ok");
      expect(
        classifyTreeFreshness("feat/x", FEATURE_BASE_STALE_WARN_COMMITS - 1).severity
      ).toBe("ok");
    });

    it("warns once the branch base is genuinely overdue for a rebase", () => {
      const r = classifyTreeFreshness("feat/x", FEATURE_BASE_STALE_WARN_COMMITS);
      expect(r.severity).toBe("warn");
      expect(r.repair).toMatch(/rebase/);
    });

    it("holds a detached checkout to the feature-branch rule, not the main rule", () => {
      // env.git.branch is "HEAD" (or null) on a detached checkout; it is not
      // claiming to be main, so it must not trip the main-staleness error.
      expect(classifyTreeFreshness("HEAD", 30).severity).toBe("ok");
      expect(classifyTreeFreshness(null, 30).severity).toBe("ok");
    });
  });
});
