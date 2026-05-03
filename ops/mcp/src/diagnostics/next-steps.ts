import type {
  Priority,
  RecommendedStep,
  SafetyLevel,
  Severity,
} from "./schema.js";
import type { McpFileReport } from "./mcp-config.js";
import { buildEnvironmentReport } from "./environment-report.js";
import { buildDoctorReport } from "./doctor.js";
import { buildStateIndex } from "./state-index.js";

export type { RecommendedStep } from "./schema.js";

/** @deprecated Use Priority from schema.js */
export type NextStepPriority = Priority;

export type NextStepsReport = {
  severity: Severity;
  summary: string;
  recommended_steps: RecommendedStep[];
  diagnosis_digest: {
    doctor_severity: Severity;
    doctor_summary: string;
    doctor_suggested_repairs_count: number;
    doctor_error_checks: number;
    doctor_warn_checks: number;
    env_warning_codes: string[];
    state_missing_important: number;
    mcp_config_issues: number;
    portability_script_ok: boolean;
  };
};

function rankSeverity(
  a: NextStepsReport["severity"],
  b: NextStepsReport["severity"]
): NextStepsReport["severity"] {
  const o = { ok: 0, warn: 1, error: 2 };
  return o[a] >= o[b] ? a : b;
}

function mcpConfigIssueCount(files: McpFileReport[]): number {
  let n = 0;
  for (const f of files) {
    if (!f.exists) n++;
    else if (f.parseError) n++;
    else if (f.invokesDistDirectly) n++;
    else if (f.usesCanonicalLaunch === false) n++;
  }
  return n;
}

function mcpNeedsManualFix(files: McpFileReport[]): boolean {
  return files.some((f) => {
    if (!f.exists) return true;
    if (f.parseError) return true;
    if (f.invokesDistDirectly) return true;
    if (f.usesCanonicalLaunch === false) return true;
    return false;
  });
}

export type NextStepsSignals = {
  nodeModules: boolean;
  distIndex: boolean;
  betterSqliteLoadable: boolean;
  mcpFiles: McpFileReport[];
  portabilityScriptOk: boolean;
  gitDirtyLines: number | null;
  kubectlAvailable: boolean;
  ghAvailable: boolean;
  stateMissingImportant: number;
  doctorSeverity: Severity;
  doctorSummary: string;
  doctorSuggestedRepairsCount: number;
  doctorErrorChecks: number;
  doctorWarnChecks: number;
  envWarningCodes: string[];
};

/**
 * Pure next-step synthesis for tests and for `buildNextStepsReport`.
 * Does not execute subprocesses.
 */
