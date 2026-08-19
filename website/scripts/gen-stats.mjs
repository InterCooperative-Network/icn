#!/usr/bin/env node
// gen-stats.mjs — the small set of repository numbers the public site is
// willing to stand behind.
//
// OUTPUT: website/src/data/stats.json (gitignored; also refreshed by the
//         sync-stats.yml cron).
//
// ─── What was removed, and why (#1368) ───────────────────────────────────────
//
// This script used to emit six numbers. Four were removed because their source
// or freshness could not be trusted, and #1368 is explicit: "Do not publish
// repository statistics unless their source and freshness are trustworthy."
//
//   rustLinesOfCode — the old implementation ran `find | xargs wc -l`, which
//                     counts blank and comment lines, contradicting its own
//                     comment. It also disagreed with both docs/status.toml
//                     (~414K) and docs/PHASE_PROGRESS.md (~458,000). Three
//                     numbers, no arbiter. LoC is not a public claim anyway.
//
//   testCount       — counted `#[test]` occurrences by grep, which misses
//                     table-driven cases and double-counts macros.
//                     docs/status.toml marks its own test count "stale" and
//                     docs/PHASE_PROGRESS.md carries an incompatible baseline.
//
//   mergedPRs       — mechanically obtainable, but a merged-PR count is a
//                     vanity metric. #1368 rules those out, and a high count
//                     invites exactly the "activity means readiness" inference
//                     the project is trying not to make.
//
//   activeBranches  — actively misleading. It counted every remote ref,
//                     including long-dead ones, and presented the total as a
//                     sign of life.
//
//   docFiles        — replaced by gen-docs-classification.mjs, which reports
//                     how many documents the site actually *publishes* rather
//                     than how many .md files exist on disk.
//
// What is left is mechanical, single-sourced, and checkable by a reader in
// one command, which is the only bar a public number needs to clear.

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");
const repoRoot = path.resolve(websiteRoot, "..");

function run(args, opts = {}) {
  try {
    return execFileSync(args[0], args.slice(1), {
      cwd: repoRoot,
      encoding: "utf-8",
      ...opts,
    }).trim();
  } catch {
    return "";
  }
}

// Crate count: a directory count under icn/crates. Verifiable with
// `ls icn/crates | wc -l`. No interpretation, no aggregation.
const cratesDir = path.join(repoRoot, "icn", "crates");
const crates = fs.existsSync(cratesDir)
  ? fs
      .readdirSync(cratesDir)
      .filter((d) => fs.statSync(path.join(cratesDir, d)).isDirectory()).length
  : 0;

const latestCommit = run(["git", "rev-parse", "--short", "HEAD"]) || "unknown";
const latestCommitFull = run(["git", "rev-parse", "HEAD"]) || "";
// Commit date, not build date: rebuilding does not make the repository newer.
const latestCommitDate = run(["git", "log", "-1", "--format=%cs"]) || "";

const stats = {
  crates,
  latestCommit,
  latestCommitFull,
  latestCommitDate,
  syncedAt: new Date().toISOString(),
  // Carried so a consumer can tell at a glance which fields are safe to show.
  trust: {
    crates: "mechanical: directory count under icn/crates",
    latestCommit: "mechanical: git rev-parse HEAD",
    latestCommitDate: "mechanical: committer date of HEAD",
  },
};

const outPath = path.join(websiteRoot, "src", "data", "stats.json");
fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(stats, null, 2) + "\n");
console.log(
  `[gen-stats] ${crates} crates · HEAD ${latestCommit} (${latestCommitDate}) → src/data/stats.json`,
);
