# ICN Accessibility Rules

**Status:** v0 — canonical accessibility requirements for the ICN design language.
**Parent doc:** [brief-v0.md](./brief-v0.md)
**WCAG target:** WCAG 2.2 Level AA minimum, AAA where practical.

---

## 1. Why this matters

ICN is institutional infrastructure for collective self-rule. Accessibility is not polish. It is part of institutional legitimacy.

If the system is only legible to people with good eyesight, fast bandwidth, native English, high literacy, big screens, and insider vocabulary, then it is not infrastructure for collective self-rule. It is infrastructure for a class of insiders.

This document defines the rules every public surface must meet.

---

## 2. Five layers of accessibility

### 2.1 Sensory

- Strong contrast
- Scalable typography
- Touch-friendly controls
- Reduced motion where possible
- Non-color-only meaning
- Readable diagrams
- Adequate spacing
- Screen-reader-compatible semantics

### 2.2 Cognitive

- Plain language first
- Stable layout patterns
- Chunked information
- Limited jargon in public-facing contexts
- Consistent labels
- Visible next steps
- Predictable status indicators

### 2.3 Linguistic

- Avoid idioms
- Avoid unexplained abstractions
- Use short labels
- Use helper text
- Support translation and local overrides cleanly
- Assume the reader's first language may not be English

### 2.4 Institutional

- Users should not need preexisting expertise in law, cooperative governance, cryptography, or systems theory to orient themselves.
- Formal terms are allowed but must be paired with plain-language explanations on first use.

### 2.5 Social

- The system should feel learnable without insider status.
- No implied hierarchy of technical sophistication between users.

---

## 3. Core rules (WCAG-grounded)

### 3.1 Color is never the only cue

**Rule:** Meaning must never depend on color alone. Every color-coded meaning must also be conveyed by text, icon, pattern, position, or structure.

**WCAG:** 1.4.1 Use of Color (Level A)

**Examples:**
- ✅ Maturity bands: the band carries a text label ("Strong today", "Advancing now") *and* a color. A colorblind reader loses the color cue but retains full meaning.
- ✅ ClosureLoop station 09: uses an amber gradient to signal convergence *and* spans both columns of the grid *and* is the last numbered station in the sequence.
- ❌ Six `.inst-card` variants distinguished only by accent bar color, with identical eyebrow structure. Fix: each variant also carries its category in the eyebrow text.

**Enforcement:** when reviewing a design, run a grayscale preview. If two things are now indistinguishable, meaning is leaking into color.

### 3.2 Contrast ratios meet WCAG AA

**Rule:**
- Normal text (under 18pt regular / 14pt bold): **4.5:1** minimum
- Large text (18pt regular / 14pt bold and above): **3:1** minimum
- Interactive components and meaningful graphics: **3:1** minimum against adjacent colors

**WCAG:** 1.4.3 Contrast (Minimum) (Level AA), 1.4.11 Non-text Contrast (Level AA)

**Current tokens to verify:**
- `--text-primary` (`#e8edf5`) on `--bg-primary` (`#06090f`) — ~17:1, passes AAA
- `--text-secondary` (`#94a3b8`) on `--bg-primary` — ~7.1:1, passes AAA
- `--text-muted` (`#64748b`) on `--bg-primary` — ~4.7:1, passes AA
- `--text-muted` on `--bg-card` (`#111827`) — ~3.7:1, **fails AA for small text**

**Action:** any muted-on-card text must be at least 14pt/bold to qualify as large text, or the token pair must be swapped. Eyebrow labels and station-number pills sit in this zone and need review.

### 3.3 Focus states are always visible

**Rule:** Every interactive element must show a visible focus indicator when reached by keyboard. The indicator must be distinguishable from the default state.

**WCAG:** 2.4.7 Focus Visible (Level AA), 2.4.11 Focus Not Obscured (Level AA)

**Current state:** the site has no `:focus-visible` rules. Keyboard users cannot see where they are as they tab through.

**Required fix:** add a global `:focus-visible` rule with a visible ring (outline or box-shadow), applied to links, buttons, form controls, and interactive cards.

### 3.4 Motion respects user preferences

**Rule:** Any non-essential motion (parallax, auto-animations, reveal-on-scroll, hover transitions over 200ms) must be disabled when the user prefers reduced motion.

