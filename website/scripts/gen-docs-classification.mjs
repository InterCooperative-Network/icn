#!/usr/bin/env node
// gen-docs-classification.mjs — decide what the public docs surface publishes,
// and how each published page is labelled.
//
// SOURCE OF TRUTH: docs/registry.toml (repo root) — the documentation control
//                  plane. It already classifies 100% of docs/**/*.md through
//                  longest-prefix [[doc_path_defaults]] plus explicit
//                  [docs."path"] overlays, with a CI-validated closed
//                  vocabulary.
// OUTPUT:          website/src/data/docs-classification.generated.json
//
// Why this exists (#2609): the website used to walk docs/ off the filesystem
// and publish every .md it found — 965 pages, including docs/internal/ (whose
// own README says it is "not part of the public-facing documentation"),
// partner-specific pilot material, and 156 agent session logs. Archive pages
// rendered identically to canonical ones, so a 2025 sprint report was
// indistinguishable from current architecture.
//
// This script does NOT invent a taxonomy. It reads the repo's own vocabulary
// and derives two things from it:
//
//   1. publish  — may this file appear on the public site at all?
//   2. layer    — which of the four public layers does it belong to?
//                 learn | reference | decisions | archive
//
// The merge order below mirrors docs/scripts/doc_control_check.py's
// merge_defaults(): longest matching prefix wins, explicit entry overlays.
// If the two ever disagree, the Python one is canonical — fix this file.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parse as parseToml } from "smol-toml";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");
const repoRoot = path.resolve(websiteRoot, "..");

const REGISTRY_REL = "docs/registry.toml";
const registryPath = path.join(repoRoot, REGISTRY_REL);
const docsRoot = path.join(repoRoot, "docs");
const outPath = path.join(
  websiteRoot,
  "src",
  "data",
  "docs-classification.generated.json",
);

function fail(message) {
  console.error(`[gen-docs-classification] FAIL: ${message}`);
  process.exit(1);
}

// ─── Publication rules ───────────────────────────────────────────────────────
//
// Each rule states WHY a path is withheld. "Withheld" means not rendered as a
// page on intercooperative.network — the file stays in the repository and
// stays reachable on GitHub. #2609 is explicit that history is preserved, not
// deleted; this is a publication decision, not a retention decision.

const WITHHELD_PREFIXES = [
  {
    prefix: "docs/internal/",
    reason:
      "docs/internal/README.md states this directory is not part of the public-facing documentation.",
  },
  {
    prefix: "docs/pilots/",
    reason:
      "Partner-specific operational material. ICN's public surface describes generic substrate; institution-specific content belongs to that institution's own package.",
  },
  {
    prefix: "docs/summit/",
    reason:
      "Partner/event-specific draft material naming a real external organisation.",
  },
  {
    prefix: "docs/development/sessions/",
    reason:
      "Per-session working logs. Useful archaeology in the repository, but they are not documentation and would swamp the public archive.",
  },
  {
    prefix: "docs/handoffs/",
    reason: "Point-in-time working handoffs between contributors.",
  },
  {
    prefix: "docs/reference/project-index/generated/",
    reason:
      "Machine-generated inventories (one exceeds half a megabyte). Meant for tooling, not readers.",
  },
  {
    prefix: "docs/dev/handoff-",
    reason: "Point-in-time working handoffs between contributors.",
  },
  {
    prefix: "docs/design/session-handoff-",
    reason: "Point-in-time working handoffs between contributors.",
  },
  {
    prefix: "docs/architecture/SESSION_SUMMARY_",
    reason:
      "Per-session working logs. Useful archaeology in the repository, but they are not documentation and would swamp the public archive.",
  },
  {
    prefix: "docs/superpowers/",
    reason: "Agent tooling notes, not project documentation.",
  },
  {
    prefix: "docs/ai/",
    reason: "Agent tooling notes, not project documentation.",
  },
];

