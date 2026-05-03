import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  analyzeNextStepsFromSignals,
  buildNextStepsReport,
  type NextStepsSignals,
} from "../diagnostics/next-steps.js";
import { buildVerificationPlan } from "../diagnostics/verification-plan.js";
import { buildRepoMap } from "../diagnostics/repo-map.js";
import { COMMAND_CATALOG } from "../diagnostics/command-catalog.js";
import type { McpFileReport } from "../diagnostics/mcp-config.js";
import { SAFETY_LEVELS, SEVERITY_LEVELS } from "../diagnostics/schema.js";

const ALLOWED_SAFETY = SAFETY_LEVELS;

const canonicalMcpFiles: McpFileReport[] = [
  {
    path: ".mcp.json",
    exists: true,
    usesCanonicalLaunch: true,
    invokesDistDirectly: false,
  },
  {
    path: ".cursor/mcp.json",
    exists: true,
    usesCanonicalLaunch: true,
    invokesDistDirectly: false,
  },
];

function baseSignals(over: Partial<NextStepsSignals>) {
  return {
    nodeModules: true,
    distIndex: true,
    betterSqliteLoadable: true,
    mcpFiles: canonicalMcpFiles,
    portabilityScriptOk: true,
    gitDirtyLines: 0,
    kubectlAvailable: true,
    ghAvailable: true,
    stateMissingImportant: 0,
    doctorSeverity: "ok" as const,
    doctorSummary: "ok",
    doctorSuggestedRepairsCount: 0,
    doctorErrorChecks: 0,
    doctorWarnChecks: 0,
    envWarningCodes: [] as string[],
    ...over,
  };
}

describe("analyzeNextStepsFromSignals", () => {
  it("flags missing node_modules as error with npm ci", () => {
    const r = analyzeNextStepsFromSignals(
      baseSignals({
        nodeModules: false,
        distIndex: false,
        betterSqliteLoadable: false,
      })
    );
    expect(r.severity).toBe("error");
    const ci = r.recommended_steps.find((s) => s.command === "npm ci");
    expect(ci?.working_directory).toBe("ops/mcp");
    expect(ci?.blocks_agent_work).toBe(true);
  });

  it("recommends build when node_modules present but dist missing", () => {
    const r = analyzeNextStepsFromSignals(
      baseSignals({
        nodeModules: true,
        distIndex: false,
      })
    );
    expect(r.recommended_steps.some((s) => s.command === "npm run build")).toBe(true);
    expect(r.severity).toMatch(/warn|error/);
  });

  it("treats missing kubectl as warn severity, not error", () => {
    const r = analyzeNextStepsFromSignals(
      baseSignals({
        kubectlAvailable: false,
        doctorSeverity: "ok",
      })
    );
    expect(r.severity).toBe("warn");
    expect(r.severity).not.toBe("error");
    const k = r.recommended_steps.find((s) => s.title.includes("Kubernetes"));
    expect(k?.blocks_agent_work).toBe(false);
  });
});

describe("buildNextStepsReport", () => {
  it("does not throw on sparse temp repo", async () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-mcp-next-"));
    const r = await buildNextStepsReport(dir);
    expect(Array.isArray(r.recommended_steps)).toBe(true);
    expect(r.diagnosis_digest).toBeDefined();
    expect([...SEVERITY_LEVELS]).toContain(r.severity);
  });
});

describe("buildVerificationPlan", () => {
  it("mcp area includes build, test, portability, startup, and rg audits", () => {
    const plan = buildVerificationPlan("mcp", "standard");
    const cmds = plan.steps.map((s) => s.command).join("\n");
    expect(cmds).toContain("npm run build");
    expect(cmds).toContain("npm test");
    expect(cmds).toContain("check-mcp-portability.py");
    expect(cmds).toContain("start:stdio");
    expect(cmds).toMatch(/execSync/);
    expect(cmds).toMatch(/shell: true/);
    expect(cmds).toMatch(/dist\/index/);
  });

  it("full includes MCP steps plus broader rust/docs/vocabulary (and website when not quick)", () => {
    const mcp = buildVerificationPlan("mcp", "standard").steps.length;
    const full = buildVerificationPlan("full", "standard");
    expect(full.steps.length).toBeGreaterThan(mcp);
    const joined = full.steps.map((s) => s.command).join("\n");
    expect(joined).toContain("cargo fmt");
    expect(joined).toContain("docs/INDEX.md");
    expect(joined).toContain("payment");
    expect(joined).toContain("blockchain");
    expect(joined).toContain("website");
  });
});

describe("buildRepoMap", () => {
  it("marks missing paths present:false", () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-mcp-map-"));
    const { entries } = buildRepoMap(dir);
    const icn = entries.find((e) => e.path === "icn");
    expect(icn?.present).toBe(false);
  });
});

describe("safety labels (catalog + verification plans)", () => {
  const destructiveHints = [/git\s+clean\b/i, /git\s+reset\b.*--hard/i, /\brm\s+-rf\b/i];

  function assertDestructiveLabel(cmd: string, safety: string) {
    if (destructiveHints.some((re) => re.test(cmd))) {
      expect(safety).toBe("destructive");
    }
  }

  it("restricts catalog safety to allowed enum", () => {
    for (const g of COMMAND_CATALOG.groups) {
      for (const c of g.commands) {
        expect(ALLOWED_SAFETY).toContain(c.safety);
        assertDestructiveLabel(c.command, c.safety);
      }
    }
  });

  it("restricts verification plan safety to allowed enum", () => {
    const areas = ["mcp", "docs", "rust", "website", "vocabulary", "pr", "full"] as const;
    const risks = ["quick", "standard", "thorough"] as const;
    for (const area of areas) {
      for (const risk of risks) {
        for (const step of buildVerificationPlan(area, risk).steps) {
          expect(ALLOWED_SAFETY).toContain(step.safety);
          assertDestructiveLabel(step.command, step.safety);
        }
      }
    }
  });

  it("labels git reset example as destructive in catalog", () => {
    const row = COMMAND_CATALOG.groups
      .flatMap((g) => g.commands)
      .find((c) => c.id === "git_reset_hard_example");
    expect(row?.safety).toBe("destructive");
    expect(row?.command).toMatch(/reset --hard/);
  });
});
