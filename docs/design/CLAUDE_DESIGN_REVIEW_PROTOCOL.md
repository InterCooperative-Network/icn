---
Status: operational
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-19
Last Updated: 2026-05-19
Purpose: Define how Claude Design seed artifacts are reviewed before any piece of them is promoted into the ICN repo as canonical truth. Holds the canonical-vs-generated boundary.
---

# Claude Design Review Protocol

> **One sentence.** Claude Design generates seeds; the repo decides what becomes canonical, and most of a seed never does.

This protocol governs what happens between "Claude Design produced something" and "this is now ICN truth." It exists because polished output is the most dangerous shape of misinformation in a design system: a seed that looks like a final product invites everyone — including future agents — to treat it as one.

The handoff template ([CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](CLAUDE_DESIGN_HANDOFF_TEMPLATE.md)) is the shape of every review request. The hard rejection list ([MUST_NOT_SHIP.md](MUST_NOT_SHIP.md)) is the floor every artifact clears. This document is the process that connects them.

---

## 1. Seed status definitions

Every artifact from a Claude Design session falls into exactly one of these states. The status is declared by the human reviewer in [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md), not by the artifact itself.

| Status | What it is | What it is not |
|---|---|---|
| **generated seed** | Output of a Claude Design session — markdown, HTML, CSS, screenshots, prose, illustrations. Default state of everything coming out of a seed bundle. | Canonical. Implementation evidence. Brand. |
| **imported reference** | A copy of an existing ICN repo doc that the seed bundled for context. May be stale relative to the live repo. | Authoritative. The repo is. |
| **UI kit / illustrative mock** | A visual composition meant to communicate direction or tone. May depict capability that doesn't exist. | Production UI. A shipped surface. A promise. |
| **screenshot** | A rendered image of something — a prototype, an illustrative mock, an existing site. | Evidence the depicted thing is implemented, accessible, or live. |
| **candidate doctrine** | A piece of generated seed content that a reviewer has marked as worth promoting to canonical, pending the review gates in §3. | Promoted. Still seed-only until the gates close green. |
| **production implementation** | Code, content, or design that has been merged into the canonical repo paths after passing every gate. | Anything that exists only inside a seed bundle. |

**Default status of any artifact inside a seed: generated seed.** Promotion is an explicit, reviewed step.

---

## 2. Canonical-vs-generated boundary

The boundary is asymmetric and unconditional.

- **Canonical truth lives in the upstream ICN repo.** Specifically: anything declared canonical in [`docs/registry.toml`](../registry.toml) `[control].canonical_doc_paths`, plus the `Canonical: yes` rows in [`docs/INDEX.md`](../INDEX.md). Generated artifacts are never automatically canonical.
- **Generated Claude Design files are not canonical by default.** A polished seed produces files that look load-bearing. They are not.
- **Imported repo docs inside a seed may become stale** between the moment the seed was produced and the moment it is reviewed. Always re-read the live repo version before citing a doc the seed bundled.
- **Screenshots are not implementation evidence.** A screenshot can depict a future state, an illustrative mock, an existing site, or a hallucination, and the image will look the same regardless. Implementation is verified against code, not pixels.
- **UI kits are illustrative unless explicitly promoted.** A component shown in a kit is not a component shipped in the repo. Promotion requires a code change in the appropriate canonical path (`website/src/`, `sdk/`, or `web/`), not an import of the kit.
- **Polish is not evidence.** Visual finish, brand consistency, and prose quality of a seed say nothing about whether the content matches ICN doctrine, accessibility floor, vocabulary, or kernel truth.

**Rule of thumb:** if a reviewer has not signed off on a specific artifact via [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md), the artifact is seed-only.

---

## 3. Required review gates

A piece of generated seed content can only become canonical after passing every gate below. Gates are sequential; a failure at any gate sends the artifact back to seed-only status (or to discard).

### 3.1 Truth-label review

Each artifact must carry one of the truth labels from [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) and [ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md):

