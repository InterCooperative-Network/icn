---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-13
Last Updated: 2026-05-13
Purpose: Brief for VE-001 — How ICN Works / Closure Loop. The canonical public explainer for ICN's nine-station institutional closure.
---

# VE-001 — How ICN Works / Closure Loop

## Truth label

**`repo-grounded public explainer`** for the closure-loop model. A rendering of the live [`ClosureLoop.astro`](../../../../website/src/components/ClosureLoop.astro) component is `implemented / current UI`. Per-surface depictions inside the loop (e.g. action-card runtime) inherit their own labels.

## Source anchors

- [`docs/design-language/concept-map.md`](../../../design-language/concept-map.md) — canonical → public terminology for all nine stations
- [`docs/design-language/brief-v0.md`](../../../design-language/brief-v0.md) — design language brief, §6 visual primitives
- [`website/src/components/ClosureLoop.astro`](../../../../website/src/components/ClosureLoop.astro) — canonical Astro implementation
- [`website/src/data/icons.ts`](../../../../website/src/data/icons.ts) — symbol family for the nine stations
- [`docs/architecture/THE_COMMONS.md`](../../../architecture/THE_COMMONS.md) — substrate doctrine
- [`docs/reference/project-index/current-truth-map.md`](../../../reference/project-index/current-truth-map.md) — what is real now
- [`docs/reference/project-index/show-readiness-map.md`](../../../reference/project-index/show-readiness-map.md) — what may be shown

## Audience

Public website visitor. First-conversation reader. Organizer hearing about ICN for the first time. Grant reviewer who needs the institutional thesis in one screen.

## Core question

> *"What is ICN actually doing?"*

## One-sentence message

ICN turns the work of an institution — identity, standing, authority, governance, policy, accounting, execution, receipts/provenance, and the member experience — into one closed loop, and every round closes back into the next.

## Must show

- All nine stations, in order, named with their public labels from the concept map.
- Each station's one-line gloss from the concept map (when room allows).
- The icon from the canonical symbol family for each station.
- The **closure** — a visible mark that the loop returns to standing for the next round. The current Astro component uses a dashed "the loop closes" card.
- The convergence at station 09 (member experience) — where the substrate becomes a thing a person can see, use, and live in.
- Loop ordering: `01 identity → 02 standing → 03 authority → 04 governance → 05 policy → 06 accounting → 07 execution → 08 receipts/provenance → 09 member experience → return to 02 standing`.

## Must avoid

- Any "transaction" or "payment" framing for station 06 (accounting). It is *governed social accounting*, not commercial finance.
- Any token / wallet / coin imagery at any station.
- Depicting the loop as a hierarchy or a ladder; it is a closure, not a stack.
- Implying that the loop is fully automated; the loop describes the *system grammar*, not a single shipped runtime.
- Auto-rotating / auto-animating renderings; reduced-motion users must see the same loop static.
- Centralizing the federation imagery — no single sovereign center, even visually.
- Any specific institution-package vocabulary (NYCN, Summit) on the generic surface.

## Canonical copy

These are the exact strings that may appear on the asset. They come from [`ClosureLoop.astro`](../../../../website/src/components/ClosureLoop.astro) and [`concept-map.md`](../../../design-language/concept-map.md). Do not paraphrase.

| # | Label | Note |
|---|---|---|
| 01 | Identity | Cryptographic identity held by the member — not a platform account. |
| 02 | Standing | Provable participation inside a scope. The institution can verify it directly. |
| 03 | Authority | Derived from scope rules and the decisions members have made under them. |
| 04 | Governance | Proposals, deliberation, and decisions the institution will honor. |
| 05 | Policy | Shared rules produced by decisions, shaping what can happen next. |
| 06 | Accounting | Obligations, treasury, patronage, mutual-credit positions — governed social accounting. |
| 07 | Execution | Where accepted decisions translate into real operational effect. |
| 08 | Receipts & Provenance | The chain from authority to outcome — durable, auditable institutional memory. |
| 09 | Member Experience | Where all of the above becomes something a person can see, use, and live in. |

Closing copy:

> **The loop closes.** Every round of standing starts the next. Identity does not begin at 01 again — it is reinforced by the provenance that station 08 produces and the member experience at 09.

## Visual grammar

- **Extends** the canonical `ClosureLoop.astro`. Any new variant — homepage hero, doc front-matter figure, deck slide, social card — derives from that component, not a parallel implementation.
- Mobile-first vertical flow with `↓` connectors between stations; desktop two-column stagger with the last station spanning both columns (already implemented).
- Each station: number pill, icon, label, one-line note (`full` variant) or label only (`compact` variant).
- Token-grounded: every color, spacing value, and border-radius from `website/src/styles/global.css`.
- Station 09 (member experience) styled as convergence: amber accent gradient against the otherwise teal-accented stations.

## Accessibility requirements

- `aria-label` on the figure describing the model, not the chrome: *"ICN's nine-station institutional closure loop"*.
- Each station readable as text alone — icon + number + label + note.
- Icons `aria-hidden="true"` because they sit next to visible text labels.
- Grayscale-safe: verified that station 09's convergence treatment remains distinguishable without color.
- WCAG 2.2 AA contrast on every label and note, both themes.
- Reduced-motion: no auto-animation. Hover state is the only transition, and it does not gate meaning.
- Renders usefully at narrow viewport (≤ 360 px wide).

## Review checklist

Run [VISUAL_REVIEW_CHECKLIST.md](../VISUAL_REVIEW_CHECKLIST.md) end-to-end before any new variant of this asset ships. Per-brief notes:

- §1 source grounding — verify the nine stations against `concept-map.md` and the live component.
- §3 vocabulary — station 06 is the vocabulary trap; check the gloss for `payment` / `balance` / `currency` drift.
- §5 substrate honesty — the loop *as a model* is honest; specific capabilities embedded in a variant (action-card runtime, federation, etc.) carry their own labels per bible §3.
- §8 visual grammar — verify the new variant extends `ClosureLoop.astro` rather than re-implementing it.

## Status

`gate-open`. Brief gate (bible §10) satisfied. Source-asset variants beyond the existing `ClosureLoop.astro` may be built when needed.
