/**
 * Regenerate docs/guides/developer/agent-mcp-contracts.json from TypeScript source.
 * Run: npm run export:contracts (from ops/mcp). No MCP server required.
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  buildAgentMcpContractExport,
  stableSerializeMcpContract,
} from "../diagnostics/tool-schemas.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "..", "..", "..", "..");
const outPath = join(repoRoot, "docs", "guides", "developer", "agent-mcp-contracts.json");

mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, stableSerializeMcpContract(buildAgentMcpContractExport()), "utf-8");
console.log(`Wrote ${outPath}`);
