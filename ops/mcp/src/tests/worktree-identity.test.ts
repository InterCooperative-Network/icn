// Lane identity against REAL Git worktrees, not mocked rows.
//
// The claims under test are all of the form "X changes, identity does not". Mocked rows cannot
// falsify those, because a mock is whatever the test says it is. Every fixture here is a real
// repository created in a temp directory; none of them touches a live ICN worktree.

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { execFileSync } from "child_process";
import { existsSync, mkdtempSync, readdirSync, rmSync, symlinkSync, writeFileSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  branchChanged,
  discoverWorktree,
  laneGeneration,
  readBranchState,
} from "../runtime/worktree-identity.js";
import { initDb } from "../state/db.js";
import {
  activeSessionsForWorktree,
  classifyWorktree,
  registerSession,
  releaseSession,
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

  it("survives a rebase that ACTUALLY rewrites history", () => {
    const before = discoverWorktree(wtA)!.worktree_id;

    // This test used to run `git rebase --onto main main`, which rewrites NOTHING: it captured
    // `head0` and then discarded it with `void head0;`, so it asserted identity survived an
    // operation that never happened. Build genuinely divergent history instead, and prove the
    // rewrite occurred before claiming identity survived it.
    writeFileSync(join(repoA, "trunk.txt"), "trunk\n");
    git(repoA, "add", "trunk.txt");
    git(repoA, "commit", "-qm", "trunk advances");

    writeFileSync(join(wtA, "lane.txt"), "lane\n");
    git(wtA, "add", "lane.txt");
    git(wtA, "commit", "-qm", "lane work");

    const head0 = readBranchState(wtA).head;
    git(wtA, "rebase", "-q", "main");
    const head1 = readBranchState(wtA).head;

    expect(head0).toBeTruthy();
    expect(head1).toBeTruthy();
    expect(head1).not.toBe(head0); // the rewrite is real, so the claim below means something
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
      }

      releaseSession(db, a.session_id, { reason: "completed" });

      // b is untouched: still registered, still holding its claim and its supervision.
      expect(activeSessionsForWorktree(db, identity.worktree_id).map((r) => r.id)).toEqual([
        b.session_id,
      ]);
      expect(
        db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(b.session_id)
      ).toEqual({ c: 1 });
      // a surrendered everything.
      expect(
        db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(a.session_id)
      ).toEqual({ c: 0 });
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
      db.prepare(
        "INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES (?, 'x', 'text', '{}', ?)"
      ).run(a1.session_id, Date.now());

      // SessionEnd: all authority surrendered.
      const rel = releaseSession(db, a1.session_id, { reason: "other" });
      expect(rel.dropped).toEqual({ file_claims: 1, undelivered_messages: 1 });

      // Later --resume: SAME provider conversation, NEW runtime activation.
      const b = registerSession(db, { repo: "repoA", identity, provider_session_id: CONV });
      expect(b.created).toBe(true);
      expect(b.session_id).not.toBe(a1.session_id);
      expect(b.provider_session_id).toBe(a1.provider_session_id);

      // B inherits nothing ephemeral from A.
      expect(
        db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(b.session_id)
      ).toEqual({ c: 0 });
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

// ═══════════════════════════════════════════════════════════════════════════
// LANE IDENTITY IN TIME (A3)
//
// worktree_id answers "which lane, in space". It does NOT answer "which lane, WHEN".
// Git recycles `<repo>/.git/worktrees/<basename>` after `git worktree remove`, and a worktree
// can be recreated at the EXACT SAME pathname — at which point repo, admin dir AND recorded
// path all match, and a brand-new generation inherits the previous one's unreleased rows.
// Verified before the fix: a fresh `gen2` worktree classified REGISTERED-ACTIVE holding
// `gen1`'s session row. A path comparison is structurally incapable of catching that, which is
// why the discriminator is a token minted per generation inside the directory git deletes.
// ═══════════════════════════════════════════════════════════════════════════

describe("worktree generation — identity in time", () => {
  let gRoot: string;
  let gRepo: string;

  beforeAll(() => {
    gRoot = mkdtempSync(join(tmpdir(), "icn-lane-generation-"));
    gRepo = join(gRoot, "repo");
    makeRepo(gRepo);
  });
  afterAll(() => rmSync(gRoot, { recursive: true, force: true }));

  const genOf = (path: string): string | null => {
    const id = discoverWorktree(path);
    expect(id).not.toBeNull();
    return id!.worktree_generation;
  };

  it("is minted, non-empty, and stable when re-read", () => {
    const wt = join(gRoot, "stable");
    git(gRepo, "worktree", "add", "-q", "-b", "stable-lane", wt);
    const first = genOf(wt);
    expect(first).toBeTruthy();
    // Re-reading must NOT mint a second token — that would make every call a new generation.
    expect(genOf(wt)).toBe(first);
  });

  it("survives commits, branch switch, branch rename, detached HEAD", () => {
    const wt = join(gRoot, "churn");
    git(gRepo, "worktree", "add", "-q", "-b", "churn-lane", wt);
    const before = genOf(wt);

    writeFileSync(join(wt, "a.txt"), "a\n");
    git(wt, "add", "a.txt");
    git(wt, "commit", "-qm", "c1");
    expect(genOf(wt)).toBe(before);

    git(wt, "checkout", "-q", "-b", "churn-other");
    expect(genOf(wt)).toBe(before);

    git(wt, "branch", "-m", "churn-renamed");
    expect(genOf(wt)).toBe(before);

    git(wt, "checkout", "-q", "--detach");
    expect(genOf(wt)).toBe(before);
  });

  it("survives a GENUINE history rewrite (HEAD actually changes)", () => {
    const wt = join(gRoot, "rebased");
    git(gRepo, "worktree", "add", "-q", "-b", "rebase-lane", wt);
    const before = genOf(wt);

    // Divergent history: main moves on, the lane commits on top of the OLD base.
    writeFileSync(join(gRepo, "trunk.txt"), "trunk\n");
    git(gRepo, "add", "trunk.txt");
    git(gRepo, "commit", "-qm", "trunk moves");

    writeFileSync(join(wt, "mine.txt"), "mine\n");
    git(wt, "add", "mine.txt");
    git(wt, "commit", "-qm", "mine");
    const headBefore = git(wt, "rev-parse", "HEAD");

    git(wt, "rebase", "-q", "main");
    const headAfter = git(wt, "rev-parse", "HEAD");

    // The point of the test: if HEAD did not move, the rebase was a no-op and the identity
    // claim below is worthless. An earlier version of this fixture rebased `--onto main main`,
    // which rewrites nothing at all.
    expect(headAfter).not.toBe(headBefore);
    expect(genOf(wt)).toBe(before);
  });

  it("is identical when the same worktree is reached through a symlink", () => {
    const wt = join(gRoot, "symlinked");
    git(gRepo, "worktree", "add", "-q", "-b", "symlink-lane", wt);
    const direct = genOf(wt);
    const link = join(gRoot, "symlink-alias");
    symlinkSync(wt, link);
    expect(genOf(link)).toBe(direct);
    // and the lane itself is still one lane
    expect(discoverWorktree(link)!.worktree_id).toBe(discoverWorktree(wt)!.worktree_id);
  });

  it("DIFFERS after remove/recreate at the EXACT SAME path", () => {
    const wt = join(gRoot, "recreated-same");
    git(gRepo, "worktree", "add", "-q", "-b", "same-gen1", wt);
    const idBefore = discoverWorktree(wt)!;
    const genBefore = idBefore.worktree_generation;

    git(gRepo, "worktree", "remove", "--force", wt);
    git(gRepo, "worktree", "add", "-q", "-b", "same-gen2", wt);
    const idAfter = discoverWorktree(wt)!;

    // Everything git-derived is IDENTICAL — this is exactly why a path check cannot help.
    expect(idAfter.worktree_id).toBe(idBefore.worktree_id);
    expect(idAfter.worktree_path).toBe(idBefore.worktree_path);
    expect(idAfter.repo_id).toBe(idBefore.repo_id);
    // Only the generation separates them.
    expect(idAfter.worktree_generation).toBeTruthy();
    expect(idAfter.worktree_generation).not.toBe(genBefore);
  });

  it("DIFFERS after remove/recreate at a DIFFERENT path with the same basename", () => {
    const older = join(gRoot, "old", "shared-name");
    const newer = join(gRoot, "new", "shared-name");
    mkdirSync(join(gRoot, "old"), { recursive: true });
    mkdirSync(join(gRoot, "new"), { recursive: true });
    git(gRepo, "worktree", "add", "-q", "-b", "diff-gen1", older);
    const genBefore = genOf(older);
    git(gRepo, "worktree", "remove", "--force", older);
    git(gRepo, "worktree", "add", "-q", "-b", "diff-gen2", newer);
    expect(genOf(newer)).not.toBe(genBefore);
  });

  it("differs between same-basename lanes in one repo and across repos", () => {
    const one = join(gRoot, "p1", "task-review");
    const two = join(gRoot, "p2", "task-review");
    mkdirSync(join(gRoot, "p1"), { recursive: true });
    mkdirSync(join(gRoot, "p2"), { recursive: true });
    git(gRepo, "worktree", "add", "-q", "-b", "coexist-1", one);
    git(gRepo, "worktree", "add", "-q", "-b", "coexist-2", two);
    expect(genOf(one)).not.toBe(genOf(two));
    // and against the OTHER repository's identically-named lane from the shared fixture
    expect(genOf(one)).not.toBe(discoverWorktree(wtB)!.worktree_generation);
  });

  it("a recreated lane does not adopt the previous generation's session rows", () => {
    const wt = join(gRoot, "adoption");
    git(gRepo, "worktree", "add", "-q", "-b", "adopt-gen1", wt);
    const db = initDb(":memory:");
    const first = discoverWorktree(wt)!;
    registerSession(db, {
      repo: "gen",
      identity: first,
      branch_state: readBranchState(first.worktree_path),
      provider_session_id: "gen1-session",
      agent_pid: 999999,
    });
    expect(activeSessionsForWorktree(db, first.worktree_id)).toHaveLength(1);

    git(gRepo, "worktree", "remove", "--force", wt);
    git(gRepo, "worktree", "add", "-q", "-b", "adopt-gen2", wt);
    const second = discoverWorktree(wt)!;
    expect(second.worktree_id).toBe(first.worktree_id); // git really did recycle it

    // The row still EXISTS — nothing is destroyed — it is simply no longer this lane's.
    expect(
      (db.prepare("SELECT COUNT(*) AS c FROM sessions").get() as { c: number }).c
    ).toBe(1);
    expect(activeSessionsForWorktree(db, second.worktree_id)).toHaveLength(0);

    // ...and the lane therefore reads PROTECTED, not actionable.
    const c = classifyWorktree(db, second.worktree_id, { observed_pids: [] });
    expect(c.state).toBe("UNREGISTERED-OBSERVED");
    expect(c.session_id).toBeNull();
    expect(c.contention.count).toBe(0);
  });

  it("a RESUMED conversation in a recreated lane keeps its attribution", () => {
    // THE FAIL-SAFE INVERSION THIS FILTER ONCE CAUSED.
    //
    // `moved` is `prior.worktree_id !== worktreeId`, and git RECYCLES the admin dir — so a lane
    // removed and recreated at the same pathname keeps its worktree_id, `moved` is false, and a
    // resumed conversation that deduped into its old row kept the DEAD generation. The filter
    // then discarded the row of a LIVE, just-registered, heartbeating session. Measured:
    // register reported success, and classify answered UNREGISTERED-OBSERVED with contention 0.
    const wt = join(gRoot, "resumed");
    git(gRepo, "worktree", "add", "-q", "-b", "resume-gen1", wt);
    const db = initDb(":memory:");
    const first = discoverWorktree(wt)!;
    registerSession(db, {
      repo: "gen",
      identity: first,
      branch_state: readBranchState(first.worktree_path),
      provider_session_id: "resumed-conversation",
      agent_pid: process.pid,
    });

    git(gRepo, "worktree", "remove", "--force", wt);
    git(gRepo, "worktree", "add", "-q", "-b", "resume-gen2", wt);
    const second = discoverWorktree(wt)!;
    expect(second.worktree_id).toBe(first.worktree_id); // git recycled it
    expect(second.worktree_generation).not.toBe(first.worktree_generation);

    const again = registerSession(db, {
      repo: "gen",
      identity: second,
      branch_state: readBranchState(second.worktree_path),
      provider_session_id: "resumed-conversation",
      agent_pid: process.pid,
    });
    expect(again.deduplicated).toBe(true);

    expect(activeSessionsForWorktree(db, second.worktree_id)).toHaveLength(1);
    const c = classifyWorktree(db, second.worktree_id, { observed_pids: [] });
    expect(c.state).not.toBe("UNREGISTERED-OBSERVED");
    expect(c.contention.count).toBe(1);
    expect(c.session_id).toBe(again.session_id);
  });

  it("the generation MOVES with the lane on a genuine lane change", () => {
    // The lane-move UPDATE carries worktree_generation, and nothing covered it: the only prior
    // lane-move test used synthetic, non-existent lane directories, so the column was never
    // compared against a real second lane.
    const laneA = join(gRoot, "move-a");
    const laneB = join(gRoot, "move-b");
    git(gRepo, "worktree", "add", "-q", "-b", "move-a", laneA);
    git(gRepo, "worktree", "add", "-q", "-b", "move-b", laneB);
    const db = initDb(":memory:");
    const a = discoverWorktree(laneA)!;
    const b = discoverWorktree(laneB)!;
    expect(a.worktree_generation).not.toBe(b.worktree_generation);

    registerSession(db, {
      repo: "gen", identity: a, branch_state: readBranchState(a.worktree_path),
      provider_session_id: "moving-conversation", agent_pid: process.pid,
    });
    const moved = registerSession(db, {
      repo: "gen", identity: b, branch_state: readBranchState(b.worktree_path),
      provider_session_id: "moving-conversation", agent_pid: process.pid,
    });
    expect(moved.deduplicated).toBe(true);

    const row = db
      .prepare("SELECT worktree_generation AS g FROM sessions WHERE id = ?")
      .get(moved.session_id) as { g: string | null };
    expect(row.g).toBe(b.worktree_generation);
    expect(activeSessionsForWorktree(db, b.worktree_id)).toHaveLength(1);
    expect(activeSessionsForWorktree(db, a.worktree_id)).toHaveLength(0);
  });

  it("the READ path never mints, and never writes outside a Git admin directory", () => {
    // classify() takes a caller-supplied worktree_id — `session_lifecycle`'s unvalidated input,
    // or a CLI `--worktree <name>` that falls back to a path relative to the process cwd. A
    // read path that mints created a file at an attacker-chosen location.
    const victim = join(gRoot, "not-an-admin-dir");
    mkdirSync(victim, { recursive: true });
    expect(laneGeneration(victim)).toBeNull();
    expect(readdirSync(victim)).toEqual([]);

    expect(laneGeneration(victim, { mint: true })).toBeNull();
    expect(readdirSync(victim)).toEqual([]);

    const fresh = join(gRoot, "unminted");
    git(gRepo, "worktree", "add", "-q", "-b", "unminted-lane", fresh);
    const admin = git(fresh, "rev-parse", "--absolute-git-dir");
    expect(laneGeneration(admin)).toBeNull();
    expect(existsSync(join(admin, "icn-lane-generation"))).toBe(false);
    expect(discoverWorktree(fresh)!.worktree_generation).toBeTruthy();
    expect(existsSync(join(admin, "icn-lane-generation"))).toBe(true);
  });

  it("falls back to the PATH for pre-v5 rows, in both directions", () => {
    // The only protection a NULL-generation row has. Deleting the path comparison left the
    // whole suite green, because the one test covering unknown generations exercises only the
    // KEEP direction — which a missing filter also produces. Both directions are needed.
    const wt = join(gRoot, "pathfallback");
    git(gRepo, "worktree", "add", "-q", "-b", "pathfallback", wt);
    const db = initDb(":memory:");
    const id = discoverWorktree(wt)!;
    registerSession(db, {
      repo: "gen", identity: id, branch_state: readBranchState(id.worktree_path),
      provider_session_id: "pre-v5-row", agent_pid: process.pid,
    });
    // Simulate a row written before v5: no generation recorded.
    db.prepare("UPDATE sessions SET worktree_generation = NULL").run();

    // KEEP: the recorded path still matches the live one.
    expect(activeSessionsForWorktree(db, id.worktree_id)).toHaveLength(1);

    // DROP: the recorded path is a lane that no longer lives here — the recycled-admin-dir
    // case, which is the whole reason the fallback exists.
    db.prepare("UPDATE sessions SET worktree_path = ?").run(join(gRoot, "some-removed-lane"));
    expect(activeSessionsForWorktree(db, id.worktree_id)).toHaveLength(0);

    // ...and a row that never recorded a path at all stays KEPT: unknown is never "different".
    db.prepare("UPDATE sessions SET worktree_path = NULL").run();
    expect(activeSessionsForWorktree(db, id.worktree_id)).toHaveLength(1);
  });

  it("keeps rows when the generation is UNKNOWN on either side (fail safe)", () => {
    const wt = join(gRoot, "unknown-gen");
    git(gRepo, "worktree", "add", "-q", "-b", "unknown-lane", wt);
    const db = initDb(":memory:");
    const id = discoverWorktree(wt)!;
    registerSession(db, {
      repo: "gen",
      identity: id,
      branch_state: readBranchState(id.worktree_path),
      provider_session_id: "unknown-gen-session",
      agent_pid: 999999,
    });
    // Simulate a pre-v5 row: generation not recorded. It must still be attributed, because
    // "unknown" may never be read as "different" — that direction loses protection.
    db.prepare("UPDATE sessions SET worktree_generation = NULL").run();
    expect(activeSessionsForWorktree(db, id.worktree_id)).toHaveLength(1);
  });
});
