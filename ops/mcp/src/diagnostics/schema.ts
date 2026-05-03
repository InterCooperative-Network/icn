/**
 * Shared contract types for agent-facing icn-ops MCP tools.
 * Version bumps when additive fields change; breaking renames require major bump.
 */
export const AGENT_TOOL_CONTRACT_VERSION = "1.0.0";

export const SEVERITY_LEVELS = ["ok", "warn", "error"] as const;
export type Severity = (typeof SEVERITY_LEVELS)[number];

export const SAFETY_LEVELS = [
  "read_only",
  "modifies_local",
  "destructive",
  "external_side_effect",
] as const;
export type SafetyLevel = (typeof SAFETY_LEVELS)[number];

export const RUNTIME_BUCKETS = ["quick", "medium", "long"] as const;
export type RuntimeBucket = (typeof RUNTIME_BUCKETS)[number];

export const PRIORITY_LEVELS = ["high", "medium", "low"] as const;
export type Priority = (typeof PRIORITY_LEVELS)[number];

export const TOOL_STABILITY_LEVELS = ["experimental", "stable"] as const;
export type ToolStability = (typeof TOOL_STABILITY_LEVELS)[number];

/** MCP tools that return structured JSON for agents (excluding meta schema tool). */
export const AGENT_OUTPUT_TOOL_NAMES = [
  "icn_ops_environment_report",
  "icn_ops_doctor",
  "icn_ops_agent_brief",
  "icn_ops_command_catalog",
  "icn_ops_state_index",
  "icn_ops_next_steps",
  "icn_ops_verification_plan",
  "icn_ops_repo_map",
] as const;

export type AgentOutputToolName = (typeof AGENT_OUTPUT_TOOL_NAMES)[number];

export const SCHEMA_TOOL_NAME = "icn_ops_tool_schemas" as const;

/** All tools that participate in the contract bundle (including meta). */
export const AGENT_TOOLS_WITH_CONTRACT = [
  ...AGENT_OUTPUT_TOOL_NAMES,
  SCHEMA_TOOL_NAME,
] as const;

export type AgentWorkingDirectory =
  | "repo_root"
  | "icn"
  | "ops/mcp"
  | "sdk/typescript"
  | "web/pilot-ui"
  | "website";

/** One doctor / environment-style check line. */
export type DiagnosticCheck = {
  id: string;
  severity: Severity;
  message: string;
  detail?: string;
};

/** Recommended action from icn_ops_next_steps (commands are strings only; not run by MCP). */
export type RecommendedStep = {
  title: string;
  reason: string;
  command?: string;
  working_directory: AgentWorkingDirectory;
  safety: SafetyLevel;
  priority: Priority;
  blocks_agent_work: boolean;
  /** Required when safety is destructive; optional otherwise. */
  caution?: string;
};

/** Single command row in icn_ops_command_catalog. */
export type CommandCatalogEntry = {
  id: string;
  purpose: string;
  command: string;
  working_directory: AgentWorkingDirectory;
  safety: SafetyLevel;
  runtime: RuntimeBucket;
  when_to_use: string;
  caution?: string;
};

/** One row in icn_ops_verification_plan steps. */
export type VerificationPlanStep = {
  order: number;
  command: string;
  working_directory: AgentWorkingDirectory;
  purpose: string;
  expected_success_signal: string;
  safety: SafetyLevel;
  estimated_runtime: RuntimeBucket;
  notes?: string;
};

/** Repo layout entry from icn_ops_repo_map. */
export type RepoMapEntry = {
  path: string;
  present: boolean;
  kind: "file" | "directory";
  description: string;
  agent_use: string;
  caution?: string;
};

/** State / doc path entry from icn_ops_state_index. */
export type StateIndexEntry = {
  id: string;
  absolutePath: string;
  present: boolean;
  kind: "file" | "directory";
  description: string;
};
