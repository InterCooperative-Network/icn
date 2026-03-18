---
description: Astro and website conventions for icn-website. Applied when editing site source files.
globs: ["src/**/*.astro", "src/**/*.ts", "src/**/*.css"]
---

## Routing

**File-based routing only.** Every page is an `.astro` file in `src/pages/`. No programmatic route generation. The URL path mirrors the file path exactly.

## Content

**Use Astro content collections for structured data.** Any list of more than ~3 items (blog posts, team members, features) goes in `src/content/` with a Zod schema defined in `src/content/config.ts`. Do not hardcode arrays in `.astro` frontmatter.

**Documentation lives at repo root `docs/`.** The website reads docs directly from `../docs/` via `path.resolve(process.cwd(), '..', 'docs')`. Do not create `src/content/docs/` — that pattern is obsolete. To update documentation, edit files in the repo root `docs/` directory.

## Styling

**No inline styles.** Never use `style="..."` attributes. Use CSS custom properties from `src/styles/global.css` in scoped `<style>` blocks within `.astro` files.

**Always use design tokens.** Reference `--bg-primary`, `--accent-teal`, `--text-primary`, etc. rather than hardcoded hex values. Check `src/styles/global.css` for the full list.

**Responsive-first.** Write mobile styles as the default. Use `@media (min-width: ...)` for larger viewports — not `max-width` overrides.

**Theme-aware.** The site supports dark (default) and light themes via `[data-theme="light"]`. Use CSS custom properties — they automatically pick up the correct theme value. Never add `prefers-color-scheme` media queries.

## JavaScript

**TypeScript for all scripts.** No `.js` files. No `any` types.

**No `onclick` attributes.** Event handlers go in `<script>` blocks at the bottom of `.astro` files, or in `src/scripts/` as ES modules. Use `document.querySelector` with proper null checks.

## Components

**Typed props with `interface Props {}`** in the component's frontmatter. All props must be typed — no implicit `any`.

## Images

**Use `src/assets/` for optimized images.** Astro's image pipeline optimizes and hashes images in `src/assets/`. Raw `<img src="/public/...">` tags skip optimization — avoid them for content images.

## Packages

**Check before installing.** D3.js (`d3`), Fuse.js (`fuse.js`), and `marked` are already available. Do not add new npm packages for functionality that existing dependencies already provide.
