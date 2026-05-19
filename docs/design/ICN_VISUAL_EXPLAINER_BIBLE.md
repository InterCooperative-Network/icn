---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-13
Last Updated: 2026-05-13
Purpose: Control-plane doctrine for every ICN visual explainer — diagrams, infographics, generated images, Astro/SVG components, and concept art. Defines what may be depicted, how it must be sourced, how truth is labeled, and how generated images are gated before becoming production assets.
---

# ICN Visual Explainer Bible — v0

> **One sentence.** Visuals are part of the institutional interface; they must be source-grounded, truth-labeled, accessibility-floored, and aligned with the [ICN Commons Design System](ICN_DESIGN_SYSTEM.md) — or they do not ship.

This bible is the control plane for the visual explainer system. It is read before any new explainer is drafted and before any generated image is treated as a candidate asset. It does not produce visuals; it governs how visuals are produced.

## 1. Purpose

ICN keeps growing surfaces that try to explain it — homepage diagrams, doc figures, deck slides, social cards, talk material, member-shell concepts. Without a control plane:

- visuals drift away from what the substrate actually does
- vocabulary slides into payment/wallet/token framing
- accessibility becomes a polish phase
- generated images become quoted "as if real" without becoming source assets
- institution-specific (NYCN, Summit) material leaks into generic ICN explainers
- briefs lose their connection to the [current truth map](../reference/project-index/current-truth-map.md) and the [show-readiness map](../reference/project-index/show-readiness-map.md)

The bible exists to keep every visual explainer aligned to:

1. **The substrate's actual state** — what exists in main today.
2. **The design language** — the [brief](../design-language/brief-v0.md), [concept map](../design-language/concept-map.md), [visual system](ICN_VISUAL_SYSTEM.md), [accessibility baseline](ACCESSIBILITY_BASELINE.md), and [content style guide](CONTENT_STYLE_GUIDE.md).
3. **Regulatory framing** — ICN is digital public infrastructure, not a payment system, not a token economy, not a wallet app.
4. **The truth labels** — every visual carries a label declaring what kind of claim it is making.

## 2. Source hierarchy

When a visual makes a claim, the claim is checked against this order (highest wins):

