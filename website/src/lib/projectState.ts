// projectState.ts — typed access to the generated public project-state
// projection.
//
// Data comes from docs/status.toml via scripts/gen-project-state.mjs. Importing
// the JSON directly gives every field the type `string`, which loses exactly
// the distinctions that matter here — a band and an evidence class are closed
// vocabularies, and a page that renders an unrecognised one should fail to
// build rather than render an unstyled badge.
//
// The assertions below are safe because the generator validates both
// vocabularies before writing the file and exits non-zero on an unmapped
// value. This module is the seam where that guarantee becomes a type.

import generated from "../data/project-state.generated.json";
import type { Band } from "../components/MaturityBadge.astro";
import type { EvidenceId } from "../components/EvidenceBadge.astro";

export interface EvidenceClass {
  id: EvidenceId;
  label: string;
  detail: string;
}

export interface Subsystem {
  id: string;
  name: string;
  band: Band;
  bandLabel: string;
  /** The docs/status.toml value this band was mapped from. */
  internalStatus: string;
  evidence: EvidenceClass;
  /** ISO date (YYYY-MM-DD) the subsystem was last verified against source. */
  lastVerified: string;
  /** The project's own record of what is missing. Published, not summarised. */
  gaps: string[];
  evidenceNotes: string[];
  crates: string[];
}

export interface ProjectState {
  /** Repo-relative path of the canonical source. */
  source: string;
  sourceLastCommit: string | null;
  newestVerified: string;
  oldestVerified: string;
  subsystemCount: number;
  subsystems: Subsystem[];
}

export const PROJECT_STATE = generated as unknown as ProjectState;

/** Subsystems in a given maturity band, in the generator's stable order. */
export function subsystemsInBand(band: Band): Subsystem[] {
  return PROJECT_STATE.subsystems.filter((s) => s.band === band);
}

/**
 * The distinct evidence classes present in the current data. Used to build a
 * legend that explains exactly what a reader will encounter on the page and
 * nothing more — a legend listing classes that do not appear is noise.
 */
export function evidenceClassesInUse(): EvidenceClass[] {
  return [
    ...new Map(
      PROJECT_STATE.subsystems.map((s) => [s.evidence.id, s.evidence]),
    ).values(),
  ];
}
