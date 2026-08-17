#!/usr/bin/env node
// check-docs-boundary.mjs — the public documentation boundary is a security
// boundary. This asserts it held.
//
//   node scripts/check-docs-boundary.mjs [--manifest <path>] [--quiet]
//
// Run AFTER `npm run build`. Reads `dist/` and the generated classification.
//
// ─── The invariant ───────────────────────────────────────────────────────────
//
// `docs/registry.toml` decides, for every markdown file under `docs/`, whether
// it may be published and which public layer it belongs to.
// `gen-docs-classification.mjs` derives that decision; this script verifies the
// built site actually obeyed it.
//
// The failure this exists to prevent is concrete and already happened once: the
// docs route published all 965 files with no filter, which put
// `docs/internal/` — a directory whose own README says it is not public —
// legal-exposure analysis, gap analyses, partner operational material, and 156
// agent session logs on the public internet. A refactor of the route, a change
// to a registry prefix, or a new top-level docs directory could silently do it
// again. Nothing else in the repository would notice.
//
// ─── Checks ──────────────────────────────────────────────────────────────────
//
//   1. Withheld documents produced NO page in dist/.            [blocking]
//   2. Published documents each produced exactly one page.      [blocking]
//   3. Archive pages carry noindex and the historical banner.   [blocking]
//   4. Non-archive pages do NOT carry noindex.                  [blocking]
//   5. No page under a forbidden path class appears at all.     [blocking]
//   6. The withheld/published split matches the committed
//      baseline, or the difference is reported for review.      [report]
//
// Check 6 is deliberately not blocking: adding documentation legitimately
// changes the counts. What matters is that a human sees the delta in the PR,
// which is what the printed report and the manifest diff give.
//
// ─── The manifest ────────────────────────────────────────────────────────────
//
// Writes `public-docs-manifest.json`: one row per document with its source
// path, public path, layer, canonical flag, indexing policy, registry status,
// replacement link, and — for withheld documents — the reason. It is the
// reviewable artifact for "what does this change actually publish?", and it is
// useful beyond the website: anything that needs to know ICN's public
// documentation surface can read it instead of re-deriving it.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");
const distRoot = path.join(websiteRoot, "dist");
const classificationPath = path.join(
  websiteRoot,
  "src",
  "data",
  "docs-classification.generated.json",
);
const baselinePath = path.join(websiteRoot, "docs-boundary-baseline.json");

const args = process.argv.slice(2);
const quiet = args.includes("--quiet");
const manifestArg = args.indexOf("--manifest");
const manifestPath =
  manifestArg >= 0
    ? path.resolve(args[manifestArg + 1])
    : path.join(websiteRoot, "public-docs-manifest.json");

/**
 * Path classes that must never appear in public output, independent of what
 * the registry currently says. This is the belt to the registry's braces: if
 * someone adds a permissive prefix rule, or a doc moves into one of these
 * directories without its registry row following, the site still must not
 * publish it.
 *
 * Each entry names why, so the failure message explains itself.
 */
const FORBIDDEN_PATH_CLASSES = [
  {
    prefix: "docs/internal/",
    why: "docs/internal/README.md states this directory is not part of the public-facing documentation",
  },
  {
    prefix: "docs/pilots/",
    why: "partner-specific operational material; institution-specific content belongs to that institution's package",
  },
  {
    prefix: "docs/summit/",
    why: "partner/event-specific draft material naming a real external organisation",
  },
  {
    prefix: "docs/development/sessions/",
    why: "per-session agent working logs; archaeology, not documentation",
  },
];

function fail(list, msg) {
  list.push(msg);
}

// ─── Load inputs ─────────────────────────────────────────────────────────────

if (!fs.existsSync(distRoot)) {
  console.error(
    "[docs-boundary] FAIL: dist/ not found. Run `npm run build` before this check.",
  );
  process.exit(1);
}
if (!fs.existsSync(classificationPath)) {
  console.error(
    "[docs-boundary] FAIL: classification not generated. Run `npm run generate`.",
  );
  process.exit(1);
}

const classification = JSON.parse(fs.readFileSync(classificationPath, "utf-8"));

/** Slug → true when dist contains a rendered page for it. */
function builtPagePath(slug) {
  const candidate = path.join(distRoot, "docs", slug, "index.html");
  return fs.existsSync(candidate) ? candidate : null;
}

const errors = [];
const rows = [];

let publishedBuilt = 0;
let withheldLeaked = 0;
let missingPages = 0;