/**
 * The same withheld classes, matched by name rather than by directory.
 *
 * WITHHELD_PREFIXES above assumes a class of material lives in one place. Two
 * of the reasons it states are about what a document *is*, not where it sits:
 * a per-conversation meeting packet is the same artifact whether it lands in
 * docs/strategy/ or docs/dev/. Enforcing those by directory alone let
 * dated organizer-meeting prep — naming real people and a real external
 * organisation — publish as current reference. `.claude/rules/design.md`
 * forbids NYCN/Summit language outside an institution package; this is where
 * that rule is enforced for the docs surface.
 *
 * Deliberately narrow: these match packet-shaped filenames, not the words
 * "meeting" or "aid" anywhere in a path.
 */
const WITHHELD_PATTERNS = [
  {
    pattern: /\/[^/]*(MEETING_BRIEF|MEETING_PREP|ONE_PAGE_AID|SCREEN_DECK)[^/]*\.md$/i,
    reason:
      "Per-conversation meeting packets naming real people and external organisations. Preparation material for one conversation is not public documentation.",
  },
  {
    pattern: /\/[^/]*NYCN[^/]*\.md$/i,
    reason:
      "Names a real external partner organisation. `.claude/rules/design.md` confines NYCN/Summit material to that institution's own package — ICN's public surface describes generic substrate. The documents stay in the repository and on GitHub.",
  },
];

/**
 * Last resort: the document says of itself that it is not public.
 *
 * Names are an unreliable handle on a content class — the same packet shipped
 * as MEETING_PREP, ONE_PAGE_AID, SCREEN_DECK and DISCOVERY_BRIEF. These
 * phrases are self-declarations, so a document carrying one is withheld
 * wherever it lives and whatever it is called.
 *
 * Matched against the head of the file only. An index that *lists* such a
 * document quotes the phrase in its body; the document that *is* one says so
 * at the top. Scanning whole bodies withheld docs/INDEX.generated.md.
 */
const SELF_DECLARED_INTERNAL =
  /internal prep doc|private prep|intentionally\s+\*{0,2}not\*{0,2}\s+committed/i;
const SELF_DECLARED_HEAD_BYTES = 1200;

/** Roles that are never published regardless of location. */
const WITHHELD_ROLES = new Set(["internal", "development_session"]);

// ─── Layer rules ─────────────────────────────────────────────────────────────
//
// Four public layers, per #2609. Order matters: first match wins.

const LEARN_PREFIXES = [
  "docs/onboarding/",
  "docs/guides/",
  "docs/examples/",
  "docs/GETTING_STARTED.md",
  "docs/OVERVIEW.md",
  "docs/glossary.md",
  "docs/README.md",
  "docs/INDEX.md",
];

const DECISION_PREFIXES = ["docs/adr/", "docs/rfcs/"];

/**
 * Truth classes and statuses that force the archive layer no matter where the
 * file lives. `historical` is the registry's own word for "not authoritative
 * for current behaviour".
 */
const ARCHIVE_TRUTH_CLASSES = new Set(["historical"]);
const ARCHIVE_STATUSES = new Set(["archived", "superseded"]);
const ARCHIVE_ROLES = new Set(["archive"]);

/**
 * Paths that are historical in substance but that the registry's prefix rules
 * classify only as `descriptive`, because no explicit row exists for them.
 * Each entry names the evidence for the reclassification.
 */
const ARCHIVE_PREFIX_OVERRIDES = [
  {
    prefix: "docs/phases/",
    reason: "registry recommends action=archive for this prefix",
  },
  {
    prefix: "docs/mobile/archive/",
    reason: "second, undeclared archive root",
  },
  {
    prefix: "docs/SYNC_LOG.md",
    reason: "the file itself declares: RETIRED (2026-07-13)",
  },
  {
    prefix: "docs/GOLDEN_PROMPT.md",
    reason: "the file itself declares: archived redirect stub",
  },
  {
    prefix: "docs/PHASE_HISTORY.md",
    reason: "phase history is by definition a historical record",
  },
  {
    prefix: "docs/REORGANIZATION_2026.md",
    reason: "record of a completed one-time reorganisation",
  },
];

// ─── Read the registry ───────────────────────────────────────────────────────

