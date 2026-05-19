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
| 9 | [docs/design/CLAUDE_DESIGN_CONTEXT.md](CLAUDE_DESIGN_CONTEXT.md) | Self-contained briefing for paste into Claude Design or any external collaborator — brand, tokens, vocabulary, surface inventory, kernel binding |
| 10 | [docs/design/CLAUDE_DESIGN_SETUP.md](CLAUDE_DESIGN_SETUP.md) | How to use Claude Design — external paste flow, local `icn-design-advisor` agent (repo-shipped), optional `design` plugin pack, mixed-mode workflow, job-card templates |
| 11 | [docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) | Seven-gate review protocol for promoting Claude Design seed artifacts to canonical — holds the canonical-vs-generated boundary |
| 12 | [docs/design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) | Seed-level handoff template (seed → Claude Code) — fill one copy per seed before any promotion work |
| 13 | [docs/design/MUST_NOT_SHIP.md](MUST_NOT_SHIP.md) | Hard rejection floor — twelve patterns and artifacts that disqualify a surface from production |
| 14 | [docs/design/claude-design-seed/README.md](claude-design-seed/README.md) | Seed directory workflow — what a seed is, where canonical truth lives, how seeds are reviewed and promoted (companions: `CHANGELOG.md`, `REVIEW_NOTES.md`) |
| 15 | [docs/design/icons/CANDIDATES.md](icons/CANDIDATES.md) | Candidate icon family review package — 14 candidate icons proposed for promotion into `website/src/data/icons.ts`, with per-icon rationale, ARIA usage, distinct-silhouette test, and "do not ship until" checklist (companion: `contact-sheet.svg`) |

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

## Kernel architecture binding

The design system is not free-standing. It is the human-layer expression of ICN's kernel architecture. Every load-bearing design choice encodes a kernel invariant at the surface a member can see and act on.

The mapping is documented in [`docs/DESIGN_PRINCIPLES.md`](../DESIGN_PRINCIPLES.md), which catalogs the three-tier invariant hierarchy (operational / firewall / frozen-core) and the eight kernel primitives ([ADR-001](../strategy/ADR-001-What-ICN-Is.md)). The table below shows which design principle expresses which kernel invariant. A change on one side implies review on the other.

### Design principle → kernel invariant

| # | Design principle (from §"Design principles (compact)") | Kernel tier and invariant | What the surface is enforcing |
|---|--------------------------------------------------------|---------------------------|-------------------------------|
| 1 | Make the invisible visible | Tier-3 #10 `UniversalReceiptGeneration` + Tier-2 #6 opaque receipt cascade | If the kernel cannot show proof, the UI must not assert legitimacy |
| 2 | Member-first | ADR-001 entity model + Tier-3 #1 `RightOfExit` | The DID is the anchor; the institution is a changing context the member can leave |
| 3 | Scope coequality | ADR-001 entity model (no civilizational ladder) | Cooperatives, communities, federations are co-equal kernel entities, not nested |
| 4 | Proof must be understandable | Tier-1 §1.3 canonical encodings + Tier-3 #10 | The receipt is canonical; the summary is the UI affordance over it |
| 5 | Civic seriousness without bureaucratic deadness | (no kernel binding — tone choice) | Tone is the only fully app-layer principle in this list |
| 6 | Mobile-first and low-bandwidth | Tier-1 §1.1 adversarial-by-default | A member on a 2 Mbps link on a 5-year-old phone is the threat-modeled participant |
| 7 | No fake futurism | [ICN-Definition.md §"What ICN Is Not"](../strategy/ICN-Definition.md) | The kernel is not a blockchain or a platform; the UI must not imply it is |
| 8 | Plain language is required | Tier-3 #4 `NoSilentPowerChanges` + [`CONTENT_STYLE_GUIDE.md`](CONTENT_STYLE_GUIDE.md) §"Regulatory-safe vocabulary" | Authz changes must be legible as plain power deltas, not jargon |
| 9 | Receipts are first-class UI | Tier-3 #10 `UniversalReceiptGeneration` | If a transition matters, the kernel emits a receipt; the UI exposes it |
| 10 | Reduced motion and large tap targets are defaults | Tier-1 §1.4 no-panics (extended to participation: the surface does not "panic" the member) | The accessibility floor is part of institutional legitimacy, not polish |

### Product surface → kernel primitive(s)