for (const [slug, entry] of Object.entries(classification.entries)) {
  const pagePath = builtPagePath(slug);

  if (entry.publish !== true) {
    // ─── Check 1 · withheld documents must not be published ────────────────
    if (pagePath) {
      withheldLeaked += 1;
      fail(
        errors,
        `withheld document was published: /docs/${slug}\n` +
          `      source: ${entry.rel}\n` +
          `      reason it is withheld: ${entry.withheldReason}\n` +
          `      fix: the docs route must consult classify() before emitting a page`,
      );
    }
    rows.push({
      source: entry.rel,
      publicPath: null,
      published: false,
      withheldReason: entry.withheldReason,
    });
    continue;
  }

  // ─── Check 2 · published documents must actually be built ────────────────
  if (!pagePath) {
    missingPages += 1;
    fail(
      errors,
      `document is classified publishable but produced no page: /docs/${slug}\n` +
        `      source: ${entry.rel}`,
    );
    rows.push({
      source: entry.rel,
      publicPath: `/docs/${slug}`,
      published: false,
      withheldReason: "classified publishable but no page was built",
    });
    continue;
  }

  publishedBuilt += 1;
  const html = fs.readFileSync(pagePath, "utf-8");
  const hasNoindex = /<meta\s+name="robots"\s+content="[^"]*noindex/i.test(
    html,
  );
  const hasArchiveBanner =
    /Historical document — not current project state/.test(html);

  if (entry.layer === "archive") {
    // ─── Check 3 · archive pages must be unmistakable and de-indexed ───────
    if (!hasNoindex) {
      fail(
        errors,
        `archive page is missing noindex: /docs/${slug}\n` +
          `      a historical page without noindex competes with current documentation in search results`,
      );
    }
    if (!hasArchiveBanner) {
      fail(
        errors,
        `archive page is missing its historical banner: /docs/${slug}\n` +
          `      a reader arriving from a search engine has no way to know this is not current`,
      );
    }
  } else {
    // ─── Check 4 · current pages must stay indexable ───────────────────────
    if (hasNoindex) {
      fail(
        errors,
        `current documentation page carries noindex: /docs/${slug} (layer=${entry.layer})\n` +
          `      this removes canonical documentation from search results`,
      );
    }
    if (hasArchiveBanner) {
      fail(
        errors,
        `current documentation page renders the historical banner: /docs/${slug} (layer=${entry.layer})`,
      );
    }
  }

  rows.push({
    source: entry.rel,
    publicPath: `/docs/${slug}`,
    published: true,
    layer: entry.layer,
    canonical: entry.canonical,
    registryStatus: entry.status,
    truthClass: entry.truthClass,
    indexing: hasNoindex ? "noindex, follow" : "index",
    lastReviewed: entry.lastReviewed,
    supersededBy: entry.supersededBy,
    archiveReason: entry.archiveReason,
  });
}

// ─── Check 5 · forbidden path classes must not appear at all ────────────────

for (const forbidden of FORBIDDEN_PATH_CLASSES) {
  const leaked = rows.filter(
    (r) => r.published && r.source.startsWith(forbidden.prefix),
  );
  for (const row of leaked) {
    fail(
      errors,
      `forbidden path class published: ${row.publicPath}\n` +
        `      source: ${row.source}\n` +
        `      ${forbidden.prefix} must never be public — ${forbidden.why}`,
    );
  }
}

// ─── Write the manifest ──────────────────────────────────────────────────────

rows.sort((a, b) => a.source.localeCompare(b.source));

const manifest = {
  schema: "icn-public-docs-manifest/v1",
  generatedFrom: classification.source,
  counts: {
    total: rows.length,
    published: rows.filter((r) => r.published).length,
    withheld: rows.filter((r) => !r.published).length,
    byLayer: rows.reduce((acc, r) => {
      if (!r.published) return acc;
      acc[r.layer] = (acc[r.layer] ?? 0) + 1;
      return acc;
    }, {}),
  },
  withheldByReason: classification.withheldByReason,
  documents: rows,
};

fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");

// ─── Check 6 · baseline drift, reported not enforced ─────────────────────────

let drift = null;
if (fs.existsSync(baselinePath)) {
  const baseline = JSON.parse(fs.readFileSync(baselinePath, "utf-8"));
  const d = {
    published: manifest.counts.published - baseline.counts.published,
    withheld: manifest.counts.withheld - baseline.counts.withheld,
  };
  if (d.published !== 0 || d.withheld !== 0) drift = { baseline, d };
}

// ─── Report ──────────────────────────────────────────────────────────────────

if (!quiet) {
  const c = manifest.counts;
  console.log(
    `[docs-boundary] ${c.total} documents · ${c.published} published ` +
      `(${Object.entries(c.byLayer)
        .map(([k, v]) => `${k} ${v}`)
        .join(" · ")}) · ` +
      `${c.withheld} withheld`,
  );
  console.log(
    `[docs-boundary] manifest → ${path.relative(websiteRoot, manifestPath)}`,
  );
}

if (drift) {
  const sign = (n) => (n > 0 ? `+${n}` : `${n}`);
  console.log(
    `[docs-boundary] NOTE: published ${sign(drift.d.published)}, ` +
      `withheld ${sign(drift.d.withheld)} vs baseline ` +
      `(${path.basename(baselinePath)}, recorded ${drift.baseline.recordedAt}).`,
  );
  console.log(
    `[docs-boundary]       A change here is expected when documentation is added or reclassified. ` +
      `Confirm the delta is intended, then refresh the baseline with \`npm run check:docs-boundary -- --update-baseline\`.`,
  );
}

if (args.includes("--update-baseline")) {
  fs.writeFileSync(
    baselinePath,
    JSON.stringify(
      {
        schema: "icn-public-docs-baseline/v1",
        note: "Expected shape of the public documentation surface. Drift is reported, not enforced — refresh deliberately when documentation is added or reclassified.",
        recordedAt: new Date().toISOString().slice(0, 10),
        counts: manifest.counts,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(
    `[docs-boundary] baseline refreshed → ${path.basename(baselinePath)}`,
  );
}

if (errors.length > 0) {
  console.error(
    `\n[docs-boundary] FAIL — ${errors.length} boundary violation(s):`,
  );
  for (const e of errors) console.error("  ✗ " + e);
  console.error(
    `\n  The public documentation boundary is a security boundary. See\n` +
      `  docs/design/PUBLIC_SITE_IA.md §7 and website/scripts/gen-docs-classification.mjs.`,
  );
  process.exit(1);
}

console.log(
  `[docs-boundary] PASS — ${publishedBuilt} published pages built, ` +
    `${manifest.counts.withheld} withheld and none leaked, ` +
    `archive pages carry noindex and a banner, current pages do not.`,
);
