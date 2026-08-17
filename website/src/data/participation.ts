// participation.ts — the public participation routes (#1368).
//
// #1368's requirement is not "list some links". It is that every public
// call-to-action resolves to a real, maintained next step, and that for each
// route we can say who it is for, what the actual next action is, where it
// goes, what maintains that destination, and what happens if the destination
// becomes unavailable.
//
// Those last two fields are the ones that usually go missing, and they are the
// reason the March 2026 version of this page went stale: it routed people to a
// Matrix room and a Buttondown list chosen before anyone had asked who would
// keep them alive. Recording the maintenance story next to the link makes the
// answer checkable, and makes an unmaintainable route obvious at the point
// someone proposes adding it.
//
// ─── The rule for adding a route ─────────────────────────────────────────────
//
// If you cannot fill in `maintainedBy` and `ifUnavailable` honestly, the route
// does not go on the site. "Someone will keep an eye on it" is not an answer.

export interface ParticipationRoute {
  id: string;
  /** Who this is for, in their own terms — not a job title we assign them. */
  audience: string;
  /** Short heading for the route. */
  title: string;
  /** What this person actually gets out of taking the route. */
  body: string;
  /** The concrete next action. A verb, not a destination. */
  nextAction: string;
  href: string;
  external?: boolean;
  /**
   * What keeps this destination alive. Must name a mechanism, not an
   * intention.
   */
  maintainedBy: string;
  /** What a visitor should do if this route is broken or unresponsive. */
  ifUnavailable: string;
}

export const PARTICIPATION_ROUTES: ParticipationRoute[] = [
  {
    id: "developers",
    audience: "Technical contributors",
    title: "Write code, tests, or tooling",
    body: "The repository is a large Rust workspace with a real contribution surface: kernel and app crates, a gateway, a CLI, and a documentation control plane. The architectural guardrails are written down, and they are enforced — reading them first will save you a review round.",
    nextAction: "Read the contributing guide, then build the workspace",
    href: "https://github.com/InterCooperative-Network/icn/blob/main/CONTRIBUTING.md",
    external: true,
    maintainedBy:
      "Lives in the repository and is updated with the code it describes. Broken setup steps are ordinary bugs and get fixed like any other.",
    ifUnavailable:
      "Open an issue on the repository. A broken contributor path is treated as a defect, not a documentation nicety.",
  },
  {
    id: "first-issue",
    audience: "Technical contributors, first time",
    title: "Pick a scoped first issue",
    body: "A small, deliberately curated set of issues that are genuinely self-contained — they have a clear boundary, a stated expectation, and do not require holding the whole architecture in your head. The label is not applied to make the project look welcoming; if an issue turns out to be larger than advertised, say so on the issue.",
    nextAction: "Browse the open good-first-issue label",
    href: "https://github.com/InterCooperative-Network/icn/issues?q=is%3Aissue+is%3Aopen+label%3Agood-first-issue",
    external: true,
    maintainedBy:
      "Curated by hand on the repository. Issues are removed from the label when they stop being newcomer-sized.",
    ifUnavailable:
      "If the list is empty, that is accurate rather than broken — ask in Discussions what would be useful right now.",
  },
  {
    id: "organizers",
    audience: "Organizers, writers, researchers, designers",
    title: "Contribute without writing code",
    body: "The project is short on plain-language review, accessibility testing on real assistive technology, governance and policy thinking, translation, and documentation that a non-engineer can follow. This is not filler work — the member-facing surface is the part of ICN furthest behind, and that is a design and language problem at least as much as an engineering one.",
    nextAction: "Start a discussion describing what you would like to work on",
    href: "https://github.com/InterCooperative-Network/icn/discussions/new?category=general",
    external: true,
    maintainedBy:
      "GitHub Discussions on the main repository, watched by the maintainers alongside issues.",
    ifUnavailable:
      "Use the Discord server instead — it does not require a GitHub account.",
  },
  {
    id: "chat",
    audience: "Anyone, without a GitHub account",
    title: "Ask a question somewhere informal",
    body: "Not every question is worth opening an issue for, and requiring a GitHub account to ask one narrows who can participate to people already comfortable with developer tooling. The chat server is the doorway that does not.",
    nextAction: "Join the Discord server",
    href: "https://discord.gg/sAEFCnEahn",
    external: true,
    maintainedBy:
      "Run by the project. It is the deliberate non-GitHub entry point, and exists so participation does not require a developer account.",
    ifUnavailable:
      "If the invite has expired, open an issue on the repository saying so — an expired invite is a broken front door.",
  },
  {
    id: "institutions",
    audience: "Cooperatives, communities, federations",
    title: "Work out whether ICN could carry anything for you",
    body: 'The honest current answer for most institutions is "not yet, and here is specifically why". Before any conversation about adoption, it is worth understanding what exists, what is demonstrated only against fixtures, and what the project has not built. That page is written to be read by someone deciding whether to spend their organisation\'s time.',
    nextAction:
      "Read the institutional evaluation page, then open a discussion",
    href: "/for-cooperatives",
    maintainedBy:
      "A page on this site, written against the same generated project-state data as the maturity account.",
    ifUnavailable:
      "Read What's real now directly — it carries the per-subsystem claims and their evidence.",
  },
  {
    id: "follow",
    audience: "People who want project updates",
    title: "Follow the work without joining it",
    body: "There is no mailing list. Setting one up would mean asking you to trust that we will actually send it, and the project would rather point at something that updates whether or not anyone remembers to write a newsletter. Watching releases on the repository, or reading the recent-state-changes section of the maturity page, both do that.",
    nextAction: "Watch the repository, or read recent state changes",
    href: "https://github.com/InterCooperative-Network/icn",
    external: true,
    maintainedBy:
      "GitHub's own watch mechanism. Nothing for the project to forget to do.",
    ifUnavailable:
      "The recent-state-changes section of What's real now is generated from the repository on every build.",
  },
  {
    id: "support",
    audience: "Supporters and funders",
    title: "Sustain the engineering work",
    body: "GitHub Sponsors is the only funding rail the project currently has. There is no foundation, no token, no equity, and no membership tier that buys influence over decisions — sponsoring supports the work and does not purchase standing in any institution ICN serves.",
    nextAction: "Sponsor the project on GitHub",
    href: "https://github.com/sponsors/InterCooperative-Network",
    external: true,
    maintainedBy:
      "GitHub Sponsors, tied to the organisation account. No separate payment infrastructure to maintain or lose.",
    ifUnavailable:
      "Open a discussion — the project would rather talk about it than route money through something improvised.",
  },
];

/** Where the work is visible day to day. Distinct from a participation route. */
export const WORKING_SURFACES = [
  {
    label: "Source and issues",
    href: "https://github.com/InterCooperative-Network/icn",
    blurb:
      "Everything the project builds, in public, including the parts that are not finished.",
  },
  {
    label: "Discussions",
    href: "https://github.com/InterCooperative-Network/icn/discussions",
    blurb: "Design and institutional questions that are not yet issues.",
  },
  {
    label: "Decision record",
    href: "/docs#decisions",
    blurb: "The ADRs and RFCs behind why the system is shaped the way it is.",
  },
  {
    label: "Current project state",
    href: "/whats-real-now",
    blurb:
      "What is built, how it is evidenced, and what the project records as missing.",
  },
];
