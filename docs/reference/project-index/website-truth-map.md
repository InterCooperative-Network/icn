---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# Website Truth Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), ADR-0032, and [`source-of-truth-map.md`](source-of-truth-map.md). This map routes public website truth discipline; it does not make the website canonical over the repo.

## Purpose

The public website is the front door to ICN. It must be clear, readable, and honest. It should summarize current truth without becoming a second source of truth.

This map exists to prevent public copy from drifting ahead of implementation or lagging behind current runtime evidence.

## Core rule

```text
The website summarizes current truth.
It does not create current truth.
```

When the website and repo disagree, defer to the source-of-truth hierarchy, then fix the website.

## Source paths

| Area | Path |
|---|---|
| Website source | `website/` |
| Main public pages | `website/src/pages/` |
| Website docs (rendered from repo-root) | repo-root `docs/` via `website/src/pages/docs/` (see `website/src/lib/paths.ts`) |
| Website truth boundary ADR | `docs/adr/ADR-0032-website-truth-boundary.md` |
| Show-readiness rules | `docs/reference/project-index/show-readiness-map.md` |
| Current truth | `docs/STATE.md`, `docs/PHASE_PROGRESS.md`, `docs/reference/project-index/current-truth-map.md` |

## Important website surfaces

| Surface | Role | Drift risk |
|---|---|---|
| `index.astro` | Public landing page. | medium: can over-simplify into product pitch. |
| `whats-real-now.astro` | Public maturity anchor. | high: freshness anchor can lag current state. |
| `for-cooperatives.astro` | Adoption framing. | high: can imply readiness or formal pilots. |
| `for-developers.astro` | Contributor/developer framing. | medium: can point to stale commands or surfaces. |
| `get-involved.astro` | Participation path. | medium: can overpromise. |
| `src/content/docs/` | Website docs mirror/content. | high: can become stale duplicate documentation. |

## Public claim checklist

Before publishing or updating a public page, check:

- Does it preserve Phase 2 as in progress, not complete?
- Does it avoid describing NYCN as a formal pilot?
- Does it avoid claiming live federation?
- Does it avoid production-readiness claims?
- Does it distinguish implemented, partial, fixture-backed, design-direction, and future work?
- Does it avoid implying service hosting or Tool Commons are implemented runtime ecosystems?
- Does it avoid broad security/autonomy claims without current evidence?
- Does it keep public ICN copy free of private/package-local NYCN specifics unless intentionally reviewed?
- Does it use current ICN vocabulary and avoid deprecated product framing?

## Current safe website framing

Safe short framing:

```text
ICN is institutional infrastructure for democratic organizations: cooperatives, communities, and federations.
It is a constraint engine: apps translate institutional meaning into constraints, and the kernel enforces constraints without understanding that meaning.
```

Safe current status framing:

```text
Phase 0 and Phase 1 are complete. Phase 2 is in progress and not complete.
NYCN is the intended first cooperative partner and active partnership track, not yet a formal pilot.
Member standing and action cards exist. The pilot UI has a bounded demo slice for standing and action cards.
```

## What the website must not imply

The website must not imply:

- ICN is a finished product;
- ICN is production deployed as a multi-cooperative network;
- NYCN is a signed/formal pilot;
- federation is live in production;
- Tool Commons is a live app ecosystem;
- service hosting is implemented as a complete runtime feature;
- the mobile/member shell is complete;
- all privacy, ZKP, steward, or post-quantum paths are production-ready;
- old demo/product framing is the current ICN identity.

## Docs mirror rule

Website docs mirrors should be routing surfaces, not duplicate root READMEs.

If a website docs file contains copied status numbers, old crate counts, old app topology, or stale commands, prefer replacing it with a short truth-boundary note that points to canonical docs.

## Review cadence

Review `whats-real-now` whenever any of these change:

- Phase status;
- member standing/action-card surface status;
- demo fixture status;
- NYCN pilot status;
- federation status;
- service/tool runtime status;
- public-safe vocabulary rules.

Only bump the reviewed date after checking claims against current anchors.

## Related issues and PRs

| Item | Purpose |
|---|---|
| #1779 | Tracks public website and pilot-ui truth sync. |
| ADR-0032 | Defines website truth-boundary doctrine. |
| `show-readiness-map.md` | Defines external non-claims and demo-safe framing. |
| `stale-and-archived-map.md` | Defines stale public-surface handling. |
