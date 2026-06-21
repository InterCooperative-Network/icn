import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { initDb } from "./state/db.js";
import { registerSessionTools } from "./tools/sessions.js";
import { registerTaskTools } from "./tools/tasks.js";
import { registerRepoTools } from "./tools/repos.js";
import { registerHealthTools } from "./tools/health.js";
import { registerDecisionTools, syncDecisionIndex } from "./tools/decisions.js";
import { startGitPolling } from "./polling/git.js";
import { startClusterPolling } from "./polling/cluster.js";
import { startBuildsPolling } from "./polling/builds.js";
import { readFileSync, existsSync } from "fs";
import { join } from "node:path";
import { resolveMonorepoRoot, resolveOpsStatePath } from "./paths.js";
import { registerEventTools } from "./tools/events.js";
import { registerCommsTools } from "./tools/comms.js";
import { registerWatcherTools } from "./tools/watchers.js";
import { startProcessPolling } from "./polling/processes.js";
import { registerAgentOpsTools } from "./tools/agent-ops.js";

const ICN_ROOT = resolveMonorepoRoot();
const SPRINT_FILE = resolveOpsStatePath("sprint", "current.json");
const REPO_MAP_FILE = resolveOpsStatePath("config", "repo-map.json");
const AGENT_CONTEXT_SPINE_FILE = join(
  ICN_ROOT,
  "docs",
  "reference",
  "project-index",
  "generated",
  "agent-context-spine.json"
);

async function main() {
  const db = initDb();

  const server = new McpServer({
    name: "icn-ops",
    version: "0.1.0",
  });

  // Sync ADR files on disk into SQLite index (catches manually-written ADRs)
  syncDecisionIndex(db);

  // Register all tool groups
  registerSessionTools(server, db);
  registerTaskTools(server, db);
  registerRepoTools(server, db);
  registerHealthTools(server, db);
  registerDecisionTools(server, db);
  registerEventTools(server, db);
  registerCommsTools(server, db);
  registerWatcherTools(server, db);
  registerAgentOpsTools(server);

  // Resources — passively available context (sprint state, repo topology)
  server.resource(
    "sprint-state",
    "icn-ops://sprint/current",
    { mimeType: "application/json" },
    async () => {
      try {
        if (!existsSync(SPRINT_FILE)) {
          return {
            contents: [
              {
                uri: "icn-ops://sprint/current",
                mimeType: "application/json",
                text: JSON.stringify({ error: "missing_file", path: SPRINT_FILE }),
              },
            ],
          };
        }
        const text = readFileSync(SPRINT_FILE, "utf-8");
        return {
          contents: [
            {
              uri: "icn-ops://sprint/current",
              mimeType: "application/json",
              text,
            },
          ],
        };
      } catch (e) {
        return {
          contents: [
            {
              uri: "icn-ops://sprint/current",
              mimeType: "application/json",
              text: JSON.stringify({
                error: "read_failed",
                message: e instanceof Error ? e.message : String(e),
              }),
            },
          ],
        };
      }
    }
  );

  server.resource(
    "repo-map",
    "icn-ops://config/repo-map",
    { mimeType: "application/json" },
    async () => {
      try {
        if (!existsSync(REPO_MAP_FILE)) {
          return {
            contents: [
              {
                uri: "icn-ops://config/repo-map",
                mimeType: "application/json",
                text: JSON.stringify({ error: "missing_file", path: REPO_MAP_FILE }),
              },
            ],
          };
        }
        const text = readFileSync(REPO_MAP_FILE, "utf-8");
        return {
          contents: [
            {
              uri: "icn-ops://config/repo-map",
              mimeType: "application/json",
              text,
            },
          ],
        };
      } catch (e) {
        return {
          contents: [
            {
              uri: "icn-ops://config/repo-map",
              mimeType: "application/json",
              text: JSON.stringify({
                error: "read_failed",
                message: e instanceof Error ? e.message : String(e),
              }),
            },
          ],
        };
      }
    }
  );

  server.resource(
    "agent-context-spine",
    "icn-ops://project-index/agent-context-spine",
    { mimeType: "application/json" },
    async () => {
      const uri = "icn-ops://project-index/agent-context-spine";
      try {
        if (!existsSync(AGENT_CONTEXT_SPINE_FILE)) {
          return {
            contents: [
              {
                uri,
                mimeType: "application/json",
                text: JSON.stringify({
                  error: "missing_file",
                  path: AGENT_CONTEXT_SPINE_FILE,
                  hint: "Generate with: python3 scripts/generate-agent-context-spine.py --write",
                }),
              },
            ],
          };
        }
        const text = readFileSync(AGENT_CONTEXT_SPINE_FILE, "utf-8");
        return {
          contents: [{ uri, mimeType: "application/json", text }],
        };
      } catch (e) {
        return {
          contents: [
            {
              uri,
              mimeType: "application/json",
              text: JSON.stringify({
                error: "read_failed",
                message: e instanceof Error ? e.message : String(e),
              }),
            },
          ],
        };
      }
    }
  );

  // Background polling — pre-warms health_cache so first tool calls return instantly
  const timers = [
    startGitPolling(db, ICN_ROOT),
    startClusterPolling(db),
    startBuildsPolling(db),
    startProcessPolling(db),
  ];

  // Connect via stdio transport (Claude Code manages the process lifecycle)
  const transport = new StdioServerTransport();
  await server.connect(transport);

  // Graceful shutdown
  const shutdown = () => {
    timers.forEach(clearInterval);
    db.close();
    process.exit(0);
  };
  process.on("SIGTERM", shutdown);
  process.on("SIGINT", shutdown);
  process.on("SIGHUP", shutdown);
  process.stdin.on("end", shutdown);
  process.stdin.on("close", shutdown);
}

main().catch((err) => {
  console.error("MCP server fatal error:", err);
  process.exit(1);
});
