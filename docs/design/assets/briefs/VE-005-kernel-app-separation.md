---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-13
Last Updated: 2026-05-13
Purpose: Brief for VE-005 — Kernel / App Separation. The canonical architecture explainer for ICN's meaning firewall.
---

# VE-005 — Kernel / App Separation

## Truth label

**`repo-grounded architecture explainer`** for the meaning firewall and the `PolicyOracle` → `ConstraintSet` boundary. Specific oracles depicted (e.g. the `TrustPolicyOracle`) are `implemented / current UI` for the existing ones in `apps/trust` / `icn-trust` and `future-state / roadmap` for any oracle not yet built.

## Source anchors

- [`docs/architecture/KERNEL_APP_SEPARATION.md`](../../../architecture/KERNEL_APP_SEPARATION.md) — canonical doc, including the existing mermaid sequence diagram for TrustPolicyOracle
- `icn/crates/icn-kernel-api/` — kernel API surface (PolicyOracle, ConstraintSet, PolicyDecision)
- `icn/apps/governance/`, `icn/apps/trust` (where applicable) — app-side oracles
- [`docs/architecture/THE_COMMONS.md`](../../../architecture/THE_COMMONS.md) — substrate doctrine
- [`CLAUDE.md`](../../../../CLAUDE.md) — *"The Meaning Firewall"* section
- `icn/crates/icn-trust/` — trust graph types that must **not** appear inside kernel boxes

## Audience

Developer onboarding to ICN. Architect reviewing the kernel/app boundary. Grant reviewer who needs to see that ICN has a real architectural firewall, not just slogans. External contributor who needs to know which crates may import which.

## Core question

> *"How does ICN keep meaning out of the kernel?"*

## One-sentence message

Apps translate domain semantics (trust scores, governance rules, membership criteria) into generic constraints; the kernel enforces those constraints without understanding their meaning — that boundary is the meaning firewall, and it is structural, not stylistic.

## Must show

- Two regions: **Apps (domain-aware)** and **Kernel (domain-blind)**, with a labeled boundary between them.
- The boundary labeled **"Meaning Firewall"** explicitly.
- The objects that cross the boundary in **one direction** (app → kernel): `ConstraintSet`, `PolicyDecision`, capability tokens.
- Examples of what the kernel **never sees**: trust scores, trust classes, governance rules, membership criteria, charter clauses.
- The `PolicyOracle` trait implemented on the app side — including at least `TrustPolicyOracle` as a real example.
- The flow: `PolicyRequest` enters → app computes domain-specific value → app converts to generic constraints → returns `PolicyDecision` → kernel enforces.
- Two example crate names per region: kernel-side (`icn-kernel-api`, `icn-core`, `icn-net`, `icn-gateway`, `icn-gossip`) and app-side (`apps/governance`, `apps/trust`, future `apps/membership`).

## Must avoid

- Any depiction of a domain type (e.g. `TrustGraph`, `TrustClass`, `MembershipCriteria`) inside the kernel region. This is the visible violation the diagram exists to teach against.
- Implying that the kernel reads semantic meaning of any kind.
- Implying the apps can bypass the kernel — they cannot.
- Two-way arrows across the boundary that imply the kernel can ask the app for meaning. Constraints flow inward; enforcement flows outward.
- Implying that all oracles depicted are shipped if some are still proposed. Label `future-state / roadmap` where relevant.
- Crypto / token / wallet imagery; the boundary is architectural, not financial.
- Renderings that look like a corporate microservices diagram. This is a meaning firewall, not a service mesh.

## Canonical copy

These strings come from [`KERNEL_APP_SEPARATION.md`](../../../architecture/KERNEL_APP_SEPARATION.md) and [`CLAUDE.md`](../../../../CLAUDE.md). Do not paraphrase.

- Title: *"Kernel / App Separation — The Meaning Firewall"*
- Boundary label: *"Meaning Firewall"*
- Boundary subtitle: *"Domain semantics end here. Constraints flow inward; enforcement flows outward."*
- Apps box subtitle: *"Domain-aware. Translate meaning into constraints."*
- Kernel box subtitle: *"Domain-blind. Enforces constraints without understanding meaning."*
- Allowed-across-boundary list: `ConstraintSet`, `PolicyDecision { Allow, Deny }`, capability tokens.
- Forbidden-in-kernel list: trust scores, trust classes, governance rules, membership criteria, charter clauses, scope semantics.
- Example oracle: `TrustPolicyOracle` — translates trust score → `ConstraintSet { rate_limit, credit_multiplier, max_topics, trust_score (custom field) }`.

## Visual grammar

- New SVG-in-docs figure or new Astro/SVG component (no existing canonical primitive — VE-005 introduces it). Mermaid is acceptable for the sequence diagram form (already present in `KERNEL_APP_SEPARATION.md`); the **conceptual** figure should be a source-asset SVG or Astro component for legibility.
- Two regions stacked or side-by-side. Boundary line is the visual anchor — clearly labeled, visually heavier than other strokes.
- Arrows are strictly directional. No bidirectional arrows across the boundary.
- Icons drawn from the canonical symbol family. Domain icons (e.g. `authority`, `governance`) live only in the app region; kernel-side concepts get neutral structural marks.
- Token-grounded; both themes pass contrast independently.
- Civic and structural, not corporate-microservices.

## Accessibility requirements

- `aria-label` on the figure: *"The meaning firewall: apps translate domain semantics into generic constraints; the kernel enforces them without understanding meaning."*
- Each region readable as text — region label + subtitle + crate list + boundary list.
- Arrows `aria-hidden="true"` with `role="presentation"`; meaning carried in text as well as direction.
- Grayscale-safe — the boundary remains visually heavier than other strokes without color.
- WCAG 2.2 AA contrast on every label, both themes.
- No motion required; if a sequence variant animates a request flow, animation respects reduced-motion.

## Review checklist

Run [VISUAL_REVIEW_CHECKLIST.md](../VISUAL_REVIEW_CHECKLIST.md). Per-brief notes:

- §6 kernel / app boundary — the load-bearing review for this brief. Domain types in the kernel region is a structural failure, not a styling note.
- §1 source grounding — the example oracle's `ConstraintSet` fields are verified against `icn-kernel-api` and the trust oracle's actual constraints. **Do not** invent fields.
- §3 vocabulary — `PolicyOracle`, `ConstraintSet`, `PolicyDecision` are precise. Do not soften them to "policy plugin" / "rules object" / "decision object."
- §9 generated-image rules — a generated-image sketch of this diagram is **not** load-bearing. Architecture explainers especially must be rebuilt as source assets with verified strings; generated-text labels in this brief are presumed wrong until checked against the source.

## Status

`briefed`. Brief gate satisfied. Source-asset build (SVG-in-docs or Astro component) may begin.
