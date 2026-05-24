/**
 * Funding pipeline data — mirrors docs/strategy/grants/funding-pipeline.md.
 *
 * Edit when status changes (submitted, awarded, declined, deadline passed).
 * Source of truth is the markdown doc; this file is the rendering layer.
 */

export type FundingStatus =
  | "apply-now"      // open call, ready to submit
  | "prep"           // needs work before submission
  | "watch"          // cycle closed, monitor next
  | "relationship"   // cultivate; not a cold application
  | "in-motion"      // already engaged
  | "skip";          // researched and ruled out

export type FundingTier = 1 | 2 | 3 | 4 | 5;

export interface FundingTarget {
  name: string;
  url: string;
  status: FundingStatus;
  tier: FundingTier;
  amount: string;
  deadline: string;
  fit: string;
  application?: string; // local file path under docs/strategy/grants/applications/
}

export const fundingTargets: FundingTarget[] = [
  // Tier 1 — Strongest fit
  {
    name: "NLnet NGI Zero Commons Fund",
    url: "https://nlnet.nl/commonsfund/",
    status: "apply-now",
    tier: 1,
    amount: "€5K–€50K (scales up)",
    deadline: "2026-06-01 (next; every 2 months)",
    fit: "Direct. P2P infrastructure, decentralized identity, federated governance. Non-EU eligible.",
    application: "nlnet-ngi-zero-commons.md",
  },
  {
    name: "Sovereign Tech Fund",
    url: "https://www.sovereign.tech/programs/fund",
    status: "apply-now",
    tier: 1,
    amount: "€50K–€1M",
    deadline: "Rolling",
    fit: "Direct. Open digital base technology. ⚠️ Cannot stack with other public funding on same activity — carve slice carefully.",
    application: "sovereign-tech-fund.md",
  },
  {
    name: "NLnet NGI Fediversity",
    url: "https://nlnet.nl/fediversity/",
    status: "apply-now",
    tier: 1,
    amount: "€5K–€50K",
    deadline: "Every 2 months",
    fit: "ICN is federated cooperative infrastructure. EU priority but non-EU possible for exceptional quality.",
  },
  {
    name: "NLnet NGI TALER",
    url: "https://nlnet.nl/taler/",
    status: "prep",
    tier: 1,
    amount: "€5K–€50K",
    deadline: "2026-06-01 (currently open)",
    fit: "Stretch — TALER is payment-focused, ICN intentionally avoids payment vocabulary. Skip unless we have a clean settlement-primitives angle.",
  },

  // Tier 2 — Moderate fit, requires positioning
  {
    name: "Mozilla Democracy x AI Cohort 2026",
    url: "https://www.mozillafoundation.org/en/what-we-do/grantmaking/incubator/democracy-ai-cohort/",
    status: "prep",
    tier: 2,
    amount: "Up to $50K + 12-week cohort",
    deadline: "2026 cohort — verify window",
    fit: "Reframe ICN's governance substrate as the 'AI-resistant community-led governance' pillar. Direct hit on a named priority area.",
    application: "mozilla-democracy-ai.md",
  },
  {
    name: "Capital Impact Co-op Innovation Award",
    url: "https://www.capitalimpact.org/programs/co-op-innovation-awards/",
    status: "watch",
    tier: 2,
    amount: "$10K–$50K",
    deadline: "Annual; 2027 cycle (2026 already awarded)",
    fit: "Worker-coop preference. Best play: NYCN applies with ICN as substrate vendor in 2027.",
  },
  {
    name: "NGI Sargasso",
    url: "https://ngisargasso.eu/",
    status: "relationship",
    tier: 2,
    amount: "Cascade funding €40K–€150K typical",
    deadline: "Open Call #4 currently",
    fit: "Requires EU+US/Canada consortium. Multi-month relationship building required. EU partner search in progress.",
    application: "ngi-sargasso-eu-partner-search.md",
  },
  {
    name: "Mozilla Builders Accelerator",
    url: "https://builders.mozilla.org/programs/",
    status: "skip",
    tier: 2,
    amount: "Up to $100K + accelerator",
    deadline: "Current theme: Local AI",
    fit: "Not a fit under current Local AI theme. Watch for future themes.",
  },

  // Tier 3 — Warm leads
  {
    name: "ACCES-VR NY Self-Employment Plan",
    url: "https://www.acces.nysed.gov/vr",
    status: "in-motion",
    tier: 3,
    amount: "Up to $15K",
    deadline: "Process-based",
    fit: "Existing engagement. Closest near-term money. SSDI-compatible structure. Push the IPE forward.",
    application: "acces-vr-self-employment.md",
  },
  {
    name: "Cooperative Fund of the Northeast (CFNE)",
    url: "https://cooperativefund.org/",
    status: "relationship",
    tier: 3,
    amount: "CDFI loans + TA, not grants",
    deadline: "Ongoing",
    fit: "Contact: Joe Marraffino. Network play — introductions to coop-tech grant decision-makers.",
  },
  {
    name: "Institute for Cooperative Digital Economy (ICDE)",
    url: "https://newschool.edu/icde/",
    status: "relationship",
    tier: 3,
    amount: "Co-applicant role on research grants",
    deadline: "Ongoing",
    fit: "Contact: Frank Cetera. Co-applicant for research / public-interest tech pitches.",
  },
  {
    name: "GitHub Sponsors",
    url: "https://github.com/sponsors/InterCooperative-Network",
    status: "in-motion",
    tier: 3,
    amount: "Ongoing recurring",
    deadline: "Continuous",
    fit: "Active. Continue. Polish narrative.",
  },
  {
    name: "Open Source Collective (Open Collective)",
    url: "https://oscollective.org/",
    status: "prep",
    tier: 3,
    amount: "Fiscal hosting (501(c)(6))",
    deadline: "—",
    fit: "Evaluate vs current fiscal sponsor (Alchemical). Possibly better fee structure.",
  },

  // Tier 4 — Watch, indirect
  {
    name: "Sloan Foundation Open Source in Science",
    url: "https://sloan.org/programs/digital-technology/open-source-in-science",
    status: "watch",
    tier: 4,
    amount: "$750K to OSPOs / institutions",
    deadline: "By invitation / RFP",
    fit: "Funds OSPOs at universities. RIT (Rochester) has one — partnership angle.",
  },
  {
    name: "Ford Foundation Public Interest Tech",
    url: "https://www.fordfoundation.org/work/challenging-inequality/technology-and-society/public-interest-technology-and-its-origins/",
    status: "relationship",
    tier: 4,
    amount: "Varies",
    deadline: "By invitation / RFPs",
    fit: "2019 Critical Digital Infrastructure RFP with Sloan/Mozilla/OSF/Omidyar may recur. Enter via network.",
  },
  {
    name: "Filecoin Foundation ProPGF",
    url: "https://www.filecoin.io/blog/posts/introducing-fil-propgf-a-new-era-in-community-led-public-goods-funding-for-the-filecoin-ecosystem/",
    status: "watch",
    tier: 4,
    amount: "$3M+ batches; six-month rounds",
    deadline: "Two more 2026 rounds planned",
    fit: "Filecoin-ecosystem framing required. ICN's settlement vocabulary makes this awkward but possible.",
  },
  {
    name: "Knight Foundation Tech & Democracy",
    url: "https://knightfoundation.org/topics/technology-and-democracy/",
    status: "watch",
    tier: 4,
    amount: "Varies",
    deadline: "Open challenges occasionally",
    fit: "Focused on 26 cities (Rochester not on list). Open challenges only.",
  },
  {
    name: "Open Society Foundations",
    url: "https://www.opensocietyfoundations.org/grants",
    status: "relationship",
    tier: 4,
    amount: "Multi-year strategic",
    deadline: "By invitation",
    fit: "Enter via Joe Marraffino / Frank Cetera network introductions.",
  },
  {
    name: "Schmidt Sciences",
    url: "https://grantedai.com/grants/schmidt-sciences-trustworthy-ai-rfp-2026-safety-governance-research-schmidt-a1b2c3d4",
    status: "skip",
    tier: 4,
    amount: "$1M–$5M",
    deadline: "2026 Trustworthy AI",
    fit: "ICN isn't AI-focused.",
  },
  {
    name: "Plurality Institute",
    url: "https://www.plurality.institute/",
    status: "relationship",
    tier: 4,
    amount: "Not a grantmaker",
    deadline: "—",
    fit: "Academic research network. Contact for visibility / EU-side network introductions.",
  },

  // Tier 5 — Local NY (mostly skip)
  {
    name: "Excell Partners (Upstate NY pre-seed)",
    url: "https://www.excellny.com/",
    status: "skip",
    tier: 5,
    amount: "Equity venture",
    deadline: "—",
    fit: "Dilutive capital; cooperative ethos doesn't fit.",
  },
  {
    name: "NextCorps (Finger Lakes incubator)",
    url: "https://nextcorps.org/",
    status: "relationship",
    tier: 5,
    amount: "Resources + community",
    deadline: "—",
    fit: "Local. Worth a coffee meeting. Not direct grants.",
  },
  {
    name: "USDA Rural Cooperative Development Grants",
    url: "https://www.rd.usda.gov/programs-services/business-programs/rural-cooperative-development-grant-program",
    status: "skip",
    tier: 5,
    amount: "Federal",
    deadline: "Annual (FY26 cycle TBD)",
    fit: "Only nonprofit institutions can apply. ICN not eligible directly. Could apply via a coop-dev nonprofit partner.",
  },
];

export const fundingStatusLabel: Record<FundingStatus, { label: string; tone: "info" | "warn" | "critical" | "neutral" }> = {
  "apply-now":    { label: "Apply now", tone: "info" },
  "prep":         { label: "Prep", tone: "warn" },
  "watch":        { label: "Watch", tone: "neutral" },
  "relationship": { label: "Cultivate", tone: "warn" },
  "in-motion":    { label: "In motion", tone: "info" },
  "skip":         { label: "Skip", tone: "neutral" },
};

export const tierLabel: Record<FundingTier, string> = {
  1: "Tier 1 — Strongest fit",
  2: "Tier 2 — Moderate fit",
  3: "Tier 3 — Warm leads",
  4: "Tier 4 — Watch, indirect",
  5: "Tier 5 — Local NY",
};

export function pipelineSummary() {
  const total = fundingTargets.length;
  const applyNow = fundingTargets.filter(t => t.status === "apply-now").length;
  const inMotion = fundingTargets.filter(t => t.status === "in-motion").length;
  const prep = fundingTargets.filter(t => t.status === "prep").length;
  const relationship = fundingTargets.filter(t => t.status === "relationship").length;
  return { total, applyNow, inMotion, prep, relationship };
}
