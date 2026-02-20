# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working in this repository.

## What This Repo Is

The public website for the InterCooperative Network at `intercooperative.network`. Built with Astro 5 (static output) + TypeScript. Deployed to GitHub Pages via `.github/workflows/deploy.yml`.

**This repo is content and presentation only.** The ICN substrate documentation lives in `icn/docs/` — it is synced into this repo at build time and must never be edited here.

## Commands

```bash
npm install               # Install dependencies
npm run dev               # Sync docs + start dev server at http://localhost:4321
npm run build             # Production build (runs doc sync automatically via prebuild)
npm run preview           # Preview the production build
npm run sync              # Run doc sync only (no dev server)
npm run lint              # Astro TypeScript check
npm run format            # Prettier formatting (all files)
npm run deploy            # Build + deploy to GitHub Pages via gh-pages
```

`npm run dev` automatically runs `sync-from-icn.sh` before starting — docs are always fresh in dev.

## Content Sync — CRITICAL

`src/content/docs/` and `src/data/stats.json` are **gitignored** — they are regenerated on every build.

The sync script pulls from the ICN repo at `../icn` (sibling directory on the dev VM):
```bash
bash scripts/sync-from-icn.sh
# Or override the ICN repo path:
ICN_REPO=/path/to/icn bash scripts/sync-from-icn.sh
```

**DO NOT edit any file in `src/content/docs/`** — your edits will be silently overwritten by the next `npm run dev` or `npm run build`. If you need to change documentation, edit the source in `/home/ubuntu/projects/icn/docs/`, then run sync.

## Site Structure

```
src/
├── pages/              # File-based routing — each .astro = one URL
│   ├── index.astro         → /
│   ├── about.astro         → /about
│   ├── architecture.astro  → /architecture
│   ├── community.astro     → /community
│   ├── roadmap.astro       → /roadmap
│   ├── blog/               → /blog/*
│   ├── docs/               → /docs/*
│   └── rss.xml.ts          → /rss.xml
├── components/         # Reusable Astro components
│   ├── NetworkGraph.astro  # D3.js interactive network visualization
│   ├── Search.astro        # Fuse.js client-side search
│   └── ThemeToggle.astro   # Dark/light theme switcher
├── content/            # Astro content collections
│   ├── config.ts           # Collection schemas (Zod)
│   ├── blog/               # Blog posts (git-tracked)
│   └── docs/               # ICN docs (gitignored — synced)
├── data/               # Static data
│   └── stats.json          # Generated at build (gitignored)
├── layouts/            # Page layout wrappers
├── lib/                # Utilities
│   └── markdown.ts         # Markdown rendering with link rewriting
└── styles/
    └── global.css          # Design system — all CSS custom properties
```

## Design System

All styling uses CSS custom properties from `src/styles/global.css`. Never hardcode colors or use arbitrary values.

**Colors:**
```css
--bg-primary: #06090f          /* Page background */
--bg-secondary: #0c1118        /* Secondary surfaces */
--bg-card: #111827             /* Card backgrounds */
--text-primary: #e8edf5        /* Body text */
--text-secondary: #94a3b8      /* Secondary text */
--text-heading: #f1f5f9        /* Heading text */
--accent-teal: #2dd4bf         /* Primary accent */
--accent-blue: #38bdf8         /* Secondary accent */
--accent-purple: #a78bfa       /* Tertiary accent */
--border-subtle: rgba(148,163,184,0.08)
--border-default: rgba(148,163,184,0.12)
```

**Typography:**
```css
--font-sans: 'Inter', system-ui, sans-serif    /* Body text */
--font-heading: 'Outfit', var(--font-sans)     /* Headings */
--font-mono: 'JetBrains Mono', monospace       /* Code */
```

**Spacing scale:** `--space-xs` through `--space-4xl` (0.25rem to 6rem)

**Transitions:** `--ease-out`, `--duration-fast` (150ms), `--duration-normal` (250ms), `--duration-slow` (400ms)

The site supports both dark (default) and light themes via `[data-theme="light"]` overrides in `global.css`. The `ThemeToggle` component switches between them. Do not use `prefers-color-scheme` media queries — always use `[data-theme]`.

## Astro Config

- Output: `static` (all pages pre-rendered)
- Site: `https://intercooperative.network`
- Integrations: `@astrojs/sitemap`
- Code highlighting: Shiki `github-dark` theme

## Deployment

GitHub Actions workflow at `.github/workflows/deploy.yml`:
- Clones the ICN repo, runs sync, builds Astro, deploys to GitHub Pages
- Triggers on push to `main` or manual dispatch

For manual deploy: `npm run deploy` (builds and pushes to `gh-pages` branch).
