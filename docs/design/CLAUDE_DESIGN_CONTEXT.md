---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-17
Last Updated: 2026-05-17
Purpose: Self-contained ICN design context for paste into Claude Design (or any external collaborator). Designed to fit a single context window with the four canonical docs attached.
---

# Claude Design — ICN Context

Paste this file at the start of any Claude Design session that produces ICN-side design
work. It contains everything Claude needs to make decisions without re-asking the user.
Detail lives in the linked docs; this file is the briefing.

---

## 1. What ICN is

InterCooperative Network (ICN) is **P2P infrastructure for democratic institutions**.
Not a blockchain. Not a SaaS platform. Not a fintech product. Not a crypto wallet. It is
a kernel that enforces constraints, plus a small set of apps that translate institutional
meaning into those constraints. Cooperatives, communities, and federations run it on
their own hardware to prove their decisions, own their records, and coordinate without
a landlord.

If a design choice would imply ICN is any of the things in the "Not" list, the design
choice is wrong.

---

## 2. Brand position

**Yes**: civic, trustworthy, cooperative, calm, durable, technical, accessible,
serious without being dead.

**No**: crypto-bro, SaaS dashboard, fintech, govtech-portal, hacker-terminal,
glassmorphism, neon, gamified, surveillance-heavy, mystical, "AI sheen."

ICN should feel like **public infrastructure for collective self-rule**.

---

## 3. Hard constraints (non-negotiable)

| # | Constraint | Source of truth |
|---|------------|-----------------|
| 1 | **WCAG 2.2 Level AA is the floor.** Contrast, keyboard, screen reader, 44×44 tap targets, reduced motion, 200% zoom, no color-only meaning. | [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) |
| 2 | **No payment / wallet / balance / currency / token vocabulary for ICN-native primitives.** Use the vocabulary in §6 below. | [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) |
| 3 | **No NYCN, "New York Cooperative Network," "Summit," or "reference federation" language on the public ICN website.** ICN is generic substrate; NYCN/Summit are separate institution packages. | [ADR-0032](../adr/ADR-0032-website-truth-boundary.md) |
| 4 | **Maturity-band honesty.** Do not depict capabilities, flows, or polish that don't exist in the implementation. Concept art is allowed, but must be labeled as such. | [ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md), [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) |
| 5 | **Receipts are first-class UI.** Every action that creates an obligation, casts a vote, settles, or rotates a key must declare its mandate, reversibility, and the receipt it will produce — before confirm. | [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md), [ADR-0027](../adr/ADR-0027-action-card-contract.md) |
| 6 | **Member-first, not institution-first.** The person persists across institutional contexts; the institution is the changing context, not the anchor. | [ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) §"Design principles" |
| 7 | **Scope coequality.** Cooperatives, communities, and federations are co-equal forms with different jobs — never a ladder, never a hierarchy. | [ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) |
| 8 | **Mobile-first, low-bandwidth.** Design target: a 5-year-old phone on a 2 Mbps link, in three languages the member chose, when the member is tired and skeptical. | [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) §"Bandwidth and device" |

---

## 4. Design tokens (current website implementation)

These are the live tokens from `website/src/styles/global.css`. Never invent new color hex
values; use these. Both themes pass WCAG AA independently.

### 4.1 Color

```
                           Dark (default)     Light
Backgrounds
  --bg-primary             #06090f            #ffffff
  --bg-secondary           #0c1118            #f8fafc
  --bg-card                #111827            #ffffff
  --bg-surface             #0f1620            #f1f5f9

Text
  --text-primary           #e8edf5            #0f172a
  --text-heading           #f1f5f9            #020617
  --text-secondary         #a3b0c2            #475569
  --text-muted             #8b99ad            #64748b

Accents (one per closure-loop concept group)
  --accent-teal            #2dd4bf            #0d9488   (identity, standing, authority)
  --accent-amber           #fb923c            #c2410c   (governance, policy, execution, member experience)
  --accent-blue            #38bdf8            #0369a1   (accounting)
  --accent-green           #4ade80            #15803d   (provenance, receipts)
  --accent-purple          #a78bfa            #7c3aed   (reserved)
  --accent-rose            #fb7185            #be123c   (reserved, dangerous action)
```

Color is never the only carrier of meaning. Every color-coded state has a text or icon
companion.

### 4.2 Typography

- **Body**: Inter, system-ui fallback.
- **Headings**: Outfit, Inter fallback.
- **Mono / labels / receipts**: JetBrains Mono, Fira Code fallback.
- Base size 16px. Body line-height 1.6. Headings use `clamp()` for fluid scale.

### 4.3 Spacing scale

