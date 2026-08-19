#!/usr/bin/env node
// check-links.mjs — internal links and compatibility redirects resolve.
//
//   node scripts/check-links.mjs
//
// Run AFTER `npm run build`. Reads only `dist/`; makes no network requests.
//
// ─── Scope, and why it stops where it does ───────────────────────────────────
//
// Internal links only. An external link checker on every pull request is a
// reliability tax: it fails when someone else's site is slow, rate-limits the
// runner, and trains people to re-run CI until it goes green — which is worse
// than not checking at all, because it also erodes trust in the checks that do
// mean something.
//
// Internal links are different: they are fully determined by the build, so a
// broken one is always a real defect and never a flake.
//
// ─── The invariant ───────────────────────────────────────────────────────────
//
//   1. Every internal href resolves to a page in dist/.            [blocking]
//   2. Every compatibility redirect still exists and still points where it
//      was meant to.                                               [blocking]
//   3. Fragment targets exist.                             [blocking on site
//                                                   pages, report in /docs/*]
//
// ─── Why check 3 is split ────────────────────────────────────────────────────
//
// A broken fragment on a page this repository authors (`website/src/pages/*`)
// is our defect and blocks. A broken fragment inside rendered documentation is
// a stale table-of-contents entry in a source markdown file — a real defect,
// but in a different maintenance surface, and there are currently 15 of them
// across a 750-page corpus. Blocking on those would mean either fixing 15
// unrelated documents inside a website change, or adding an allowlist that
// rots. Neither is worth it, so they are counted and reported.
//
// Promotion criterion: when the reported count reaches zero, flip
// DOCS_FRAGMENTS_BLOCKING to true so it cannot climb back up.
//
// Check 2 is the one that motivated this. The #2608 pass collapsed /roadmap
// into /whats-real-now and /community into /get-involved, keeping both as
// redirects so external links survive. A later refactor of astro.config.mjs
// would break those silently — nothing renders differently, and no test
// covers a URL that no longer exists in the source tree.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");
const distRoot = path.join(websiteRoot, "dist");

/**
 * Redirects that exist for compatibility and must keep working. Kept here
 * rather than read from astro.config.mjs on purpose: this is the independent
 * statement of intent that the config is checked against. If the two disagree,
 * that disagreement is the finding.
 */
const REQUIRED_REDIRECTS = [
  {
    from: "/roadmap",
    to: "/whats-real-now",
    why: "#2608 merged the roadmap into the project-state page",
  },
  {
    from: "/community",
    to: "/get-involved",
    why: "#2608 merged the community page into participation routes",
  },
  {
    from: "/community/cooperatives",
    to: "/for-cooperatives",
    why: "pre-Astro URL",
  },
  {
    from: "/docs/cooperatives/getting-started",
    to: "/for-cooperatives",
    why: "pre-Astro URL, reported 404 by an outside developer",
  },
];

/**
 * Flip to true once the reported fragment count in rendered documentation
 * reaches zero. See the header note.
 */
const DOCS_FRAGMENTS_BLOCKING = false;

/** Optional structured output, for the weekly drift report. */
const REPORT_JSON = (() => {
  const i = process.argv.indexOf("--report-json");
  return i >= 0 ? process.argv[i + 1] : null;
})();

const errors = [];
const reported = [];

if (!fs.existsSync(distRoot)) {
  console.error("[links] FAIL: dist/ not found. Run `npm run build` first.");
  process.exit(1);
}

// ─── Index every built page ──────────────────────────────────────────────────

/** Set of routes dist actually serves, normalised without a trailing slash. */
const routes = new Set();
/** Route → absolute file path, for fragment checking. */
const routeFile = new Map();

function indexDist(dir, prefix = "") {
  for (const entry of fs.readdirSync(dir)) {
    const abs = path.join(dir, entry);
    if (fs.statSync(abs).isDirectory()) {
      indexDist(abs, `${prefix}/${entry}`);
      continue;
    }
    if (entry === "index.html") {
      const route = prefix === "" ? "/" : prefix;
      routes.add(route);
      routeFile.set(route, abs);
    } else {
      routes.add(`${prefix}/${entry}`);
    }
  }
}
indexDist(distRoot);

// ─── 2 · Compatibility redirects ─────────────────────────────────────────────

