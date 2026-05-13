---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-13
Last Updated: 2026-05-13
Purpose: Brief for VE-002 — Where You Are Acting / Scope Model. The canonical public explainer for ICN's five co-equal scopes of institutional action.
---

# VE-002 — Where You Are Acting / Scope Model

## Truth label

**`repo-grounded public explainer`** for the scope model. A rendering of the live [`ScopeModel.astro`](../../../../website/src/components/ScopeModel.astro) component is `implemented / current UI`. Concrete cross-scope flows (e.g. a live federation between two real cooperatives) carry their own labels — typically `future-state / roadmap`.

## Source anchors

- [`docs/design-language/concept-map.md`](../../../design-language/concept-map.md) — Scope concepts section
- [`website/src/components/ScopeModel.astro`](../../../../website/src/components/ScopeModel.astro) — canonical Astro implementation
- [`docs/architecture/CELLS_AND_SCOPES.md`](../../../architecture/CELLS_AND_SCOPES.md) — cell-based federation model
- [`docs/architecture/SCOPE_BOUNDED_TRUST.md`](../../../architecture/SCOPE_BOUNDED_TRUST.md) — trust scope architecture
- [`docs/architecture/THE_COMMONS.md`](../../../architecture/THE_COMMONS.md) — substrate doctrine
- [`docs/reference/project-index/show-readiness-map.md`](../../../reference/project-index/show-readiness-map.md) — federation honesty constraints

## Audience

Public website visitor. Member trying to orient themselves. Organizer learning what "scope" means in ICN. Grant reviewer comparing ICN to platform models.

## Core question

> *"Where am I acting when I act in ICN?"*

## One-sentence message

Scope is context of action, not hierarchy — every ICN action belongs to one or more of five co-equal scopes at once: commons, federation, community, cooperative, self.

## Must show

- The five scopes: **commons, federation, community, cooperative, self**.
- A vertical rail with five nodes (the current implementation) — visually communicating "you are always inside all of these at once."
- Each scope's one-line gloss from the concept map.
- A footer line explaining that a single decision can touch several scopes at once and the receipt it produces is tagged with every scope it passed through.
- The icon from the canonical symbol family for each scope (per `ScopeModel.astro`).
- Reading order top-to-bottom: widest shared context (commons) to most personal (self). Documented in the figcaption as **not** a hierarchy.

## Must avoid

- Any depiction of the scopes as a ladder, pyramid, or rank.
- Any implication that the federation scope is a sovereign center.
- Crypto / DAO / token framing for "commons."
- "Account" or "wallet" framing for "self."
- NYCN, Summit, or any specific federation depicted as the canonical example.
- Treating "community" and "cooperative" as the same thing — they are co-equal but distinct.

## Canonical copy

From [`ScopeModel.astro`](../../../../website/src/components/ScopeModel.astro). Do not paraphrase.

| # | Scope | Gloss |
|---|---|---|
| — | Commons | Shared resources governed by the institutions that use them. |
| — | Federation | How distinct institutions coordinate without merging. |
| — | Community | A group of members with shared rules, purpose, and standing. |
| — | Cooperative | A scoped institution owned and governed by its members. |
| — | Self | The individual member — the identity that holds standing in each scope above. |

Intro:

> Scope is **context of action**, not hierarchy. Every ICN action belongs to one or more scopes at once.

Footer:

> Read top to bottom: widest shared context down to the most personal. A single decision can touch several scopes at once — that is the point. The receipt it produces is tagged with every scope it passed through.

## Visual grammar

- **Extends** the canonical `ScopeModel.astro`. Any new variant — homepage figure, doc explainer, deck slide — derives from that component.
- Single vertical rail (teal accent) with five nodes where the rail touches each scope card.
- Each card: framed icon, label, one-line gloss.
- Mobile-first; the rail layout is unchanged across viewports.
- Token-grounded.

## Accessibility requirements

- `aria-label` on the figure: *"ICN's five scopes of institutional action, from the most shared context to the most personal."*
- The rail and nodes are decorative; `aria-hidden="true"`.
- Each scope readable as label + gloss alone.
- Icons paired with text labels and `aria-hidden="true"`.
- Grayscale-safe.
- WCAG 2.2 AA contrast on labels and glosses, both themes.
- No motion required; hover transition does not gate meaning.

## Review checklist

Run [VISUAL_REVIEW_CHECKLIST.md](../VISUAL_REVIEW_CHECKLIST.md). Per-brief notes:

- §5 substrate honesty — a depiction of cross-scope federation in production is `future-state / roadmap` until live multi-coop federation exists; the model itself is `repo-grounded public explainer`.
- §7 scope / package boundary — verify no NYCN / Summit copy in any variant.
- §8 visual grammar — verify the new variant extends `ScopeModel.astro` and reuses the rail metaphor.

## Status

`briefed`. Brief gate satisfied. Source-asset variants beyond the existing `ScopeModel.astro` may be built when needed.