- `implemented / current UI` — exists in the canonical repo paths today
- `repo-grounded public explainer` — describes implemented or imminent behavior with linked evidence
- `repo-grounded architecture explainer` — describes the architecture, with kernel/app boundary respected
- `illustrative direction` — concept art; explicitly not implementation
- `future-state / roadmap` — depicts a target; tied to an issue or ADR
- `historical` — past state, kept for record
- `do not use` — flagged as unfit; remains for archive only

An artifact missing a truth label is not reviewable. Send it back.

### 3.2 Accessibility review

Against [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) and [docs/design-language/accessibility.md](../design-language/accessibility.md). The artifact must plan for:

- WCAG 2.2 Level AA minimum
- high-contrast light and dark themes (not just dark)
- `prefers-reduced-motion` honored
- large text / 200% zoom without horizontal scroll or content loss
- low-bandwidth mode (no autoplay, no heavy hero media, no decorative video)
- visible keyboard focus on every interactive element
- screen-reader semantics (landmarks, headings, `aria-*` where required)
- color is not the only carrier of meaning
- room for long translated labels without truncation
- right-to-left readiness
- a language selector path (even if not yet wired)
- glossary access from member-facing terminology

A seed that displays only dark-mode aesthetic perfection passes none of these by default. Verify each.

### 3.3 Vocabulary review

Against the regulatory-safe vocabulary in [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) §"Regulatory-safe vocabulary" and the canonical → public mapping in [docs/design-language/concept-map.md](../design-language/concept-map.md). Required swaps:

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

Any seed string using the right-hand vocabulary fails the gate. Generated seeds tend to import fintech and SaaS habits silently — search for them.

### 3.4 Implementation-status review

For every claim or depiction in the artifact, the reviewer answers: *does the ICN repo actually do this today?* If the answer is "no" or "not yet," the artifact must be relabeled as `illustrative direction` or `future-state / roadmap`. The artifact may not be promoted as `implemented / current UI` without a code citation.

This is where most seed content fails. Generated UI kits depict signed actions, live federation, real receipts, working compute pools — none of which the repo has shipped end-to-end in the surfaces shown.

### 3.5 Language / RTL readiness review

The artifact must:

- not be fixed-width English-only
- not assume Latin character widths in label sizing
- leave room for translated labels (40% expansion is a safe planning floor)
- not assume left-to-right reading order in flow direction
- support a glossary path for institutional terminology
- expose the three vocabulary layers when relevant: canonical term, public label, local institutional vocabulary

A seed that ships in one language with no translation seams is illustrative at best.

### 3.6 Low-bandwidth / reduced-motion / large-text review

- low-bandwidth: no autoplay video, no heavy hero animation, no required JavaScript for first paint of critical content
- reduced-motion: every motion behavior gated by `prefers-reduced-motion`
- large-text: layout reflows at 200% without content loss, no horizontal scroll on standard breakpoints

If the seed depends on motion for meaning, it fails this gate even if it would otherwise pass §3.2.

### 3.7 Source-doc drift review

Any seed-internal copy of an ICN repo doc is suspect by default. For each `imported reference` in the seed:

- diff the seed copy against the live repo version
- list every drift point in [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md) §"Known drift findings"
- treat the live repo version as authoritative
- if the seed introduces a change worth keeping, propose it as an upstream PR against the canonical path — do not promote the seed's drifted copy

Drift in the seed is normal. Promoting the drifted copy is the failure mode.

---

## 4. Promotion rule

> **No Claude Design artifact becomes canonical merely because it looks polished.**

A piece of generated seed content can be promoted only when:

1. It is labeled `candidate doctrine` in [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md).
2. Every applicable gate in §3 closes green.
3. A reviewer with design ownership signs off in writing.
4. The change is opened as a normal repo PR against the appropriate canonical path — not committed silently from inside the seed directory.
5. The PR cites this protocol, the seed version, and the truth label.

Promotion of a UI artifact (component, page, asset) additionally requires a code change in `website/src/`, `sdk/`, or `web/` — not just a docs reference. Importing a seed file into a canonical path is not promotion; it is a future-self trap.

---

## 5. Rejection rule

Any artifact that violates ICN truth boundaries, accessibility floor, vocabulary, or member-rights requirements remains seed-only or is discarded outright. There is no "we'll fix it later" path that ends in canonical promotion.

