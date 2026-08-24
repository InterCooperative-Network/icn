import Database from "better-sqlite3";
import { execFileSync } from "child_process";

/**
 * Highest version the ladder in `migrate()` stamps.
 *
 * `initDb` uses this to skip the write transaction when there is nothing to apply, so a stale
 * value here would mean a NEW migration never runs. That failure would be silent, so it is
 * pinned by a test that runs the real ladder on a fresh database and asserts the highest
 * stamped version equals this constant — bump both together or the test fails.
 */
export const LATEST_SCHEMA_VERSION = 5;

/**
 * Environment for child `git` calls with the repo-selecting variables REMOVED.
 *
 * `GIT_DIR` overrides `-C`, and git EXPORTS it into hooks when running in a linked worktree —
 * which is the only kind ICN uses. Inherited, it made two different lanes resolve to the SAME
 * worktree_id and pointed the registry at an unrelated repository. This is the same lesson as
 * ICN_ROOT, one layer down: an unchecked environment variable must never be able to
 * misattribute a session to the wrong lane.
 */
const GIT_SANITISED_ENV: NodeJS.ProcessEnv = (() => {
  const e = { ...process.env };
  for (const k of [
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
  ]) {
    delete e[k];
  }
  return e;
})();
import { existsSync, mkdirSync, realpathSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
/** Pre-existing location: beside whichever copy of this JS is executing. */
const LEGACY_DB_PATH = join(__dirname, "../../data/icn-ops.db");

/**
 * The session registry must be ONE database per repository, not one per worktree.
 *
 * The old default resolved beside the executing JS. That was harmless while only the MCP server
 * used it, but this runtime added a second writer — the hook CLI, which runs out of the agent's
 * OWN worktree — while the MCP server is pinned by ~/.claude.json to an absolute path in the
 * mcp-host worktree. The result observed on this VM: two live databases with different schemas,
 * a hook-registered session invisible to every mcp__icn-ops__* tool, and file_claims failing a
 * foreign-key check because the session id existed only in the other file.
 *
 * Git already names the thing shared by every worktree of a repo: --git-common-dir. Deriving
 * from it is consistent with how this runtime identifies lanes (worktree-identity.ts), needs no
 * machine-specific path, and works for bare stores and plain clones alike.
 */
export function resolveDefaultDbPath(from: string = __dirname): string {
  try {
    const common = execFileSync("git", ["-C", from, "rev-parse", "--git-common-dir"], {
      encoding: "utf-8",
      stdio: ["ignore", "pipe", "ignore"],
      env: GIT_SANITISED_ENV,
    }).trim();
    if (common) {
      const abs = common.startsWith("/") ? common : join(from, common);
      return join(realpathSync(abs), "icn-ops.db");
    }
  } catch {
    // Not inside a repo, or git unavailable — fall through.
  }
  // Falling back means going BACK to a per-worktree database, which is exactly the split this
  // function exists to close. It must never happen quietly.
  process.stderr.write(
    "icn-ops: could not derive the shared registry from git; falling back to the per-worktree " +
      `path ${LEGACY_DB_PATH}. Sessions registered here will NOT be visible to other worktrees ` +
      "or to the MCP server. Set ICN_OPS_DB to pin the registry explicitly.\n"
  );
  return LEGACY_DB_PATH;
}

/**
 * Which database file a caller gets: explicit argument, then `ICN_OPS_DB`, then the
 * git-derived shared default.
 *
 * Split out of `initDb` so it can be TESTED without opening anything — asserting this against
 * the real environment must never risk writing to the live ICN registry.
 *
 * An EMPTY `ICN_OPS_DB` is not a path: better-sqlite3 opens "" as an anonymous temporary
 * database, so `register` reported a session id into storage that vanished and the very next
 * `status` said not-registered. Whitespace behaves identically. Both are treated as unset.
 */
export function resolveDbPath(dbPath?: string): string {
  const envPath = process.env["ICN_OPS_DB"];
  return dbPath ?? (envPath && envPath.trim() !== "" ? envPath : resolveDefaultDbPath());
}

export function initDb(dbPath?: string): Database.Database {
  const path = resolveDbPath(dbPath);

  // Ensure data directory exists
  const dir = dirname(path);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }

  const db = new Database(path);

  // ORDER MATTERS — and `journal_mode` is the exception that order cannot fix. busy_timeout
  // goes FIRST so it covers every later statement, but SQLite does not run the busy handler
  // for a journal-mode change at all: under a concurrent writer this line raised SQLITE_BUSY
  // in ~27 ms wherever it sat (measured; with busy_timeout moved above it, still 4 ms). The
  // blast radius was not cosmetic — the CLI turned it into "registry unavailable", exit 1,
  // session never registered, and the MCP server's main().catch(exit 1) into "the server does
  // not start".
  db.pragma("busy_timeout = 5000");
  try {
    // WAL for concurrent reads across agent sessions. Best-effort ON PURPOSE: journal mode is
    // a PERSISTENT property of the database file, so once any process has set it, it stays
    // set, and the next uncontended open converts a file that is still in the old mode.
    // Losing the whole process over a pragma another writer happens to be blocking is
    // strictly the worse outcome.
    db.pragma("journal_mode = WAL");
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code !== "SQLITE_BUSY") throw e;
  }
  db.pragma("foreign_keys = ON");

  // ONLY take the write lock when the ladder actually has work to do, and when it does, run the
  // WHOLE ladder in ONE immediate transaction.
  //
  // The transaction came first: each step used to be a bare PRAGMA -> ALTER -> INSERT with no
  // lock, so concurrent opens raced and 9 of 40 parallel initDb calls on a fresh database threw
  // `duplicate column name` — which the MCP server's main().catch(exit 1) turns into "the server
  // does not start".
  //
  // The GUARD came second, and matters just as much: the transaction ran on EVERY open, so a
  // read-only `classify` took an EXCLUSIVE write lock on a database several MCP processes share.
  // That is why "registry unavailable" was reachable routinely rather than only on a corrupt
  // file. Measured on an already-migrated database while another process held BEGIN IMMEDIATE:
  // this path now opens in 7 ms where it previously failed outright, and a genuine cold start
  // still waits out a transient lock (1676 ms against a 2 s holder) instead of dying in 29 ms.
  if (migrationNeeded(db)) {
    db.transaction(() => migrate(db)).immediate();
  }
  return db;
}

