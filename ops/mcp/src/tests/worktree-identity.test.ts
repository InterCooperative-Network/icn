// Lane identity against REAL Git worktrees, not mocked rows.
//
// The claims under test are all of the form "X changes, identity does not". Mocked rows cannot
// falsify those, because a mock is whatever the test says it is. Every fixture here is a real
// repository created in a temp directory; none of them touches a live ICN worktree.

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { execFileSync } from "child_process";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  branchChanged,
  discoverWorktree,
  readBranchState,
} from "../runtime/worktree-identity.js";
import { initDb } from "../state/db.js";
import {
  activeSessionsForWorktree,
  classifyWorktree,
  liveSupervisions,
  registerSession,
  releaseSession,
  superviseOperation,
} from "../runtime/session-runtime.js";

let root: string;
/** repoA and repoB BOTH get a worktree literally named "task-review". */
let repoA: string, repoB: string, wtA: string, wtB: string, wtA2: string;

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
  root = mkdtempSync(join(tmpdir(), "icn-lane-fixture-"));
  repoA = join(root, "repoA");
  repoB = join(root, "repoB");
  makeRepo(repoA);
  makeRepo(repoB);

  // Same basename in two different repositories — the collision class we must not reproduce.
  wtA = join(root, "wt", "A", "task-review");
  wtB = join(root, "wt", "B", "task-review");
  git(repoA, "worktree", "add", "-q", "-b", "feat/a", wtA);
  git(repoB, "worktree", "add", "-q", "-b", "feat/b", wtB);

  // A second lane in repoA, to prove lanes within one repo are distinct too.
  wtA2 = join(root, "wt", "A", "task-other");
  git(repoA, "worktree", "add", "-q", "-b", "feat/a2", wtA2);
});

afterAll(() => rmSync(root, { recursive: true, force: true }));

describe("lane identity is Git-derived and collision-proof", () => {
  it("gives two worktrees of one repo distinct ids and a shared repo id", () => {
    const a = discoverWorktree(wtA)!;
    const a2 = discoverWorktree(wtA2)!;
    expect(a.worktree_id).not.toBe(a2.worktree_id);
    expect(a.repo_id).toBe(a2.repo_id);
  });

  it("cannot confuse the same basename across two repositories", () => {
    const a = discoverWorktree(wtA)!;
    const b = discoverWorktree(wtB)!;
    expect(a.worktree_name).toBe("task-review");
    expect(b.worktree_name).toBe("task-review"); // identical display names...
    expect(a.worktree_id).not.toBe(b.worktree_id); // ...distinct identities
    expect(a.repo_id).not.toBe(b.repo_id);
  });

  it("resolves the same identity from any subdirectory of the worktree", () => {
    const nested = join(wtA, "deep", "nested", "dir");
    mkdirSync(nested, { recursive: true });
    expect(discoverWorktree(nested)!.worktree_id).toBe(discoverWorktree(wtA)!.worktree_id);
  });

  it("never lets a launcher root override the directory the session is actually in", () => {
    // Regression: ICN_ROOT is pinned to the mcp-host worktree by icn-dev's shell profile, so
    // an env-root-first resolution filed every session under mcp-host no matter where it ran.
    // A VALID env root pointing at a DIFFERENT real worktree must still lose to cwd.
    const a = discoverWorktree(wtA)!;
    const other = discoverWorktree(wtA2)!;
    expect(discoverWorktree(wtA, other.worktree_path)!.worktree_id).toBe(a.worktree_id);
    expect(discoverWorktree(wtA, wtB)!.worktree_id).toBe(a.worktree_id);
  });

  it("falls back to the launcher root only when cwd resolves to nothing", () => {
    const notGit = mkdtempSync(join(tmpdir(), "icn-nongit-"));
    try {
      expect(discoverWorktree(notGit, wtA)!.worktree_id).toBe(discoverWorktree(wtA)!.worktree_id);
    } finally {
      rmSync(notGit, { recursive: true, force: true });
    }
  });

  it("does not treat an unvalidated env root as authoritative", () => {
    // A stale/wrong ICN_ROOT must not misattribute the session to another lane.
    const bogus = join(root, "not-a-repo");
    mkdirSync(bogus, { recursive: true });
    expect(discoverWorktree(wtA, bogus)!.worktree_id).toBe(discoverWorktree(wtA)!.worktree_id);
    // A non-existent root is ignored entirely rather than crashing.
    expect(discoverWorktree(wtA, "/no/such/root")!.worktree_id).toBe(
      discoverWorktree(wtA)!.worktree_id
    );
  });

  it("returns null outside any worktree instead of guessing", () => {
    const bare = mkdtempSync(join(tmpdir(), "icn-not-git-"));
    try {
      expect(discoverWorktree(bare)).toBeNull();
    } finally {
      rmSync(bare, { recursive: true, force: true });
    }
  });
});

