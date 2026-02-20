// src/lib/markdown.ts — Shared markdown renderer with link rewriting
import fs from 'node:fs';
import path from 'node:path';
import { marked } from 'marked';

/**
 * Render markdown to HTML with link rewriting.
 * Relative .md links are resolved to /docs/ paths on this site,
 * or fall back to GitHub links for files we don't sync.
 */
export function renderMarkdown(content: string): string {
    const docsRoot = path.join(process.cwd(), 'src/content/docs');

    // Build slug inventory
    const allSlugs = new Set<string>();
    function collectSlugs(dir: string, prefix: string = '') {
        try {
            const entries = fs.readdirSync(dir);
            for (const entry of entries) {
                const fp = path.join(dir, entry);
                if (fs.statSync(fp).isDirectory()) {
                    collectSlugs(fp, `${prefix}${entry}/`);
                } else if (entry.endsWith('.md')) {
                    allSlugs.add(`${prefix}${entry.replace('.md', '')}`);
                }
            }
        } catch { /* dir may not exist yet */ }
    }
    collectSlugs(docsRoot);

    // Custom renderer for links
    const renderer = new marked.Renderer();
    renderer.link = function ({ href, title: linkTitle, text }: { href: string; title?: string | null; text: string }) {
        if (!href || href.startsWith('http') || href.startsWith('#') || href.startsWith('/')) {
            const titleAttr = linkTitle ? ` title="${linkTitle}"` : '';
            const target = href?.startsWith('http') ? ' target="_blank" rel="noopener"' : '';
            return `<a href="${href}"${titleAttr}${target}>${text}</a>`;
        }

        let cleanHref = href.replace(/\.md$/, '').replace(/\.md#/, '#');
        const baseName = cleanHref.replace(/^(\.\.\/)+/, '').replace(/^\.\//, '');

        if (allSlugs.has(baseName)) {
            cleanHref = `/docs/${baseName}`;
        } else if (allSlugs.has(baseName.toUpperCase())) {
            cleanHref = `/docs/${baseName.toUpperCase()}`;
        } else {
            cleanHref = `https://github.com/InterCooperative-Network/icn/blob/main/${baseName.includes('/') ? 'docs/' : ''}${baseName}`;
        }

        const anchor = href.includes('#') ? `#${href.split('#')[1]}` : '';
        const titleAttr = linkTitle ? ` title="${linkTitle}"` : '';
        return `<a href="${cleanHref}${anchor}"${titleAttr}>${text}</a>`;
    };

    return marked(content, { gfm: true, breaks: false, renderer }) as string;
}
