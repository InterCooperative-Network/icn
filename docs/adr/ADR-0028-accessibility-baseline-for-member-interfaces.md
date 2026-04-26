---
id: "0028"
title: "Accessibility Baseline for Member Interfaces"
status: "proposed"
date: "2026-04-26"
deciders: []
tags: ["accessibility", "participation-floor", "wcag", "language-justice", "deadline-justice", "forward-direction"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "proposed"
references:
  - "ADR-0027 (Action Card Contract)"
  - "docs/design-language/accessibility.md"
  - "GitHub #1610 (language/glossary model)"
  - "GitHub #1611 (promote archived human-interface notes)"
  - "GitHub #1366 (React Native SDK stabilization)"
---

# ADR 0028: Accessibility Baseline for Member Interfaces

## Status

**Proposed (2026-04-26).** Accessibility is a participation floor, not a feature. This ADR names the floor so every member-facing surface has a measurable target.

## Context

ICN's substrate is built on the assumption that participation is universal. A cooperative that excludes members on the basis of bandwidth, language, ability, time, or technology is, by its own logic, not a cooperative. The assumption is structural: it shows up in CCL determinism (so every member gets the same answer), in member standing as a queryable surface (so members can see what they are authorized to do), in mutual credit semantics (so members are not gated by capital).

But the assumption needs to land in interfaces. Today, `docs/design-language/accessibility.md` names the layered model (sensory, cognitive, linguistic, economic, temporal) and is in good shape as a design doctrine. What is missing is an ADR that records the **runtime contract**: which surfaces must meet which floor, what counts as compliance, and how non-compliance is escalated.

Pressures forcing this decision now:
- Action cards (ADR-0027) are the upcoming primary member-facing surface. Without an accessibility contract, cards risk shipping before their assistive semantics are written.
- Glossary endpoint ([#1610](https://github.com/InterCooperative-Network/icn/issues/1610)) is open. It is one piece of the linguistic-layer floor; the floor needs to be named so the endpoint plugs into it.
- The mobile SDK ([#1366](https://github.com/InterCooperative-Network/icn/issues/1366)) is the surface where bandwidth and offline concerns are most acute; it needs a floor to verify against.

## Decision (proposed)

The accessibility baseline applies to every ICN member-facing interface (gateway endpoints emitting member-readable content, mobile/web shells, action cards, receipt rendering, glossary, standing UI). Five layers; each has a floor.

### Sensory layer floor (WCAG 2.2 AA)
- Color contrast meeting WCAG 2.2 AA on every text and meaningful UI element.
- Scalable typography respecting user font-size preferences.
- Motion-reduction respect via `prefers-reduced-motion`.
- No information conveyed by color alone.
- Focus-visible rings on every interactive element.
- Semantic HTML at the DOM level; ARIA used to extend, not replace, semantics.

### Cognitive layer floor
- Plain language at the surface; jargon defined inline or via glossary.
- Stable layouts across pages of the same kind.
- Information chunked; long surfaces broken into logical sections.
- Consistent labels and indicators across surfaces.
- Visible "what's next" affordances on every flow.
- Progress indicators on multi-step flows.

### Linguistic layer floor
- Every member-facing string has a plain-language version.
- Glossary endpoint ([#1610](https://github.com/InterCooperative-Network/icn/issues/1610)) returns canonical definitions for institutional vocabulary.
- Locale metadata declared in package manifest (ADR-0024).
- Translation pipeline: ICN does not perform translation; institutions do. ICN ensures every translatable string is exposed as content, not embedded in code paths.
- No idioms, regional cultural references, or assumed first-language English.

### Economic layer floor
- No economic gating on participation. Members do not pay to read their own standing or to participate in deliberation.
- Mutual credit positions surface in regulatory-safe vocabulary (positions, obligations, allocations, settlement) — never as wallet/balance/payment/currency.
- Hardware floor: surfaces work on low-end Android devices (>= Android 9, 1 GB RAM target).

### Temporal layer floor
- No deadline imposed without an explicit declared rationale.
- Caregiving, shift-work, and timezone considerations expressible as deadline-justice metadata on action cards (ADR-0027).
- Offline tolerance: read paths (standing, receipts, action cards) cached on device; write paths queue and sync.
- No "must respond within N minutes" without a written escalation rule (ADR-0023 territory).

## Compliance and escalation

- Every member-facing endpoint declares (via OpenAPI extension or equivalent) its accessibility profile.
- A future build-time check (registry candidate, future ADR) verifies declared profile against actual emission.
- Accessibility violations are graded as **institutional defects**, not bugs. The remediation pathway runs through ADR-0029 conflict object model once that's accepted; today, escalation is to ICN maintainers.

## Consequences

- The floor is measurable. Surfaces either meet it or do not; "we'll get to a11y later" is no longer a coherent statement.
- Action cards (ADR-0027), glossary endpoint ([#1610](https://github.com/InterCooperative-Network/icn/issues/1610)), mobile SDK ([#1366](https://github.com/InterCooperative-Network/icn/issues/1366)), and standing UI all plug into a written contract.
- Trade-off: meeting all five floors is real engineering work. Some surfaces will ship at partial floor compliance with the gap declared.
- Trade-off: the temporal-justice floor is opinionated. Some institutions prefer hard deadlines; this ADR says hard deadlines are expressible but not the default, and require written rationale.

## Implementation status

Proposed. Floor declared; nothing yet enforces it.

Pre-existing pieces:
- Design-language doctrine in [docs/design-language/accessibility.md](../design-language/accessibility.md).
- Website accessibility patterns visible in `website/src/styles/global.css` and `Layout.astro` (skip-link, focus-visible, prefers-reduced-motion).
- Glossary as a static document at [docs/glossary.md](../glossary.md).

Required to call this `implemented`:
- Glossary endpoint ([#1610](https://github.com/InterCooperative-Network/icn/issues/1610)).
- Locale metadata in package manifest (ADR-0024).
- Action card accessibility metadata (ADR-0027).
- Build-time linter that verifies declared profile against actual surface emission.
- Mobile SDK offline tolerance ([#1366](https://github.com/InterCooperative-Network/icn/issues/1366)).
- Plain-language and glossary contract (registry candidate ADR-0055).

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Defer accessibility to per-surface decisions | Produces inconsistent floors and silent exclusion. The floor must be substrate-level. |
| Adopt WCAG AA without ICN-specific layers | WCAG covers sensory and cognitive. Linguistic, economic, and temporal layers are ICN-specific concerns (mutual credit vocabulary, deadline justice, offline) that WCAG does not address. |
| Make accessibility advisory (recommended, not required) | "Recommended" floors are floors only on paper. The point of an ADR is that compliance is measurable. |
| Build a runtime accessibility scanner that auto-fails non-compliant surfaces | Auto-fail in production blocks members. Compliance enforcement happens at review time and in CI, not at runtime. |
