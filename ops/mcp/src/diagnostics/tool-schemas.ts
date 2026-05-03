import {
  AGENT_TOOL_CONTRACT_VERSION,
  PRIORITY_LEVELS,
  RUNTIME_BUCKETS,
  SAFETY_LEVELS,
  SCHEMA_TOOL_NAME,
  SEVERITY_LEVELS,
  TOOL_STABILITY_LEVELS,
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

/** Static file + optional live tool superset: enums and provenance for offline agents. */
export type AgentMcpContractExport = AgentToolSchemaBundle & {
  generated_from: string;
  enums: {
    severity: readonly string[];
    safety: readonly string[];
    runtime: readonly string[];
    priority: readonly string[];
    tool_stability: readonly string[];
  };
};

/** Deep-sort object keys for deterministic JSON (drift tests + export script). */
export function sortKeysDeep(x: unknown): unknown {
  if (Array.isArray(x)) {
    return x.map(sortKeysDeep);
  }
  if (x !== null && typeof x === "object" && !Array.isArray(x)) {
    const proto = Object.getPrototypeOf(x);
    if (proto !== null && proto !== Object.prototype) {
      return x;
    }
    const o = x as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(o).sort()) {
      out[k] = sortKeysDeep(o[k]);
    }
    return out;
  }
  return x;
}

export function stableSerializeMcpContract(obj: unknown): string {
  return `${JSON.stringify(sortKeysDeep(obj), null, 2)}\n`;
}

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
        output_schema_summary:
          "AgentMcpContractExport: matches docs/guides/developer/agent-mcp-contracts.json (generated_from, enums, contract_version, unknown_fields_policy, tools[]).",
        stability: "experimental",
        notes: "Meta tool; contract_version applies to all listed tools. Prefer typed fields over prose in clients.",
      },
    ],
  };
}

export function buildAgentMcpContractExport(): AgentMcpContractExport {
  const base = buildAgentToolSchemaBundle();
  return {
    ...base,
    generated_from: "icn-ops ops/mcp: diagnostics/schema.ts + diagnostics/tool-schemas.ts (buildAgentToolSchemaBundle)",
    enums: {
      severity: [...SEVERITY_LEVELS],
      safety: [...SAFETY_LEVELS],
      runtime: [...RUNTIME_BUCKETS],
      priority: [...PRIORITY_LEVELS],
      tool_stability: [...TOOL_STABILITY_LEVELS],
    },
  };
}
