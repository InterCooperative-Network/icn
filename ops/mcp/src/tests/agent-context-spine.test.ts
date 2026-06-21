import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  buildAgentContextSpineView,
  buildPathBrief,
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

const BRIEF_SAMPLE = {
  schema: "icn-agent-context-spine/v0",
  status: "generated",
  canonical: false,
  generated: "2026-06-21T00:00:00+00:00",
  source_commit: "deadbeef",
  generator: "scripts/generate-agent-context-spine.py",
  regenerate: "x",
  check: "x",
  nodes: [
    {
      id: "crate:icn-trust",
      type: "crate",
      name: "icn-trust",
      path: "icn/crates/icn-trust",
      source_of_truth: "icn/Cargo.toml",
      evidence: [{ source: "icn/Cargo.toml" }],
    },
    { id: "subsystem:trust", type: "subsystem", name: "trust", source_of_truth: "CLAUDE.md", evidence: [{ source: "CLAUDE.md" }] },
    {
      id: "guidance:rust-workspace",
      type: "path_guidance",
      name: "ICN Rust workspace crate",
      match: "icn/",
      source_of_truth: "AGENTS.md",
      review_focus: ["No panics in protocol paths", "Determinism"],
      verification_commands: ["cd icn && cargo test"],
      risk_surfaces: [],
      invariants: ["invariant:determinism"],
      docs: ["doc:agents"],
      recommended_skills: ["skill:navigator"],
      recommended_agents: ["agent:icn-code-reviewer"],
      evidence: [{ source: "icn" }],
    },
    {
      id: "guidance:mcp",
      type: "path_guidance",
      name: "icn-ops MCP server (TypeScript)",
      match: "ops/mcp/",
      source_of_truth: "docs/guides/developer/agent-mcp-tooling.md",
      review_focus: ["Read-only / no mutation"],
      verification_commands: ["npm --prefix ./ops/mcp test"],
      risk_surfaces: [],
      invariants: [],
      docs: ["doc:agent-mcp-tooling"],
      recommended_skills: ["skill:navigator", "skill:doctor"],
      recommended_agents: ["agent:icn-ops", "agent:icn-code-reviewer"],
      evidence: [{ source: "ops/mcp" }],
    },
    {
      id: "guidance:docs",
      type: "path_guidance",
      name: "Documentation (truth / claim discipline)",
      match: "docs/",
      source_of_truth: "docs/reference/project-index/source-of-truth-map.md",
      review_focus: ["No production/live/pilot overclaims"],
      verification_commands: ["python3 docs/scripts/doc_control_check.py"],
      risk_surfaces: ["claim_surface:public-website-claims"],
      invariants: [],
      docs: ["doc:source-of-truth-map"],
      recommended_skills: ["skill:truth-sync", "skill:navigator"],
      recommended_agents: ["agent:icn-docs-truth-auditor"],
      evidence: [{ source: "docs" }],
    },
  ],
  edges: [
    {
      from: "crate:icn-trust",
      to: "subsystem:trust",
      type: "owned_by_subsystem",
      evidence: { source: "CLAUDE.md" },
    },
  ],
};

describe("buildPathBrief", () => {
  it("briefs a known ops/mcp path", () => {
    const dir = fixtureRoot(BRIEF_SAMPLE);
    const view = buildPathBrief(dir, ["ops/mcp/src/tools/agent-ops.ts"]);
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    const entry = (view.paths as Array<Record<string, string[]>>)[0];
    expect(entry.areas).toContain("icn-ops MCP server (TypeScript)");
    expect(entry.verification_commands).toContain("npm --prefix ./ops/mcp test");
    expect(entry.recommended_agents).toContain("agent:icn-ops");
  });

  it("briefs a known docs path with claim-surface risk", () => {
    const dir = fixtureRoot(BRIEF_SAMPLE);
    const view = buildPathBrief(dir, ["docs/INDEX.md"]);
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    const entry = (view.paths as Array<Record<string, string[]>>)[0];
    expect(entry.claim_surfaces).toContain("claim_surface:public-website-claims");
    expect(entry.recommended_agents).toContain("agent:icn-docs-truth-auditor");
  });

  it("resolves crate -> subsystem from the graph for a Rust path", () => {
    const dir = fixtureRoot(BRIEF_SAMPLE);
    const view = buildPathBrief(dir, ["icn/crates/icn-trust/src/lib.rs"]);
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    const entry = (view.paths as Array<Record<string, string[]>>)[0];
    expect(entry.subsystems).toContain("trust");
    expect(entry.matched_nodes).toContain("crate:icn-trust");
    expect(entry.invariants).toContain("invariant:determinism");
  });

  it("combines multiple paths and deduplicates commands and skills", () => {
    const dir = fixtureRoot(BRIEF_SAMPLE);
    const view = buildPathBrief(dir, [
      "ops/mcp/src/a.ts",
      "ops/mcp/src/b.ts", // same area twice -> must dedupe
      "docs/INDEX.md",
    ]);
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    const combined = view.combined as Record<string, string[]>;
    // navigator appears in both mcp and docs guidance -> exactly once
    expect(combined.recommended_skills.filter((s) => s === "skill:navigator").length).toBe(1);
    expect(
      combined.verification_commands.filter((c) => c === "npm --prefix ./ops/mcp test").length
    ).toBe(1);
  });

  it("returns a clear fallback for an unknown path", () => {
    const dir = fixtureRoot(BRIEF_SAMPLE);
    const view = buildPathBrief(dir, ["totally/unknown/file.xyz"]);
    expect(view.ok).toBe(true);
    if (!view.ok) return;
    const entry = (view.paths as Array<Record<string, unknown>>)[0];
    expect(entry.note).toMatch(/no direct guidance match/i);
    expect(entry.recommended_skills).toContain("skill:navigator");
    expect((entry.matched_nodes as string[]).length).toBe(0);
  });

  it("fails clearly when the artifact is missing (brief mode)", () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-mcp-brief-missing-"));
    const view = buildPathBrief(dir, ["ops/mcp/x.ts"]);
    expect(view.ok).toBe(false);
    if (view.ok) return;
    expect(view.error).toMatch(/not found/i);
  });
});
