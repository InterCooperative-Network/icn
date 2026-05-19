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
| **Local** | Claude Code in this repo, `icn-design-advisor` agent (repo-shipped) and optionally the `design` Claude Code plugin pack | Reviewing code-adjacent design, auditing live components, applying decisions to files |

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

## 2. Local mode — Claude Code design routing

Local mode runs design work inside Claude Code in this repo, with full file access.
Unlike external mode, **what's bundled by this repo and what comes from a Claude Code
plugin are different surfaces**. Be precise about which you're invoking.

### 2.1 What this repo ships (always available)

These are committed under `.claude/` and work the moment you're in this worktree:

| Surface | Path | What it does |
|---------|------|--------------|
| **`icn-design-advisor` agent** | `.claude/agents/icn-design-advisor.md` | Specialist agent for ICN design work — knows the doctrine, hard constraints, kernel binding, and pre-ship flag list. Dispatch via the `Task`/`Agent` tool with `subagent_type: icn-design-advisor`. |
| **Design rule** | `.claude/rules/design.md` | Auto-loads when files under `docs/design/**`, `docs/design-language/**`, `docs/mobile/**`, `website/src/**`, `web/**`, or `sdk/react-native/**` are touched. Surfaces the required reading and hard constraints inline. |
| **ICN-specific slash commands** | `.claude/commands/` | `/icn-audit`, `/icn-demo-check`, `/icn-state-sync`, `/icn-trace`. None are design-specific; listed so you know what *is* repo-shipped. |
| **ICN-specific skills** | `.claude/skills/` | `bench`, `changelog`, `demo-validate`, `devnet`, `push`, `pr-create`, etc. None are design-specific. |

If you're working in this repo with Claude Code, the agent and rule above are loaded
automatically — that alone is enough to do design review, copy review, accessibility
review, and component spec work without any plugins.

### 2.2 What requires the `design` plugin (optional)

The richer `/design:*` slash commands — `/design:design-system`,
`/design:design-critique`, `/design:accessibility-review`, `/design:ux-copy`,
`/design:design-handoff`, `/design:user-research`, `/design:research-synthesis` — are
**not bundled by this repo**. They come from a Claude Code plugin pack (the `design`
plugin in the Claude Code marketplace).

If you have the plugin installed, you'll see those commands offered when you type `/`.
If you don't, the commands won't resolve. To check what's available in your current
session, run `/help` or type `/` and inspect the autocomplete.

**Install path** (if you want them):

```
# In Claude Code, from the marketplace UI:
#   Settings → Plugins → install "design"
# Or via CLI (varies by Claude Code version):
claude plugins install design
```

The plugin's design skills are generic (not ICN-aware). To bind them to ICN doctrine,
prefix the invocation:

```
/design:design-critique Before critiquing, read docs/design/CLAUDE_DESIGN_CONTEXT.md
and docs/design/ICN_DESIGN_SYSTEM.md so your critique is bound to ICN doctrine.
Then critique: <paste mockup link or description>
```

Or — and this is the better path — just dispatch the `icn-design-advisor` agent
described in §2.1 above. It already knows the doctrine.

### 2.3 Local-mode workflow (preferred — no plugins required)

```
1. Be in this repo. (Working directory matters — the design rule is path-scoped.)
2. Dispatch the icn-design-advisor agent, OR work directly with file edits and let
   the .claude/rules/design.md auto-load doctrine.
3. Hand the agent the artifact: a path, a paste, or a description.
4. The agent reads ICN doctrine docs and applies the hard constraints from §3 of
   CLAUDE_DESIGN_CONTEXT.md.
5. Review the result. If it touches a file, verify before commit (per CLAUDE.md).
```

### 2.4 When local mode is better than external mode

- The job needs to touch live files (component code, copy in the codebase, ADR draft).
- The job needs to verify against current implementation (token usage, content audit).
- The job is review-shaped (critique, audit, handoff) more than create-shaped.

### 2.5 When external mode is better than local mode

