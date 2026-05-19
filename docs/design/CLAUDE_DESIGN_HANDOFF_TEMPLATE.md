---
Status: operational
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-19
Last Updated: 2026-05-19
Purpose: Standard template for passing a Claude Design seed to Claude Code. Fill in one copy per seed before any promotion work begins.
---

# Claude Design Handoff Template

> **One sentence.** When Claude Design produces a seed, this template is the contract that hands it to Claude Code without losing the truth boundary.

This is not the per-PR member-surface checklist (that lives separately and travels with each member-facing PR). This is the seed-level handoff: it records what the seed contains, what may be promoted, what must remain seed-only, and which paths Claude Code is allowed to touch.

**Use it like this.** Copy this file into `docs/design/claude-design-seed/handoff-<seed-version>.md` (or its equivalent in the seed's review directory), fill every field, and link it from [`docs/design/claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md). A seed without a filled handoff template is not ready for promotion work.

---

## 1. Seed identity

- **Design bundle URL:** `_______________________`
- **Seed name / version:** _______________________ (e.g. "ICN Commons Design System — Claude Design Seed v0.1")
- **Seed produced on:** YYYY-MM-DD
- **Handoff prepared on:** YYYY-MM-DD
- **Bundle author / Claude Design session lead:** _______________________
- **Handoff reviewer (this template's author):** _______________________

## 2. Source docs the bundle was produced against

List every ICN repo doc the seed was given as input. The seed almost certainly bundled imported copies of these; record the version it bundled vs the live repo version.

| Repo doc | Seed-bundled version (commit / last reviewed) | Live repo version (commit / last reviewed) | Drift? |
|---|---|---|---|
| `docs/design/ICN_DESIGN_SYSTEM.md` | | | |
| `docs/design/CLAUDE_DESIGN_CONTEXT.md` | | | |
| `docs/design/CLAUDE_DESIGN_SETUP.md` | | | |
| `docs/design/ACCESSIBILITY_BASELINE.md` | | | |
| `docs/design/CONTENT_STYLE_GUIDE.md` | | | |
| `docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md` | | | |
| `docs/design-language/brief-v0.md` | | | |
| `docs/design-language/concept-map.md` | | | |
| `docs/design-language/accessibility.md` | | | |
| `docs/mobile/icn-mobile-ux-spec-v1.md` | | | |
| `docs/spec/member-shell-v0.md` | | | |
| _other_ | | | |

If any row shows drift, file it under §10 "Drift report summary" and treat the live repo version as authoritative.

## 3. Generated files in the seed

What the seed produced. Classify every file as **generated seed**, **UI kit / illustrative mock**, **preview card / showcase**, **generated asset**, or **screenshot**. Default status is seed-only.

| Path inside the bundle | What it is | Status | Notes |
|---|---|---|---|
| | | | |

(Add rows as needed. If the bundle ships an `INVENTORY.md`, this section can summarize that file rather than duplicate it.)

## 4. Imported references

Files the seed copied verbatim from the upstream repo for offline reference. Imported ≠ fresh. Use the live repo for authoritative reads.

| Path inside the bundle | Upstream path | Bundled freshness | Action |
|---|---|---|---|
| | | | |

## 5. Proposed upstream promotions

For each piece of generated seed content the reviewer believes could become canonical, name the proposed upstream path and the gate(s) that still need to close. **Default status is `candidate doctrine` — not promoted.**

| Seed artifact | Proposed upstream path | Gates outstanding (from REVIEW_PROTOCOL.md §3) | Reviewer notes |
|---|---|---|---|
| | | | |

A row in this table is a proposal, not an authorization. Promotion only happens after the review gates close green and a separate, scope-locked PR lands.

## 6. Explicit non-goals for this handoff

State every thing this handoff is *not* asking Claude Code to do. Be specific. Generic non-goals do not protect the truth boundary.

- Do not recreate generated prototypes pixel-perfectly. **Claude Code should not recreate generated prototypes pixel-perfectly unless a later task specifically asks for production UI implementation.**
- Do not migrate seed React / JSX recreations into `website/src/` or `web/pilot-ui/`.
- Do not adopt placeholder logo, wordmark, or icon assets from the seed.
- Do not create production CSS from seed tokens in this PR.
- Do not adopt seed pattern names as canonical concept ids without a separate `concept-map.md` PR.
- Do not claim the seed is canonical.
- Do not claim member surfaces are shipped if the seed depicts them as illustrative.
- _add seed-specific non-goals here_

## 7. Truth labels in scope

Every artifact this handoff names carries one of the labels from [ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) and [ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md):

- [ ] `implemented / current UI`
- [ ] `repo-grounded public explainer`
- [ ] `repo-grounded architecture explainer`
- [ ] `illustrative direction`
- [ ] `future-state / roadmap`
- [ ] `historical`
- [ ] `do not use`

For each artifact named in §3 or §5, declare the label here. Member-shell mocks default to `illustrative direction`.

## 8. Accessibility obligations to plan for

Claude Code must plan for, even on a docs-only PR:

- [ ] WCAG 2.2 Level AA minimum (contrast, target size, keyboard, semantics)
- [ ] High-contrast light and dark themes (not just default dark)
- [ ] `prefers-reduced-motion` honored — motion never carries meaning
- [ ] Large-text / 200% zoom reflow without horizontal scroll or content loss
- [ ] Low-bandwidth mode (no autoplay video, no heavy hero animation, no required JS for first paint)
- [ ] Visible keyboard focus on every interactive element
- [ ] Screen-reader semantics (landmarks, headings, `aria-*` where required)
- [ ] Color is not the only carrier of meaning
- [ ] Room for translated labels (~1.5× English length is the planning floor)
- [ ] Right-to-left readiness (flow direction, label wrapping, mirrored chrome)
- [ ] Language selector path is named, even if not wired
- [ ] Glossary access from member-facing terminology
- [ ] Three vocabulary layers preserved where relevant: canonical term, public label, local institutional vocabulary

Floor doc: [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md). Component-level rules: [docs/design-language/accessibility.md](../design-language/accessibility.md).

## 9. Vocabulary scan result

Run a vocabulary scan against the seed's member-facing strings. Reference: [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) §"Regulatory-safe vocabulary" and [docs/design-language/concept-map.md](../design-language/concept-map.md).

- [ ] No `wallet / balance / currency / crypto / token / payment / payout / debt / user / account holder / dashboard / admin panel` in member-facing copy (Reject-pattern examples exempt and labeled)
- [ ] Canonical terms used consistently: **member, standing, mandate / authority, obligation, allocation, settlement, unit, position, receipt, provenance**
- [ ] Every first-use of a canonical term shows the public label or links the glossary
- [ ] No invented canonical term (new concepts route through `concept-map.md` first)
- [ ] Three vocabulary layers preserved where applicable: canonical / public / local-institutional

Findings: _______________________

## 10. Drift report summary

Summarize the seed's `DRIFT_REPORT.md` (if present) and any additional drift found against the live repo.

| Finding | Severity | Live repo source of truth | Action |
|---|---|---|---|
| | | | |

For each finding: state the drift, name the live repo source that wins, and route the correction to a separate docs PR if needed.

## 11. Unresolved human decisions

Decisions that block follow-up PRs until resolved. The seed's own `CLAUDE_CODE_BUNDLE.md` typically enumerates these; list them here so they cannot be silently absorbed by a subsequent agent.

| Decision | Owner | Why blocking | Status |
|---|---|---|---|
| | | | |

## 12. Allowed paths Claude Code may touch in this handoff

Default is **none**. List every path explicitly. Anything not on this list is forbidden.

Examples for a docs-only seed-promotion PR:

- `docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md` — create or update
- `docs/design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md` — create or update
- `docs/design/MUST_NOT_SHIP.md` — create or update
- `docs/design/CLAUDE_DESIGN_SETUP.md` — append section
- `docs/design/ICN_DESIGN_SYSTEM.md` — add references
- `docs/design/claude-design-seed/README.md` — create
- `docs/design/claude-design-seed/CHANGELOG.md` — append entry
- `docs/design/claude-design-seed/REVIEW_NOTES.md` — create or append
- `docs/design-language/accessibility.md` — targeted correction(s) only
- `docs/INDEX.md` — slot new entries under design section
- `.claude/skills/icn-commons-design/SKILL.md` — create or update

## 13. Forbidden paths

Hard non-goals. Any change to these paths inside this PR is out of scope and must be removed before merge.

- `website/src/**`
- `website/public/**`
- `web/pilot-ui/**`
- `sdk/**`
- `apps/**`
- `crates/**`
- `bins/**`
- `docs/adr/**` (ADRs are architectural decisions, not seed promotions)
- any other production code or production CSS

## 14. Verification commands

Run before opening the PR. For a docs-only handoff, the minimum set is:

```bash
# Doc structure / canon / supersession check
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml

# Diff is docs-only
git status --short
git diff --stat origin/main...HEAD
```

If the PR touches a member-facing surface (it should not, for a seed-promotion handoff), additionally use the per-PR member-surface checklist that lives alongside the design system. That checklist enumerates `pnpm test`, `pnpm typecheck`, vocabulary linter, axe-core, and visual diff against preview cards.

## 15. PR title and branch guidance

- **Branch:** `docs/claude-design-seed-review-protocol` (or seed-specific equivalent for a follow-up)
- **PR title:** `docs(design): add Claude Design seed review and handoff protocol` (or scope-specific equivalent)
- **Commit style:** ICN's `<type>(<scope>): <summary>` convention. Use `docs(design)` for seed-promotion PRs.
- **Co-author:** add the Claude Design session author if attribution is desired; otherwise omit.

## 16. Sign-off

- **Handoff prepared by:** _______________________ on YYYY-MM-DD
- **Truth-label / accessibility / vocabulary review by:** _______________________ on YYYY-MM-DD
- **Decision to proceed with promotion PR:** yes / no / hold

If no or hold, record the reason and the unresolved decision in [`claude-design-seed/REVIEW_NOTES.md`](claude-design-seed/REVIEW_NOTES.md). Do not proceed silently.

---

## Operating note for Claude Code

When Claude Code reads a handoff filled out using this template:

- Treat §6 (non-goals), §12 (allowed paths), and §13 (forbidden paths) as load-bearing constraints. They override any inferred scope from the seed's preview HTML or visual finish.
- **Do not recreate generated prototypes pixel-perfectly** unless a later, separate task explicitly asks for production UI implementation.
- The seed's polish does not relax the truth boundary. A preview card depicts a pattern; it is not the pattern's implementation.
- Promotion of any artifact requires the gates in [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3 to close green. A "candidate doctrine" row in §5 of this template is a proposal, not authorization.
- Surface findings, drift, and unresolved decisions in the PR description, not silently in commits.

---

## See also

- [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) — review gates this handoff feeds
- [MUST_NOT_SHIP.md](MUST_NOT_SHIP.md) — hard rejection floor
- [claude-design-seed/README.md](claude-design-seed/README.md) — what the seed directory is and how it is organized
- [claude-design-seed/CHANGELOG.md](claude-design-seed/CHANGELOG.md) — imported seed versions
- [claude-design-seed/REVIEW_NOTES.md](claude-design-seed/REVIEW_NOTES.md) — per-seed human review summary
- [CLAUDE_DESIGN_CONTEXT.md](CLAUDE_DESIGN_CONTEXT.md) — paste briefing for external Claude Design sessions
- [CLAUDE_DESIGN_SETUP.md](CLAUDE_DESIGN_SETUP.md) — workflow, external and local modes
- [ICN_DESIGN_SYSTEM.md](ICN_DESIGN_SYSTEM.md) — design system entry point