1. **Per-PR truth and current state** — [`docs/STATE.md`](../STATE.md), [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md), and the gateway / app source itself (`icn-gateway`, `icn/apps/governance/src/http/`, the relevant crate code).
2. **Project-index truth maps** — [`docs/reference/project-index/current-truth-map.md`](../reference/project-index/current-truth-map.md), [`docs/reference/project-index/show-readiness-map.md`](../reference/project-index/show-readiness-map.md), and [`docs/reference/project-index/runtime-surface-map.md`](../reference/project-index/runtime-surface-map.md).
3. **Architecture canon** — [`docs/architecture/THE_COMMONS.md`](../architecture/THE_COMMONS.md), [`docs/architecture/KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md), [`docs/architecture/MEMBER_STANDING.md`](../architecture/MEMBER_STANDING.md), [`docs/genesis.md`](../genesis.md), and the rest of `docs/architecture/`.
4. **Design canon** — this bible plus the documents listed in §1 point 2.
5. **Existing implementation surfaces** — the Astro primitives `ClosureLoop`, `ScopeModel`, `ProvenanceTrail`, and `MemberSurface`. A new explainer must not contradict an already-implemented primitive; it either extends it or replaces it with an explicit migration.
6. **Strategy / planning material** — used only for direction, never as a claim of present capability.

A visual that asserts something at level 4 or below while level 1–3 says otherwise is wrong, regardless of how clean the visual is.

## 3. Truth labels

Every visual explainer carries one truth label. The label is required in the brief and on the asset itself (figure caption, alt text, or visible chip) wherever it lands.

These labels are written for the **visual-explainer use case** — they describe what kind of claim a *visual* is making about ICN, not what `truth_class` a doc carries in `registry.toml`. The two systems overlap but are not the same; the registry labels concern document-level provenance, the labels below concern asset-level claim type.

| Label | Meaning | When to use |
|---|---|---|
| `implemented / current UI` | Depicts a surface that exists in `main` and is exercised today — a real gateway response, a real Astro component, a real CLI output. | A screenshot of `ClosureLoop.astro`. A diagram of `/me/standing` shaped to its actual response. The K3s proof-loop runbook visualized as it actually runs. |
| `repo-grounded public explainer` | An explainer of the ICN model, grounded directly in repo material (canonical docs, source code, runtime surfaces). May abstract for clarity, but never contradict the source. | The nine-station closure loop as a concept diagram. The five-scope model. The decision-to-receipt provenance flow as a public-facing story. |
| `repo-grounded architecture explainer` | An explainer for a developer or architect audience, grounded in architecture docs and crate code. Reads at a different abstraction level than the public explainer. | The kernel/app meaning-firewall diagram. PolicyOracle request flow. Per-crate responsibilities. |
| `illustrative direction` | Concept art for a surface, flow, or member experience that is not yet shipped end-to-end. Communicates intent and grammar; capabilities beneath may be real, but the unified surface is not. | `MemberSurface` content. Action-card anatomy at member-app scale. Steward / operator cockpit concepts. |
| `future-state / roadmap` | Depicts a capability the project is moving toward but has not built yet. Implementation does not exist; spec or direction does. | Live multi-coop federation. One-command per-coop deployment. Charter customization workflow as a member-facing surface. Commons compute economy at federation scale. |
| `historical` | Snapshot from an earlier phase, kept for trajectory, archive, or teaching. Not the current state. | Earlier closure-loop drafts. Pre-Phase-1 architecture diagrams. Old member-shell sketches kept for comparison. |
| `do not use` | The visual is recognized as wrong, misleading, or against doctrine, and is preserved only as a counter-example or to mark something removed. Never embedded, never shipped, never used as a starting point without an explicit migration brief. | Crypto / token / wallet-styled drafts. Glowing-globe network maps. Old material that violates §5 or §11. |

Labels are **not** marketing copy. Do not soften, blur, or skip them. If a visual sits between two labels, it carries the **less optimistic** one.

A visual without a label is **not shippable**.

> **See also:** [Appendix A](#appendix-a--truth-label-band-seven-canonical-states) formalizes the seven labels as a renderable band, with visual treatment notes and the mandatory carry-through rule (a version that hides its chip is rejected).

## 4. Visual doctrine (compact)

The [visual system](ICN_VISUAL_SYSTEM.md) and the [brief](../design-language/brief-v0.md) are the long-form doctrine. The bible reuses those rules in compact form:

1. **Make the invisible visible.** Authority, scope, mandates, and receipts are legible without expert knowledge.
2. **Member-first, not institution-first.** The person persists; the institution is the changing context.
3. **Scope coequality.** Cooperatives, communities, and federations are co-equal forms — never a hierarchy, never a ladder.
4. **Proof must be understandable.** Provenance reveals through progressive disclosure.
5. **Civic seriousness without bureaucratic deadness.** Calm, capable, trustworthy.
6. **Mobile-first and low-bandwidth.** Five-year-old phone on a slow link is the design target.
7. **No fake futurism.** No glassmorphism, neon, AI sheen, glowing network globes, fake coins.
8. **Plain language is required.** Jargon links to a glossary entry.
9. **Receipts are first-class UI.** Always exportable, always shareable, never buried.
10. **Reduced motion and large tap targets are defaults.** Accessibility is the floor.

A visual that visibly violates any of these is rejected at review, not revised in-place.

## 5. Vocabulary rules (binding)

ICN-native primitives use ICN-native vocabulary. The [content style guide](CONTENT_STYLE_GUIDE.md) is the long-form rule; here is the short form for visuals:

| Use | Do not use for ICN primitives |
|---|---|
| identity | account, login |
| standing | member status, account level, rank, reputation |
| mandate / authority | permission (UI-only), admin role |
| obligation | debt, IOU, balance owed |
| allocation | budget line, payment plan |
| settlement | payment, transfer, payout, transaction |
| unit | currency, token, coin, money |
| position | balance, wallet, account, holdings |
| receipt | confirmation, transaction record |
| provenance | log, audit log, history |

Hard prohibitions on ICN-native visuals:

- **no crypto / web3 / token / wallet / payment / balance / currency framing**
- **no coin imagery, no wallet icons, no exchange chrome**
- **no "transaction success" affordances styled like fintech apps**
- **no "stake," "mint," "burn," "swap"**

External systems may be named for what they are (bank transfer, ACH, currency) when the visual is specifically describing the external system. The boundary is: ICN-native primitives use ICN-native vocabulary; external systems can be named as themselves.

## 6. Accessibility floor

Every visual explainer must, before being treated as a candidate asset:

- **Carry meaning in structure + label + icon silhouette, not color alone.** A grayscale pass must remain readable.
- **Meet WCAG 2.2 AA contrast for text and meaningful graphics** (4.5:1 body, 3:1 large text / UI). Both dark and light theme variants.
- **Be readable at small sizes.** Icons designed for 16–22px must remain silhouette-distinct at 16px.
- **Carry alt text or `aria-label` describing the claim, not the chrome.** A screen reader user must learn the same thing a sighted reader does.
- **Avoid auto-playing motion.** Reduced-motion preference is honored. No motion as the sole carrier of state change.
- **Respect tap-target minimums** (44×44 CSS px) for any interactive surface depicted at near-real scale.
- **Not require expert vocabulary** in the first reading layer. Glossary-linked terms are fine; unlinked jargon is not.
- **Render usefully on a 2 Mbps connection on a low-end device.** No 4MB PNG hero images, no hero videos.

Accessibility is a release blocker, not polish. A non-accessible visual is treated as not done.

## 7. Core ICN explainer models

These are the canonical models. Every visual explainer either belongs to one of them or proposes a new model under §10 (brief gate).

### 7.1 The closure loop (nine stations)

`identity → standing → authority → governance → policy → accounting → execution → receipts/provenance → member experience`, with the loop closing back into the next round of standing.

- **Canonical surface:** [`website/src/components/ClosureLoop.astro`](../../website/src/components/ClosureLoop.astro)
- **Concept anchor:** [`concept-map.md`](../design-language/concept-map.md) — Closure-loop stations
- **Truth label:** `repo-grounded public explainer` for the model. A screenshot or rendering of the live Astro component is `implemented / current UI`. Per-surface capabilities depicted by the model carry their own labels (see §3).

### 7.2 The scope model (five scopes)

`commons → federation → community → cooperative → self`. Five scopes of action, co-equal, not a hierarchy. A single action can touch several scopes at once. The receipt it produces is tagged with every scope it passed through.

- **Canonical surface:** [`website/src/components/ScopeModel.astro`](../../website/src/components/ScopeModel.astro)
- **Concept anchor:** [`concept-map.md`](../design-language/concept-map.md) — Scope concepts
- **Truth label:** `repo-grounded public explainer` for the model. Concrete scope surfaces (e.g. live federation between two coops) carry their own labels and may be `future-state / roadmap`.

### 7.3 The provenance trail (decision → receipt)

`proposal → decision → execution → effect → receipt`. The chain a decision walks to become a durable institutional fact.

- **Canonical surface:** [`website/src/components/ProvenanceTrail.astro`](../../website/src/components/ProvenanceTrail.astro)
- **Live anchors:** `/v1/gov/me/action-cards` (cards), `/v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt` (receipt retrieval), three currently emitted source paths.
- **Truth label:** `repo-grounded public explainer` for the model. A visual that depicts the *three currently emitted* source paths is `implemented / current UI`. A visual that depicts all five source paths is `future-state / roadmap` until the two RFC-gated paths land.

### 7.4 The kernel/app separation

The meaning firewall: apps translate domain semantics into generic constraints; the kernel enforces constraints without understanding meaning.

- **Canonical doc:** [`docs/architecture/KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md)
- **Truth label:** `repo-grounded architecture explainer`. Specific oracles that are not yet implemented carry `future-state / roadmap` where shown alongside live ones.

