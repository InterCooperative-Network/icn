import type { VerificationPlanStep } from "./schema.js";

export type { VerificationPlanStep } from "./schema.js";

export type VerificationArea =
  | "mcp"
  | "docs"
  | "rust"
  | "website"
  | "vocabulary"
  | "pr"
  | "full";

export type RiskLevel = "quick" | "standard" | "thorough";

export type VerificationPlan = {
  area: VerificationArea;
  risk_level: RiskLevel;
  steps: VerificationPlanStep[];
};

const SHELL_AUDIT =
  'rg "shell: true|/bin/sh|bash -c" ops/mcp/src || true';
const EXEC_AUDIT = 'rg "execSync" ops/mcp/src || true';
const DIST_MCP_AUDIT =
  'rg "dist/index\\\\.js" .mcp.json .cursor/mcp.json || true';

function mcpCoreSteps(risk: RiskLevel): VerificationPlanStep[] {
  const steps: VerificationPlanStep[] = [
    {
      order: 1,
      command: "npm run build",
      working_directory: "ops/mcp",
      purpose: "Typecheck and compile MCP server",
      expected_success_signal: "tsc completes with exit code 0",
      safety: "modifies_local",
      estimated_runtime: "quick",
      notes: "Produces dist/; required before relying on dist-only launches.",
    },
    {
      order: 2,
      command: "npm test",
      working_directory: "ops/mcp",
      purpose: "Run MCP unit tests",
      expected_success_signal: "vitest reports all files passed",
      safety: "read_only",
      estimated_runtime: "quick",
    },
    {
      order: 3,
      command: "python3 scripts/check-mcp-portability.py",
      working_directory: "repo_root",
      purpose: "Enforce MCP JSON parity and portable wiring",
      expected_success_signal: "prints MCP portability check passed",
      safety: "read_only",
      estimated_runtime: "quick",
    },
    {
      order: 4,
      command: "timeout 5 npm --prefix ./ops/mcp run start:stdio",
      working_directory: "repo_root",
      purpose: "Smoke stdio startup (GNU timeout exit 124 means still running)",
      expected_success_signal: "No immediate Node stack trace; exit 124 acceptable",
      safety: "read_only",
      estimated_runtime: "quick",
      notes: "On macOS use gtimeout from coreutils if timeout is missing.",
    },
    {
      order: 5,
      command: EXEC_AUDIT,
      working_directory: "repo_root",
      purpose: "Audit for Node execSync in MCP sources",
      expected_success_signal: "Only expected hits (e.g. catalog text, SQLite db.exec)",
      safety: "read_only",
      estimated_runtime: "quick",
    },
    {
      order: 6,
      command: SHELL_AUDIT,
      working_directory: "repo_root",
      purpose: "Confirm no shell-based subprocess wrappers in ops/mcp/src",
      expected_success_signal: "No matches",
      safety: "read_only",
      estimated_runtime: "quick",
    },
    {
      order: 7,
      command: DIST_MCP_AUDIT,
      working_directory: "repo_root",
      purpose: "Ensure MCP configs do not invoke dist/index.js directly",
      expected_success_signal: "No matches",
      safety: "read_only",
      estimated_runtime: "quick",
    },
  ];
  if (risk === "thorough") {
    steps.push({
      order: 8,
      command: "npm ci",
      working_directory: "ops/mcp",
      purpose: "Clean install MCP deps (native rebuild via postinstall)",
      expected_success_signal: "npm ci exits 0",
      safety: "modifies_local",
      estimated_runtime: "medium",
      notes: "Rewrites node_modules; run when lockfile or Node ABI changed.",
    });
  }
  return steps.map((s, i) => ({ ...s, order: i + 1 }));
}

function docsSteps(risk: RiskLevel): VerificationPlanStep[] {
  const steps: VerificationPlanStep[] = [
    {
      order: 1,
      command: "test -f docs/INDEX.md && echo ok",
      working_directory: "repo_root",
      purpose: "Docs index exists",
      expected_success_signal: "ok",
      safety: "read_only",
      estimated_runtime: "quick",
    },
  ];
  if (risk !== "quick") {
    steps.push({
      order: 2,
      command: "test -f docs/STATE.md && echo ok || true",
      working_directory: "repo_root",
      purpose: "STATE.md optional presence",
      expected_success_signal: "ok or skipped",
      safety: "read_only",
      estimated_runtime: "quick",
    });
  }
  return steps.map((s, i) => ({ ...s, order: i + 1 }));
}

