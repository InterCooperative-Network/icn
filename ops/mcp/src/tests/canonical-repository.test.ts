import { afterAll, afterEach, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { registerAgentOpsTools } from "../tools/agent-ops.js";
import {
  readFileAtRevision,
  resolveCanonicalRepository,
  resolveRevision,
  type CanonicalRepository,
  type Revision,
} from "../canonical/repository.js";

// These tests assert ARCHITECTURAL NON-DEPENDENCE: that a canonical, revision-addressed answer
// cannot be influenced by a workspace at all. They are written against real git repositories
// with genuinely hostile configuration, because the claim is about what git does, and a mock
// would only assert that the test agrees with itself.

const GIT = [
  "-c", "user.email=test@example.invalid",
  "-c", "user.name=ICN Test",
  "-c", "commit.gpgsign=false",
  "-c", "init.defaultBranch=main",
];

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", [...GIT, ...args], {
    cwd,
    encoding: "utf-8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

const temps: string[] = [];
function tempDir(label: string): string {
  const d = mkdtempSync(path.join(tmpdir(), `icn-canon-${label}-`));
  temps.push(d);
  return d;
}
afterAll(() => {
  for (const d of temps) rmSync(d, { recursive: true, force: true });
});

const ARTIFACT = "docs/reference/project-index/generated/agent-context-spine.json";
const CANONICAL_BODY = '{"schema":"canonical","nodes":[],"edges":[]}';

type Fixture = {
  icnRoot: string;
  bare: string;
  work: string;
  sentinel: string;
};

/**
 * A canonical bare store, plus a hostile workspace pointing at the same history.
 *
 * The workspace carries a `.gitattributes` filter whose driver writes a sentinel and rewrites
 * content — the mechanism that defeated working-tree inspection in the previous slice.
 */
function fixture(label: string): Fixture {
  const work = tempDir(`${label}-work`);
  git(work, "init", ".");
  mkdirSync(path.dirname(path.join(work, ARTIFACT)), { recursive: true });
  writeFileSync(path.join(work, ARTIFACT), CANONICAL_BODY);
  writeFileSync(path.join(work, ".gitattributes"), "* filter=hostile\n");
  git(work, "add", ".");
  git(work, "commit", "-m", "canonical content");

  const sentinel = path.join(tempDir(`${label}-sentinel`), "FILTER_RAN");
  git(work, "config", "filter.hostile.clean", `sh -c 'echo ran >> "${sentinel}"; cat'`);
  git(work, "config", "filter.hostile.smudge", `sh -c 'echo ran >> "${sentinel}"; cat'`);

  const bare = tempDir(`${label}-canonical`);
  git(bare, "clone", "--bare", work, ".");

  const icnRoot = tempDir(`${label}-icnroot`);
  const cfgDir = path.join(icnRoot, "ops", "state", "config");
  mkdirSync(cfgDir, { recursive: true });
  writeFileSync(
    path.join(cfgDir, "repo-map.json"),
    JSON.stringify({ repos: { icn: { local: ".", local_store: bare } } })
  );
  return { icnRoot, bare, work, sentinel };
}

const savedRoot = process.env["ICN_ROOT"];
afterEach(() => {
  if (savedRoot === undefined) delete process.env["ICN_ROOT"];
  else process.env["ICN_ROOT"] = savedRoot;
});

async function canonical(f: Fixture): Promise<{ repo: CanonicalRepository; rev: Revision }> {
  process.env["ICN_ROOT"] = f.icnRoot;
  const repo = await resolveCanonicalRepository();
  if (!repo.ok) throw new Error(`fixture repo unresolved: ${repo.error.message}`);
  const rev = await resolveRevision(repo.value, "refs/heads/main");
  if (!rev.ok) throw new Error(`fixture revision unresolved: ${rev.error.message}`);
  return { repo: repo.value, rev: rev.value };
}

describe("canonical storage — the store itself must have no working tree", () => {
  it("resolves a bare store declared in operator config", async () => {
    const f = fixture("resolve");
    process.env["ICN_ROOT"] = f.icnRoot;
    const repo = await resolveCanonicalRepository();
    expect(repo.ok).toBe(true);
  });

  it("refuses a git directory that has a working tree attached", async () => {
    // A non-bare store would reintroduce a working tree, and with it everything that can be done
    // to one. The refusal is the invariant, not a tidiness check.
    const f = fixture("nonbare");
    writeFileSync(
      path.join(f.icnRoot, "ops", "state", "config", "repo-map.json"),
      JSON.stringify({ repos: { icn: { local_store: path.join(f.work, ".git") } } })
    );
    process.env["ICN_ROOT"] = f.icnRoot;
    const repo = await resolveCanonicalRepository();
    expect(repo.ok).toBe(false);
    if (!repo.ok) expect(repo.error.code).toBe("not_bare");
  });

  it("refuses a path that is not a git directory at all", async () => {
    // Pointing at a worktree ROOT rather than its git dir lands here. Both rejections matter;
    // neither degrades into reading the tree.
    const f = fixture("notgitdir");
    writeFileSync(
      path.join(f.icnRoot, "ops", "state", "config", "repo-map.json"),
      JSON.stringify({ repos: { icn: { local_store: f.work } } })
    );
    process.env["ICN_ROOT"] = f.icnRoot;
    const repo = await resolveCanonicalRepository();
    expect(repo.ok).toBe(false);
    if (!repo.ok) expect(repo.error.code).toBe("not_a_git_dir");
  });

  it("refuses when no canonical store is declared", async () => {
    const f = fixture("undeclared");
    writeFileSync(
      path.join(f.icnRoot, "ops", "state", "config", "repo-map.json"),
      JSON.stringify({ repos: { icn: { local: "." } } })
    );
    process.env["ICN_ROOT"] = f.icnRoot;
    const repo = await resolveCanonicalRepository();
    expect(repo.ok).toBe(false);
    if (!repo.ok) expect(repo.error.code).toBe("not_configured");
  });
});

describe("canonical reads — a workspace cannot influence the answer", () => {
  it("is unchanged when the workspace's file is rewritten", async () => {
    const f = fixture("edit");
    const { repo, rev } = await canonical(f);
    const before = await readFileAtRevision(repo, rev, ARTIFACT);

    writeFileSync(path.join(f.work, ARTIFACT), '{"schema":"TAMPERED","nodes":[],"edges":[]}');

    const after = await readFileAtRevision(repo, rev, ARTIFACT);
    expect(before.ok && after.ok).toBe(true);
    if (before.ok && after.ok) {
      expect(after.value).toBe(before.value);
      expect(after.value).toContain("canonical");
      expect(after.value).not.toContain("TAMPERED");
    }
  });

  it("is unchanged when the workspace's index is staged with different content", async () => {
    const f = fixture("index");
    const { repo, rev } = await canonical(f);
    writeFileSync(path.join(f.work, ARTIFACT), '{"schema":"STAGED","nodes":[],"edges":[]}');
    git(f.work, "add", ARTIFACT);

    const read = await readFileAtRevision(repo, rev, ARTIFACT);
    expect(read.ok).toBe(true);
    if (read.ok) expect(read.value).not.toContain("STAGED");
  });

  it("is unchanged when the workspace's git config is rewritten", async () => {
    const f = fixture("config");
    const { repo, rev } = await canonical(f);
    git(f.work, "config", "core.worktree", tempDir("decoy"));
    git(f.work, "config", "status.showUntrackedFiles", "no");

    const read = await readFileAtRevision(repo, rev, ARTIFACT);
    expect(read.ok).toBe(true);
    if (read.ok) expect(read.value).toContain("canonical");
  });

  it("invokes no clean or smudge filter", async () => {
    const f = fixture("filters");
    const { repo, rev } = await canonical(f);

    // Control: the filter is real and git will run it, through a working tree.
    rmSync(f.sentinel, { force: true });
    writeFileSync(path.join(f.work, ARTIFACT), CANONICAL_BODY + "\n");
    execFileSync("git", ["status", "--porcelain"], { cwd: f.work, stdio: "ignore" });
    expect(existsSync(f.sentinel), "control: the filter must be reachable via a working tree")
      .toBe(true);

    rmSync(f.sentinel, { force: true });
    const read = await readFileAtRevision(repo, rev, ARTIFACT);
    expect(read.ok).toBe(true);
    expect(existsSync(f.sentinel), "no repository-defined filter may run for a canonical read")
      .toBe(false);
  });

  it("needs no working tree at all — the store has none", async () => {
    const f = fixture("noworktree");
    const { repo, rev } = await canonical(f);
    // Deleting the workspace entirely must not affect a canonical read.
    rmSync(f.work, { recursive: true, force: true });

    const read = await readFileAtRevision(repo, rev, ARTIFACT);
    expect(read.ok).toBe(true);
    if (read.ok) expect(read.value).toContain("canonical");
  });
});

describe("canonical reads — revision identity is fixed once and carried", () => {
  it("keeps answering at the resolved revision after the branch moves", async () => {
    const f = fixture("moving");
    const { repo, rev } = await canonical(f);

    // The branch advances in canonical storage itself — the hardest case, since the store is
    // the thing we trust.
    writeFileSync(path.join(f.work, ARTIFACT), '{"schema":"NEWER","nodes":[],"edges":[]}');
    git(f.work, "add", ARTIFACT);
    git(f.work, "commit", "-m", "newer");
    git(f.work, "push", f.bare, "main");

    const head = await resolveRevision(repo, "refs/heads/main");
    expect(head.ok).toBe(true);
    if (head.ok) expect(head.value).not.toBe(rev);

    // The revision captured earlier still answers as it did: re-reading the ref later would
    // splice two revisions into one answer.
    const pinned = await readFileAtRevision(repo, rev, ARTIFACT);
    expect(pinned.ok).toBe(true);
    if (pinned.ok) {
      expect(pinned.value).toContain("canonical");
      expect(pinned.value).not.toContain("NEWER");
    }
  });

  it("resolves a ref to a full immutable commit id", async () => {
    const f = fixture("revid");
    const { rev } = await canonical(f);
    expect(rev).toMatch(/^[0-9a-f]{40}$/);
  });
});

describe("canonical reads — failure is explicit, never a workspace fallback", () => {
  it("fails when the path is absent at that revision, even though it exists in the workspace", async () => {
    const f = fixture("absent");
    const { repo, rev } = await canonical(f);
    // Present on disk in the workspace, absent from the committed revision.
    writeFileSync(path.join(f.work, "only-in-workspace.json"), '{"leak":true}');

    const read = await readFileAtRevision(repo, rev, "only-in-workspace.json");
    expect(read.ok).toBe(false);
    if (!read.ok) expect(read.error.code).toBe("missing_path");
  });

  it("fails on an unknown revision rather than substituting anything", async () => {
    const f = fixture("unknownrev");
    process.env["ICN_ROOT"] = f.icnRoot;
    const repo = await resolveCanonicalRepository();
    expect(repo.ok).toBe(true);
    if (!repo.ok) return;
    const rev = await resolveRevision(repo.value, "refs/heads/does-not-exist");
    expect(rev.ok).toBe(false);
    if (!rev.ok) expect(rev.error.code).toBe("unknown_revision");
  });

  it("rejects a path that escapes the repository root", async () => {
    const f = fixture("escape");
    const { repo, rev } = await canonical(f);
    const read = await readFileAtRevision(repo, rev, "../../../etc/passwd");
    expect(read.ok).toBe(false);
  });
});

describe("canonical reads — content is returned exactly as committed", () => {
  it("preserves leading and trailing whitespace", async () => {
    const f = fixture("fidelity");
    const body = "\n  {\"schema\":\"spaced\",\"nodes\":[],\"edges\":[]}  \n\n";
    writeFileSync(path.join(f.work, ARTIFACT), body);
    git(f.work, "add", ARTIFACT);
    git(f.work, "commit", "-m", "whitespace");
    git(f.work, "push", f.bare, "main");

    process.env["ICN_ROOT"] = f.icnRoot;
    const repo = await resolveCanonicalRepository();
    expect(repo.ok).toBe(true);
    if (!repo.ok) return;
    const rev = await resolveRevision(repo.value, "refs/heads/main");
    expect(rev.ok).toBe(true);
    if (!rev.ok) return;

    const read = await readFileAtRevision(repo.value, rev.value, ARTIFACT);
    expect(read.ok).toBe(true);
    // Trimming would silently change the committed bytes; content is not an identifier.
    if (read.ok) expect(read.value).toBe(body);
  });

  it("refuses a blob too large to return intact rather than returning a shortened one", async () => {
    const f = fixture("oversize");
    // runCommand truncates oversized output and still reports success. For file content that
    // makes a corrupted answer look like a smaller one, which is the fail-open shape this
    // whole line of work keeps meeting.
    //
    // The size is chosen deliberately. runCommand sets execFile's maxBuffer to
    // maxStdoutBytes + maxStderrBytes + 64 KiB, so a blob far above the limit makes the
    // subprocess fail outright and never reaches the truncation branch at all — a test using
    // one passed while the guard was mutated away. This sits in the window ABOVE the 8 MiB
    // output cap but BELOW maxBuffer, which is where truncation-reported-as-success actually
    // happens.
    const big = "x".repeat(8 * 1024 * 1024 + 32 * 1024);
    writeFileSync(path.join(f.work, "big.txt"), big);
    git(f.work, "add", "big.txt");
    git(f.work, "commit", "-m", "oversize");
    git(f.work, "push", f.bare, "main");

    process.env["ICN_ROOT"] = f.icnRoot;
    const repo = await resolveCanonicalRepository();
    expect(repo.ok).toBe(true);
    if (!repo.ok) return;
    const rev = await resolveRevision(repo.value, "refs/heads/main");
    expect(rev.ok).toBe(true);
    if (!rev.ok) return;

    const read = await readFileAtRevision(repo.value, rev.value, "big.txt");
    expect(read.ok).toBe(false);
    if (!read.ok) expect(read.error.code).toBe("read_failed");
  });

  it("reports a storage failure as read_failed, not as an absent path", async () => {
    const f = fixture("classify");
    const { repo, rev } = await canonical(f);
    // Destroy the object store: the path still exists in the tree, the bytes do not.
    rmSync(path.join(f.bare, "objects"), { recursive: true, force: true });

    const read = await readFileAtRevision(repo, rev, ARTIFACT);
    expect(read.ok).toBe(false);
    // Calling this "missing_path" would send an operator looking for a file that is committed.
    if (!read.ok) expect(read.error.code).toBe("read_failed");
  });
});

// End-to-end through the MCP handler. The unit tests above resolve `refs/heads/main`, but the
// handler resolves `refs/remotes/origin/main` — so a wrong production ref could pass every unit
// test while every real call failed. This fixture provides ONLY the production ref.
describe("icn_ops_agent_context_spine — the production path", () => {
  async function clientFor(icnRoot: string): Promise<Client> {
    process.env["ICN_ROOT"] = icnRoot;
    const server = new McpServer({ name: "icn-ops-test", version: "0.0.0" });
    registerAgentOpsTools(server);
    const [ct, st] = InMemoryTransport.createLinkedPair();
    const client = new Client({ name: "test-client", version: "0.0.0" });
    await Promise.all([server.connect(st), client.connect(ct)]);
    return client;
  }

  it("serves the spine from canonical storage at the production ref", async () => {
    const f = fixture("e2e");
    const head = git(f.work, "rev-parse", "HEAD");
    // Only refs/remotes/origin/main resolves: refs/heads/main is removed, so a handler reading
    // any other ref cannot succeed here.
    git(f.bare, "update-ref", "refs/remotes/origin/main", head);
    git(f.bare, "update-ref", "-d", "refs/heads/main");

    const client = await clientFor(f.icnRoot);
    const res = (await client.callTool({
      name: "icn_ops_agent_context_spine",
      arguments: {},
    })) as { isError?: boolean; content: Array<{ type: string; text: string }> };

    expect(res.isError).toBeFalsy();
    const body = JSON.parse(res.content[0]?.text ?? "{}") as Record<string, unknown>;
    const source = body["source"] as Record<string, unknown>;
    expect(source?.["source_revision"]).toBe(head);
    expect(source?.["worktree_consulted"]).toBe(false);
    expect(source?.["ref"]).toBe("refs/remotes/origin/main");
    expect(body["artifact"]).toBe(ARTIFACT);
  });

  it("fails closed as an MCP error when canonical storage is not configured", async () => {
    const f = fixture("e2e-unconfigured");
    writeFileSync(
      path.join(f.icnRoot, "ops", "state", "config", "repo-map.json"),
      JSON.stringify({ repos: { icn: { local: "." } } })
    );

    const client = await clientFor(f.icnRoot);
    const res = (await client.callTool({
      name: "icn_ops_agent_context_spine",
      arguments: {},
    })) as { isError?: boolean; content: Array<{ type: string; text: string }> };

    // A refusal delivered as a successful call is an absent answer arriving as a passing one.
    expect(res.isError).toBe(true);
    const body = JSON.parse(res.content[0]?.text ?? "{}") as Record<string, unknown>;
    expect(body["code"]).toBe("not_configured");
    expect(body["worktree_consulted"]).toBe(false);
  });
});
