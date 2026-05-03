import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { resolveMonorepoRoot } from "../paths.js";
import { buildEnvironmentReport } from "../diagnostics/environment-report.js";
import { buildDoctorReport } from "../diagnostics/doctor.js";
import { AGENT_BRIEF } from "../diagnostics/agent-brief.js";
import { COMMAND_CATALOG } from "../diagnostics/command-catalog.js";
import { buildStateIndex } from "../diagnostics/state-index.js";

export function registerAgentOpsTools(server: McpServer): void {
  const repoRoot = resolveMonorepoRoot();

  server.tool(
    "icn_ops_environment_report",
    "Structured environment snapshot (git, Node/npm, optional gh/kubectl, MCP config parity hints). Never fails on missing optional tools; see warnings array.",
    {},
    async () => {
      const report = await buildEnvironmentReport(repoRoot);
      return {
        content: [{ type: "text", text: JSON.stringify(report, null, 2) }],
      };
    }
  );

  server.tool(
    "icn_ops_doctor",
    "Read-only diagnosis: MCP wiring, native sqlite module, portability script, dirty tree, optional CLIs. Returns severity, checks, and suggested repair commands (not executed).",
    {},
    async () => {
      const report = await buildDoctorReport(repoRoot);
      return {
        content: [{ type: "text", text: JSON.stringify(report, null, 2) }],
      };
    }
  );

  server.tool(
    "icn_ops_agent_brief",
    "Compact structured briefing: safe vocabulary, forbidden terms, verification commands, PR hygiene, MCP troubleshooting.",
    {},
    async () => {
      return {
        content: [{ type: "text", text: JSON.stringify(AGENT_BRIEF, null, 2) }],
      };
    }
  );

  server.tool(
    "icn_ops_command_catalog",
    "Catalog of common verification commands (not executed). Each entry includes cwd hint, safety level, and expected runtime.",
    {},
    async () => {
      return {
        content: [{ type: "text", text: JSON.stringify(COMMAND_CATALOG, null, 2) }],
      };
    }
  );

  server.tool(
    "icn_ops_state_index",
    "Canonical state and architecture doc paths with presence flags (does not invent missing files).",
    {
      include_absent: z
        .boolean()
        .optional()
        .describe("If true, list absent entries explicitly (default true)."),
    },
    async ({ include_absent }) => {
      const { entries } = buildStateIndex(repoRoot);
      const wantAbsent = include_absent !== false;
      const filtered = wantAbsent
        ? entries
        : entries.filter((e) => e.present);
      return {
        content: [{ type: "text", text: JSON.stringify({ entries: filtered }, null, 2) }],
      };
    }
  );
}
