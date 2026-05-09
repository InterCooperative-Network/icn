# ICN Website Docs Mirror

This directory is a website content surface, not the source of project truth.

Do not treat this file as a second root README. Current ICN truth lives in the main repo docs and project-index maps.

## Current truth anchors

Use these files first:

- `docs/STATE.md`
- `docs/PHASE_PROGRESS.md`
- `docs/ARCHITECTURE.md`
- `docs/INDEX.md`
- `docs/reference/project-index/source-of-truth-map.md`
- `docs/reference/project-index/current-truth-map.md`
- `docs/reference/project-index/runtime-surface-map.md`
- `docs/reference/project-index/show-readiness-map.md`
- `docs/reference/project-index/source-tree-map.md`
- `docs/reference/project-index/rust-workspace-map.md`

## Public website rule

The public website should summarize current truth. It should not create current truth.

If the website and repo docs disagree, defer to the source-of-truth hierarchy in `docs/reference/project-index/source-of-truth-map.md`, then update or remove the stale website copy.

## Current short framing

ICN is institutional infrastructure for democratic organizations: cooperatives, communities, and federations. It is a constraint engine. Apps translate institutional meaning into constraints; the kernel enforces constraints without understanding that meaning.

Current phase framing:

- Phase 0 is complete.
- Phase 1 is complete.
- Phase 2 is in progress and not complete.
- NYCN is the intended first cooperative partner and active partnership track, not yet a formal pilot.
- Member standing and action cards exist as member-facing read models.
- The pilot UI has a bounded fixture-backed demo slice for standing and action cards.
- Production readiness, live federation, formal NYCN pilot status, implemented service hosting, and complete mobile/member UX must not be claimed.

## Safe vocabulary

Prefer ICN's current institutional vocabulary:

- settlement
- unit
- position
- obligation
- allocation
- receipt
- provenance
- evidence
- identity
- standing
- action card

When old docs or older examples use deprecated product framing, update the public surface or clearly mark the material as historical.

## Where to edit public copy

Important website entry points include:

- `website/src/pages/index.astro`
- `website/src/pages/whats-real-now.astro`
- `website/src/pages/for-cooperatives.astro`
- `website/src/pages/for-developers.astro`
- `website/src/pages/get-involved.astro`

Before publishing website changes, check the current anchors above and preserve the non-claim boundaries.
