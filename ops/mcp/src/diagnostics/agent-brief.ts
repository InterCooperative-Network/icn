/** Compact briefing for autonomous agents (structured, not narrative). */

export type AgentBrief = {
  read_first: { path: string; why: string }[];
  safe_vocabulary: string[];
  forbidden_vocabulary: string[];
  public_surface: string[];
  verification_by_area: { area: string; commands: string[] }[];
  pr_hygiene: string[];
  completeness_warning: string;
  mcp_troubleshooting: string[];
};

export const AGENT_BRIEF: AgentBrief = {
  read_first: [
    { path: "AGENTS.md", why: "Non-negotiable invariants, change routing, doc placement." },
    { path: "docs/STATE.md", why: "Current implementation status when present." },
    { path: "CLAUDE.md", why: "Workspace conventions (Rust root under icn/, gateway port 8080)." },
    {
      path: "docs/architecture/KERNEL_APP_SEPARATION.md",
      why: "Kernel vs Policy Oracle boundary when touching enforcement.",
    },
  ],
  safe_vocabulary: [
    "settlement (not payment) for mutual-credit flows",
    "Digital Public Infrastructure / coordination substrate",
    "constraint engine / Policy Oracle / meaning firewall",
    "cooperative, federation, governance proposal (domain terms)",
  ],
  forbidden_vocabulary: [
    "Describing ICN primarily as a blockchain, token, or retail payment network",
    "Kernel crates importing domain semantics (trust scores, coop policy types)",
    "Assuming gateway port 8000 (canonical is 8080 unless config states otherwise)",
  ],
  public_surface: [
    "Website content is not automatic proof of production readiness in the daemon.",
    "Public claims must align with ADR-0033-style evidence discipline when applicable.",
    "Separate repo-root website/ from the Rust workspace under icn/.",
  ],
  verification_by_area: [
    {
      area: "Rust crates",
      commands: [
        "cd icn && cargo fmt --all --check",
        "cd icn && cargo clippy --workspace --all-targets --all-features -- -D warnings",
        "cd icn && cargo test",
      ],
    },
    {
      area: "ops MCP",
      commands: [
        "npm --prefix ./ops/mcp ci",
        "npm --prefix ./ops/mcp run build",
        "npm --prefix ./ops/mcp test",
        "python3 scripts/check-mcp-portability.py",
      ],
    },
    {
      area: "Gateway + TS types drift",
      commands: [
        "cd icn && cargo build -p icnctl && ./target/debug/icnctl api export-openapi -o ../docs/api/openapi.generated.yaml",
        "cd sdk/typescript && npm ci && npm run generate-types && npm run check-types",
      ],
    },
  ],
  pr_hygiene: [
    "Conventional commits: type(scope): summary",
    "Keep PRs reviewable; split unrelated work",
    "Do not commit secrets; generated-only commits must touch only declared paths",
    "Run change-routing checks from AGENTS.md for touched areas before requesting review",
  ],
  completeness_warning:
    "Docs and demos may describe intent ahead of code. Verify behavior in source, tests, and runtime before claiming completeness.",
  mcp_troubleshooting: [
    "Native addon ABI: run npm ci or npm rebuild better-sqlite3 under the same Node the MCP host uses.",
    "Launch must use npm --prefix ./ops/mcp run start:stdio (enforced vs .mcp.json and .cursor/mcp.json).",
    "Use icn_ops_doctor after environment changes.",
  ],
};