/**
 * Whether the schema ladder still has work. Read-only and lock-free by design: a brand-new
 * file has no `schema_version` table at all, and ANY unreadable state answers YES so the
 * guarded ladder still runs and stays the single source of truth. This is an optimisation in
 * front of `migrate()`, never a replacement for it.
 */
function migrationNeeded(db: Database.Database): boolean {
  try {
    const row = db
      .prepare("SELECT COUNT(*) AS count FROM schema_version WHERE version = ?")
      .get(LATEST_SCHEMA_VERSION) as { count: number };
    return row.count === 0;
  } catch {
    return true;
  }
}

/** SQL DDL batches on the SQLite connection (better-sqlite3 API). */
export function migrate(db: Database.Database): void {
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

  // Schema v4: adds watchers_process.worktree_id.
  //
  // RESERVED. Its consumer (supervision of long-running operations) was removed from this PR
  // and lives on ops/agent-supervision-lifecycle. The column is retained rather than reverted
  // because it is already stamped in the live shared registry, an unused column is inert, and
  // re-editing a published migration is exactly what schema-upgrade.test.ts pins against.
  //
  // A NEW migration rather than an edit to v3. The only v3-stamped databases found on this
  // machine were throwaway test artifacts, not production state — an earlier version of this
  // comment overstated that as "demonstrably exists", which review correctly challenged. The
  // reason stands on its own without it: v3 has been published on this branch, a stamped
  // version is skipped wholesale, and nothing can prove no clone has run it. Adding a number
  // is free; an in-place edit is unrecoverable. Pinned by schema-upgrade.test.ts.
  //
  // WHY IT MATTERS: watchers_process had no lane of its own, so supervisions were found via the
  // owning session row. When a resumed conversation re-registered from a different worktree the
  // row moved, and the supervision moved with it — leaving the lane where the build was ACTUALLY
  // RUNNING with no protection. Observed: a live `cargo test --workspace` in lane A while lane A
  // classified retireable:true.
  //
  // The column doubles as a discriminator. `watch_process` (a pre-existing, unrelated feature)
  // inserts into this same table with worktree_id NULL, so lane-scoped supervision queries no
  // longer reap those rows out from under the background poller that owns them.
  const v4 = db
    .prepare("SELECT COUNT(*) as count FROM schema_version WHERE version = 4")
    .get() as { count: number };
  if (v4.count === 0) {
    const cols = new Set(
      (db.prepare("PRAGMA table_info(watchers_process)").all() as Array<{ name: string }>).map(
        (c) => c.name
      )
    );
    if (!cols.has("worktree_id")) {
      db.exec("ALTER TABLE watchers_process ADD COLUMN worktree_id TEXT");
    }
    db.exec(
      "CREATE INDEX IF NOT EXISTS idx_watchers_worktree ON watchers_process(worktree_id, status)"
    );
    db.prepare("INSERT INTO schema_version (version) VALUES (4)").run();
  }

  // Schema v5: sessions.worktree_generation — WHICH GENERATION of a lane a row belongs to.
  //
  // worktree_id identifies a lane in space, not in time. Git recycles
  // `<repo>/.git/worktrees/<basename>` after `git worktree remove`, and a worktree can be
  // recreated at the EXACT SAME pathname — repo, admin dir and recorded path all match, so a
  // brand-new generation silently adopted the previous one's unreleased rows. Verified: a fresh
  // `gen2` worktree classified REGISTERED-ACTIVE holding `gen1`'s session row. Comparing paths
  // cannot see this; only a token minted per generation can. See laneGeneration().
  //
  // A NEW migration, not an edit to v3/v4, for the reason v4's comment already gives: a stamped
  // version is skipped wholesale, so editing a published one silently leaves existing databases
  // on the old shape.
  //
  // Deliberately NOT backfilled. There is no way to know which generation an existing row was
  // written under, and inventing one would be worse than admitting ignorance: a NULL reads as
  // "unknown", and the filter keeps every row it cannot judge. Pre-upgrade rows therefore stay
  // protected exactly as they are today, and the lane self-heals as sessions re-register.
  const v5 = db
    .prepare("SELECT COUNT(*) as count FROM schema_version WHERE version = 5")
    .get() as { count: number };
  if (v5.count === 0) {
    const cols = new Set(
      (db.prepare("PRAGMA table_info(sessions)").all() as Array<{ name: string }>).map(
        (c) => c.name
      )
    );
    if (!cols.has("worktree_generation")) {
      db.exec("ALTER TABLE sessions ADD COLUMN worktree_generation TEXT");
    }
    db.prepare("INSERT INTO schema_version (version) VALUES (5)").run();
  }
}
