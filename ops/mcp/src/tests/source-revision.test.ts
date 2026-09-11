import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import {
  describeSourceRevision,
  wasTruncated,
  worktreeFingerprint,
} from "../diagnostics/source-revision.js";
import { runCommand } from "../utils/commands.js";
import { registerAgentOpsTools } from "../tools/agent-ops.js";

// These tests build real git repositories in a temp directory rather than mocking git.
// The behaviour under test IS the reading of git state, so a mock would assert that the
// test's own idea of a dirty tree matches itself.

const GIT_ENV = [
  "-c", "user.email=test@example.invalid",
  "-c", "user.name=ICN Test",
  "-c", "commit.gpgsign=false",
  "-c", "init.defaultBranch=main",
];

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", [...GIT_ENV, ...args], {
    cwd,
    encoding: "utf-8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

const temps: string[] = [];
function tempDir(label: string): string {
  const d = mkdtempSync(path.join(tmpdir(), `icn-srcrev-${label}-`));
  temps.push(d);
  return d;
}

/** A bare origin with one commit, plus a clone that tracks it and is level with it. */
function originWithClone(): { origin: string; clone: string } {
  const origin = tempDir("origin");
  git(origin, "init", "--bare", ".");
  const seed = tempDir("seed");
  git(seed, "clone", origin, ".");
  writeFileSync(path.join(seed, "README.md"), "seed\n");
  git(seed, "add", "README.md");
  git(seed, "commit", "-m", "seed");
  git(seed, "push", "-u", "origin", "main");
  const clone = tempDir("clone");
  git(clone, "clone", origin, ".");
  // A clone does not necessarily leave FETCH_HEAD behind, and currency is only claimable with
  // evidence that the tracking refs were actually refreshed. This mirrors a synced host.
  git(clone, "fetch", "origin");
  return { origin, clone };
}

afterAll(() => {
  for (const d of temps) rmSync(d, { recursive: true, force: true });
});

describe("describeSourceRevision — a clean, current source", () => {
  it("reports its revision and is trustworthy with no warnings", async () => {
    const { clone } = originWithClone();
    const s = await describeSourceRevision(clone);

    expect(s.source_revision).toMatch(/^[0-9a-f]{40}$/);
    expect(s.source_checkout).toBe(clone);
    expect(s.source_ref).toBe("main");
    expect(s.upstream).toBe("origin/main");
    expect(s.dirty).toBe(false);
    expect(s.dirty_paths).toBe(0);
    expect(s.behind_upstream).toBe(0);
    expect(s.trustworthy).toBe(true);
    expect(s.warnings).toEqual([]);
  });

  it("reports the same revision git itself reports", async () => {
    const { clone } = originWithClone();
    const s = await describeSourceRevision(clone);
    expect(s.source_revision).toBe(git(clone, "rev-parse", "HEAD"));
  });
});

