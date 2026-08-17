# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working in this directory.

## What This Is

The public website for the InterCooperative Network at `intercooperative.network`. Built with Astro 5 (static output) + TypeScript. Lives inside the ICN monorepo at `website/`.

**This directory is content and presentation only.** Documentation lives at repo root `docs/` and is read directly at build time via `import.meta.url`-relative paths. No sync scripts, no symlinks.

## Commands

```bash
cd website
npm ci                    # Install dependencies (respects the lockfile)
npm run generate          # Regenerate the projections of canonical repo state
npm run dev               # Start dev server at http://localhost:4321 (runs generate first)
npm run build             # Production build (runs generate first)
npm run preview           # Preview the production build
npm run lint              # Astro TypeScript check
npm run format            # Prettier formatting (all files)
npm run deploy            # Build + deploy to GitHub Pages via gh-pages
```

**No CI check builds or type-checks `website/` on a pull request.** The deploy
workflow runs on push to `main` only, so a broken build fails *after* merge.
`npm run build && npm run lint` locally is the entire safety net — run both.

## Generated projections of canonical state

The website renders *projections* of repository truth. It never keeps its own
copy of a claim. Five generators run before every build and dev server
(`npm run generate`), writing gitignored files into `src/data/*.generated.json`:

| Generator | Reads | Produces |
|---|---|---|
| `gen-concepts.mjs` | `docs/design-language/concept-map.md` | Public plain-language labels for every ICN concept |
| `gen-project-state.mjs` | `docs/status.toml` | Per-subsystem maturity band + evidence class |
| `gen-docs-classification.mjs` | `docs/registry.toml` | Which docs publish, and in which of the four layers |
| `gen-build-log.mjs` | git history | A small set of meaningful, reviewed state changes |
| `gen-stats.mjs` | the repo | The few counts whose source and freshness are defensible |

**Every one of these fails the build rather than emitting partial data.** A
public surface that silently renders an empty or mis-mapped projection is worse
than a red build. If a generator starts failing, fix the source it reads or the
mapping — do not add a fallback.

**Never edit `src/data/*.generated.json`, and never hardcode a project-state
claim in an `.astro` file.** Edit the source named in the generated file's
`source` field. See
[docs/reference/project-index/public-state-projection.md](../docs/reference/project-index/public-state-projection.md)
for the vocabulary mappings and why each exists.

## Docs Integration

Docs pages (`/docs/*`) read markdown directly from `../docs/` (repo root `docs/` directory)
using `import.meta.url`-relative path resolution. This works regardless of working directory.

**DO NOT create `src/content/docs/`** — docs are read from repo root, not copied into website.
To change documentation, edit files in `docs/` at repo root.

**Not every doc under `docs/` is published.** `gen-docs-classification.mjs`
derives the publish decision and the public layer (Learn / Current reference /
Decisions / Archive) from `docs/registry.toml`. Material whose registry role is
`internal` or `development_session`, and partner-specific directories, are
withheld from the site while remaining in the repository. Archived pages render
with an interrupting banner and `noindex`.

Files that resolve the docs path:

- `src/pages/docs/[...slug].astro` — dynamic doc page renderer (applies the publish filter)
- `src/pages/docs/index.astro` — docs landing, four layers
- `src/pages/docs/archive.astro` — the historical index, with its own search
- `src/lib/docsClassification.ts` / `src/lib/docsTree.ts` — classification and navigation
- `src/lib/markdown.ts` — markdown renderer with link rewriting

## Site Structure

```
src/
├── pages/              # File-based routing
├── components/         # Search (Fuse.js), ThemeToggle, and the ICN visual primitives
├── content/            # Astro content collections
│   ├── config.ts       # Collection schemas (blog only)
│   └── blog/           # Blog posts (git-tracked)
├── data/               # Structured data + gitignored *.generated.json projections
├── layouts/            # Page layout wrappers
├── lib/                # Typed accessors for the generated projections, markdown, paths
├── scripts/            # The generators (see above)
└── styles/global.css   # Design system — tokens and shared primitives
```

Visual primitives, in rough order of how introductory they are:
`FragmentationFigure` (today vs ICN) · `PublicLoop` (six plain-language stages)
· `ClosureLoop` (the canonical nine stations) · `ScopeModel` ·
`ProvenanceTrail` · `MemberSurface`. `Term` renders an ICN concept
plain-language-first; `MaturityBadge` and `EvidenceBadge` are the two claim
axes and always travel together; `DemoLabel` marks fixture-backed surfaces.

## Design System

All styling uses CSS custom properties from `src/styles/global.css`. Never hardcode colors.
Dark theme default, light theme via `[data-theme="light"]`. Use `--accent-teal`, `--bg-primary`, etc.
Fonts: Inter (body), Outfit (headings), JetBrains Mono (code).

Shared primitives live at the end of `global.css`: type scale (`--text-*`),
measure and page-width tokens, `.lede` / `.sublede` / `.pull` / `.figure-note`,
`.callout`, `.data-table`, `.disclosure`, `.chip-row`, `.scroll-x`, `.sr-only`,
and `--target-min` (the 44px interactive floor). **Check there before adding a
style to a page's scoped `<style>`** — page-scoped styles are for genuinely
page-specific composition, not another spelling of "a card".

Binding constraints, in force order:
[MUST_NOT_SHIP.md](../docs/design/MUST_NOT_SHIP.md) (twelve hard rejections) →
[.claude/rules/design.md](../.claude/rules/design.md) →
[ADR-0032](../docs/adr/ADR-0032-website-truth-boundary.md) (claim discipline) →
[PUBLIC_SITE_IA.md](../docs/design/PUBLIC_SITE_IA.md) (page jobs, plain-language
convention).

Two things that are gone on purpose and should not come back: gradient/shimmer
text (MUST_NOT_SHIP item 8) and any diagram shaped as a globe or a hub with
spokes (item 9).

## Astro gotcha

A multi-line union type with leading pipes in `.astro` frontmatter fails to
parse (`Unexpected "|"`, reported at an unrelated line). Write exported unions
on one line:

```ts
export type EvidenceId = 'test-backed' | 'ci-backed' | 'fixture-backed';
```

## Deployment

**Automatic.** `.github/workflows/website-deploy.yml` builds and publishes to the `gh-pages` branch on every push to `main` that touches `website/**`, `docs/**`, or the workflow itself. Uses `peaceiris/actions-gh-pages@v4` to force-push the `website/dist/` build output to `gh-pages`. Site is served at `https://intercooperative.network` by GitHub Pages.

**Manual fallback:** `npm run deploy` from `website/` still works (builds and pushes via the `gh-pages` npm package). Use this only when the workflow is broken or you need to push a one-off deploy without a main commit.

**Triggering without content changes:** `gh workflow run website-deploy.yml` — useful for re-deploying after a docs/stats change that didn't modify `website/`.
