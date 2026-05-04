# ICN Website

The public-facing website for the InterCooperative Network at `intercooperative.network`. Built with Astro 5 (static output) + TypeScript. Lives inside the ICN monorepo at `website/`. Deploys automatically to the `gh-pages` branch on pushes to `main` that touch `website/**` or `docs/**`.

## Design language (read this first)

The website is the first implementation surface of ICN's universal civic design language. Every public-facing edit should be checkable against the canonical design-language docs:

- **[brief-v0](../docs/design-language/brief-v0.md)** — canonical source of truth for the design language (principles, semantic layers, visual primitives, anti-patterns)
- **[concept-map](../docs/design-language/concept-map.md)** — canonical term → public plain-language label → localization notes, for every ICN concept
- **[accessibility](../docs/design-language/accessibility.md)** — WCAG rules, contrast requirements, keyboard and screen-reader expectations, and the review checklist every PR must pass

If an edit introduces something these docs do not describe, either the docs evolve or the edit is out of scope for the design language. Do not let ad-hoc decisions silently drift the system.

The brief is explicit about what the system is and is not: institutional, civic, calm, legible, federated, auditable, serious, accessible by design. Not crypto, not startup SaaS, not enterprise dashboard sludge, not generic govtech, not hacker-terminal aesthetic, not cyberpunk network maps, not AI vapor, not futuristic spectacle. Edits that drift toward those anti-patterns should be rejected.

## Token surface

All visual tokens live in **[`src/styles/global.css`](src/styles/global.css)** as CSS custom properties (colors, spacing scale, radii, typography, motion durations, shadows). It is the single source for runtime values; this README does not duplicate hex codes, font names, or scale numbers because they belong in one place and were drifting when they were duplicated here.

Two operating rules apply across all `.astro` files:

- **Never hardcode colors, fonts, or scale values.** Reference design tokens (`var(--accent-teal)`, `var(--bg-primary)`, `var(--space-md)`, etc.) in scoped `<style>` blocks.
- **Theme-aware.** The site supports dark (default) and light themes via `[data-theme="light"]`. Tokens already resolve correctly per theme — do not add `prefers-color-scheme` media queries.

See `.claude/rules/astro-conventions.md` for the full conventions list.

## Quick start

Prerequisites: Node.js 18+ and npm.

```bash
# From the icn monorepo root
cd website
npm install
npm run dev
```

The dev server runs at `http://localhost:4321`.

## Site structure

```
website/
├── public/                 # Static assets served at site root
├── src/
│   ├── pages/              # File-based routing — every page is one .astro file
│   ├── components/         # Reusable Astro components
│   ├── content/
│   │   ├── config.ts       # Astro content collection schemas (blog only)
│   │   └── blog/           # Blog posts, git-tracked
│   ├── layouts/            # Page layout wrappers
│   ├── lib/                # Build-time helpers (markdown rendering, link rewriting)
│   ├── styles/global.css   # Design tokens — single source of truth for colors/typography/spacing
│   └── data/               # Build-time data
├── astro.config.mjs        # Astro configuration
└── package.json
```

There is **no Tailwind** in this site. Styling uses scoped `<style>` blocks in `.astro` files, with values pulled from CSS custom properties in `src/styles/global.css`.

## Documentation integration

Documentation lives at the **repo root `docs/` directory**, not inside `website/`. Pages under `/docs/*` read markdown files from `../docs/` at build time via `import.meta.url`-relative path resolution.

**To change documentation, edit files under the repo-root `docs/` directory.** Do not create `src/content/docs/` here — that pattern is obsolete and will not surface on the site.

The files that resolve and render docs:

- `src/pages/docs/[...slug].astro` — dynamic doc page renderer
- `src/pages/docs/index.astro` — docs index/sidebar
- `src/lib/markdown.ts` — markdown renderer with link rewriting

## Scripts

```bash
npm run dev        # Astro dev server at http://localhost:4321
npm run build      # Production build (runs scripts/gen-stats.mjs first)
npm run preview    # Preview the production build locally
npm run lint       # Astro TypeScript / content check
npm run format     # Prettier formatting
npm run deploy     # Build and force-push dist/ to gh-pages (manual fallback)
```

The `prebuild` step (`scripts/gen-stats.mjs`) runs automatically before `build`.

## Adding content

- **Pages** — create an `.astro` file under `src/pages/`. The URL path mirrors the file path.
- **Blog posts** — add a markdown file under `src/content/blog/` matching the schema in `src/content/config.ts`.
- **Documentation** — edit files under the **repo-root `docs/`** directory; the site picks them up at build time.

For lists of more than ~3 items, prefer Astro content collections over hardcoded arrays in `.astro` frontmatter.

## Deployment

Automatic. `.github/workflows/website-deploy.yml` builds and publishes to `gh-pages` on every push to `main` that touches `website/**`, `docs/**`, or the workflow itself. Uses `peaceiris/actions-gh-pages@v4` to force-push the `website/dist/` build output to `gh-pages`. The site is served at `https://intercooperative.network` by GitHub Pages.

Manual fallback: `npm run deploy` from `website/` builds and pushes via the `gh-pages` npm package. Use only when the workflow is broken or for a one-off deploy without a `main` commit.

## Contributing

The website is institutional infrastructure copy, not a marketing surface. Edits should:

- be checkable against the design-language docs above (especially `brief-v0` anti-patterns and `accessibility` gates)
- preserve regulatory-safe vocabulary (obligation, allocation, settlement, unit, position, receipt, provenance, evidence) and avoid framing ICN-native primitives as payment, currency, balance, or wallet surfaces
- preserve plain language and semantic HTML
- include alt text for any image
- be tested at narrow viewports and with the dark/light theme toggle
- not introduce partner-specific or institution-specific content (those belong in partner repositories)

## Links

- **Monorepo (where this site lives):** [InterCooperative-Network/icn](https://github.com/InterCooperative-Network/icn)
- **Documentation root:** [`docs/`](../docs/)
- **Design language:** [`docs/design-language/`](../docs/design-language/)
- **Issues / discussions:** filed against the monorepo at `InterCooperative-Network/icn`

## License

Same terms as the ICN project. See the monorepo root for license details.
