/**
 * Cooperative services catalog. The "how do I..." list — the cooperative
 * version of an HR / payroll / IT-helpdesk index.
 *
 * Each service is a question a member might have ("how do I get reimbursed"),
 * answered by a process page or external link. Today most are stubs pointing
 * at the right docs; over time each gets a real workflow page under /process/.
 */
export interface Service {
  /** The member's question, in their voice */
  question: string;
  /** Short answer / what happens */
  blurb: string;
  /** Emoji */
  icon: string;
  /** Where to go */
  href: string;
  /** Internal vs external */
  external?: boolean;
  /** Section grouping */
  group: "money" | "work" | "governance" | "people" | "infra";
}

export const services: Service[] = [
  // Money
  { question: "Get reimbursed for an expense", blurb: "Submit through the treasury proposal flow.",
    icon: "🧾", href: "/process/reimburse", group: "money" },
  { question: "Claim a sponsor stipend", blurb: "Stipend allocation is per the charter; request via proposal.",
    icon: "💵", href: "/process/stipend", group: "money" },
  { question: "See where the money is going", blurb: "Treasury page — sponsor income, expenses, runway.",
    icon: "📊", href: "/treasury", group: "money" },

  // Work
  { question: "Open a PR", blurb: "Follow PR_STACK_PROTOCOL.md. Use the template.",
    icon: "🔃", href: "https://github.com/InterCooperative-Network/icn/blob/main/ops/coordination/PR_STACK_PROTOCOL.md",
    external: true, group: "work" },
  { question: "File an issue", blurb: "Pick the right template (bug / RFC / docs / design / governance).",
    icon: "🎫", href: "https://github.com/InterCooperative-Network/icn/issues/new/choose",
    external: true, group: "work" },
  { question: "Run a release", blurb: "Pre-deployment checklist + release runbook.",
    icon: "🚢", href: "/process/release", group: "work" },
  { question: "Onboard a new contributor", blurb: "Welcome path, charter ack, allowlist update.",
    icon: "🧭", href: "/welcome", group: "people" },

  // Governance
  { question: "Propose a decision", blurb: "Open an RFC. The governance flow walks you through it.",
    icon: "🗳", href: "/process/proposal", group: "governance" },
  { question: "See open decisions", blurb: "What proposals are open, what's been decided lately.",
    icon: "📋", href: "/decisions", group: "governance" },
  { question: "Read the charter / bylaws", blurb: "How decisions are made here. Who can do what.",
    icon: "📜", href: "/charter", group: "governance" },

  // People
  { question: "Find a person", blurb: "Member directory — who's here, what they're on, how to reach.",
    icon: "👥", href: "/directory", group: "people" },
  { question: "I need help — where do I go", blurb: "Matrix channels, on-call, mutual care resources.",
    icon: "🆘", href: "/care", group: "people" },
  { question: "Resolve a conflict", blurb: "The cooperative's conflict-resolution path.",
    icon: "🕊", href: "/care#conflict", group: "people" },

  // Infra
  { question: "File a security report", blurb: "Private vulnerability advisory — not a public issue.",
    icon: "🔒", href: "https://github.com/InterCooperative-Network/icn/security/advisories/new",
    external: true, group: "infra" },
  { question: "Get access to deploy", blurb: "Steward grant via proposal. See standing.ts for current holders.",
    icon: "🔑", href: "/process/steward-grant", group: "infra" },
  { question: "Report a problem with icn.zone itself", blurb: "Open an issue with label `area:dev-portal`.",
    icon: "🐛", href: "https://github.com/InterCooperative-Network/icn/issues/new?labels=area:dev-portal",
    external: true, group: "infra" },
];

export const serviceGroups: Record<Service["group"], { label: string }> = {
  money:      { label: "Money & treasury" },
  work:       { label: "Day-to-day work" },
  governance: { label: "Decisions & governance" },
  people:     { label: "People & care" },
  infra:      { label: "Infrastructure & access" },
};
