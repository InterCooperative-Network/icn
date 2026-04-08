# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working in this directory.

## What This Is

The public website for the InterCooperative Network at `intercooperative.network`. Built with Astro 5 (static output) + TypeScript. Lives inside the ICN monorepo at `website/`.

**This directory is content and presentation only.** Documentation lives at repo root `docs/` and is read directly at build time via `import.meta.url`-relative paths. No sync scripts, no symlinks.

## Commands

```bash
cd website
npm install               # Install dependencies
npm run dev               # Start dev server at http://localhost:4321
npm run build             # Production build
npm run preview           # Preview the production build
npm run lint              # Astro TypeScript check
npm run format            # Prettier formatting (all files)
npm run deploy            # Build + deploy to GitHub Pages via gh-pages
```

## Docs Integration

Docs pages (`/docs/*`) read markdown directly from `../docs/` (repo root `docs/` directory)
using `import.meta.url`-relative path resolution. This works regardless of working directory.

**DO NOT create `src/content/docs/`** — docs are read from repo root, not copied into website.
To change documentation, edit files in `docs/` at repo root.

Files that resolve the docs path:

- `src/pages/docs/[...slug].astro` — dynamic doc page renderer
- `src/pages/docs/index.astro` — docs index/sidebar
- `src/lib/markdown.ts` — markdown renderer with link rewriting

## Site Structure

```
src/
├── pages/              # File-based routing
├── components/         # NetworkGraph (D3.js), Search (Fuse.js), ThemeToggle
├── content/            # Astro content collections
│   ├── config.ts       # Collection schemas (blog only)
│   └── blog/           # Blog posts (git-tracked)
├── layouts/            # Page layout wrappers
├── lib/markdown.ts     # Markdown rendering with link rewriting
└── styles/global.css   # Design system — all CSS custom properties
```

## Design System

All styling uses CSS custom properties from `src/styles/global.css`. Never hardcode colors.
Dark theme default, light theme via `[data-theme="light"]`. Use `--accent-teal`, `--bg-primary`, etc.
Fonts: Inter (body), Outfit (headings), JetBrains Mono (code).

## Deployment

**Automatic.** `.github/workflows/website-deploy.yml` builds and publishes to the `gh-pages` branch on every push to `main` that touches `website/**`, `docs/**`, or the workflow itself. Uses `peaceiris/actions-gh-pages@v4` to force-push the `website/dist/` build output to `gh-pages`. Site is served at `https://intercooperative.network` by GitHub Pages.

**Manual fallback:** `npm run deploy` from `website/` still works (builds and pushes via the `gh-pages` npm package). Use this only when the workflow is broken or you need to push a one-off deploy without a main commit.

**Triggering without content changes:** `gh workflow run website-deploy.yml` — useful for re-deploying after a docs/stats change that didn't modify `website/`.
