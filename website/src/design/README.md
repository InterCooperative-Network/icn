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
