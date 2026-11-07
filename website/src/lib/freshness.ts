// freshness.ts — how the two date signals on /whats-real-now relate.
//
// The page carries two signals that answer different questions:
//
//   verification  — when a subsystem's maturity and evidence were last read
//                   against source (docs/status.toml, per subsystem).
//   selected log  — a bounded selection of reviewed, merged changes
//                   (gen-build-log.mjs: newest-first, capped, filtered).
//
// The log is a selection, never a census. It is capped at a fixed number of
// entries and drawn from a fixed window of recent commits, so a count derived
// from it is a property of the filter, not of the project. Nothing in this
// module produces a "changes since review" total, and the result type carries
// `exhaustive: false` as a literal so a caller cannot render the selection as
// complete without changing this contract.
//
// Nothing here reads or writes maturity bands, evidence classes, or the
// canonical status file. A merged change never moves a subsystem's maturity;
// that is a separate, human review (ADR-0032).

export interface DatedEntry {
  /** ISO date, YYYY-MM-DD. */
  date: string;
  /** Free-form area label from the commit's conventional-commit scope. */
  scope?: string;
}

export interface FreshnessInput {
  /** Oldest and newest per-subsystem verification dates (ISO). */
  verified: { oldest: string; newest: string };
  /** The hand-set editorial review date for the page prose (ISO). */
  proseReviewed: string;
  /** The selected log entries, in any order. Not modified. */
  entries: readonly DatedEntry[];
  /**
   * The date this page was built (ISO), carried through as an absolute date.
   * It is never turned into an elapsed-days figure: that number is only true
   * on the day of the build and would quietly age between deployments.
   */
  builtOn: string;
}

export interface Freshness {
  verifiedNewest: string;
  verifiedOldest: string;
  /** Absolute date this page was built. Not an age. */
  builtOn: string;
  proseReviewed: string;
  /** True when no subsystem row has been verified after the oldest date. */
  verificationUniform: boolean;
  /** Selected entries dated strictly after the newest verification. */
  selectedAfterVerification: DatedEntry[];
  /** Distinct scope labels touched by those entries, sorted. */
  areasTouched: string[];
  /**
   * Always false. The selection is capped and filtered; it is not a complete
   * list of changes and must never be presented as one.
   */
  exhaustive: false;
}

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

function parseIso(value: string): number {
  if (!ISO_DATE.test(value)) {
    throw new Error(
      `freshness: expected an ISO date (YYYY-MM-DD), got "${value}"`,
    );
  }
  const ms = Date.parse(`${value}T00:00:00Z`);
  if (Number.isNaN(ms)) {
    throw new Error(`freshness: unparseable date "${value}"`);
  }
  return ms;
}

/**
 * Entries dated strictly after `date`. ISO dates compare correctly as
 * strings, but both sides are validated first so a malformed date fails the
 * build instead of silently matching nothing.
 */
export function entriesAfter<T extends DatedEntry>(
  entries: readonly T[],
  date: string,
): T[] {
  parseIso(date);
  return entries.filter((e) => {
    parseIso(e.date);
    return e.date > date;
  });
}

export function describeFreshness(input: FreshnessInput): Freshness {
  const { verified, proseReviewed, entries, builtOn } = input;
  parseIso(proseReviewed);
  parseIso(builtOn);

  const after = entriesAfter(entries, verified.newest);
  const areas = new Set<string>();
  for (const e of after) {
    if (e.scope) areas.add(e.scope);
  }

  return {
    verifiedNewest: verified.newest,
    verifiedOldest: verified.oldest,
    builtOn,
    proseReviewed,
    verificationUniform: verified.oldest === verified.newest,
    selectedAfterVerification: after,
    areasTouched: [...areas].sort(),
    exhaustive: false,
  };
}
