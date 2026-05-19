---
Status: operational
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-19
Last Updated: 2026-05-19
Purpose: Repository-facing explanation of the Claude Design seed workflow. Records what a seed is, what canonical truth is, and how the two relate without contaminating each other.
---

# Claude Design Seed — Repository Workflow

> **One sentence.** A seed is a generated proposal for ICN design; canonical truth lives in the upstream repo; this directory is where seeds wait while review decides which pieces of them, if any, are promoted.

This README is the entry point for anyone — human or agent — encountering the `docs/design/claude-design-seed/` directory for the first time. It explains the workflow, the boundaries, and what changes when a new seed arrives.

---

## 1. What a seed is

A **Claude Design seed** is the output of a Claude Design session: a packaged bundle that typically contains generated markdown, HTML preview cards, CSS token files, UI kit recreations, illustrative screenshots, governance docs (the seed's own `INVENTORY.md`, `PROMOTION_MAP.md`, `DRIFT_REPORT.md`, `PRODUCTION_READINESS.md`, `MUST_NOT_SHIP.md`, and a `CLAUDE_CODE_BUNDLE.md` handoff), and sometimes imported copies of upstream ICN repo docs for offline reference.

Each seed has a version (e.g. `v0.1`) and a status banner ("review required · not production canonical" by default). The seed's own README sets the framing; this directory sets the rules for what the repo does with it.

## 2. What a seed is not

A seed is **not**:

- the canonical design system
- a brand decision
- an implementation
- evidence that any depicted capability is shipped
- a sanctioned source of CSS, components, or assets for production paths
- a place from which to import code into `website/src/`, `web/pilot-ui/`, or `sdk/`

A seed may *contain* candidate doctrine that, after review, becomes canonical via a separate PR. But until that review closes, the seed is seed-only.

## 3. Where canonical truth lives

Canonical ICN design truth is the union of:

- `Canonical: yes` rows in [`docs/INDEX.md`](../../INDEX.md)
- `canonical_doc_paths` in [`docs/registry.toml`](../../registry.toml) (the doc control plane)
- the design subsystems entry point [ICN_DESIGN_SYSTEM.md](../ICN_DESIGN_SYSTEM.md)
- the doctrine docs it indexes ([ACCESSIBILITY_BASELINE.md](../ACCESSIBILITY_BASELINE.md), [CONTENT_STYLE_GUIDE.md](../CONTENT_STYLE_GUIDE.md), [ICN_VISUAL_SYSTEM.md](../ICN_VISUAL_SYSTEM.md), [ICN_VISUAL_EXPLAINER_BIBLE.md](../ICN_VISUAL_EXPLAINER_BIBLE.md), the design-language docs under [`docs/design-language/`](../../design-language/), the mobile UX spec at [`docs/mobile/icn-mobile-ux-spec-v1.md`](../../mobile/icn-mobile-ux-spec-v1.md), the member-shell spec at [`docs/spec/member-shell-v0.md`](../../spec/member-shell-v0.md))
- the ADRs that bind design (currently 0026, 0027, 0028, 0032, 0033)
- the production code in `website/src/`, `web/pilot-ui/`, and `sdk/` — for anything that has actually shipped

The seed directory is not on this list and never automatically joins it.

## 4. How to review a seed

The protocol is [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](../CLAUDE_DESIGN_REVIEW_PROTOCOL.md). The shape of the handoff is [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](../CLAUDE_DESIGN_HANDOFF_TEMPLATE.md). The rejection floor is [MUST_NOT_SHIP.md](../MUST_NOT_SHIP.md).

Workflow:

1. **Receive the bundle.** Download or unpack it locally. Do not commit the raw bundle into the repo.
2. **Read the bundle's authoritative handoff docs first.** In order: `seed/CLAUDE_CODE_BUNDLE.md`, `seed/INVENTORY.md`, `seed/PROMOTION_MAP.md`, `seed/DRIFT_REPORT.md`, `seed/PRODUCTION_READINESS.md`, `seed/MUST_NOT_SHIP.md`. Open preview HTML last, not first — polish is the distortion field.
3. **Fill in a handoff template** from [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](../CLAUDE_DESIGN_HANDOFF_TEMPLATE.md). Record every promotion candidate, every non-goal, every allowed/forbidden path.
4. **Walk the review gates** in [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](../CLAUDE_DESIGN_REVIEW_PROTOCOL.md) §3 against each named artifact. Record findings in [REVIEW_NOTES.md](REVIEW_NOTES.md).
5. **Append a row** to [CHANGELOG.md](CHANGELOG.md) summarizing the seed version, the artifacts, the gate outcomes, and the human decisions still required.
6. **Open scope-locked promotion PRs.** Each `candidate doctrine` row becomes its own small PR against the canonical path — never a bulk import.

## 5. How to promote pieces of a seed

Promotion is always:

1. Done by a normal repo PR against the appropriate canonical path. **Never** by adding files under `claude-design-seed/` and citing them as authoritative.
2. Scope-locked: one promotion per PR, or one tightly-related cluster. The first seed-promotion PR (this one) is the protocol/handoff/must-not-ship scaffold itself — every subsequent promotion is its own PR.
3. Reviewed against the [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](../CLAUDE_DESIGN_REVIEW_PROTOCOL.md) gates with findings recorded in [REVIEW_NOTES.md](REVIEW_NOTES.md).
4. Documented with the truth label the artifact carries forward.

What gets promoted is the *idea or pattern*, not the seed file itself. The seed file remains at its original path; the canonical path gets the cleaned, repo-style version.

## 6. What must stay seed-only

By default, the following stay seed-only forever:

- placeholder logo, wordmark, and brand marks
- React / JSX recreations of Astro components (production stays Astro for the website, React Native for mobile)
- preview HTML cards (they are showcase, not source)
- screenshots (they are artifacts, not source)
- seed-internal governance docs (`SEED_v*.md`, `AUDIT_PASS_*.md`, `REVIEW_PROMPTS.md`, the seed's own `INVENTORY.md` / `PROMOTION_MAP.md` / `DRIFT_REPORT.md` / `PRODUCTION_READINESS.md` / `CLAUDE_CODE_BUNDLE.md`)
- bundled imports of upstream docs (they go stale; trust the live repo)

A seed-only artifact may be referenced from [REVIEW_NOTES.md](REVIEW_NOTES.md) for context. It may not be cited from canonical docs.

## 7. How Claude Design and Claude Code work together

| Role | Where | Job |
|---|---|---|
| **Claude Design (external)** | claude.ai or any Claude Design session | Produce seeds — visual artifacts, governance docs, illustrative direction. Output is generated seed by default. |
| **Human reviewer** | this repo | Walk gates, fill handoff template, write review notes, decide what is promoted. |
| **Claude Code** | this repo, via the [icn-commons-design](../../../.claude/skills/icn-commons-design/SKILL.md) skill and the [icn-design-advisor](../../../.claude/agents/icn-design-advisor.md) agent | Apply review protocol to the seed, draft docs-only promotion PRs, enforce the rejection floor. Does **not** recreate generated prototypes pixel-perfectly without explicit later instruction. |

The pattern is: Claude Design generates → human reviews → Claude Code drafts the docs-only promotion PR → human merges. UI implementation is a separate, later track with its own scope and approval.

## 8. Directory layout

```
docs/design/claude-design-seed/
├── README.md            # this file
├── CHANGELOG.md         # imported seed versions, one entry per seed
└── REVIEW_NOTES.md      # human review summary per seed
```

The bundle's preview HTML, UI kits, screenshots, and assets are **not** copied into this directory. They live wherever the bundle was extracted (typically outside the repo, under a job/scratch directory). What lives here is the *governance trail*: the changelog of seeds and the review notes that explain why each piece was promoted or held.

When a seed *is* archived for record (rare — only when its review notes need to cite seed-internal paths), it goes under `docs/design/claude-design-seed/archive/<seed-version>/` and is treated as `historical`. Default behavior is not to archive — the bundle URL and the review notes are sufficient record.

## 9. See also

- [CLAUDE_DESIGN_REVIEW_PROTOCOL.md](../CLAUDE_DESIGN_REVIEW_PROTOCOL.md) — gates
- [CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](../CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) — handoff shape
- [MUST_NOT_SHIP.md](../MUST_NOT_SHIP.md) — hard rejection floor
- [CLAUDE_DESIGN_CONTEXT.md](../CLAUDE_DESIGN_CONTEXT.md) — paste briefing for external Claude Design sessions
- [CLAUDE_DESIGN_SETUP.md](../CLAUDE_DESIGN_SETUP.md) — external vs local mode workflow
- [ICN_DESIGN_SYSTEM.md](../ICN_DESIGN_SYSTEM.md) — canonical design system entry point
- [CHANGELOG.md](CHANGELOG.md) — imported seed versions
- [REVIEW_NOTES.md](REVIEW_NOTES.md) — human review summary per seed
