// src/lib/markdown.ts — Shared markdown renderer with link rewriting
import fs from "node:fs";
import path from "node:path";
import { marked } from "marked";
import { resolveRepoDocsRoot } from "./paths";
import { isPublished } from "./docsClassification";
import { createHeadingSlugger } from "./headingSlug";

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

  // Custom renderer.
  const renderer = new marked.Renderer();

  // GitHub-compatible heading slugs, scoped to this page so duplicate
  // headings get the same numeric suffix GitHub gives them. The algorithm
  // and the reasoning behind it live in ./headingSlug.
  const headingSlug = createHeadingSlugger();

  renderer.heading = function ({
    tokens,
    depth,
  }: {
    tokens: Array<{ raw?: string; text?: string }>;
    depth: number;
  }) {
    const text = this.parser.parseInline(tokens as never);
    const id = headingSlug(text);
    return `<h${depth} id="${id}">${text}</h${depth}>\n`;
  };

  renderer.link = function ({
    href,
    title: linkTitle,
    text,
  }: {
    href: string;
    title?: string | null;
    text: string;
  }) {
    if (!href || href.startsWith("http") || href.startsWith("#")) {
      const titleAttr = linkTitle ? ` title="${linkTitle}"` : "";
      const target = href?.startsWith("http")
        ? ' target="_blank" rel="noopener"'
        : "";
      return `<a href="${href}"${titleAttr}${target}>${text}</a>`;
    }

    // Root-absolute links into the repository, e.g. `/docs/adr/ADR-0001-x.md`
    // or `/CONTRIBUTING.md`. These used to pass through untouched, producing
    // hundreds of dead routes ending in `.md` — scripts/check-links.mjs found
    // 396 of them. They are repository paths, not site paths, so resolve them
    // the same way relative links are resolved.
    if (href.startsWith("/")) {
      const titleAttr = linkTitle ? ` title="${linkTitle}"` : "";
      // A path with a file extension is a repository path, not a site route —
      // site routes are extensionless. This covers `.md` and also the
      // occasional link straight at a `.yaml`, `.toml` or `.rs` file.
      if (!/\.[a-z0-9]{1,5}(#|$)/i.test(href)) {
        // A genuine site path (e.g. /whats-real-now). Leave it alone.
        return `<a href="${href}"${titleAttr}>${text}</a>`;
      }
      const [rawPath, rawFragment] = href.split("#");
      const anchor = rawFragment ? `#${rawFragment}` : "";
      const repoPath = rawPath.replace(/^\//, "");
      const docSlug = repoPath.startsWith("docs/")
        ? repoPath.slice("docs/".length).replace(/\.md$/, "")
        : null;
      if (docSlug && allSlugs.has(docSlug) && isPublished(docSlug)) {
        return `<a href="/docs/${docSlug}${anchor}"${titleAttr}>${text}</a>`;
      }
      // Repo-root files (CONTRIBUTING.md, AGENTS.md) and withheld docs live on
      // GitHub, not on this site.
      return `<a href="https://github.com/InterCooperative-Network/icn/blob/main/${repoPath}${anchor}"${titleAttr} target="_blank" rel="noopener">${text}</a>`;
    }

    let cleanHref = href.replace(/\.md$/, "").replace(/\.md#/, "#");
    const baseName = cleanHref.replace(/^(\.\.\/)+/, "").replace(/^\.\//, "");

    // A doc that exists in the repository but is withheld from the public site
    // must not be linked as if it were published — that produces a dead /docs/
    // route. Send the reader to the source on GitHub instead, which is where
    // the content actually is. (#2609: withholding is a publication decision,
    // not a retention one, so the material stays reachable.)
    if (allSlugs.has(baseName) && isPublished(baseName)) {
      cleanHref = `/docs/${baseName}`;
    } else if (
      allSlugs.has(baseName.toUpperCase()) &&
      isPublished(baseName.toUpperCase())
    ) {
      cleanHref = `/docs/${baseName.toUpperCase()}`;
    } else if (allSlugs.has(baseName) || allSlugs.has(baseName.toUpperCase())) {
      cleanHref = `https://github.com/InterCooperative-Network/icn/blob/main/docs/${baseName}.md`;
    } else {
      cleanHref = `https://github.com/InterCooperative-Network/icn/blob/main/${baseName.includes("/") ? "docs/" : ""}${baseName}`;
    }

    const anchor = href.includes("#") ? `#${href.split("#")[1]}` : "";
    const titleAttr = linkTitle ? ` title="${linkTitle}"` : "";
    return `<a href="${cleanHref}${anchor}"${titleAttr}>${text}</a>`;
  };

  return marked(body, { gfm: true, breaks: false, renderer }) as string;
}
