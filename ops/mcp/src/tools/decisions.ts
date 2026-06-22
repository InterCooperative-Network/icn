import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "fs";
import { join } from "path";
import { resolveMonorepoRoot } from "../paths.js";

// ADRs are canonical under `docs/adr/` at the monorepo root. Operational
// state previously held a parallel copy under `ops/state/decisions/`; that
// caused source-of-truth drift and has been retired (see
// `ops/state/decisions/README.md`).
const DECISIONS_DIR = join(resolveMonorepoRoot(), "docs", "adr");

// Match both legacy `NNNN-slug.md` and canonical `ADR-NNNN-slug.md`.
const ADR_FILENAME_RE = /^(?:ADR-)?(\d{4})-.*\.md$/;

/**
 * Extended ADR metadata extracted from the file body.
 *
 * The repository carries ADRs in three metadata styles, all valid:
 *
 *   1. YAML frontmatter (preferred for new ADRs; ADR-0012, 0013, 0014, …):
 *
 *        ---
 *        id: "0014"
 *        title: "..."
 *        status: "proposed"
 *        ...
 *        ---
 *
 *   2. Classic markdown bold labels (ADR-0001..0009, 0011, 0015, 0016):
 *
 *        # ADR-0001: Title
 *        **Date**: 2026-02-19
 *        **Status**: accepted
 *        **Tags**: ...
 *
 *   3. Bullet metadata (ADR-0010):
 *
 *        # ADR-0010: Title
 *        - Status: Proposed
 *        - Date: 2026-02-09
 *
 * The indexer recognizes all three so historical ADRs index correctly
 * without rewriting their bodies.
 */
export interface AdrMetadata {
  title: string;
  tags: string;
  date: string | null;
  status: string | null;
  supersedes: string[];
  superseded_by: string[];
  amends: string[];
  implementation_status: string | null;
}

function emptyMetadata(): AdrMetadata {
  return {
    title: "",
    tags: "",
    date: null,
    status: null,
    supersedes: [],
    superseded_by: [],
    amends: [],
    implementation_status: null,
  };
}

