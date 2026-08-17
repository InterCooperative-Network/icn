#!/usr/bin/env node
// gen-project-state.mjs — project canonical repo state onto the public site.
//
// SOURCE OF TRUTH: docs/status.toml (repo root) — the only machine-readable,
//                  per-subsystem maturity artifact in the repo.
// OUTPUT:          website/src/data/project-state.generated.json (gitignored).
//
// Why this exists (#1369): the public state surface used to be a hand-edited
// website JSON file. That is a second project tracker, and it drifted a month
// behind docs/status.toml. This script removes the second tracker: the website
// now renders a *projection* of the canonical file and nothing else.
//
// ─── The two axes ────────────────────────────────────────────────────────────
//
// docs/reference/project-index/claim-boundaries.md is explicit that
// implementation maturity and evidence strength are ORTHOGONAL and that a
// public claim must carry both. So every projected subsystem carries:
//
//   band     — how mature the capability is        (ADR-0032 public vocabulary)
//   evidence — how we know                         (derived from evidence_type)
//
// Keeping evidence separate is what stops "merged code" from reading as
// "production-ready" on the public page. A subsystem can be `strong` on the
// maturity axis and still say "demonstrated in a fixture-backed rehearsal" on
// the evidence axis, and the reader sees both.
//
// ─── FAIL CLOSED ─────────────────────────────────────────────────────────────
//
// Any unknown vocabulary value, a missing source file, or a subsystem count
// below the floor exits non-zero and takes the build down. Publishing a
// silently-empty or silently-mis-mapped state page is worse than a red build,
// because the page's whole purpose is to be trustworthy.

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { parse as parseToml } from "smol-toml";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");
const repoRoot = path.resolve(websiteRoot, "..");

const STATUS_REL = "docs/status.toml";
const statusPath = path.join(repoRoot, STATUS_REL);
const outPath = path.join(
  websiteRoot,
  "src",
  "data",
  "project-state.generated.json",
);

const MIN_SUBSYSTEMS = 8;

function fail(message) {
  console.error(`[gen-project-state] FAIL: ${message}`);
  console.error(`[gen-project-state] source: ${STATUS_REL}`);
  process.exit(1);
}

// ─── Mapping tables ──────────────────────────────────────────────────────────
//
// These are the ONLY place the internal vocabulary meets the public one.
// The rationale for each row is documented in
// docs/reference/project-index/public-state-projection.md. Changing a row here
// changes a public claim — do not edit one without editing that document.

/** docs/status.toml `status` → ADR-0032 public maturity band. */
const BAND_BY_STATUS = {
  // Verified working and then some. The band the repo is willing to defend.
  "exceeds-spec": "strong",
  confirmed: "strong",
  // Real implementation, verification incomplete — the surfaces lag the code.
  "partially-confirmed": "maturing",
  // A deliberately reduced implementation. Real, but not the full concept.
  simplified: "maturing",
  // Works, but not yet reliable enough to lean on. Active frontier.
  "working-immature": "advancing",
  // Scaffolding exists; the capability does not.
  "foundation-only": "notyet",
};

/**
 * docs/status.toml `evidence_type` → public evidence class.
 *
 * This is the axis that prevents readiness inflation. `test` evidence means
 * the code is exercised by the suite — it does NOT mean the capability has
 * been run by an institution, and the label has to say so.
 */
const EVIDENCE_BY_TYPE = {
  test: {
    id: "test-backed",
    label: "Test-backed",
    detail: "Exercised by the automated test suite in this repository.",
  },
  ci: {
    id: "ci-backed",
    label: "CI-backed",
    detail: "Enforced by a check that runs on every change.",
  },
  demo: {
    id: "fixture-backed",
    label: "Fixture-backed",
    detail:
      "Demonstrated against deterministic fixture data, not live institutional use.",
  },
  "code-review": {
    id: "reviewed",
    label: "Reviewed",
    detail:
      "Established by reading the implementation, not by an automated check.",
  },
  "human-asserted": {
    id: "asserted",
    label: "Asserted",
    detail:
      "Stated by a maintainer. No automated check currently proves this one.",
  },
};

/** Public label for each band. Mirrors MaturityBadge.astro's defaults. */
const BAND_LABELS = {
  strong: "Strong today",
  advancing: "Advancing now",
  maturing: "Real but maturing",
  notyet: "Not yet",
  behind: "Behind the system",
};

// ─── Read + validate ─────────────────────────────────────────────────────────

