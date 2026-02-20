import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "fs";
import { join } from "path";

const ICN_ROOT = process.env["ICN_ROOT"] ?? "/home/ubuntu/projects";
const DECISIONS_DIR = join(ICN_ROOT, "icn-ops/state/decisions");

// Sync-on-boot: index any ADR files that exist on disk but aren't in SQLite.
// Handles ADRs written manually or before the MCP server existed.
export function syncDecisionIndex(db: Database.Database): void {
  if (!existsSync(DECISIONS_DIR)) return;

  const files = readdirSync(DECISIONS_DIR).filter((f) => /^\d{4}-.*\.md$/.test(f));
  const insert = db.prepare(
    "INSERT OR IGNORE INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
  );

  for (const file of files) {
    const id = file.substring(0, 4);
    const filePath = join(DECISIONS_DIR, file);
    const content = readFileSync(filePath, "utf-8");

    // Extract title from first heading: "# ADR-NNNN: Title"
    const titleMatch = content.match(/^#\s+ADR-\d+:\s+(.+)$/m);
    const title = titleMatch ? titleMatch[1].trim() : file.replace(/^\d{4}-/, "").replace(/\.md$/, "");

    // Extract tags from "**Tags**: tag1, tag2"
    const tagsMatch = content.match(/\*\*Tags\*\*:\s+(.+)$/m);
    const tags = tagsMatch && tagsMatch[1].trim() !== "—" ? tagsMatch[1].trim() : "";

    // Extract date from "**Date**: YYYY-MM-DD"
    const dateMatch = content.match(/\*\*Date\*\*:\s+(\S+)/);
    const date = dateMatch ? dateMatch[1] : null;

    insert.run(id, title, tags, filePath, date);
  }
}

function nextAdrNumber(dir: string): string {
  const files = readdirSync(dir)
    .filter((f) => /^\d{4}-/.test(f))
    .sort();
  if (files.length === 0) return "0001";
  const last = files[files.length - 1];
  const num = parseInt(last.substring(0, 4)) + 1;
  return num.toString().padStart(4, "0");
}

export function registerDecisionTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "log_decision",
    "Write a new Architecture Decision Record (ADR) to state/decisions/ and index it.",
    {
      title: z.string().describe("Short title for the decision"),
      context: z.string().describe("What's the situation? What problem are we solving?"),
      decision: z.string().describe("What did we decide?"),
      consequences: z.string().describe("Trade-offs, what becomes easier/harder"),
      alternatives: z
        .string()
        .optional()
        .describe("Alternatives considered and why rejected"),
      tags: z
        .array(z.string())
        .optional()
        .describe("Tags: kernel, networking, deployment, orchestration, etc."),
      status: z
        .enum(["proposed", "accepted", "superseded", "deprecated"])
        .optional()
        .default("accepted"),
    },
    async ({ title, context, decision, consequences, alternatives, tags, status }) => {
      const num = nextAdrNumber(DECISIONS_DIR);
      const slug = title
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-|-$/g, "");
      const filename = `${num}-${slug}.md`;
      const filePath = join(DECISIONS_DIR, filename);
      const date = new Date().toISOString().split("T")[0];

      const content = `# ADR-${num}: ${title}

**Date**: ${date}
**Status**: ${status}
**Tags**: ${(tags ?? []).join(", ") || "—"}

## Context

${context}

## Decision

${decision}

## Consequences

${consequences}
${
  alternatives
    ? `
## Alternatives Considered

${alternatives}
`
    : ""
}`;

      writeFileSync(filePath, content);

      db.prepare(
        "INSERT OR REPLACE INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
      ).run(num, title, (tags ?? []).join(","), filePath, date);

      return {
        content: [
          {
            type: "text",
            text: `ADR-${num} written to ${filename}`,
          },
        ],
      };
    }
  );

  server.tool(
    "search_decisions",
    "Full-text search over ADR titles and tags.",
    {
      query: z.string().describe("Search term"),
    },
    async ({ query }) => {
      // Split into individual terms and require ALL to match (AND logic)
      // e.g. "mcp registration" finds ADRs containing both "mcp" and "registration"
      const terms = query.split(/\s+/).filter(Boolean);
      const conditions = terms
        .map(() => "(title LIKE ? OR tags LIKE ?)")
        .join(" AND ");
      const params = terms.flatMap((t) => [`%${t}%`, `%${t}%`]);

      const rows = db
        .prepare(
          `SELECT id, title, tags, file_path, created_at
           FROM decision_index
           WHERE ${conditions}
           ORDER BY id DESC`
        )
        .all(...params);
      return {
        content: [{ type: "text", text: JSON.stringify(rows, null, 2) }],
      };
    }
  );

  server.tool(
    "get_decision",
    "Read the full content of an ADR by number or title search.",
    {
      id: z.string().describe("ADR number (e.g. '0001') or partial title"),
    },
    async ({ id }) => {
      // Try exact ID first
      let row = db
        .prepare("SELECT file_path FROM decision_index WHERE id = ?")
        .get(id) as { file_path: string } | undefined;

      // Fall back to partial title match
      if (!row) {
        row = db
          .prepare(
            "SELECT file_path FROM decision_index WHERE title LIKE ? ORDER BY id DESC LIMIT 1"
          )
          .get(`%${id}%`) as { file_path: string } | undefined;
      }

      // Final fallback: scan filesystem for a file matching the ID prefix
      if (!row) {
        const files = existsSync(DECISIONS_DIR)
          ? readdirSync(DECISIONS_DIR).filter((f) => f.startsWith(id.padStart(4, "0")))
          : [];
        if (files.length > 0) {
          row = { file_path: join(DECISIONS_DIR, files[0]) };
        }
      }

      if (!row || !existsSync(row.file_path)) {
        return {
          content: [{ type: "text", text: `ADR not found: ${id}` }],
          isError: true,
        };
      }

      const content = readFileSync(row.file_path, "utf-8");
      return {
        content: [{ type: "text", text: content }],
      };
    }
  );
}
