---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-13
Last Updated: 2026-05-13
Purpose: The gate every ICN visual explainer clears before shipping. Cross-cuts source grounding, truth labeling, vocabulary, accessibility, and the kernel/app boundary.
---

# ICN Visual Review Checklist

Use this list before any visual explainer ships — public website figure, doc figure, Astro/SVG component, deck slide intended for outside audiences. Doctrine is in the [ICN Visual Explainer Bible](../ICN_VISUAL_EXPLAINER_BIBLE.md). This file is the gate.

A visual that fails any item is not shippable. Document the failure on the brief; do not ship around it.

## 1. Source grounding

- [ ] The brief lists source anchors in the file paths under [bible §2](../ICN_VISUAL_EXPLAINER_BIBLE.md#2-source-hierarchy).
- [ ] Every claim made by the visual is supported by one of those anchors.
- [ ] Where the visual contradicts a higher-priority anchor, the visual has been revised — not the anchor.
- [ ] Anchors have been re-checked against `main` within the last 30 days, or sooner if a phase transition occurred.

## 2. Truth label

- [ ] The visual carries one of the seven labels from [bible §3](../ICN_VISUAL_EXPLAINER_BIBLE.md#3-truth-labels): `implemented / current UI`, `repo-grounded public explainer`, `repo-grounded architecture explainer`, `illustrative direction`, `future-state / roadmap`, `historical`, `do not use`.
- [ ] The label appears in the brief, in the visible figure caption / chip / alt text wherever the asset lands, and in the filename if applicable.
- [ ] If the visual sits between two labels, it carries the less optimistic one.

## 3. Vocabulary

- [ ] No prohibited terms in [bible §5](../ICN_VISUAL_EXPLAINER_BIBLE.md#5-vocabulary-rules-binding): no `wallet`, no `balance`, no `payment`, no `currency`, no `token`, no `coin`, no `transaction` for ICN-native primitives.
- [ ] Preferred vocabulary used where applicable: `identity`, `standing`, `mandate`, `obligation`, `allocation`, `settlement`, `unit`, `position`, `receipt`, `provenance`.
- [ ] No crypto / web3 / token framing.
- [ ] External systems (bank transfer, ACH, currency) named only when the visual is specifically describing the external system.

## 4. Accessibility floor

- [ ] Meaning carried by structure + label + icon silhouette, **not color alone**. Verified by grayscale pass.
- [ ] WCAG 2.2 AA contrast met for text and meaningful graphics (4.5:1 body, 3:1 large text / UI). Both themes.
- [ ] Silhouette-distinct at 16 px for any icon-bearing element.
- [ ] Alt text or `aria-label` describes the claim, not the chrome.
- [ ] No auto-playing motion; reduced-motion preference honored; motion never the sole carrier of state.
- [ ] Tap targets ≥ 44×44 CSS px where the visual depicts a real interactive surface.
- [ ] First reading layer requires no expert vocabulary; jargon links to a glossary entry.
- [ ] Asset weight reasonable for 2 Mbps on a low-end device.

## 5. Substrate honesty

- [ ] No production-readiness claim across the substrate.
- [ ] No formal NYCN pilot claim. NYCN is the *intended* first cooperative partner.
- [ ] No live multi-coop federation claim. The federation primitives exist; live federation does not.
- [ ] No complete mobile / member app claim. The unified member surface is illustrative direction.
- [ ] Action-card runtime depicted as **three of five** currently emitted source paths if all five are shown — or labeled `future-state / roadmap`.
- [ ] K3s deployment depicted as homelab-class, not production multi-tenant.
- [ ] Numbers (LOC, tests, crates, phases) match [`STATE.md`](../../STATE.md) / [`PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md).

## 6. Kernel / app boundary

- [ ] No kernel-side concept depicted importing a domain semantic (trust score, member status, governance rule) — that violates the meaning firewall.
- [ ] Where the visual depicts policy → constraints, it shows the boundary at which domain semantics convert to generic `ConstraintSet` / `PolicyDecision`.
- [ ] App-side concepts are not depicted *inside* the kernel box.

## 7. Scope / package boundary

- [ ] No NYCN-specific copy, vocabulary, fixtures, branding, or partner reference in a generic ICN asset.
- [ ] No Summit-specific copy or branding in a generic ICN asset.
- [ ] No real cooperative names, real members, real partners.
- [ ] Fictional entities and members read as examples (see bible §8).

## 8. Visual grammar

- [ ] The visual extends or references an existing primitive (`ClosureLoop`, `ScopeModel`, `ProvenanceTrail`, `MemberSurface`) where applicable, rather than inventing a parallel grammar.
- [ ] Iconography drawn from the canonical symbol family in [`website/src/data/icons.ts`](../../../website/src/data/icons.ts) and [`Icon.astro`](../../../website/src/components/Icon.astro). No parallel icon systems.
- [ ] Layout obeys the design language: civic, calm, legible, mobile-first.
- [ ] No anti-patterns from [`ICN_VISUAL_SYSTEM.md`](../ICN_VISUAL_SYSTEM.md) §7 (corporate SaaS sludge, crypto-bro futurism, network-globe nonsense, bureaucratic deadness, custodial-finance framing).
- [ ] Rejected-pattern self-check against [`ICN_VISUAL_EXPLAINER_BIBLE.md` Appendix B](../ICN_VISUAL_EXPLAINER_BIBLE.md#appendix-b--rejected-patterns-side-by-side-comparison):
  - [ ] **Pattern 1 — Glassmorphism + fintech identity:** no wallet / balance / fintech card framing
  - [ ] **Pattern 2 — "AI sheen" + decorative glow:** no gradient text, no glowing borders, no "powered by AI" chrome
  - [ ] **Pattern 3 — Glowing network-globe / central hub:** no globe, no hub-and-spoke, no random connecting lines
  - [ ] **Pattern 4 — Faux-foundation seal:** no heraldic crest, no fake-Latin, no invented age
  - [ ] **Pattern 5 — Color-only status:** every state has a text + icon companion (grayscale-pass)
  - [ ] **Pattern 6 — English-only fixed-width UI:** flex containers, longest-localized-form sized, RTL-tested
  - [ ] **Pattern 7 — Screenshots as proof:** no screenshot cited as implementation evidence; code / test / ADR link instead
  - [ ] **Pattern 8 — "Dashboard":** no "dashboard" / "admin panel" framing; surfaces named by what they do

## 9. Generated-image rules (where applicable)

- [ ] If any input was generated by an image model, the asset is treated as a **sketch** only.
- [ ] The sketch is not load-bearing. It is not on the public website. It is not in canonical docs. It is not in a deck shown outside the project.
- [ ] If the visual ships, it has been rebuilt as Astro / SVG / source asset per [bible §12](../ICN_VISUAL_EXPLAINER_BIBLE.md#12-production-source-rule).
- [ ] **Every glyph of generated text** has been verified against its source ([bible §11.1](../ICN_VISUAL_EXPLAINER_BIBLE.md#111-generated-text-inside-images-is-unreliable)). Hallucinated endpoints, garbled UI labels, invented numbers, plausible-but-wrong schema fields have been removed or replaced with verified strings in the source build.
- [ ] No fake UIs with plausible-looking real data.
- [ ] No fixture data that could plausibly be read as real member personal data.

## 10. Production-source rule

- [ ] If the visual is load-bearing — homepage, doc front-matter, public-facing diagram, deck hero, in-product surface — it ships as a source asset (Astro/SVG component or hand-authored SVG using design tokens).
- [ ] PNG-only is not used for load-bearing visuals.

## Sign-off

A visual is ready to ship when this checklist is **fully checked** and recorded against the brief.

A failing item is documented on the brief. Where the failure is structural (visual cannot reach the floor), the asset moves to `do-not-use` and a follow-up brief is opened.

## See also

- [docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md](../ICN_VISUAL_EXPLAINER_BIBLE.md)
- [docs/design/assets/ASSET_REGISTER.md](ASSET_REGISTER.md)
- [docs/design/ACCESSIBILITY_BASELINE.md](../ACCESSIBILITY_BASELINE.md)
- [docs/design/CONTENT_STYLE_GUIDE.md](../CONTENT_STYLE_GUIDE.md)
- [docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md](../ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md)