### 7.5 The member surface (illustrative)

The unified member-facing surface — identity, standing, position, governance, receipts in one coherent view.

- **Canonical surface:** [`website/src/components/MemberSurface.astro`](../../website/src/components/MemberSurface.astro)
- **Truth label:** `illustrative direction`. The capabilities beneath each panel are real; the unified product surface tying them together is not yet shipped.

### 7.6 Federation without centralization

Distinct institutions coordinating through shared infrastructure without disappearing into one sovereign center.

- **Canonical anchors:** [`docs/architecture/FEDERATION_INTEROP_CONTRACT.md`](../architecture/FEDERATION_INTEROP_CONTRACT.md), `icn-federation` crate.
- **Truth label:** `repo-grounded architecture explainer` for the primitives that exist in `icn-federation`; `future-state / roadmap` for any depiction of two real cooperatives federating in production. **No live-federation claim** — see show-readiness map.

### 7.7 Commons and compute

Trust-gated distributed task execution and shared resources governed by participating institutions.

- **Canonical anchors:** [`docs/design/compute-substrate-design.md`](compute-substrate-design.md), [`docs/design/COMMONS_EVOLUTION.md`](COMMONS_EVOLUTION.md), `icn-compute` crate.
- **Truth label:** `repo-grounded architecture explainer` for the primitives that exist; `future-state / roadmap` for commons-credit accounting flows or federation-scale compute coordination that exceed current state.