function splitArray(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value
      .map((v) => String(v).trim())
      .filter((v) => v.length > 0 && v !== "[]");
  }
  if (typeof value === "string") {
    const trimmed = value.trim();
    if (trimmed === "" || trimmed === "[]") return [];
    // Allow `["ADR-0001", "ADR-0002"]` or `ADR-0001, ADR-0002`.
    const stripped = trimmed.replace(/^\[|\]$/g, "");
    return stripped
      .split(",")
      .map((s) => s.trim().replace(/^["']|["']$/g, ""))
      .filter((s) => s.length > 0);
  }
  return [];
}

/**
 * Parse a YAML-frontmatter block. Intentionally minimal: handles top-level
 * scalar fields (`key: value`) and list fields written as either
 *
 *   key: ["a", "b"]
 *   key: []
 *
 * or
 *
 *   key:
 *     - "a"
 *     - "b"
 *
 * Quoted strings have surrounding quotes stripped. Unknown fields are
 * returned as plain strings in the `extra` map for downstream callers.
 */
function parseFrontmatter(block: string): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  const lines = block.split(/\r?\n/);
  let currentListKey: string | null = null;
  for (const raw of lines) {
    const line = raw.replace(/\s+$/, "");
    if (line === "") {
      currentListKey = null;
      continue;
    }
    // List item under a key written in block style.
    if (currentListKey && /^\s*-\s*/.test(line)) {
      const item = line
        .replace(/^\s*-\s*/, "")
        .trim()
        .replace(/^["']|["']$/g, "");
      const existing = (out[currentListKey] as string[] | undefined) ?? [];
      existing.push(item);
      out[currentListKey] = existing;
      continue;
    }
    const m = line.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.*)$/);
    if (!m) {
      currentListKey = null;
      continue;
    }
    const key = m[1];
    const rest = m[2].trim();
    if (rest === "") {
      // Possible block-list ahead.
      currentListKey = key;
      out[key] = [];
      continue;
    }
    currentListKey = null;
    if (rest.startsWith("[") && rest.endsWith("]")) {
      out[key] = splitArray(rest);
    } else {
      out[key] = rest.replace(/^["']|["']$/g, "");
    }
  }
  return out;
}

/**
 * Extract ADR metadata from a file body, recognizing YAML frontmatter,
 * classic `**Field**:` markdown, and bullet `- Field:` lists. The first
 * recognized form wins for any given field; bullet/classic forms can fill
 * in fields that frontmatter omitted.
 */
export function parseAdrMetadata(content: string): AdrMetadata {
  const out = emptyMetadata();

  // 1) YAML frontmatter (highest priority, if present).
  const fmMatch = content.match(/^---\r?\n([\s\S]*?)\r?\n---\s*\r?\n/);
  if (fmMatch) {
    const fm = parseFrontmatter(fmMatch[1]);
    if (typeof fm.title === "string") out.title = fm.title;
    if (typeof fm.status === "string") out.status = fm.status.toLowerCase();
    if (typeof fm.date === "string") out.date = fm.date;
    if (typeof fm.tags !== "undefined") {
      const tags = splitArray(fm.tags);
      if (tags.length > 0) out.tags = tags.join(", ");
    }
    if (typeof fm.implementation_status === "string") {
      out.implementation_status = fm.implementation_status;
    }
    out.supersedes = splitArray(fm.supersedes);
    out.superseded_by = splitArray(fm.superseded_by);
    out.amends = splitArray(fm.amends);
  }

  // 2) Title from first `# ADR-NNNN: Title` line if frontmatter omitted it.
  if (!out.title) {
    const titleMatch = content.match(/^#\s+ADR[-\s]?\d+:\s+(.+)$/m);
    if (titleMatch) out.title = titleMatch[1].trim();
  }

  // 3) Classic `**Field**: value` markdown (multiple ADRs use this style).
  if (!out.date) {
    const m = content.match(/\*\*Date\*\*\s*:\s*(\S+)/);
    if (m) out.date = m[1].trim();
  }
  if (!out.status) {
    const m = content.match(/\*\*Status\*\*\s*:\s*([^\n*]+)/);
    if (m) out.status = m[1].trim().toLowerCase();
  }
  if (!out.tags) {
    const m = content.match(/\*\*Tags\*\*\s*:\s*([^\n]+)/);
    if (m && m[1].trim() !== "—") out.tags = m[1].trim();
  }
  if (out.supersedes.length === 0) {
    const m = content.match(/\*\*Supersedes\*\*\s*:\s*([^\n]+)/);
    if (m && m[1].trim() !== "N/A" && m[1].trim() !== "—") {
      out.supersedes = splitArray(m[1]);
    }
  }
  if (out.superseded_by.length === 0) {
    const m = content.match(/\*\*Superseded by\*\*\s*:\s*([^\n]+)/i);
    if (m && m[1].trim() !== "N/A" && m[1].trim() !== "—") {
      out.superseded_by = splitArray(m[1]);
    }
  }
  if (out.amends.length === 0) {
    const m = content.match(/\*\*Amend(?:ed|s) by\*\*\s*:\s*([^\n]+)/i);
    if (m && m[1].trim() !== "N/A" && m[1].trim() !== "—") {
      out.amends = splitArray(m[1]);
    }
  }

  // 4) Bullet metadata (ADR-0010 style).
  if (!out.status) {
    const m = content.match(/^[-*]\s*Status\s*:\s*([^\n]+)/m);
    if (m) out.status = m[1].trim().toLowerCase();
  }
  if (!out.date) {
    const m = content.match(/^[-*]\s*Date\s*:\s*(\S+)/m);
    if (m) out.date = m[1].trim();
  }
  if (out.superseded_by.length === 0) {
    const m = content.match(/^[-*]\s*Superseded by\s*:\s*([^\n]+)/im);
    if (m) out.superseded_by = splitArray(m[1]);
  }

  // Tags fallback: if still empty, leave empty string (vs. dash).
  if (!out.tags) out.tags = "";

  return out;
}

// Sync-on-boot: index any ADR files that exist on disk but aren't in SQLite.
// Handles ADRs written manually or before the MCP server existed.
export function syncDecisionIndex(db: Database.Database): void {
  if (!existsSync(DECISIONS_DIR)) return;

  const files = readdirSync(DECISIONS_DIR).filter((f) => ADR_FILENAME_RE.test(f));
  // Use INSERT OR REPLACE so existing rows whose `file_path` still points
  // to the retired `ops/state/decisions/` location are re-pointed to the
  // canonical `docs/adr/` path on the next boot. INSERT OR IGNORE would
  // leave stale paths in place, breaking `get_decision` after the move.
  const upsert = db.prepare(
    "INSERT OR REPLACE INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
  );

  for (const file of files) {
    const m = file.match(ADR_FILENAME_RE);
    if (!m) continue;
    const id = m[1];
    const filePath = join(DECISIONS_DIR, file);
    const content = readFileSync(filePath, "utf-8");
    const meta = parseAdrMetadata(content);

    // Title fallback: derive from filename if we still don't have one.
    const title =
      meta.title ||
      file.replace(/^(?:ADR-)?\d{4}-/, "").replace(/\.md$/, "");

    upsert.run(id, title, meta.tags, filePath, meta.date);
  }
}

export function nextAdrNumber(dir: string): string {
  const files = readdirSync(dir)
    .filter((f) => ADR_FILENAME_RE.test(f))
    .map((f) => parseInt(f.match(ADR_FILENAME_RE)![1], 10))
    .sort((a, b) => a - b);
  if (files.length === 0) return "0001";
  return (files[files.length - 1] + 1).toString().padStart(4, "0");
}

/**
 * Run the decision-index search. The query is split into whitespace-delimited
 * terms and ALL must match (title OR tags). An empty or whitespace-only query
 * has no terms — return the full index ordered by id rather than building an
 * invalid `WHERE` clause, which SQLite rejects with a syntax error.
 */
export function searchDecisionRows(
  db: Database.Database,
  query: string
): unknown[] {
  const terms = query.split(/\s+/).filter(Boolean);
  if (terms.length === 0) {
    return db
      .prepare(
        `SELECT id, title, tags, file_path, created_at
         FROM decision_index
         ORDER BY id DESC`
      )
      .all();
  }
  // e.g. "mcp registration" finds ADRs containing both "mcp" and "registration".
  const conditions = terms
    .map(() => "(title LIKE ? OR tags LIKE ?)")
    .join(" AND ");
  const params = terms.flatMap((t) => [`%${t}%`, `%${t}%`]);
  return db
    .prepare(
      `SELECT id, title, tags, file_path, created_at
       FROM decision_index
       WHERE ${conditions}
       ORDER BY id DESC`
    )
    .all(...params);
}

export function registerDecisionTools(
  server: McpServer,
  db: Database.Database
): void {
  server.tool(
    "log_decision",
    "Write a new Architecture Decision Record (ADR) to docs/adr/ and index it.",
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
        .enum(["proposed", "accepted", "amended", "superseded", "deprecated"])
        .optional()
        .default("accepted"),
    },
    async ({ title, context, decision, consequences, alternatives, tags, status }) => {
      const num = nextAdrNumber(DECISIONS_DIR);
      const slug = title
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-|-$/g, "");
      const filename = `ADR-${num}-${slug}.md`;
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
      const rows = searchDecisionRows(db, query);
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
      // Try exact ID first. Treat a row whose `file_path` no longer exists
      // (e.g. a pre-migration row pointing into ops/state/decisions/) as
      // "not found" so the filesystem fallback below has a chance to
      // resolve the real file under docs/adr/.
      let row = db
        .prepare("SELECT file_path FROM decision_index WHERE id = ?")
        .get(id) as { file_path: string } | undefined;
      if (row && !existsSync(row.file_path)) row = undefined;

      // Fall back to partial title match — same staleness check.
      if (!row) {
        row = db
          .prepare(
            "SELECT file_path FROM decision_index WHERE title LIKE ? ORDER BY id DESC LIMIT 1"
          )
          .get(`%${id}%`) as { file_path: string } | undefined;
        if (row && !existsSync(row.file_path)) row = undefined;
      }

      // Final fallback: scan filesystem for a file matching the ID prefix.
      // Accept both legacy `NNNN-slug.md` and canonical `ADR-NNNN-slug.md`.
      if (!row) {
        const padded = id.padStart(4, "0");
        const files = existsSync(DECISIONS_DIR)
          ? readdirSync(DECISIONS_DIR).filter(
              (f) => f.startsWith(padded) || f.startsWith(`ADR-${padded}`)
            )
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
      const meta = parseAdrMetadata(content);

      // Surface the lifecycle metadata at the top of the response so
      // callers see status / supersession / implementation status without
      // having to re-parse the body.
      const header = [
        `# Lifecycle metadata`,
        `- status: ${meta.status ?? "—"}`,
        `- date: ${meta.date ?? "—"}`,
        `- tags: ${meta.tags || "—"}`,
        meta.supersedes.length > 0
          ? `- supersedes: ${meta.supersedes.join(", ")}`
          : null,
        meta.superseded_by.length > 0
          ? `- superseded_by: ${meta.superseded_by.join(", ")}`
          : null,
        meta.amends.length > 0
          ? `- amends: ${meta.amends.join(", ")}`
          : null,
        meta.implementation_status
          ? `- implementation_status: ${meta.implementation_status}`
          : null,
      ]
        .filter((s): s is string => s !== null)
        .join("\n");

      return {
        content: [{ type: "text", text: `${header}\n\n---\n\n${content}` }],
      };
    }
  );
}
