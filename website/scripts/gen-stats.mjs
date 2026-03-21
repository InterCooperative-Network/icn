#!/usr/bin/env node
// Generate src/data/stats.json at build time from the monorepo.
// Runs as a prebuild step — keeps stats.json gitignored.

import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const cwd = process.cwd();
const repoRoot = fs.existsSync(path.resolve(cwd, "icn", "crates"))
  ? cwd
  : path.resolve(cwd, "..");

const cratesDir = path.join(repoRoot, "icn", "crates");
const crates = fs.existsSync(cratesDir)
  ? fs.readdirSync(cratesDir).filter((d) =>
      fs.statSync(path.join(cratesDir, d)).isDirectory()
    ).length
  : 0;

function run(cmd, opts = {}) {
  try {
    return execSync(cmd, { encoding: "utf-8", ...opts }).trim();
  } catch {
    return "";
  }
}

// Count Rust lines of code (non-blank, non-comment lines in .rs files)
function countRustLoc() {
  try {
    const out = run(
      `find "${path.join(repoRoot, "icn", "crates")}" -name "*.rs" | xargs wc -l 2>/dev/null | tail -1`
    );
    const match = out.match(/(\d+)/);
    return match ? parseInt(match[1], 10) : 0;
  } catch {
    return 0;
  }
}

// Count #[test] functions as a proxy for test count
function countTests() {
  try {
    const out = run(
      `grep -r "#\\[test\\]" "${path.join(repoRoot, "icn", "crates")}" --include="*.rs" | wc -l`
    );
    return parseInt(out.trim(), 10) || 0;
  } catch {
    return 0;
  }
}

// Count merged PRs: gh API (accurate), fallback to git merge commits
function countMergedPRs() {
  const ghOut = run(
    `gh pr list --state merged --limit 1000 --json number --jq 'length' -R InterCooperative-Network/icn 2>/dev/null`,
    { cwd: repoRoot }
  );
  if (ghOut && /^\d+$/.test(ghOut)) return parseInt(ghOut, 10);
  try {
    const out = run(
      `git log --oneline --merges --format="%h" 2>/dev/null | wc -l`,
      { cwd: repoRoot }
    );
    return parseInt(out.trim(), 10) || 0;
  } catch {
    return 0;
  }
}

// Active branches: gh API first, fallback to git branch -r
function countActiveBranches() {
  const ghOut = run(
    `gh api "repos/InterCooperative-Network/icn/branches?per_page=100" --paginate --jq 'length' 2>/dev/null | awk '{s+=$1} END{print s}'`,
    { cwd: repoRoot }
  );
  if (ghOut && /^\d+$/.test(ghOut)) return parseInt(ghOut, 10);
  const gitOut = run(`git branch -r 2>/dev/null | grep -v HEAD | wc -l`, { cwd: repoRoot });
  return parseInt(gitOut.trim(), 10) || 0;
}

// Doc files: count all .md files under docs/ at repo root
function countDocFiles() {
  const docsDir = path.join(repoRoot, "docs");
  if (!fs.existsSync(docsDir)) return 0;
  const out = run(`find "${docsDir}" -name "*.md" | wc -l`);
  return parseInt(out.trim(), 10) || 0;
}

const latestCommit = run("git rev-parse --short HEAD", { cwd: repoRoot }) || "unknown";
const latestCommitFull = run("git rev-parse HEAD", { cwd: repoRoot }) || "";

const rustLinesOfCode = countRustLoc();
const testCount = countTests();
const mergedPRs = countMergedPRs();
const activeBranches = countActiveBranches();
const docFiles = countDocFiles();

const stats = {
  crates,
  rustLinesOfCode,
  testCount,
  mergedPRs,
  activeBranches,
  docFiles,
  latestCommit,
  latestCommitFull,
  syncedAt: new Date().toISOString(),
};

const outPath = path.join(
  fs.existsSync(path.resolve(cwd, "src")) ? cwd : path.join(repoRoot, "website"),
  "src",
  "data",
  "stats.json"
);

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(stats, null, 2) + "\n");
console.log(
  `[gen-stats] ${crates} crates · ${rustLinesOfCode.toLocaleString()} LoC · ${testCount} tests · ${mergedPRs} PRs · ${activeBranches} branches · ${docFiles} docs → ${outPath}`
);