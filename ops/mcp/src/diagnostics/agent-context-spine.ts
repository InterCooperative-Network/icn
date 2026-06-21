import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { safeJsonParse } from "./safe-json.js";

/**
 * Read-only view over the generated Agent Context Spine artifact.
 *
 * The spine (docs/reference/project-index/generated/agent-context-spine.json) is
 * a non-canonical, evidence-grounded orientation map produced by
 * scripts/generate-agent-context-spine.py. This module only READS it: it never
 * regenerates, mutates, or executes anything. It fails clearly when the artifact
 * is missing or malformed so agents fall back to the project-index docs.
 */

export const SPINE_REL =
  "docs/reference/project-index/generated/agent-context-spine.json";

const REGENERATE = "python3 scripts/generate-agent-context-spine.py --write";
const VALIDATE = "python3 scripts/check-agent-context-spine.py";

type Evidence = { source: string; kind?: string };

export type SpineNode = {
  id: string;
  type: string;
  name: string;
  subsystem?: string;
  path?: string;
  description?: string;
  status?: string;
  evidence?: Evidence[];
  [k: string]: unknown;
};

export type SpineEdge = {
  from: string;
  to: string;
  type: string;
  evidence?: { source: string };
};

type Spine = {
  schema?: string;
  status?: string;
  canonical?: boolean;
  generated?: string;
  source_commit?: string;
  regenerate?: string;
  check?: string;
  description?: string;
  node_types?: string[];
  edge_types?: string[];
  nodes: SpineNode[];
  edges: SpineEdge[];
};

export type SpineFilter = {
  node?: string;
  type?: string;
  subsystem?: string;
  path?: string;
};

export type SpineView =
  | {
      ok: false;
      artifact: string;
      error: string;
      hint: string;
      preview?: string;
    }
  | {
      ok: true;
      artifact: string;
      mode: "summary" | "node" | "filtered";
      [k: string]: unknown;
    };

function countBy<T>(items: T[], key: (t: T) => string): Record<string, number> {
  const out: Record<string, number> = {};
  for (const it of items) {
    const k = key(it);
    out[k] = (out[k] ?? 0) + 1;
  }
  return out;
}

function compact(n: SpineNode): Record<string, unknown> {
  const c: Record<string, unknown> = { id: n.id, type: n.type, name: n.name };
  if (typeof n.subsystem === "string") c.subsystem = n.subsystem;
  if (typeof n.path === "string") c.path = n.path;
  if (typeof n.status === "string") c.status = n.status;
  if (typeof n.description === "string") c.description = n.description;
  return c;
}

function loadSpine(
  repoRoot: string
): { ok: true; spine: Spine; artifact: string } | (SpineView & { ok: false }) {
  const artifact = SPINE_REL;
  const abs = join(repoRoot, SPINE_REL);
  if (!existsSync(abs)) {
    return {
      ok: false,
      artifact,
      error: `Agent context spine artifact not found at ${SPINE_REL}`,
      hint: `Generate it with: ${REGENERATE}`,
    };
  }
  let text: string;
  try {
    text = readFileSync(abs, "utf-8");
  } catch (e) {
    return {
      ok: false,
      artifact,
      error: `Could not read ${SPINE_REL}: ${e instanceof Error ? e.message : String(e)}`,
      hint: `Regenerate with: ${REGENERATE}`,
    };
  }
  const parsed = safeJsonParse(text, SPINE_REL);
  if (!parsed.ok) {
    return {
      ok: false,
      artifact,
      error: parsed.error,
      hint: `Regenerate with: ${REGENERATE}`,
      ...(parsed.preview ? { preview: parsed.preview } : {}),
    };
  }
  const value = parsed.value as Partial<Spine>;
  if (!Array.isArray(value.nodes) || !Array.isArray(value.edges)) {
    return {
      ok: false,
      artifact,
      error: `${SPINE_REL} is missing 'nodes' and/or 'edges' arrays`,
      hint: `Regenerate with: ${REGENERATE} then validate with: ${VALIDATE}`,
    };
  }
  return { ok: true, spine: value as Spine, artifact };
}