Specific rejection triggers (non-exhaustive — full list lives in [MUST_NOT_SHIP.md](MUST_NOT_SHIP.md)):

- placeholder logo presented as final brand
- member-app screenshots without an `illustrative direction` truth label
- action card without mandate / reversibility / receipt declared before confirm
- status conveyed by color alone
- English-only fixed-width UI
- crypto / wallet / token / balance / payment framing applied to ICN-native primitives
- generic SaaS dashboard patterns
- AI sheen / glassmorphism / glowing-globe network maps
- faux-government seals or fake-foundation marks
- screenshots cited as evidence the surface is implemented
- generated seed artifacts presented as canonical

A rejected artifact may remain in the seed directory for archive and learning. It may not be cited as ICN truth, used in member-facing material, or imported into canonical paths.

---

## 6. Reviewer's loop

When a new seed arrives:

1. Read the bundle's `seed/CLAUDE_CODE_BUNDLE.md`, `seed/INVENTORY.md`, `seed/PROMOTION_MAP.md`, `seed/DRIFT_REPORT.md`, `seed/PRODUCTION_READINESS.md`, and `seed/MUST_NOT_SHIP.md` before opening any prototype HTML or screenshot.
2. Open [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) and fill in a fresh copy for this seed.
3. Walk the gates in §3 against each named artifact. Record findings in [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md).
4. Update [`claude-design-seed/CHANGELOG.md`](claude-design-seed/CHANGELOG.md) with the seed version, key findings, and final status counts (how many `candidate doctrine`, how many `illustrative direction`, how many `do not use`).
5. For each `candidate doctrine`, open a small, scope-locked repo PR against the canonical path.
6. Leave the rest of the seed in its external bundle location (typically `$CLAUDE_JOB_DIR` or a separate scratch directory) — do **not** copy preview HTML, UI kits, generated assets, screenshots, or imported references into the repo. Only the governance trail — the seed's entry in [`claude-design-seed/CHANGELOG.md`](claude-design-seed/CHANGELOG.md) and its section in [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md) — lives in the canonical repo. The bundle URL plus the review trail is sufficient record.

---

## 7. What Claude Code is and is not asked to do

Claude Code working from a Claude Design seed is asked to:

- read the bundle's authoritative handoff docs
- apply this protocol to the seed's content
- promote `candidate doctrine` pieces via docs-only or scope-limited PRs
- update the changelog and review notes for the seed
- enforce the rejection rule and the hard list in [MUST_NOT_SHIP.md](MUST_NOT_SHIP.md)

Claude Code is **not** asked to:

- recreate generated prototypes pixel-perfectly
- migrate React prototype code into `website/src/` or `web/pilot-ui/`
- adopt placeholder logo, brand, or icon assets from a seed
- generate production CSS from seed tokens
- claim production readiness on member-facing surfaces depicted in a seed
- treat a polished seed as canonical truth

If a later task explicitly asks for production UI implementation against a promoted candidate, that is a separate PR with its own scope, its own review, and its own non-goals.

---

## 8. See also

- [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) — shape of every review request
- [MUST_NOT_SHIP.md](MUST_NOT_SHIP.md) — hard rejection floor
- [claude-design-seed/README.md](claude-design-seed/README.md) — what the seed directory is for and how it is organized
- [claude-design-seed/CHANGELOG.md](claude-design-seed/CHANGELOG.md) — imported seed versions
- [claude-design-seed/REVIEW_NOTES.md](claude-design-seed/REVIEW_NOTES.md) — per-seed human review summary
- [CLAUDE_DESIGN_CONTEXT.md](CLAUDE_DESIGN_CONTEXT.md) — paste briefing for external Claude Design sessions
- [CLAUDE_DESIGN_SETUP.md](CLAUDE_DESIGN_SETUP.md) — workflow, external and local modes
- [ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) — design system entry point
- [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) — truth labels and source hierarchy for visual material
- [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) — per-surface accessibility floor
- [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) — regulatory-safe vocabulary, dangerous-action copy
- [docs/design-language/concept-map.md](../design-language/concept-map.md) — canonical → public label mapping
- [docs/design-language/accessibility.md](../design-language/accessibility.md) — component-level accessibility rules
