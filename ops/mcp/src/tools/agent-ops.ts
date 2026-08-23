import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { readFileSync } from "fs";
import { join } from "node:path";
import { resolveMonorepoRoot } from "../paths.js";
import { activeSessionsForWorktree } from "../runtime/session-runtime.js";
import { discoverWorktree, readBranchState } from "../runtime/worktree-identity.js";
import { buildEnvironmentReport } from "../diagnostics/environment-report.js";
import { buildDoctorReport } from "../diagnostics/doctor.js";
import { AGENT_BRIEF } from "../diagnostics/agent-brief.js";
import { COMMAND_CATALOG } from "../diagnostics/command-catalog.js";
import { buildStateIndex } from "../diagnostics/state-index.js";
import { buildNextStepsReport } from "../diagnostics/next-steps.js";
import { buildVerificationPlan } from "../diagnostics/verification-plan.js";
import { buildRepoMap } from "../diagnostics/repo-map.js";
import {
  buildAgentContextSpineView,
  buildPathBrief,
} from "../diagnostics/agent-context-spine.js";

export function registerAgentOpsTools(
  server: McpServer,
  db?: Database.Database
): void {
  const repoRoot = resolveMonorepoRoot();

  server.tool(
    "icn_ops_agent_runtime",
    "START HERE. What this agent runtime can do and who this session is. Returns the " +
      "GENERATED capability manifest (MCP tools verified against the live server, canonical " +
      "skills, hooks, helper scripts, truth-domain owners) plus this session's lifecycle " +
      "identity. Nothing here is hand-maintained: adding a capability in its canonical " +
      "location and regenerating makes it appear, and CI fails if the manifest drifts.",
    {
      cwd: z
        .string()
        .optional()
        .describe("Any path inside your worktree; used to report your lane and session."),
      section: z
        .enum(["all", "mcp_tools", "skills", "hooks", "helpers", "truth_domains", "session"])
        .optional()
        .default("all")
        .describe("Narrow the response when you only need one part."),
    },
    async ({ cwd, section }) => {
      // Resolve the CALLER's lane first. repoRoot honours ICN_ROOT, which on icn-dev is pinned
      // to the mcp-host worktree — so using it would hand an agent working in lane X the
      // capability manifest of lane Y (or, as observed, a confusing ENOENT for a manifest that
      // exists perfectly well in the caller's own worktree). The lane wins; repoRoot is the
      // fallback for callers that supply no cwd.
      const identity = discoverWorktree(cwd ?? process.cwd(), null);
      const manifestRoot = identity?.worktree_path ?? repoRoot;
      const manifestPath = join(
        manifestRoot,
        "docs/reference/project-index/generated/agent-capabilities.json"
      );
      let manifest: Record<string, unknown> = {};
      let manifestError: string | null = null;
      try {
        manifest = JSON.parse(readFileSync(manifestPath, "utf-8")) as Record<string, unknown>;
      } catch (e) {
        manifestError =
          `capability manifest unreadable at ${manifestPath} ` +
          `(${e instanceof Error ? e.message : String(e)}). ` +
          "Regenerate: python3 scripts/generate-agent-capabilities.py --write";
      }

      // Session identity is reported honestly: an unregistered session is told so rather than
      // being given a plausible-looking blank record.
      let session: Record<string, unknown> = { registered: false };
      if (identity) {
        session["lane"] = {
          repo_id: identity.repo_id,
          worktree_id: identity.worktree_id,
          worktree_path: identity.worktree_path,
          worktree_name: identity.worktree_name,
          live_branch: readBranchState(identity.worktree_path),
        };
        if (db) {
          const rows = activeSessionsForWorktree(db, identity.worktree_id);
          session["registered"] = rows.length > 0;
          session["sessions"] = rows.map((r) => ({
            session_id: r.id,
            provider_session_id: r.provider_session_id,
            progress_count: r.progress_count,
            current_activity: r.current_activity,
          }));
          session["contention"] = rows.length > 1;
          if (rows.length === 0) {
            session["note"] =
              "No session is registered for this lane. Lifecycle tracking is NOT active; " +
              "the SessionStart hook may not be installed for this launcher.";
          }
        }
      } else {
        session["note"] = "not inside a Git worktree";
      }

      const payload: Record<string, unknown> =
        section === "session"
          ? { session }
          : section === "all"
            ? { ...manifest, session }
            : { [section]: manifest[section] ?? [], session };
      if (manifestError) payload["manifest_error"] = manifestError;

      return { content: [{ type: "text", text: JSON.stringify(payload, null, 2) }] };
    }
  );

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

  server.tool(
    "icn_ops_next_steps",
    "Read-only workflow guidance: severity, short summary, and recommended next verification or setup steps (commands are strings only; never executed by MCP). Uses environment, doctor, state index, MCP parity, and worktree hints without echoing full raw diagnostics.",
    {},
    async () => {
      const report = await buildNextStepsReport(repoRoot);
      return {
        content: [{ type: "text", text: JSON.stringify(report, null, 2) }],
      };
    }
  );

  server.tool(
    "icn_ops_verification_plan",
    "Ordered verification checklist for an area (commands are recommendations only; not executed). risk_level tunes breadth: quick | standard | thorough.",
    {
      area: z
        .enum(["mcp", "docs", "rust", "website", "vocabulary", "pr", "full"])
        .describe("Subsystem or scope to plan checks for."),
      risk_level: z
        .enum(["quick", "standard", "thorough"])
        .optional()
        .describe("Default standard. thorough adds longer installs/tests where applicable."),
    },
    async ({ area, risk_level }) => {
      const plan = buildVerificationPlan(area, risk_level ?? "standard");
      return {
        content: [{ type: "text", text: JSON.stringify(plan, null, 2) }],
      };
    }
  );

  server.tool(
    "icn_ops_repo_map",
    "Compact repo layout map for agents: key directories with present flag, one-line description, agent_use, and optional caution. Paths are checked on disk; absent paths are present:false.",
    {},
    async () => {
      const map = buildRepoMap(repoRoot);
      return {
        content: [{ type: "text", text: JSON.stringify(map, null, 2) }],
      };
    }
  );

  server.tool(
    "icn_ops_agent_context_spine",
    "Read-only view of the generated Agent Context Spine (docs/reference/project-index/generated/agent-context-spine.json): a non-canonical, evidence-grounded orientation map of crates, subsystems, docs, routes, invariants, claim surfaces, truth sources, skills/agents and MCP tools. Pass paths=[...] for a CODE-QUALITY BRIEF on changed files (subsystem, invariants, docs, verification commands, claim/API risk, recommended ICN skills/agents, review focus) — query this before editing or reviewing a change. Otherwise: no filter returns a summary; node=<id> returns one node + its incident edges (or a contains-match list); type/subsystem/path filter the node list. Never executes commands or mutates files. Structure is not runtime liveness; asserts no production/live/pilot readiness.",
    {
      paths: z
        .array(z.string())
        .optional()
        .describe("Changed file paths (repo-relative). Returns a per-path + combined code-quality brief. Takes precedence over the other filters."),
      node: z.string().optional().describe("Node id (e.g. crate:icn-gateway). Exact match returns the node and its incident edges; otherwise a contains-match list of ids/names."),
      type: z
        .string()
        .optional()
        .describe("Filter nodes by type (crate, subsystem, doc, route_surface, skill, agent, mcp_tool, script, invariant, claim_surface, truth_source, generated_artifact, path_guidance)."),
      subsystem: z.string().optional().describe("Filter nodes by subsystem id (e.g. trust, gossip)."),
      path: z.string().optional().describe("Filter nodes whose path contains this substring (e.g. icn-gateway)."),
    },
    async ({ paths, node, type, subsystem, path }) => {
      const view =
        paths && paths.length > 0
          ? buildPathBrief(repoRoot, paths)
          : buildAgentContextSpineView(repoRoot, { node, type, subsystem, path });
      return {
        content: [{ type: "text", text: JSON.stringify(view, null, 2) }],
      };
    }
  );
}
