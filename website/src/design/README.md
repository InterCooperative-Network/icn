# website/src/design/

Pointer from the website source tree to the ICN Commons Design System.

The design system itself does not live in the website. It lives in the repo's `docs/design/` and `docs/design-language/` directories so that institution packages, member shell, documentation, and demo materials can all share one source.

## Where to read

- [docs/design/ICN_DESIGN_SYSTEM.md](../../../docs/design/ICN_DESIGN_SYSTEM.md) — top-level entry point
- [docs/design/ICN_VISUAL_SYSTEM.md](../../../docs/design/ICN_VISUAL_SYSTEM.md) — visual doctrine
- [docs/design-language/brief-v0.md](../../../docs/design-language/brief-v0.md) — design language
- [docs/design/ACCESSIBILITY_BASELINE.md](../../../docs/design/ACCESSIBILITY_BASELINE.md) — accessibility floor
- [docs/design/CONTENT_STYLE_GUIDE.md](../../../docs/design/CONTENT_STYLE_GUIDE.md) — voice and vocabulary
- [docs/design/CLAUDE_DESIGN_CONTEXT.md](../../../docs/design/CLAUDE_DESIGN_CONTEXT.md) — paste-ready collaborator context

### Visual explainers and assets

Before building or revising any visual on the site — a diagram, an infographic, a homepage figure, a new Astro/SVG component, a generated-image sketch — read the visual explainer control plane:

- [docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md](../../../docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md) — doctrine (source hierarchy, truth labels, vocabulary rules, accessibility floor, brief gate, generated-image workflow, production-source rule)
- [docs/design/assets/ASSET_REGISTER.md](../../../docs/design/assets/ASSET_REGISTER.md) — live register of planned and tracked visual assets
- [docs/design/assets/VISUAL_REVIEW_CHECKLIST.md](../../../docs/design/assets/VISUAL_REVIEW_CHECKLIST.md) — pre-ship review
- [docs/design/assets/briefs/](../../../docs/design/assets/briefs/) — per-asset briefs

The canonical Astro primitives that ground the bible — `ClosureLoop`, `ScopeModel`, `ProvenanceTrail`, `MemberSurface` — live in [`src/components/`](../components/). New explainers should extend or reference them rather than invent a parallel grammar.

### Source-controlled visual explainers

- **`HowIcnWorksExplainer.astro`** ([`src/components/HowIcnWorksExplainer.astro`](../components/HowIcnWorksExplainer.astro)) — first source asset for **VE-001** (How ICN Works / Closure Loop). Composes the canonical `ClosureLoop` primitive with a visible truth-label chip, the brief's one-sentence message, and a substrate-honesty footer. See [`docs/design/assets/briefs/VE-001-how-icn-works-closure-loop.md`](../../../docs/design/assets/briefs/VE-001-how-icn-works-closure-loop.md) and the [asset register](../../../docs/design/assets/ASSET_REGISTER.md) for status.

## What goes in this directory

Currently nothing. This directory exists as a documented anchor for future work:

- design tokens, if extracted to a shared module
- per-surface component contracts, if formalized
- per-surface design notes that are too website-specific for the cross-repo design docs

When something lands here, it must defer to `docs/design/*` for principles, not redefine them.

## What does not go in this directory

- Brand kits for any specific institution package. Institution packages live in their own repos.
- References to specific institution packages or events. ICN is generic substrate; package-specific design lives in the package's own repo.
- Member personal data, even as fixtures.
