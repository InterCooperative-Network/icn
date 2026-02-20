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
import { readFileSync } from "fs";
import { join } from "path";

const ICN_ROOT = process.env["ICN_ROOT"] ?? "/home/ubuntu/projects";
const SPRINT_FILE = join(ICN_ROOT, "icn-ops/state/sprint/current.json");
const REPO_MAP_FILE = join(ICN_ROOT, "icn-ops/state/config/repo-map.json");

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

  // Resources — passively available context (sprint state, repo topology)
  server.resource(
    "sprint-state",
    "icn-ops://sprint/current",
    { mimeType: "application/json" },
    async () => ({
      contents: [
        {
          uri: "icn-ops://sprint/current",
          mimeType: "application/json",
          text: readFileSync(SPRINT_FILE, "utf-8"),
        },
      ],
    })
  );

  server.resource(
    "repo-map",
    "icn-ops://config/repo-map",
    { mimeType: "application/json" },
    async () => ({
      contents: [
        {
          uri: "icn-ops://config/repo-map",
          mimeType: "application/json",
          text: readFileSync(REPO_MAP_FILE, "utf-8"),
        },
      ],
    })
  );

  // Background polling — pre-warms health_cache so first tool calls return instantly
  const timers = [
    startGitPolling(db, ICN_ROOT),
    startClusterPolling(db),
    startBuildsPolling(db),
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
}

main().catch((err) => {
  console.error("MCP server fatal error:", err);
  process.exit(1);
});
