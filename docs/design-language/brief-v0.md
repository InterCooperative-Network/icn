# ICN Universal Civic Design Language — Brief v0

**Status:** v0 — canonical source of truth for the ICN design language.
**Scope:** website today; product UI, diagrams, docs, and onboarding over time.
**Last updated:** current refactor session.

---

## 1. Purpose

ICN needs a design language that makes institutional life **legible**.

Not attractive. Not modern. Not technically impressive.

It needs a system of symbols, language, structure, and visual patterns that helps people understand:

- who is acting
- in what scope
- with what standing
- under what authority
- according to what rules
- affecting what resources
- producing what record
- in relation to which other institutions

The design language must make ICN understandable across regions, organizations, cultures, and levels of expertise without flattening the meaning of the system.

**Core principle:** *same system grammar, locally expressed*

---

## 2. North star

ICN should feel like:

- institutional
- civic
- calm
- legible
- federated
- auditable
- serious
- structured
- human-centered but not soft
- accessible by design

It should feel like **public infrastructure for collective self-rule**.

It should not feel like:

- crypto
- startup SaaS
- enterprise dashboard sludge
- generic govtech
- hacker terminal aesthetic
- cyberpunk network maps
- AI vapor
- futuristic spectacle
- obscure priest-language software

---

## 3. What the design language must do

- make complex institutional systems easier to understand
- teach users the system through repetition
- support mobile-first real-world use
- support plain language and local translation
- preserve semantic consistency across different institutions
- distinguish public meaning, truth-state, and technical depth
- remain readable under stress, low attention, or low expertise
- allow local adaptation without semantic collapse

This is not branding alone. It is part of the institutional interface.

---

## 4. Core design principles

### 4.1 Structure over spectacle

Every visual element should clarify relation, authority, state, scope, proof, or consequence. No decorative futurism.

### 4.2 One system, many surfaces

The website, product, docs, onboarding, and diagrams should all teach the same grammar.

### 4.3 Stable meaning, flexible expression

Core concepts remain stable. Visible wording, examples, and helper text can vary by institution, region, or language.

### 4.4 Accessibility is foundational

Meaning must survive low vision, colorblindness, stress, mobile use, translation, and low familiarity. Accessibility is not an add-on. It is part of institutional legitimacy.

### 4.5 Repetition builds literacy

Symbols, card structures, scope markers, status bands, and relationship patterns should recur consistently. Users should start learning the system just by seeing the same visual patterns recur.

### 4.6 Human-centered, not infantilized

The system should be easy to understand without being dumbed down.

### 4.7 Institutional seriousness without bureaucratic deadness

Alive and usable, but durable, formal, and trustworthy.

---

## 5. Three layers of meaning

Every public-facing surface must operate across three layers simultaneously:

### Layer 1 — Canonical semantic layer

Stable system concepts. Used internally and for technical precision. Never changes.

Examples: Identity, Standing, Authority, Governance, Policy, Accounting, Execution, Provenance, Federation, Commons, Scope, Agreement, Allocation, Obligation, Settlement, Attestation, Role, Membership, Resource, Member experience.

### Layer 2 — Default public vocabulary

Plain-language English. The default user-facing layer for unlocalized public surfaces.

Examples:
- Standing → "your recognized status"
- Authority → "what you are allowed to do"
- Provenance → "history and proof"
- Federation → "how groups work together"

### Layer 3 — Local institutional vocabulary

Overridable by organization, federation, language, or region. A local co-op, an agricultural federation, a mutual-aid network, and an indigenous governance body may all prefer different visible labels for the same underlying concept.

The site should default to Layer 2 on public-facing surfaces. Formal Layer 1 terms are allowed when precision requires them, but must be paired with Layer 2 explanations. Layer 3 is not yet implemented; it is the architecture target for localization.

---

## 6. Visual primitives

These are the recurring forms the system uses. Each should be a reusable component, not per-page CSS.

