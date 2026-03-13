// Event engine — central event emission and query for the HUD system.
// Events are the shared nervous system: tools emit them, agents poll them.

import type Database from "better-sqlite3";

const MAX_EVENTS = 1000;

export interface Event {
  id: number;
  scope: string;
  type: string;
  payload: unknown;
  created_at: number;
}

/** Emit an event into the log. Auto-prunes to MAX_EVENTS. */
export function emitEvent(
  db: Database.Database,
  scope: string,
  type: string,
  payload: unknown
): number {
  const result = db
    .prepare(
      "INSERT INTO events (scope, type, payload, created_at) VALUES (?, ?, ?, ?)"
    )
    .run(scope, type, JSON.stringify(payload), Date.now());

  // Prune old events (keep last MAX_EVENTS)
  db.prepare(
    `DELETE FROM events WHERE id <= (
      SELECT id FROM events ORDER BY id DESC LIMIT 1 OFFSET ?
    )`
  ).run(MAX_EVENTS);

  return Number(result.lastInsertRowid);
}

/** Query events, optionally filtered by scope, type, and cursor. */
export function queryEvents(
  db: Database.Database,
  opts: {
    scope?: string;
    types?: string[];
    since_id?: number;
    since_ts?: number;
    limit?: number;
  }
): Event[] {
  const conditions: string[] = [];
  const params: unknown[] = [];

  if (opts.scope) {
    conditions.push("scope = ?");
    params.push(opts.scope);
  }
  if (opts.types && opts.types.length > 0) {
    conditions.push(`type IN (${opts.types.map(() => "?").join(",")})`);
    params.push(...opts.types);
  }
  if (opts.since_id) {
    conditions.push("id > ?");
    params.push(opts.since_id);
  }
  if (opts.since_ts) {
    conditions.push("created_at > ?");
    params.push(opts.since_ts);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(" AND ")}` : "";
  const limit = opts.limit ?? 50;

  const rows = db
    .prepare(`SELECT * FROM events ${where} ORDER BY id DESC LIMIT ?`)
    .all(...params, limit) as Array<{ id: number; scope: string; type: string; payload: string; created_at: number }>;

  return rows.map((r) => ({
    ...r,
    payload: JSON.parse(r.payload) as unknown,
  }));
}
