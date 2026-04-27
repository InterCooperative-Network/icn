---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-04-26
Last Updated: 2026-04-26
Purpose: The accessibility floor every ICN member-facing surface must meet. Anchors ADR-0028 (proposed) at the design layer; pairs with docs/design-language/accessibility.md for component-level patterns.
---

# ICN Accessibility Baseline

> **One sentence.** Accessibility is not a polish phase. It is a precondition for democratic legitimacy.

This document is the floor. A surface that does not meet this floor is not shippable, regardless of how complete its functionality is.

For component-level patterns, copy, and detailed guidance, see [docs/design-language/accessibility.md](../design-language/accessibility.md). For the binding decision, see [ADR-0028 — Accessibility Baseline for Member Interfaces](../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md).

## Standards anchor

- **WCAG 2.2 Level AA** is the minimum for every member-facing surface.
- ICN-specific requirements (receipts, provenance, scope, dangerous actions) extend WCAG; they do not replace it.

## Per-surface obligations

| Surface | Hard requirements |
|---------|-------------------|
| Public website | WCAG 2.2 AA. No motion that auto-starts. Skip-to-content link. Keyboard-navigable. Glossary linked from any unfamiliar term. |
| Documentation | Same as website. Plain-language summary at the top of every doc. Code blocks have language tags. Headings are properly nested. |
| Member shell | Above plus: identity-aware focus order; never trap keyboard focus inside a modal without an Escape; never require fine motor precision for a routine action. |
| Standing view | Above plus: full content readable without color cues; non-color status indicators; "challenge" path always reachable from this view. |
| Action cards | Above plus: confirm step requires explicit input (no default-yes); mandate and receipt named in plain language before confirm. |
| Receipts/provenance | Above plus: progressive disclosure (summary → explanation → raw); export and share are first-class buttons. |
| Governance flows | Above plus: vote actions are reversible up until threshold; deliberation thread is keyboard-navigable; quorum and threshold are stated in plain language. |
| Federation/operator | Above plus: operators see no more provenance than members on records that affect members; admin actions confirm with named scope. |

## Cross-cutting requirements

### Color and contrast
- Text and meaningful graphics meet WCAG AA contrast (4.5:1 for body text, 3:1 for large text and UI components).
- Status is never color-only — paired with text, icon, or position.
- The system does not assume light or dark theme; both must pass contrast independently.

### Keyboard
- Every interactive element reachable by keyboard.
- Visible focus indicator on every focusable element. The indicator is not "thin gray underline"; it is unambiguous.
- No keyboard trap. Escape always exits a modal or popover.
- Tab order matches reading order.

### Screen reader semantics
- Real HTML elements over generic `<div>` with role attributes.
- Form fields have programmatically associated labels.
- Live regions announce state changes (e.g., "Vote recorded", "Receipt issued").
- Receipts and provenance carry a description that screen readers can read coherently — no walls of opaque hashes without a label.

### Tap targets
- Minimum 44×44 CSS pixels for any interactive element on touch surfaces.
- Adjacent tap targets do not crowd; minimum 8px between actionable hit areas.

### Motion
- Reduced-motion preference (`prefers-reduced-motion`) is honored.
- No motion that auto-plays without a user action on first load.
- No parallax that affects readability.

### Bandwidth and device
- Public pages render usefully on a 2 Mbps connection on a five-year-old phone.
- Critical paths do not require JavaScript to convey content; JS adds, never gates.
- Assets are sized for member devices, not designer monitors.

### Plain language
- The first paragraph of any complex page is plain language.
- Glossary entries are linked inline, not buried at the bottom.
- A member can read what they are about to do without expert help.

### Multilingual
- The member chooses language; the surface does not auto-detect from IP.
- Translation covers the summary layer; the formal record layer remains in its canonical form with a link.

### Dangerous-action confirmation
See [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md). Summary: every dangerous action must declare what it does, the mandate that backs it, reversibility, and the receipt to be produced — before confirm. Confirm is explicit input, not a default-yes button.

### Receipts and provenance
- Member can export a receipt as a downloadable file.
- Member can share a receipt without exposing private fields by default.
- Receipts have human-readable summaries; the canonical record is reachable but not the only view.

## What this baseline is not

- Not a substitute for testing with real members on real devices.
- Not a waiver. "Accessibility blocked by upstream component" is a bug to fix, not a permanent state.
- Not exhaustive. Component-level guidance lives in [docs/design-language/accessibility.md](../design-language/accessibility.md).

## Verification

A surface is considered to have met the baseline when:

1. Automated checks (axe / Pa11y / equivalent) report zero AA violations on the surface.
2. Keyboard-only walkthrough completes the primary task.
3. Screen-reader walkthrough (NVDA, VoiceOver) completes the primary task.
4. Reduced-motion and high-contrast modes do not break the layout.
5. The surface renders usefully on a 2 Mbps link on a low-end device.

ADR-0028 will define the binding verification protocol once accepted.

## See also

- [ADR-0028 — Accessibility Baseline for Member Interfaces](../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md)
- [docs/design-language/accessibility.md](../design-language/accessibility.md)
- [docs/design/CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md)
- [docs/design/ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md)