- **Institutional card** (`.inst-card`) — exists today with 6 semantic accent variants
- **Closure station marker** — exists today as ClosureLoop + station-number pills
- **Scope container** — not yet built
- **Agreement connector** — not yet built
- **Provenance trail / receipt strip** — not yet built
- **Truth-state band** — exists as custom CSS on three pages; not yet a reusable component
- **Resource / commons field** — not yet built
- **Federation relation line** — not yet built
- **Role / authority badge** — not yet built
- **Action block** — not yet built
- **Eyebrow** (`.eyebrow`) — exists today
- **System divider** (`.system-divider`) — defined, unused
- **Section frame** (`.section-frame`) — defined, unused

---

## 7. Current maturity of the design language

As of the latest assessment:

**Built and working:**
- Token system (74 custom properties)
- Dark theme with reasonable contrast (AA+ verified)
- Mobile layout verified
- ClosureLoop component (first canonical diagram)
- `.inst-card` with six semantic accent variants
- `.station-number` pill
- `.eyebrow` mono label pattern
- Maturity bands with text + color (not color alone)
- Public reading order + cross-page flow
- **Icon component + initial symbol family** (`Icon.astro` + `src/data/icons.ts`)
  - First 10 icons: identity, standing, authority, governance, policy,
    accounting, execution, provenance, federation, commons
  - Silhouette-distinct at 16–22px
  - Stroke-based line art (1.5px, round caps), monochrome via currentColor
  - Grayscale-safe: integrated into ClosureLoop and `.inst-card` eyebrows;
    each card remains distinguishable under `filter: grayscale(1)`
- Accessibility foundations: skip-link, focus-visible, reduced-motion
  support, `aria-expanded` hamburger, `aria-current` on nav, `aria-hidden`
  on decorative SVGs

**Not yet built:**
- Scope model diagram
- Federation relation map
- Provenance trail visual
- Commons / resource governance visual
- Reusable truth-state band component (blocked by semantic divergence
  across three current call sites — see Phase 2 notes in session log)
- Full institutional object card grammar (icon ✓, scope, status, action ✗)
- Icon for station 09 Member Experience (convergence station — still
  unresolved; the amber gradient + column span currently carries it)
- Canonical concept map as a runtime data artifact (exists as markdown,
  not yet imported by components)
- Localization scaffold (i18n)
- Real logo
- Light theme maintenance discipline

---

## 7a. Icon system rules

The first implementation of the ICN symbol system lives in:

- `website/src/data/icons.ts` — the sprite definitions (SVG paths per concept)
- `website/src/components/Icon.astro` — the renderer
- `website/src/components/ClosureLoop.astro` — the first integration
- `website/src/pages/index.astro` (inst-card eyebrows) — the second integration

### Style rules (enforced by review)

- **One concept, one icon.** Icon ids match canonical concept ids in
  `concept-map.md`. No parallel icon systems, no per-page icon reinterpretation.
- **Stroke-based line art.** 1.5px stroke, round line caps, round joins,
  `fill="none"` on the parent `<svg>`. No filled shapes unless absolutely
  required for silhouette distinction.
- **24×24 viewBox, designed for 16–22px display.** Silhouettes must remain
  distinct at 16px. If an icon becomes illegible at that size, simplify it.
- **Monochrome via `currentColor`.** Icons inherit their stroke color from the
  parent element. Never bake color into path attributes.
- **Silhouette-distinct.** Each icon must be recognizable without relying on
  its label. At 20px, you should be able to identify the concept from shape
  alone.
- **Civic, not decorative.** No fantasy insignia, no crypto aesthetics, no
  cartoon flourishes, no product-UI doodles. Closer to transit signage or
  constitutional stamps than to consumer app icons.
- **Culturally careful.** Avoid metaphors that only make sense in English or
  Western institutional traditions. When a concept is culturally loaded
  (e.g. "standing", "commons"), lean on abstract structural forms over
  narrative scenes.

### Accessibility rules

