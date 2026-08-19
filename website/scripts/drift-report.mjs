#!/usr/bin/env node
// drift-report.mjs — surface the slow-moving problems the PR gate cannot see.
//
//   node scripts/drift-report.mjs [--links-report <path>]
//
// Run after `npm run build && npm run check`. Reads artifacts those already
// produced; runs nothing itself.
//
// ─── What this is for ────────────────────────────────────────────────────────
//
// The PR gate blocks on things a diff can break. Three things degrade with the
// passage of time instead, so no pull request ever fails because of them:
//
//   · docs/status.toml going stale, leaving the public maturity page showing
//     verification dates nobody has revisited;
//   · the published/withheld documentation split drifting from its recorded
//     baseline as documentation is added elsewhere in the repo;
//   · stale in-document anchors accumulating across the docs corpus.
//
// ─── Why it exits non-zero ───────────────────────────────────────────────────
//
// A scheduled job gates no merge, so a red cross costs nobody anything — which
// makes it the one place a warning can afford to be loud. The failure mode this
// avoids is the usual one for maintenance workflows: `continue-on-error`
// everywhere, a permanently green weekly run, and the actual signal buried on
// page four of a log nobody opens.
//
// So: non-blocking for development, hard to miss operationally. The PR gate
// never runs this, and none of these conditions can fail an ordinary change.
//
// Writes a GitHub job summary and ::warning annotations when running in
// Actions; prints the same content locally.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");

function flag(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : fallback;
}

const linksReportPath = flag(
  "links-report",
  path.join(websiteRoot, "link-report.json"),
);

/** Days after which the newest subsystem verification is called stale. */
const STATE_STALE_DAYS = 120;
/** Absolute change in published-doc count that is worth a human look. */
const DOC_COUNT_DRIFT = 10;

const findings = [];

function drift(title, detail, action) {
  findings.push({ title, detail, action });
}

// ─── 1 · Canonical state freshness ───────────────────────────────────────────

const statePath = path.join(
  websiteRoot,
  "src",
  "data",
  "project-state.generated.json",
);
if (fs.existsSync(statePath)) {
  const state = JSON.parse(fs.readFileSync(statePath, "utf-8"));
  const age = Math.floor(
    (Date.now() - new Date(state.newestVerified + "T00:00:00Z").getTime()) /
      86400000,
  );
  if (age > STATE_STALE_DAYS) {
    drift(
      "Canonical project state is stale",
      `Newest subsystem verification in ${state.source} is ${age} days old (${state.newestVerified}). ` +
        `The public maturity page shows this date, so the staleness is visible to readers rather than hidden — ` +
        `but every subsystem claim on the site is now resting on a ${age}-day-old check.`,
      `Re-verify subsystems against source and update last_verified in ${state.source}.`,
    );
  }
} else {
  drift(
    "Project-state projection missing",
    "src/data/project-state.generated.json was not found.",
    "Run `npm run generate`.",
  );
}

// ─── 2 · Documentation publication drift ─────────────────────────────────────

const manifestPath = path.join(websiteRoot, "public-docs-manifest.json");
const baselinePath = path.join(websiteRoot, "docs-boundary-baseline.json");
if (fs.existsSync(manifestPath) && fs.existsSync(baselinePath)) {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf-8"));
  const baseline = JSON.parse(fs.readFileSync(baselinePath, "utf-8"));
  const dPub = manifest.counts.published - baseline.counts.published;
  const dHeld = manifest.counts.withheld - baseline.counts.withheld;

  if (Math.abs(dPub) >= DOC_COUNT_DRIFT || Math.abs(dHeld) >= DOC_COUNT_DRIFT) {
    const sign = (n) => (n > 0 ? `+${n}` : `${n}`);
    drift(
      "Public documentation surface has drifted from its baseline",
      `Published ${sign(dPub)}, withheld ${sign(dHeld)} against the baseline recorded ${baseline.recordedAt} ` +
        `(now ${manifest.counts.published} published / ${manifest.counts.withheld} withheld). ` +
        `This is expected when documentation is added or reclassified — the point is that somebody sees it.`,
      "Review `public-docs-manifest.json` in this run's artifacts, confirm the delta is intended, " +
        "then refresh with `npm run check:docs -- --update-baseline`.",
    );
  }
} else {
  drift(
    "Documentation manifest or baseline missing",
    "public-docs-manifest.json or docs-boundary-baseline.json was not found.",
    "Run `npm run build && npm run check:docs`.",
  );
}

// ─── 3 · Stale anchors in the docs corpus ────────────────────────────────────

if (fs.existsSync(linksReportPath)) {
  const links = JSON.parse(fs.readFileSync(linksReportPath, "utf-8"));
  if (links.staleDocFragments > 0) {
    drift(
      `${links.staleDocFragments} stale anchor(s) in rendered documentation`,
      `Table-of-contents entries pointing at headings that no longer exist. These are defects in source ` +
        `markdown rather than in the website, which is why they do not block a pull request.\n\n` +
        links.staleDocFragmentSamples.map((s) => `  · ${s}`).join("\n"),
      "Fix the anchors in the source documents. When the count reaches zero, flip " +
        "DOCS_FRAGMENTS_BLOCKING in scripts/check-links.mjs so it cannot climb back.",
    );
  }
} else {
  drift(
    "Link report missing",
    `${path.relative(websiteRoot, linksReportPath)} was not found.`,
    "Run `node scripts/check-links.mjs --report-json link-report.json`.",
  );
}

// ─── Output ──────────────────────────────────────────────────────────────────

const summaryFile = process.env.GITHUB_STEP_SUMMARY;
const lines = [];

if (findings.length === 0) {
  lines.push("## Website drift watch — clear");
  lines.push("");
  lines.push(
    "Canonical project state is current, the public documentation surface matches its " +
      "baseline, and no stale anchors were found.",
  );
  console.log(
    "[drift] no drift. Project state current, docs surface at baseline, no stale anchors.",
  );
} else {
  lines.push(
    `## Website drift watch — ${findings.length} item(s) need attention`,
  );
  lines.push("");
  lines.push(
    "_Nothing here blocks a merge. These are conditions that worsen with time rather " +
      "than with any particular change, so no pull request would ever surface them._",
  );
  lines.push("");
  for (const f of findings) {
    lines.push(`### ${f.title}`);
    lines.push("");
    lines.push(f.detail);
    lines.push("");
    lines.push(`**What to do:** ${f.action}`);
    lines.push("");
    // Annotations put the item in the run's header, not only in the log.
    console.log(
      `::warning title=${f.title}::${f.detail.split("\n")[0]} — ${f.action}`,
    );
  }
  console.error(`\n[drift] ${findings.length} drift item(s):`);
  for (const f of findings) {
    console.error(`  ! ${f.title}`);
    console.error(`    ${f.detail.split("\n")[0]}`);
    console.error(`    → ${f.action}\n`);
  }
}

if (summaryFile) {
  fs.appendFileSync(summaryFile, lines.join("\n") + "\n");
}

// Non-zero on drift, deliberately — see the header note.
process.exit(findings.length === 0 ? 0 : 1);
