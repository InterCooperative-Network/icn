---
name: icn-commons-design
description: >
  ICN Commons design review and seed-promotion companion. Use when reviewing Claude Design
  seeds, drafting docs-only promotion PRs, auditing member-facing surfaces against ICN
  doctrine, or enforcing the canonical-vs-generated boundary. Provides review-gate routing
  (truth-label, accessibility, vocabulary, implementation-status, language/RTL,
  low-bandwidth/reduced-motion/large-text, source-doc drift), the must-not-ship floor,
  and a doctrine-bound critique pattern for action cards, receipts, standing views, and
  the member shell. Triggers on: "Claude Design seed", "design review", "member shell",
  "action card", "receipt strip", "standing view", "truth label", "accessibility audit",
  "vocabulary review", "design handoff", "promotion candidate", "canonical-vs-generated",
  "must not ship", "design system", "icn-commons", "design language", "design tokens".
version: 0.1.0
truth_contract:
  canonical_sources:
    - docs/design/ICN_DESIGN_SYSTEM.md             # design system entry point
    - docs/design/CLAUDE_DESIGN_CONTEXT.md         # paste briefing for external Claude Design
    - docs/design/CLAUDE_DESIGN_SETUP.md           # external + local mode workflow
    - docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md # seven review gates, canonical-vs-generated boundary
    - docs/design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md # seed → Claude Code handoff template
    - docs/design/MUST_NOT_SHIP.md                 # twelve-item rejection floor
    - docs/design/ACCESSIBILITY_BASELINE.md        # per-surface accessibility floor
    - docs/design/CONTENT_STYLE_GUIDE.md           # regulatory-safe vocabulary, dangerous-action copy
    - docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md    # truth labels, source hierarchy, rejected-patterns appendix
    - docs/design-language/brief-v0.md             # canonical design language brief
    - docs/design-language/concept-map.md          # canonical → public label mapping
    - docs/design-language/accessibility.md        # component-level accessibility rules
    - docs/mobile/icn-mobile-ux-spec-v1.md         # mobile member UX spec
    - docs/spec/member-shell-v0.md                 # member shell spec
    - docs/design/claude-design-seed/README.md     # seed workflow
    - docs/design/claude-design-seed/CHANGELOG.md  # imported seed versions
    - docs/design/claude-design-seed/REVIEW_NOTES.md # per-seed human review summary
  live_load_required:
    - "git branch --show-current"
    - "git status --short"
  examples_only: []
  never_hardcode:
    - seed version (always read from claude-design-seed/CHANGELOG.md)
    - canonical doc paths (always read from docs/INDEX.md and docs/registry.toml)
    - production code paths (website/src/, web/pilot-ui/, sdk/ — never imported from a seed)
    - logo decisions (placeholder marks remain rejected until logo direction is taken)
---

# ICN Commons Design Skill

Routing and review layer for ICN design work, including Claude Design seed review, member-surface critique, and promotion of seed artifacts to canonical repo paths.

This skill points at **canonical repo paths** only. Anything under `docs/design/claude-design-seed/` is governance trail, not source of truth. The seed bundle, when present, lives outside the repo (typically under a job scratch directory); it is consulted for review but not imported.

## What this skill helps with

