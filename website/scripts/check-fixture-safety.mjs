#!/usr/bin/env node
// check-fixture-safety.mjs — the public walkthrough stays fictional, labelled,
// deterministic, and offline.
//
//   node scripts/check-fixture-safety.mjs
//
// Run AFTER `npm run build`. Reads `dist/see-it-work/index.html` and the
// fixture module.
//
// ─── The invariant ───────────────────────────────────────────────────────────
//
// /see-it-work shows a member's standing, a decision, an authority basis, a
// mutation preview and a receipt. Every one of those is a shape that also
// exists for real institutional data. The page is safe only because its values
// are invented and it says so — and both halves of that are easy to lose in a
// later edit that wires up "real" data to make the demo more impressive.
//
// ─── Checks ──────────────────────────────────────────────────────────────────
//
//   1. The truth label is present in the rendered HTML.            [blocking]
//   2. The page states plainly that its contents are invented.     [blocking]
//   3. No real partner or event name appears.                      [blocking]
//   4. No live data fetching: no fetch/XHR/WebSocket/EventSource,
//      no absolute API URL, no runtime data attribute.             [blocking]
//   5. No credential-shaped strings.                               [blocking]
//   6. Rendering is deterministic — no clock, no randomness.       [blocking]
//   7. No mutation affordance: the read-only walkthrough must not grow a
//      confirm/submit control, which under ADR-0027 would require mandate,
//      reversibility and receipt declarations it does not have.    [blocking]
//
// ─── On name matching ────────────────────────────────────────────────────────
//
// Check 3 is the one that could become a false-positive machine. It does NOT
// regex arbitrary names. It matches a short, closed list of real organisation
// and event names that the repository already treats as partner-scoped and
// confines to `institutions/<name>/`. That is a structural boundary the repo
// already asserts elsewhere, restated at the publication point.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { findMutationControls } from "./lib/html-controls.mjs";
import { extractInlineScripts } from "./lib/html-scripts.mjs";
import { parseHtml } from "./lib/html-tree.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");

const pagePath = path.join(websiteRoot, "dist", "see-it-work", "index.html");
const fixturePath = path.join(websiteRoot, "src", "data", "walkthrough.ts");

/**
 * Real organisations and events that belong only to an institution package.
 * `.claude/rules/design.md`: "No NYCN / Summit / 'reference federation'
 * language on anything that isn't an institution package."
 */
const PARTNER_NAMES = [
  "NYCN",
  "New York Cooperative Network",
  "New York Cooperative Summit",
  "NYCN Organizers Cooperative",
  "Summit 2026",
];

