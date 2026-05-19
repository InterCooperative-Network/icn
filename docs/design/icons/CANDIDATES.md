---
Status: operational
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-19
Last Updated: 2026-05-19
Purpose: Candidate icon family review package — 14 candidate icons proposed for promotion into `website/src/data/icons.ts`. Each carries a documented rationale, distinct-silhouette test, ARIA usage, and explicit do/don't notes. Promotion blocked on designer sign-off + 16px on-device pass.
---

# Candidate Icon Family · Review Package

> **One sentence.** Fourteen candidate icons proposed for promotion into ICN's production icon set, each anchored to a canonical concept id and held to the icon-style rules in [`docs/design-language/brief-v0.md §7a`](../../design-language/brief-v0.md).

This document is the review package. It lives in the canonical repo so the icon family is reviewable in-place, without re-opening the Claude Design seed. The contact sheet is in [`contact-sheet.svg`](contact-sheet.svg).

**Status:** candidate icons · review required · not production canonical.

**Promotion gate:** the seven review gates in [`../CLAUDE_DESIGN_REVIEW_PROTOCOL.md`](../CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3, with the truth-label, accessibility, and source-doc drift gates load-bearing for icons. Each icon needs a designer sign-off and a 16px silhouette pass before it lands in production.

**Production destination (when promoted):** [`website/src/data/icons.ts`](../../../website/src/data/icons.ts).

**Contact sheet:** [`contact-sheet.svg`](contact-sheet.svg) — all 25 icons (11 production + 14 candidates) at a glance. Candidates are amber-marked.

**Source:** the v0.1+v0.2 Claude Design seed (the seed's `assets/icons.js` and the seed's `ICON_CANDIDATES.md`). See [`../claude-design-seed/CHANGELOG.md`](../claude-design-seed/CHANGELOG.md) for seed identity.

---

## Style obligations (apply to every candidate)

- 24×24 viewBox
- 1.5px stroke, `currentColor`, no fills
- `stroke-linecap: round`, `stroke-linejoin: round`
- No baked-in color, no gradients, no decorative flourish
- Silhouette distinct from every other icon at 16px
- No literal handshake / coin / wallet / globe / scroll-with-ribbon
- No emoji equivalent; no Material / Heroicons substitution
- Decorative use → `aria-hidden="true"`; standalone use → `aria-label`
- Paired with a visible text label on first use

These rules are enforced by [`website/src/components/Icon.astro`](../../../website/src/components/Icon.astro) for any icon that lands in production.

---

## Per-icon rationale

### 1 · `membership`

| Field | Value |
|-------|-------|
| Concept | Recognized participation in a scope |
| Silhouette | Horizontal member card · avatar dot + two text lines |
| Distinct from | `policy` (vertical doc, corner-fold), `identity` (just person), `member_experience` (frame around person) |
| Public label | "Membership" |
| ARIA standalone | `aria-label="Membership in this scope"` |
| Do | Use beside the scope name; use in scope-switcher and membership cards |
| Don't | Use for identity-in-general; don't replace with a generic person glyph |
| Status | candidate · review required |

### 2 · `attestation`

| Field | Value |
|-------|-------|
| Concept | A signed statement another party can rely on |
| Silhouette | A declared horizontal line with a verifying check below |
| Distinct from | `standing` (shield + check inside), `provenance` (linked records) |
| Public label | "Attestation" or "Verified by" |
| ARIA standalone | `aria-label="Signed attestation"` |
| Do | Use on signed witness statements, endorsements, social-trust evidence |
| Don't | Use for cryptographic proof generally (use `provenance`); don't use for stamps of authority (use `authority`) |
| Status | candidate · review required |

### 3 · `role`

| Field | Value |
|-------|-------|
| Concept | A named bundle of authority and responsibility |
| Silhouette | Two stacked chevrons — institutional rank insignia |
| Distinct from | `identity` (person), `authority` (sealed stamp), `role_grant` (seal + arrow + person) |
| Public label | "Role" |
| ARIA standalone | `aria-label="Role in this scope"` |
| Do | Pair with the role name ("Role · Member", "Role · Steward") |
| Don't | Use the role icon as a status badge without naming the role |
| Status | candidate · review required |

### 4 · `agreement`

| Field | Value |
|-------|-------|
| Concept | Recorded understanding between two parties |
| Silhouette | Two small boxes connected by two horizontal lines |
| Distinct from | `federation` (three circles, not two boxes), `obligation` (one bound rectangle) |
| Public label | "Agreement" |
| ARIA standalone | `aria-label="Two-party agreement"` |
| Do | Use for mutual contracts, MOUs, charter side-agreements |
| Don't | Use literal handshake imagery; don't use for single-party obligations |
| Status | candidate · review required |

### 5 · `resource`

| Field | Value |
|-------|-------|
| Concept | Something shared, used, or governed |
| Silhouette | Three stacked horizontal slabs |
| Distinct from | `commons` (3×3 grid), `accounting` (balance scale) |
| Public label | "Resource" |
| ARIA standalone | `aria-label="Tracked resource"` |
| Do | Use for material or computational stock the institution holds |
| Don't | Use for member-owned positions (use `position` / `accounting`); don't use stacked-coin imagery |
| Status | candidate · review required |

### 6 · `allocation`

| Field | Value |
|-------|-------|
| Concept | A decision to direct resources or authority |
| Silhouette | Circle with a vertical bisector + diagonal slice line |
| Distinct from | `governance` (three-node deliberation), `settlement` (arrow into target) |
| Public label | "Allocation" |
| ARIA standalone | `aria-label="Allocation decision"` |
| Do | Use on budget approvals, capacity assignments, patronage allocations |
| Don't | Use as a generic "split" icon for unrelated UI; don't use for income / revenue |
| Status | candidate · review required |

### 7 · `obligation`

| Field | Value |
|-------|-------|
| Concept | A structured claim one party has on another |
| Silhouette | A rectangle bound by a vertical cinch band |
| Distinct from | `agreement` (two separate boxes), `commitment-to-self` (no icon — not a concept) |
| Public label | "Obligation" |
| ARIA standalone | `aria-label="Open obligation"` |
| Do | Pair with the obligation's state ("Open obligation", "Settled obligation") |
| Don't | Use the word "debt" with this icon — vocabulary contract forbids it |
| Status | candidate · review required |

### 8 · `settlement`

| Field | Value |
|-------|-------|
| Concept | Closing an obligation per its terms |
| Silhouette | Directed arrow arriving into a target circle that carries the verifying tick |
| Distinct from | `attestation` (no arrow), `execution` (gate + arrow through) |
| Public label | "Settlement" |
| ARIA standalone | `aria-label="Settled obligation"` |
| Do | Use on receipts that close an obligation; pair with the receipt id |
| Don't | Use the word "payment" with this icon — vocabulary contract forbids it |
| Status | candidate · review required |

### 9 · `dispute`

| Field | Value |
|-------|-------|
| Concept | A structured challenge or appeal path |
| Silhouette | Two opposing horizontal arrows |
| Distinct from | `agreement` (parallel lines bound), `execution` (single arrow through gate) |
| Public label | "Dispute" or "Challenge" |
| ARIA standalone | `aria-label="Dispute path"` |
| Do | Use on the "Challenge" affordance of the challenge / review / exit pattern; pair with the dispute window |
| Don't | Use literal scales-of-justice imagery (that conflicts with `accounting`) |
| Status | candidate · review required |

### 10 · `unit`

| Field | Value |
|-------|-------|
| Concept | Generic accounting unit (never currency, never coin) |
| Silhouette | A single hexagon |
| Distinct from | `commons` (square grid), `policy` (document) — only hex in the family |
| Public label | "Unit" |
| ARIA standalone | `aria-label="Accounting unit"` |
| Do | Pair with the unit's name ("Brightworks hour", "NYCN credit"); show the count alongside |
| Don't | Use for currency, token, coin, or any monetary metaphor |
| Status | candidate · review required |

### 11 · `charter`

| Field | Value |
|-------|-------|
| Concept | The founding rules of an institution |
| Silhouette | Tall document with rule-lines + a circular seal near the bottom |
| Distinct from | `policy` (doc + corner-fold + rule-lines, no seal) |
| Public label | "Charter" |
| ARIA standalone | `aria-label="Institution charter"` |
| Do | Use beside the institution name; link to the canonical charter record |
| Don't | Use heraldic seal imagery — the seal here is a mark of "sealed", not "official" |
| Status | candidate · review required |

### 12 · `commons_credit`

| Field | Value |
|-------|-------|
| Concept | A credit drawn against a commons it does not own |
| Silhouette | The commons grid with the center cell emphasized as a marker |
| Distinct from | `commons` (grid only), `unit` (hexagon) |
| Public label | "Commons credit" |
| ARIA standalone | `aria-label="Commons credit position"` |
| Do | Pair with the commons name and the credit count |
| Don't | Use the word "balance" — vocabulary contract forbids it |
| Status | candidate · review required |

### 13 · `role_grant`

| Field | Value |
|-------|-------|
| Concept | The act of granting a role to a member |
| Silhouette | A small institutional seal with an arrow descending into a member figure |
| Distinct from | `role` (two chevrons, no arrow, no person), `authority` (single sealed stamp) |
| Public label | "Role grant" |
| ARIA standalone | `aria-label="Role granted to member"` |
| Do | Use on the receipt of a role-grant action; pair with the role name and the mandate that authorized it |
| Don't | Use as a generic "promote" icon |
| Status | candidate · review required |

### 14 · `language`

| Field | Value |
|-------|-------|
| Concept | A language used by a member-facing surface |
| Silhouette | Speech container with two horizontal text lines and a tail |
| Distinct from | Anything else — the only speech-shaped icon in the family |
| Public label | "Language" |
| ARIA standalone | `aria-label="Interface language"` |
| Do | Use on the explicit language selector; do not auto-detect from IP |
| Don't | Use a globe (that triggers the "glowing network globe" reject pattern in [`../MUST_NOT_SHIP.md`](../MUST_NOT_SHIP.md) item 9) |
| Status | candidate · review required |

---

## Icon review checklist

For each candidate, the reviewer confirms:

- [ ] Renders cleanly at 16px, 20px, 24px, 32px, 48px
- [ ] Silhouette distinct from every other icon in the family (compare at 16px on the contact sheet)
- [ ] Reads in light, dark, hc-dark, hc-light, and grayscale
- [ ] No baked-in color; renders correctly with `color: currentColor`
- [ ] No literal handshake / coin / wallet / globe / faux-government seal
- [ ] No English idiom dependence
- [ ] Pairs with the canonical concept id in [`docs/design-language/concept-map.md`](../../design-language/concept-map.md)
- [ ] Has a documented `aria-label` for standalone use
- [ ] Has a documented public label for paired use
- [ ] Vocabulary contract for the concept is preserved in the icon's "Do" / "Don't" notes

---

## "Do not ship until" checklist

The 14 candidates do not ship into production until every box below closes green. This is the production-adoption gate; it is separate from this review-package PR.

- [ ] Designer signs off on each silhouette
- [ ] 16px render checked on real devices (not in-iframe)
- [ ] All 25 icons rendered side-by-side and confirmed silhouette-distinct (the contact sheet is the artifact for this check)
- [ ] Each candidate has a matching public-label string in the localization corpus
- [ ] [`website/src/data/icons.ts`](../../../website/src/data/icons.ts) is updated through a docs-only candidate-review PR (this one) first, then a production-adoption PR — never both in a single PR
- [ ] No icon ships before its referenced canonical concept is stable in [`docs/design-language/concept-map.md`](../../design-language/concept-map.md) (every candidate's concept id already exists upstream)

The production-adoption PR is out of scope here. It runs after sign-off.

---

## Open questions for the reviewer

- The seed adds `role_grant` and `commons_credit` as joined-word concept ids. [`docs/design-language/concept-map.md`](../../design-language/concept-map.md) uses spaced "Commons credit" / "Role grant" in some places and hyphenated `role-grant` / `commons-credit` in others. Decide canonical hyphenation before the production-adoption PR.
- `attestation` could be split into `social_attestation` (witness) vs `cryptographic_attestation` (signed proof) if the kernel grows distinct types. Single icon stands today.
- `unit` is deliberately abstract (hexagon). If the kernel introduces a distinguished "Commons-credit unit" vs "Time-credit unit" vs "External settlement unit", the unit icon may need a small marker overlay. Track as a v0.3 concern.

---

## See also

- [`contact-sheet.svg`](contact-sheet.svg) — all 25 icons (11 production + 14 candidates), at a glance, in a single SVG
- [`../CLAUDE_DESIGN_REVIEW_PROTOCOL.md`](../CLAUDE_DESIGN_REVIEW_PROTOCOL.md) — review gates the candidates must clear before promotion
- [`../MUST_NOT_SHIP.md`](../MUST_NOT_SHIP.md) — hard rejection floor (item 9: glowing-globe / central-hub maps)
- [`../../design-language/concept-map.md`](../../design-language/concept-map.md) — canonical concept ids the icons anchor to
- [`../../design-language/brief-v0.md`](../../design-language/brief-v0.md) §7a — canonical icon style rules
- [`../../../website/src/data/icons.ts`](../../../website/src/data/icons.ts) — current 11 production icons (the destination, when candidates are promoted)
- [`../../../website/src/components/Icon.astro`](../../../website/src/components/Icon.astro) — production Icon component enforcing focus / `aria-hidden` / `currentColor`
- [`../claude-design-seed/CHANGELOG.md`](../claude-design-seed/CHANGELOG.md) — seed identity and provenance