## 8. Product-surface concept rules

Visuals that depict a product surface (member shell, action card at app scale, receipt detail, steward cockpit, operator console) must:

- be labeled `illustrative direction` unless the surface is shipped end-to-end as one coherent product;
- show only capabilities that exist beneath the surface, even when the surface itself is illustrative;
- not introduce a navigation pattern not validated by an existing primitive or an accepted spec;
- name fictional entities and members clearly (no real cooperative, no real person, no institution-package name);
- avoid NYCN, Summit, or any specific package's branding, copy, vocabulary, or fixtures (see §11);
- not depict push notifications, transaction popups, currency totals, balance widgets, or any chrome that contradicts §5.

Sample fictional entity names approved for use in generic ICN visuals: `Brightworks Collective`, `Northeast Worker Federation`, `Riverbend Community`, `Open Commons Compute Pool`. Sample fictional member names: `Avery Smoke`, `Robin Placeholder`, `Sam Example`. Use these or invent equivalents; do not reach for real names.

## 9. Image / visual asset plan

The bible recognizes four asset classes. Each has a distinct production path.

| Class | Examples | Production path |
|---|---|---|
| **Astro/SVG component** | `ClosureLoop`, `ScopeModel`, `ProvenanceTrail`, `MemberSurface` | Live source in `website/src/components/`. Tokenized, themable, grayscale-tested, integrated with `Icon.astro`. This is the **canonical** form. |
| **SVG-in-docs figure** | Inline SVG inside a doc that explains a model | Hand-authored SVG using design tokens; never raster. |
| **Diagram from text** | Mermaid sequence/flow diagrams inside markdown | Mermaid sources only when the diagram is purely structural and not a member-facing visual. |
| **Generated image sketch** | Mood / scene / illustrative concept produced by an image model | Treated as a **sketch** under §11. Never shipped as a final asset. |

A new visual that becomes load-bearing (homepage, what-is-ICN, doc front-matter, deck hero) must be authored as class 1 or class 2. Class 4 cannot be load-bearing.

