// ICN Icon Sprite — canonical institutional icons for the design language.
//
// Each icon maps to a canonical concept id from
// docs/design-language/concept-map.md. The paths use currentColor for stroke
// and are designed for a 24x24 viewBox with 1.5px strokes and round caps.
//
// Style rules (enforced by review, not runtime):
//   - stroke-width: 1.5 (inherited from the Icon component)
//   - stroke-linecap: round
//   - stroke-linejoin: round
//   - no fills (fill="none" on the parent svg)
//   - no color baked into paths — parent inherits via currentColor
//   - designed to read cleanly at 16-20px
//   - silhouettes distinct from each other at small sizes
//   - institutional line-art: calm, precise, not decorative
//
// Adding an icon:
//   1. Add an entry to the ICONS map keyed by concept id.
//   2. Provide { label, description, paths } — paths is an array of
//      JSX-ready path strings (just the "d" attribute value).
//   3. Pair with a canonical concept in docs/design-language/concept-map.md
//      so the concept map can reference the icon slug.
//   4. Review at 16px, 20px, and 24px. Silhouette should remain distinct.

export interface IconDef {
  /** Canonical concept id (matches concept-map.md). */
  id: string;
  /** Default accessible label when the icon is used standalone. */
  label: string;
  /** Short description — what this icon represents. */
  description: string;
  /** Array of SVG path "d" values rendered as <path d={...} /> */
  paths: string[];
  /** Optional circles, rects, or other primitive shapes. */
  extras?: Array<
    | { type: "circle"; cx: number; cy: number; r: number; fill?: string }
    | { type: "line"; x1: number; y1: number; x2: number; y2: number }
  >;
}

