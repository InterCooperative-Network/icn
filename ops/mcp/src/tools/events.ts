// Event tools — emit and query the event log.
// Agents call check_events at task boundaries instead of polling loops.

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { emitEvent, queryEvents } from "../state/events.js";

export function registerEventTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "emit_event",
    "Emit an event to the event log. Use for signaling state changes across agents.",
    {
      scope: z
        .string()
        .describe("Event scope: 'global', 'session:<id>', 'sprint:<id>'"),
      type: z
        .string()
        .describe("Event type: e.g. 'build.complete', 'task.updated', 'ci.failed'"),
      payload: z
        .string()
        .optional()
        .default("{}")
        .describe("JSON payload string"),
    },
    async ({ scope, type, payload }) => {
      let parsed: unknown;
      try {
        parsed = JSON.parse(payload);
      } catch {
        return {
          content: [{ type: "text", text: "Error: payload must be valid JSON" }],
          isError: true,
        };
      }
      const id = emitEvent(db, scope, type, parsed);
      return {
        content: [{ type: "text", text: JSON.stringify({ event_id: id }) }],
      };
    }
  );

  server.tool(
    "check_events",
    "Check for recent events. Call at task boundaries instead of polling loops.",
    {
      scope: z.string().optional().describe("Filter by scope (e.g. 'global')"),
      types: z
        .string()
        .optional()
        .describe("Comma-separated event types to filter"),
      since_id: z.number().optional().describe("Return events after this ID"),
      since_ts: z
        .number()
        .optional()
        .describe("Return events after this Unix ms timestamp"),
      limit: z.number().optional().default(20).describe("Max events to return"),
    },
    async ({ scope, types, since_id, since_ts, limit }) => {
      const events = queryEvents(db, {
        scope: scope ?? undefined,
        types: types ? types.split(",").map((t) => t.trim()) : undefined,
        since_id: since_id ?? undefined,
        since_ts: since_ts ?? undefined,
        limit: limit ?? 20,
      });
      return {
        content: [{ type: "text", text: JSON.stringify(events, null, 2) }],
      };
    }
  );
}
