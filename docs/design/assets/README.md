---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-13
Last Updated: 2026-05-13
Purpose: Directory README for ICN visual asset planning — the asset register, the visual review checklist, and per-asset briefs.
---

# ICN Visual Assets — Planning Layer

This directory is the **planning layer** for ICN visual explainers. It does not contain production assets, generated images, or shipped figures. It contains:

- the **[asset register](ASSET_REGISTER.md)** — the live list of planned and tracked visual assets
- the **[visual review checklist](VISUAL_REVIEW_CHECKLIST.md)** — the gate every asset clears before it can ship
- per-asset **briefs** under [`briefs/`](briefs/) — one file per asset, named `VE-NNN-slug.md`

All three layers defer to the [ICN Visual Explainer Bible](../ICN_VISUAL_EXPLAINER_BIBLE.md) for doctrine. The bible governs; this directory plans and reviews.

## Where production assets live

| Class | Lives in | Notes |
|---|---|---|
| Astro / SVG components | [`website/src/components/`](../../../website/src/components/) | Tokenized, themable, grayscale-tested. Canonical form. |
| Optimized raster assets used by the website | [`website/src/assets/`](../../../website/src/assets/) | Goes through Astro's image pipeline. |
| SVG-in-docs figures | Inline in the doc that explains them | Hand-authored, design-token-grounded. |
| Mermaid diagrams | Inline in markdown docs | Structural diagrams only. |

This directory **does not host production assets**. Production assets live where the consumer expects them.

## Where sketches live

Generated-image sketches and exploration material live alongside the brief that motivated them, inside a `sketches/` folder under the brief. Example: `briefs/VE-005-kernel-app-separation/sketches/`. The folder is not registered. Sketches never enter `website/src/assets/`.

Per the [bible](../ICN_VISUAL_EXPLAINER_BIBLE.md) §11, sketches are not load-bearing and never become shipped assets.

## Lifecycle

1. **Idea** — captured as a row in [ASSET_REGISTER.md](ASSET_REGISTER.md). Status: `planned`.
2. **Brief** — written to `briefs/VE-NNN-slug.md`. Status: `briefed`. Brief gate (bible §10) closes the brief to generation until satisfied.
3. **Sketch** (optional) — exploration sketches live under the brief. Always labeled. Never load-bearing.
4. **Build** — Astro / SVG / source asset implemented in the consumer (website or docs). Status: `built`.
5. **Review** — runs the [VISUAL_REVIEW_CHECKLIST.md](VISUAL_REVIEW_CHECKLIST.md). Status moves to `shipped` only when the review passes.
6. **Decay** — when the substrate changes, the brief and the asset are re-reviewed. Status returns to `briefed` or moves to `historical` / `do not use`.

## What does not go here

- Brand kits for any institution package (NYCN, Summit, future packages).
- Member personal data, even as fixtures.
- Real partner / cooperative / member names.
- Generated images promoted to "final."
- Asset content for institution-specific surfaces — those live in the institution package's own repo.

## See also

- [docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md](../ICN_VISUAL_EXPLAINER_BIBLE.md) — doctrine
- [docs/design/ICN_DESIGN_SYSTEM.md](../ICN_DESIGN_SYSTEM.md) — design system entry point
- [docs/design/ICN_VISUAL_SYSTEM.md](../ICN_VISUAL_SYSTEM.md) — visual doctrine
- [docs/design/CONTENT_STYLE_GUIDE.md](../CONTENT_STYLE_GUIDE.md) — voice and vocabulary
- [docs/design/ACCESSIBILITY_BASELINE.md](../ACCESSIBILITY_BASELINE.md) — accessibility floor
