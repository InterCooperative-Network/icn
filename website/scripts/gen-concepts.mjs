#!/usr/bin/env node
// gen-concepts.mjs — project the canonical ICN concept map onto the public site.
//
// SOURCE OF TRUTH: docs/design-language/concept-map.md (repo root).
// OUTPUT:          website/src/data/concepts.generated.json (gitignored).
//
// The concept map already carries the exact thing the public site needs for
// plain-language-first presentation (#1740):
//
//   Canonical  → the internal ICN term  ("standing")
//   Public     → the plain-language label ("Your recognized status")
//   Short      → a one-line gloss suitable for helper text
//
// The website must never re-author those labels. Copying them into an .astro
// file would let the public wording drift away from the canonical map with
// nothing to detect it. So we parse the map at build time instead, and the
// <Term> component reads only from this projection.
//
// FAIL CLOSED: if the map moves, changes shape, or parses to fewer concepts
// than the floor below, this script exits non-zero and takes the build with
// it. A public glossary that silently empties out is worse than a red build.

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");
const repoRoot = path.resolve(websiteRoot, "..");

const SOURCE_REL = "docs/design-language/concept-map.md";
const sourcePath = path.join(repoRoot, SOURCE_REL);
const outPath = path.join(
  websiteRoot,
  "src",
  "data",
  "concepts.generated.json",
);

// Floor derived from the map as of this writing (22 concepts). If the map
// legitimately shrinks below this, lower it deliberately in the same commit
// that shrinks the map — do not let it drift silently.
const MIN_CONCEPTS = 18;

function fail(message) {
  console.error(`[gen-concepts] FAIL: ${message}`);
  console.error(`[gen-concepts] source: ${SOURCE_REL}`);
  process.exit(1);
}

if (!fs.existsSync(sourcePath)) {
  fail(`concept map not found at ${sourcePath}`);
}

const raw = fs.readFileSync(sourcePath, "utf-8");

// Field lines look like:  - **Canonical:** `identity`
//                         - **Public:** *Who you are*
//                         - **Short:** Cryptographic identity held by ...
const FIELD_RE = /^-\s+\*\*([A-Za-z ]+):\*\*\s*(.+)$/;

/** Strip the markdown emphasis/code wrappers the map uses for values. */
function clean(value) {
  return value
    .trim()
    .replace(/^`(.+)`$/, "$1")
    .replace(/^\*(.+)\*$/, "$1")
    .replace(/^_(.+)_$/, "$1")
    .trim();
}

const concepts = {};
const order = [];

let currentGroup = null;
let currentHeading = null;
let currentFields = null;

function flush() {
  if (!currentHeading || !currentFields) return;
  const canonical = currentFields.canonical;
  const publicLabel = currentFields.public;
  const short = currentFields.short;

  // A concept without all three of these is unusable for public rendering —
  // the whole point is plain label + canonical term + gloss.
  if (!canonical || !publicLabel || !short) {
    fail(
      `concept "${currentHeading}" is missing a required field ` +
        `(canonical=${!!canonical} public=${!!publicLabel} short=${!!short})`,
    );
  }
  if (concepts[canonical]) {
    fail(`duplicate canonical concept id "${canonical}"`);
  }

  concepts[canonical] = {
    canonical,
    heading: currentHeading,
    group: currentGroup,
    public: publicLabel,
    short,
    colorToken: currentFields.colorToken ?? null,
    loopPosition: currentFields.loopPosition ?? null,
    iconSlug: currentFields.iconSlug ?? null,
  };
  order.push(canonical);
  currentHeading = null;
  currentFields = null;
}

for (const line of raw.split("\n")) {
  const h2 = /^##\s+(?!#)(.+)$/.exec(line);
  if (h2) {
    flush();
    currentGroup = h2[1].trim();
    continue;
  }

  const h3 = /^###\s+(.+)$/.exec(line);
  if (h3) {
    flush();
    // Headings are "01. Identity" for loop stations, "Scope" elsewhere.
    currentHeading = h3[1].trim().replace(/^\d+\.\s*/, "");
    currentFields = {};
    continue;
  }

  if (!currentFields) continue;

  const field = FIELD_RE.exec(line);
  if (!field) continue;

  const key = field[1].trim().toLowerCase();
  const value = clean(field[2]);

  switch (key) {
    case "canonical":
      currentFields.canonical = value;
      break;
    case "public":
      currentFields.public = value;
      break;
    case "short":
      currentFields.short = value;
      break;
    case "color token":
      currentFields.colorToken = value;
      break;
    case "loop position":
      currentFields.loopPosition = value;
      break;
    case "icon slug":
      currentFields.iconSlug = value;
      break;
    default:
      // "Localization notes" and any future field: not projected publicly.
      break;
  }
}
flush();

if (order.length < MIN_CONCEPTS) {
  fail(
    `parsed only ${order.length} concepts, expected at least ${MIN_CONCEPTS}. ` +
      `The concept map's shape probably changed — fix the parser, do not lower the floor casually.`,
  );
}

// The nine closure-loop stations are load-bearing for the public explainers.
// If any of them stops parsing, several pages silently lose their vocabulary.
const REQUIRED_LOOP = [
  "identity",
  "standing",
  "authority",
  "governance",
  "policy",
  "accounting",
  "execution",
  "provenance",
  "member_experience",
];
const missingLoop = REQUIRED_LOOP.filter((id) => !concepts[id]);
if (missingLoop.length > 0) {
  fail(`missing required closure-loop concepts: ${missingLoop.join(", ")}`);
}

const payload = {
  source: SOURCE_REL,
  sourceHash: crypto
    .createHash("sha256")
    .update(raw)
    .digest("hex")
    .slice(0, 12),
  conceptCount: order.length,
  order,
  concepts,
};

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(payload, null, 2) + "\n");
console.log(
  `[gen-concepts] ${order.length} concepts from ${SOURCE_REL} → src/data/concepts.generated.json`,
);
