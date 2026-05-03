import { existsSync, statSync } from "node:fs";
import { join } from "node:path";
import type { RepoMapEntry } from "./schema.js";

export type { RepoMapEntry } from "./schema.js";

const ENTRIES: Omit<RepoMapEntry, "present" | "kind">[] = [
  {
    path: "docs",
    description: "Canonical documentation tree.",
    agent_use: "Read STATE, architecture, guides before claiming behavior.",
  },
  {
    path: "ops/mcp",
    description: "icn-ops MCP server (TypeScript stdio).",
    agent_use: "Diagnostics tools, pollers, agent workflow guidance.",
  },
  {
    path: "scripts",
    description: "Repo automation (portability, bootstrap, worktrees).",
    agent_use: "Run scripted checks from repo root; paths are portable.",
  },
  {
    path: "website",
    description: "Public site source (if present).",
    agent_use: "Content and build; not the Rust daemon.",
    caution: "Absence is normal on sparse checkouts.",
  },
  {
    path: "icn",
    description: "Rust workspace root (Cargo.toml).",
    agent_use: "All cargo * commands run from here per AGENTS.md.",
  },
  {
    path: "docs/adr",
    description: "Architecture Decision Records.",
    agent_use: "Cite ADRs when changing boundaries or public claims.",
  },
  {
    path: "docs/rfcs",
    description: "RFC drafts and process.",
    agent_use: "Protocol and design evolution.",
  },
  {
    path: "ops/state",
    description: "Orchestration state (sprint, truth spine, config).",
    agent_use: "Sprint JSON, repo-map; may be absent on fresh clones.",
  },
  {
    path: "sdk/typescript",
    description: "TypeScript SDK.",
    agent_use: "Regenerate types after gateway API drift.",
  },
  {
    path: "sdk/react-native",
    description: "React Native SDK (if present).",
    agent_use: "Mobile client; verify package exists before edits.",
  },
  {
    path: "web/pilot-ui",
    description: "Pilot PWA (if present).",
    agent_use: "End-user UI experiments.",
  },
  {
    path: "deploy",
    description: "Deployment manifests (K8s, etc.).",
    agent_use: "Ops-facing; no secrets in git.",
    caution: "Treat as sensitive; follow placeholder policy.",
  },
];

export function buildRepoMap(repoRoot: string): { entries: RepoMapEntry[] } {
  const entries: RepoMapEntry[] = [];
  for (const e of ENTRIES) {
    const absolutePath = join(repoRoot, e.path);
    let present = false;
    let kind: "file" | "directory" = "directory";
    try {
      if (!existsSync(absolutePath)) {
        present = false;
      } else {
        const st = statSync(absolutePath);
        kind = st.isDirectory() ? "directory" : "file";
        present = true;
      }
    } catch {
      present = false;
    }
    entries.push({ ...e, present, kind });
  }
  return { entries };
}