if (!fs.existsSync(registryPath)) {
  fail(`registry not found at ${registryPath}`);
}

let registry;
try {
  registry = parseToml(fs.readFileSync(registryPath, "utf-8"));
} catch (err) {
  fail(`could not parse ${REGISTRY_REL}: ${err.message}`);
}

const pathDefaults = Array.isArray(registry.doc_path_defaults)
  ? [...registry.doc_path_defaults]
  : [];
if (pathDefaults.length === 0) {
  fail(`${REGISTRY_REL} has no [[doc_path_defaults]] — cannot classify safely`);
}
// Longest prefix wins: sort descending by prefix length and take the first hit.
pathDefaults.sort((a, b) => (b.prefix ?? "").length - (a.prefix ?? "").length);

const explicitRows = registry.docs ?? {};

/** Merge defaults + explicit overlay for one repo-relative path. */
function mergedRecord(relPath) {
  const base = pathDefaults.find((d) => relPath.startsWith(d.prefix ?? "\0"));
  const explicit = explicitRows[relPath];
  return {
    truthClass: explicit?.truth_class ?? base?.truth_class ?? null,
    role: explicit?.role ?? base?.role ?? null,
    canonical: explicit?.canonical ?? base?.canonical ?? null,
    status: explicit?.status ?? null,
    recommendedAction:
      explicit?.recommended_action ?? base?.recommended_action ?? null,
    lastUpdated: explicit?.last_updated ?? base?.last_reviewed ?? null,
    title: explicit?.title ?? null,
    description: explicit?.description ?? null,
    hasExplicitRow: Boolean(explicit),
  };
}

// ─── Walk docs/ and classify ─────────────────────────────────────────────────

/** Read the head of a file as text, for self-declaration markers. */
function readHead(absPath, n) {
  try {
    const fd = fs.openSync(absPath, "r");
    const buf = Buffer.alloc(n);
    const bytes = fs.readSync(fd, buf, 0, n, 0);
    fs.closeSync(fd);
    return buf.subarray(0, bytes).toString("utf-8");
  } catch {
    return "";
  }
}

/** Read just the frontmatter block, for `superseded_by` and status hints. */
function readFrontmatter(absPath) {
  let head;
  try {
    const fd = fs.openSync(absPath, "r");
    const buf = Buffer.alloc(2048);
    const bytes = fs.readSync(fd, buf, 0, 2048, 0);
    fs.closeSync(fd);
    head = buf.subarray(0, bytes).toString("utf-8");
  } catch {
    return {};
  }
  const m = /^---\r?\n([\s\S]*?)\r?\n---/.exec(head);
  if (!m) return {};
  const out = {};
  for (const line of m[1].split("\n")) {
    const kv = /^([A-Za-z_][A-Za-z0-9_ ]*):\s*(.+)$/.exec(line);
    if (kv) out[kv[1].trim().toLowerCase()] = kv[2].trim();
  }
  return out;
}

const entries = {};
const counts = {
  total: 0,
  published: 0,
  withheld: 0,
  learn: 0,
  reference: 0,
  decisions: 0,
  archive: 0,
};
const withheldByReason = {};

