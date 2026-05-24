/**
 * Shortlink table for icn.zone.
 *
 * Resolution is left-to-right by definition order in this table. The first
 * matching entry wins. Patterns support `:param` segments which are
 * substituted into `to` as `{param}`.
 *
 * Internal shortlinks (kind: "internal") route inside icn.zone — they may
 * specify a `scope` to require sign-in/standing before the redirect happens.
 *
 * External shortlinks (kind: "external") issue a 302 to an off-domain URL.
 * They resolve regardless of session scope.
 *
 * Add entries here. The convention is: shortest path that's unambiguous, no
 * hyphens unless necessary, params only when there's a real (1:N) family.
 */

export type ShortlinkEntry =
  | { kind: "external"; pattern: string; to: string; note?: string }
  | { kind: "internal"; pattern: string; to: string; scope?: "staff" | "contributor" | "cooperative-member" | "steward"; note?: string };

export const shortlinks: ShortlinkEntry[] = [
  // ============================ Software / Repos ============================
  { kind: "external", pattern: "repo",         to: "https://github.com/InterCooperative-Network/icn",                                  note: "The canonical icn repo" },
  { kind: "external", pattern: "org",          to: "https://github.com/InterCooperative-Network",                                       note: "The GitHub org" },
  { kind: "external", pattern: "nycn",         to: "https://github.com/InterCooperative-Network/nycn",                                  note: "NYCN repo (private)" },
  { kind: "external", pattern: "learn",        to: "https://github.com/InterCooperative-Network/icn-learn",                             note: "ICN Academy (private)" },
  { kind: "external", pattern: "bridge",       to: "https://github.com/InterCooperative-Network/icn-community-bridge",                  note: "Discord-Matrix bridge (private)" },

  // ============================ Project / Board ============================
  { kind: "external", pattern: "board",        to: "https://github.com/orgs/InterCooperative-Network/projects/15",                      note: "ICN System Roadmap project" },
  { kind: "external", pattern: "pr/:n",        to: "https://github.com/InterCooperative-Network/icn/pull/{n}",                          note: "Specific PR in icn" },
  { kind: "external", pattern: "issue/:n",     to: "https://github.com/InterCooperative-Network/icn/issues/{n}",                        note: "Specific issue in icn" },
  { kind: "external", pattern: "prs",          to: "https://github.com/InterCooperative-Network/icn/pulls",                             note: "Open PRs in icn" },
  { kind: "external", pattern: "issues",       to: "https://github.com/InterCooperative-Network/icn/issues",                            note: "Open issues in icn" },

  // ============================ Docs / State ============================
  { kind: "external", pattern: "state",        to: "https://github.com/InterCooperative-Network/icn/blob/main/docs/STATE.md",           note: "Living state doc" },
  { kind: "external", pattern: "phase",        to: "https://github.com/InterCooperative-Network/icn/blob/main/docs/PHASE_PROGRESS.md",  note: "Phase progress" },
  { kind: "external", pattern: "history",      to: "https://github.com/InterCooperative-Network/icn/blob/main/docs/PHASE_HISTORY.md",   note: "Completed phases" },
  { kind: "external", pattern: "arch",         to: "https://github.com/InterCooperative-Network/icn/blob/main/docs/ARCHITECTURE.md",    note: "System architecture" },
  { kind: "external", pattern: "claude",       to: "https://github.com/InterCooperative-Network/icn/blob/main/CLAUDE.md",               note: "Agent context doc" },
  { kind: "external", pattern: "agents",       to: "https://github.com/InterCooperative-Network/icn/blob/main/AGENTS.md",               note: "Agent operating instructions" },

  // ============================ Marketing crosslink ============================
  { kind: "external", pattern: "site",         to: "https://intercooperative.network",                                                  note: "The public site (long explanation surface)" },
  { kind: "external", pattern: "what",         to: "https://intercooperative.network/what-is-icn",                                      note: "What is ICN?" },
  { kind: "external", pattern: "real",         to: "https://intercooperative.network/whats-real-now",                                   note: "What's real now" },

  // ============================ ADRs and ideas ============================
  { kind: "external", pattern: "adr/:n",       to: "https://github.com/InterCooperative-Network/icn/blob/main/docs/adr/{n}.md",         note: "ADR by 4-digit number, e.g. 0026" },
  { kind: "external", pattern: "idea/:n",      to: "https://github.com/InterCooperative-Network/icn/blob/main/ops/ideas/framing/{n}.md", note: "idea-NNNN framing brief" },

  // ============================ Internal (require sign-in) ============================
  { kind: "internal", pattern: "me",           to: "/inside",                  scope: "staff",   note: "Your inside dashboard" },
  { kind: "internal", pattern: "today",        to: "/inside/standups",         scope: "staff",   note: "Today's standup" },
  { kind: "internal", pattern: "standup",      to: "/inside/standups",         scope: "staff",   note: "Standups" },
  { kind: "internal", pattern: "notices",      to: "/inside/notices",          scope: "staff",   note: "Pinned team notices" },
  { kind: "internal", pattern: "deploy",       to: "/inside/deploy",           scope: "steward", note: "Deploy status (steward)" },

  // ============================ Public-page aliases ============================
  { kind: "internal", pattern: "dash",         to: "/",                        note: "The public dashboard / landing" },
  { kind: "internal", pattern: "wiki",         to: "/wiki",                    note: "Wiki index" },
  { kind: "internal", pattern: "spec",         to: "/spec-ladder",             note: "Spec ladder" },
  { kind: "internal", pattern: "glossary",     to: "/glossary",                note: "Glossary" },
  { kind: "internal", pattern: "contrib",      to: "/contribute",              note: "Public on-ramp" },
];

/** Match a path (no leading slash) against the table. Returns the resolved
 *  entry plus the captured params, or null if no match. */
export function resolveShortlink(pathNoSlash: string): { entry: ShortlinkEntry; params: Record<string, string>; resolved: string } | null {
  for (const entry of shortlinks) {
    const params = matchPattern(entry.pattern, pathNoSlash);
    if (params) {
      const resolved = substitute(entry.to, params);
      return { entry, params, resolved };
    }
  }
  return null;
}

function matchPattern(pattern: string, path: string): Record<string, string> | null {
  const ps = pattern.split("/");
  const xs = path.split("/");
  if (ps.length !== xs.length) return null;
  const params: Record<string, string> = {};
  for (let i = 0; i < ps.length; i++) {
    const a = ps[i]!;
    const b = xs[i]!;
    if (a.startsWith(":")) {
      params[a.slice(1)] = b;
    } else if (a !== b) {
      return null;
    }
  }
  return params;
}

function substitute(template: string, params: Record<string, string>): string {
  return template.replace(/\{([^}]+)\}/g, (_, k) => params[k] ?? "");
}
