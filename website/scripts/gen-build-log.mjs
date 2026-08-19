#!/usr/bin/env node
// gen-build-log.mjs — a restrained public record of meaningful state changes.
//
// SOURCE: git history on the current branch.
// OUTPUT: website/src/data/build-log.generated.json (gitignored).
//
// #1369 asks for a Build Log so the public site does not look dormant while
// development is active, and is equally clear that it must not be a commit
// feed: "Avoid publishing raw commit noise."
//
// The filter that makes this a state log rather than a commit log:
//
//   1. Only squash-merged commits — every entry carries a (#N) and therefore
//      links to a reviewed pull request. Work-in-progress commits never had a
//      public review surface and do not belong on a public page.
//   2. Only conventional-commit types that correspond to an institutional
//      state change. `chore`, `style`, `refactor`, `ci`, `build`, `test` are
//      real work, but nothing about the project's public state changed.
//   3. Grouped into a small set of classes a non-developer can read, so the
//      log answers "what kind of thing changed" rather than "what file moved".
//
// Nothing here is a readiness signal. An entry means a change landed and was
// reviewed. It does not mean a capability became usable, deployed, or ready —
// that question is answered only by the project-state surface, which is
// generated from docs/status.toml and carries its own evidence axis.

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(here, "..");
const repoRoot = path.resolve(websiteRoot, "..");
const outPath = path.join(
  websiteRoot,
  "src",
  "data",
  "build-log.generated.json",
);

/** How many entries the public page shows. Small on purpose. */
const MAX_ENTRIES = 12;
/** How far back to look. Keeps the scan bounded on a large history. */
const SCAN_COMMITS = 400;

const REPO_URL = "https://github.com/InterCooperative-Network/icn";

/**
 * Conventional-commit type → public class.
 *
 * `docs` is included only when the scope indicates a decision or a
 * state re-verification (handled below) — a typo fix in a guide is not a
 * public state change.
 */
const CLASS_BY_TYPE = {
  feat: {
    id: "capability",
    label: "Capability",
    detail: "New behaviour landed in the implementation.",
  },
  fix: {
    id: "correctness",
    label: "Correctness",
    detail: "A defect was corrected, often one found by review or testing.",
  },
  security: {
    id: "hardening",
    label: "Hardening",
    detail: "A security or safety property was tightened.",
  },
  perf: {
    id: "capability",
    label: "Capability",
    detail: "New behaviour landed in the implementation.",
  },
  spec: {
    id: "decision",
    label: "Decision or specification",
    detail: "A design decision or specification was recorded.",
  },
};

/**
 * Scopes that promote a `docs(...)` commit into a public state change.
 * A decision record landing IS a state change; a doc typo is not.
 */
const DECISION_DOC_SCOPES = new Set([
  "adr",
  "rfc",
  "rfcs",
  "design",
  "identity",
  "architecture",
  "spec",
  "governance",
]);
const STATE_DOC_SCOPES = new Set(["status", "state", "project-index", "truth"]);

function run(args) {
  try {
    return execFileSync("git", args, { cwd: repoRoot, encoding: "utf-8" });
  } catch {
    return "";
  }
}

// %cs = committer date, short (YYYY-MM-DD). %s = subject. %H = full hash.
const rawLog = run([
  "log",
  `-${SCAN_COMMITS}`,
  "--no-merges",
  "--format=%H%cs%s",
]);

const SUBJECT_RE = /^([a-z]+)(?:\(([^)]+)\))?:\s*(.+?)\s*(?:\(#(\d+)\))\s*$/;

const entries = [];
for (const line of rawLog.split("\n")) {
  if (!line.trim()) continue;
  const [hash, date, subject] = line.split("");
  if (!subject) continue;

  const m = SUBJECT_RE.exec(subject);
  if (!m) continue; // no PR reference → never had a public review surface

  const [, type, scope, description, pr] = m;

  let klass = CLASS_BY_TYPE[type];
  if (!klass && type === "docs") {
    const s = (scope ?? "").toLowerCase();
    if (DECISION_DOC_SCOPES.has(s)) {
      klass = {
        id: "decision",
        label: "Decision or specification",
        detail: "A design decision or specification was recorded.",
      };
    } else if (STATE_DOC_SCOPES.has(s)) {
      klass = {
        id: "reclassification",
        label: "State re-verification",
        detail:
          "The project re-checked what it claims about itself and updated the record.",
      };
    }
  }
  if (!klass) continue;

  // Trailing "(#2469)" issue references inside the description are noise on a
  // public page; the PR link already gets the reader to the discussion.
  const cleanDescription = description.replace(/\s*\(#\d+\)\s*$/, "").trim();

  entries.push({
    date,
    class: klass,
    scope: scope ?? null,
    description:
      cleanDescription.charAt(0).toUpperCase() + cleanDescription.slice(1),
    pr: Number(pr),
    prUrl: `${REPO_URL}/pull/${pr}`,
    commit: hash.slice(0, 8),
  });

  if (entries.length >= MAX_ENTRIES) break;
}

const payload = {
  // The newest entry's date is the honest "as of" for this surface. The build
  // timestamp would make an idle month look freshly active.
  newestEntryDate: entries[0]?.date ?? null,
  scanned: SCAN_COMMITS,
  shown: entries.length,
  entries,
};

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(payload, null, 2) + "\n");
console.log(
  `[gen-build-log] ${entries.length} meaningful state changes ` +
    `(newest ${payload.newestEntryDate ?? "none"}) → src/data/build-log.generated.json`,
);