function walk(dir) {
  for (const name of fs.readdirSync(dir).sort()) {
    const abs = path.join(dir, name);
    const stat = fs.statSync(abs);
    if (stat.isDirectory()) {
      walk(abs);
      continue;
    }
    if (!name.endsWith(".md")) continue;

    const rel = path.relative(repoRoot, abs).split(path.sep).join("/");
    const slug = rel.replace(/^docs\//, "").replace(/\.md$/, "");
    counts.total += 1;

    const record = mergedRecord(rel);
    const fm = readFrontmatter(abs);

    // 1. Publish decision.
    let publish = true;
    let withheldReason = null;

    const withheldPrefix = WITHHELD_PREFIXES.find((w) =>
      rel.startsWith(w.prefix),
    );
    const withheldRule =
      withheldPrefix ?? WITHHELD_PATTERNS.find((w) => w.pattern.test(`/${rel}`));
    if (withheldRule) {
      publish = false;
      withheldReason = withheldRule.reason;
    } else if (
      SELF_DECLARED_INTERNAL.test(readHead(abs, SELF_DECLARED_HEAD_BYTES))
    ) {
      publish = false;
      withheldReason =
        "The document declares itself internal or single-conversation preparation material.";
    } else if (record.role && WITHHELD_ROLES.has(record.role)) {
      publish = false;
      withheldReason = `registry role="${record.role}"`;
    }

    if (!publish) {
      counts.withheld += 1;
      withheldByReason[withheldReason] =
        (withheldByReason[withheldReason] ?? 0) + 1;
      entries[slug] = { rel, slug, publish: false, withheldReason };
      continue;
    }

    // 2. Layer decision.
    const archiveOverride = ARCHIVE_PREFIX_OVERRIDES.find((a) =>
      rel.startsWith(a.prefix),
    );
    const isArchive =
      Boolean(archiveOverride) ||
      (record.truthClass && ARCHIVE_TRUTH_CLASSES.has(record.truthClass)) ||
      (record.role && ARCHIVE_ROLES.has(record.role)) ||
      (record.status && ARCHIVE_STATUSES.has(record.status)) ||
      ARCHIVE_STATUSES.has((fm.status ?? "").toLowerCase());

    let layer;
    let archiveReason = null;
    if (isArchive) {
      layer = "archive";
      archiveReason =
        archiveOverride?.reason ??
        (record.status && ARCHIVE_STATUSES.has(record.status)
          ? `registry status="${record.status}"`
          : record.role === "archive"
            ? "registry role=archive"
            : `registry truth_class="${record.truthClass}"`);
    } else if (DECISION_PREFIXES.some((p) => rel.startsWith(p))) {
      layer = "decisions";
    } else if (LEARN_PREFIXES.some((p) => rel === p || rel.startsWith(p))) {
      layer = "learn";
    } else {
      layer = "reference";
    }

    counts.published += 1;
    counts[layer] += 1;

    // `superseded_by` / `Superseded by` points at the replacement. It is what
    // lets an archive banner say "the current page is X" instead of just
    // "this is old".
    const supersededByRaw =
      fm.superseded_by ?? fm["superseded by"] ?? fm.supersededby ?? null;

    entries[slug] = {
      rel,
      slug,
      publish: true,
      layer,
      archiveReason,
      truthClass: record.truthClass,
      role: record.role,
      canonical: record.canonical === "yes",
      status: record.status ?? fm.status?.toLowerCase() ?? null,
      lastReviewed: fm["last reviewed"] ?? record.lastUpdated ?? null,
      supersededBy: supersededByRaw
        ? String(supersededByRaw).replace(/^[`'"]|[`'"]$/g, "")
        : null,
      registryTitle: record.title,
      registryDescription: record.description,
    };
  }
}

if (!fs.existsSync(docsRoot)) fail(`docs/ not found at ${docsRoot}`);
walk(docsRoot);

// ─── Sanity floors ───────────────────────────────────────────────────────────
//
// These guard against a registry edit silently emptying or silently opening
// the public surface.

if (counts.published < 100) {
  fail(
    `only ${counts.published} of ${counts.total} docs would publish — the ` +
      `classification probably broke. Refusing to ship a near-empty docs surface.`,
  );
}
if (counts.withheld === 0) {
  fail(
    `no documents were withheld, which means the internal/partner filters did ` +
      `not match anything. Refusing to publish an unfiltered docs surface.`,
  );
}
if (counts.archive === 0) {
  fail(
    `no documents classified as archive — the historical/superseded detection ` +
      `is not working, and stale pages would render as current.`,
  );
}

const payload = {
  source: REGISTRY_REL,
  counts,
  withheldByReason,
  entries,
};

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(payload, null, 2) + "\n");
console.log(
  `[gen-docs-classification] ${counts.total} docs → ` +
    `${counts.published} published ` +
    `(learn ${counts.learn} · reference ${counts.reference} · ` +
    `decisions ${counts.decisions} · archive ${counts.archive}), ` +
    `${counts.withheld} withheld`,
);