for (const redirect of REQUIRED_REDIRECTS) {
  const file = routeFile.get(redirect.from);
  if (!file) {
    errors.push(
      `compatibility redirect missing: ${redirect.from}\n` +
        `      expected it to redirect to ${redirect.to} — ${redirect.why}\n` +
        `      fix: restore the entry in astro.config.mjs \`redirects\``,
    );
    continue;
  }
  const html = fs.readFileSync(file, "utf-8");
  // Astro static redirects render a meta refresh plus a canonical link.
  const target =
    /<meta\s+http-equiv="refresh"\s+content="0;\s*url=([^"]+)"/i.exec(
      html,
    )?.[1] ?? /<link\s+rel="canonical"\s+href="([^"]+)"/i.exec(html)?.[1];
  if (!target) {
    errors.push(`redirect page at ${redirect.from} has no refresh target`);
  } else if (!target.replace(/\/$/, "").endsWith(redirect.to)) {
    errors.push(
      `redirect ${redirect.from} points at ${target}, expected ${redirect.to}\n` +
        `      ${redirect.why}`,
    );
  }
}

// ─── 1 & 3 · Internal hrefs and fragments ────────────────────────────────────

const HREF_RE = /\shref="([^"]+)"/g;

let checkedLinks = 0;
const brokenByTarget = new Map();

for (const [route, file] of routeFile) {
  const html = fs.readFileSync(file, "utf-8");

  // Fragment targets available on this page: any id="..." attribute.
  const ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]));

  for (const m of html.matchAll(HREF_RE)) {
    const raw = m[1];
    if (
      raw.startsWith("http://") ||
      raw.startsWith("https://") ||
      raw.startsWith("mailto:") ||
      raw.startsWith("//") ||
      raw.startsWith("data:")
    ) {
      continue; // external or non-navigational; out of scope by design
    }

    checkedLinks += 1;

    // ─── in-page fragment ───────────────────────────────────────────────────
    if (raw.startsWith("#")) {
      const id = decodeURIComponent(raw.slice(1));
      if (id && !ids.has(id)) {
        const entry = `${route} → ${raw}\n      fragment target does not exist on this page`;
        if (route.startsWith("/docs/") && !DOCS_FRAGMENTS_BLOCKING) {
          reported.push(entry);
        } else {
          brokenByTarget.set(
            `${route} → ${raw}`,
            "fragment target does not exist on this page",
          );
        }
      }
      continue;
    }

    if (!raw.startsWith("/")) continue; // relative links are not used on this site

    const [pathPart, fragment] = raw.split("#");
    const normalised = pathPart.replace(/\/$/, "") || "/";

    if (!routes.has(normalised)) {
      const key = `${normalised}`;
      const existing = brokenByTarget.get(key);
      brokenByTarget.set(
        key,
        existing
          ? existing
          : `no page built at this route (first seen linked from ${route})`,
      );
      continue;
    }

    // ─── cross-page fragment ────────────────────────────────────────────────
    if (fragment) {
      const targetFile = routeFile.get(normalised);
      if (targetFile) {
        const targetHtml = fs.readFileSync(targetFile, "utf-8");
        if (
          !new RegExp(
            `\\sid="${fragment.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"`,
          ).test(targetHtml)
        ) {
          const entry = `${normalised}#${fragment}\n      fragment does not exist on the target page (linked from ${route})`;
          if (route.startsWith("/docs/") && !DOCS_FRAGMENTS_BLOCKING) {
            reported.push(entry);
          } else {
            brokenByTarget.set(
              `${normalised}#${fragment}`,
              `fragment does not exist on the target page (linked from ${route})`,
            );
          }
        }
      }
    }
  }
}

for (const [target, why] of brokenByTarget) {
  errors.push(`broken internal link: ${target}\n      ${why}`);
}

// ─── Report ──────────────────────────────────────────────────────────────────

if (REPORT_JSON) {
  fs.writeFileSync(
    REPORT_JSON,
    JSON.stringify(
      {
        schema: "icn-website-link-check/v1",
        checkedLinks,
        pages: routeFile.size,
        redirects: REQUIRED_REDIRECTS.length,
        blocking: errors.length,
        staleDocFragments: reported.length,
        staleDocFragmentSamples: reported
          .slice(0, 20)
          .map((r) => r.split("\n")[0]),
      },
      null,
      2,
    ) + "\n",
  );
}

if (reported.length > 0) {
  console.log(
    `[links] NOTE: ${reported.length} stale fragment(s) in rendered documentation ` +
      `(table-of-contents entries pointing at headings that no longer exist).`,
  );
  console.log(
    `[links]       These are source-markdown defects, not website defects. ` +
      `Reported, not blocking — see the header note in this script.`,
  );
  for (const r of reported.slice(0, 8))
    console.log("        · " + r.split("\n")[0]);
  if (reported.length > 8)
    console.log(`        · …and ${reported.length - 8} more`);
}

if (errors.length > 0) {
  console.error(`\n[links] FAIL — ${errors.length} problem(s):`);
  for (const e of errors) console.error("  ✗ " + e);
  process.exit(1);
}

console.log(
  `[links] PASS — ${checkedLinks} internal links across ${routeFile.size} pages resolve, ` +
    `${REQUIRED_REDIRECTS.length} compatibility redirects intact. External links not checked, by design.`,
);
