#!/usr/bin/env node
// check-public-state.mjs — the public project-state projection cannot silently
// diverge from canonical repository state.
//
//   node scripts/check-public-state.mjs
//
// Runs after `npm run generate`. Reads docs/status.toml and the generated
// projection and asserts they agree.
//
// ─── The invariant ───────────────────────────────────────────────────────────
//
// #1369's whole point is that the public maturity account is a projection of
// `docs/status.toml`, not a second tracker. `gen-project-state.mjs` enforces
// that at generation time. This script enforces the properties generation
// alone cannot:
//
//   1. Every subsystem in the canonical file reaches the projection, and no
//      extra subsystem appears in it.                              [blocking]
//   2. Only fields on the publish allowlist appear in the output.  [blocking]
//      This is the leak check — `docs/status.toml` carries operator notes
//      (`review_note`, `verified_by`) and a `[summary]` table with a
//      "running since <date>" deployment string that
//      show-readiness-map.md § red lines forbids publishing.
//   3. Maturity and evidence stay on separate axes: no subsystem may carry a
//      band without an evidence class, and vice versa.             [blocking]
//   4. Freshness dates are real dates from the source, never a build
//      timestamp — otherwise a rebuild would look like a
//      re-verification.                                            [blocking]
//   5. Generation is deterministic: generating twice from an unchanged source
//      produces byte-identical output.                             [blocking]
//   6. The newest verification date is not older than a staleness
//      threshold.                                                  [report]
//
// Check 6 reports rather than fails: state going stale is a project-management
// fact, not a broken build, and failing CI for it would only teach people to
// bump the date without re-checking anything — the exact behaviour ADR-0032
// rule 6 warns against.

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { parse as parseToml } from "smol-toml";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");
const repoRoot = path.resolve(websiteRoot, "..");

const statusPath = path.join(repoRoot, "docs", "status.toml");
const generatedPath = path.join(
  websiteRoot,
  "src",
  "data",
  "project-state.generated.json",
);

/** Days after which the newest verification date is reported as stale. */
const STALENESS_DAYS = 120;

/**
 * The complete set of keys the public projection may contain, per subsystem.
 * Anything outside this list is a leak, and the check fails closed: a new
 * field added to the generator has to be added here deliberately, which is the
 * moment to ask whether it should be public at all.
 */
const ALLOWED_SUBSYSTEM_KEYS = new Set([
  "id",
  "name",
  "band",
  "bandLabel",
  "internalStatus",
  "evidence",
  "lastVerified",
  "gaps",
  "crates",
]);

const ALLOWED_TOP_KEYS = new Set([
  "source",
  "sourceLastCommit",
  "newestVerified",
  "oldestVerified",
  "projectedSummary",
  "subsystemCount",
  "bands",
  "subsystems",
]);

/** Vocabularies. Must match gen-project-state.mjs and ADR-0032. */
const VALID_BANDS = new Set([
  "strong",
  "advancing",
  "maturing",
  "behind",
  "notyet",
]);
const VALID_EVIDENCE = new Set([
  "test-backed",
  "ci-backed",
  "fixture-backed",
  "reviewed",
  "asserted",
]);

const errors = [];
const notes = [];

function fail(msg) {
  errors.push(msg);
}

// ─── Load ────────────────────────────────────────────────────────────────────

for (const [label, p] of [
  ["docs/status.toml", statusPath],
  ["src/data/project-state.generated.json", generatedPath],
]) {
  if (!fs.existsSync(p)) {
    console.error(
      `[public-state] FAIL: ${label} not found. Run \`npm run generate\`.`,
    );
    process.exit(1);
  }
}

const status = parseToml(fs.readFileSync(statusPath, "utf-8"));
const generatedRaw = fs.readFileSync(generatedPath, "utf-8");
const generated = JSON.parse(generatedRaw);

// ─── 1 · Subsystem coverage ──────────────────────────────────────────────────

const canonicalIds = new Set(Object.keys(status.subsystems ?? {}));
const projectedIds = new Set(generated.subsystems.map((s) => s.id));

for (const id of canonicalIds) {
  if (!projectedIds.has(id)) {
    fail(
      `subsystem "${id}" is in docs/status.toml but missing from the public projection\n` +
        `      the public page would silently omit a subsystem the project tracks`,
    );
  }
}
for (const id of projectedIds) {
  if (!canonicalIds.has(id)) {
    fail(
      `subsystem "${id}" is in the public projection but not in docs/status.toml\n` +
        `      the website is claiming something canonical state does not say`,
    );
  }
}

// ─── 2 · Field allowlist (the leak check) ────────────────────────────────────

for (const key of Object.keys(generated)) {
  if (!ALLOWED_TOP_KEYS.has(key)) {
    fail(
      `unexpected top-level field "${key}" in the public projection\n` +
        `      add it to ALLOWED_TOP_KEYS only after confirming it is safe to publish`,
    );
  }
}

