import { describe, it, expect } from "vitest";
import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  buildAgentMcpContractExport,
  stableSerializeMcpContract,
} from "../diagnostics/tool-schemas.js";

const testFile = fileURLToPath(import.meta.url);
const repoRoot = join(dirname(testFile), "..", "..", "..", "..");
const CONTRACT_PATH = join(repoRoot, "docs", "guides", "developer", "agent-mcp-contracts.json");

describe("agent-mcp-contracts.json drift", () => {
  it("matches buildAgentMcpContractExport() (run: npm run export:contracts in ops/mcp)", () => {
    expect(existsSync(CONTRACT_PATH), `missing ${CONTRACT_PATH}`).toBe(true);
    const onDisk = readFileSync(CONTRACT_PATH, "utf-8");
    const fresh = stableSerializeMcpContract(buildAgentMcpContractExport());
    expect(onDisk).toBe(fresh);
  });
});
