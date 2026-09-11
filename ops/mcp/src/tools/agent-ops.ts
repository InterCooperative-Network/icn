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
  readFileAtRevision,
  resolveCanonicalRepository,
  resolveRevision,
} from "../canonical/repository.js";
import {
  SPINE_REL,
  buildAgentContextSpineViewFromText,
  buildPathBriefFromText,
} from "../diagnostics/agent-context-spine.js";
import {
  describeSourceRevision,
  snapshotWorktree,
  type SourceProvenance,
} from "../diagnostics/source-revision.js";
import {
  buildAgentContextSpineView,
  buildPathBrief,
} from "../diagnostics/agent-context-spine.js";

export function registerAgentOpsTools(
  server: McpServer,
  db?: Database.Database
): void {
  const repoRoot = resolveMonorepoRoot();

  // Every repository-DERIVED answer carries the provenance of the tree it was read from, so a
  // stale or dirty source is visible in the answer instead of looking identical to a current one.
  // Statically compiled catalogs (agent_brief, command_catalog, verification_plan) are
  // deliberately NOT stamped: they are baked into the build and are not reads of the checkout, so
  // tying them to its revision would assert a relationship that does not exist.
  //
  // `build` is a thunk rather than an already-computed value so that HEAD is captured BEFORE the
  // payload is read. Otherwise a checkout fast-forwarded mid-call would return data read from the
  // old tree under a stamp naming the new one — a revision-attribution bug in the very feature
  // that exists to prevent them, and one made likelier by the host-synchronisation work this is
  // a step towards. A revision that moves across the read fails closed.
  //
  // `root` is the tree the payload is read from, which is not always repoRoot —
  // icn_ops_agent_runtime deliberately reads the CALLER's lane instead.
  //
  // SCOPE. This covers answers whose CONTENT is read out of one checkout, where the reader
  // cannot otherwise tell which. It does not cover `repo_status` / `worktree_status` in
  // tools/repos.ts: those report ON a set of trees and already name each one per row, so the
  // provenance is intrinsic rather than missing — and `repo_status` returns an array, which this
  // object-shaped stamp cannot carry without changing its contract. Giving those surfaces
  // staleness semantics is a separate change, tracked with the bare-store work (R1-B).
  //
  // This is distinct from a generated artifact's own `source_commit` (the Agent Context Spine
  // carries one): that records the revision the artifact was generated FROM, while `source`
  // records the checkout this answer was read OUT OF. When the two disagree, the artifact is
  // stale relative to the tree holding it — precisely the condition worth surfacing.
  /**
   * The ref canonical reads resolve. Declared once here rather than taken from a request: which
   * revision is authoritative is an operator question, not a caller question.
   */
  const CANONICAL_REF = "refs/remotes/origin/main";

  /**
   * Canonical storage could not answer.
   *
   * This returns an error rather than reading a working tree. Falling back would reintroduce the
   * exact dependency R1-B removes, and would do so precisely when the canonical path is broken —
   * the moment the fallback is least trustworthy.
   */
  function canonicalUnavailable(error: {
    code: string;
    message: string;
  }): { content: { type: "text"; text: string }[] } {
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify(
            {
              error: `canonical read unavailable: ${error.message}`,
              code: error.code,
              worktree_consulted: false,
              note:
                "This answer is served only from controlled canonical storage at an exact " +
                "revision. No working tree is inspected, and none is substituted when canonical " +
                "storage cannot answer.",
            },
            null,
            2
          ),
        },
      ],
    };
  }

  async function repoDerived(
    build: () => Record<string, unknown> | Promise<Record<string, unknown>>,
    root?: string,
    provenance: SourceProvenance = "controlled"
  ): Promise<{ content: { type: "text"; text: string }[] }> {
    // `root` stays undefined for the ordinary case so describeSourceRevision resolves it the
    // way it actually was — ICN_ROOT or the server's location. Defaulting it to repoRoot here
    // and passing that on would make every response claim `explicit_root`, which is only true
    // of the caller-lane case.
    const target = root ?? repoRoot;
    // Two working-tree scans, not three: the before/after guard needs both, and the source
    // description reuses the second rather than taking a third of the same tree.
    const before = await snapshotWorktree(target, provenance);
    const payload = await build();
    const after = await snapshotWorktree(target, provenance);
    const source = await describeSourceRevision(root, provenance, after);
    if (
      before.fingerprint === null ||
      after.fingerprint === null ||
      before.fingerprint !== after.fingerprint
    ) {
      source.trustworthy = false;
      source.warnings.push(
        "the checkout's commit or working tree changed while this answer was being read, or " +
          "could not be determined; the payload may describe state the stamp does not"
      );
    }
    return {
      content: [{ type: "text", text: JSON.stringify({ ...payload, source }, null, 2) }],
    };
  }

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
      provider_session_id: z
        .string()
        .optional()
        .describe(
          "Your own harness conversation id, if you know it. Without it this tool can report " +
            "who occupies the lane but CANNOT tell you whether YOU are registered."
        ),
      section: z
        .enum(["all", "mcp_tools", "skills", "hooks", "helpers", "truth_domains", "session"])
        .optional()
        .default("all")
        .describe("Narrow the response when you only need one part."),
    },
    async ({ cwd, section, provider_session_id }) => {
      // Resolve the CALLER's lane first. repoRoot honours ICN_ROOT, which on icn-dev is pinned
      // to the mcp-host worktree — so using it would hand an agent working in lane X the
      // capability manifest of lane Y (or, as observed, a confusing ENOENT for a manifest that
      // exists perfectly well in the caller's own worktree). The lane wins; repoRoot is the
      // fallback for callers that supply no cwd.
      const identity = discoverWorktree(cwd ?? process.cwd(), null);
      const manifestRoot = identity?.worktree_path ?? repoRoot;
      return await repoDerived(async () => {
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
      // `registered` is a claim about the CALLER. Deriving it from lane occupancy told a
      // review subagent, a second agent in the worktree, or a session whose own registration
      // failed that it was registered because SOMEONE ELSE had a row — undoing the one thing
      // the SessionStart hook reports honestly. Unknown is reported as unknown.
      let session: Record<string, unknown> = { registered: null };
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
          session["lane_sessions"] = rows.map((r) => ({
            session_id: r.id,
            provider_session_id: r.provider_session_id,
            progress_count: r.progress_count,
            current_activity: r.current_activity,
          }));
          session["contention"] = rows.length > 1;

          if (provider_session_id) {
            const mine = rows.find((r) => r.provider_session_id === provider_session_id);
            session["registered"] = Boolean(mine);
            if (mine) {
              session["my_session_id"] = mine.id;
              session["my_progress_count"] = mine.progress_count;
            } else {
              session["note"] =
                `No session is registered for provider_session_id ${provider_session_id}. ` +
                "Lifecycle tracking is NOT active for YOU, regardless of other occupants.";
            }
          } else {
            session["note"] =
              rows.length === 0
                ? "No session is registered for this lane at all, so lifecycle tracking is not " +
                  "active here. The SessionStart hook may not be installed for this launcher."
                : "Pass provider_session_id to learn whether YOU are registered; " +
                  `${rows.length} session(s) occupy this lane, which says nothing about you.`;
          }
        } else {
          session["note"] =
            "No session registry is wired into this server, so registration cannot be checked.";
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
      return payload;
      // Pass the lane only when discovery actually found one. When it falls back to repoRoot,
      // the root was NOT explicitly chosen — it came from ICN_ROOT or the server's location, and
      // the stamp should say so rather than claiming this call site picked it.
      //
      // This is the ONLY tool that resolves its root from client input, which makes it the only
      // one whose checkout is untrusted. `cwd` is what the caller supplied; when they supplied
      // it and it resolved to a lane, the resulting tree is theirs to configure, so its working
      // tree is never inspected (see SourceProvenance). Falling back to repoRoot, or resolving
      // from the server's own cwd, is operator-controlled and keeps full inspection.
      },
      identity?.worktree_path,
      cwd !== undefined && identity ? "caller_selected" : "controlled");
    }
  );

  server.tool(
    "icn_ops_environment_report",
    "Structured environment snapshot (git, Node/npm, optional gh/kubectl, MCP config parity hints). Never fails on missing optional tools; see warnings array.",
    {},
    async () => {
      return await repoDerived(() => buildEnvironmentReport(repoRoot));
    }
  );

  server.tool(
    "icn_ops_doctor",
    "Read-only diagnosis: MCP wiring, native sqlite module, portability script, dirty tree, optional CLIs. Returns severity, checks, and suggested repair commands (not executed).",
    {},
    async () => {
      return await repoDerived(() => buildDoctorReport(repoRoot));
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
      // The read itself must sit inside the thunk: closing over an already-built value would
      // let both revision probes observe a new HEAD while `entries` came from the old tree.
      return await repoDerived(() => {
        const { entries } = buildStateIndex(repoRoot);
        const wantAbsent = include_absent !== false;
        return { entries: wantAbsent ? entries : entries.filter((e) => e.present) };
      });
    }
  );

  server.tool(
    "icn_ops_next_steps",
    "Read-only workflow guidance: severity, short summary, and recommended next verification or setup steps (commands are strings only; never executed by MCP). Uses environment, doctor, state index, MCP parity, and worktree hints without echoing full raw diagnostics.",
    {},
    async () => {
      return await repoDerived(() => buildNextStepsReport(repoRoot));
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
      return await repoDerived(() => buildRepoMap(repoRoot));
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
      // Revision-addressed read from canonical storage (R1-B). The spine is committed content,
      // so nothing about answering from it requires a working tree — and reading it from one
      // made the answer depend on whichever checkout happened to host the server, which on this
      // host was 23 commits behind with uncommitted edits.
      //
      // The ref is resolved to a revision ONCE and that revision is carried through the read, so
      // a branch moving mid-operation cannot produce a spliced answer.
      const repo = await resolveCanonicalRepository();
      if (!repo.ok) return canonicalUnavailable(repo.error);
      const revision = await resolveRevision(repo.value, CANONICAL_REF);
      if (!revision.ok) return canonicalUnavailable(revision.error);
      const spineText = await readFileAtRevision(repo.value, revision.value, SPINE_REL);
      if (!spineText.ok) return canonicalUnavailable(spineText.error);

      const view =
        paths && paths.length > 0
          ? buildPathBriefFromText(spineText.value, paths)
          : buildAgentContextSpineViewFromText(spineText.value, {
              node,
              type,
              subsystem,
              path,
            });
      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(
              {
                ...view,
                source: {
                  kind: "canonical_revision_read",
                  source_revision: revision.value,
                  source_store: repo.value.gitDir,
                  ref: CANONICAL_REF,
                  worktree_consulted: false,
                },
              },
              null,
              2
            ),
          },
        ],
      };
    }
  );
}