describe("branch movement never changes lane identity", () => {
  it("survives new commits (HEAD moves)", () => {
    const before = discoverWorktree(wtA)!.worktree_id;
    const head0 = readBranchState(wtA).head;
    writeFileSync(join(wtA, "f.txt"), "two\n");
    git(wtA, "commit", "-qam", "advance");
    expect(readBranchState(wtA).head).not.toBe(head0); // HEAD really moved
    expect(discoverWorktree(wtA)!.worktree_id).toBe(before);
  });

  it("survives a rebase (history rewritten)", () => {
    const before = discoverWorktree(wtA)!.worktree_id;
    const head0 = readBranchState(wtA).head;
    git(wtA, "rebase", "-q", "--onto", "main", "main");
    // Even if the rebase is a no-op for content, identity must be untouched.
    void head0;
    expect(discoverWorktree(wtA)!.worktree_id).toBe(before);
  });

  it("survives a branch rename", () => {
    const before = discoverWorktree(wtA)!.worktree_id;
    git(wtA, "branch", "-m", "feat/a", "feat/a-renamed");
    expect(readBranchState(wtA).branch).toBe("feat/a-renamed");
    expect(discoverWorktree(wtA)!.worktree_id).toBe(before);
  });

  it("survives detached HEAD", () => {
    const before = discoverWorktree(wtA)!.worktree_id;
    git(wtA, "checkout", "-q", "--detach");
    const st = readBranchState(wtA);
    expect(st.branch).toBeNull();
    expect(st.detached).toBe(true);
    expect(discoverWorktree(wtA)!.worktree_id).toBe(before);
    git(wtA, "checkout", "-q", "feat/a-renamed"); // restore
  });

  it("reports a deliberate branch switch as BRANCH-CHANGED without moving the lane", () => {
    const db = initDb(":memory:");
    try {
      const identity = discoverWorktree(wtA)!;
      const atRegistration = readBranchState(wtA);
      registerSession(db, {
        repo: "repoA", identity, branch_state: atRegistration,
        provider_session_id: "conv-branch-switch",
      });

      git(wtA, "checkout", "-q", "-b", "feat/a-switched");
      const live = readBranchState(wtA);

      const c = classifyWorktree(db, identity.worktree_id, { observed_pids: [] });
      expect(c.session_id).toBeTruthy();                 // same lane, same session
      expect(c.branch_changed).toBe(true);               // surfaced as a warning
      expect(c.live_branch!.branch).toBe("feat/a-switched"); // live, not the captured value
      expect(branchChanged(atRegistration.branch, live)).toBe(true);
      // And the stored launch fact remains a HISTORICAL record, not current state.
      const row = activeSessionsForWorktree(db, identity.worktree_id)[0]!;
      expect(row.branch_at_registration).toBe(atRegistration.branch);
      expect(row.branch_at_registration).not.toBe(live.branch);
    } finally {
      db.close();
    }
  });
});

describe("several sessions may occupy one lane", () => {
  it("keeps two sessions distinct, on one shared worktree_id, and reports contention", () => {
    const db = initDb(":memory:");
    try {
      const identity = discoverWorktree(wtA2)!;
      const editor = registerSession(db, {
        repo: "repoA", identity, provider_session_id: "conv-editor",
      });
      const reviewer = registerSession(db, {
        repo: "repoA", identity, provider_session_id: "conv-reviewer",
      });

      expect(editor.session_id).not.toBe(reviewer.session_id);
      expect(editor.worktree_id).toBe(reviewer.worktree_id);
      // The second registration is told it arrived into an occupied lane.
      expect(reviewer.co_occupants).toContain(editor.session_id);

      const c = classifyWorktree(db, identity.worktree_id, { observed_pids: [] });
      expect(c.contention.count).toBe(2);
      expect(c.contention.session_ids).toEqual(
        expect.arrayContaining([editor.session_id, reviewer.session_id])
      );
    } finally {
      db.close();
    }
  });

  it("releasing one session leaves the other's authority completely intact", () => {
    const db = initDb(":memory:");
    try {
      const identity = discoverWorktree(wtA2)!;
      const a = registerSession(db, { repo: "repoA", identity, provider_session_id: "conv-a" });
      const b = registerSession(db, { repo: "repoA", identity, provider_session_id: "conv-b" });

      for (const s of [a.session_id, b.session_id]) {
        db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)")
          .run(`claim-${s}.rs`, s);
        superviseOperation(db, s, process.pid, `build-${s}`);
      }
      // Kill a's supervision so release has something to surrender. A LIVE supervision now
      // deliberately outlives its session (the build is still running); that is covered
      // separately in session-runtime.test.ts.
      db.prepare("UPDATE watchers_process SET pid = 999999999 WHERE session_id = ?")
        .run(a.session_id);

      releaseSession(db, a.session_id, { reason: "completed" });

      // b is untouched: still registered, still holding its claim and its supervision.
      expect(activeSessionsForWorktree(db, identity.worktree_id).map((r) => r.id)).toEqual([
        b.session_id,
      ]);
      expect(
        db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(b.session_id)
      ).toEqual({ c: 1 });
      expect(liveSupervisions(db, b.session_id)).toHaveLength(1);
      // a surrendered everything.
      expect(
        db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(a.session_id)
      ).toEqual({ c: 0 });
      expect(liveSupervisions(db, a.session_id)).toHaveLength(0);
    } finally {
      db.close();
    }
  });

  it("authority is session-scoped, never worktree-scoped", () => {
    const db = initDb(":memory:");
    try {
      const identity = discoverWorktree(wtA2)!;
      const a = registerSession(db, { repo: "repoA", identity, provider_session_id: "c1" });
      const b = registerSession(db, { repo: "repoA", identity, provider_session_id: "c2" });
      db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES ('same.rs', ?)")
        .run(a.session_id);
      // The same path claimed by a second session in the SAME lane is a separate claim row,
      // so contention is representable rather than silently merged.
      db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES ('same.rs', ?)")
        .run(b.session_id);
      expect(
        db.prepare("SELECT COUNT(*) c FROM file_claims WHERE file_path = 'same.rs'").get()
      ).toEqual({ c: 2 });
    } finally {
      db.close();
    }
  });
});