- **Claude Design seed review.** Walking [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](../../../docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3 gates against a freshly-arrived seed. Filling in a fresh copy of [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](../../../docs/design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md). Recording findings in [REVIEW_NOTES.md](../../../docs/design/claude-design-seed/REVIEW_NOTES.md) and the [CHANGELOG.md](../../../docs/design/claude-design-seed/CHANGELOG.md).
- **Accessibility review.** Against [ACCESSIBILITY_BASELINE.md](../../../docs/design/ACCESSIBILITY_BASELINE.md) (per-surface floor) and [docs/design-language/accessibility.md](../../../docs/design-language/accessibility.md) (component-level rules). WCAG 2.2 AA, high-contrast light/dark, `prefers-reduced-motion`, 200% zoom, low-bandwidth, screen-reader semantics, color-not-alone, translated-label headroom, RTL readiness, glossary access.
- **Vocabulary review.** Against [CONTENT_STYLE_GUIDE.md](../../../docs/design/CONTENT_STYLE_GUIDE.md) §"Regulatory-safe vocabulary" and [docs/design-language/concept-map.md](../../../docs/design-language/concept-map.md). Enforce **member / standing / mandate / obligation / allocation / settlement / unit / position / receipt / provenance**; reject fintech and SaaS habits.
- **Truth-label review.** Against [ICN_VISUAL_EXPLAINER_BIBLE.md](../../../docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md) and [ADR-0033](../../../docs/adr/ADR-0033-public-maturity-claims-and-evidence-links.md). One of: `implemented / current UI`, `repo-grounded public explainer`, `repo-grounded architecture explainer`, `illustrative direction`, `future-state / roadmap`, `historical`, `do not use`.
- **Design handoff review.** Verifying a filled [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](../../../docs/design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) is complete before any promotion PR opens.
- **Member-shell / action-card / receipt critique.** Against [docs/spec/member-shell-v0.md](../../../docs/spec/member-shell-v0.md), [docs/mobile/icn-mobile-ux-spec-v1.md](../../../docs/mobile/icn-mobile-ux-spec-v1.md), and [ADR-0027 (Action Card Contract)](../../../docs/adr/ADR-0027-action-card-contract.md). Every action card declares mandate · reversibility · receipt before confirm. Every receipt is first-class UI.
- **Canonical-vs-generated boundary enforcement.** Refuses to treat seed artifacts as canonical, regardless of polish. Routes promotion candidates through scope-locked PRs against canonical paths — never bulk imports.

## Non-negotiables (do not break)

These are the rules a seed review or promotion PR must satisfy. They are reproduced here so the skill carries them; full statements live in the canonical docs above.

0. **A seed is not canonical.** Default status of every seed artifact is `generated seed`. Promotion requires the review gates in [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](../../../docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3 to close green.
1. **WCAG 2.2 Level AA is the floor.** Both themes must pass independently, in default + large-text modes. Contrast, keyboard, screen-reader, 44×44 tap targets, reduced motion, 200% zoom.
2. **Color never carries meaning alone.** Pair every color cue with text + icon. Grayscale-render the surface; if state distinction collapses, the surface fails.
3. **No payment / wallet / balance / currency / debt / user / dashboard / admin-panel** vocabulary on ICN-native primitives. See [CONTENT_STYLE_GUIDE.md](../../../docs/design/CONTENT_STYLE_GUIDE.md) §"Regulatory-safe vocabulary".
4. **Member-first.** The DID is the anchor; institutions are the changing context.
5. **Cooperatives, communities, federations are co-equal.** Never depicted as a hierarchy or a ladder.
6. **Receipts are first-class UI.** Every consequential action declares mandate · reversibility · receipt before confirm ([ADR-0027](../../../docs/adr/ADR-0027-action-card-contract.md)).
7. **Every visual asset carries a truth label.** Member-shell screenshots without an `illustrative direction` chip are rejected.
8. **No fake futurism.** No glassmorphism, no neon, no "AI sheen", no glowing-globe network maps, no central-hub diagrams, no faux-foundation seals, no SaaS dashboard chrome.
9. **Mobile-first, low-bandwidth.** Design target is a five-year-old phone on a 2 Mbps link, in a language the member chose.
10. **All motion gated by `prefers-reduced-motion`.** Motion never carries meaning.
11. **Placeholder logo stays placeholder.** Not for production, not for institution packages, not for marketing. Until logo direction is taken.
12. **No production UI / production CSS / SDK / prototype migration** from a seed without an explicit, separately-scoped follow-up PR. Docs-only promotion PRs are the first move.

Full rejection floor: [MUST_NOT_SHIP.md](../../../docs/design/MUST_NOT_SHIP.md).

## Operating loop for a new seed

When a Claude Design seed arrives:

1. **Read live state first** — `git branch --show-current`, `git status --short`. Confirm working in a docs-PR-appropriate branch (typically `docs/claude-design-seed-review-protocol` for the first seed, scope-specific for follow-ups).
2. **Read the bundle's authoritative handoff docs in order:** `seed/CLAUDE_CODE_BUNDLE.md`, `seed/INVENTORY.md`, `seed/PROMOTION_MAP.md`, `seed/DRIFT_REPORT.md`, `seed/PRODUCTION_READINESS.md`, `seed/MUST_NOT_SHIP.md`. Open preview HTML last, not first.
3. **Fill in [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](../../../docs/design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md)** for this seed. Enumerate allowed paths, forbidden paths, non-goals, promotion candidates, drift findings, unresolved decisions.
4. **Walk the review gates in [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](../../../docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3.** Record findings in [REVIEW_NOTES.md](../../../docs/design/claude-design-seed/REVIEW_NOTES.md).
5. **Append a row to [CHANGELOG.md](../../../docs/design/claude-design-seed/CHANGELOG.md).** Bundle URL, status, summary, promotion candidates, held items, drift, production-readiness caveats, human decisions.
6. **Open scope-locked promotion PRs** for each `candidate doctrine` row. One promotion per PR (or one tight cluster). First seed-promotion PR is the protocol/handoff/must-not-ship scaffold itself.

## Promotion routing

When a piece of seed content is a promotion candidate, route it to the right canonical path:

| Seed artifact type | Canonical destination |
|---|---|
| Review protocol / handoff template / rejection floor | `docs/design/` (this skill's anchors) |
| Token additions (modes, HC themes) | `website/src/styles/global.css` — **separate PR**, needs CSS review |
| Accessibility obligations (theme × mode coverage) | `docs/design/ACCESSIBILITY_BASELINE.md` — needs design owner |
| Vocabulary additions / pattern names | `docs/design-language/concept-map.md` — needs design-language owner |
| Truth-label scheme refinements | `docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md` — needs design owner |
| Member-shell pattern refinements | `docs/spec/member-shell-v0.md` and/or `docs/mobile/icn-mobile-ux-spec-v1.md` — needs mobile/spec owner |
| UI component implementations | `website/src/components/` (Astro) or `sdk/react-native/...` (RN) — **separate PR**, needs full member-surface checklist, never from seed kit recreations as-is |

A row in the seed's `PROMOTION_MAP.md` is a proposal. The decision to open a promotion PR is the reviewer's.

## What this skill will not do

- Recreate generated prototypes pixel-perfectly. The seed's preview HTML and UI kits are illustrative; production code is Astro / React Native, not React/JSX recreations.
- Migrate React prototype code into `website/src/` or `web/pilot-ui/`.
- Adopt the placeholder logo, wordmark, or icon assets from a seed.
- Generate production CSS from seed tokens in the seed-promotion PR.
- Claim a seed is canonical.
- Imply a member-facing surface is shipped when the seed depicts it as illustrative.

For a request that asks for production-UI implementation against a promoted candidate, that is a separate PR with its own scope and approval — surface the constraint and ask the user to confirm before proceeding.

## If invoked without other guidance

Ask the user:

1. **Mode** — seed review, accessibility audit, vocabulary review, truth-label audit, design handoff check, member-surface critique, or canonical-vs-generated boundary enforcement?
2. **Artifact** — bundle URL, file path, paste, or repo path?
3. **Truth state** — is the target shipped (`implemented / current UI`), grounded but not member-final (`repo-grounded public/architecture explainer`), illustrative (`illustrative direction`), or aspirational (`future-state / roadmap`)?
4. **PR scope** — docs-only seed promotion, member-surface implementation (separate track), or in-flight review?
5. **Allowed paths** — confirm before any write.

Then route to the relevant canonical docs above and apply the corresponding review gates.

## See also

- Specialist agent: [icn-design-advisor](../../../.claude/agents/icn-design-advisor.md)
- Mobile specialist: [icn-mobile-advisor](../../../.claude/agents/icn-mobile-advisor.md)
- Design rule auto-load: [`.claude/rules/design.md`](../../../.claude/rules/design.md)
- Per-surface accessibility gate: [docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md](../../../docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md)