## 10. The "do not generate yet" brief gate

A brief moves to image generation **only after** every gate item below is satisfied:

1. **Source anchors listed.** Every claim in the brief points to a file path under §2.
2. **Truth label assigned.** One of the seven labels in §3.
3. **Audience named.** Member, organizer, developer, grant reviewer, or "first-conversation reader."
4. **Core question stated.** One sentence the visual answers.
5. **Message stated.** One sentence the viewer should walk away with.
6. **Must-show / must-avoid lists complete.** Including vocabulary, including iconography.
7. **Canonical copy drafted.** The exact words that will appear on the visual.
8. **Visual grammar named.** Which existing primitive does it extend, replace, or reference?
9. **Accessibility requirements written.** Contrast, label, alt text, motion, and grayscale plan.
10. **Review checklist run.** See [`assets/VISUAL_REVIEW_CHECKLIST.md`](assets/VISUAL_REVIEW_CHECKLIST.md).

Until all ten items are present, the brief is closed to generation. The brief lives; the image does not.

## 11. Generated-image workflow

Generated images (image models, AI tools, external collaborators) are permitted as **sketches** under tight rules:

- **Truth label is `illustrative direction`** — never `implemented / current UI`, never `repo-grounded`, never `verified`. `sketch` is **not** a truth label; it is a workflow / storage marker used in filenames, figure captions, and folder names (e.g. `member-shell.sketch.png`, `sketches/`, a "SKETCH — not for ship" chip) to make the exploration status visible at a glance. Every generated image carries the truth label `illustrative direction` in the brief and on the asset, with the `sketch` marker layered on top wherever it lives.
- **Hard rule: generated images are sketches only, until rebuilt as Astro / SVG / source assets** under §12. There is no path on which a generated PNG becomes a load-bearing asset by being upgraded or polished. The path is always: sketch → brief → source asset → ship the source asset, not the sketch.
- **Never shipped as a final asset.** A sketch can inform a class 1 (Astro/SVG) or class 2 (SVG-in-docs) build. It cannot become the asset itself.
- **Never load-bearing.** A sketch may sit in an internal brief, an internal slide, or an exploration doc. It does not sit on the public website, in canonical docs, in a deck shown outside the project, or in a member-facing surface.
- **No fake UIs.** A sketch must not generate a fake member shell, a fake receipt, a fake action card with plausible-looking real data, a fake email, or a fake notification — those create the impression of capability that is not shipped.
- **No member personal data.** Including fixture data that could plausibly be read as real.
- **No regulated-financial imagery.** No coins, no wallets, no bank ledgers, no payment popups.
- **No real cooperative names, real members, real partners.** Including NYCN.
- **Stored separately from production assets.** Sketches live in brief folders or scratch directories; they do not enter `website/src/assets/` or any registered location.

### 11.1 Generated text inside images is unreliable

This is a separate, load-bearing warning.

Image-generation models routinely produce text that is **wrong, garbled, hallucinated, or invented**, even when given exact strings. They will misspell ICN, invent endpoints, render gibberish in UI chrome, substitute plausible-looking jargon for canonical vocabulary, and produce numbers that look authoritative but came from nowhere.

For the ICN visual explainer system this means:

- **Every glyph of text in a generated image must be manually verified against its source** before the image is used anywhere — internal brief, exploration doc, slide, anything.
- **Any text that cannot be verified must be removed or covered**, not "left in as direction." Direction does not justify shipping a wrong endpoint name or a hallucinated receipt schema.
- **If the message of the visual depends on the text being correct** — a labeled diagram, a UI mock, an annotated screenshot — the only acceptable path is to rebuild the visual as a source asset (Astro, SVG, hand-authored) with the real strings. Do not ship the generated text image.
- **Numbers, dates, identifiers, vote counts, balances, hash strings, and any "screenshot-like" text in a generated image are presumed false** until checked against the substrate.