// The other direction: each way a source can be untrustworthy must be detected.
// A stamp that only ever says "fine" would be worse than no stamp, because it would
// lend false confidence to exactly the stale answers it exists to expose.
describe("describeSourceRevision — an untrustworthy source is detected", () => {
  it("flags a tree with uncommitted changes", async () => {
    const { clone } = originWithClone();
    writeFileSync(path.join(clone, "README.md"), "locally edited\n");
    writeFileSync(path.join(clone, "untracked.txt"), "new\n");

    const s = await describeSourceRevision(clone);
    expect(s.dirty).toBe(true);
    expect(s.dirty_paths).toBe(2);
    expect(s.trustworthy).toBe(false);
    expect(s.warnings.join(" ")).toMatch(/uncommitted/);
  });

  it("flags a clean tree holding commits its upstream does not have", async () => {
    const { clone } = originWithClone();
    writeFileSync(path.join(clone, "unpushed.md"), "local only\n");
    git(clone, "add", "unpushed.md");
    git(clone, "commit", "-m", "unpushed");

    const s = await describeSourceRevision(clone);
    // The working tree is spotless and nothing is behind — the one-directional check that
    // `HEAD..upstream` performs would call this level and certify it.
    expect(s.dirty).toBe(false);
    expect(s.behind_upstream).toBe(0);
    // But it holds a commit that exists nowhere else, so it is not level.
    expect(s.ahead_of_upstream).toBe(1);
    expect(s.trustworthy).toBe(false);
    expect(s.warnings.join(" ")).toMatch(/exists nowhere else/);
  });

  it("flags a tree that is behind its upstream", async () => {
    const { origin, clone } = originWithClone();

    const pusher = tempDir("pusher");
    git(pusher, "clone", origin, ".");
    writeFileSync(path.join(pusher, "second.md"), "second\n");
    git(pusher, "add", "second.md");
    git(pusher, "commit", "-m", "second");
    git(pusher, "push", "origin", "main");

    git(clone, "fetch", "origin");

    const s = await describeSourceRevision(clone);
    expect(s.behind_upstream).toBe(1);
    expect(s.dirty).toBe(false);
    expect(s.trustworthy).toBe(false);
    expect(s.warnings.join(" ")).toMatch(/behind origin\/main/);
  });

  it("flags a tree whose staleness cannot be measured (no upstream)", async () => {
    const solo = tempDir("solo");
    git(solo, "init", ".");
    writeFileSync(path.join(solo, "a.md"), "a\n");
    git(solo, "add", "a.md");
    git(solo, "commit", "-m", "a");

    const s = await describeSourceRevision(solo);
    expect(s.source_revision).toMatch(/^[0-9a-f]{40}$/);
    expect(s.upstream).toBeNull();
    expect(s.behind_upstream).toBeNull();
    // Known revision and a clean tree are not enough: unmeasurable staleness is not "current".
    expect(s.trustworthy).toBe(false);
    expect(s.warnings.join(" ")).toMatch(/no upstream/);
  });

  // A revision can be readable while cleanliness is not: a truncated .git/index is what a
  // crashed process — or a host that ran out of disk — leaves behind. This is the branch the
  // fail-open mutation lives in, so it needs a real fault rather than a mocked one.
  it("refuses to call a tree clean when cleanliness cannot be determined", async () => {
    const { clone } = originWithClone();
    const head = git(clone, "rev-parse", "HEAD");
    writeFileSync(path.join(clone, ".git", "index"), "GARBAGE-NOT-AN-INDEX");

    const s = await describeSourceRevision(clone);
    // The revision is still readable — refs are intact.
    expect(s.source_revision).toBe(head);
    // But cleanliness is unknown, and unknown must not be reported as clean.
    expect(s.dirty).toBeNull();
    expect(s.dirty_paths).toBeNull();
    expect(s.trustworthy).toBe(false);
    expect(s.warnings.join(" ")).toMatch(/could not determine whether the source checkout is clean/);
  });

  it("flags a directory that is not a git tree at all", async () => {
    const plain = tempDir("plain");
    const s = await describeSourceRevision(plain);

    expect(s.source_revision).toBeNull();
    expect(s.source_ref).toBeNull();
    // Unknown cleanliness must never be reported as clean.
    expect(s.dirty).toBeNull();
    expect(s.dirty_paths).toBeNull();
    expect(s.trustworthy).toBe(false);
    expect(s.warnings.join(" ")).toMatch(/not a readable git tree/);
  });
});

describe("describeSourceRevision — how the root was chosen is reported", () => {
  const saved = process.env["ICN_ROOT"];
  afterEach(() => {
    if (saved === undefined) delete process.env["ICN_ROOT"];
    else process.env["ICN_ROOT"] = saved;
  });

  it("reports explicit_root when a root is passed, even if ICN_ROOT is set", async () => {
    const { clone } = originWithClone();
    process.env["ICN_ROOT"] = "/some/other/checkout";
    const s = await describeSourceRevision(clone);
    // icn_ops_agent_runtime deliberately overrides ICN_ROOT with the caller's lane; saying
    // "ICN_ROOT" here would contradict the path actually reported in source_checkout.
    expect(s.resolved_from).toBe("explicit_root");
    expect(s.source_checkout).toBe(clone);
  });

  it("reports ICN_ROOT when the environment pins the root and none is passed", async () => {
    const { clone } = originWithClone();
    process.env["ICN_ROOT"] = clone;
    const s = await describeSourceRevision();
    expect(s.resolved_from).toBe("ICN_ROOT");
    expect(s.source_checkout).toBe(clone);
  });

  it("reports server_location when nothing pins the root", async () => {
    delete process.env["ICN_ROOT"];
    const s = await describeSourceRevision();
    expect(s.resolved_from).toBe("server_location");
  });
});

