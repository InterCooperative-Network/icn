---
Status: operational
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-19
Last Updated: 2026-05-19
Purpose: Hard-stop list of design patterns and artifacts that must not ship in any production ICN surface. Each item is here because it has appeared or could plausibly appear in or near a Claude Design seed, and every recurrence in the future would be a regression.
---

# What Must Not Ship

> **One sentence.** Polish is not approval; these are the patterns and artifacts that disqualify a surface from production no matter how finished it looks.

This document is a floor, not a ceiling. The full design doctrine lives across [ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md), [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md), [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md), [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md), [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md), and the design-language docs under [`docs/design-language/`](../design-language/). This file is the short, hard rejection list every promotion has to clear before any other doctrine applies.

Adopted from the Claude Design seed's `seed/MUST_NOT_SHIP.md` (v0.1) and extended for repo-side promotion review. The seed's items are integrated, not preserved verbatim — the canonical repo wins on phrasing.

---

## 1 · The placeholder logo as final brand

The current "ICN Commons" three-circle wordmark — wherever it appears, in any seed, preview, screenshot, or imported asset — is a **placeholder**. It must not ship as the final brand mark, and it must not appear in any institution package (NYCN, Summit, future federations) as if it were settled identity.

**How to detect:** the mark appears anywhere without a visible "placeholder mark · not final" caption nearby. If the caption is missing, the mark is being asserted as final.

**What to do instead:** finish the logo exploration described in [ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) and the seed's README ("federated ring · interlocking civic forms · transit-map relation mark · receipt/provenance mark · cooperative weave · modular institutional glyph"), pick a direction, and replace the placeholder atomically across all surfaces. Until that decision is taken, the placeholder is rejected from any production surface.

---

## 2 · Member-app screenshots without an illustrative truth label

Member-shell screenshots are **illustrative direction** unless the underlying surface is implemented end-to-end in the canonical repo (`website/src/`, `sdk/`, or `web/pilot-ui/`). Every screenshot, exported PNG, or embedded mock of a member surface must carry a visible truth-label chip ([ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md), [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md)).

**How to detect:** any image of a phone or browser running the ICN member shell that does not show the truth-label strip near the top of the device viewport. Marketing material that crops the strip out is the most common offender.

**What to do instead:** export with the strip intact. If the strip is too visually heavy for a marketing context, the answer is to use a different asset — not to remove the label. The `MemberSurface` component upstream enforces this rule by design (`showTruthLabelChip=true`); do not silently override.

---

## 3 · Any action card without mandate, reversibility, and receipt

Per [ADR-0027 (Action Card Contract)](../adr/ADR-0027-action-card-contract.md): every action that produces a receipt must declare, before the confirm step, **the mandate that authorizes it**, **its reversibility**, and **the receipt it will produce**. A confirm button without these three is a regulatory and institutional liability — the surface is asserting authority the kernel cannot prove.

**How to detect:** any Confirm / Submit / Send / Vote button that does not sit below a labeled "Mandate" / "Reversibility" / "Produces" row.

**What to do instead:** use the action-card pattern documented in the design system. If a flow truly has no mandate, no reversibility statement, or no receipt — it is not an ICN action, it is a preference setting. Use a different pattern.

---

## 4 · Any status conveyed by color alone

Color reinforces; it never carries. Every color-coded state must pair with a text or icon companion that names the state. Grayscale-rendered, the surface must remain fully legible.

**How to detect:** open the surface, drop saturation to 0%, and check whether every state remains distinguishable. If state distinction collapses, the surface is failing this rule.

**Common offenders:** "your standing is green" without text · red-rimmed cards that don't say "rejected" · accent-only sync states without "saved" / "sent" / "confirmed" labels · maturity bands distinguished only by accent bar color.

**What to do instead:** every accent pairs with a text label and a concept icon. Reference: [docs/design-language/accessibility.md](../design-language/accessibility.md) §3.1.

---

## 5 · English-only fixed-width UI

ICN is global infrastructure. A surface that only renders English, that breaks at long-string locales (German, Finnish), or that fixed-pixels its containers so non-English content gets clipped, is not member-ready.

**How to detect:** switch the surface to Spanish and German; switch to Arabic with `dir="rtl"`; verify every label fits, every chip wraps, every action card holds its layout. If any of these fails, the surface is English-only by construction.

**What to do instead:**

- Every text container is flexible-width with `min-width: 0` and `text-wrap` appropriate to the content.
- Every label is authored with the longest plausible localized form in mind (~1.5× English string length as the planning floor).
- The language selector is explicit and member-chosen, never IP-detected.
- Formal records always carry the canonical original alongside any translation.
- Flow direction respects `dir="rtl"` end-to-end (chrome, controls, icons).

---

## 6 · Crypto, wallet, token, balance, or payment framing on ICN-native primitives

ICN-native primitives are obligations, allocations, settlements, units, positions, and receipts. Importing fintech or crypto vocabulary onto them is a regulatory liability and a category error.

**How to detect:** grep member-facing strings for `wallet | balance | currency | crypto | token | coin | payment | payout | transfer | transaction fee | debt | IOU`. Any match outside an explicit "Reject" example is a violation.

**What to do instead:** use the canonical vocabulary in [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) §"Regulatory-safe vocabulary" and [docs/design-language/concept-map.md](../design-language/concept-map.md):

- **member**, not user / account holder
- **standing**, not reputation / rank / account level
- **mandate / authority**, not permission / admin power
- **obligation**, not debt / IOU / balance owed
- **allocation**, not payment plan / payout
- **settlement**, not payment / transfer / payout / transaction
- **unit**, not currency / token / coin
- **position**, not balance / wallet / account
- **receipt**, not confirmation / transaction record
- **provenance**, not log / audit log (when member-facing)