export const ICONS: Record<string, IconDef> = {
  // ─── Closure loop stations ───────────────────────────────────
  identity: {
    id: "identity",
    label: "Identity",
    description: "Cryptographic identity held by the member.",
    // A person silhouette with a small key mark — identity as held, not issued.
    paths: [
      // head
      "M12 3.75a3.25 3.25 0 1 0 0 6.5 3.25 3.25 0 0 0 0-6.5Z",
      // shoulders / body
      "M5.5 20.25c0-3.25 2.9-5.75 6.5-5.75s6.5 2.5 6.5 5.75",
    ],
  },

  standing: {
    id: "standing",
    label: "Standing",
    description: "Provable participation inside a scope.",
    // A badge/seal with a check: recognized status as verifiable claim.
    paths: [
      // shield/badge outline
      "M12 3.25 5.5 6v5.5c0 4.25 2.75 7.75 6.5 9.25 3.75-1.5 6.5-5 6.5-9.25V6L12 3.25Z",
      // check mark inside
      "m9 11.75 2.25 2.25L15.25 10",
    ],
  },

  authority: {
    id: "authority",
    label: "Authority",
    description: "What you are allowed to do, derived from scope rules.",
    // A rounded rectangular seal-stamp: institutional mandate, not "power."
    paths: [
      // outer seal
      "M5 7.5A2.5 2.5 0 0 1 7.5 5h9A2.5 2.5 0 0 1 19 7.5v9a2.5 2.5 0 0 1-2.5 2.5h-9A2.5 2.5 0 0 1 5 16.5v-9Z",
      // inner ring
      "M12 9.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5Z",
      // stamp base line
      "M8 19.25h8",
    ],
  },

  governance: {
    id: "governance",
    label: "Governance",
    description: "How group decisions are made.",
    // Three nodes arranged around a center point — an assembly / council
    // meeting around a shared decision. Simpler silhouette than three full
    // figures, reads cleanly at 16-22px.
    paths: [
      // top node
      "M12 4.5a2 2 0 1 0 0 4 2 2 0 0 0 0-4Z",
      // bottom-left node
      "M5.5 15a2 2 0 1 0 0 4 2 2 0 0 0 0-4Z",
      // bottom-right node
      "M18.5 15a2 2 0 1 0 0 4 2 2 0 0 0 0-4Z",
      // center dot — the shared matter being decided
      "M12 13.25a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Z",
      // connecting lines from each node to center
      "M12 8.5 12 11.5",
      "m7 14.5 4-2",
      "m17 14.5-4-2",
    ],
  },

  policy: {
    id: "policy",
    label: "Policy",
    description: "The shared rules that bind practice.",
    // A document with horizontal rule-lines: policy as written, binding rules.
    paths: [
      // document frame
      "M6.75 4.25h8.5l3 3v11.5a1.5 1.5 0 0 1-1.5 1.5H6.75a1.5 1.5 0 0 1-1.5-1.5V5.75a1.5 1.5 0 0 1 1.5-1.5Z",
      // fold corner
      "M15.25 4.25v3h3",
      // rule lines
      "M8.5 12h7",
      "M8.5 15h7",
      "M8.5 9.5h3.5",
    ],
  },

  accounting: {
    id: "accounting",
    label: "Accounting",
    description: "Obligations, allocations, and mutual-credit positions.",
    // A two-pan balance: relational accounting, not commerce.
    paths: [
      // vertical staff
      "M12 4.25v15.5",
      // horizontal beam
      "M5.25 8.25h13.5",
      // left pan + chains
      "M5.25 8.25 3 13.5h4.5L5.25 8.25Z",
      // right pan + chains
      "M18.75 8.25 16.5 13.5H21l-2.25-5.25Z",
      // base
      "M8.5 19.75h7",
    ],
  },

  execution: {
    id: "execution",
    label: "Execution",
    description: "Where accepted decisions become operational effect.",
    // An arrow passing through a threshold: decision becoming action.
    paths: [
      // threshold/gate lines
      "M5.25 8.5v7",
      "M18.75 8.5v7",
      // arrow shaft
      "M5.25 12h13.5",
      // arrowhead
      "m15 8.5 3.75 3.5L15 15.5",
    ],
  },

  provenance: {
    id: "provenance",
    label: "Receipts & Provenance",
    description: "The chain from authority to outcome.",
    // Three linked rectangular records — institutional memory as a chain.
    paths: [
      // record 1
      "M3.25 7.75h5v8.5h-5z",
      // record 2
      "M9.5 7.75h5v8.5h-5z",
      // record 3
      "M15.75 7.75h5v8.5h-5z",
      // link between 1 and 2
      "M8.25 12h1.25",
      // link between 2 and 3
      "M14.5 12h1.25",
    ],
  },

  federation: {
    id: "federation",
    label: "Federation",
    description: "How distinct institutions coordinate without merging.",
    // Three circles connected by lines, no central hub — plurality, not control.
    paths: [
      // circle 1 (top)
      "M12 3.25a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5Z",
      // circle 2 (bottom left)
      "M5.5 14.25a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5Z",
      // circle 3 (bottom right)
      "M18.5 14.25a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5Z",
      // connecting lines
      "M10.5 7 7 13",
      "M13.5 7 17 13",
      "M8 16.75h8",
    ],
  },

  commons: {
    id: "commons",
    label: "Commons",
    description: "Shared resources governed by participating institutions.",
    // A shared field/grid with four corners — common ground.
    paths: [
      // outer frame
      "M4.25 4.25h15.5v15.5H4.25z",
      // cross-hatch field
      "M4.25 9.5h15.5",
      "M4.25 14.5h15.5",
      "M9.5 4.25v15.5",
      "M14.5 4.25v15.5",
    ],
  },

  member_experience: {
    id: "member_experience",
    label: "Member Experience",
    description:
      "Where the member inhabits the system through a surface they can see and act on.",
    // A rounded surface frame with a simplified person silhouette inside.
    // The frame = the member-facing layer; the person = the member inhabiting
    // it. Distinct from `identity` (person without frame) and from
    // `authority` (rounded seal with a small centered circle inside):
    // here the inner element is a head + shoulders, not a ring, so the
    // silhouette reads as "person in context" even at 16px.
    paths: [
      // outer rounded-rect frame — the surface the member inhabits
      "M5.25 5A2.25 2.25 0 0 1 7.5 2.75h9A2.25 2.25 0 0 1 18.75 5v14A2.25 2.25 0 0 1 16.5 21.25h-9A2.25 2.25 0 0 1 5.25 19V5Z",
      // small person head inside the frame
      "M12 8.25a1.75 1.75 0 1 0 0 3.5 1.75 1.75 0 0 0 0-3.5Z",
      // small person shoulders inside the frame
      "M8.75 17.25c0-1.75 1.45-3 3.25-3s3.25 1.25 3.25 3",
    ],
  },
};

/** Ordered list of the nine closure-loop stations for use in ClosureLoop.
    All nine stations now have canonical icons. */
export const CLOSURE_STATION_ICONS: Array<keyof typeof ICONS> = [
  "identity",
  "standing",
  "authority",
  "governance",
  "policy",
  "accounting",
  "execution",
  "provenance",
  "member_experience",
];

export type IconName = keyof typeof ICONS;