- The job is producing a visual composition (Claude Code is not a visual canvas).
- The job is concept exploration (multiple variants, no file touch needed).
- The job needs a long uninterrupted session with rich image attachments.

---

## 3. Mixing modes (the common pattern)

The realistic workflow is: **explore externally, decide, then apply locally.**

```
External: claude.ai paste flow → critique a mockup → get 3 directions
         ↓
You:      pick one
         ↓
Local:   dispatch icn-design-advisor agent → produce the spec in docs/design/components/<name>.md
         (or `/design:design-system extend` if the `design` plugin is installed)
         ↓
Local:   icn-design-advisor handoff mode → produce dev-ready spec for the website team
         (or `/design:design-handoff` if installed)
         ↓
Local:   icn-design-advisor accessibility-audit mode against the live preview
         (or `/design:accessibility-review` if installed)
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

## 6. Seed import, review, and promotion workflow

Claude Design now produces **seeds** — packaged bundles that include generated docs, HTML preview cards, CSS token files, UI kit recreations, illustrative screenshots, and the bundle's own governance trail. A seed is a *proposal* for ICN design, not a design system in itself. Canonical truth still lives in the upstream repo paths; the seed waits for review.

This section covers the workflow for handing a seed off to Claude Code without contaminating canonical truth with generated polish.

### 6.1 What a seed is, what it is not

- **A seed is:** the output of an external Claude Design session — markdown, HTML, CSS, screenshots, prose, illustrations, plus a self-describing governance bundle (the seed's own `INVENTORY.md`, `PROMOTION_MAP.md`, `DRIFT_REPORT.md`, `PRODUCTION_READINESS.md`, `MUST_NOT_SHIP.md`, and a `CLAUDE_CODE_BUNDLE.md` handoff).
- **A seed is not:** canonical truth, brand decision, implementation evidence, sanctioned source of CSS / components / assets for production paths, or a place from which to import code into `website/src/`, `web/pilot-ui/`, or `sdk/`.

Default status of every seed artifact is `generated seed`. Promotion is an explicit, gated step — see §6.4 below.

### 6.2 Generated-vs-canonical boundary

This boundary is asymmetric and unconditional:

- Canonical truth lives in the upstream ICN repo paths declared in [`docs/registry.toml`](../registry.toml) `[control].canonical_doc_paths` and the `Canonical: yes` rows in [`docs/INDEX.md`](../INDEX.md).
- Generated Claude Design files are not canonical by default.
- Imported repo docs inside a seed may be stale; trust the live repo.
- Screenshots are not implementation evidence.
- UI kits are illustrative unless explicitly promoted.
- Polish is not evidence.

The full version of this boundary, with the rule-of-thumb, lives in [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §2. The hard rejection floor lives in [MUST_NOT_SHIP.md](MUST_NOT_SHIP.md).

### 6.3 Docs / protocol-first handoff

The first PR for any new seed is **docs-only**. It establishes the protocol scaffold (or extends it), records the seed's drift findings, and updates `claude-design-seed/CHANGELOG.md` + `REVIEW_NOTES.md`. It does not touch production UI, CSS, SDK, or assets.

The shape of the handoff is [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](CLAUDE_DESIGN_HANDOFF_TEMPLATE.md). Fill in a fresh copy per seed; the template's §6 (non-goals), §12 (allowed paths), and §13 (forbidden paths) are load-bearing — they override any inferred scope the seed's preview HTML might suggest.

### 6.4 When Claude Code should implement docs only

Default. Every new seed lands as docs-only first. Specifically:

- Adopting the protocol / handoff template / rejection floor / skill scaffold ← **first PR, docs-only**
- Fixing seed-identified drift in canonical docs (e.g., the seed's `DRIFT_REPORT.md` entries) ← **docs-only, scope-locked**
- Adding seed workflow files under `docs/design/claude-design-seed/` ← **docs-only**

Claude Code in docs-only mode does **not** recreate generated prototypes pixel-perfectly, does **not** migrate React / JSX recreations from a seed, does **not** adopt placeholder logo or icon assets, and does **not** create production CSS from seed tokens.

### 6.5 When production UI implementation is allowed

Only after:

1. The relevant promotion candidate has been reviewed via [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3.
2. The human decisions in [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md) §"What requires human decision" are resolved.
3. A separate, scope-locked PR is opened against the appropriate canonical path (`website/src/`, `sdk/`, etc.).
4. That PR carries the full per-PR member-surface checklist (the seed's `HANDOFF.md` shape, retained here as the per-PR template) — accessibility obligations, vocabulary scan, truth-label requirements, member-rights affordances, sync-state honesty, verification commands.

A production-UI PR is not a continuation of the seed-promotion PR. It is a downstream artifact with its own scope, review, and approval.

### 6.6 How to use the handoff template

1. Copy [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) into a scratch location (typically `$CLAUDE_JOB_DIR/handoff-<seed-version>.md` or a worktree).
2. Fill every section: seed identity, source docs the bundle was produced against, generated files, imported references, proposed upstream promotions, explicit non-goals, truth labels, accessibility obligations, vocabulary scan result, drift report summary, unresolved human decisions, allowed paths, forbidden paths, verification commands, PR title / branch guidance, sign-off.
3. Link the filled handoff from [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md). A seed without a filled handoff is not ready for promotion work.
4. Update [`claude-design-seed/CHANGELOG.md`](claude-design-seed/CHANGELOG.md) once review closes.

### 6.7 How to use the review protocol

The protocol enumerates seven required review gates: truth-label, accessibility, vocabulary, implementation-status, language/RTL readiness, low-bandwidth / reduced-motion / large-text, and source-doc drift. A piece of generated seed content can only be promoted after every applicable gate closes green.

The reviewer's loop:

1. Read the bundle's authoritative handoff docs in order: `CLAUDE_CODE_BUNDLE.md`, `INVENTORY.md`, `PROMOTION_MAP.md`, `DRIFT_REPORT.md`, `PRODUCTION_READINESS.md`, `MUST_NOT_SHIP.md` from the seed. Open preview HTML last.
2. Open [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) and fill in a fresh copy.
3. Walk the gates in [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3 against each named artifact.
4. Record findings in [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md).
5. Append a row to [`claude-design-seed/CHANGELOG.md`](claude-design-seed/CHANGELOG.md).
6. Open scope-locked promotion PRs.

Full protocol detail in [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md). Rejection floor in [MUST_NOT_SHIP.md](MUST_NOT_SHIP.md). Workflow narrative in [`claude-design-seed/README.md`](claude-design-seed/README.md).

---

## 7. Maintenance

Keep this doc and `CLAUDE_DESIGN_CONTEXT.md` in sync with:

- The token list in `website/src/styles/global.css`
- The concept vocabulary in `docs/design-language/concept-map.md`
- Surface state (built vs spec-only) in `docs/design/ICN_DESIGN_SYSTEM.md`
- New ADRs binding design (currently 0026, 0027, 0028, 0032, 0033)

Re-review on: any of the above changing, or quarterly, whichever is sooner.

---

## 8. See also

- [CLAUDE_DESIGN_CONTEXT.md](CLAUDE_DESIGN_CONTEXT.md) — the paste briefing
- [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) — seed → canonical review gates
- [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) — seed-level handoff template
- [MUST_NOT_SHIP.md](MUST_NOT_SHIP.md) — hard rejection floor
- [claude-design-seed/README.md](claude-design-seed/README.md) — seed directory workflow
- [claude-design-seed/CHANGELOG.md](claude-design-seed/CHANGELOG.md) — imported seed versions
- [claude-design-seed/REVIEW_NOTES.md](claude-design-seed/REVIEW_NOTES.md) — per-seed human review summary
- [ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) — design system entry point
- [../DESIGN_PRINCIPLES.md](../DESIGN_PRINCIPLES.md) — kernel-side counterpart
- [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) — for diagrams and infographics specifically
- [assets/VISUAL_REVIEW_CHECKLIST.md](assets/VISUAL_REVIEW_CHECKLIST.md) — pre-ship review for visual deliverables