**WCAG:** 2.3.3 Animation from Interactions (Level AAA)

**Current state:** the `.reveal` IntersectionObserver fade-up animation in `Layout.astro` ignores reduced-motion preferences.

**Required fix:** wrap motion rules in `@media (prefers-reduced-motion: no-preference)` or add a `@media (prefers-reduced-motion: reduce)` override that sets `transition: none` and `animation: none`.

### 3.5 Keyboard navigation works everywhere

**Rule:** Every page must be fully usable with a keyboard alone. No interaction should require a mouse or touch.

**WCAG:** 2.1.1 Keyboard (Level A), 2.1.2 No Keyboard Trap (Level A)

**Specific checks:**
- Tab order follows visual order
- The mobile menu opens with keyboard (Enter/Space on the hamburger button)
- All CTAs are reachable
- `:focus-visible` shows the current element
- No keyboard traps in modals or menus
- A skip-to-main-content link is the first tabbable element on every page

### 3.6 Screen reader semantics are correct

**Rule:** Interactive state, landmark regions, and meaningful structure must be exposed to assistive technology.

**WCAG:** 4.1.2 Name, Role, Value (Level A), 1.3.1 Info and Relationships (Level A)

**Specific requirements:**
- The `<main>` element wraps page content (exists)
- The hamburger button has `aria-expanded` reflecting menu state
- Decorative SVGs use `aria-hidden="true"`
- Meaningful SVGs use `role="img"` and `aria-label`
- Headings form a logical outline (h1 → h2 → h3, no skips)
- `aria-current="page"` on the active nav link
- Diagram components (`ClosureLoop`, future `ScopeMap`) expose their structure via `aria-label` or inline `<figcaption>`

### 3.7 Touch targets are big enough

**Rule:** Interactive elements must be at least **44×44 CSS pixels** on mobile, or spaced so adjacent touch targets do not overlap.

**WCAG:** 2.5.5 Target Size (Enhanced) (Level AAA), 2.5.8 Target Size (Minimum) (Level AA)

**Current state:** mobile nav links, hero CTAs, and footer links are large enough. Some secondary ghost buttons and the station-number pills may be borderline. Audit required.

### 3.8 Text can scale without breaking

**Rule:** The page must remain usable when text is zoomed to 200% without horizontal scrolling or content loss.

**WCAG:** 1.4.4 Resize Text (Level AA), 1.4.10 Reflow (Level AA)

**Current state:** headings use `clamp()`, body text uses rem. Should be fine but untested.

### 3.9 Small text is not smaller than necessary

**Rule:** Default body text should be at least 16 CSS pixels. Mono labels and eyebrows should be at least 11 CSS pixels and ideally 12+.

**Current state:** `.station-number` uses `0.6875rem` (~11px) and `.eyebrow` uses the same. At the lower bound of the rule.

**Action:** keep as-is for now but verify with real users. Do not go smaller.

### 3.10 Alt text is meaningful or absent

**Rule:**
- Decorative images: `alt=""` and/or `aria-hidden="true"`
- Meaningful images: alt text that conveys the same information the image provides
- SVG icons used as UI affordances: `aria-label` describing the action
- SVG icons paired with visible text labels: `aria-hidden="true"`

### 3.11 Icons follow the Icon component rules

**Rule:** All canonical ICN icons flow through `website/src/components/Icon.astro`, which enforces:
- `focusable="false"` always (icons never trap tab focus)
- `aria-hidden="true"` when no `label` prop is provided (decorative, paired with text)
- `role="img"` + `aria-label` when a `label` prop is provided (standalone, meaningful)
- `stroke="currentColor"` so the icon inherits color from its parent rather than baking it in

When integrating an icon:
- ✅ Pass no `label` if the icon sits next to visible text that already names it. The `Icon.astro` component will mark it decorative.
- ✅ Pass `label="Identity"` if the icon is standalone and must be named by the screen reader.
- ✅ Run the grayscale test (`filter: grayscale(1)` on `<html>`) after adding a new icon integration. Every card and every labeled context must remain distinguishable.
- ❌ Never bake color into icon path `stroke` or `fill` attributes. Use `currentColor` and set color on the parent.
- ❌ Never use an emoji as a substitute for an icon in a canonical component. Emojis do not have stable silhouettes across fonts and platforms.