The rule, restated: **generated text inside generated images is not evidence, not copy, and not source. It is a placeholder that must be replaced with verified strings in a source asset.**

The bible's stance is: **a generated image is a way of thinking about a future asset, not the asset itself.**

## 12. Production-source rule

Any visual that becomes load-bearing — homepage, doc front-matter, public-facing diagram, deck hero, social card, in-product surface — must be a **source asset**: Astro/SVG component, hand-authored SVG, or token-grounded design file with an editable export path.

PNG-only assets are not acceptable as load-bearing. The reasons:

- they cannot be themed (dark/light, accessibility palettes)
- they cannot inherit tokens
- they cannot be diffed in review
- they encode a single layout choice that goes stale
- they tend to outlast the truth they were drawn against

The rule:

> **An important visual must be re-built as source before it ships. The sketch is permitted upstream; the source is what ships.**

## 13. Cross-references and see-also

- [docs/design/ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) — top-level entry point for the ICN Commons Design System
- [docs/design/ICN_VISUAL_SYSTEM.md](ICN_VISUAL_SYSTEM.md) — stable visual doctrine
- [docs/design/CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) — voice and vocabulary
- [docs/design/ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) — accessibility floor
- [docs/design/ICN_PROMPT_LIBRARY.md](ICN_PROMPT_LIBRARY.md) — image-generation prompt patterns
- [docs/design-language/brief-v0.md](../design-language/brief-v0.md) — canonical design language brief
- [docs/design-language/concept-map.md](../design-language/concept-map.md) — canonical → public terminology
- [docs/design-language/accessibility.md](../design-language/accessibility.md) — component-level accessibility patterns
- [docs/architecture/THE_COMMONS.md](../architecture/THE_COMMONS.md) — substrate doctrine
- [docs/architecture/KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md) — meaning-firewall architecture
- [docs/reference/project-index/current-truth-map.md](../reference/project-index/current-truth-map.md) — what is real now
- [docs/reference/project-index/show-readiness-map.md](../reference/project-index/show-readiness-map.md) — what may be shown
- [docs/reference/project-index/runtime-surface-map.md](../reference/project-index/runtime-surface-map.md) — actual gateway surfaces
- [docs/design/assets/ASSET_REGISTER.md](assets/ASSET_REGISTER.md) — the live register of visual assets
- [docs/design/assets/VISUAL_REVIEW_CHECKLIST.md](assets/VISUAL_REVIEW_CHECKLIST.md) — pre-ship review

## 14. Non-goals

- Not a brand kit for any institution package (NYCN, Summit, future federations).
- Not a CSS framework or token system; those live in `website/src/styles/global.css` and the design language.
- Not a substitute for accessibility testing on real devices with real members.
- Not a doctrine that freezes navigation models or component APIs.
- Not a generator. It governs production; it does not produce.

## 15. How this bible evolves

- Updates to the bible follow the same review path as the design system: design review + one technical reviewer + truth-map cross-check.
- Adding a new core model (§7) requires a brief, an implementation surface or a clear path to one, and a review against §10.
- Changing a vocabulary rule (§5) requires sign-off from the same reviewers and a sweep of existing briefs.
- Truth-label changes propagate down to every asset that carries the label and every brief that uses it.

The bible is `Last Reviewed` quarterly at minimum, and after any phase transition in `PHASE_PROGRESS.md`.

---

## Appendix A · Truth-label band (seven canonical states)

> **Status:** promoted from the v0.2 Claude Design seed (`seed/VISUAL_EXPLAINER_APPENDIX.md`) per [`claude-design-seed/CHANGELOG.md`](claude-design-seed/CHANGELOG.md). Formalizes the seven labels from §3 as a renderable band with visual treatment and carry-through rule.

### Why this exists

Every visual explainer asserts something about the system. The truth-label band names what is being asserted — implemented, illustrative, archived — so the reader can calibrate trust before reading. A version of an asset that hides its truth label is rejected.

