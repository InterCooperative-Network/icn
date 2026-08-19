// docsClassification.ts — typed access to the public docs classification.
//
// Data comes from docs/registry.toml via scripts/gen-docs-classification.mjs.
// The website does not decide, per file, whether something is current or
// historical — the repository's documentation control plane already decided,
// and this module reads that decision.
//
// See src/pages/docs/index.astro and src/pages/docs/[...slug].astro for how
// the four layers are presented.

import generated from "../data/docs-classification.generated.json";

/** The four public docs layers defined by #2609. */
export type DocLayer = "learn" | "reference" | "decisions" | "archive";

export interface PublishedDoc {
  /** Repo-relative path, e.g. "docs/architecture/FOO.md". */
  rel: string;
  /** URL slug under /docs/, e.g. "architecture/FOO". */
  slug: string;
  publish: true;
  layer: DocLayer;
  /** Why this landed in the archive layer. Null unless layer === "archive". */
  archiveReason: string | null;
  truthClass: string | null;
  role: string | null;
  canonical: boolean;
  status: string | null;
  lastReviewed: string | null;
  /** Path or reference to the document that replaced this one, if known. */
  supersededBy: string | null;
  registryTitle: string | null;
  registryDescription: string | null;
}

export interface WithheldDoc {
  rel: string;
  slug: string;
  publish: false;
  withheldReason: string;
}

export type DocRecord = PublishedDoc | WithheldDoc;

interface ClassificationData {
  source: string;
  counts: {
    total: number;
    published: number;
    withheld: number;
    learn: number;
    reference: number;
    decisions: number;
    archive: number;
  };
  withheldByReason: Record<string, number>;
  entries: Record<string, DocRecord>;
}

const data = generated as unknown as ClassificationData;

export const DOCS_SOURCE = data.source;
export const DOCS_COUNTS = data.counts;
export const DOCS_WITHHELD_BY_REASON = data.withheldByReason;

/**
 * Classification for one slug, or undefined if the file is not in the
 * generated set at all (which should not happen — the generator walks the
 * same tree the page renderer does).
 */
export function classify(slug: string): DocRecord | undefined {
  return data.entries[slug];
}

/** True when this slug may be rendered as a public page. */
export function isPublished(slug: string): boolean {
  return data.entries[slug]?.publish === true;
}

/** Every published doc, optionally narrowed to one layer. */
export function publishedDocs(layer?: DocLayer): PublishedDoc[] {
  const all = Object.values(data.entries).filter(
    (e): e is PublishedDoc => e.publish === true,
  );
  return layer ? all.filter((d) => d.layer === layer) : all;
}

/**
 * Human-facing description of each layer. Used on the docs landing page so a
 * reader knows what they are choosing between before they click.
 */
export const LAYER_META: Record<
  DocLayer,
  { title: string; blurb: string; href: string }
> = {
  learn: {
    title: "Learn",
    blurb:
      "Start here. Orientation, guides, worked examples, and the glossary — written to be read in order rather than looked up.",
    href: "/docs#learn",
  },
  reference: {
    title: "Current reference",
    blurb:
      "The working documentation for the system as it stands: architecture, specifications, APIs, and operational detail.",
    href: "/docs#reference",
  },
  decisions: {
    title: "Decisions — ADRs and RFCs",
    blurb:
      "Why the system is shaped the way it is. Architecture decision records and requests for comment, including the ones that were superseded.",
    href: "/docs#decisions",
  },
  archive: {
    title: "Archive",
    blurb:
      "Historical material kept for the record. Old plans, completed phases, and superseded documents — accurate about their moment, not about the system today.",
    href: "/docs/archive",
  },
};
