// concept-model.test.mjs — semantic regression tests for the generated public vocabulary.
//
// Guards the public concept projection (docs/design-language/concept-map.md →
// scripts/gen-concepts.mjs → src/data/concepts.generated.json) against the
// specific conflations that drifted into public copy during the closure-loop
// generation. Each assertion targets a concrete phrase or structural assumption
// that caused actual drift; this is not a keyword police.
//
// Context: #2801 Tranche 1 (current-project truth reconciliation). Public brand
// remains ICN. Nothing here implies a protocol or identifier rename.

import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const website = path.resolve(here, "../..");

function generate() {
  execFileSync(process.execPath, ["scripts/gen-concepts.mjs"], {
    cwd: website,
    stdio: "pipe",
  });
  return JSON.parse(
    fs.readFileSync(
      path.join(website, "src/data/concepts.generated.json"),
      "utf8",
    ),
  );
}

// The public causal spine the site explains at first contact. These are
// public/design-language IDs, not code identifiers; existing crate and API
// names are untouched.
const REQUIRED_PUBLIC_CAUSAL = [
  "institution",
  "standing",
  "process",
  "decision",
  "authority",
  "effect",
  "evidence",
  "continuity",
  "agreement",
];

test("generated concept model carries the public causal spine", () => {
  const data = generate();
  for (const id of REQUIRED_PUBLIC_CAUSAL) {
    assert.ok(data.concepts[id], `missing public causal concept: ${id}`);
  }
});

test("every projected concept has canonical, public, short, and group", () => {
  const data = generate();
  for (const concept of Object.values(data.concepts)) {
    assert.ok(concept.canonical, `concept missing canonical: ${JSON.stringify(concept)}`);
    assert.ok(concept.public, `${concept.canonical}: missing public label`);
    assert.ok(concept.short, `${concept.canonical}: missing short gloss`);
    assert.ok(concept.group, `${concept.canonical}: missing group`);
  }
});

test("stale closure-loop assumptions are not load-bearing", () => {
  const data = generate();
  // The old nine-station loop required these to carry a loopPosition. The
  // current model re-layers them (identity → technical family; accounting →
  // resource/commitment state; provenance → evidence; member_experience →
  // surface). They may still exist as concepts, but nothing may *require*
  // them to be ordered loop stations.
  for (const id of ["identity", "accounting", "provenance", "member_experience"]) {
    const c = data.concepts[id];
    if (!c) continue;
    assert.equal(
      c.loopPosition,
      null,
      `${id} still carries a loopPosition; the closure loop is no longer the public ontology`,
    );
  }
});

test("public labels do not carry the known category errors", () => {
  const data = generate();

  // person = key
  if (data.concepts.identity) {
    assert.doesNotMatch(
      data.concepts.identity.short,
      /cryptographic identity held by the member/i,
      "identity.short still teaches Human Subject = Principal",
    );
  }

  // attestation = truth
  assert.ok(data.concepts.attestation, "attestation concept missing");
  assert.notEqual(
    data.concepts.attestation.public.toLowerCase(),
    "a verified claim",
    "attestation.public still says 'A verified claim' (authentic ≠ true)",
  );

  // standing = status/rank
  assert.ok(data.concepts.standing, "standing concept missing");
  assert.notEqual(
    data.concepts.standing.public.toLowerCase(),
    "your recognized status",
    "standing.public still reads as status/rank rather than 'why you have a say'",
  );

  // authority = permission
  assert.ok(data.concepts.authority, "authority concept missing");
  assert.notEqual(
    data.concepts.authority.public.toLowerCase(),
    "what you are allowed to do",
    "authority.public still reads as technical permission",
  );

  // scope = entity
  if (data.concepts.scope) {
    assert.doesNotMatch(
      data.concepts.scope.short,
      /a distinct institutional entity/i,
      "scope.short still conflates scope with entity/domain",
    );
  }

  // evidence = proof of legitimacy
  if (data.concepts.provenance) {
    assert.notEqual(
      data.concepts.provenance.public.toLowerCase(),
      "history and proof",
      "provenance.public still uses unqualified 'proof'",
    );
  }
});

test("public loop component explains the current causal model", () => {
  const loopSource = fs.readFileSync(
    path.join(website, "src/components/PublicLoop.astro"),
    "utf8",
  );
  assert.doesNotMatch(
    loopSource,
    /label:\s*['"]Proof['"]/,
    "PublicLoop still labels a stage 'Proof'",
  );
  assert.match(loopSource, /Why you have a say/i, "PublicLoop lacks the standing stage");
  assert.match(loopSource, /What evidence remains/i, "PublicLoop lacks the evidence stage");
});
