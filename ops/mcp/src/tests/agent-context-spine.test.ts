import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  buildAgentContextSpineView,
  SPINE_REL,
} from "../diagnostics/agent-context-spine.js";

function fixtureRoot(spine: unknown): string {
  const dir = mkdtempSync(join(tmpdir(), "icn-mcp-spine-"));
  const abs = join(dir, SPINE_REL);
  mkdirSync(join(abs, ".."), { recursive: true });
  writeFileSync(abs, typeof spine === "string" ? spine : JSON.stringify(spine, null, 2));
  return dir;
}

const SAMPLE = {
  schema: "icn-agent-context-spine/v0",
  status: "generated",
  canonical: false,
  generated: "2026-06-21T00:00:00+00:00",
  source_commit: "deadbeef",
  generator: "scripts/generate-agent-context-spine.py",
  regenerate: "python3 scripts/generate-agent-context-spine.py --write",
  check: "python3 scripts/generate-agent-context-spine.py --check",
  node_types: ["crate", "subsystem", "route_surface"],
  edge_types: ["owned_by_subsystem", "exposes"],
  nodes: [
    {
      id: "crate:icn-gateway",
      type: "crate",
      name: "icn-gateway",
      path: "icn/crates/icn-gateway",
      description: "Gateway crate.",
      source_of_truth: "icn/Cargo.toml",
      evidence: [{ source: "icn/Cargo.toml", kind: "workspace-member" }],
    },
    {
      id: "crate:icn-trust",
      type: "crate",
      name: "icn-trust",
      subsystem: "trust",
      path: "icn/crates/icn-trust",
      description: "Trust crate.",
      source_of_truth: "icn/Cargo.toml",
      evidence: [{ source: "icn/Cargo.toml", kind: "workspace-member" }],
    },
    {
      id: "subsystem:trust",
      type: "subsystem",
      name: "trust",
      description: "Trust subsystem.",
      source_of_truth: "CLAUDE.md",
      evidence: [{ source: "CLAUDE.md", kind: "core-subsystems-section" }],
    },
    {
      id: "route_surface:gateway",
      type: "route_surface",
      name: "gateway-route-surface",
      path: "docs/reference/project-index/generated/route-inventory.md",
      description: "Route pointer.",
      source_of_truth: "docs/scripts/route_inventory.py",
      evidence: [{ source: "docs/scripts/route_inventory.py", kind: "route-source" }],
    },
  ],
  edges: [
    {
      from: "crate:icn-trust",
      to: "subsystem:trust",
      type: "owned_by_subsystem",
      evidence: { source: "CLAUDE.md" },
    },
    {
      from: "crate:icn-gateway",
      to: "route_surface:gateway",
      type: "exposes",
      evidence: { source: "docs/reference/project-index/generated/route-inventory.md" },
    },
  ],
};

describe("buildAgentContextSpineView", () => {
  it("loads the artifact and returns a summary by default", () => {
    const dir = fixtureRoot(SAMPLE);
    const view = buildAgentContextSpineView(dir);
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    expect(view.mode).toBe("summary");
    expect(view.counts).toEqual({ nodes: 4, edges: 2 });
    expect((view.nodes_by_type as Record<string, number>).crate).toBe(2);
    expect((view.edges_by_type as Record<string, number>).exposes).toBe(1);
    // meta marks it non-canonical and carries the readiness caveat
    const meta = view.meta as Record<string, unknown>;
    expect(meta.canonical).toBe(false);
    expect(String(meta.note)).toMatch(/not.*runtime|readiness/i);
  });

  it("returns one node plus incident edges for an exact node id", () => {
    const dir = fixtureRoot(SAMPLE);
    const view = buildAgentContextSpineView(dir, { node: "crate:icn-gateway" });
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    expect(view.mode).toBe("node");
    expect((view.node as { id: string }).id).toBe("crate:icn-gateway");
    const incident = view.incident_edges as Array<{ type: string; other: string }>;
    expect(incident.length).toBe(1);
    expect(incident[0].type).toBe("exposes");
    expect(incident[0].other).toBe("route_surface:gateway");
  });

  it("falls back to a contains-match list for a non-exact node query", () => {
    const dir = fixtureRoot(SAMPLE);
    const view = buildAgentContextSpineView(dir, { node: "icn-" });
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    expect(view.mode).toBe("filtered");
    expect(view.match_count).toBe(2); // icn-gateway + icn-trust
  });

  it("filters by type", () => {
    const dir = fixtureRoot(SAMPLE);
    const view = buildAgentContextSpineView(dir, { type: "crate" });
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    expect(view.mode).toBe("filtered");
    expect(view.match_count).toBe(2);
  });

  it("filters by subsystem", () => {
    const dir = fixtureRoot(SAMPLE);
    const view = buildAgentContextSpineView(dir, { subsystem: "trust" });
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    expect(view.match_count).toBe(1);
    expect((view.matches as Array<{ id: string }>)[0].id).toBe("crate:icn-trust");
  });

  it("filters by path substring", () => {
    const dir = fixtureRoot(SAMPLE);
    const view = buildAgentContextSpineView(dir, { path: "icn-gateway" });
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    expect(view.match_count).toBe(1);
    expect((view.matches as Array<{ id: string }>)[0].id).toBe("crate:icn-gateway");
  });

  it("fails clearly when the artifact is missing", () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-mcp-spine-missing-"));
    const view = buildAgentContextSpineView(dir);
    expect(view.ok).toBe(false);
    if (view.ok) return;
    expect(view.error).toMatch(/not found/i);
    expect(view.hint).toMatch(/generate-agent-context-spine\.py --write/);
  });

  it("fails clearly when the artifact is malformed JSON", () => {
    const dir = fixtureRoot("{ this is not json");
    const view = buildAgentContextSpineView(dir);
    expect(view.ok).toBe(false);
    if (view.ok) return;
    expect(view.error).toMatch(/invalid JSON/i);
  });

  it("fails clearly when nodes/edges arrays are absent", () => {
    const dir = fixtureRoot({ schema: "x", status: "generated", canonical: false });
    const view = buildAgentContextSpineView(dir);
    expect(view.ok).toBe(false);
    if (view.ok) return;
    expect(view.error).toMatch(/nodes.*edges/i);
  });
});
