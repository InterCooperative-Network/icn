/** Pinned notices. Edit by hand; the landing renders the top 3. */
export interface Notice {
  /** When this was pinned (ISO date) */
  pinnedOn: string;
  /** Short headline */
  headline: string;
  /** Body — one or two sentences */
  body: string;
  /** Tag color */
  tag: "focus" | "ask" | "warn" | "info";
  /** Optional link */
  link?: { label: string; href: string };
}

export const notices: Notice[] = [
  {
    pinnedOn: "2026-05-15",
    headline: "Phase 2 — partner-bound",
    body: "We're holding, waiting on the NYCN organizer meeting. Substrate and spec ladder are shipped; what's left is human procedure.",
    tag: "focus",
    link: { label: "Phase progress →", href: "/phase" },
  },
  {
    pinnedOn: "2026-05-19",
    headline: "219 Dependabot alerts on main",
    body: "1 critical · 113 high · 87 moderate · 18 low. Need a triage pass — likely most are transitive npm deps in sdk/typescript and website.",
    tag: "warn",
    link: { label: "Dependabot →", href: "https://github.com/InterCooperative-Network/icn/security/dependabot" },
  },
  {
    pinnedOn: "2026-05-15",
    headline: "27 follow-up drafts to file",
    body: "From the architecture-spec sprint wrap-up roster. Three filed (#1834–#1840); the rest are in the wrap doc waiting for batch decisions.",
    tag: "ask",
    link: { label: "Wrap doc →", href: "https://github.com/InterCooperative-Network/icn/blob/main/docs/dev/handoff-2026-05-15.md" },
  },
];
