import { afterAll, describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { buildDoctorReport, classifyTreeCleanliness } from "../diagnostics/doctor.js";

// The input space here is genuinely small — four kinds of branch by three states of the
// porcelain probe — so it is enumerated EXHAUSTIVELY rather than sampled. A table of
// hand-picked cases would leave the reader wondering which combination was omitted; a full
// cross-product cannot.

const BRANCHES: { label: string; value: string | null }[] = [
  { label: "main", value: "main" },
  { label: "feature branch", value: "claude/some-work" },
  { label: "detached (HEAD)", value: "HEAD" },
  { label: "detached (null)", value: null },
];

const PORCELAIN: { label: string; value: number | null }[] = [
  { label: "unprobed (null)", value: null },
  { label: "clean (0)", value: 0 },
  { label: "dirty (3)", value: 3 },
];

/** The full expected verdict for every point in the space. */
function expected(branch: string | null, lines: number | null): "ok" | "warn" | "error" {
  if (lines === null) return "warn"; // unknown is never fine
  if (lines === 0) return "ok";
  return branch === "main" ? "error" : "warn";
}

describe("classifyTreeCleanliness — exhaustive over the whole input space", () => {
  const cases = BRANCHES.flatMap((b) => PORCELAIN.map((p) => ({ b, p })));

  it("enumerates every combination (guards against the table silently shrinking)", () => {
    expect(cases).toHaveLength(BRANCHES.length * PORCELAIN.length);
    expect(cases).toHaveLength(12);
  });

  it.each(cases)("$b.label + $p.label", ({ b, p }) => {
    const got = classifyTreeCleanliness(b.value, p.value);
    expect(got.severity).toBe(expected(b.value, p.value));
  });

  // The invariant, asserted over the space rather than at a point: the ONLY way to be reported
  // healthy is to have actually been measured clean. Everything else is warn or error.
  it("never reports ok unless cleanliness was confirmed", () => {
    for (const { b, p } of cases) {
      const got = classifyTreeCleanliness(b.value, p.value);
      if (got.severity === "ok") {
        expect(p.value, `ok returned for porcelain=${p.label} on ${b.label}`).toBe(0);
      }
    }
  });
});

describe("classifyTreeCleanliness — the specific regressions it exists to prevent", () => {
  it("reports an unrunnable probe as UNVERIFIED, not as a skipped check that passes", () => {
    const got = classifyTreeCleanliness("main", null);
    // The previous implementation returned severity "ok" with the message
    // "dirty-tree check skipped." Because the report folds only severity, that rendered the
    // whole environment healthy on the strength of a check that never ran.
    expect(got.severity).not.toBe("ok");
    expect(got.message).toMatch(/UNVERIFIED/);
  });

  it("fails a main checkout that is serving uncommitted content", () => {
    const got = classifyTreeCleanliness("main", 43);
    // 43 is the real number observed on this host's MCP-serving checkout.
    expect(got.severity).toBe("error");
    expect(got.message).toMatch(/do not match any commit/);
    expect(got.detail).toMatch(/unverified/i);
  });

  it("does not punish a feature lane for having work in it", () => {
    const got = classifyTreeCleanliness("claude/some-work", 43);
    expect(got.severity).toBe("warn");
    expect(got.message).toMatch(/expected during active work/);
  });

  it("describes a detached checkout as detached rather than as a feature branch", () => {
    for (const b of [null, "HEAD"]) {
      const got = classifyTreeCleanliness(b, 2);
      expect(got.message).toMatch(/^Detached checkout/);
    }
  });

  it("offers a repair only when there is something to repair", () => {
    expect(classifyTreeCleanliness("main", 0).repair).toBeUndefined();
    expect(classifyTreeCleanliness("main", null).repair).toBeTruthy();
    expect(classifyTreeCleanliness("main", 5).repair).toBeTruthy();
  });
});

// Wiring. The tests above prove the classifier; they say nothing about whether the report
// actually asks it. A correct helper that nothing calls would pass every one of them, so the
// severity is asserted end-to-end through buildDoctorReport.
describe("buildDoctorReport — the classifier is actually wired into the verdict", () => {
  const temps: string[] = [];
  function repo(branch: string, dirty: boolean): string {
    const d = mkdtempSync(path.join(tmpdir(), "icn-doctor-"));
    temps.push(d);
    const git = (...a: string[]): void => {
      execFileSync("git", [
        "-c", "user.email=t@e.invalid", "-c", "user.name=T",
        "-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false",
        ...a,
      ], { cwd: d, stdio: "ignore" });
    };
    git("init", ".");
    writeFileSync(path.join(d, "README.md"), "seed\n");
    git("add", "README.md");
    git("commit", "-m", "seed");
    if (branch !== "main") git("checkout", "-b", branch);
    if (dirty) writeFileSync(path.join(d, "README.md"), "uncommitted edit\n");
    return d;
  }
  afterAll(() => {
    for (const d of temps) rmSync(d, { recursive: true, force: true });
  });

  // NOTE ON WHAT IS ASSERTED HERE. The top-level `report.severity` is NOT used as evidence: a
  // synthetic temp repo legitimately errors on node_modules, better-sqlite3 and the portability
  // script, so it reads "error" whatever the dirty check says. Asserting it would pass for the
  // wrong reason. The discriminating fact is that the dirty_tree CHECK exists and its severity
  // moves with the branch — which is only possible if the classifier is actually being called
  // with it.
  it("emits a dirty_tree check whose severity tracks the branch", async () => {
    const onMain = await buildDoctorReport(repo("main", true));
    const onLane = await buildDoctorReport(repo("claude/work", true));

    const mainCheck = onMain.checks.find((c) => c.id === "dirty_tree");
    const laneCheck = onLane.checks.find((c) => c.id === "dirty_tree");

    expect(mainCheck, "dirty_tree check must be present").toBeDefined();
    expect(laneCheck, "dirty_tree check must be present").toBeDefined();
    expect(mainCheck?.severity).toBe("error");
    expect(laneCheck?.severity).toBe("warn");
    // Identical trees apart from the branch: the difference can only come from the classifier.
    expect(mainCheck?.severity).not.toBe(laneCheck?.severity);
  });

  it("states the uncommitted path count so the verdict is actionable", async () => {
    const report = await buildDoctorReport(repo("main", true));
    const check = report.checks.find((c) => c.id === "dirty_tree");
    expect(check?.message).toMatch(/1 uncommitted path\(s\)/);
  });

  it("reports a clean main checkout as ok", async () => {
    const report = await buildDoctorReport(repo("main", false));
    const check = report.checks.find((c) => c.id === "dirty_tree");
    expect(check?.severity).toBe("ok");
  });
});

// The classifier can only be as trustworthy as the measurement it is handed. buildEnvironmentReport
// runs a PLAIN `git status --porcelain`, which honours the inspected repository's own settings —
// so a repo can hide its dirt from it. These assert the doctor is fed the hardened probe instead.
describe("buildDoctorReport — a repository cannot hide its dirt from the verdict", () => {
  const temps: string[] = [];
  function repo(configure: (git: (...a: string[]) => void, dir: string) => void): string {
    const d = mkdtempSync(path.join(tmpdir(), "icn-doctor-adv-"));
    temps.push(d);
    const git = (...a: string[]): void => {
      execFileSync("git", [
        "-c", "user.email=t@e.invalid", "-c", "user.name=T",
        "-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false", ...a,
      ], { cwd: d, stdio: "ignore" });
    };
    git("init", ".");
    writeFileSync(path.join(d, "README.md"), "seed\n");
    git("add", "README.md");
    git("commit", "-m", "seed");
    configure(git, d);
    return d;
  }
  afterAll(() => {
    for (const d of temps) rmSync(d, { recursive: true, force: true });
  });

  it("sees untracked content a repo configured to hide it", async () => {
    const d = repo((git, dir) => {
      git("config", "status.showUntrackedFiles", "no");
      writeFileSync(path.join(dir, "smuggled.json"), "{}\n");
    });

    // Control: the plain probe the doctor USED to depend on reports nothing.
    const plain = execFileSync("git", ["status", "--porcelain"], { cwd: d, encoding: "utf-8" }).trim();
    expect(plain, "control: the config must actually hide it from a plain probe").toBe("");

    const report = await buildDoctorReport(d);
    const check = report.checks.find((c) => c.id === "dirty_tree");
    // On main with hidden-but-real dirt, the verdict must not be "clean".
    expect(check?.severity).toBe("error");
  });

  it("sees modifications a redirected core.worktree would conceal", async () => {
    const decoy = mkdtempSync(path.join(tmpdir(), "icn-doctor-decoy-"));
    temps.push(decoy);
    writeFileSync(path.join(decoy, "README.md"), "seed\n");
    const d = repo((git, dir) => {
      writeFileSync(path.join(dir, "README.md"), "locally modified\n");
      git("config", "core.worktree", decoy);
    });

    const plain = execFileSync("git", ["status", "--porcelain"], { cwd: d, encoding: "utf-8" }).trim();
    expect(plain, "control: core.worktree must actually redirect the plain probe").toBe("");

    const report = await buildDoctorReport(d);
    const check = report.checks.find((c) => c.id === "dirty_tree");
    expect(check?.severity).toBe("error");
  });
});
