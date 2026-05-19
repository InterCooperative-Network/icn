---
Status: operational
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-19
Last Updated: 2026-05-19
Purpose: Human-readable review summary for each Claude Design seed. One section per seed. Newest first. Reviewers update their own section; once a seed is promoted or retired, its section is frozen.
---

# Claude Design Seed — Review Notes

> **One sentence.** This is the file a reviewer fills out while walking the gates; the changelog records what *happened*, this file records what was *seen*.

Each seed gets one section. Section format:

```
## <Seed name and version> (<YYYY-MM-DD>)
### What is strong
### What is not canonical
### What should be proposed upstream
### What should remain seed-only
### What should be discarded
### What requires human decision
### Known drift findings
### Production-readiness caveats
### Reviewer
```

---

## ICN Commons Design System — Claude Design Seed v0.1 (2026-05-19)

> **Status banner from the bundle:** "review required · not production canonical"

### What is strong

- **Token discipline.** The three-layer raw / theme / element token system in `colors_and_type.css` is internally consistent and matches the live `website/src/styles/global.css` palette exactly. The seed CSS is intentionally narrower (token-only, no component classes) — a clean subset, not a competing definition.
- **Truth-label discipline.** The seed's persistent on-screen truth-label strip across the member shell, on-SVG caption stamped into both placeholder logo files, and the explicit "hidden chip is rejected" rule in the seed's own `MUST_NOT_SHIP.md` are well-drawn. The seven-state enumeration (`strong today`, `advancing now`, `real but maturing`, `illustrative direction`, `behind the system`, `not yet`, plus the `do not use` archive bucket) is a sharper instrument than the prior loose labeling.
- **Vocabulary fidelity.** The seed's `AUDIT_PASS_2.md` shows a clean grep against `wallet / balance / currency / crypto / token / payment / payout / debt / user / account holder / dashboard`. UI-kit copy uses **obligation / mandate / receipt / standing / settlement / position / unit / scope / allocation** consistently. The three-vocabulary-layer pattern (canonical / public / local-institutional) is demonstrated in `language-patterns.html`.
- **Member-shell mock alignment with the mobile UX spec.** Five-tab navigation in the right order, persistent acting-as/scope context, action-card contract substantially present. Gaps are bounded and named (see drift D-04).
- **Self-aware governance.** The seed ships its own `INVENTORY.md`, `PROMOTION_MAP.md`, `DRIFT_REPORT.md`, `PRODUCTION_READINESS.md`, `MUST_NOT_SHIP.md`, and `CLAUDE_CODE_BUNDLE.md`. The bundle treats itself as a proposal under review, not a finished system — that posture is exactly what makes promotion possible.

### What is not canonical

- The entire bundle is `generated seed` by default. Nothing inside it becomes canonical merely by being read into the repo. This includes every preview HTML card, every UI kit, every screenshot, every imported reference, and the seed's own governance docs.
- The placeholder logo and wordmark are explicitly marked as not-final inside the SVG itself. They remain rejected for any production surface.
- All UI kit React/JSX files are recreations of upstream Astro components. They are illustrative; production stays Astro on the website and React Native on mobile.
- Imported copies of upstream docs (`docs/design/...`, `docs/design-language/...`, `docs/mobile/...`) are reference snapshots, not authoritative. The live repo is authoritative.

### What should be proposed upstream

Landed in this PR (docs-only):

