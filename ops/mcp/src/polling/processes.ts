// Background process watcher — checks registered PIDs every 10s.
// When a PID exits, marks the watcher completed and emits an event + mailbox alert.

import type Database from "better-sqlite3";
import { emitEvent } from "../state/events.js";
// ONE definition of liveness. This module had its own, which treated EPERM (process exists but
// belongs to another uid) as DEAD — the opposite of the runtime's. Since this poller runs every
// 10s over status='running', a watched process owned by a different uid was marked completed
// within 10s and the agent got a mailbox alert saying its process had finished when it had not.
import { pidAlive } from "../runtime/session-runtime.js";

const INTERVAL_MS = 10_000;

interface Watcher {
  id: number;
  session_id: string;
  pid: number;
  label: string;
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