/** Runtime data access that would make the page non-static. */
const LIVE_DATA_PATTERNS = [
  { re: /\bfetch\s*\(/, why: "fetch() call" },
  { re: /XMLHttpRequest/, why: "XMLHttpRequest" },
  { re: /new\s+WebSocket/, why: "WebSocket" },
  { re: /new\s+EventSource/, why: "EventSource" },
  {
    re: /https?:\/\/[^"'\s]*\/(api|v1|rpc|gateway)\b/i,
    why: "API endpoint URL",
  },
];

/** Non-determinism that would make two builds differ. */
const NONDETERMINISM_PATTERNS = [
  { re: /Math\.random\s*\(/, why: "Math.random()" },
  { re: /Date\.now\s*\(/, why: "Date.now()" },
  { re: /new\s+Date\s*\(\s*\)/, why: "new Date() with no argument" },
  { re: /performance\.now\s*\(/, why: "performance.now()" },
];

/** Credential-shaped strings. Structural shapes, not name matching. */
const CREDENTIAL_PATTERNS = [
  {
    re: /\b(?:secret|passwd|password|api[_-]?key|private[_-]?key)\s*[:=]\s*["'][^"']{6,}/i,
    why: "assigned credential",
  },
  { re: /-----BEGIN [A-Z ]*PRIVATE KEY-----/, why: "PEM private key" },
  { re: /\beyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,}\./, why: "JWT" },
];

for (const [label, p] of [
  ["dist/see-it-work/index.html", pagePath],
  ["src/data/walkthrough.ts", fixturePath],
]) {
  if (!fs.existsSync(p)) {
    console.error(
      `[fixture-safety] FAIL: ${label} not found. Run \`npm run build\` first.`,
    );
    process.exit(1);
  }
}

const errors = [];

const html = fs.readFileSync(pagePath, "utf-8");
const fixture = fs.readFileSync(fixturePath, "utf-8");

// ─── One parse, shared by the two structural checks ──────────────────────────
//
// Checks 4 and 7 both ask questions about the document's structure, and both
// used to ask them of raw markup. Parsing once here means each asks parse5
// instead, and means a parse failure is handled in exactly one place.
//
// Fail closed. If the page cannot be parsed, neither structural check has
// looked at anything, and "found no problems" would be a lie of the same shape
// as the regexps they replaced — a PASS emitted by a check that read nothing.
let parsedPage = null;
try {
  parsedPage = parseHtml(html);
} catch (err) {
  errors.push(
    `dist/see-it-work/index.html could not be parsed (${err.message})\n` +
      `      checks 4 and 7 are reported as failing rather than evaluated on` +
      ` unparsed HTML`,
  );
}

// ─── 1 · Truth label ─────────────────────────────────────────────────────────

if (!/Truth label/i.test(html)) {
  errors.push(
    `the rendered walkthrough carries no truth label\n` +
      `      docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md §3: "A visual without a label is not shippable"`,
  );
}
if (!/illustrative direction/i.test(html)) {
  errors.push(
    `the walkthrough's truth label is not "illustrative direction"\n` +
      `      the record shapes are real but the assembled surface is not shipped, and a visual` +
      ` between two labels carries the less optimistic one`,
  );
}

// ─── 2 · Explicit fiction statement ──────────────────────────────────────────

if (!/invented for this walkthrough/i.test(html)) {
  errors.push(
    `the walkthrough does not state that its contents are invented\n` +
      `      #2610: "Fixture-backed/demo status is unmistakable"`,
  );
}

// ─── 3 · Partner names ───────────────────────────────────────────────────────

for (const name of PARTNER_NAMES) {
  const re = new RegExp(name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "i");
  if (re.test(html)) {
    errors.push(
      `real partner/event name "${name}" appears in the public walkthrough\n` +
        `      partner-specific names belong only in that institution's package`,
    );
  }
  if (re.test(fixture)) {
    errors.push(
      `real partner/event name "${name}" appears in src/data/walkthrough.ts`,
    );
  }
}

// ─── 4 · Live data ───────────────────────────────────────────────────────────

// Inspect the bodies of the inline scripts the page actually ships, external
// `<script src=...>` references excluded. Extraction goes through a real HTML5
// parser rather than a regexp: the pattern this replaced did not match
// `</script >`, so a block closed with a space before the `>` was never
// extracted and this check passed without having read it (CodeQL alert 106,
// js/bad-tag-filter). See scripts/lib/html-scripts.mjs.
let inlineScripts;
if (parsedPage) {
  inlineScripts = extractInlineScripts(parsedPage).join("\n");
} else {
  // The parse failure above is already blocking. Scan the whole document as
  // well rather than nothing: LIVE_DATA_PATTERNS match plain source text
  // (`fetch(`, `WebSocket`, an API URL) rather than tag structure, so a raw
  // scan here over-reports instead of under-reporting.
  inlineScripts = html;
}

for (const { re, why } of LIVE_DATA_PATTERNS) {
  if (re.test(inlineScripts) || re.test(fixture)) {
    errors.push(
      `the walkthrough performs live data access (${why})\n` +
        `      it must render identically with no network available — #2610 requires a` +
        ` self-contained read-only walkthrough`,
    );
  }
}

if (/data-(?:live|runtime|endpoint|api)=/.test(html)) {
  errors.push(
    `the walkthrough carries a runtime data attribute, implying live wiring`,
  );
}

// ─── 5 · Credentials ─────────────────────────────────────────────────────────

for (const { re, why } of CREDENTIAL_PATTERNS) {
  if (re.test(html) || re.test(fixture)) {
    errors.push(`credential-shaped string in the walkthrough (${why})`);
  }
}

// ─── 6 · Determinism ─────────────────────────────────────────────────────────

for (const { re, why } of NONDETERMINISM_PATTERNS) {
  if (re.test(fixture)) {
    errors.push(
      `non-deterministic value in the fixture (${why})\n` +
        `      the walkthrough must render identically on every build`,
    );
  }
}

// ─── 7 · No mutation affordance ──────────────────────────────────────────────
//
// Structural, not textual. `<input type='submit'>` with single quotes,
// `<input type=submit>` unquoted, and `<button><span>Confirm</span></button>`
// with the label nested are all working controls that the three regexps this
// replaced did not match — see scripts/lib/html-controls.mjs for the full list.
//
// No raw-text fallback when the parse failed: a mutation control is defined by
// element and attribute structure, so there is nothing honest to scan for
// without a tree. The blocking parse error above stands on its own.
if (parsedPage) {
  const reported = new Set();
  for (const { why, detail } of findMutationControls(parsedPage)) {
    if (reported.has(why)) continue;
    reported.add(why);
    errors.push(
      `the read-only walkthrough contains a mutation affordance (${why}: ${detail})\n` +
        `      ADR-0027 requires any action producing a receipt to declare mandate,` +
        ` reversibility and the receipt before the confirm step`,
    );
  }
}

// ─── Report ──────────────────────────────────────────────────────────────────

if (errors.length > 0) {
  console.error(`\n[fixture-safety] FAIL — ${errors.length} problem(s):`);
  for (const e of errors) console.error("  ✗ " + e);
  console.error(
    `\n  Reference: docs/design/PUBLIC_SITE_IA.md §8, docs/design/MUST_NOT_SHIP.md items 2 and 3.`,
  );
  process.exit(1);
}

console.log(
  `[fixture-safety] PASS — /see-it-work is labelled "illustrative direction", states its own ` +
    `fiction, names no partner, fetches nothing, holds no credentials, is deterministic, ` +
    `and offers no mutation.`,
);
