/**
 * Interim standing allowlist.
 *
 * Keyed by GitHub login. Today this is a hand-maintained file. Tomorrow it
 * becomes a query against the trust graph + governance state.
 *
 * MIGRATION: replace this static map with a call to ICN governance. The shape
 * `{ contributor, cooperativeMember, steward }` is preserved — the source
 * becomes the trust graph, not this file.
 */

export interface StandingGrants {
  /** Has acknowledged the contributor charter. */
  contributor?: boolean;
  /** List of cooperative slugs this person has member-standing in. */
  cooperativeMember?: string[];
  /** List of steward surfaces. e.g. ["deploy", "security"]. */
  steward?: string[];
}

export const standing: Record<string, StandingGrants> = {
  // Bootstrap: the primary maintainer
  "fahertym": {
    contributor: true,
    cooperativeMember: ["nycn"],
    steward: ["deploy", "security", "ops"],
  },
  // Add additional collaborators as they sign on.
};