function spineMeta(spine: Spine): Record<string, unknown> {
  return {
    schema: spine.schema,
    status: spine.status,
    canonical: spine.canonical ?? false,
    generated: spine.generated,
    source_commit: spine.source_commit,
    regenerate: spine.regenerate ?? REGENERATE,
    check: spine.check ?? VALIDATE,
    note:
      "Orientation artifact, not a canonical source of truth. Structure is not " +
      "runtime liveness; it asserts no production/live/pilot readiness. Defer to " +
      "docs/reference/project-index/source-of-truth-map.md.",
  };
}

/**
 * Build a compact, read-only view of the spine.
 * - No filter -> summary (metadata + per-type counts).
 * - `node` -> exact node id (full node + incident edges) or a contains-match list.
 * - `type` / `subsystem` / `path` -> filtered compact node list.
 */
export function buildAgentContextSpineView(
  repoRoot: string,
  filter: SpineFilter = {}
): SpineView {
  const loaded = loadSpine(repoRoot);
  if (!loaded.ok) return loaded;
  const { spine, artifact } = loaded;
  const nodes = spine.nodes;
  const edges = spine.edges;

  // node lookup
  if (filter.node) {
    const q = filter.node;
    const exact = nodes.find((n) => n.id === q);
    if (exact) {
      const incident = edges
        .filter((e) => e.from === exact.id || e.to === exact.id)
        .map((e) => {
          const out = e.from === exact.id;
          const otherId = out ? e.to : e.from;
          const other = nodes.find((n) => n.id === otherId);
          return {
            direction: out ? "out" : "in",
            type: e.type,
            other: otherId,
            other_type: other?.type,
            other_name: other?.name,
            evidence: e.evidence?.source,
          };
        });
      return {
        ok: true,
        artifact,
        mode: "node",
        meta: spineMeta(spine),
        node: exact,
        incident_edges: incident,
      };
    }
    const ql = q.toLowerCase();
    const matches = nodes.filter(
      (n) =>
        n.id.toLowerCase().includes(ql) || n.name.toLowerCase().includes(ql)
    );
    return {
      ok: true,
      artifact,
      mode: "filtered",
      meta: spineMeta(spine),
      query: { node: q },
      match_count: matches.length,
      matches: matches.map(compact),
    };
  }

  // attribute filters
  if (filter.type || filter.subsystem || filter.path) {
    let matches = nodes;
    if (filter.type) matches = matches.filter((n) => n.type === filter.type);
    if (filter.subsystem)
      matches = matches.filter((n) => n.subsystem === filter.subsystem);
    if (filter.path) {
      const pl = filter.path.toLowerCase();
      matches = matches.filter(
        (n) => typeof n.path === "string" && n.path.toLowerCase().includes(pl)
      );
    }
    return {
      ok: true,
      artifact,
      mode: "filtered",
      meta: spineMeta(spine),
      query: {
        ...(filter.type ? { type: filter.type } : {}),
        ...(filter.subsystem ? { subsystem: filter.subsystem } : {}),
        ...(filter.path ? { path: filter.path } : {}),
      },
      match_count: matches.length,
      matches: matches.map(compact),
    };
  }

  // default summary
  return {
    ok: true,
    artifact,
    mode: "summary",
    meta: spineMeta(spine),
    counts: { nodes: nodes.length, edges: edges.length },
    nodes_by_type: countBy(nodes, (n) => n.type),
    edges_by_type: countBy(edges, (e) => e.type),
    node_types: spine.node_types ?? Object.keys(countBy(nodes, (n) => n.type)).sort(),
    edge_types: spine.edge_types ?? Object.keys(countBy(edges, (e) => e.type)).sort(),
    usage:
      "Filter with node=<id> (exact node + incident edges, or a contains match), " +
      "or type=/subsystem=/path= to list matching nodes.",
  };
}