---

## 7 · Generic SaaS dashboard / admin-console patterns

ICN does not have a dashboard. The member shell is not a dashboard, the operator surface is not a dashboard, the federation view is not a dashboard. "Dashboard" implies metrics-over-meaning, optimization-over-accountability, and platform-over-institution.

**How to detect:** the word "dashboard" or "admin panel" anywhere in member-facing copy. Stacked metric tiles for their own sake. A "stats" view that exists without answering a specific institutional question. KPI tiles, gauge widgets, and analytics chrome borrowed from product-analytics SaaS.

**What to do instead:** name surfaces by what they do. **Standing view** — "who you are, where you are recognized." **Action queue** — "what needs you now." **Activity** — "what happened, with proof." **Position** — "what you have contributed and what you can draw on."

---

## 8 · "AI sheen" / glassmorphism

No glowing borders, no shimmer effects, no `✨` decorations, no gradient text on copy that isn't the brand name, no animated particles, no "powered by AI" chrome, no frosted-glass cards layered over neon gradients, no chromatic-aberration accents.

**How to detect:** any surface that asserts intelligence (smartness, magic, power) through visual treatment rather than through what it tells the member.

**What to do instead:** the institution is not "smart" — the member is. Surface copy that helps the member decide. Skip the chrome. ICN looks like public infrastructure, not a startup deck.

---

## 9 · Glowing-globe / central-hub network maps

A glowing globe with arcs is not a network diagram; it is a venture-pitch motif. A central hub with spokes is not federation; it is hub-and-spoke architecture. Both encode the wrong story about ICN.

**How to detect:** any diagram that uses a sphere, a globe, or a single central node radiating to peripheral nodes. Animated arcs traveling along great-circle routes. World maps with pulsing dots.

**What to do instead:** depict ICN's actual topology — peers in named scopes, federations of co-equal entities, provenance threads between participants. The "persistent member across institutional contexts" motif from the design language is the right primitive. Reference: [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md).

---

## 10 · Faux-government / faux-foundation seals

No round seals with laurel branches, no eagles, no shield motifs, no "Department of ___" chrome, no fake watermarks asserting officialdom. ICN is institutional, not governmental.

**How to detect:** circular emblems flanked by laurels, classical-seal compositions, embossed-foundation marks, fake watermarks on member-facing documents.

**What to do instead:** the institution package's own brand mark (NYCN, Summit, the cooperative federation that issued the receipt) is what carries authority on a document, not borrowed state iconography. ICN substrate is unbranded by default.

---

## 11 · Screenshots cited as implementation evidence

A screenshot proves nothing about what is implemented. It depicts what was rendered into pixels at some moment — possibly from a live deploy, possibly from a mock, possibly hallucinated.

**How to detect:** a PR description, marketing page, or roadmap doc that links a screenshot as the proof a feature is shipped. A "current state" claim backed by an image and no code link.

**What to do instead:** cite the code path, the test, the ADR, the commit. If a screenshot accompanies the citation, it is an illustration, not the evidence. Per [ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md), public maturity claims require linked evidence — and an image is not evidence.

---

## 12 · Generated seed artifacts presented as canonical

A Claude Design seed produces polished output. Polished output is not canonical truth. Anything under `docs/design/claude-design-seed/` is seed-only by default; anything inside a delivered bundle is seed-only by default; anything that looks like a final design system but has not been promoted via [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3 is seed-only.

**How to detect:** a seed artifact (HTML preview card, UI kit composition, generated CSS, imported screenshot) cited or referenced from a canonical doc, an ADR, a press surface, or a member-facing page. A seed file linked as if it were the source of a doctrine.

**What to do instead:** promote the underlying *idea* through a normal repo PR against the appropriate canonical path. The seed file stays where it is — under `docs/design/claude-design-seed/` — as record. Citations point at the promoted canonical doc, not back at the seed.

---

## How to enforce this list

- **At review time:** [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3 includes the truth-label, accessibility, vocabulary, implementation-status, language/RTL, low-bandwidth/reduced-motion/large-text, and source-doc drift gates. This file is the rejection floor those gates default to.
- **At PR time:** the design rule in [`.claude/rules/design.md`](../../.claude/rules/design.md) auto-loads doctrine when design paths are touched. Reviewers cite item numbers from this file when blocking a PR.
- **At marketing/comms time:** anyone preparing a public asset of ICN checks it against this list before publishing. The seed's `MUST_NOT_SHIP.md` was originally a self-check for the seed author; the repo-side version is a self-check for every contributor.

A surface that violates any of the twelve items is not shipped. There is no "we'll fix it later" path that ends in canonical promotion of a violating surface.

---

## See also

- [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) — gates that run before promotion
- [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) — handoff shape for seed → Claude Code
- [ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) — design system entry point
- [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) — truth-label scheme, source hierarchy, rejected-patterns appendix
- [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) — per-surface accessibility floor
- [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) — regulatory-safe vocabulary, dangerous-action copy
- [docs/design-language/concept-map.md](../design-language/concept-map.md) — canonical → public label mapping
- [docs/design-language/accessibility.md](../design-language/accessibility.md) — component-level accessibility rules
- [ADR-0027 — Action Card Contract](../adr/ADR-0027-action-card-contract.md)
- [ADR-0032 — Website Truth Boundary](../adr/ADR-0032-website-truth-boundary.md)
- [ADR-0033 — Public Maturity Claims and Evidence Links](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md)
