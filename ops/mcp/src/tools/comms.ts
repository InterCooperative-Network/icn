// Agent communication tools — mailbox for agent-to-agent and human-to-agent messaging.

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { emitEvent } from "../state/events.js";

export function registerCommsTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "send_message",
    "Send a message to another agent's mailbox, or broadcast to all active sessions.",
    {
      to_session: z
        .string()
        .describe("Target session ID, or 'broadcast' for all active sessions"),
      from_session: z.string().optional().describe("Sender session ID"),
      kind: z
        .enum(["text", "event", "alert"])
        .optional()
        .default("text")
        .describe("Message kind"),
      payload: z
        .string()
        .describe("Message content (JSON string or plain text)"),
    },
    async ({ to_session, from_session, kind, payload }) => {
      const now = Date.now();

      if (to_session === "broadcast") {
        // Insert into all active sessions' mailboxes
        const sessions = db
          .prepare(
            `SELECT id FROM sessions
             WHERE datetime(last_heartbeat) > datetime('now', '-30 minutes')`
          )
          .all() as Array<{ id: string }>;

        const insert = db.prepare(
          "INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES (?, ?, ?, ?, ?)"
        );
        let count = 0;
        for (const s of sessions) {
          if (s.id !== from_session) {
            insert.run(s.id, from_session ?? null, kind, payload, now);
            count++;
          }
        }

        emitEvent(db, "global", "message.broadcast", {
          from: from_session,
          kind,
          recipient_count: count,
        });

        return {
          content: [
            { type: "text", text: `Broadcast sent to ${count} sessions` },
          ],
        };
      }

      // Single recipient
      db.prepare(
        "INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES (?, ?, ?, ?, ?)"
      ).run(to_session, from_session ?? null, kind, payload, now);

      emitEvent(db, `session:${to_session}`, "message.received", {
        from: from_session,
        kind,
      });

      return {
        content: [{ type: "text", text: "Message sent" }],
      };
    }
  );

  server.tool(
    "get_messages",
    "Read messages from your mailbox. Call between tasks to check for inter-agent communication.",
    {
      session_id: z.string().describe("Your session ID"),
      unread_only: z
        .boolean()
        .optional()
        .default(true)
        .describe("Only return unread messages"),
      mark_read: z
        .boolean()
        .optional()
        .default(true)
        .describe("Mark returned messages as read"),
      limit: z.number().optional().default(20),
    },
    async ({ session_id, unread_only, mark_read, limit }) => {
      const condition = unread_only ? "AND read_at IS NULL" : "";
      const messages = db
        .prepare(
          `SELECT * FROM mailbox
           WHERE to_session = ? ${condition}
           ORDER BY id DESC LIMIT ?`
        )
        .all(session_id, limit) as Array<{
          id: number;
          to_session: string;
          from_session: string | null;
          kind: string;
          payload: string;
          created_at: number;
          read_at: number | null;
        }>;

      if (mark_read && messages.length > 0) {
        const ids = messages.map((m) => m.id);
        db.prepare(
          `UPDATE mailbox SET read_at = ? WHERE id IN (${ids.map(() => "?").join(",")})`
        ).run(Date.now(), ...ids);
      }

      return {
        content: [{ type: "text", text: JSON.stringify(messages, null, 2) }],
      };
    }
  );

  server.tool(
    "ack_messages",
    "Mark specific messages as read.",
    {
      message_ids: z.array(z.number()).describe("Message IDs to acknowledge"),
    },
    async ({ message_ids }) => {
      if (message_ids.length === 0) {
        return { content: [{ type: "text", text: "No IDs provided" }] };
      }
      db.prepare(
        `UPDATE mailbox SET read_at = ? WHERE id IN (${message_ids.map(() => "?").join(",")})`
      ).run(Date.now(), ...message_ids);
      return {
        content: [
          { type: "text", text: `${message_ids.length} messages acknowledged` },
        ],
      };
    }
  );
}
