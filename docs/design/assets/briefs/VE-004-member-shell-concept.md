---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-13
Last Updated: 2026-05-13
Purpose: Brief for VE-004 — Member Shell Concept. The illustrative-direction visual for the unified member-facing surface ICN is designed to produce.
---

# VE-004 — Member Shell Concept

## Truth label

**`illustrative direction`**. The capabilities beneath each panel (`/me/standing`, `/me/action-cards`, action-item completion-receipt retrieval, governance domains) are real. The unified product surface tying them together is **not yet shipped**. Every variant of this asset declares "illustrative" visibly.

This brief is also a guardrail: any depiction of the member shell that softens this label, hides it, or implies the shell is a shipped product violates the bible (§3, §8) and the show-readiness map.

## Source anchors

- [`website/src/components/MemberSurface.astro`](../../../../website/src/components/MemberSurface.astro) — canonical illustrative implementation
- [`docs/architecture/MEMBER_STANDING.md`](../../../architecture/MEMBER_STANDING.md) — member-standing contract
- [`docs/reference/project-index/runtime-surface-map.md`](../../../reference/project-index/runtime-surface-map.md) — actual gateway surfaces beneath the shell
- [`docs/reference/project-index/show-readiness-map.md`](../../../reference/project-index/show-readiness-map.md) — *"complete mobile / member app"* is on the do-not-show list
- [`docs/mobile/icn-mobile-ux-spec-v1.md`](../../../mobile/icn-mobile-ux-spec-v1.md) — mobile member UX spec (build-facing, not shipped)
- [`docs/design/CONTENT_STYLE_GUIDE.md`](../../CONTENT_STYLE_GUIDE.md) — vocabulary rules

## Audience

Public website visitor wanting to picture what ICN feels like. Member or organizer who can read "this is the direction" honestly. Grant reviewer who needs to see the substrate's intended surface without being misled into thinking it ships today.

## Core question

> *"What would it feel like to inhabit ICN as a member?"*

## One-sentence message

A member sees one coherent surface — who they are, where they hold standing, their position in governed social accounting, the decisions waiting on them, and the receipts that prove the chain — and it is grounded in real gateway capabilities, but the unified surface itself is not yet shipped.

## Must show

- Five panels on a "civic screen frame" (not a phone bezel):
  1. **Acting as** — self + the scopes the member is currently inside (fictional names).
  2. **Standing** — recognized member status inside a fictional cooperative, with chips for "recognized since" and role.
  3. **Position** — governed social accounting: contributions recorded, allocations drawn, open obligations, patronage status. **Never** a balance.
  4. **Open decision** — a pending proposal with vote pending, close window, scope.
  5. **Recent receipts** — three receipts with id, plain description, stamped date, scope label.
- A visible "MEMBER SURFACE · CONCEPT" eyebrow on the device frame.
- A dashed "what we are building toward" footer making the illustrative-direction label explicit.
- All icons from the canonical symbol family (identity, authority, accounting, governance, provenance).

## Must avoid

- The words `wallet`, `balance`, `currency`, `coin`, `payment`, `transaction` anywhere on the surface.
- A currency symbol next to a number.
- Any "send" or "receive" affordance.
- Any "stake / mint / burn / swap."
- A phone bezel or device-mockup chrome implying app-store readiness.
- Real cooperative names, real member names, real federations.
- NYCN / Summit / any institution-package vocabulary or fixtures.
- Push notifications, app-store badges, in-app purchase chrome.
- Fixture data that could plausibly be read as a real member's record.

## Canonical copy

From [`MemberSurface.astro`](../../../../website/src/components/MemberSurface.astro). Fictional entity and member names are approved per bible §8.

- Frame label: `Member surface · concept`
- Acting as: `Self` — your cryptographic identity. Inside `Brightworks Collective (cooperative)`. Inside `Northeast Worker Federation (federation)`.
- Standing: `Recognized member · Brightworks Collective`. Chips: `Recognized since 2024-11`, `Role: Member`.
- Position (panel title): `Mutual-credit position inside Brightworks`. Stats: `Contributions recorded — 142 hours`, `Allocations drawn — 38 hours`, `Open obligations — 2 outstanding`, `Patronage (Q3) — pending approval`.
- Open decision: `Proposal: Extend the commons compute pool cap for the next quarter`. Chips: `Your vote: pending`, `Closes in 3 days`, `Scope: Federation`.
- Recent receipts:
  - `#482 — Patronage settlement Q3 — scope: Cooperative — stamped 2024-10-31`
  - `#481 — Shared room booking approved — scope: Community — stamped 2024-10-28`
  - `#479 — Federation cross-clearing complete — scope: Federation — stamped 2024-10-24`
- Footer: *"This is an illustration of how governance, economics, scope, and provenance could come together as a coherent member surface. The capabilities beneath each panel are real; the unified interface that ties them together is still being built."*

If a variant changes any string above, the new strings live in the brief — not in source — until reviewed.

## Visual grammar

- **Extends** the canonical `MemberSurface.astro`. Variants are layout / framing changes on the same component grammar; they do not introduce new panel types without an updated brief.
- Civic "screen frame" with a labeled top bar — **not** a phone bezel.
- Stacked panels with subtle hairlines; no left accent stripes (icons + labels carry the meaning).
- Chip pattern from the design language (`.ms-chip`).
- Token-grounded.
- Receipt list uses a 2-column grid (id on the left, text + stamp on the right).

## Accessibility requirements

- `aria-label` on the figure makes the illustrative status explicit: *"An illustrative member-facing surface… This is a design illustration, not a claim to a shipped app."*
- Each panel has an `<h*>` or `aria-labelledby` semantic; the eyebrow text is not the only heading.
- Stats use `<dl>` with `<dt>`/`<dd>` (already implemented).
- Icons `aria-hidden="true"` next to visible text.
- Grayscale-safe — no signal carried by color alone.
- WCAG 2.2 AA contrast in both themes.
- No motion required.
- Renders usefully at 360 px and above; stat grid collapses to one column under 520 px (already implemented).

## Review checklist

Run [VISUAL_REVIEW_CHECKLIST.md](../VISUAL_REVIEW_CHECKLIST.md). Per-brief notes:

- §2 truth label — `illustrative direction` is visible on every variant. A version that hides the label is rejected.
- §3 vocabulary — `wallet` / `balance` / `currency` are the trap. Scan every chip, every stat label, every receipt line.
- §5 substrate honesty — no "complete member app" claim. The unified surface is not shipped.
- §9 generated-image rules — a generated-image sketch of this surface is **not** load-bearing; if a variant ships, it is rebuilt as Astro / SVG with verified strings.
- §10 production-source — the canonical form is `MemberSurface.astro`. Static raster exports are not load-bearing.

## Status

`briefed`. Brief gate satisfied. The existing `MemberSurface.astro` is the source asset; new variants extend it under this brief.