```
--space-xs   0.25rem      4px
--space-sm   0.5rem       8px
--space-md   1rem         16px   ← default gap
--space-lg   1.5rem       24px
--space-xl   2rem         32px
--space-2xl  3rem         48px
--space-3xl  4rem         64px
--space-4xl  6rem         96px
```

### 4.4 Radii

```
--radius-sm   0.375rem
--radius-md   0.5rem
--radius-lg   0.75rem    ← default for cards
--radius-xl   1rem
--radius-2xl  1.5rem
```

### 4.5 Motion

```
--duration-fast   150ms
--duration-normal 250ms   ← default
--duration-slow   400ms
--ease-out        cubic-bezier(0.16, 1, 0.3, 1)
```

All motion is gated by `@media (prefers-reduced-motion: reduce)` — designs must work
without it.

---

## 5. Concept vocabulary (the words to use)

These are the 25 canonical concepts. Public-facing surfaces use the **Public** label by
default; technical surfaces may use the **Canonical** label paired with the Public label
on first use. Full localization notes in [concept-map.md](../design-language/concept-map.md).

| Canonical | Public (English default) | One-line meaning |
|-----------|--------------------------|------------------|
| `identity` | *Who you are* | Self-held cryptographic anchor; not a platform account |
| `standing` | *Your recognized status* | Provable participation in a scope |
| `authority` | *What you are allowed to do* | Action-space granted by scope rules |
| `governance` | *How group decisions are made* | Proposals, deliberation, decisions |
| `policy` | *The rules this follows* | Binding rules the institution has adopted |
| `accounting` | *How resources and obligations are tracked* | Relational social accounting (not commercial finance) |
| `execution` | *What actually happens* | Where decisions become operational fact |
| `provenance` | *History and proof* | Verifiable chain from decision to outcome |
| `member_experience` | *What people can do and see* | The surface a person interacts with |
| `scope` | *Where you are acting* | A recognized institutional context |
| `cooperative` | *A cooperative* | Member-owned, member-governed institution |
| `community` | *A community* | Civic institution with shared rules and purpose |
| `federation` | *How groups work together* | Voluntary coordinating scope; never centralized |
| `commons` | *Shared resources* | Governed shared resources |
| `agreement` | *A formal relationship* | Recorded understanding between scopes/members |
| `allocation` | *What has been assigned* | A decision to direct resources or authority |
| `obligation` | *What is owed or required* | A structured claim one party has on another |
| `settlement` | *How accounts are resolved* | Discharging an obligation per its terms |
| `attestation` | *A verified claim* | A signed statement another party can rely on |
| `role` | *Your function in this context* | Named bundle of authority and responsibility |
| `membership` | *Your place in the group* | Recognized participation in a scope |
| `resource` | *Something shared, used, or governed* | An asset the institution tracks |
| `mandate` | (no public label needed — used on Action Cards) | The capability that authorizes an action |
| `receipt` | *A record of what happened* | Provenance-bearing record of an act |
| `unit` | (used with a number) | Generic accounting unit (avoid "currency," "token," "coin") |

### Words to avoid for ICN-native primitives

| Avoid | Use instead | Why |
|-------|-------------|-----|
| payment, transfer, payout | settlement | "Payment" implies commercial finance; settlement is generic |
| balance, wallet, account | position | "Wallet" implies custody; position is value-neutral |
| debt, IOU, balance owed | obligation | "Debt" loads commercial connotations |
| currency, token, coin | unit | "Currency" implies money; unit is generic |
| permission, role-grant | mandate, authority | "Permission" sounds platform-y; authority is institutional |
| confirmation, transaction | receipt | "Transaction" implies commerce |
| user, account holder | member | "User" implies platform; member implies institution |
| reputation, rank | standing | "Reputation" is social; standing is institutional |

External systems may be named for what they are (a bank transfer is a bank transfer). The
rule is: ICN-native primitives use ICN-native vocabulary.

---

## 6. Surface inventory (what exists, what doesn't)

| Surface | State | Implementation | Notes |
|---------|-------|----------------|-------|
| Public website | Live | Astro 5 + TypeScript at `website/` | Deployed to intercooperative.network |
| Documentation | Live | Astro pages reading `docs/*.md` at build time | Same site as website |
| Member shell | Spec only | — | Anchored to gateway API in [icn-mobile-ux-spec-v1.md](../mobile/icn-mobile-ux-spec-v1.md) |
| Standing view | Spec only | — | Source: `GET /v1/gov/me/standing` |
| Action cards | Spec only | — | Source: `GET /v1/gov/me/action-cards`; contract: [ADR-0027](../adr/ADR-0027-action-card-contract.md) |
| Receipts / provenance | Spec only | — | Progressive disclosure mandatory; export and share are first-class |
| Governance flows | Spec only | — | Proposal → deliberation → threshold → decision → receipt |
| Mobile app (CoopWallet) | Prototype | React Native at `sdk/react-native/examples/CoopWallet/` | 5-tab navigation per spec |
| Pilot UI | Legacy | Vanilla HTML/CSS at `web/pilot-ui/` | Does **not** inherit the design system; do not redesign without explicit ask |

