---
Status: operational
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-17
Last Updated: 2026-05-17
Purpose: How to set up and use Claude Design for ICN — both as an external paste-driven collaborator and as a local Claude Code design skill.
---

# Claude Design — ICN Setup and Workflow

How to use Claude for design work on ICN, reliably, without re-explaining the system
every time.

There are two modes. Pick the one that matches the surface you're at.

| Mode | Surface | Use when |
|------|---------|----------|
| **External** | claude.ai (web app) or any non-repo Claude session | Producing visual artifacts, exploring a concept, getting a second opinion outside the codebase |
| **Local** | Claude Code in this repo, `/design:*` skills | Reviewing code-adjacent design, auditing live components, applying decisions to files |

---

## 1. External mode — claude.ai paste flow

### 1.1 Setup (one-time, ~2 minutes)

You will paste three things at the start of every Claude Design session. Have them ready
as a project file, a snippet manager entry, or a saved gist:

1. **[`docs/design/CLAUDE_DESIGN_CONTEXT.md`](CLAUDE_DESIGN_CONTEXT.md)** — the briefing.
   Self-contained: brand position, tokens, vocabulary, surface inventory, kernel binding.
2. **A "job card"** — one paragraph stating what you want produced (artifact, audience,
   constraints, deliverable format). Templates in §1.3 below.
3. **Reference attachments** — the one or two reading-order docs from CONTEXT §9 that
   are most relevant to this specific job. Do not attach all seven; pick by need.

That's it. The briefing already tells Claude what kind of artifact to return and what to
flag as assumptions.

### 1.2 Per-session workflow

```
1. New Claude.ai conversation.
2. Paste CLAUDE_DESIGN_CONTEXT.md.
3. Paste the job card (§1.3 templates).
4. Attach the 1–2 reference docs (§1.4 doc-to-job table).
5. Ask Claude to acknowledge constraints before producing.
   ("Before you start: list the three hard constraints that apply to this job.")
6. Review the deliverable. Flag anything that asserts capability the kernel can't prove
   (tier-1 truthfulness rule).
7. If the deliverable is worth keeping, route it through the asset lifecycle in [`assets/README.md`](assets/README.md) — register the asset in [`assets/ASSET_REGISTER.md`](assets/ASSET_REGISTER.md), write a brief under [`assets/briefs/`](assets/briefs/), and (for generated-image exploration) place sketches under that brief's own `sketches/` folder. Sketches are never load-bearing and never enter `website/src/assets/` — see [`ICN_VISUAL_EXPLAINER_BIBLE.md`](ICN_VISUAL_EXPLAINER_BIBLE.md) §11.
```

### 1.3 Job-card templates

Paste one of these after the context doc. Fill in `<brackets>`.

**Visual design (new artifact)**

```
JOB: Visual design — <surface name>.
TARGET: <breakpoint>, <theme>.
INCLUDES: <what must be on screen — e.g. "status pill, mandate line, primary action, secondary action, dismissed receipt link">
DELIVERABLE: <single PNG mock | annotated HTML/CSS | SVG composition>
RATIONALE: One paragraph explaining choices against §2 (Brand) and §3 (Constraints) of the briefing.
ASSUMPTIONS: Flag anything you assumed about data, capability, or behavior.
LABEL: <concept | UX direction | implementation-ready>  (per ADR-0033)
```

**Component spec**

```
JOB: Component spec — <component name from CONTEXT §6 "not yet built" list>.
FORMAT: Markdown spec — variants, states, props, accessibility notes, code example.
TOKENS: Use the tokens in CONTEXT §4. Do not invent new colors.
ACCESSIBILITY: WCAG 2.2 AA. Reduced motion. 44×44 tap targets. Color is not the only carrier.
DELIVERABLE: A single Markdown table block per section. No screenshots.
```

**Copy review**

```
JOB: Copy review.
INPUT:
  <paste the copy, one string per line>
RULES: Apply CONTEXT §5 vocabulary and CONTENT_STYLE_GUIDE.md voice rules.
DELIVERABLE: For each string — PASS | FLAG <reason> | REWRITE <new version>.
DO NOT REWRITE strings that already pass. Pass-through is signal.
```

**Accessibility audit**

```
JOB: Accessibility audit against WCAG 2.2 Level AA.
INPUT:
  <paste HTML or describe the surface>
RULES: Apply ACCESSIBILITY_BASELINE.md per-surface obligations + design-language/accessibility.md component rules.
DELIVERABLE: WCAG criterion | PASS | FAIL <evidence> | FIX <proposed change>
INCLUDE: Color contrast (with computed ratios), keyboard reachability, screen reader semantics, tap targets, motion handling, plain language.
```