function rustSteps(risk: RiskLevel): VerificationPlanStep[] {
  const steps: VerificationPlanStep[] = [
    {
      order: 1,
      command: "cargo fmt --all --check",
      working_directory: "icn",
      purpose: "Rust formatting gate",
      expected_success_signal: "exit 0",
      safety: "read_only",
      estimated_runtime: "medium",
    },
  ];
  if (risk !== "quick") {
    steps.push({
      order: 2,
      command: "cargo clippy --workspace --all-targets --all-features -- -D warnings",
      working_directory: "icn",
      purpose: "Clippy denied warnings",
      expected_success_signal: "exit 0",
      safety: "read_only",
      estimated_runtime: "long",
    });
  }
  if (risk === "thorough") {
    steps.push({
      order: 3,
      command: "cargo test",
      working_directory: "icn",
      purpose: "Full workspace tests",
      expected_success_signal: "all tests passed",
      safety: "read_only",
      estimated_runtime: "long",
      notes: "Can take many minutes; not required for trivial doc-only PRs.",
    });
  }
  return steps.map((s, i) => ({ ...s, order: i + 1 }));
}

function websiteSteps(_risk: RiskLevel): VerificationPlanStep[] {
  return [
    {
      order: 1,
      command: "test -d website && npm run build || echo skip",
      working_directory: "repo_root",
      purpose: "Build website when directory exists",
      expected_success_signal: "build ok or skip",
      safety: "modifies_local",
      estimated_runtime: "medium",
      notes: "Skip if website/ absent.",
    },
  ];
}

function vocabularySteps(_risk: RiskLevel): VerificationPlanStep[] {
  return [
    {
      order: 1,
      command: 'rg -n "payment" docs icn crates icn/apps || true',
      working_directory: "repo_root",
      purpose: "Scan for payment wording (prefer settlement framing)",
      expected_success_signal: "rg completes; review hits manually",
      safety: "read_only",
      estimated_runtime: "quick",
      notes:
        "Safe vocabulary: settlement, coordination substrate, mutual credit. Forbidden framing: retail payment network, blockchain-as-product pitch.",
    },
    {
      order: 2,
      command: 'rg -n "blockchain|token( |$)|crypto rail|8000\\b" docs icn crates icn/apps || true',
      working_directory: "repo_root",
      purpose: "Scan for blockchain/token hype and wrong gateway port assumptions",
      expected_success_signal: "rg completes; review hits manually",
      safety: "read_only",
      estimated_runtime: "quick",
      notes:
        "Cross-check with icn_ops_agent_brief forbidden_vocabulary; prefer Digital Public Infrastructure / coordination substrate wording.",
    },
  ];
}

function prSteps(_risk: RiskLevel): VerificationPlanStep[] {
  return [
    {
      order: 1,
      command: "gh pr checks <PR_NUMBER> || true",
      working_directory: "repo_root",
      purpose: "Inspect CI for a PR",
      expected_success_signal: "gh returns JSON or human-readable status",
      safety: "external_side_effect",
      estimated_runtime: "quick",
      notes: "Requires gh auth; touches GitHub API. Replace <PR_NUMBER>.",
    },
  ];
}

export function buildVerificationPlan(
  area: VerificationArea,
  riskLevel: RiskLevel = "standard"
): VerificationPlan {
  let steps: VerificationPlanStep[] = [];
  switch (area) {
    case "mcp":
      steps = mcpCoreSteps(riskLevel);
      break;
    case "docs":
      steps = docsSteps(riskLevel);
      break;
    case "rust":
      steps = rustSteps(riskLevel);
      break;
    case "website":
      steps = websiteSteps(riskLevel);
      break;
    case "vocabulary":
      steps = vocabularySteps(riskLevel);
      break;
    case "pr":
      steps = prSteps(riskLevel);
      break;
    case "full": {
      const merged: VerificationPlanStep[] = [
        ...mcpCoreSteps(riskLevel),
        ...docsSteps(riskLevel),
        ...rustSteps(riskLevel),
        ...vocabularySteps(riskLevel),
      ];
      if (riskLevel !== "quick") {
        merged.push(...websiteSteps(riskLevel));
      }
      if (riskLevel === "thorough") {
        merged.push(...prSteps(riskLevel));
      }
      steps = merged.map((s, i) => ({ ...s, order: i + 1 }));
      break;
    }
    default:
      steps = [];
  }
  return { area, risk_level: riskLevel, steps };
}
