import Database from "better-sqlite3";
import { existsSync, mkdirSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const DEFAULT_DB_PATH = join(__dirname, "../../data/icn-ops.db");

export function initDb(dbPath?: string): Database.Database {
  const path = dbPath ?? process.env["ICN_OPS_DB"] ?? DEFAULT_DB_PATH;

  // Ensure data directory exists
  const dir = dirname(path);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }

  const db = new Database(path);

  // WAL mode for concurrent reads across agent sessions
  db.pragma("journal_mode = WAL");
  db.pragma("foreign_keys = ON");
  // Wait up to 5 s when another writer holds the lock (multi-agent safety)
  db.pragma("busy_timeout = 5000");

  migrate(db);
  return db;
}

/** SQL DDL batches on the SQLite connection (better-sqlite3 API). */
function migrate(db: Database.Database): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS sessions (
      id TEXT PRIMARY KEY,
      repo TEXT NOT NULL,
      worktree TEXT,
      task_description TEXT,
      started_at TEXT DEFAULT (datetime('now')),
      last_heartbeat TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS file_claims (
      file_path TEXT NOT NULL,
      session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
      claimed_at TEXT DEFAULT (datetime('now')),
      PRIMARY KEY (file_path, session_id)
    );

    CREATE TABLE IF NOT EXISTS health_cache (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL,
      polled_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS decision_index (
      id TEXT PRIMARY KEY,
      title TEXT NOT NULL,
      tags TEXT,
      file_path TEXT NOT NULL,
      created_at TEXT
    );

    CREATE TABLE IF NOT EXISTS schema_version (
      version INTEGER PRIMARY KEY,
      applied_at TEXT DEFAULT (datetime('now'))
    );
  `);

  // Record migration version
  const version = db
    .prepare(
      "SELECT COUNT(*) as count FROM schema_version WHERE version = 1"
    )
    .get() as { count: number };
  if (version.count === 0) {
    db.prepare("INSERT INTO schema_version (version) VALUES (1)").run();
  }

  // Schema v2: Event bus + mailbox + process watchers
  const v2 = db
    .prepare("SELECT COUNT(*) as count FROM schema_version WHERE version = 2")
    .get() as { count: number };
  if (v2.count === 0) {
    db.exec(`
      CREATE TABLE IF NOT EXISTS events (
        id INTEGER PRIMARY KEY,
        scope TEXT NOT NULL,
        type TEXT NOT NULL,
        payload TEXT NOT NULL DEFAULT '{}',
        created_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_events_scope ON events(scope, id);
      CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);

      CREATE TABLE IF NOT EXISTS mailbox (
        id INTEGER PRIMARY KEY,
        to_session TEXT NOT NULL,
        from_session TEXT,
        kind TEXT NOT NULL DEFAULT 'text',
        payload TEXT NOT NULL DEFAULT '{}',
        created_at INTEGER NOT NULL,
        read_at INTEGER
      );
      CREATE INDEX IF NOT EXISTS idx_mailbox_to ON mailbox(to_session, id);

      CREATE TABLE IF NOT EXISTS watchers_process (
        id INTEGER PRIMARY KEY,
        session_id TEXT NOT NULL,
        pid INTEGER NOT NULL,
        label TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        completed_at INTEGER,
        exit_code INTEGER,
        status TEXT NOT NULL DEFAULT 'running'
      );
      CREATE INDEX IF NOT EXISTS idx_watchers_session ON watchers_process(session_id, status);
    `);
    db.prepare("INSERT INTO schema_version (version) VALUES (2)").run();
  }

  // Schema v3: agent session runtime identity + progress semantics (Refs icn#2653 follow-on).
  //
  // MIGRATION-SAFETY EVIDENCE (checked before this block was finalised):
  //   The only persistent ICN ops database on icn-dev is
  //   worktrees/icn/mcp-host/ops/mcp/data/icn-ops.db. At the time of writing it reported
  //   schema_version = {1, 2} and the original six-column `sessions` table — it has never
  //   executed any v3. An earlier draft of this migration (which included a `state` column)
  //   only ever ran against :memory: and a throwaway scratch database outside ICN state, both
  //   since discarded. Editing v3 in place is therefore safe and no v4 reconciliation is
  //   required. If a future draft changes shape AFTER a real database has stamped v3, add a
  //   new numbered migration instead of editing this one — a stamped version is skipped
  //   wholesale, so an in-place edit would silently leave that database on the old shape.
  //
  // IDENTITY MODEL
  //   repo_id / worktree_id are Git-derived and STABLE: they do not move when the branch is
  //   committed to, rebased, renamed, detached or switched. `worktree` (legacy) and
  //   worktree_name are display strings and are never join keys — two repos may both contain
  //   a worktree called `task-review`.
  //
  //   provider_session_id is the harness conversation id (Claude Code's hook `session_id`),
  //   which is STABLE ACROSS `--resume`. The row's `id` is the RUNTIME ACTIVATION id and is
  //   unique per live activation. The unique index covers only rows that exist, and release
  //   deletes rows, so one conversation may legitimately have many activations over time but
  //   never two at once.
  //
  //   There is deliberately NO `state` column. Release keeps the pre-existing DELETE
  //   semantics, so a row's EXISTENCE is the lifecycle state and a released session cannot
  //   retain any authority by construction. History lives in ops/state/session-log.jsonl.
  const v3 = db
    .prepare("SELECT COUNT(*) as count FROM schema_version WHERE version = 3")
    .get() as { count: number };
  if (v3.count === 0) {
    const existing = new Set(
      (db.prepare("PRAGMA table_info(sessions)").all() as Array<{ name: string }>).map(
        (c) => c.name
      )
    );
    const columns: Array<[string, string]> = [
      // ── authoritative stable identity ──
      ["repo_id", "TEXT"],
      ["worktree_id", "TEXT"],
      ["worktree_path", "TEXT"],
      ["worktree_name", "TEXT"],
      ["provider_session_id", "TEXT"],
      // ── advisory launch metadata (historical facts, never current state) ──
      ["branch_at_registration", "TEXT"],
      ["head_at_registration", "TEXT"],
      ["task_ref", "TEXT"],
      ["pr_ref", "TEXT"],
      ["parent_session_id", "TEXT"],
      ["provider", "TEXT"],
      ["transcript_path", "TEXT"],
      // ── correlation only (PIDs are reusable; never identity) ──
      ["agent_pid", "INTEGER"],
      ["host", "TEXT"],
      // ── liveness / progress ──
      ["current_activity", "TEXT"],
      ["last_progress", "TEXT"],
      ["progress_count", "INTEGER NOT NULL DEFAULT 0"],
    ];
    for (const [name, decl] of columns) {
      if (!existing.has(name)) {
        db.exec(`ALTER TABLE sessions ADD COLUMN ${name} ${decl}`);
      }
    }
    db.exec(`
      -- One LIVE activation per harness conversation. Not unique over time: a conversation
      -- that is released and later resumed gets a new activation row, which is correct.
      CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_provider_session
        ON sessions(provider_session_id) WHERE provider_session_id IS NOT NULL;
      -- Deliberately NOT unique: several sessions may legitimately observe one lane, and
      -- concurrent occupancy is something the runtime must be able to report, not forbid.
      CREATE INDEX IF NOT EXISTS idx_sessions_worktree_id ON sessions(worktree_id);
      CREATE INDEX IF NOT EXISTS idx_sessions_parent ON sessions(parent_session_id);
    `);
    db.prepare("INSERT INTO schema_version (version) VALUES (3)").run();
  }
}
