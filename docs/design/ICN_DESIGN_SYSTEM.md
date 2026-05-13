---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-04-26
Last Updated: 2026-04-26
Purpose: Top-level entry point for the ICN Commons Design System. Anchors visual doctrine, design language, accessibility baseline, content style, and per-surface guidance under one named system.
---

# ICN Commons Design System

> **One sentence.** ICN looks like democratic infrastructure that ordinary people can actually use — civic, legible, cooperative, technical without being cold.

This document is the entry point. Substance lives in subdocuments. Treat this file as the table of contents; do not duplicate content here.

## What this system covers

ICN is institutional infrastructure for cooperatives, communities, and federations. The design system spans every surface where ICN meets a person:

- public website
- documentation
- member shell (the surface a member sees when participating in their institutions)
- standing view (who the member is, where they have authority, what receipts they hold)
- action cards (member-facing mandates and authorizations)
- receipts and provenance views
- governance flows (proposals, deliberation, mandates, dispute paths)
- federation, operator, and admin surfaces
- compute and commons surfaces

It does **not** cover NYCN, Summit, or any specific institution package. Institution packages bring their own brand, their own copy, and their own surfaces; they consume ICN primitives and design tokens, not the other way around.

## Brand position (one paragraph)

ICN is institutional infrastructure, not a crypto product, not project-management SaaS, not a social network. It should feel civic, trustworthy, cooperative, technical, legible, and durable. It should work on a five-year-old phone, in a public library, in three languages the member chose, and in conditions where the member is tired and skeptical.

What ICN must not look like: a venture-backed dashboard, a speculative finance product, a cyberpunk terminal, a gamified social platform, a dead government portal, or AI-vapor futurism.

## Constituent documents

Living docs that make up the system. Read in order on first pass:

| # | Document | Role |
|---|----------|------|
| 1 | [docs/design-language/brief-v0.md](../design-language/brief-v0.md) | Canonical design language brief — symbols, scope, member-first framing |
| 2 | [docs/design/ICN_VISUAL_SYSTEM.md](ICN_VISUAL_SYSTEM.md) | Stable visual doctrine across web, docs, product, demos |
| 3 | [docs/design-language/concept-map.md](../design-language/concept-map.md) | Shared vocabulary for institutional concepts shown in UI |
| 4 | [docs/design/ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) | Floor for every member-facing surface (WCAG 2.2 AA, plus ICN-specific obligations) |
| 5 | [docs/design-language/accessibility.md](../design-language/accessibility.md) | Detailed accessibility patterns, copy, and component-level guidance |
| 6 | [docs/design/CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) | Plain language, vocabulary, dangerous-action copy, formal-record markers |
| 7 | [docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) | Control plane for every visual explainer — source hierarchy, truth labels, brief gate, generated-image workflow |
| 8 | [docs/design/assets/ASSET_REGISTER.md](assets/ASSET_REGISTER.md) | Live register of planned and tracked visual assets (one row per asset, briefs in `assets/briefs/`) |
| 9 | [docs/design/CLAUDE_DESIGN_CONTEXT.md](CLAUDE_DESIGN_CONTEXT.md) | Ready-to-paste design context for Claude Design / external collaborators |

ADRs that bind this work:

- [ADR-0028 — Accessibility Baseline for Member Interfaces](../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md) (`proposed`)
- [ADR-0027 — Action Card Contract](../adr/ADR-0027-action-card-contract.md) (`proposed`)
- [ADR-0032 — Website Truth Boundary](../adr/ADR-0032-website-truth-boundary.md) (`accepted`)
- [ADR-0033 — Public Maturity Claims and Evidence Links](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md) (`proposed`)

## Design principles (compact)

These are the load-bearing rules. The full statement of each lives in the visual system and language brief; this list exists so a reviewer can hold the system in their head.

1. **Make the invisible visible.** Authority, scope, mandates, and receipts must be legible without expert knowledge.
2. **Member-first.** The person persists across institutional contexts; the institution is the changing context, not the anchor.
3. **Scope coequality.** Cooperatives, communities, and federations are co-equal forms with different jobs — never depicted as a hierarchy or a ladder.
4. **Proof must be understandable.** Provenance reveals through progressive disclosure: a one-line answer, then explanation, then raw material.
5. **Civic seriousness without bureaucratic deadness.** Calm, capable, trustworthy. Never sterile, authoritarian, or theatrical.
6. **Mobile-first and low-bandwidth.** A five-year-old phone on a slow link is the design target, not the edge case.
7. **No fake futurism.** No glassmorphism, neon, or "AI sheen" unless it serves clarity. Decoration that doesn't carry meaning is removed.
8. **Plain language is required, not a courtesy.** If a term needs jargon, it gets a glossary entry; the UI links to the entry.
9. **Receipts are first-class UI.** A member must be able to read, share, and export a receipt. Receipts are not buried in a settings menu.
10. **Reduced motion and large tap targets are defaults, not options.** Accessibility is the floor.

## Product-surface obligations (compact)

Each surface has a non-negotiable floor. Detail lives in the accessibility baseline; this is the index.

- **Public website**: WCAG 2.2 AA, no member personal data, maturity-band honesty (see ADR-0032), no NYCN/Summit references.
- **Documentation**: scannable, link-rich, plain-language summary at the top, formal definitions linked.
- **Member shell**: identity-aware, scope-aware, never displays an action button without showing the authority that backs it.
- **Standing view**: a member can read their own standing without help. Includes who attests it and where to challenge it.
- **Action cards**: every card declares the mandate that authorized it and the receipt produced when it acts.
- **Receipts and provenance**: progressive disclosure (summary → explanation → raw record), always exportable.
- **Governance flows**: proposal, deliberation, threshold, decision, receipt — every step legible to a non-expert member.
- **Federation/operator surfaces**: power asymmetry is shown plainly; operators see the same provenance the member sees, never more.
- **Compute/commons surfaces**: workload manifest is the member-readable contract, not a hidden config blob.

## Non-goals

- Not a brand kit for a specific institution.
- Not a token system or a CSS framework. Tokens may follow; this document does not invent them.
- Not a doctrine that locks navigation models or component APIs.
- Not a substitute for accessibility testing on real devices with real members.

## How to use this system

- Building a new ICN-native UI? Start with the visual system, then the language brief, then the accessibility baseline.
- Building an institution package (NYCN, Summit, future federations)? Inherit the design system, add your own brand layer, do not modify ICN-side primitives.
- Bringing in an external collaborator (Claude Design, agency, contributor)? Hand them [CLAUDE_DESIGN_CONTEXT.md](CLAUDE_DESIGN_CONTEXT.md) verbatim.
- Adding a new public claim to the website? It is bound by ADR-0032 and ADR-0033 — claim only what proof supports.

## See also

- [docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) — visual explainer control plane (doctrine for diagrams, infographics, generated images, source assets)
- [docs/design/assets/ASSET_REGISTER.md](assets/ASSET_REGISTER.md) — live register of planned and tracked visual assets
- [docs/design/assets/VISUAL_REVIEW_CHECKLIST.md](assets/VISUAL_REVIEW_CHECKLIST.md) — pre-ship review for every visual explainer
- [docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md](../strategy/ICN_CONSTITUTIONAL_ROADMAP.md)
- [docs/architecture/KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md)
- [docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md)