- **Paired with text on first use.** When an icon is introduced on a page,
  it must appear alongside its visible label. Icons never carry first-time
  meaning alone.
- **`aria-hidden="true"` when decorative.** When the icon sits next to a
  visible text label, it is decorative from the screen-reader's perspective
  and must be hidden. The `Icon.astro` component does this automatically
  when the `label` prop is omitted.
- **`role="img"` + `aria-label` when standalone.** When an icon must carry
  meaning by itself (rare, discouraged), pass a `label` prop and the
  component will set both attributes.
- **`focusable="false"` always.** Icons never trap tab focus.
- **Grayscale-safe.** Every icon + card combination must remain
  distinguishable under `filter: grayscale(1)`. Run the grayscale check
  before shipping a new integration.

### Adding a new icon

1. Add an `IconDef` entry to `ICONS` in `src/data/icons.ts` keyed by the
   canonical concept id from `concept-map.md`.
2. Draw the paths in a 24×24 viewBox using round-cap strokes.
3. Review the silhouette at 16px, 20px, and 22px against the existing set.
   If it is confusable with another icon at those sizes, refine.
4. Add the icon slug to the concept entry in `concept-map.md` once the
   concept map grows an `iconSlug` field (currently tracked informally).
5. Integrate into the relevant page by importing `Icon.astro` and using
   `<Icon name="..." size={...} label="..." />` or decorative form.
6. Run the grayscale test on every page that uses it.

### Open questions (v1 targets)

- **Additional concepts** beyond the initial 10 — scope, agreement,
  allocation, obligation, settlement, attestation, role, membership,
  resource — still need icons.
- **Runtime concept-map consumption.** Currently `concept-map.md` is a
  human-readable markdown table and icons are separately defined in
  `icons.ts`. These should eventually be unified so a single source of
  truth drives both human docs and runtime rendering.
- **Icon for the NetworkGraph replacement.** The placeholder decorative
  graph on the homepage hero could become a motion/composition built from
  the real icon system once the full set exists.

---

## 8. Anti-patterns

ICN should not look or sound like:

- a crypto protocol explainer
- a venture-funded SaaS homepage
- a generic enterprise dashboard
- a government PDF portal
- a cyberpunk network map
- a developer docs site pretending to be a public institution
- a symbolic system so abstract that only insiders can read it
- icon chaos
- color overloading
- long unbroken prose everywhere
- diagrams that require prior expertise
- centralizing federation imagery
- soft consumer polish that undermines seriousness

---

## 9. Open questions for v1

- exact icon family style
- exact palette spec with WCAG-verified contrast pairs
- logo/mark direction
- degree of local institutional theming allowed
- how much terminology override is safe before semantic drift
- whether some concepts need two-level display everywhere
- how to formalize design tokens across site and product

---

## 10. Working definition

> ICN should use a universal civic design language: a stable system of symbols, structures, colors, and plain-language labels that makes institutional life legible across regions, organizations, and languages. The underlying semantic model should remain consistent, while visible wording and examples can be localized by each institution or federation. The system should be understandable on mobile, under stress, across literacy levels, and without insider knowledge. It should feel like public infrastructure for collective self-rule.

---

## 11. Related artifacts

- `docs/design-language/concept-map.md` — canonical → public → local term mapping
- `docs/design-language/accessibility.md` — accessibility rules and WCAG requirements
- `website/src/styles/global.css` — token definitions and shared component classes
- `website/src/components/ClosureLoop.astro` — the first canonical diagram primitive

---

## 12. How to use this document

This is the canonical source of truth for the ICN design language. Every public-facing page, component, copy change, and visual decision should be checkable against it.

**Rule for contributors:** if an edit introduces something this brief does not describe, either the brief needs to evolve (v0 → v1) or the edit is out of scope for the design language. Do not let ad-hoc decisions silently drift the system.

**Rule for future agents:** this brief supersedes any earlier design direction scattered across chat history, README snippets, or informal notes. Start here.