The eight primitives ([ADR-001 §"The Eight Primitives"](../strategy/ADR-001-What-ICN-Is.md)) are not visible to members. They surface through the product surfaces below. A surface that renders a primitive is responsible for not silently extending it past what the kernel can prove.

| Surface | Primary primitive | Secondary | Surface responsibility |
|---------|-------------------|-----------|-----------------------|
| Public website | (none — descriptive only) | Naming | Must not claim capabilities the kernel cannot prove (bound by [ADR-0032](../adr/ADR-0032-website-truth-boundary.md), [ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md)) |
| Documentation | (none — descriptive only) | — | Same truth-boundary rule as website |
| Member shell | Identity | Authorization, Naming | Anchors the DID; renders capability set; switches scope without losing identity |
| Standing view | Authorization | Identity, State | Renders the member's capability graph in plain language. App-layer trust scores → UI bands |
| Action cards | Authorization | State, Coordination | Every card declares the mandate ([ADR-0027](../adr/ADR-0027-action-card-contract.md)) — the capability that backs it — before confirm |
| Receipts / provenance | State | Identity | Renders the opaque receipt cascade. Progressive disclosure: summary → explanation → raw record |
| Governance flows | Coordination | State, Time, Authorization, Communication | The "Governance" app renders here. UI must not imply consensus the app has not reached |
| Federation / operator surfaces | Communication | Coordination, Naming | Renders inter-entity coordination. Operators see no more provenance than members on shared records |
| Compute / commons surfaces | Compute | State, Authorization | Workload manifest is the member-readable contract — the same artifact the kernel runs |

### Bidirectional rule

- **Kernel changes that affect a primitive imply UI review.** A new ConstraintSet field, a new opaque-receipt class, or a Wave-2+ firewall extraction may require copy, vocabulary, or component changes in the surface that renders it.
- **UI changes must not violate the firewall.** Adding a UI affordance that shows information the kernel cannot prove (or that depends on app-layer semantics leaking into a kernel response) is a tier-2 violation expressed at the human layer — not just a content bug.

### What this binding does not do

- It does not promote the design system to a kernel-enforced surface. Design doctrine remains app-layer, evolved through ADRs and the accessibility gate.
- It does not lock product navigation. The mapping above is per-surface-category, not per-screen.
- It does not constrain institution packages (NYCN, Summit, future federations), which bring their own brand layer on top of the ICN substrate.

## Non-goals

- Not a brand kit for a specific institution.
- Not a token system or a CSS framework. Tokens may follow; this document does not invent them.
- Not a doctrine that locks navigation models or component APIs.
- Not a substitute for accessibility testing on real devices with real members.

## How to use this system

- Building a new ICN-native UI? Start with the visual system, then the language brief, then the accessibility baseline.
- Building an institution package (NYCN, Summit, future federations)? Inherit the design system, add your own brand layer, do not modify ICN-side primitives.
- Bringing in an external collaborator (Claude Design, agency, contributor)? Hand them [CLAUDE_DESIGN_CONTEXT.md](CLAUDE_DESIGN_CONTEXT.md) verbatim. For the full workflow (paste-flow templates, doc-to-job attachment table, repo-shipped `icn-design-advisor` agent, optional `design` plugin pack, failure modes), see [CLAUDE_DESIGN_SETUP.md](CLAUDE_DESIGN_SETUP.md).
- Adding a new public claim to the website? It is bound by ADR-0032 and ADR-0033 — claim only what proof supports.

## See also

- [docs/DESIGN_PRINCIPLES.md](../DESIGN_PRINCIPLES.md) — canonical three-tier kernel invariant index (the kernel-side counterpart to this document)
- [docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) — visual explainer control plane (doctrine for diagrams, infographics, generated images, source assets)
- [docs/design/assets/ASSET_REGISTER.md](assets/ASSET_REGISTER.md) — live register of planned and tracked visual assets
- [docs/design/assets/VISUAL_REVIEW_CHECKLIST.md](assets/VISUAL_REVIEW_CHECKLIST.md) — pre-ship review for every visual explainer
- [docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md](../strategy/ICN_CONSTITUTIONAL_ROADMAP.md)
- [docs/architecture/KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md)
- [docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md)
- [docs/strategy/ADR-001-What-ICN-Is.md](../strategy/ADR-001-What-ICN-Is.md) — the eight kernel primitives this design system renders
