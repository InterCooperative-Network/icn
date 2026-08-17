// docsTree.ts — build the docs navigation tree from the published set.
//
// Shared by /docs and /docs/archive so the two pages cannot disagree about
// what exists or what it is called. Both start from the classification
// generated out of docs/registry.toml, then read each file only to resolve a
// display title.

import fs from "node:fs";
import path from "node:path";
import { resolveRepoDocsRoot } from "./paths";
import {
  publishedDocs,
  type DocLayer,
  type PublishedDoc,
} from "./docsClassification";

export interface DocEntry {
  slug: string;
  title: string;
  path: string;
  layer: DocLayer;
  /** Present on archive entries; explains why the doc is classified historical. */
  archiveReason: string | null;
  canonical: boolean;
  lastReviewed: string | null;
}

export interface DocSection {
  /** Display name, e.g. "Architecture". */
  name: string;
  /** Stable id for anchors. */
  id: string;
  docs: DocEntry[];
}

const ACRONYMS: Array<[RegExp, string]> = [
  [/\bApi\b/g, "API"],
  [/\bCcl\b/g, "CCL"],
  [/\bIcn\b/g, "ICN"],
  [/\bDid\b/g, "DID"],
  [/\bAdr\b/g, "ADR"],
  [/\bRfc\b/g, "RFC"],
  [/\bSdis\b/g, "SDIS"],
  [/\bP2p\b/g, "P2P"],
  [/\bCi\b/g, "CI"],
];

export function titleFromFilename(filename: string): string {
  let out = filename
    .replace(/\.md$/, "")
    .replace(/[-_]/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
  for (const [re, replacement] of ACRONYMS) out = out.replace(re, replacement);
  return out;
}

/**
 * Display title for a doc: frontmatter `title:` → first H1 → registry title →
 * filename transform. Any parse failure falls through to the next option.
 */
function titleForDoc(absPath: string, doc: PublishedDoc): string {
  try {
    const content = fs.readFileSync(absPath, "utf-8");

    if (content.startsWith("---")) {
      const end = content.indexOf("\n---", 3);
      if (end > 0) {
        const frontmatter = content.slice(3, end);
        const match = frontmatter.match(/^\s*title\s*:\s*(.+?)\s*$/m);
        if (match) {
          const unquoted = match[1]
            .trim()
            .replace(/^["']([\s\S]*)["']$/, "$1")
            .trim();
          if (unquoted.length > 0) return unquoted;
        }
      }
    }

    let body = content;
    if (content.startsWith("---")) {
      const end = content.indexOf("\n---", 3);
      if (end > 0) body = content.slice(end + 4);
    }
    const h1 = body.match(/^#\s+(.+?)\s*$/m);
    if (h1) return h1[1].trim();
  } catch {
    // fall through
  }

  if (doc.registryTitle) return doc.registryTitle;
  return titleFromFilename(doc.slug.split("/").pop() ?? doc.slug);
}

function toEntry(doc: PublishedDoc, docsRoot: string): DocEntry {
  const abs = path.join(docsRoot, doc.slug + ".md");
  return {
    slug: doc.slug,
    title: titleForDoc(abs, doc),
    path: `/docs/${doc.slug}`,
    layer: doc.layer,
    archiveReason: doc.archiveReason,
    canonical: doc.canonical,
    lastReviewed: doc.lastReviewed,
  };
}

/**
 * Group one layer's documents into sections by their top-level directory.
 * Root-level files land in a section named `rootSectionName`.
 */
export function sectionsForLayer(
  layer: DocLayer,
  rootSectionName = "Overview",
): DocSection[] {
  const docsRoot = resolveRepoDocsRoot();
  const byDir = new Map<string, DocEntry[]>();

  for (const doc of publishedDocs(layer)) {
    const parts = doc.slug.split("/");
    const dir = parts.length > 1 ? parts[0] : "";
    const key = dir === "" ? rootSectionName : titleFromFilename(dir);
    if (!byDir.has(key)) byDir.set(key, []);
    byDir.get(key)!.push(toEntry(doc, docsRoot));
  }

  const sections: DocSection[] = [...byDir.entries()]
    .map(([name, docs]) => ({
      name,
      id: name.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
      docs: docs.sort((a, b) => a.title.localeCompare(b.title)),
    }))
    .sort((a, b) => {
      // The root/overview section leads; everything else is alphabetical.
      if (a.name === rootSectionName) return -1;
      if (b.name === rootSectionName) return 1;
      return a.name.localeCompare(b.name);
    });

  return sections;
}

/** Flat list of entries for a set of layers — used to build the search index. */
export function entriesForLayers(layers: DocLayer[]): DocEntry[] {
  const docsRoot = resolveRepoDocsRoot();
  return layers
    .flatMap((layer) => publishedDocs(layer))
    .map((doc) => toEntry(doc, docsRoot))
    .sort((a, b) => a.title.localeCompare(b.title));
}
