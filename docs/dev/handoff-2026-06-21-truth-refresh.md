# Session Handoff — 2026-06-21 — Truth-Layer Stale-State Refresh (after #2128–#2133)

**Topic:** Stale-state / truth-layer refresh pass plus narrow guards so the canonical docs do not silently drift again.
**Branch:** `docs/state-truth-refresh-2128-2133`
**Base:** `origin/main @ b63dc13c` (`fix(project-index): make file-record check convergent`)
**Closes:** none via keyword (closed #2126 manually with evidence). **Refs:** #2128, #2130, #2131, #2132, #2133, #2129, #2047, #2112, #2113, #2114, #1868.

This pass is **docs/control-plane + generated-orientation + CI tooling only.** No runtime Rust, no schema, no contract URN, no ADR, no RFC, no kernel/gateway API change, no auth-decision change, no K3s/DNS/GitHub-settings mutation. The one new script (`scripts/check-state-lag.py`) is an advisory checker, not runtime.

---

## Check outputs (recorded per the task)

Baseline on `main` (`b63dc13c`) and after this branch's edits, run from the repo root:

| Check | Baseline (main) | After edits (this branch) |
|---|---|---|
| `python3 scripts/check-agent-context-spine.py` | exit 0 (131 nodes / 92 edges) | exit 0 |
| `python3 scripts/check-claude-plugin.py` | exit 0 | exit 0 |
| `python3 scripts/check-claude-plugin-root-resolution.py` | exit 0 (9/9) | exit 0 |
| `python3 scripts/generate-live-state-overlay.py --check --no-gh` | exit 0 (13 sections) | exit 0 (**14** sections — added `architecture_freshness`) |
| `python3 scripts/generate_repo_record.py --repo icn=. --check` | exit 0 | exit 1 (**stale-warn by design** — this branch edits inventoried files; refreshed by a separate generated-only commit, non-blocking) |
| `python3 docs/scripts/route_inventory.py --check` | exit 0 (287 routes) | exit 0 |
| `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml` | exit 0 (65 warnings) | exit 0 (65-warning baseline unchanged) |
| `python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .` | exit 1 (`04-eight-primitives` + `18-institution-primitives` STALE) | **exit 0 (all fresh)** |
| `python3 scripts/check-state-lag.py` (new) | exit 1 (caught #2128 lag in the then-newest #2129 block) | **exit 0** |

The `generate_repo_record.py --check` exit-1 is the documented snapshot-lag warning (`docs/ci/GENERATED_TRUTH_DRIFT.md`), not a regression: the file-record inventories per-file SHAs, so it drifts on any PR touching an inventoried file and is refreshed on a cadence, not per-PR.

## What was refreshed

- **`docs/STATE.md`** — new top sync block for the post-#2129 window (#2128 Agent Context Spine v0, #2130 file-record refresh, #2131 live-state overlay, #2132 generated-truth drift gate, #2133 convergent file-record check). It **corrects the stale claim** in the #2129 block that "#2128 is OPEN, not merged / not present on `main`" (#2128 merged as `41c7082b`). Truth boundaries preserved: every artifact is `Canonical: no`, orientation-only, not a truth root; no production / live-federation / formal-pilot / Phase-2-complete / entity-auth-enforced / `/auth/verify`-trusted-issuance claim.
- **`docs/PHASE_PROGRESS.md`** — one compact post-June-14 truth/control-plane note. **Phase posture unchanged** (Phase 2 ⏳, partner-bound).
- **`docs/ARCHITECTURE.md` §04 + §18 (the only two stale freshness sections), with real corrections:**
  - §4.4 WASM overclaimed "Fuel metering enforced by ICN runtime. Deterministic imports (no system time…)" → corrected to experimental / non-deterministic, matching `icn-compute/src/wasm_executor.rs` (`Engine::default()`, coarse per-call counter, wall-clock `SystemTime::now()`) and the already-corrected §11.
  - §18 "Institution-in-a-Box" charter "written in TOML" → corrected to YAML (live v1 interface: `serde_yaml` `CclDocument::from_yaml`; `contracts/templates/*.yaml`; Phase 1 canonical).
  - **`docs/freshness.toml`** §04/§18 bumped to 2026-06-21 with code-anchored `review_note`s. These are **earned bumps** — both sections held a real inaccuracy that was fixed first; not blind date-bumps. No other section touched.
- **Issue hygiene** — #2126 closed as completed (satisfied by on-main #2130/#2133) + `epic:arch-invariants` label added; #2112/#2113 labeled `epic:arch-invariants` + commented (narrowed/confirmed-open); #2114 commented (invariant-index gap remains); #2047 commented (audit reduced to §04/§18, now reviewed-and-corrected on this branch — recommend close on merge, **not closed** because freshness is clean on-branch, not yet on `main`); #1868 commented (body stale; remaining target = broad `governance:write` fallback retirement, recommend retitle/successor).

## What remains stale / open

- `generate_repo_record.py --check` warns stale on this branch (expected; refresh is a separate generated-only commit).
- **#2047** stays OPEN until this branch merges (freshness clean on-branch only). Recommend close-on-merge.
- **#2112** stays OPEN: per-route proof-level (L0–L8) tagging is the one unmet acceptance criterion (inventory + CI check exist).
- **#2113** (role-based `icnctl` command map) and **#2114** (invariants catalog) remain unimplemented real gaps.
- **#1868** body is stale; the live closure target is broad-fallback retirement (gated on the #2080 token-issuance lane).
- Flagged follow-up (out of scope here): `docs/status.toml` governance evidence still says "Charter system (TOML format …)" — same stale-charter-format wording as the §18 fix, in a separately-reviewed doc.

## Guards added (recurrence prevention)

- **`scripts/check-state-lag.py`** (new, stdlib) — canonical-state lag check. Scans the **newest** `docs/STATE.md` sync block (STATE.md is append-only; history is exempt) for any PR/issue asserted "open / not merged / not on `main`" that git history shows merged, and warns. This is exactly the #2128 recurrence; the check was red on the pre-fix STATE.md and is green after the correction. Wired into `.github/workflows/generated-truth.yml` as an **observational** step (exit 1 → `::warning::`, ≥2 → fail), and listed in `docs/ci/GENERATED_TRUTH_DRIFT.md`.
- **Live-state overlay freshness summary** — `scripts/generate-live-state-overlay.py` gained an `architecture_freshness` section (#14) that surfaces the exact stale `docs/ARCHITECTURE.md` sections at session start, so staleness is visible during grounding instead of rotting silently. `--check` updated (14 required sections); overlay stays on-demand with no committed snapshot.
- **Generated-truth artifact rule** — `docs/ci/GENERATED_TRUTH_DRIFT.md` now states that every committed generated artifact must either be covered by the drift gate or be explicitly declared on-demand/no-snapshot, and records that the PR-stable deterministic checks are now a confirmed-clean baseline (promotion to blocking is a deliberate, un-taken branch-protection/cadence decision; `generate_repo_record.py --check` is explicitly **not** a promotion candidate because it lags by design; doc freshness stays advisory).

## Project-direction blockers (unchanged by this pass)

- **#2080** — trusted positive token issuance (production `/auth/verify` path). PR1 (#2111 entity-id claim seam) + PR2 (#2121 `DenyUntilWired` `TokenAuthoritySource` seam) landed fail-closed and unwired; the positive Membership-backed source remains the open security-sensitive step.
- **#2081** — treasury entity-auth enforcement cutover (make `require_entity_access` enforce instead of observe). Still OBSERVE-mode.
- **#2041** — human accessibility (screen-reader + keyboard + zoom + contrast + switch) pass. Owed; member-shell / July Demo Candidate 0.1 is **not** organizer/pilot-ready until this closes.
- **#1703** — NYCN organizer presentation → pilot formalization → first operator rehearsal. The Phase 2 human gate (in the partner `nycn` repo). Unchanged.
- **#1955** — CI Test job sled-storage "No space left on device" flake. Re-run-once class, not a real failure.

## Explicit non-claims

This pass does **NOT** claim: production readiness; live federation; a formal NYCN pilot; Phase 2 completion; that entity-aware auth is enforced; that `/auth/verify` trusted positive issuance is complete; that member-shell is organizer/pilot-ready; OpenAPI completeness; or that any generated artifact (spine, file-record, overlay, route inventory) is a truth root. Canonical state remains `docs/STATE.md` + `docs/PHASE_PROGRESS.md`.