- `seed/REVIEW_PROTOCOL.md` → `docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md` (extended to cover the seven review gates the user requested)
- `seed/HANDOFF.md` + user-spec extensions → `docs/design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md` (the seed-level handoff template; the seed's original HANDOFF.md remains useful as the *per-PR* member-surface checklist for any future production-UI PR)
- `seed/MUST_NOT_SHIP.md` (7 items) + user-spec extensions → `docs/design/MUST_NOT_SHIP.md` (12-item floor)
- `SKILL.md` (seed) → `.claude/skills/icn-commons-design/SKILL.md` (edited to reference canonical repo paths, not seed paths)
- `docs/design/claude-design-seed/{README,CHANGELOG,REVIEW_NOTES}.md` — new directory, this trail

Held for separate review tracks (do not land in this PR):

- Token extensions in `colors_and_type.css` → `website/src/styles/global.css` (modes section). Decision blocker: are HC themes v1.0 or v1.x?
- Theme × mode coverage requirement → `docs/design/ACCESSIBILITY_BASELINE.md` (new section). Depends on the modes decision above.
- Rejected-patterns concept → `docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md` (appendix). Visual content; needs design owner pass.
- Seven-state truth-label band → `docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md` (existing truth-label section). Decision blocker: promote, simplify, or hold?
- Seed pattern names (`scope frame`, `mandate line`, `challenge / review / exit`, `sync state`) → `docs/design-language/concept-map.md`. Decision blocker: pattern names or canonical concepts?

### What should remain seed-only

- Every preview HTML card (`preview/*.html`) — they are showcase, not source. Pattern reference, not patterns.
- Every UI kit file (`ui_kits/website/*`, `ui_kits/member-shell/*`) — recreations, not production code. Migrating them would create a dual codebase.
- The seed's own governance docs (`seed/SEED_v0.1.md`, `seed/AUDIT_PASS_2.md`, `seed/REVIEW_PROMPTS.md`, `seed/INVENTORY.md`, `seed/PROMOTION_MAP.md`, `seed/DRIFT_REPORT.md`, `seed/PRODUCTION_READINESS.md`, `seed/CLAUDE_CODE_BUNDLE.md`) — they exist to support this seed's review workflow; the repo doesn't need them after this PR lands.
- `fonts/README.md` (offline-safe three-tier font plan) — strategy doc; once self-hosted, the strategy lives in the deployment, not the design system.

### What should be discarded

- `assets/logo-mark.svg`, `assets/logo-wordmark.svg` — placeholder marks; not for shipping anywhere.
- The faux-foundation seal example in `preview/rejected-patterns.html` — already a "reject" example; do not migrate the visual itself, only the documented rule (which is now item #10 in [MUST_NOT_SHIP.md](../MUST_NOT_SHIP.md)).
- All debug-iteration screenshots (`01-website-debug2.png`, `02-website-debug2.png`, `01-rejected-patterns-v2.png` / `v3.png`, `01-rejected-v4.png`, `website-dark.png` early iteration, `01–04-website-scroll.png`, `member-shell-1.png` / `2.png`, `website-debug.png`).

### What requires human decision

| Decision | Owner | Why blocking |
|---|---|---|
| Should HC themes (`hc-dark`, `hc-light`) be a v1.0 obligation or a v1.x stretch goal? | Matt Faherty | Determines whether the second seed-promotion PR adds CSS or just docs. |
| Should `data-mode~="large-text"` / `low-bandwidth"` ship as member preferences plumbed through the gateway, or as CSS-only opt-ins? | Architecture review | Determines API surface for the future member shell. |
| Should the seed's pattern names (`scope frame`, `mandate line`, `challenge / review / exit`, `sync state`) be promoted into [`docs/design-language/concept-map.md`](../../design-language/concept-map.md) as canonical concepts? | Design + design-language owners | Determines whether they become kernel-bound terms or stay as design-system patterns only. |
| Logo exploration — when does it start and who drives it? | Matt Faherty | Until decided, the placeholder remains rejected for production. |
| Spanish first-pass — does it run against this seed, against the live upstream website, or both in parallel? | Localization owner | Determines where translation work lands. |
| Should the seven-state truth-label enumeration be promoted into [`docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md`](../ICN_VISUAL_EXPLAINER_BIBLE.md)? | Design owner | Determines whether the bible's truth-label scheme gains the additional granularity. |

### Known drift findings

(Summarized; full table in [CHANGELOG.md](CHANGELOG.md).)

- **D-01 (HIGH, fixed in this PR):** [`docs/design-language/accessibility.md`](../../design-language/accessibility.md) claimed the site has no `:focus-visible` rules. It does — global ring + per-element selectors in [`website/src/styles/global.css`](../../../website/src/styles/global.css). Fixed at §3.3 and §5 known failures #1; `Last Reviewed:` bumped to 2026-05-19.
- **D-02 (MEDIUM, additive proposal):** seed adds `hc-dark`, `hc-light`, `data-mode~="large-text"`, `data-mode~="low-bandwidth"`. Not contradictory; not in this PR.
- **D-03 (LOW, by design):** seed CSS narrower than upstream `global.css`. No conflict.
- **D-04 (LOW, minor gaps):** member-shell mock vs [`docs/mobile/icn-mobile-ux-spec-v1.md`](../../mobile/icn-mobile-ux-spec-v1.md). Track for v0.2 — missing explicit threshold display, deadline timestamp, proof affordance, per-action sync-state chip.
- **D-05 (LOW, by design):** seed introduces pattern names not in [`concept-map.md`](../../design-language/concept-map.md). Design-system layer, not kernel layer.
- **D-06 (MEDIUM, bounded):** imported doc `Last Reviewed:` dates predate the seed by up to ~3 days. Consumers should re-check live repo for changes since each doc's `Last Reviewed:`.

### Production-readiness caveats

This seed is **ready for review**. It is **not ready for production**. Specifically:

- No axe-core run against the cards or kits
- No NVDA / VoiceOver screen-reader pass
- No on-device tap target measurement
- No 200% browser-zoom test on the kits
- HC themes defined in tokens but not threaded through the UI kits' theme toggles
- RTL demonstrated in one stress card but not threaded through chrome (nav, footer)
- Spanish first-pass not started
- No real-device test on a five-year-old phone

These are *seed* gaps. They are not blockers for the docs-only promotion PR that landed (this one). They are blockers for any future promotion of seed UI patterns into production.

### Reviewer

- **This review:** prepared 2026-05-19 by Matt Faherty (via Claude Code seed-promotion session)
- **Sign-off on docs-only promotion PR:** yes — the protocol, handoff template, must-not-ship floor, skill scaffold, seed-workflow docs, and the targeted D-01 accessibility doc correction are scope-appropriate for a docs-only first PR.
- **Sign-off on token / UI / icon / brand promotions:** deferred — each requires its own scope-locked PR and the human decisions enumerated above.