See `docs/design-language/brief-v0.md §7a` for the full icon system rules.

---

## 4. Concrete do-and-don't examples

### Color and meaning

- ✅ Status card: `<div class="status-card status-strong">` → green left border + `<div class="status-label">Strong today</div>` text label inside. Both channels.
- ❌ Status card: `<div class="status-strong">` → green background only. No label. Relies on color.

### Typography and size

- ✅ Hero headline: `font-size: clamp(1.75rem, 7vw, 2.5rem)` → scales with viewport.
- ❌ Hero headline: `font-size: 48px` → fixed, breaks mobile and screen-reader zoom.

### Interactive state

- ✅ Button: `:hover` + `:focus-visible` share the same treatment, so mouse and keyboard users both see it.
- ❌ Button: only `:hover` styled. Keyboard users see nothing.

### Language

- ✅ Public page copy: "Every decision produces a receipt that can be traced back to the rule that authorized it."
- ❌ Public page copy: "The governance dispatch bridge wires payloads into executor slots via the trust policy oracle." (Accurate, but unreadable without expertise. Belongs on `/for-developers`.)

### Diagrams

- ✅ ClosureLoop: numbered stations with both canonical label and description, visible labels at every breakpoint, `aria-label` on the figure.
- ❌ NetworkGraph (homepage hero): decorative force-directed SVG with floating text labels, no description, no semantic structure.

---

## 5. Current known failures

These are the accessibility failures identified in the latest assessment. Each should be addressed in the accessibility foundation pass.

1. **No `:focus-visible` styles anywhere.** Keyboard users cannot see where they are. Fails 2.4.7.
2. **No skip-to-main-content link.** Screen reader users tab through the full nav on every page load. Fails 2.4.1.
3. **Hamburger menu lacks `aria-expanded`.** Screen reader users cannot tell if the menu is open. Fails 4.1.2.
4. **No `prefers-reduced-motion` handling.** The `.reveal` animation and theme toggle ignore user preferences. Fails 2.3.3.
5. **Decorative SVGs not marked `aria-hidden`.** The nav logo, footer logo, and several button icons are exposed to screen readers as meaningless SVG content.
6. **`NetworkGraph` has no accessible description.** Decorative but rendered into the tree without `aria-hidden`.
7. **`--text-muted` on `--bg-card`** passes large-text AA but fails small-text AA. Audit required for `.eyebrow` and `.station-number` uses.
8. **No `aria-current` on nav links.** The active nav item is styled but not announced as current.

---

## 6. Review checklist

Before any public-facing PR ships, run this checklist:

- [ ] Every color-coded meaning has a non-color cue
- [ ] All interactive elements have `:focus-visible` styles
- [ ] Motion respects `prefers-reduced-motion`
- [ ] Touch targets are ≥44px on mobile
- [ ] Headings form a logical outline
- [ ] New SVGs are marked `aria-hidden="true"` (decorative) or have `role="img"` + `aria-label` (meaningful)
- [ ] New interactive elements have accessible names (button text, `aria-label`, or `aria-labelledby`)
- [ ] Text can scale to 200% without breaking layout
- [ ] New mono or eyebrow text is ≥11px and audited against its background
- [ ] Keyboard can reach and operate every interactive element
- [ ] No keyboard traps
- [ ] The page passes a grayscale preview (no meaning lost)

---

## 7. Tools

**Manual:**
- Browser devtools Accessibility panel (axe-core)
- Keyboard-only navigation test
- Screen reader test (VoiceOver / NVDA / Orca)
- Grayscale preview (`filter: grayscale(1)` on `html`)
- 200% zoom test

**Automated:**
- `axe-core` via Playwright (recommended for CI)
- Lighthouse accessibility audit
- WCAG contrast checkers (WebAIM)

---

## 8. Related artifacts

- [brief-v0.md](./brief-v0.md) — the design language brief
- [concept-map.md](./concept-map.md) — canonical → public term mapping
- [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md) — upstream architecture-review reflex; the participation-access half of that checklist sits one layer above the surface-level rules in this doc
- `website/src/layouts/Layout.astro` — site shell, nav, skip link, focus handling
- `website/src/styles/global.css` — token definitions, focus-visible rules, motion handling
