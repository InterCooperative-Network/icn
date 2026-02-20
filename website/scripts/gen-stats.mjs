#!/usr/bin/env node
// Generate src/data/stats.json at build time from the monorepo.
// Runs as a prebuild step — keeps stats.json gitignored.

import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const cwd = process.cwd();
// Handle both `cd website && npm run build` and `npm --prefix website run build`
const repoRoot = fs.existsSync(path.resolve(cwd, "icn", "crates"))
  ? cwd
  : path.resolve(cwd, "..");

const cratesDir = path.join(repoRoot, "icn", "crates");
const crates = fs.existsSync(cratesDir)
  ? fs.readdirSync(cratesDir).filter((d) =>
      fs.statSync(path.join(cratesDir, d)).isDirectory()
    ).length
  : 0;

let latestCommit = "unknown";
try {
  latestCommit = execSync("git rev-parse --short HEAD", {
    cwd: repoRoot,
    encoding: "utf-8",
  }).trim();
} catch {
  // git not available in CI — fine
}

const stats = {
  crates,
  rustLinesOfCode: 0, // expensive to compute; skip in build
  testCount: 0,
  mergedPRs: 0,
  latestCommit,
  latestCommitFull: "",
  syncedAt: new Date().toISOString(),
};

const outPath = path.join(
  // Write to src/data/ relative to the website directory
  fs.existsSync(path.resolve(cwd, "src")) ? cwd : path.join(repoRoot, "website"),
  "src",
  "data",
  "stats.json"
);

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(stats, null, 2) + "\n");
console.log(`[gen-stats] Wrote ${outPath} (${crates} crates)`);