for (const s of generated.subsystems) {
  for (const key of Object.keys(s)) {
    if (!ALLOWED_SUBSYSTEM_KEYS.has(key)) {
      fail(
        `unexpected field "${key}" on subsystem "${s.id}" in the public projection\n` +
          `      docs/status.toml carries operator-only fields (review_note, verified_by);` +
          ` none of them may reach a public surface`,
      );
    }
  }
}

// `[summary]` must not be projected: it contains a deployment liveness string
// that show-readiness-map.md § red lines forbids publishing, plus two figures
// contradicted by docs/PHASE_PROGRESS.md.
if (generated.projectedSummary !== false) {
  fail(
    `the [summary] table is being projected\n` +
      `      it carries a "running since <date>" deployment claim and two figures` +
      ` contradicted elsewhere; see docs/reference/project-index/public-state-projection.md §2`,
  );
}

const serialised = JSON.stringify(generated);
for (const banned of [
  "review_note",
  "verified_by",
  "NEEDS OPS RE-CONFIRMATION",
  "running since",
]) {
  if (serialised.includes(banned)) {
    fail(
      `operator-only or forbidden string "${banned}" appears in the public projection`,
    );
  }
}

// ─── 3 · The two axes stay separate ──────────────────────────────────────────

for (const s of generated.subsystems) {
  if (!VALID_BANDS.has(s.band)) {
    fail(`subsystem "${s.id}" has unknown maturity band "${s.band}"`);
  }
  if (!s.evidence || !VALID_EVIDENCE.has(s.evidence.id)) {
    fail(
      `subsystem "${s.id}" has no valid evidence class (got ${JSON.stringify(s.evidence?.id)})\n` +
        `      maturity and evidence are orthogonal and must both be carried —` +
        ` see docs/reference/project-index/claim-boundaries.md`,
    );
  }
  if (!s.evidence?.detail || s.evidence.detail.length < 20) {
    fail(
      `subsystem "${s.id}" evidence class has no explanatory detail\n` +
        `      the wording is what stops "test-backed" reading as "has been run by an institution"`,
    );
  }
}

// ─── 4 · Freshness is sourced, not stamped ───────────────────────────────────

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;
for (const field of ["newestVerified", "oldestVerified"]) {
  if (!DATE_RE.test(generated[field] ?? "")) {
    fail(
      `${field} is not a plain YYYY-MM-DD date (got ${JSON.stringify(generated[field])})`,
    );
  }
}

const canonicalDates = Object.values(status.subsystems ?? {})
  .map((s) => String(s.last_verified))
  .sort();
if (generated.newestVerified !== canonicalDates[canonicalDates.length - 1]) {
  fail(
    `newestVerified (${generated.newestVerified}) does not match the newest last_verified ` +
      `in docs/status.toml (${canonicalDates[canonicalDates.length - 1]})`,
  );
}

// A build timestamp would make every rebuild look like a re-verification.
if (/T\d{2}:\d{2}/.test(generated.newestVerified ?? "")) {
  fail(`newestVerified looks like a build timestamp rather than a source date`);
}

// ─── 5 · Determinism ─────────────────────────────────────────────────────────

try {
  execFileSync("node", [path.join(here, "gen-project-state.mjs")], {
    cwd: websiteRoot,
    stdio: "pipe",
  });
  const regenerated = fs.readFileSync(generatedPath, "utf-8");
  if (regenerated !== generatedRaw) {
    fail(
      `regenerating from an unchanged source produced different output\n` +
        `      the projection must be deterministic or every build churns the site`,
    );
  }
} catch (err) {
  fail(`re-running the generator failed: ${err.message}`);
}

// ─── 6 · Staleness, reported ─────────────────────────────────────────────────

const newest = new Date(generated.newestVerified + "T00:00:00Z");
const ageDays = Math.floor((Date.now() - newest.getTime()) / 86400000);
if (ageDays > STALENESS_DAYS) {
  notes.push(
    `newest subsystem verification is ${ageDays} days old (${generated.newestVerified}).\n` +
      `      The public page shows this date, so staleness is visible rather than hidden —\n` +
      `      but ${STALENESS_DAYS}+ days suggests docs/status.toml is due a re-verification pass.`,
  );
}

// ─── Report ──────────────────────────────────────────────────────────────────

for (const n of notes) console.log("[public-state] NOTE: " + n);

if (errors.length > 0) {
  console.error(`\n[public-state] FAIL — ${errors.length} problem(s):`);
  for (const e of errors) console.error("  ✗ " + e);
  console.error(
    `\n  Reference: docs/reference/project-index/public-state-projection.md`,
  );
  process.exit(1);
}

console.log(
  `[public-state] PASS — ${generated.subsystems.length} subsystems projected from ` +
    `${generated.source}, no operator-only fields leaked, both claim axes present, ` +
    `freshness sourced (${generated.newestVerified}, ${ageDays}d), generation deterministic.`,
);
