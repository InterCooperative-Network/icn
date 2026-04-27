---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-04-26
Last Updated: 2026-04-26
Purpose: Ready-to-paste context for Claude Design or any external collaborator generating ICN-side design work. Keep brief; this file is meant to be copied into a tool's context window verbatim.
---

# Claude Design — ICN Context

This file exists to be pasted into Claude Design (or a similar tool) whenever someone produces ICN-side design work. Keep it short. Link to richer docs only after the paste.

---

## Company / design system name

**InterCooperative Network / ICN Commons Design System**

## Blurb

ICN is infrastructure for democratic institutions: governance, membership, federation, resource coordination, receipts, provenance, CCL-defined institutional rules, and member-facing participation surfaces. The visual system should feel trustworthy, civic, legible, cooperative, technical without being cold, and accessible on low-end devices.

## Primary surfaces

Public website, documentation, member shell, action cards, standing views, receipt and provenance views, governance flows, federation and operator views, compute and commons views.

## Design priorities

Accessibility, clarity, institutional seriousness, mobile-first participation, low-bandwidth use, plain-language explanation, visible provenance. Avoid: crypto-bro aesthetics, corporate SaaS sludge, fintech dashboard styling, gamified social patterns, neon futurism, or "AI sheen" without meaning.

## Hard constraints

- **WCAG 2.2 AA is the floor** — color contrast, keyboard navigation, screen reader semantics, large tap targets, reduced motion, low-bandwidth mode, plain language. Detail in [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md).
- **No payment/wallet/balance/currency vocabulary for ICN-native primitives.** Use: obligation, allocation, settlement, unit, position, settlement asset, external settlement instruction, bridge receipt. See [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md).
- **No NYCN, "New York Cooperative Network", "Summit", or "reference federation" language on the public ICN website.** ICN is generic substrate; NYCN is a separate institution package in a separate private repo.
- **Maturity-band honesty.** Public claims must match implementation reality. See [ADR-0032 — Website Truth Boundary](../adr/ADR-0032-website-truth-boundary.md).
- **Receipts are first-class UI.** Any action shown must have a path to its provenance. See [ADR-0026 — Receipt and Provenance Proof Envelope](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md).

## Brand position (compact)

- **Yes**: civic, trustworthy, cooperative, calm, durable, technical.
- **No**: crypto, SaaS dashboard, fintech, govtech-portal, hacker-terminal, glassmorphism, neon.

## When in doubt

Read the canonical files in this order:

1. [docs/design-language/brief-v0.md](../design-language/brief-v0.md) — design language
2. [docs/design/ICN_VISUAL_SYSTEM.md](ICN_VISUAL_SYSTEM.md) — visual doctrine
3. [docs/design/ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) — accessibility floor
4. [docs/design/CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) — voice, vocabulary, dangerous-action copy