export function analyzeNextStepsFromSignals(s: NextStepsSignals): NextStepsReport {
  const steps: RecommendedStep[] = [];
  let severity: NextStepsReport["severity"] = "ok";

  const bump = (next: NextStepsReport["severity"]) => {
    severity = rankSeverity(severity, next);
  };

  if (!s.nodeModules) {
    bump("error");
    steps.push({
      title: "Install MCP dependencies",
      reason: "ops/mcp/node_modules is missing; MCP server and tests cannot run.",
      command: "npm ci",
      working_directory: "ops/mcp",
      safety: "modifies_local",
      priority: "high",
      blocks_agent_work: true,
    });
  }

  if (s.nodeModules && !s.distIndex) {
    bump("warn");
    steps.push({
      title: "Build MCP TypeScript output",
      reason: "ops/mcp/dist/index.js is missing; some workflows expect a prebuilt dist.",
      command: "npm run build",
      working_directory: "ops/mcp",
      safety: "modifies_local",
      priority: "medium",
      blocks_agent_work: false,
    });
  }

  if (s.nodeModules && !s.betterSqliteLoadable) {
    bump("error");
    steps.push({
      title: "Rebuild native sqlite addon",
      reason: "better-sqlite3 failed to load (often Node ABI mismatch).",
      command: "npm rebuild better-sqlite3",
      working_directory: "ops/mcp",
      safety: "modifies_local",
      priority: "high",
      blocks_agent_work: true,
    });
    steps.push({
      title: "Or reinstall MCP deps for current Node",
      reason: "If rebuild fails, reinstall so postinstall runs under the same Node as the MCP host.",
      command: "npm ci",
      working_directory: "ops/mcp",
      safety: "modifies_local",
      priority: "high",
      blocks_agent_work: true,
    });
  }

  const mcpFix = mcpNeedsManualFix(s.mcpFiles);
  const portabilityFail = !s.portabilityScriptOk;
  if (mcpFix || portabilityFail) {
    bump("error");
    const bits: string[] = [];
    if (mcpFix) bits.push("MCP JSON wiring or parse errors");
    if (portabilityFail) bits.push("scripts/check-mcp-portability.py failed");
    steps.push({
      title: "Resolve MCP portability or launch wiring",
      reason: `${bits.join("; ")}. Inspect script output and align .mcp.json / .cursor/mcp.json with the canonical npm --prefix ./ops/mcp run start:stdio stanza.`,
      command: "python3 scripts/check-mcp-portability.py",
      working_directory: "repo_root",
      safety: "read_only",
      priority: "high",
      blocks_agent_work: true,
    });
  }

  if (s.gitDirtyLines != null && s.gitDirtyLines > 0) {
    bump("warn");
    steps.push({
      title: "Review dirty worktree",
      reason: `Uncommitted changes span ${s.gitDirtyLines} path(s); confirm scope before editing.`,
      command: "git status --short",
      working_directory: "repo_root",
      safety: "read_only",
      priority: "medium",
      blocks_agent_work: false,
    });
  }

  if (!s.kubectlAvailable) {
    bump("warn");
    steps.push({
      title: "Kubernetes CLI unavailable",
      reason: "kubectl missing or not configured; cluster-oriented MCP tools are limited.",
      working_directory: "repo_root",
      safety: "read_only",
      priority: "low",
      blocks_agent_work: false,
    });
  }

  if (!s.ghAvailable) {
    bump("warn");
    steps.push({
      title: "GitHub CLI unavailable or unauthenticated",
      reason: "gh missing or auth failed; PR-status helpers are unavailable.",
      working_directory: "repo_root",
      safety: "read_only",
      priority: "low",
      blocks_agent_work: false,
    });
  }

  if (s.envWarningCodes.length > 0) {
    bump("warn");
  }

  if (s.stateMissingImportant > 0) {
    bump("warn");
    steps.push({
      title: "Verify ops state files",
      reason: "Expected sprint or repo-map JSON is missing for orchestration workflows.",
      working_directory: "repo_root",
      safety: "read_only",
      priority: "medium",
      blocks_agent_work: false,
    });
  }

  if (s.doctorSeverity === "error" && steps.filter((x) => x.blocks_agent_work).length === 0) {
    bump("error");
    steps.push({
      title: "Review icn_ops_doctor output",
      reason: s.doctorSummary,
      working_directory: "repo_root",
      safety: "read_only",
      priority: "high",
      blocks_agent_work: true,
    });
  }

  if (steps.length === 0) {
    steps.push({
      title: "Proceed with scoped verification",
      reason: "No blocking MCP issues detected; use icn_ops_verification_plan for area-specific checks.",
      working_directory: "repo_root",
      safety: "read_only",
      priority: "low",
      blocks_agent_work: false,
    });
  }

  if (severity === "ok" && s.doctorSeverity === "warn") {
    severity = "warn";
  } else if (severity === "ok" && s.doctorSeverity === "error") {
    severity = "error";
  }

  const summary =
    severity === "error"
      ? "Address blocking MCP or dependency issues before deep agent work."
      : severity === "warn"
        ? "Environment usable; resolve warnings when they affect your task."
        : "Environment looks ready; pick a verification plan for your change area.";

  return {
    severity,
    summary,
    recommended_steps: steps,
    diagnosis_digest: {
      doctor_severity: s.doctorSeverity,
      doctor_summary: s.doctorSummary,
      doctor_suggested_repairs_count: s.doctorSuggestedRepairsCount,
      doctor_error_checks: s.doctorErrorChecks,
      doctor_warn_checks: s.doctorWarnChecks,
      env_warning_codes: s.envWarningCodes,
      state_missing_important: s.stateMissingImportant,
      mcp_config_issues: mcpConfigIssueCount(s.mcpFiles),
      portability_script_ok: s.portabilityScriptOk,
    },
  };
}

export async function buildNextStepsReport(repoRoot: string): Promise<NextStepsReport> {
  const env = await buildEnvironmentReport(repoRoot);
  const doctor = await buildDoctorReport(repoRoot);
  const state = buildStateIndex(repoRoot);

  const importantState = ["ops/state/sprint/current.json", "ops/state/config/repo-map.json"];
  const stateMissing = importantState.filter(
    (id) => !state.entries.find((e) => e.id === id)?.present
  ).length;

  const portabilityCheck = doctor.checks.find((c) => c.id === "portability_script");
  const portabilityScriptOk = portabilityCheck?.severity === "ok";

  const errCount = doctor.checks.filter((c) => c.severity === "error").length;
  const warnCount = doctor.checks.filter((c) => c.severity === "warn").length;

  return analyzeNextStepsFromSignals({
    nodeModules: env.paths.nodeModules,
    distIndex: env.paths.distIndex,
    betterSqliteLoadable: env.betterSqlite3.loadable,
    mcpFiles: env.mcpConfigs.files,
    portabilityScriptOk,
    gitDirtyLines: env.git.porcelainLines,
    kubectlAvailable: env.kubectl.available,
    ghAvailable: env.gh.available,
    stateMissingImportant: stateMissing,
    doctorSeverity: doctor.severity,
    doctorSummary: doctor.summary,
    doctorSuggestedRepairsCount: doctor.suggested_repairs.length,
    doctorErrorChecks: errCount,
    doctorWarnChecks: warnCount,
    envWarningCodes: env.warnings.map((w) => w.code),
  });
}
