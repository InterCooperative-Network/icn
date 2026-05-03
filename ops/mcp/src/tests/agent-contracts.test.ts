import { describe, it, expect } from "vitest";
import {
  AGENT_TOOL_CONTRACT_VERSION,
  AGENT_TOOLS_WITH_CONTRACT,
  AGENT_OUTPUT_TOOL_NAMES,
  PRIORITY_LEVELS,
  RUNTIME_BUCKETS,
  SAFETY_LEVELS,
  SEVERITY_LEVELS,
  TOOL_STABILITY_LEVELS,
} from "../diagnostics/schema.js";
import {
  buildAgentToolSchemaBundle,
  buildAgentMcpContractExport,
} from "../diagnostics/tool-schemas.js";
import { COMMAND_CATALOG } from "../diagnostics/command-catalog.js";
import { buildVerificationPlan } from "../diagnostics/verification-plan.js";
import { analyzeNextStepsFromSignals, buildNextStepsReport } from "../diagnostics/next-steps.js";
import { buildRepoMap } from "../diagnostics/repo-map.js";
import { buildStateIndex } from "../diagnostics/state-index.js";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { McpFileReport } from "../diagnostics/mcp-config.js";

function isRecord(x: unknown): x is Record<string, unknown> {
  return typeof x === "object" && x !== null && !Array.isArray(x);
}

function assertVerificationStepShape(s: unknown): void {
  expect(isRecord(s)).toBe(true);
  if (!isRecord(s)) return;
  expect(typeof s.order).toBe("number");
  expect(typeof s.command).toBe("string");
  expect(typeof s.working_directory).toBe("string");
  expect(typeof s.purpose).toBe("string");
  expect(typeof s.expected_success_signal).toBe("string");
  expect(typeof s.safety).toBe("string");
  expect(SAFETY_LEVELS).toContain(s.safety);
  expect(typeof s.estimated_runtime).toBe("string");
  expect(RUNTIME_BUCKETS).toContain(s.estimated_runtime);
}

function assertCatalogEntryShape(c: unknown): void {
  expect(isRecord(c)).toBe(true);
  if (!isRecord(c)) return;
  for (const k of ["id", "purpose", "command", "working_directory", "safety", "runtime", "when_to_use"]) {
    expect(typeof c[k]).toBe("string");
  }
  expect(SAFETY_LEVELS).toContain(c.safety);
  expect(RUNTIME_BUCKETS).toContain(c.runtime);
}

function assertRepoMapEntryShape(e: unknown): void {
  expect(isRecord(e)).toBe(true);
  if (!isRecord(e)) return;
  expect(typeof e.path).toBe("string");
  expect(typeof e.present).toBe("boolean");
  expect(["file", "directory"]).toContain(e.kind);
  expect(typeof e.description).toBe("string");
  expect(typeof e.agent_use).toBe("string");
}

function assertStateIndexEntryShape(e: unknown): void {
  expect(isRecord(e)).toBe(true);
  if (!isRecord(e)) return;
  expect(typeof e.id).toBe("string");
  expect(typeof e.absolutePath).toBe("string");
  expect(typeof e.present).toBe("boolean");
  expect(["file", "directory"]).toContain(e.kind);
  expect(typeof e.description).toBe("string");
}

function assertRecommendedStepShape(s: unknown): void {
  expect(isRecord(s)).toBe(true);
  if (!isRecord(s)) return;
  expect(typeof s.title).toBe("string");
  expect(typeof s.reason).toBe("string");
  expect(typeof s.working_directory).toBe("string");
  expect(typeof s.safety).toBe("string");
  expect(SAFETY_LEVELS).toContain(s.safety);
  expect(PRIORITY_LEVELS).toContain(s.priority);
  expect(typeof s.blocks_agent_work).toBe("boolean");
  if (s.safety === "destructive") {
    expect(typeof s.caution === "string" && s.caution.length > 0).toBe(true);
  }
}

describe("AgentMcpContractExport", () => {
  it("extends the tool bundle with enums and generated_from", () => {
    const ex = buildAgentMcpContractExport();
    const base = buildAgentToolSchemaBundle();
    expect(ex.contract_version).toBe(base.contract_version);
    expect(ex.tools).toEqual(base.tools);
    expect(ex.generated_from.length).toBeGreaterThan(20);
    expect(ex.enums.severity).toEqual([...SEVERITY_LEVELS]);
    expect(ex.enums.safety).toEqual([...SAFETY_LEVELS]);
  });
});

