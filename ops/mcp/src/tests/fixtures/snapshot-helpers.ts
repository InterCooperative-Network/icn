// Shared fixtures for the snapshot-export tests. Synthetic data only: nothing here opens, reads
// or names the real shared registry.

import { createHash } from "crypto";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import Database from "better-sqlite3";
import { expect } from "vitest";
import { initDb } from "../../state/db.js";
import { SnapshotError, type SnapshotErrorCode } from "../../state/snapshot-export.js";

export const sha256Hex = (data: string | Buffer): string => createHash("sha256").update(data).digest("hex");
export const sha256File = (p: string): string => sha256Hex(readFileSync(p));

export function tempDir(prefix = "icn-snapshot-test-"): string {
  return mkdtempSync(join(tmpdir(), prefix));
}
export const removeDir = (d: string): void => rmSync(d, { recursive: true, force: true });

/** Distinctive strings that must never appear in an observation export. */
export const CANARY = {
  task: "CANARY-task-description-7f3a",
  taskRef: "CANARY-task-ref-91bc",
  prRef: "CANARY-pr-ref-22de",
  activity: "CANARY-current-activity-5a10",
  transcript: "/home/CANARYUSER/.claude/projects/CANARY-transcript-c001.jsonl",
  worktreeFree: "CANARY-free-worktree-label-88f2",
  worktreePath: "/home/CANARYUSER/icn-dev/worktrees/CANARY-wt-path-4d4d",
  repoId: "/home/CANARYUSER/icn-dev/repos/CANARY-icn.git",
  worktreeId: "/home/CANARYUSER/icn-dev/repos/CANARY-icn.git/worktrees/CANARY-lane-6e6e",
  branch: "CANARY-branch-security-fix-3b3b",
  head: "CANARY0head0deadbeef0000000000000000000000",
  mailbox: "CANARY-mailbox-body-a9a9",
  mailboxKind: "CANARY-mailbox-kind-b7b7",
  eventPayload: "CANARY-event-payload-0c0c",
  eventScope: "CANARY-event-scope-1d1d",
  watcherLabel: "CANARY-watcher-label-2e2e",
  health: "CANARY-health-cache-endpoint-5c5c",
  decision: "CANARY-decision-title-3f3f",
  futureColumn: "CANARY-future-column-4a4a",
  absClaim: "/home/CANARYUSER/.ssh/CANARY-id_ed25519",
  tildeClaim: "~/CANARY-tilde-claim",
  dotdotClaim: "../../CANARY-dotdot-claim",
} as const;

/** A real, migrated schema-v5 registry — the production ladder, not a hand-rolled copy. */
export function makeRegistry(dir: string, name = "registry.db"): string {
  const p = join(dir, name);
  initDb(p).close();
  return p;
}

export function addSession(db: Database.Database, id: string, claims: string[] = []): void {
  db.prepare("INSERT INTO sessions (id, repo, worktree_name) VALUES (?, 'icn', ?)").run(id, `wt-${id}`);
  for (const f of claims) {
    db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)").run(f, id);
  }
}

/**
 * A registry with a sensitive canary in EVERY place the production code can put free-form or
 * path-like text, plus an unknown column a future migration might add.
 */
