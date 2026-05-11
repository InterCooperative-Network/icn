---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# Stale and Archived Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), and [`source-of-truth-map.md`](source-of-truth-map.md). This map identifies drift-prone document classes and how to handle them. It does not make archived material current.

## Purpose

ICN has a large history of plans, architecture sketches, demo tracks, website mirrors, old README material, and implementation notes. Some of that material is valuable archaeology. Some of it is dangerous if treated as current truth.

This map exists to prevent old confidence from outranking current evidence.

## Core rule

```text
Historical detail is not current truth.
Current truth comes from code/tests, STATE.md, PHASE_PROGRESS.md, accepted ADR/RFCs, current contracts, and recent merged evidence.
```

## Drift classes

| Drift class | Examples | Risk | Action |
|---|---|---|---|
| Old implementation counts | crate counts, LOC, app topology, old status numbers | medium | replace with current state docs or generated stats |
| Old product framing | outdated demo/product descriptions | high | update or mark historical |
| Unsafe economic vocabulary | older public/demo text using deprecated framing | high | replace in public surfaces; allow only in explicit historical/avoid-list contexts |
| Old demo universe | legacy scripts or old demo names presented as main demo | high | clarify actual pilot-ui guided demo path |
| NYCN overclaim | NYCN described as formal pilot or live federation member | high | rewrite as intended first partner / active track |
| Live federation overclaim | peer networking or primitives described as production federation | high | distinguish transport, gossip, federation |
| Service/tool overclaim | service hosting or Tool Commons described as implemented runtime | high | classify as design-direction unless evidence exists |
| Security/autonomy overclaim | PQ/ZKP/steward/privacy primitives described as production posture | high | require specific source/runtime evidence |
| Root/docs mirror duplication | website or mirror docs copying stale README material | medium-high | convert mirrors to routing/truth-boundary pages |
| Archived planning docs | historical architecture maps, old phase plans, session docs | medium | annotate as historical; mine for context only |

## Known high-risk surfaces

| Surface | Risk | Current handling |
|---|---|---|
| `website/src/content/docs/README.md` | can become stale second README | should route to canonical docs, not duplicate them |
| `web/pilot-ui/README.md` | old demo/product framing can persist | should describe current organizer/member demo only |
| `website/src/pages/whats-real-now.astro` | public freshness anchor can lag runtime progress | must be checked against STATE/PHASE docs before date bump |
| historical architecture maps | detailed but sometimes old | use as archaeology only |
| old demo docs/scripts | can imply wrong demo path | distinguish legacy dev scaffolding from guided pilot UI demo |
| archived session/handoff docs | useful but time-bound | cite only with date/status context |

## Required labels for stale material

When a doc is retained but not current, use explicit language such as:

- historical;
- archived;
- superseded;
- design-direction;
- package-local;
- fixture-only;
- not current runtime truth;
- useful archaeology, not source of truth.

## Public-surface handling

Public surfaces should not make readers perform archaeology.

If a public page contains stale material, prefer one of these actions:

1. replace it with current truth;
2. replace it with a short routing note to canonical truth;
3. move it to an archive if it must be preserved;
4. clearly label it historical.

Do not leave stale public copy in place because it used to be true.

## Non-claims guardrail

Stale material must not be allowed to reintroduce claims that ICN currently has:

- Phase 2 completion;
- formal NYCN pilot status;
- production readiness;
- live federation;
- complete mobile/member UX;
- implemented service hosting;
- live Tool Commons;
- full fixture-backed demo mode;
- production SDIS/ZKP/PQ deployment posture.

## Suggested future checks

A future drift check should flag or classify:

- old repo paths and app topology;
- stale port claims;
- stale demo names;
- old implementation counts;
- public website references to NYCN/Summit unless intentionally allowlisted;
- unsafe vocabulary in public/demo surfaces;
- project-index docs without `Last Reviewed`;
- maps that claim implemented status without source/state anchors.

## Related maps

| Map | Relationship |
|---|---|
| `source-of-truth-map.md` | Defines the ranking this map enforces. |
| `project-coverage-matrix.md` | Tracks where stale/archive risk remains. |
| `website-truth-map.md` | Applies these rules to the public site. |
| `pilot-ui-current-state-map.md` | Applies these rules to the current demo surface. |
| `show-readiness-map.md` | Defines external-facing non-claims. |
