// concepts.ts — typed access to the canonical ICN concept vocabulary.
//
// The data comes from docs/design-language/concept-map.md via
// scripts/gen-concepts.mjs. The website never authors a public label for an
// ICN concept; it reads the one the concept map already defines, so the
// wording on the site and the wording in the design language cannot drift.
//
// See src/components/Term.astro for the rendering conventions.

import generated from "../data/concepts.generated.json";

export interface Concept {
  /** The stable internal term used in code and architecture ("standing"). */
  canonical: string;
  /** Heading the concept appears under in the concept map ("Standing"). */
  heading: string;
  /** Which group of the concept map it belongs to. */
  group: string | null;
  /** Plain-language label for public surfaces ("Your recognized status"). */
  public: string;
  /** One-line gloss, suitable for helper text under the label. */
  short: string;
  /** Design-token name, when the concept map assigns one. */
  colorToken: string | null;
  /** "01".."09" for closure-loop stations, otherwise null. */
  loopPosition: string | null;
  /** Icon registry slug, when the concept map assigns one. */
  iconSlug: string | null;
}

interface ConceptData {
  source: string;
  sourceHash: string;
  conceptCount: number;
  order: string[];
  concepts: Record<string, Concept>;
}

const data = generated as ConceptData;

export const CONCEPT_SOURCE = data.source;

/** Every canonical concept id, in concept-map order. */
export const CONCEPT_IDS = data.order;

/**
 * Look up a concept by its canonical id.
 *
 * Throws rather than returning undefined: a missing concept means a page is
 * referring to vocabulary the design language does not define, and rendering
 * an empty span would hide that. Astro surfaces the throw at build time.
 */
export function concept(id: string): Concept {
  const found = data.concepts[id];
  if (!found) {
    throw new Error(
      `Unknown ICN concept "${id}". Concepts come from ${data.source}; ` +
        `add it there rather than inventing a label on the website. ` +
        `Known ids: ${data.order.join(", ")}`,
    );
  }
  return found;
}

/** The nine closure-loop stations, in loop order. */
export function loopStations(): Concept[] {
  return data.order
    .map((id) => data.concepts[id])
    .filter((c): c is Concept => Boolean(c.loopPosition))
    .sort((a, b) => (a.loopPosition ?? "").localeCompare(b.loopPosition ?? ""));
}

/** All concepts in a named concept-map group. */
export function conceptsInGroup(group: string): Concept[] {
  return data.order
    .map((id) => data.concepts[id])
    .filter((c) => c.group === group);
}