// `@{u}` is a LOCAL ref. A host that never fetches sits at "0 behind" forever while the remote
// moves away from it — stale and confident, which is the exact state this module exists to
// expose. Currency therefore requires evidence that the tracking refs were actually refreshed.
describe("describeSourceRevision — a stale tracking ref cannot certify currency", () => {
  it("warns that distance is a lower bound when the last fetch is old", async () => {
    const { clone } = originWithClone();
    const fetchHead = path.join(clone, ".git", "FETCH_HEAD");
    const longAgo = new Date(Date.now() - 72 * 3600 * 1000);
    utimesSync(fetchHead, longAgo, longAgo);

    const s = await describeSourceRevision(clone);
    // Locally everything is faithful: clean, and exactly level with the tracking ref.
    expect(s.dirty).toBe(false);
    expect(s.behind_upstream).toBe(0);
    expect(s.ahead_of_upstream).toBe(0);
    // So `trustworthy` — a claim about local faithfulness — stays true...
    expect(s.trustworthy).toBe(true);
    // ...while the staleness risk is carried explicitly, never silently folded away.
    expect(s.remote_currency).toBe("unverified");
    expect(s.warnings.join(" ")).toMatch(/lower bound/);
  });

  it("never claims verified remote currency, because nothing local could prove it", async () => {
    const { clone } = originWithClone();
    const s = await describeSourceRevision(clone);
    expect(s.upstream_observed_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    // FETCH_HEAD is repository-wide: `git fetch origin otherbranch` refreshes it without
    // touching this upstream, so it can raise suspicion but must never certify.
    expect(s.remote_currency).toBe("unverified");
  });
});

// GIT_DIR overrides -C, and git exports it into child processes in a linked worktree — the only
// kind ICN uses. If it leaked through, this would name one checkout while describing another:
// the precise misattribution the stamp exists to remove.
describe("describeSourceRevision — the environment cannot redirect the probe", () => {
  const savedDir = process.env["GIT_DIR"];
  const savedWork = process.env["GIT_WORK_TREE"];
  afterEach(() => {
    if (savedDir === undefined) delete process.env["GIT_DIR"];
    else process.env["GIT_DIR"] = savedDir;
    if (savedWork === undefined) delete process.env["GIT_WORK_TREE"];
    else process.env["GIT_WORK_TREE"] = savedWork;
  });

  it("describes the checkout it was given, not the one GIT_DIR points at", async () => {
    const target = originWithClone().clone;
    const decoy = originWithClone().clone;
    // Make the decoy unmistakably different.
    writeFileSync(path.join(decoy, "decoy.md"), "decoy\n");
    git(decoy, "add", "decoy.md");
    git(decoy, "commit", "-m", "decoy commit");

    const targetHead = git(target, "rev-parse", "HEAD");
    const decoyHead = git(decoy, "rev-parse", "HEAD");
    expect(targetHead).not.toBe(decoyHead);

    process.env["GIT_DIR"] = path.join(decoy, ".git");
    process.env["GIT_WORK_TREE"] = decoy;

    const s = await describeSourceRevision(target);
    expect(s.source_checkout).toBe(target);
    expect(s.source_revision).toBe(targetHead);
    expect(s.source_revision).not.toBe(decoyHead);
  });
});

// runCommand truncates oversized output and still reports success. A probe that COUNTS things
// would read a shortened list as "fewer findings" rather than "incomplete answer", so a flagged
// file past the cutoff would simply not be seen. The marker is produced by runCommand here
// rather than hand-written, so the test fails if the two modules ever drift apart on its shape.
describe("wasTruncated — a shortened probe result is not a smaller result", () => {
  it("recognises the marker runCommand actually emits", async () => {
    const r = await runCommand("node", ["-e", "console.log('x'.repeat(5000))"], {
      timeoutMs: 10_000,
      maxStdoutBytes: 100,
    });
    expect(r.ok, "the command itself must succeed — truncation is not a failure there").toBe(true);
    expect(r.stdout.length).toBeLessThan(5000);
    expect(wasTruncated(r.stdout)).toBe(true);
  });

  it("does not flag output that fit within the budget", async () => {
    const r = await runCommand("node", ["-e", "console.log('short')"], {
      timeoutMs: 10_000,
      maxStdoutBytes: 4096,
    });
    expect(r.ok).toBe(true);
    expect(wasTruncated(r.stdout)).toBe(false);
  });
});

// THE SAFETY BRIDGE (R1-A). Inspecting a working tree makes git apply repository-controlled
// behaviour to its contents: `.gitattributes` plus `filter.<driver>.clean` is executed by
// `git status`. That is not a bounded set of knobs, so the bridge does not try to disable them
// one by one — it declines to inspect the working tree of a checkout the caller chose.
// Removing the dependence on caller working trees entirely is R1-B.
describe("the safety bridge — a caller-selected checkout is not inspected", () => {
  /** A repo whose clean filter writes a sentinel file when git runs it. */
  function repoWithSideEffectingCleanFilter(): { repo: string; sentinel: string } {
    const repo = tempDir("evil-filter");
    git(repo, "init", ".");
    writeFileSync(path.join(repo, "payload.txt"), "original\n");
    git(repo, "add", "payload.txt");
    git(repo, "commit", "-m", "init");
    writeFileSync(path.join(repo, ".gitattributes"), "* filter=evil\n");
    git(repo, "add", ".gitattributes");
    git(repo, "commit", "-m", "attrs");

    const sentinel = path.join(repo, "FILTER_EXECUTED");
    git(repo, "config", "filter.evil.clean", `sh -c 'echo ran > "${sentinel}"; cat'`);
    // Modify the file so a cleanliness probe has to convert its contents.
    writeFileSync(path.join(repo, "payload.txt"), "modified\n");
    return { repo, sentinel };
  }

  it("control: git really does execute the clean filter during a status probe", () => {
    const { repo, sentinel } = repoWithSideEffectingCleanFilter();
    execFileSync("git", ["status", "--porcelain", "--untracked-files=normal"], {
      cwd: repo,
      stdio: "ignore",
    });
    // Without this, the assertions below could pass on a git that never ran the filter at all.
    expect(existsSync(sentinel), "control: the filter must be executable via status").toBe(true);
  });

  it("does not execute a repository-defined clean filter for a caller-selected root", async () => {
    const { repo, sentinel } = repoWithSideEffectingCleanFilter();

    const s = await describeSourceRevision(repo, "caller_selected");

    expect(existsSync(sentinel), "no repository-defined code may run for a caller-selected root")
      .toBe(false);
    // Revision identity survives: it came from ref reads, which execute nothing.
    expect(s.source_revision).toBe(git(repo, "rev-parse", "HEAD"));
    expect(s.source_provenance).toBe("caller_selected");
    // Cleanliness is indeterminate — null, not false — and nothing is certified from it.
    expect(s.dirty).toBeNull();
    expect(s.dirty_paths).toBeNull();
    expect(s.index_hidden_paths).toBeNull();
    expect(s.trustworthy).toBe(false);
    expect(s.warnings.join(" ")).toMatch(/cleanliness was not probed/);
  });

  it("still inspects an operator-controlled root, so the gate is what does the work", async () => {
    const { repo, sentinel } = repoWithSideEffectingCleanFilter();
    const s = await describeSourceRevision(repo, "controlled");
    // The contrast matters: if nothing inspected the tree in either mode, the assertion above
    // would hold for the wrong reason. Controlled sources ARE inspected.
    expect(existsSync(sentinel)).toBe(true);
    expect(s.dirty).not.toBeNull();
  });

  // End-to-end through the only tool that resolves its root from client input. The unit test
  // above proves the primitive declines; this proves the tool actually asks it to.
  it("icn_ops_agent_runtime does not execute the filter for a cwd the caller supplied", async () => {
    const { repo, sentinel } = repoWithSideEffectingCleanFilter();
    const savedRoot = process.env["ICN_ROOT"];
    const host = originWithClone().clone;
    process.env["ICN_ROOT"] = host;
    try {
      const server = new McpServer({ name: "icn-ops-test", version: "0.0.0" });
      registerAgentOpsTools(server);
      const [ct, st] = InMemoryTransport.createLinkedPair();
      const client = new Client({ name: "test-client", version: "0.0.0" });
      await Promise.all([server.connect(st), client.connect(ct)]);

      const res = (await client.callTool({
        name: "icn_ops_agent_runtime",
        arguments: { cwd: repo, section: "session" },
      })) as { content: Array<{ type: string; text: string }> };
      const body = JSON.parse(res.content[0]?.text ?? "{}") as Record<string, unknown>;
      const source = body["source"] as Record<string, unknown>;

      expect(existsSync(sentinel), "the tool must not execute caller-repo code").toBe(false);
      expect(source?.["source_provenance"]).toBe("caller_selected");
      expect(source?.["dirty"]).toBeNull();
      expect(source?.["trustworthy"]).toBe(false);
    } finally {
      if (savedRoot === undefined) delete process.env["ICN_ROOT"];
      else process.env["ICN_ROOT"] = savedRoot;
    }
  });

  it("a caller-selected fingerprint covers the commit without reading the tree", async () => {
    const { repo, sentinel } = repoWithSideEffectingCleanFilter();
    const fp = await worktreeFingerprint(repo, "caller_selected");
    expect(existsSync(sentinel)).toBe(false);
    expect(fp).toBe(`${git(repo, "rev-parse", "HEAD")}:worktree-not-inspected`);
  });
});

// Repository config must not be able to decide how thoroughly the repository is inspected.
describe("describeSourceRevision — repository config cannot suppress the cleanliness check", () => {
  it("sees untracked files even when status.showUntrackedFiles=no", async () => {
    const { clone } = originWithClone();
    git(clone, "config", "status.showUntrackedFiles", "no");
    writeFileSync(path.join(clone, "untracked-manifest.json"), "{}\n");

    // Control: with the repository's own setting honoured, git reports nothing.
    const suppressed = execFileSync("git", ["status", "--porcelain"], {
      cwd: clone,
      encoding: "utf-8",
    }).trim();
    expect(suppressed, "control: the config must actually suppress").toBe("");

    const s = await describeSourceRevision(clone);
    expect(s.dirty).toBe(true);
    expect(s.dirty_paths).toBe(1);
    expect(s.trustworthy).toBe(false);
  });

  it.each([["assume-unchanged"], ["skip-worktree"]])(
    "refuses to report clean when a tracked file is hidden by --%s",
    async (flag) => {
      const { clone } = originWithClone();
      git(clone, "update-index", `--${flag}`, "README.md");
      writeFileSync(path.join(clone, "README.md"), "modified but hidden\n");

      // Control: git really does omit it, so status alone says the tree is spotless.
      const hidden = execFileSync(
        "git",
        ["status", "--porcelain", "--untracked-files=normal"],
        { cwd: clone, encoding: "utf-8" }
      ).trim();
      expect(hidden, `control: --${flag} must actually hide it`).toBe("");

      const s = await describeSourceRevision(clone);
      // `dirty` honestly reports what status said...
      expect(s.dirty).toBe(false);
      // ...but status was not the whole answer, and the stamp must not certify on it.
      expect(s.index_hidden_paths).toBe(1);
      expect(s.trustworthy).toBe(false);
      expect(s.warnings.join(" ")).toMatch(/omits them from status/);
    }
  );

  it("sees local modifications even when core.worktree points elsewhere", async () => {
    const { clone } = originWithClone();
    const decoy = tempDir("decoy-worktree");
    writeFileSync(path.join(decoy, "README.md"), "seed\n");
    writeFileSync(path.join(clone, "README.md"), "locally modified\n");
    git(clone, "config", "core.worktree", decoy);

    // Control: honouring the repository's own setting inspects the decoy and sees nothing.
    const redirected = execFileSync("git", ["status", "--porcelain"], {
      cwd: clone,
      encoding: "utf-8",
    }).trim();
    expect(redirected, "control: core.worktree must actually redirect").toBe("");

    const s = await describeSourceRevision(clone);
    expect(s.dirty).toBe(true);
    expect(s.trustworthy).toBe(false);
  });
});

// Guarding a read with HEAD alone misses a tree that was dirty during the read and cleaned
// afterwards: the commit never moves, so two HEAD probes agree while the payload describes
// content that no longer exists.
describe("worktreeFingerprint — working-tree state is part of the guarded state", () => {
  it("changes when the tree is dirtied and again when it is cleaned, with HEAD fixed", async () => {
    const { clone } = originWithClone();
    const head = git(clone, "rev-parse", "HEAD");

    const clean = await worktreeFingerprint(clone);
    writeFileSync(path.join(clone, "README.md"), "edited during the read\n");
    const dirty = await worktreeFingerprint(clone);
    execFileSync("git", ["restore", "README.md"], { cwd: clone, stdio: "ignore" });
    const restored = await worktreeFingerprint(clone);

    expect(git(clone, "rev-parse", "HEAD")).toBe(head); // HEAD never moved
    expect(dirty).not.toBe(clean);
    expect(restored).toBe(clean);
  });

  it("is null for a tree whose state cannot be determined", async () => {
    expect(await worktreeFingerprint(tempDir("nonrepo"))).toBeNull();
  });
});

// `git status` honours core.fsmonitor, which may name an executable. Callers supply the path
// probed by icn_ops_agent_runtime, so an unguarded read-only diagnostic would run a binary of
// the repository's choosing with the server's authority. This asserts the behaviour, not the
// flag: a sentinel file appears if and only if the configured program actually ran.
describe("describeSourceRevision — a read-only probe executes no repository-configured code", () => {
  it("does not run a core.fsmonitor program configured in the probed repository", async () => {
    const { clone } = originWithClone();
    const sentinel = path.join(clone, "FSMONITOR_RAN");
    const hook = path.join(clone, "fsmonitor-hook.sh");
    writeFileSync(hook, `#!/bin/sh\necho ran > "${sentinel}"\nexit 1\n`);
    chmodSync(hook, 0o755);
    git(clone, "config", "core.fsmonitor", hook);

    // Control: with the guard removed, git really does execute it — otherwise this test would
    // pass for the wrong reason on a git that ignores the setting.
    execFileSync("git", ["status", "--porcelain"], { cwd: clone, stdio: "ignore" });
    expect(existsSync(sentinel), "control: git must execute core.fsmonitor unguarded").toBe(true);
    rmSync(sentinel, { force: true });

    await describeSourceRevision(clone);
    expect(existsSync(sentinel), "probe must not execute core.fsmonitor").toBe(false);
  });
});

// The policy under test: repository-DERIVED answers are stamped; statically compiled
// catalogs are not. Both directions are asserted, because "stamp everything" and
// "stamp nothing" would each pass a one-sided test.
describe("icn_ops tools — the stamping policy", () => {
  let client: Client;
  let repo: string;
  const saved = process.env["ICN_ROOT"];

  beforeAll(async () => {
    repo = originWithClone().clone;
    process.env["ICN_ROOT"] = repo;
    const server = new McpServer({ name: "icn-ops-test", version: "0.0.0" });
    registerAgentOpsTools(server);
    const [ct, st] = InMemoryTransport.createLinkedPair();
    client = new Client({ name: "test-client", version: "0.0.0" });
    await Promise.all([server.connect(st), client.connect(ct)]);
  });

  afterAll(() => {
    if (saved === undefined) delete process.env["ICN_ROOT"];
    else process.env["ICN_ROOT"] = saved;
  });

  async function call(name: string): Promise<Record<string, unknown>> {
    const res = (await client.callTool({ name, arguments: {} })) as {
      content: Array<{ type: string; text: string }>;
    };
    return JSON.parse(res.content[0]?.text ?? "{}") as Record<string, unknown>;
  }

  it.each(["icn_ops_repo_map", "icn_ops_state_index"])(
    "%s carries the revision it was produced from",
    async (tool) => {
      const body = await call(tool);
      const source = body["source"] as Record<string, unknown> | undefined;
      expect(source, `${tool} must carry a source stamp`).toBeDefined();
      expect(source?.["source_revision"]).toBe(git(repo, "rev-parse", "HEAD"));
      expect(source?.["source_checkout"]).toBe(repo);
      expect(source?.["trustworthy"]).toBe(true);
      // Through the tool path the root is NOT explicitly supplied, so the stamp must report how
      // it was really chosen. Defaulting the wrapper's root would make this claim explicit_root
      // on every response and leave the other two values unreachable in practice.
      expect(source?.["resolved_from"]).toBe("ICN_ROOT");
      // The payload itself must survive the stamp.
      expect(body["entries"]).toBeDefined();
    }
  );

  it.each(["icn_ops_agent_brief", "icn_ops_command_catalog"])(
    "%s is compiled into the build and is NOT stamped",
    async (tool) => {
      const body = await call(tool);
      expect(body["source"]).toBeUndefined();
    }
  );
});
