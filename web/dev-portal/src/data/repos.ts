/** Cross-repo state for icn.zone's Repos page. */
export interface RepoEntry {
  name: string;
  visibility: "public" | "private";
  status: "active" | "pre-pilot" | "scaffold" | "archived";
  statusEmoji: string;
  role: string;
  url: string;
  note: string;
}

export const repos: RepoEntry[] = [
  {
    name: "icn",
    visibility: "public",
    status: "active",
    statusEmoji: "🟢",
    role: "Canonical substrate. Kernel, apps, daemon, CLI, gateway, TypeScript SDK, public website.",
    url: "https://github.com/InterCooperative-Network/icn",
    note: "Phase 2 partner-bound. Substrate shipped; spec ladder dense.",
  },
  {
    name: "nycn",
    visibility: "private",
    status: "pre-pilot",
    statusEmoji: "🟢",
    role: "NYCN institution ecosystem package. First application built on ICN.",
    url: "https://github.com/InterCooperative-Network/nycn",
    note: "Drive-ingest operator ladder shipped #21–#34. Awaiting ICN-side partner gate.",
  },
  {
    name: "icn-learn",
    visibility: "private",
    status: "scaffold",
    statusEmoji: "🔵",
    role: "ICN Academy. Role-based learning and onboarding.",
    url: "https://github.com/InterCooperative-Network/icn-learn",
    note: "Scaffolding only. Material in early form.",
  },
  {
    name: "icn-community-bridge",
    visibility: "private",
    status: "scaffold",
    statusEmoji: "🔵",
    role: "Discord ↔ Matrix bridge. Community-coordination utility.",
    url: "https://github.com/InterCooperative-Network/icn-community-bridge",
    note: "Scaffold + docs only. Not deployed.",
  },
];

export const mergeOrder = "icn → nycn → icn-learn";
