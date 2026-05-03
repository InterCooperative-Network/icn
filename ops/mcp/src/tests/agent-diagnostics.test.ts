import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { safeJsonParse } from "../diagnostics/safe-json.js";
import { buildEnvironmentReportSync } from "../diagnostics/environment-report.js";
import { buildDoctorReport } from "../diagnostics/doctor.js";
import { COMMAND_CATALOG } from "../diagnostics/command-catalog.js";
import { AGENT_BRIEF } from "../diagnostics/agent-brief.js";
import { buildStateIndex } from "../diagnostics/state-index.js";
import { inspectMcpConfigs } from "../diagnostics/mcp-config.js";

describe("safeJsonParse", () => {
  it("returns error for empty string without throwing", () => {
    const r = safeJsonParse("", "test");
    expect(r.ok).toBe(false);
  });

  it("returns error for malformed JSON", () => {
    const r = safeJsonParse("{not json", "test");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.preview).toBeDefined();
  });

  it("parses valid JSON", () => {
    const r = safeJsonParse("[1]", "test");
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual([1]);
  });
});

describe("buildEnvironmentReportSync", () => {
  it("does not throw on empty temp dir", () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-mcp-env-"));
    const r = buildEnvironmentReportSync(dir);
    expect(r.repoRoot).toBe(dir);
    expect(r.paths.nodeModules).toBe(false);
    expect(r.paths.distIndex).toBe(false);
    expect(r.warnings.length).toBeGreaterThan(0);
  });
});

describe("buildDoctorReport", () => {
  it("reports missing dist and node_modules cleanly", async () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-mcp-doc-"));
    mkdirSync(join(dir, "ops", "mcp"), { recursive: true });
    writeFileSync(
      join(dir, "ops", "mcp", "package.json"),
      JSON.stringify({ name: "x", private: true })
    );
    writeFileSync(
      join(dir, ".mcp.json"),
      JSON.stringify({
        mcpServers: {
          "icn-ops": {
            command: "npm",
            args: ["--prefix", "./ops/mcp", "run", "start:stdio"],
          },
        },
      })
    );
    mkdirSync(join(dir, ".cursor"), { recursive: true });
    writeFileSync(
      join(dir, ".cursor", "mcp.json"),
      JSON.stringify({
        mcpServers: {
          "icn-ops": {
            command: "npm",
            args: ["--prefix", "./ops/mcp", "run", "start:stdio"],
          },
        },
      })
    );
    mkdirSync(join(dir, "scripts"), { recursive: true });
    writeFileSync(
      join(dir, "scripts", "check-mcp-portability.py"),
      "print('skip')\n"
    );

    const report = await buildDoctorReport(dir);
    expect(report.checks.some((c) => c.id === "node_modules" && c.severity === "error")).toBe(
      true
    );
    expect(report.checks.some((c) => c.id === "dist" && c.severity === "warn")).toBe(true);
    expect(report.suggested_repairs.some((s) => s.includes("npm ci"))).toBe(true);
  });
});

describe("COMMAND_CATALOG", () => {
  it("has stable top-level schema", () => {
    expect(COMMAND_CATALOG.version).toBe(1);
    const mcp = COMMAND_CATALOG.groups.find((g) => g.name === "MCP checks")?.commands;
    expect(mcp?.some((c) => c.id === "mcp_subprocess_audit")).toBe(true);
    expect(mcp?.some((c) => c.id === "mcp_export_contracts")).toBe(true);
    expect(mcp?.some((c) => c.id === "mcp_contract_drift_test")).toBe(true);
    expect(Array.isArray(COMMAND_CATALOG.groups)).toBe(true);
    for (const g of COMMAND_CATALOG.groups) {
      expect(typeof g.name).toBe("string");
      expect(Array.isArray(g.commands)).toBe(true);
      for (const c of g.commands) {
        expect(c.id).toBeTruthy();
        expect(["read_only", "modifies_local", "destructive", "external_side_effect"]).toContain(
          c.safety
        );
        expect(["quick", "medium", "long"]).toContain(c.runtime);
        expect(c.command).toBeTruthy();
        expect(c.working_directory).toBeTruthy();
      }
    }
  });
});

describe("AGENT_BRIEF", () => {
  it("includes safe and forbidden vocabulary", () => {
    expect(AGENT_BRIEF.safe_vocabulary.length).toBeGreaterThan(0);
    expect(AGENT_BRIEF.forbidden_vocabulary.length).toBeGreaterThan(0);
    expect(AGENT_BRIEF.mcp_troubleshooting.join(" ")).toMatch(/doctor|rebuild|MCP/i);
  });
});

describe("buildStateIndex", () => {
  it("reports missing files without inventing them", () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-mcp-st-"));
    const { entries } = buildStateIndex(dir);
    const stateMd = entries.find((e) => e.id === "docs/STATE.md");
    expect(stateMd).toBeDefined();
    expect(stateMd!.present).toBe(false);
  });
});

describe("inspectMcpConfigs", () => {
  it("flags direct dist invocation", () => {
    const dir = mkdtempSync(join(tmpdir(), "icn-mcp-mcp-"));
    mkdirSync(join(dir, ".cursor"), { recursive: true });
    writeFileSync(
      join(dir, ".mcp.json"),
      JSON.stringify({
        mcpServers: {
          "icn-ops": { command: "node", args: ["./ops/mcp/dist/index.js"] },
        },
      })
    );
    writeFileSync(
      join(dir, ".cursor", "mcp.json"),
      JSON.stringify({
        mcpServers: {
          "icn-ops": {
            command: "npm",
            args: ["--prefix", "./ops/mcp", "run", "start:stdio"],
          },
        },
      })
    );
    const { files } = inspectMcpConfigs(dir);
    const root = files.find((f) => f.path === ".mcp.json");
    expect(root?.invokesDistDirectly).toBe(true);
    expect(root?.usesCanonicalLaunch).toBe(false);
  });
});
