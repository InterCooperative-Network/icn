import type { CommandCatalogEntry, RuntimeBucket } from "./schema.js";

export type { CommandCatalogEntry, RuntimeBucket, SafetyLevel } from "./schema.js";

/** @deprecated Use CommandCatalogEntry; kept for call-site readability. */
export type CatalogCommand = CommandCatalogEntry;

/** @deprecated Use RuntimeBucket */
export type ExpectedRuntime = RuntimeBucket;

export type CommandCatalog = {
  version: 1;
  groups: { name: string; commands: CommandCatalogEntry[] }[];
};

export const COMMAND_CATALOG: CommandCatalog = {
  version: 1,
  groups: [
    {
      name: "MCP checks",
      commands: [
        {
          id: "mcp_ci",
          purpose: "Install MCP deps and rebuild native modules for current Node",
          command: "npm ci",
          working_directory: "ops/mcp",
          safety: "modifies_local",
          runtime: "medium",
          when_to_use: "Fresh clone, Node version change, or better-sqlite3 load errors.",
          caution:
            "npm ci rewrites node_modules from the lockfile; expect native postinstall work and time on cold machines.",
        },
        {
          id: "mcp_build",
          purpose: "Compile ops/mcp TypeScript",
          command: "npm run build",
          working_directory: "ops/mcp",
          safety: "modifies_local",
          runtime: "quick",
          when_to_use: "After editing MCP server sources.",
        },
        {
          id: "mcp_test",
          purpose: "Run MCP unit tests",
          command: "npm test",
          working_directory: "ops/mcp",
          safety: "read_only",
          runtime: "quick",
          when_to_use: "Before committing MCP changes.",
          caution: "Test runners may write local caches; not a mutating production deploy.",
        },
        {
          id: "mcp_portability",
          purpose: "Verify MCP JSON configs are portable and aligned",
          command: "python3 scripts/check-mcp-portability.py",
          working_directory: "repo_root",
          safety: "read_only",
          runtime: "quick",
          when_to_use: "After editing .mcp.json or .cursor/mcp.json.",
        },
        {
          id: "mcp_subprocess_audit",
          purpose:
            "Audit ops/mcp for Node execSync; SQLite db.exec DDL in state/db.ts is unrelated",
          command: 'rg "execSync" ops/mcp/src || true',
          working_directory: "repo_root",
          safety: "read_only",
          runtime: "quick",
          when_to_use: "When changing subprocess or polling code in ops/mcp.",
        },
      ],
    },
    {
      name: "Docs checks",
      commands: [
        {
          id: "docs_index",
          purpose: "Navigate documentation index",
          command: "test -f docs/INDEX.md && echo ok",
          working_directory: "repo_root",
          safety: "read_only",
          runtime: "quick",
          when_to_use: "Finding where to document changes.",
        },
      ],
    },
    {
      name: "Rust checks",
      commands: [
        {
          id: "rust_fmt",
          purpose: "Rust formatting gate",
          command: "cargo fmt --all --check",
          working_directory: "icn",
          safety: "read_only",
          runtime: "medium",
          when_to_use: "Any Rust change before push.",
        },
        {
          id: "rust_clippy",
          purpose: "Clippy with workspace warnings denied",
          command: "cargo clippy --workspace --all-targets --all-features -- -D warnings",
          working_directory: "icn",
          safety: "read_only",
          runtime: "long",
          when_to_use: "Rust changes; scope per AGENTS.md.",
        },
        {
          id: "rust_test",
          purpose: "Full workspace tests",
          command: "cargo test",
          working_directory: "icn",
          safety: "read_only",
          runtime: "long",
          when_to_use: "Validate behavior after substantive Rust edits.",
          caution: "cargo test can run for many minutes; scope with -p or filters when iterating.",
        },
      ],
    },
    {
      name: "Website checks",
      commands: [
        {
          id: "website_build",
          purpose: "Build public site (if package scripts exist)",
          command: "npm run build",
          working_directory: "website",
          safety: "modifies_local",
          runtime: "medium",
          when_to_use: "After content or Astro changes (skip if website/ absent).",
        },
      ],
    },
    {
      name: "Vocabulary scans (manual)",
      commands: [
        {
          id: "grep_payment",
          purpose: "Find payment wording in economics docs",
          command: 'rg -n "payment" docs icn crates icn/apps || true',
          working_directory: "repo_root",
          safety: "read_only",
          runtime: "quick",
          when_to_use: "Compliance pass; prefer settlement terminology per project rules.",
        },
      ],
    },
    {
      name: "PR checks",
      commands: [
        {
          id: "gh_pr_checks",
          purpose: "List CI checks for a PR",
          command: "gh pr checks <PR_NUMBER> || true",
          working_directory: "repo_root",
          safety: "external_side_effect",
          runtime: "quick",
          when_to_use: "After push; requires gh auth.",
          caution: "Contacts GitHub; ensure gh is authenticated and the PR number is correct.",
        },
        {
          id: "gh_pr_create",
          purpose: "Open a new pull request from the current branch",
          command: "gh pr create",
          working_directory: "repo_root",
          safety: "external_side_effect",
          runtime: "quick",
          when_to_use: "After local verification; human should review title/body before running.",
          caution:
            "Creates or updates remote PR metadata; has external side effects. Never run unattended from agent automation.",
        },
        {
          id: "git_reset_hard_example",
          purpose: "DESTRUCTIVE — discard local commits and working tree (example only; do not run blindly)",
          command: "git reset --hard <REF>",
          working_directory: "repo_root",
          safety: "destructive",
          runtime: "quick",
          when_to_use: "Only when a human explicitly intends to throw away local work.",
          caution:
            "git reset --hard and git clean -fdx destroy uncommitted and unpushed work. Must never be suggested as an automatic fix.",
        },
      ],
    },
    {
      name: "Repo status",
      commands: [
        {
          id: "git_status",
          purpose: "Short worktree status",
          command: "git status --short",
          working_directory: "repo_root",
          safety: "read_only",
          runtime: "quick",
          when_to_use: "Before commit; confirm scope.",
        },
        {
          id: "git_diff_stat",
          purpose: "Diffstat for review sizing",
          command: "git diff --stat",
          working_directory: "repo_root",
          safety: "read_only",
          runtime: "quick",
          when_to_use: "Before opening PR.",
        },
      ],
    },
  ],
};
