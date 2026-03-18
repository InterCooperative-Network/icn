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

  migrate(db);
  return db;
}

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
}