describe("icn_ops_tool_schemas bundle", () => {
  it("lists every agent-facing contract tool exactly once", () => {
    const bundle = buildAgentToolSchemaBundle();
    expect(bundle.contract_version).toBe(AGENT_TOOL_CONTRACT_VERSION);
    expect(typeof bundle.unknown_fields_policy).toBe("string");
    expect(bundle.unknown_fields_policy.length).toBeGreaterThan(20);
    const names = bundle.tools.map((t) => t.tool).sort();
    const expected = [...AGENT_TOOLS_WITH_CONTRACT].sort();
    expect(names).toEqual(expected);
  });

  it("gives each tool version, stability, input+output summaries", () => {
    const bundle = buildAgentToolSchemaBundle();
    for (const t of bundle.tools) {
      expect(t.version).toBe(AGENT_TOOL_CONTRACT_VERSION);
      expect(TOOL_STABILITY_LEVELS).toContain(t.stability);
      expect(typeof t.output_schema_summary).toBe("string");
      expect(t.output_schema_summary.length).toBeGreaterThan(10);
      expect(
        t.input_schema_summary === null ||
          (typeof t.input_schema_summary === "string" && t.input_schema_summary.length > 0)
      ).toBe(true);
      expect(t.input_schema === null || isRecord(t.input_schema)).toBe(true);
    }
  });

  it("includes only known output tools plus the meta schema tool", () => {
    const bundle = buildAgentToolSchemaBundle();
    const set = new Set(bundle.tools.map((x) => x.tool));
    for (const n of AGENT_OUTPUT_TOOL_NAMES) {
      expect(set.has(n)).toBe(true);
    }
    expect(set.has("icn_ops_tool_schemas")).toBe(true);
  });
});

describe("shared enums on live outputs", () => {
  it("next_steps severities and steps conform", () => {
    const canonical: McpFileReport[] = [
      { path: ".mcp.json", exists: true, usesCanonicalLaunch: true, invokesDistDirectly: false },
      { path: ".cursor/mcp.json", exists: true, usesCanonicalLaunch: true, invokesDistDirectly: false },
    ];
    const r = analyzeNextStepsFromSignals({
      nodeModules: true,
      distIndex: true,
      betterSqliteLoadable: true,
      mcpFiles: canonical,
      portabilityScriptOk: true,
      gitDirtyLines: 0,
      kubectlAvailable: true,
      ghAvailable: true,
      stateMissingImportant: 0,
      doctorSeverity: "ok",
      doctorSummary: "ok",
      doctorSuggestedRepairsCount: 0,
      doctorErrorChecks: 0,
      doctorWarnChecks: 0,
      envWarningCodes: [],
    });
    expect(SEVERITY_LEVELS).toContain(r.severity);
    expect(SEVERITY_LEVELS).toContain(r.diagnosis_digest.doctor_severity);
    for (const step of r.recommended_steps) {
      assertRecommendedStepShape(step);
    }
  });

  it("buildNextStepsReport does not throw on sparse temp repo", async () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-mcp-contracts-sparse-"));
    const r = await buildNextStepsReport(dir);
    expect(SEVERITY_LEVELS).toContain(r.severity);
    expect(Array.isArray(r.recommended_steps)).toBe(true);
    for (const step of r.recommended_steps) {
      assertRecommendedStepShape(step);
    }
  });

  it("verification plan steps conform for all area x risk", () => {
    const areas = ["mcp", "docs", "rust", "website", "vocabulary", "pr", "full"] as const;
    const risks = ["quick", "standard", "thorough"] as const;
    for (const area of areas) {
      for (const risk of risks) {
        for (const step of buildVerificationPlan(area, risk).steps) {
          assertVerificationStepShape(step);
        }
      }
    }
  });

  it("command catalog entries conform", () => {
    for (const g of COMMAND_CATALOG.groups) {
      for (const c of g.commands) {
        assertCatalogEntryShape(c);
      }
    }
  });

  it("repo map and state index entries conform on temp repo", () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-contract-"));
    for (const e of buildRepoMap(dir).entries) {
      assertRepoMapEntryShape(e);
    }
    for (const e of buildStateIndex(dir).entries) {
      assertStateIndexEntryShape(e);
    }
  });
});

describe("safety copy rules", () => {
  it("destructive catalog commands include caution text", () => {
    for (const g of COMMAND_CATALOG.groups) {
      for (const c of g.commands) {
        if (c.safety === "destructive") {
          expect(c.caution && c.caution.length > 0).toBe(true);
        }
      }
    }
  });

  it("external_side_effect catalog entries are not described as automatically safe", () => {
    const banned = /automatically safe|auto-safe|perfectly safe|safe to run unattended|no risk to run/i;
    for (const g of COMMAND_CATALOG.groups) {
      for (const c of g.commands) {
        if (c.safety === "external_side_effect") {
          const blob = `${c.purpose} ${c.when_to_use} ${c.caution ?? ""}`;
          expect(blob).not.toMatch(banned);
        }
      }
    }
  });
});