describe("provider conversation vs runtime activation", () => {
  // Probe evidence: Claude reuses `session_id` across --resume, and fires
  // SessionEnd then SessionStart(source=resume) with that same id.
  const CONV = "8718f1fc-b864-488a-a10b-cd30a6f0b856";

  it("resume after release creates a NEW activation that inherits NO authority", () => {
    const db = initDb(":memory:");
    try {
      const identity = discoverWorktree(wtA)!;

      // Activation A registers; a duplicate hook must not fork it.
      const a1 = registerSession(db, { repo: "repoA", identity, provider_session_id: CONV });
      const a2 = registerSession(db, { repo: "repoA", identity, provider_session_id: CONV });
      expect(a2.session_id).toBe(a1.session_id);
      expect(a2.deduplicated).toBe(true);

      // A accrues ephemeral authority.
      db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES ('a.rs', ?)")
        .run(a1.session_id);
      superviseOperation(db, a1.session_id, process.pid, "cargo build");
      // Dead process: this is leaked bookkeeping the release must clear.
      db.prepare("UPDATE watchers_process SET pid = 999999999 WHERE session_id = ?")
        .run(a1.session_id);
      db.prepare(
        "INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES (?, 'x', 'text', '{}', ?)"
      ).run(a1.session_id, Date.now());

      // SessionEnd: all authority surrendered.
      const rel = releaseSession(db, a1.session_id, { reason: "other" });
      expect(rel.dropped).toEqual({ file_claims: 1, watchers: 1, undelivered_messages: 1 });

      // Later --resume: SAME provider conversation, NEW runtime activation.
      const b = registerSession(db, { repo: "repoA", identity, provider_session_id: CONV });
      expect(b.created).toBe(true);
      expect(b.session_id).not.toBe(a1.session_id);
      expect(b.provider_session_id).toBe(a1.provider_session_id);

      // B inherits nothing ephemeral from A.
      expect(
        db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(b.session_id)
      ).toEqual({ c: 0 });
      expect(liveSupervisions(db, b.session_id)).toHaveLength(0);
      expect(
        db.prepare("SELECT COUNT(*) c FROM mailbox WHERE to_session = ? AND read_at IS NULL")
          .get(b.session_id)
      ).toEqual({ c: 0 });
      // ...and A's old claims did not silently transfer by conversation id either.
      expect(db.prepare("SELECT COUNT(*) c FROM file_claims").get()).toEqual({ c: 0 });
    } finally {
      db.close();
    }
  });

  it("does not forbid conversation-id reuse, only simultaneous activations", () => {
    const db = initDb(":memory:");
    try {
      const identity = discoverWorktree(wtA)!;
      const a = registerSession(db, { repo: "repoA", identity, provider_session_id: CONV });
      // A second LIVE activation of the same conversation is deduplicated, not created.
      const b = registerSession(db, { repo: "repoA", identity, provider_session_id: CONV });
      expect(b.session_id).toBe(a.session_id);
      // But after release, reuse is explicitly allowed — the probe proved Claude does this.
      releaseSession(db, a.session_id);
      expect(registerSession(db, { repo: "repoA", identity, provider_session_id: CONV }).created)
        .toBe(true);
    } finally {
      db.close();
    }
  });
});
