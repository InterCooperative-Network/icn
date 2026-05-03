import {
  AGENT_TOOL_CONTRACT_VERSION,
  SCHEMA_TOOL_NAME,
  type ToolStability,
} from "./schema.js";

export type AgentToolSchemaEntry = {
  tool: string;
  /** Aligns with bundle.contract_version until per-tool majors diverge. */
  version: string;
  input_schema: Record<string, unknown> | null;
  /** One-line input contract for agents; complements input_schema when non-null. */
  input_schema_summary: string | null;
  output_schema_summary: string;
  stability: ToolStability;
  notes?: string;
};

export type AgentToolSchemaBundle = {
  contract_version: string;
  /** Forward-compat rule for all tools in this bundle. */
  unknown_fields_policy: string;
  tools: AgentToolSchemaEntry[];
};

const V = AGENT_TOOL_CONTRACT_VERSION;

const NO_INPUT: Pick<AgentToolSchemaEntry, "input_schema" | "input_schema_summary"> = {
  input_schema: null,
  input_schema_summary: "No tool arguments (empty object).",
};

export function buildAgentToolSchemaBundle(): AgentToolSchemaBundle {
  return {
    contract_version: V,
    unknown_fields_policy:
      "Clients MUST ignore unknown JSON keys in any tool response body so additive server fields do not break parsers.",
    tools: [
      {
        tool: "icn_ops_environment_report",
        version: V,
        ...NO_INPUT,
        output_schema_summary:
          "EnvironmentReport{repoRoot,warnings[],git,node,npm,rust,python,gh,kubectl,paths,betterSqlite3,mcpConfigs}",
        stability: "stable",
        notes: "Read-only snapshot; missing optional CLIs appear as warnings[], not fatal errors.",
      },
      {
        tool: "icn_ops_doctor",
        version: V,
        ...NO_INPUT,
        output_schema_summary:
          "DoctorReport{severity:Severity,summary,checks:DiagnosticCheck[],suggested_repairs:string[]}",
        stability: "stable",
        notes: "Suggested repairs are strings only; never executed by MCP.",
      },
      {
        tool: "icn_ops_agent_brief",
        version: V,
        ...NO_INPUT,
        output_schema_summary:
          "AgentBrief{read_first[],safe_vocabulary[],forbidden_vocabulary[],public_surface[],verification_by_area[],pr_hygiene[],completeness_warning,mcp_troubleshooting[]}",
        stability: "stable",
      },
      {
        tool: "icn_ops_command_catalog",
        version: V,
        ...NO_INPUT,
        output_schema_summary:
          "CommandCatalog{version:1,groups{name,commands:CommandCatalogEntry[]}}; entry fields id,purpose,command,working_directory,safety,runtime,when_to_use,caution?",
        stability: "stable",
        notes: "safety uses shared enum; cautions describe lockfile/time/network behavior only.",
      },
      {
        tool: "icn_ops_state_index",
        version: V,
        input_schema: {
          type: "object",
          properties: {
            include_absent: {
              type: "boolean",
              description: "List absent entries (default true).",
            },
          },
        },
        input_schema_summary: "Optional include_absent:boolean (default true).",
        output_schema_summary: "{entries:StateIndexEntry[]}",
        stability: "stable",
      },
      {
        tool: "icn_ops_next_steps",
        version: V,
        ...NO_INPUT,
        output_schema_summary:
          "NextStepsReport{severity:Severity,summary,recommended_steps:RecommendedStep[],diagnosis_digest}",
        stability: "stable",
        notes: "recommended_steps[].safety uses shared enum; destructive steps must include caution when present.",
      },
      {
        tool: "icn_ops_verification_plan",
        version: V,
        input_schema: {
          type: "object",
          required: ["area"],
          properties: {
            area: {
              type: "string",
              enum: ["mcp", "docs", "rust", "website", "vocabulary", "pr", "full"],
            },
            risk_level: {
              type: "string",
              enum: ["quick", "standard", "thorough"],
              description: "Default standard.",
            },
          },
        },
        input_schema_summary: "Required area:string enum; optional risk_level:string enum (default standard).",
        output_schema_summary:
          "VerificationPlan{area,risk_level,steps:VerificationPlanStep[]}; step fields order,command,working_directory,purpose,expected_success_signal,safety,estimated_runtime,notes?",
        stability: "stable",
      },
      {
        tool: "icn_ops_repo_map",
        version: V,
        ...NO_INPUT,
        output_schema_summary: "{entries:RepoMapEntry[]}",
        stability: "stable",
      },
      {
        tool: SCHEMA_TOOL_NAME,
        version: V,
        ...NO_INPUT,
        output_schema_summary: "AgentToolSchemaBundle (this object shape, including unknown_fields_policy).",
        stability: "experimental",
        notes: "Meta tool; contract_version applies to all listed tools. Prefer typed fields over prose in clients.",
      },
    ],
  };
}