### The seven states

| Label | When to use | Public-claim safety |
|---|---|---|
| **strong today** | Implemented, integrated, load-bearing. Evidence-linked (code + test + runtime). | Public · safe to assert |
| **advancing now** | Actively being built. Real progress, not yet reliable. | Internal disclosure only |
| **real but maturing** | Implementation exists; some surfaces or integrations still lag. | Public · qualified |
| **illustrative direction** | Concept art for an unshipped surface. Direction, not implementation. | Concept · always labeled |
| **behind the system** | Capability exists underneath; the interface is catching up. | Honest disclosure |
| **not yet** | Scoped, not currently active. No timeline implied. | Roadmap · descoped-safe |
| **deprecated · do not ship** | Retained for reference only. Removed from active product. | Archive · no public use |

These states map onto — and refine — the seven `truth label` values enumerated in §3. The §3 table remains the canonical naming surface; this band is the formal *renderable* shape. Where the two disagree, §3 wins.

### Visual treatment

The chip carries the label text and an accent that reinforces the state:

- green for `strong today`
- teal for `advancing now`
- amber for `real but maturing` and `illustrative direction`
- muted neutral for `behind the system` and `not yet`
- rose for `deprecated · do not ship`

Color reinforces; the **text label is load-bearing**. A grayscale render must still distinguish the seven states by structure, copy, and (where present) icon. This is the §6 accessibility floor applied to the chip itself.

### Mandatory carry-through

