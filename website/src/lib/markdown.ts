// src/lib/markdown.ts — Shared markdown renderer with link rewriting
import fs from "node:fs";
import path from "node:path";
import { marked } from "marked";
import { resolveRepoDocsRoot } from "./paths";

/**
 * Strip a leading YAML frontmatter block from a markdown source.
 *
 * Many ICN docs carry doc-control frontmatter (Status, Canonical,
 * Last Reviewed, Owner, Purpose) consumed by docs/scripts/doc_control_check.py.
 * Without stripping, the marked renderer treats the second `---` as a setext
 * underline and renders the YAML keys as body text on the public docs
 * explorer (e.g. /docs/design/ICN_VISUAL_EXPLAINER_BIBLE previously leaked
 * "Status: draft", "Canonical: no", "Owner: Matt Faherty", etc. as the first
 * thing a visitor saw).
 *
 * The regex anchors at the very start of the source (`^`) and only matches a
 * paired delimiter on its own line, so `---` inside body content (e.g. a
 * thematic break or a code fence) is unaffected.
 */
function stripFrontmatter(content: string): string {
  return content.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, "");
}

/**
 * Render markdown to HTML with link rewriting.
 * Relative .md links are resolved to /docs/ paths on this site,
 * or fall back to GitHub links for files we don't sync.
 */
export function renderMarkdown(content: string): string {
  const docsRoot = resolveRepoDocsRoot();
  const body = stripFrontmatter(content);

  // Build slug inventory
  const allSlugs = new Set<string>();
  function collectSlugs(dir: string, prefix: string = "") {
    try {
      const entries = fs.readdirSync(dir);
      for (const entry of entries) {
        const fp = path.join(dir, entry);
        if (fs.statSync(fp).isDirectory()) {
          collectSlugs(fp, `${prefix}${entry}/`);
        } else if (entry.endsWith(".md")) {
          allSlugs.add(`${prefix}${entry.replace(".md", "")}`);
        }
      }
    } catch {
      /* dir may not exist yet */
    }
  }
  collectSlugs(docsRoot);

  // Custom renderer for links
  const renderer = new marked.Renderer();
  renderer.link = function ({
    href,
    title: linkTitle,
    text,
  }: {
    href: string;
    title?: string | null;
    text: string;
  }) {
    if (
      !href ||
      href.startsWith("http") ||
      href.startsWith("#") ||
      href.startsWith("/")
    ) {
      const titleAttr = linkTitle ? ` title="${linkTitle}"` : "";
      const target = href?.startsWith("http")
        ? ' target="_blank" rel="noopener"'
        : "";
      return `<a href="${href}"${titleAttr}${target}>${text}</a>`;
    }

    let cleanHref = href.replace(/\.md$/, "").replace(/\.md#/, "#");
    const baseName = cleanHref.replace(/^(\.\.\/)+/, "").replace(/^\.\//, "");

    if (allSlugs.has(baseName)) {
      cleanHref = `/docs/${baseName}`;
    } else if (allSlugs.has(baseName.toUpperCase())) {
      cleanHref = `/docs/${baseName.toUpperCase()}`;
    } else {
      cleanHref = `https://github.com/InterCooperative-Network/icn/blob/main/${baseName.includes("/") ? "docs/" : ""}${baseName}`;
    }

    const anchor = href.includes("#") ? `#${href.split("#")[1]}` : "";
    const titleAttr = linkTitle ? ` title="${linkTitle}"` : "";
    return `<a href="${cleanHref}${anchor}"${titleAttr}>${text}</a>`;
  };

  return marked(body, { gfm: true, breaks: false, renderer }) as string;
}
