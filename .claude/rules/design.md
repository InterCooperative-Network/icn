---
paths:
  - "docs/design/**"
  - "docs/design-language/**"
  - "docs/mobile/**"
  - "website/src/**"
  - "web/**"
  - "sdk/react-native/**"
---

# ICN Design Rules

Design work on ICN spans the visual system, design language, accessibility floor, content style, and mobile UX. Doctrine entry point: [docs/design/ICN_DESIGN_SYSTEM.md](../../docs/design/ICN_DESIGN_SYSTEM.md).

## Required reading (load by job)

- **Any design change** → [ICN_DESIGN_SYSTEM.md](../../docs/design/ICN_DESIGN_SYSTEM.md) — design principles (10), surface obligations (9), **§"Kernel architecture binding"**
- **Visual / token / color / typography** → [brief-v0.md](../../docs/design-language/brief-v0.md), [ICN_VISUAL_SYSTEM.md](../../docs/design/ICN_VISUAL_SYSTEM.md), `website/src/styles/global.css`
- **Accessibility** → [ACCESSIBILITY_BASELINE.md](../../docs/design/ACCESSIBILITY_BASELINE.md) (floor), [accessibility.md](../../docs/design-language/accessibility.md) (component rules), [ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md](../../docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) (PR gate)
- **Copy / vocabulary** → [CONTENT_STYLE_GUIDE.md](../../docs/design/CONTENT_STYLE_GUIDE.md), [concept-map.md](../../docs/design-language/concept-map.md)
- **Mobile** → [icn-mobile-ux-spec-v1.md](../../docs/mobile/icn-mobile-ux-spec-v1.md)
- **Claude Design session** → [CLAUDE_DESIGN_CONTEXT.md](../../docs/design/CLAUDE_DESIGN_CONTEXT.md) (briefing), [CLAUDE_DESIGN_SETUP.md](../../docs/design/CLAUDE_DESIGN_SETUP.md) (workflow)

## Hard constraints

- **WCAG 2.2 Level AA is the floor.** Contrast, keyboard, screen reader, 44×44 tap targets, reduced motion, 200% zoom. Color is never the only carrier of meaning.
- **No payment / wallet / balance / currency / token / transaction vocabulary** for ICN-native primitives. Use: obligation, allocation, settlement, unit, position, mandate, receipt, standing. See CONTENT_STYLE_GUIDE.md §"Regulatory-safe vocabulary".
- **No NYCN / Summit / "reference federation" language** on anything that isn't an institution package. ICN is generic substrate.
- **Maturity-band honesty.** Do not depict capability that doesn't exist in the implementation (ADR-0032, ADR-0033).
- **Receipts are first-class UI.** Every dangerous action declares mandate + reversibility + receipt before confirm (ADR-0027).
- **Member-first, scope-coequal.** The DID is the anchor; institutions are changing contexts. Coops/communities/federations are co-equal — never a ladder.
- **Use existing tokens.** No new hardcoded color hex values. Tokens live in `website/src/styles/global.css` (snapshot in CLAUDE_DESIGN_CONTEXT.md §4).

## Kernel binding (load-bearing)

Design choices express kernel invariants at the human layer. Full mapping in [ICN_DESIGN_SYSTEM.md §"Kernel architecture binding"](../../docs/design/ICN_DESIGN_SYSTEM.md) and [DESIGN_PRINCIPLES.md](../../docs/DESIGN_PRINCIPLES.md).

If a UI change would assert something the kernel can't prove, or silently leak app-layer semantics into a kernel surface, it is a tier-2 firewall violation expressed at the human layer.

## Pre-ship flag list

Before declaring a deliverable done:

- [ ] No asserted capability the kernel can't prove (ADR-0032 / ADR-0033)
- [ ] No forbidden vocabulary from CONTENT_STYLE_GUIDE.md §"Regulatory-safe vocabulary"
- [ ] No NYCN / Summit references outside institution packages
- [ ] Grayscale check passes (color is not the only carrier of meaning)
- [ ] Tap targets ≥ 44×44 CSS pixels
- [ ] Motion gated by `prefers-reduced-motion`
- [ ] Dangerous actions declare mandate + reversibility + receipt before confirm
- [ ] No new hardcoded color hex values outside the token list
- [ ] Per-surface accessibility floor from ACCESSIBILITY_BASELINE.md met
- [ ] If organizer/member-facing: ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md 12-category checklist filed in PR body

## Specialist routing

For deep design work, dispatch to [icn-design-advisor](../agents/icn-design-advisor.md). For mobile-specific work, also dispatch to [icn-mobile-advisor](../agents/icn-mobile-advisor.md). For accessibility audits on member surfaces, reference the 12-category gate in ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md.