**Design critique**

```
JOB: Critique against ICN design principles.
INPUT: <link or paste of the mockup>
RULES: Apply CONTEXT §2 (brand), §3 (constraints), §7 (kernel binding) and ICN_DESIGN_SYSTEM.md "Design principles (compact)" list.
DELIVERABLE: 3–5 specific, prioritized observations. Each observation cites the principle violated and proposes a fix.
DO NOT: produce a redesign. Critique only.
```

**Concept exploration**

```
JOB: Concept exploration for <unbuilt primitive from CONTEXT §6>.
DIRECTIONS: 3, each in a different visual register.
LABEL: All three are concept art, not implementation. Mark accordingly per ICN_VISUAL_EXPLAINER_BIBLE.md.
DELIVERABLE: For each direction — one composition + one paragraph rationale + one assumption flag.
```

### 1.4 Doc-to-job attachment table

Beyond the briefing, attach only what the job needs:

| Job | Always attach (in addition to CONTEXT) | Often useful |
|-----|----------------------------------------|--------------|
| Visual design | `ICN_VISUAL_SYSTEM.md` | `brief-v0.md` if creating a new primitive |
| Component spec | `accessibility.md`, `brief-v0.md` §6 (visual primitives) | `ACCESSIBILITY_BASELINE.md` |
| Copy review | `CONTENT_STYLE_GUIDE.md` | `concept-map.md` |
| Accessibility audit | `ACCESSIBILITY_BASELINE.md`, `accessibility.md` | `ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` for member surfaces |
| Design critique | `ICN_DESIGN_SYSTEM.md` | the principles section of `brief-v0.md` |
| Concept exploration | `ICN_VISUAL_EXPLAINER_BIBLE.md` | `brief-v0.md` |
| Diagram or explainer | `ICN_VISUAL_EXPLAINER_BIBLE.md`, `assets/VISUAL_REVIEW_CHECKLIST.md` | a relevant `assets/briefs/VE-*.md` for prior art |
| Mobile surface | `mobile/icn-mobile-ux-spec-v1.md` | `ACCESSIBILITY_BASELINE.md` |

### 1.5 What to flag in the deliverable

Before saving anything Claude Design produces, scan for:

- Asserted capability the kernel can't prove → mark concept-only or remove (ADR-0032, ADR-0033)
- Forbidden vocabulary from CONTEXT §5 "Words to avoid"
- NYCN / Summit references on anything that isn't an institution package
- Color-only meaning (run grayscale preview mentally — or actually)
- Tap targets under 44×44
- Motion that doesn't honor `prefers-reduced-motion`
- A dangerous action without mandate / reversibility / receipt declared before confirm

A deliverable with any of these is a draft, not a ship.

---

## 2. Local mode — Claude Code design skills

Claude Code ships design skills under the `design:` namespace. Each one is invoked by
the user and operates on this repo with full file access.

| Skill | When to invoke | What it does |
|-------|----------------|--------------|
| `/design:design-system` | Audit, document, or extend a component | Runs against the current ICN doctrine; produces audit/spec/extension output |
| `/design:design-critique` | Get critique on a mockup or design choice | Structured feedback against ICN principles |
| `/design:accessibility-review` | WCAG 2.2 AA audit on a design or live page | Pass/fail per criterion with fixes |
| `/design:ux-copy` | Write or review microcopy | Applies CONTENT_STYLE_GUIDE.md voice and vocabulary |
| `/design:design-handoff` | Generate dev handoff spec | Layout, tokens, props, states, breakpoints, edge cases |
| `/design:user-research` | Plan a research effort | Interview guides, usability tests, survey design |
| `/design:research-synthesis` | Synthesize interview/test notes | Themes, insights, prioritized recommendations |

### 2.1 Setup (one-time, already done if you're reading this)

The local design skills work out of the box. Three things make them work well for ICN:

1. **`docs/design/CLAUDE_DESIGN_CONTEXT.md` exists** — the briefing the skill agents
   can read directly. ✅ done.
2. **`docs/design/ICN_DESIGN_SYSTEM.md` is the entry point and references all
   doctrine** — including the kernel binding. ✅ done.
3. **The `design:` skills know to look in `docs/design/` and `docs/design-language/`** —
   they are file-aware and follow the entry-point doc.

If you want a skill to be more aggressive about reading ICN context first, prefix the
invocation:

```
/design:design-critique Before critiquing, read docs/design/CLAUDE_DESIGN_CONTEXT.md
and docs/design/ICN_DESIGN_SYSTEM.md so your critique is bound to ICN doctrine.
Then critique: <paste mockup link or description>
```

### 2.2 Local-mode workflow

```
1. Be in this repo. (Working directory matters — skills are file-aware.)
2. Invoke the right /design:* skill from the table above.
3. Hand it the artifact: a path, a paste, or a URL.
4. The skill will read the relevant doctrine docs as needed.
5. Review the result. If it touches a file, verify before commit (per CLAUDE.md).
```

### 2.3 When local mode is better than external mode

- The job needs to touch live files (component code, copy in the codebase, ADR draft).
- The job needs to verify against current implementation (token usage, accessibility lints).
- The job is review-shaped (critique, audit, handoff) more than create-shaped.

### 2.4 When external mode is better than local mode

- The job is producing a visual composition (Claude Code is not a visual canvas).
- The job is concept exploration (multiple variants, no file touch needed).
- The job needs a long uninterrupted session with rich image attachments.

---

## 3. Mixing modes (the common pattern)

The realistic workflow is: **explore externally, decide, then apply locally.**

```
External: /design:design-critique a mockup in claude.ai → get 3 directions
         ↓
You:      pick one
         ↓
Local:   /design:design-system extend → produce the spec in docs/design/components/<name>.md
         ↓
Local:   /design:design-handoff → produce dev-ready spec for the website team
         ↓
Local:   verify with /design:accessibility-review against the live preview
```

If you only ever use one mode, the doctrine works. Mixing is more powerful for new
primitives because external mode is faster for visual iteration and local mode is
stricter about the rules.

---

## 4. Common failure modes and what they signal

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| Deliverable uses crypto / fintech vocabulary | Briefing not pasted, or attachment doc wasn't read | Re-paste CONTEXT.md §5 explicitly; ask Claude to acknowledge §3 constraint #2 before continuing |
| Deliverable invents new color hex values | Tokens section ignored | Point at CONTEXT.md §4; ask to redo using only the listed tokens |
| Deliverable depicts capability the system doesn't have | ADR-0032 / ADR-0033 not loaded | Add ADR-0032 and ADR-0033 to attachments; require explicit truth label |
| Action card omits mandate / reversibility / receipt | ADR-0027 + content style guide §"Dangerous-action copy" missing | Attach CONTENT_STYLE_GUIDE.md; require the dangerous-action template |
| Mobile design forgets one-handed reachability | Mobile spec not attached | Attach `mobile/icn-mobile-ux-spec-v1.md` |
| Critique is generic ("looks good, consider hierarchy") | Doctrine-bound criticism not asked for | Reframe job: "Critique against ICN_DESIGN_SYSTEM.md design principles, cite the principle for each observation" |
| Deliverable conflates cooperatives / communities / federations | Scope coequality constraint missed | Point at CONTEXT.md §3 constraint #7; ask for a redo |

---

## 5. The institution-package boundary

ICN design work is **not** NYCN design work, and not Summit design work. Those are
institution packages that consume ICN primitives and tokens but bring their own brand,
copy, and surfaces.

When using Claude Design for institution-package work:

- Do not paste `CLAUDE_DESIGN_CONTEXT.md` (it's the ICN substrate, not a package brand).
- Use the package's own brief from its private repo.
- ICN tokens, vocabulary, and accessibility floor still apply as inheritance, but the
  brand layer is the package's.

If you're not sure which one you're doing: **if the surface ever ships to a member
without an NYCN or Summit logo on it, it's ICN-substrate work, and CONTEXT.md applies.**

---

## 6. Maintenance

Keep this doc and `CLAUDE_DESIGN_CONTEXT.md` in sync with:

- The token list in `website/src/styles/global.css`
- The concept vocabulary in `docs/design-language/concept-map.md`
- Surface state (built vs spec-only) in `docs/design/ICN_DESIGN_SYSTEM.md`
- New ADRs binding design (currently 0026, 0027, 0028, 0032, 0033)

Re-review on: any of the above changing, or quarterly, whichever is sooner.

---

## 7. See also

- [CLAUDE_DESIGN_CONTEXT.md](CLAUDE_DESIGN_CONTEXT.md) — the paste briefing
- [ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) — design system entry point
- [../DESIGN_PRINCIPLES.md](../DESIGN_PRINCIPLES.md) — kernel-side counterpart
- [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) — for diagrams and infographics specifically
- [assets/VISUAL_REVIEW_CHECKLIST.md](assets/VISUAL_REVIEW_CHECKLIST.md) — pre-ship review for visual deliverables
