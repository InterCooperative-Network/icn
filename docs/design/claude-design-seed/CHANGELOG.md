---
Status: operational
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-19
Last Updated: 2026-05-19
Purpose: Track imported Claude Design seed versions and their review outcomes. One entry per seed produced for ICN. Newest first.
---

# Claude Design Seed Changelog

> **One sentence.** A seed gets one row when it arrives, one update when its review closes, and no edits to its row after that.

This is the governance trail for Claude Design seeds. It records what was produced, what was promoted, what was held, and what was rejected — without copying seed contents into the repo. The full review summary for each seed lives in [REVIEW_NOTES.md](REVIEW_NOTES.md).

Entry format:

```
## <YYYY-MM-DD> — <Seed name and version>
- Bundle URL: <url or path>
- Status: <review required · in review · promoted (partial) · promoted (full) · rejected · archived>
- Summary of contents:
- Promotion candidates (linked to PRs once opened):
- Held / rejected items:
- Drift findings:
- Production-readiness caveats:
- Human decisions still required:
- See also: REVIEW_NOTES.md §<seed slug>
```

---

## 2026-05-19 — ICN Commons Design System — Claude Design Seed v0.1

- **Bundle URL:** Claude Design handoff bundle (local extract under job scratch dir; not committed to repo)
- **Status:** review required · not production canonical
- **Summary of contents:**
  - Generated seed governance: `seed/SEED_v0.1.md`, `seed/REVIEW_PROTOCOL.md`, `seed/HANDOFF.md`, `seed/MUST_NOT_SHIP.md`, `seed/REVIEW_PROMPTS.md`, `seed/AUDIT_PASS_2.md`, `seed/INVENTORY.md`, `seed/PROMOTION_MAP.md`, `seed/DRIFT_REPORT.md`, `seed/PRODUCTION_READINESS.md`, `seed/CLAUDE_CODE_BUNDLE.md`
  - Generated tokens: `colors_and_type.css` (three-layer raw / theme / element, with seed-extended `hc-dark`, `hc-light`, and `data-mode~="large-text"` / `data-mode~="low-bandwidth"`)
  - Generated UI kits: `ui_kits/website/` (Astro component recreations as inline React/JSX) and `ui_kits/member-shell/` (illustrative direction for the unbuilt member shell)
  - Generated preview cards: 45 self-contained HTML cards across Brand, Colors, Type, Spacing, Components groups, plus the `canonical-vs-generated.html` boundary card
  - Generated assets: placeholder logo mark and wordmark (SVG with stamped "PLACEHOLDER · NOT FINAL" caption), 11-icon institutional set
  - Imported upstream docs (for offline reference, may be stale): the ICN design doctrine docs and a snapshot of `website/src/`
  - Screenshots: 23 PNGs from the session (debug iterations to be discarded; the keep-set is illustrative)
- **Promotion candidates landed in this PR (seed → upstream):**
  - `seed/REVIEW_PROTOCOL.md` → [`docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md`](../CLAUDE_DESIGN_REVIEW_PROTOCOL.md) (canonical-vs-generated boundary, review gates, promotion/rejection rules)
  - `seed/HANDOFF.md` + user-spec extensions → [`docs/design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md`](../CLAUDE_DESIGN_HANDOFF_TEMPLATE.md) (seed-level handoff template, not the per-PR member-surface checklist)
  - `seed/MUST_NOT_SHIP.md` (seven items) plus user-spec additions → [`docs/design/MUST_NOT_SHIP.md`](../MUST_NOT_SHIP.md) (12-item rejection floor)
  - `SKILL.md` (seed) → [`.claude/skills/icn-commons-design/SKILL.md`](../../../.claude/skills/icn-commons-design/SKILL.md) (skill scaffold, edited to point at canonical repo paths rather than seed paths)
  - Seed workflow documentation → [`docs/design/claude-design-seed/README.md`](README.md), this changelog, and [`REVIEW_NOTES.md`](REVIEW_NOTES.md)