- Every illustrative asset ships with a visible truth-label chip.
- Every export of an illustrative asset retains the chip; a version that hides it is rejected per [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 2.
- For static assets that can be circulated outside their host page, the chip is part of the SVG file itself (the placeholder logo's stamped "PLACEHOLDER · NOT FINAL" caption is the reference pattern).

### Cross-references

- [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 2 — member-app screenshots without an illustrative truth label
- [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 11 — screenshots cited as implementation evidence
- [ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md) — public maturity claims and linked evidence
- [`assets/VISUAL_REVIEW_CHECKLIST.md`](assets/VISUAL_REVIEW_CHECKLIST.md) §2 — the pre-ship truth-label check

---

## Appendix B · Rejected patterns (side-by-side comparison)

> **Status:** promoted from the v0.2 Claude Design seed (`seed/VISUAL_EXPLAINER_APPENDIX.md`). Renders the anti-patterns enumerated in [`ICN_VISUAL_SYSTEM.md`](ICN_VISUAL_SYSTEM.md) §7 and the rejection floor in [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) as concrete side-by-side comparisons. Prose anti-pattern lists are easy to skim; side-by-side comparisons hold under code review pressure.

Each pattern is named, shown in its rejected form, and paired with the correct ICN response. The right-hand "Use" column is the corrected form a reviewer should expect to see in a passing PR.

### Pattern 1 · Glassmorphism + fintech identity

| Reject | Use |
|---|---|
| "Your wallet · $1,247.50 · ↑ 12% this week" on a blurred gradient card | Governed social accounting: "Mutual-credit position · 142h contributed · 38h allocated · 2 open obligations" |

Reads as fintech / crypto. ICN does not custody value; the kernel has no "balance". Use governed social accounting: position over wallet, obligation over debt, settlement over payment, unit over currency. Rule source: [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 6 (crypto / wallet / token framing).

### Pattern 2 · "AI sheen" + decorative glow

| Reject | Use |
|---|---|
| "⚡ AI · POWERED · Smart governance assistant" with gradient text and glowing borders | Calm civic seriousness: "Three proposals to review this week. Each links its mandate, threshold, and the receipt it will produce." |

Spectacle replaces substance. Glow implies certainty the kernel cannot prove. The institution is not "smart"; the member is. Surface copy that helps the member decide; skip the chrome. Rule source: [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 8.

### Pattern 3 · Glowing network-globe / central hub map

| Reject | Use |
|---|---|
| A glowing globe with random connecting lines and floating nodes labeled "Global decentralized network" | Three co-equal scopes side-by-side: cooperative, community, federation — each with its concept icon and a one-line description, no central node, no hub |

No people, no institutions, no causality. Reads as crypto marketing. Three scopes, three icons, three names. No hub. Color reinforces; text names. Rule source: [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 9.

### Pattern 4 · Faux-foundation seal

| Reject | Use |
|---|---|
| Heraldic crest with "★ ESTABLISHED · MMXXVI ★" in faux-Latin | Restrained provisional mark with truth-label caption ("PLACEHOLDER MARK · PROVISIONAL · NOT FINAL IDENTITY") |

Invented heraldry overclaims institutional maturity. The mark is provisional; pretending otherwise is dishonest. One color, one line weight, no English text, no false age. Honest about being temporary. Rule source: [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 10.

### Pattern 5 · Color-only status

| Reject | Use |
|---|---|
| A row of cards where status is conveyed only by border color (red = rejected, green = accepted) with no text label | The same cards, each carrying a labeled chip ("Accepted · 5 for 1 against", "Rejected · quorum not reached") and a concept icon |

Color reinforces; it never carries. Removing color must leave the surface fully legible. Test: drop saturation to 0 — if state distinction collapses, the surface fails. Rule source: [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 4; [`design-language/accessibility.md`](../design-language/accessibility.md) §3.1.

### Pattern 6 · English-only fixed-width UI

| Reject | Use |
|---|---|
| A button labeled "OK" in a 64px-wide fixed container that clips at any non-English string longer than two characters | A button with `min-width` and flexible padding, designed against the longest plausible localized form (~1.5× English) |

ICN is global infrastructure. A surface that breaks at long-string locales (German, Finnish) or that fixed-pixels its containers so non-English content gets clipped is not member-ready. Verify with Spanish, German, Arabic + RTL. Rule source: [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 5.

### Pattern 7 · Screenshots as proof

| Reject | Use |
|---|---|
| A blog post asserts "ICN ships X" and links a screenshot as evidence | The same claim links the kernel code, the test, the receipt that the kernel emitted, and the maturity band on `whats-real-now` |

A clean PNG does not certify a behavior; on-device verification does. Screenshots are artifacts, not implementation evidence. Every "shipped" claim links the proving code or ADR. Rule source: [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 11; [ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md).

### Pattern 8 · "Dashboard"

| Reject | Use |
|---|---|
| A "Governance Dashboard" with stacked metric tiles ("Active proposals · 12", "Members online · 348", "Decisions this week · 7") | Named surfaces: "Standing view" (who you are, where recognized), "Action queue" (what needs you now), "Activity" (what happened, with proof), "Position" (what you have contributed and what you can draw on) |

ICN does not have a dashboard. "Dashboard" implies metrics-over-meaning, optimization-over-accountability, and platform-over-institution. Name surfaces by what they do. Rule source: [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 7.

### Carry-through

- [`ICN_VISUAL_SYSTEM.md`](ICN_VISUAL_SYSTEM.md) §7 enumerates these patterns in prose; this appendix is the canonical side-by-side reference.
- [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) is the rejection floor; this appendix is its visual carry-through.
- [`assets/VISUAL_REVIEW_CHECKLIST.md`](assets/VISUAL_REVIEW_CHECKLIST.md) §8 includes a checklist row per pattern so reviewers can cite a specific row when blocking a PR.

### Sources used to draft this appendix

- [`ICN_VISUAL_SYSTEM.md`](ICN_VISUAL_SYSTEM.md) §7 — anti-patterns and banned aesthetics
- [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) — twelve-item rejection floor
- The v0.2 Claude Design seed's `seed/VISUAL_EXPLAINER_APPENDIX.md` and the `preview/rejected-patterns.html` side-by-side renders
- The v0.2 Claude Design seed's `preview/truth-bands.html` for the seven-state band
