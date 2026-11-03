// A MIRROR of the claim_files handler's rule (tools/sessions.ts), runnable against a virtual
// clock. The real handler uses SQLite's real julianday('now'), so it cannot be driven through a
// simulated hour; series.ts needs this. observation-series-fixture.test.ts runs the mirror and the
// real handler side by side under the real clock and requires identical outcomes, so drift in the
// real rule fails a test instead of silently invalidating the fixture.
//
// The rule: a claim by session S on file F is REFUSED only if another session holds F and that
// holder's heartbeat is fresher than the TTL. A stale holder does not block, and its row stays,
// so two holders can coexist. Granting uses INSERT OR REPLACE, so re-claiming a path S already
// holds replaces its row (and resets claimed_at).

import type Database from "better-sqlite3";

export interface MirrorClaimResult {
  claimed: string[];
  conflicts: string[];
}

/** `nowSql` is a SQLite datetime string, or null to use SQLite's real clock. */
export function mirrorClaim(
  db: Database.Database,
  sessionId: string,
  files: string[],
  nowSql: string | null,
  ttlMinutes: number
): MirrorClaimResult {
  const check = db.prepare(
    `SELECT session_id FROM file_claims
      WHERE file_path = ? AND session_id != ?
        AND session_id IN (
          SELECT id FROM sessions
          WHERE julianday(COALESCE(?, 'now')) - julianday(last_heartbeat) < ? / 1440.0
        )`
  );
  const put = db.prepare(
    "INSERT OR REPLACE INTO file_claims (file_path, session_id, claimed_at) VALUES (?, ?, COALESCE(?, datetime('now')))"
  );
  const claimed: string[] = [];
  const conflicts: string[] = [];
  for (const f of files) {
    if (check.get(f, sessionId, nowSql, ttlMinutes)) conflicts.push(f);
    else {
      put.run(f, sessionId, nowSql);
      claimed.push(f);
    }
  }
  return { claimed, conflicts };
}