### Visual primitives — built vs not built

**Built** (use these): institutional card (`.inst-card`), closure-loop station marker
(`ClosureLoop`), eyebrow label (`.eyebrow`), icon component (`Icon.astro` with 10 icons
covering the closure loop), maturity bands with text + color, skip link, focus-visible
ring, reduced-motion handling.

**Not yet built** (design opportunities): scope container, agreement connector,
provenance trail / receipt strip, truth-state band as reusable component, resource /
commons field, federation relation line, role / authority badge, action block, system
divider, section frame, icons for the 15 non-closure-loop concepts.

---

## 7. Where this design system fits the kernel

This is load-bearing. ICN is a kernel + apps. Design choices encode kernel invariants at
the human layer. Full mapping in
[`ICN_DESIGN_SYSTEM.md`](ICN_DESIGN_SYSTEM.md) §"Kernel architecture binding" and
[`DESIGN_PRINCIPLES.md`](../DESIGN_PRINCIPLES.md).

Key bindings a designer must respect:

- **"Make the invisible visible"** ↔ the kernel emits a receipt for every state
  transition. If the kernel cannot prove it, the UI must not assert it.
- **"Plain language is required"** ↔ governance changes must read as plain power
  deltas, never as jargon.
- **"Receipts are first-class UI"** ↔ the opaque receipt cascade exists so every
  action has provenance. Bury a receipt and the institution has not actually happened.
- **"Scope coequality"** ↔ cooperatives, communities, and federations are co-equal
  kernel entities. Never depict them as nested or as a ladder.
- **"Mandate before confirm"** ↔ every action card declares the capability that backs
  it before the member can confirm.

If a design change would make the UI assert something the kernel cannot prove (or
silently leak app-layer semantics into a kernel surface), it is a tier-2 firewall
violation expressed at the human layer.

---

## 8. Output expectations

Tell Claude Design what kind of artifact you want. Common ones:

| Job | What to ask for | Example prompt |
|-----|-----------------|----------------|
| Visual design | A static composition (PNG/SVG/HTML mock). Specify surface, breakpoint, theme. | "Design the Action Card detail view for mobile, dark theme, using the tokens above." |
| Component spec | Variants, states, props, accessibility notes. Markdown table format. | "Spec the `provenance trail` component: variants, states, ARIA, code snippet." |
| Copy review | Pass/flag/rewrite each string against §5 and the content style guide. | "Review this copy against the regulatory-safe vocabulary table." |
| Accessibility audit | WCAG 2.2 AA pass/fail per criterion, with fixes. | "Audit this layout against WCAG 2.2 AA. Report findings and fixes." |
| Design critique | Specific, prioritized feedback against the 10 design principles. | "Critique this mockup against ICN_DESIGN_SYSTEM.md design principles." |
| Concept exploration | Multiple directions for an unbuilt primitive (§6). Label clearly as concept art. | "Three concept directions for the federation relation line. Label as concept, not implementation." |

Default deliverable: **a single artifact + a one-paragraph rationale + a flagged list of
assumptions or unknowns**. No multi-page narrative.

---

## 9. Reading order (when the briefing isn't enough)

In order, only as deep as the job requires:

1. [docs/design/ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) — entry point, design principles, surface obligations, **kernel architecture binding**
2. [docs/design-language/brief-v0.md](../design-language/brief-v0.md) — canonical design language, visual primitives, icon system rules
3. [docs/design/ICN_VISUAL_SYSTEM.md](ICN_VISUAL_SYSTEM.md) — stable visual doctrine, anti-patterns, vocabulary guidance
4. [docs/design-language/accessibility.md](../design-language/accessibility.md) — component-level WCAG rules, do/don't examples
5. [docs/design/ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) — per-surface accessibility floor
6. [docs/design/CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) — voice, dangerous-action copy template, error patterns
7. [docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md](ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) — 12-category PR-time review checklist
8. [docs/mobile/icn-mobile-ux-spec-v1.md](../mobile/icn-mobile-ux-spec-v1.md) — mobile member UX spec (5-tab navigation)
9. [docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) — truth labels, brief gate, generated-image workflow (for diagrams and infographics specifically)

---

## 10. Setup and workflow

For how to set Claude Design up and which docs to attach for which job, see
[CLAUDE_DESIGN_SETUP.md](CLAUDE_DESIGN_SETUP.md).