if (!fs.existsSync(statusPath)) {
  fail(`canonical status file not found at ${statusPath}`);
}

let status;
try {
  status = parseToml(fs.readFileSync(statusPath, "utf-8"));
} catch (err) {
  fail(`could not parse ${STATUS_REL}: ${err.message}`);
}

if (!status.subsystems || typeof status.subsystems !== "object") {
  fail(`${STATUS_REL} has no [subsystems] table`);
}

/**
 * Fields in [summary] that are self-documented as stale or that contradict
 * another canonical file. None of these may reach a public surface.
 *
 *   total_tests_note   — status.toml itself marks the count stale
 *   loc_approx         — contradicted by docs/PHASE_PROGRESS.md
 *   deployment         — carries "running since Dec 2025", which
 *                        show-readiness-map.md §red-lines forbids publishing
 *
 * We do not project [summary] at all rather than maintain a denylist that
 * could be outflanked by a new field.
 */
const PROJECT_SUMMARY = false;

const subsystems = [];
for (const [key, raw] of Object.entries(status.subsystems)) {
  const band = BAND_BY_STATUS[raw.status];
  if (!band) {
    fail(
      `subsystem "${key}" has unmapped status "${raw.status}". ` +
        `Add a row to BAND_BY_STATUS and document it in ` +
        `docs/reference/project-index/public-state-projection.md.`,
    );
  }
  const evidence = EVIDENCE_BY_TYPE[raw.evidence_type];
  if (!evidence) {
    fail(
      `subsystem "${key}" has unmapped evidence_type "${raw.evidence_type}". ` +
        `Add a row to EVIDENCE_BY_TYPE and document it.`,
    );
  }
  if (!raw.last_verified) {
    fail(`subsystem "${key}" has no last_verified date`);
  }

  subsystems.push({
    id: key,
    name: raw.name ?? key,
    band,
    bandLabel: BAND_LABELS[band],
    internalStatus: raw.status,
    evidence,
    lastVerified: String(raw.last_verified),
    // gaps[] is the honesty budget: what the repo says is missing. Publishing
    // it is the point — ADR-0032 rule 7 requires gaps be named, not buried.
    gaps: Array.isArray(raw.gaps) ? raw.gaps.map(String) : [],
    // A short evidence list helps a reader check the claim themselves.
    evidenceNotes: Array.isArray(raw.evidence) ? raw.evidence.map(String) : [],
    crates: Array.isArray(raw.crates) ? raw.crates.map(String) : [],
  });
}

if (subsystems.length < MIN_SUBSYSTEMS) {
  fail(
    `parsed only ${subsystems.length} subsystems, expected at least ${MIN_SUBSYSTEMS}`,
  );
}

// Deterministic order: strongest first, then alphabetical inside a band, so
// the page does not reshuffle between builds for no reason.
const BAND_ORDER = ["strong", "advancing", "maturing", "behind", "notyet"];
subsystems.sort((a, b) => {
  const d = BAND_ORDER.indexOf(a.band) - BAND_ORDER.indexOf(b.band);
  return d !== 0 ? d : a.name.localeCompare(b.name);
});

// ─── Freshness ───────────────────────────────────────────────────────────────
//
// The public page must show how old this claim set is, so staleness is visible
// rather than invisible. We use the newest last_verified across subsystems —
// not the build time, which would make a stale file look fresh every deploy.

const verifiedDates = subsystems.map((s) => s.lastVerified).sort();
const newestVerified = verifiedDates[verifiedDates.length - 1];
const oldestVerified = verifiedDates[0];

function gitLastCommitIso(relPath) {
  try {
    return execFileSync("git", ["log", "-1", "--format=%cI", "--", relPath], {
      cwd: repoRoot,
      encoding: "utf-8",
    }).trim();
  } catch {
    return null;
  }
}

const payload = {
  source: STATUS_REL,
  sourceLastCommit: gitLastCommitIso(STATUS_REL),
  // Newest/oldest verification dates carried by the source itself. The page
  // shows these, not the build timestamp — a rebuild must not look like a
  // re-verification.
  newestVerified,
  oldestVerified,
  projectedSummary: PROJECT_SUMMARY,
  subsystemCount: subsystems.length,
  bands: BAND_LABELS,
  subsystems,
};

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(payload, null, 2) + "\n");
console.log(
  `[gen-project-state] ${subsystems.length} subsystems from ${STATUS_REL} ` +
    `(verified ${oldestVerified}…${newestVerified}) → src/data/project-state.generated.json`,
);