- **Promotion candidates held (separate review track required):**
  - Seed token extensions in `colors_and_type.css` (`hc-dark`, `hc-light`, `data-mode~="large-text"`, `data-mode~="low-bandwidth"`) — additive proposal; needs human decision on whether HC themes are a v1.0 obligation or a v1.x stretch goal
  - `data-mode` mechanism for member preferences — needs architecture review (CSS-only opt-in vs gateway-plumbed member preference)
  - Seed pattern names ("scope frame", "mandate line", "challenge / review / exit", "sync state", seven-state truth-label enumeration) — needs design-language owner sign-off before promotion into [`docs/design-language/concept-map.md`](../../design-language/concept-map.md)
  - Seven-state truth-label enumeration → potential appendix in [`docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md`](../ICN_VISUAL_EXPLAINER_BIBLE.md) (separate PR)
  - Rejected-patterns appendix → potential appendix in [`docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md`](../ICN_VISUAL_EXPLAINER_BIBLE.md) (separate PR)
- **Held / rejected (seed-only or discard):**
  - Placeholder logo mark and wordmark — remains rejected for production until logo direction is taken
  - All UI kit React/JSX recreations — production stays Astro for the website and React Native for mobile; recreations would create a dual codebase
  - All preview HTML cards — showcase only, never source
  - All screenshots — artifacts only, never source
  - All bundled imports of upstream docs — trust the live repo; the seed's copies may be stale
- **Drift findings (from seed `DRIFT_REPORT.md`):**
  - **D-01 (HIGH, actively misleading):** [`docs/design-language/accessibility.md`](../../design-language/accessibility.md) claimed at multiple points that the site had no `:focus-visible` rules. Upstream [`website/src/styles/global.css`](../../../website/src/styles/global.css) has had a global `:focus-visible` ring (2px teal outline, 3px offset, per-element selectors for `a`, `button`, `[role="button"]`, `input`, `textarea`, `select`, `summary`, plus a skip link) since well before the seed was produced. **Fixed in this PR** — language updated to reflect current repo reality at the two affected locations (§3.3 and §5 known failures #1).
  - **D-02 (MEDIUM, additive):** seed CSS extends upstream tokens with `hc-dark`, `hc-light`, `data-mode~="large-text"`, and `data-mode~="low-bandwidth"`. Not a contradiction — these are proposed extensions for a separate review track.
  - **D-03 (LOW, by design):** seed `colors_and_type.css` is token-only; upstream `global.css` is token + components. No conflict; the seed would add token rows, not replace.
  - **D-04 (LOW, minor gaps):** member-shell mock vs [`docs/mobile/icn-mobile-ux-spec-v1.md`](../../mobile/icn-mobile-ux-spec-v1.md) — explicit threshold display, deadline timestamp, proof affordance on closed proposals, sync-state chip per action — track as v0.2 follow-up.
  - **D-05 (LOW, by design):** seed introduces pattern names ("scope frame", "mandate line", etc.) not in concept-map. Design-system patterns, not kernel concepts. Promotion requires authorization.
  - **D-06 (MEDIUM, bounded ~3 days):** imported doc `Last Reviewed:` dates predate the seed by up to ~3 days. Consumers should re-read the live repo for changes after the relevant `Last Reviewed:` date on each doc.
- **Production-readiness caveats (from seed `PRODUCTION_READINESS.md`):**
  - Theme coverage: 4 themes defined; HC themes shown only in `global-readiness.html`, not threaded through the UI kits
  - Accessibility: pattern correct; no axe-core, no NVDA/VoiceOver pass, no device verification, no 200% browser-zoom test
  - Vocabulary: clean per the seed's audit
  - Truth-label discipline: strong — persistent strip on the member shell, on-SVG caption on the logo, rule documented
  - Language / RTL: English-only first pass; RTL demonstrated in one stress card; not threaded through UI kits
  - Mobile readiness: concept-strong, hardware-verification absent
  - **Verdict:** ready for review, not for production
- **Human decisions still required:**
  - HC themes (`hc-dark` / `hc-light`) — v1.0 obligation or v1.x stretch? (owner: Matt Faherty)
  - `data-mode` mechanism — CSS-only opt-in, or member preference plumbed through the gateway? (owner: architecture review)
  - Seed pattern names — promote into [`docs/design-language/concept-map.md`](../../design-language/concept-map.md) as canonical concept ids, or keep as design-system pattern names only? (owner: design + design-language owners)
  - Logo exploration — when does it start, who drives it? (owner: Matt Faherty)
  - Spanish first-pass — against this seed, against the live upstream website, or both in parallel? (owner: localization owner)
- **See also:** [REVIEW_NOTES.md §icn-commons-design-system-v0.1](REVIEW_NOTES.md#icn-commons-design-system--claude-design-seed-v01-2026-05-19)
