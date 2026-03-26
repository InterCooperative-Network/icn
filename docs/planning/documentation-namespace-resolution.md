---
Status: draft
Canonical: no
Last Reviewed: 2026-03-26
---

# Documentation namespace resolution (plan)

**Purpose:** Record a minimal, explicit plan for overlapping `docs/` directories. No moves are executed in this document; it authorizes future tranches.

## `plans/` vs `planning/`

| Target canonical namespace | `docs/planning/` |
|----------------------------|------------------|
| Rationale | Name matches maintenance guide mental model (“planning”); `superpowers/plans/` remains agent sprint artifacts, distinct from repo planning. |
| Stay temporarily | All existing files remain in place until a tranche adds `superseded_by` / moves. |
| First safe moves (candidates) | New planning docs go under `docs/planning/` only. Next migration tranche: date-sort `docs/plans/*.md`, move **closed** plans into `docs/archive/YYYY/plans/` (preserve basename), leave a one-line stub in `docs/plans/` with link to archive path if external links exist. |
| Supersession / redirects | Stub file or top note: `Superseded-by: ../planning/<file>` (or archive path). No URL redirects (static repo). |

## `ops/` vs `operations/` vs `guides/operations/`

| Target canonical namespace | `docs/guides/operations/` for runbooks; `docs/operations/` for deeper ops architecture notes (if any); treat `docs/ops/` as **legacy alias** to drain. |
| Rationale | Single contributor-facing entry: “operations guides” already indexed from `INDEX.md`. |
| Stay temporarily | Do not move until each file is classified (runbook vs design). |
| First safe moves (candidates) | None in this tranche. Next: inventory `docs/ops/*.md` and `docs/operations/*.md`; duplicate titles merged into one canonical file under `guides/operations/` with `superseded_by`. |
| Supersession / redirects | Add `README.md` in `docs/ops/` stating drain-only status and pointing to `guides/operations/`. |

## Other collisions (watch)

| Pair | Handling |
|------|----------|
| `docs/api/` vs `docs/reference/api/` | Keep both; API **reference** canonical under `reference/api/`; `docs/api/` reserved for generated OpenAPI and checkouts—link from INDEX only. |
| `docs/state/` vs `docs/status/` vs `docs/STATE.md` | Canonical project state: `docs/STATE.md` (root). `status/` and `state/` are supplemental snapshots; do not duplicate narrative “current status” without dating. |

## Enforcement alignment

Validator orphan hints for explicit registry rows use `docs/plans/`, `docs/planning/`, `docs/onboarding/` (see `registry.toml` `[control.enforcement]`). Expand to `docs/development/` later when ready for noise.

## Tranche 3 — first structural moves (**executed in branch** 2026-03-26)

The batch below was applied with `git mv` plus stubs/READMEs; dated `docs/plans/*` and `docs/ops/SLO.md` were not moved.

Concrete, **high-confidence** candidates only. Each move should be `git mv`, one PR, with a stub or `superseded_by` where links may exist.

### A. `plans/` vs `planning/`

| Item | Choice |
|------|--------|
| Target canonical namespace for **new** work | `docs/planning/` |
| First move batch | `docs/plans/.scratch/ccl-disputes.md`, `constitution-rights.md`, `economics-treasury.md`, `governance-receipts.md`, `human-interface.md`, `security-ops.md` → `docs/archive/2026/plans-scratch/` (preserve basename) |
| Remain in place for now | All dated `docs/plans/2026-*.md` and `agent-teams-launch-prompt.md` until reviewed for archive vs `planning/` promotion |
| Redirect / supersession | Optional one-line stub files at old paths linking to `../../archive/2026/plans-scratch/<name>.md` **only if** external links hit the old URLs |

### B. `ops/` vs `operations/`

| Item | Choice |
|------|--------|
| Target canonical namespace for **runbooks** | `docs/guides/operations/` (already linked from INDEX) |
| First move batch | `docs/ops/runbooks/*.md` (8 files) → under `docs/guides/operations/runbooks/` (recreate tree), then `docs/ops/runbooks/README.md` → stub pointing to new location |
| Remain in place for now | `docs/ops/SLO.md` and all of `docs/operations/` until classified (SLO vs deployment narrative) |
| Contributor guidance | Add `docs/ops/README.md` drain notice in tranche 3 (after moves): “Runbooks moved to `guides/operations/runbooks/`” |

### C. Third collision — `docs/state/` vs `docs/STATE.md`

| Item | Choice |
|------|--------|
| Canonical project state | `docs/STATE.md` (root) — already marked canonical |
| First change | Add `docs/state/README.md` stating supplemental snapshots only; link to `../STATE.md` |
| No file moves required in tranche 3 | Yes |