export function makeSensitiveRegistry(dir: string, name = "sensitive.db"): string {
  const p = makeRegistry(dir, name);
  const db = initDb(p);
  db.exec("ALTER TABLE sessions ADD COLUMN secret_note TEXT"); // future/unknown column
  const ins = db.prepare(
    `INSERT INTO sessions
       (id, repo, repo_id, worktree, worktree_id, worktree_path, worktree_name, worktree_generation,
        branch_at_registration, head_at_registration, task_description, task_ref, pr_ref,
        parent_session_id, provider, agent_pid, host, provider_session_id, transcript_path,
        current_activity, progress_count, last_progress, secret_note)
     VALUES (@id, 'icn', @repo_id, @worktree, @worktree_id, @worktree_path, @worktree_name, @gen,
        @branch, @head, @task, @task_ref, @pr_ref, @parent, 'claude', @pid, 'icn-dev', @psid, @transcript,
        @activity, 3, datetime('now'), @secret)`
  );
  const base = {
    repo_id: CANARY.repoId,
    worktree: CANARY.worktreeFree,
    worktree_id: CANARY.worktreeId,
    worktree_path: CANARY.worktreePath,
    worktree_name: "snapshot-export",
    gen: "gen-token-1",
    branch: CANARY.branch,
    head: CANARY.head,
    task: CANARY.task,
    task_ref: CANARY.taskRef,
    pr_ref: CANARY.prRef,
    parent: null,
    pid: 4242,
    psid: "provider-conv-1",
    transcript: CANARY.transcript,
    activity: CANARY.activity,
    secret: CANARY.futureColumn,
  };
  ins.run({ ...base, id: "sess-1" });
  ins.run({ ...base, id: "sess-2", parent: "sess-1", psid: "provider-conv-2", pid: null });
  const claim = db.prepare("INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)");
  claim.run("icn/crates/icn-core/src/lib.rs", "sess-1");
  claim.run("docs/architecture/AGENT_RUNTIME.md", "sess-1");
  claim.run(CANARY.absClaim, "sess-1");
  claim.run(CANARY.tildeClaim, "sess-1");
  claim.run(CANARY.dotdotClaim, "sess-2");
  claim.run(CANARY.absClaim, "sess-2"); // same unsafe path, different session
  claim.run("icn/crates/icn-core/src/lib.rs", "sess-2");
  db.prepare(
    "INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES ('sess-1','sess-2',?,?,1)"
  ).run(CANARY.mailboxKind, JSON.stringify({ body: CANARY.mailbox }));
  db.prepare("INSERT INTO events (scope, type, payload, created_at) VALUES (?, 'x', ?, 1)").run(
    CANARY.eventScope,
    JSON.stringify({ note: CANARY.eventPayload })
  );
  db.prepare(
    "INSERT INTO watchers_process (session_id, pid, label, created_at) VALUES ('sess-1', 9, ?, 1)"
  ).run(CANARY.watcherLabel);
  db.prepare("INSERT INTO health_cache (key, value) VALUES ('svc', ?)").run(JSON.stringify({ url: CANARY.health }));
  db.prepare("INSERT INTO decision_index (id, title, tags, file_path) VALUES ('d1', ?, 't', 'docs/adr/x.md')").run(
    CANARY.decision
  );
  db.close();
  return p;
}

/** Canonical logical content of every user table, order-independent of physical layout. */
export function logicalDump(path: string): string {
  const db = new Database(path, { readonly: true, fileMustExist: true });
  try {
    const tables = (
      db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .all() as Array<{ name: string }>
    ).map((t) => t.name);
    const out: Record<string, unknown[]> = {};
    for (const t of tables) out[t] = db.prepare(`SELECT * FROM "${t}" ORDER BY rowid`).all();
    return JSON.stringify(out);
  } finally {
    db.close();
  }
}

/** Names present in `d`, ignoring SQLite's own wal-index files on a WAL source. */
export const listing = (d: string): string[] =>
  readdirSync(d)
    .filter((n) => !/\.db-(wal|shm)$/.test(n))
    .sort();

export function expectRefusal(code: SnapshotErrorCode, fn: () => unknown): SnapshotError {
  try {
    fn();
  } catch (e) {
    expect(e).toBeInstanceOf(SnapshotError);
    expect((e as SnapshotError).code).toBe(code);
    return e as SnapshotError;
  }
  throw new Error(`expected SnapshotError ${code}, but the call succeeded`);
}

/** Every string the raw bytes of `path` contain from `needles` — scans the FILE, not queries. */
export function canariesInFile(path: string, needles: readonly string[]): string[] {
  const bytes = readFileSync(path);
  return needles.filter((n) => bytes.includes(Buffer.from(n, "utf8")));
}
