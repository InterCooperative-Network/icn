// Background process watcher — checks registered PIDs every 10s.
// When a PID exits, marks the watcher completed and emits an event + mailbox alert.

import type Database from "better-sqlite3";
import { emitEvent } from "../state/events.js";

const INTERVAL_MS = 10_000;

interface Watcher {
  id: number;
  session_id: string;
  pid: number;
  label: string;
}

function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function pollOnce(db: Database.Database): void {
  const running = db
    .prepare("SELECT * FROM watchers_process WHERE status = 'running'")
    .all() as Watcher[];

  for (const w of running) {
    if (!pidAlive(w.pid)) {
      const now = Date.now();

      // Mark watcher as completed (we can't get exit code via kill(0))
      db.prepare(
        "UPDATE watchers_process SET status = 'completed', completed_at = ? WHERE id = ?"
      ).run(now, w.id);

      // Emit event
      emitEvent(db, `session:${w.session_id}`, "process.completed", {
        pid: w.pid,
        label: w.label,
        watcher_id: w.id,
      });

      // Also emit globally so any agent can see it
      emitEvent(db, "global", "process.completed", {
        pid: w.pid,
        label: w.label,
        session_id: w.session_id,
      });

      // Drop a mailbox message for the owning session
      db.prepare(
        "INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES (?, 'system', 'alert', ?, ?)"
      ).run(
        w.session_id,
        JSON.stringify({
          type: "process.completed",
          pid: w.pid,
          label: w.label,
        }),
        now
      );
    }
  }
}

export function startProcessPolling(db: Database.Database): NodeJS.Timeout {
  return setInterval(() => pollOnce(db), INTERVAL_MS);
}
