import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { describeSourceRevision } from "../diagnostics/source-revision.js";
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

  it("reports ICN_ROOT when the environment pins the root", async () => {
    const { clone } = originWithClone();
    process.env["ICN_ROOT"] = clone;
    const s = await describeSourceRevision(clone);
    expect(s.resolved_from).toBe("ICN_ROOT");
  });

  it("reports server_location when nothing pins the root", async () => {
    const { clone } = originWithClone();
    delete process.env["ICN_ROOT"];
    const s = await describeSourceRevision(clone);
    expect(s.resolved_from).toBe("server_location");
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
