// readingOrder.ts — the canonical public reading-order ladder.
//
// This is the sequence the homepage's "Reading order" list points to and
// the sequence the ReadingLadder component renders at the bottom of every
// narrative page.
//
// IMPORTANT: this is *only* the public narrative ladder. Not every page
// is on the ladder. The homepage (/) is the entry point, not a step.
// /about and /architecture are intentionally outside the ladder. The
// ladder must not pretend they belong to it.
//
// Step 5 forks: For Cooperatives and For Developers are alternate
// endpoints, not sequential. A reader picks one based on who they are.
// The component renders this fork explicitly.

export interface LadderStep {
  /** Sequence number, "01" through "05". */
  n: string;
  /** Page slug (matches activeNav prop on each page). */
  slug: string;
  /** Public-facing label. */
  label: string;
  /** href the step links to. */
  href: string;
}

export interface LadderFork {
  /** Sequence number for the forked endpoint. */
  n: string;
  /** Two parallel options. */
  options: LadderStep[];
}

/** The five steps of the public narrative ladder.
    Step 5 is a fork — see LADDER_FORK below. */
export const LADDER_STEPS: LadderStep[] = [
  {
    n: '01',
    slug: 'what-is-icn',
    label: 'What is ICN',
    href: '/what-is-icn',
  },
  {
    n: '02',
    slug: 'why-icn',
    label: 'Why ICN',
    href: '/why-icn',
  },
  {
    n: '03',
    slug: 'how-it-works',
    label: 'How It Works',
    href: '/how-it-works',
  },
  {
    n: '04',
    slug: 'whats-real-now',
    label: "What's Real Now",
    href: '/whats-real-now',
  },
];

/** Step 5 is a fork: a reader picks an audience, not a sequence. */
export const LADDER_FORK: LadderFork = {
  n: '05',
  options: [
    {
      n: '05',
      slug: 'for-cooperatives',
      label: 'For Cooperatives',
      href: '/for-cooperatives',
    },
    {
      n: '05',
      slug: 'for-developers',
      label: 'For Developers',
      href: '/for-developers',
    },
  ],
};

/** Slugs that are valid `current` values for the ReadingLadder.
    Anything else gets rejected at the component level so the ladder
    can't be misused on a page that isn't part of the sequence. */
export const LADDER_SLUGS = [
  ...LADDER_STEPS.map((s) => s.slug),
  ...LADDER_FORK.options.map((o) => o.slug),
] as const;

export type LadderSlug = (typeof LADDER_SLUGS)[number];

/** Get the previous and next steps for a given slug. Used to render
    the "←" and "→" navigation hints around the current position.
    For the forked endpoint slugs, prev = step 04 and next = null. */
export function getNeighbors(slug: LadderSlug): {
  prev: LadderStep | null;
  next: LadderStep | LadderFork | null;
} {
  const linearIdx = LADDER_STEPS.findIndex((s) => s.slug === slug);
  if (linearIdx >= 0) {
    const prev = linearIdx > 0 ? LADDER_STEPS[linearIdx - 1] : null;
    const next =
      linearIdx === LADDER_STEPS.length - 1
        ? LADDER_FORK
        : LADDER_STEPS[linearIdx + 1];
    return { prev, next };
  }
  // Slug must be one of the fork endpoints.
  const isFork = LADDER_FORK.options.some((o) => o.slug === slug);
  if (isFork) {
    return {
      prev: LADDER_STEPS[LADDER_STEPS.length - 1],
      next: null,
    };
  }
  return { prev: null, next: null };
}
