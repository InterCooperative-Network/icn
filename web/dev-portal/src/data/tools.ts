/**
 * Tool catalog. Every tool a member uses to do their work.
 *
 * The landing page renders the first ~8 as tiles; the full catalog lives at /tools.
 * Order matters: highest-use first.
 */
export interface Tool {
  /** Display name */
  name: string;
  /** One-line purpose */
  blurb: string;
  /** Emoji or symbol */
  icon: string;
  /** Where it goes — external URL or internal route */
  href: string;
  /** Category */
  category: "work" | "code" | "ops" | "comms" | "docs" | "money";
  /** Sign-in required to use (separately from icn.zone auth) */
  external: boolean;
  /** Show on landing's quick-access grid */
  quickAccess: boolean;
}

export const tools: Tool[] = [
  // Work
  { name: "Project Board", blurb: "ICN System Roadmap — items, fields, views",
    icon: "📋", href: "/board", category: "work", external: true, quickAccess: true },
  { name: "Open PRs",     blurb: "All open pull requests in icn",
    icon: "🔃", href: "/prs", category: "code", external: true, quickAccess: true },
  { name: "Open Issues",  blurb: "All open issues in icn",
    icon: "🎫", href: "/issues", category: "work", external: true, quickAccess: true },

  // Code
  { name: "icn repo",     blurb: "Canonical substrate code",
    icon: "📁", href: "/repo", category: "code", external: true, quickAccess: true },
  { name: "GitHub Org",   blurb: "All ICN repos",
    icon: "🏢", href: "/org", category: "code", external: true, quickAccess: false },
  { name: "nycn",         blurb: "NYCN institution package (private)",
    icon: "🔒", href: "/nycn", category: "code", external: true, quickAccess: false },

  // Ops
  { name: "Deploy status", blurb: "K3s cluster, deployed image SHA, smoke tests",
    icon: "⚙", href: "/inside/deploy", category: "ops", external: false, quickAccess: true },
  { name: "CI",           blurb: "GitHub Actions across the org",
    icon: "✅", href: "https://github.com/InterCooperative-Network/icn/actions", category: "ops", external: true, quickAccess: false },
  { name: "Dependabot",   blurb: "Vulnerability alerts (219 open right now)",
    icon: "🛡", href: "https://github.com/InterCooperative-Network/icn/security/dependabot", category: "ops", external: true, quickAccess: false },

  // Docs
  { name: "State doc",    blurb: "Living state record (per-PR sync edits)",
    icon: "📝", href: "/state", category: "docs", external: true, quickAccess: true },
  { name: "Phase progress", blurb: "Phase status with sync edits",
    icon: "📈", href: "/phase", category: "docs", external: true, quickAccess: false },
  { name: "Architecture", blurb: "Kernel/app boundary, meaning firewall",
    icon: "🏛", href: "/architecture", category: "docs", external: false, quickAccess: false },
  { name: "Spec ladder",  blurb: "May 14–15 architecture-spec docs",
    icon: "📚", href: "/spec-ladder", category: "docs", external: false, quickAccess: true },
  { name: "Wiki",         blurb: "Concept explainers, FAQ, ADR navigator",
    icon: "📖", href: "/wiki", category: "docs", external: false, quickAccess: false },
  { name: "Glossary",     blurb: "ICN vocabulary",
    icon: "🔤", href: "/glossary", category: "docs", external: false, quickAccess: false },

  // Comms (placeholders until set up)
  { name: "Matrix",       blurb: "Team chat (channels live here once set up)",
    icon: "💬", href: "https://matrix.to/#/#icn:matrix.org", category: "comms", external: true, quickAccess: true },
  { name: "Discussions",  blurb: "GitHub public conversations",
    icon: "💭", href: "https://github.com/InterCooperative-Network/icn/discussions", category: "comms", external: true, quickAccess: false },

  // Money
  { name: "Sponsors",     blurb: "GitHub Sponsors dashboard (where the money lives)",
    icon: "💰", href: "https://github.com/sponsors/InterCooperative-Network", category: "money", external: true, quickAccess: false },
];

export const toolCategories: Record<Tool["category"], { label: string; icon: string }> = {
  work:  { label: "Work",          icon: "📋" },
  code:  { label: "Code",          icon: "💻" },
  ops:   { label: "Ops",           icon: "⚙" },
  comms: { label: "Communication", icon: "💬" },
  docs:  { label: "Docs",          icon: "📚" },
  money: { label: "Money",         icon: "💰" },
};
