# Open PR fan-in — 2026-05-11

Scratch note for queue inspection, #1787 verification, docs fan-in disposition, and validation record.

## PR disposition table

| PR | Branch | Draft | Category | Files touched | Recommended action |
|----|--------|-------|----------|---------------|-------------------|
| 1787 | `baseline-lock-loop` | no | **Runtime / tests** | See list below | **Merge** after benchmark disposition (comment + optional `perf-regression-ok` label per maintainer); CI green except benchmark compare gate |
| 1782 | `docs/project-coverage-matrix` | yes | docs-only | `docs/reference/project-index/project-coverage-matrix.md` | **Consolidate** into fan-in branch |
| 1783 | `docs/substrate-subsystem-maps` | yes | docs-only | `identity-crypto-map.md`, `network-gossip-map.md` | **Consolidate** |
| 1784 | `docs/middle-layer-subsystem-maps` | yes | docs-only | `ccl-map.md`, `service-hosting-map.md`, `tool-commons-map.md` | **Consolidate**; guardrails on service hosting / Tool Commons claims |
| 1785 | `docs/drift-control-maps` | yes | docs-only | `pilot-ui-current-state-map.md`, `stale-and-archived-map.md`, `website-truth-map.md` | **Consolidate** |
| 1781 | `docs/public-surface-truth-sync` | yes | **docs + website + pilot-ui** | `website/...`, `web/pilot-ui/README.md`, `web/pilot-ui/package.json` | **Consolidate** after project-index maps; run npm checks |
| 1786 | `docs/demo-fixture-handoff` | yes | docs-only | `docs/demo/GOVERNANCE_PROPOSAL_FIXTURE_HANDOFF.md` | **Update** vs #1787 + pilot fixture scope; merge or keep draft |
| 1780 | `docs/remote-work-plan` | yes | docs-only | `docs/reference/project-index/remote-work-plan.md` | **Supersede / shorten** or **close** if subsumed by fan-in + maps |

### #1787 file list (runtime)

- `docs/manual/FOUNDATIONAL_MANUAL_EXECUTABLE_BASELINE_LOOP.md`
- `icn/Cargo.toml`, `icn/Cargo.lock`
- `icn/crates/icn-baseline-lock/**` (src, tests, README, fixtures WASM)
- `icn/crates/icn-baseline-lock-guest/**`
- `icn/crates/icn-boundary/**`
- `scripts/build-baseline-lock-guest.sh`

## Benchmark triage (#1787, run `25672700166`)

- **Failing job**: Benchmarks → **Compare Against Base** → step **Check for regressions** (not benchmark execution).
- **Root cause**: [`.github/workflows/benchmark.yml`](../../.github/workflows/benchmark.yml) counts lines matching `(Performance has regressed|regressed|change:.*\+[0-9]+\.[0-9]+%)`. Criterion prints intervals like `change: [-0.5% -0.2% +0.03%]`; the positive **upper bound** matches the regex even when the next line is **No change in performance detected** → **many false positives** (workflow detection bug).
- **Secondary**: Lines with explicit `Performance has regressed` may still reflect **CI noise** or **lockfile** indirect effects; triage those separately if reproducing locally matters.
- **Bypass (do not apply unless instructed)**: PR label **`perf-regression-ok`** per workflow.
- **PR comment draft** (post to #1787 when ready): see **Appendix A** below.

## Appendix A — Draft PR comment for #1787 (benchmark gate)

The Benchmarks workflow failed only on **Compare Against Base → Check for regressions** (not on running benches). The step counts lines matching a broad regex in [`.github/workflows/benchmark.yml`](../../.github/workflows/benchmark.yml) (see `REGRESSION_COUNT` / `grep` around lines 146–177). Criterion prints confidence intervals such as `change: [-0.5% -0.2% +0.03%]`; the positive **upper bound** matches `change:.*\+[0-9]+\.[0-9]+%` even when the following line is **No change in performance detected**, which inflates the regression count (for example **79** matches on this PR). Treat most hits as **false positives** until the workflow is tightened. Any line that explicitly says **`Performance has regressed`** still deserves a quick human look; some may be **CI noise** after `Cargo.lock` churn. Merge options: add label **`perf-regression-ok`** (repo-defined bypass) or land a small follow-up PR to fix the detection regex.

## Phase 2 — Local verification (#1787)

Branch: `baseline-lock-loop` (local matches PR #1787).

| Command | Result |
|---------|--------|
| `cargo fmt --all --check` (from `icn/`) | PASS |
| `cargo clippy -p icn-boundary -p icn-baseline-lock --all-targets -- -D warnings` | PASS |
| `cargo clippy --manifest-path crates/icn-baseline-lock-guest/Cargo.toml --target wasm32-unknown-unknown --lib -- -D warnings` | PASS |
| `cargo test -p icn-baseline-lock` | PASS (24 integration tests in `baseline_lock_loop.rs`) |
| `cargo test -p icn-baseline-lock test_baseline_lock_loop -- --exact` | PASS |
| `./scripts/build-baseline-lock-guest.sh` + `git diff --exit-code ...icn_baseline_lock_guest.wasm` | PASS (fixture matches build) |

**Review invariants** (covered by tests + `projector.rs` / `host.rs`; do not weaken): vote signer vs `member_pubkey`; input vs executed `module_hash`; duplicate vote semantics; `MissingReservation` vs `ReservationConsumed`; non-vote receipt signer authority (`BaselineFixtureAuthority`).

**GitHub reviews**: no `APPROVED` / `CHANGES_REQUESTED` blocking state in API snapshot; Codex/Copilot commented; human threads empty in summary.

## Phase 3 — Docs validation

| Check | Result |
|-------|--------|
| `python3 docs/scripts/doc_control_check.py --strict` | **PASS** (existing repo warnings only; no new failures) |
| `git diff --check` | **PASS** |
| Overclaiming `git grep` on merged `docs/reference/project-index`, `docs/demo/GOVERNANCE…`, `website/`, `web/pilot-ui/README.md` | **Reviewed** — hits are red-line / “do not claim” / checklist contexts, not new outward claims |
| `website/`: `npm ci` + `npm run build` | **PASS** |
| `web/pilot-ui`: `npm ci` + `npm run test` (Jest) | **PASS** (40 tests) |
| `web/pilot-ui`: `npm run test:e2e` / `npm run test:a11y` | **NOT RUN** — Playwright `webServer` failed with `OSError: [Errno 98] Address already in use` (port conflict on this host). Re-run locally with a free port or stop the conflicting process. |

**Edits on fan-in** (beyond raw merges): shortened [`remote-work-plan.md`](../reference/project-index/remote-work-plan.md) to a superseded pointer; extended [`project-index/README.md`](../reference/project-index/README.md) map table; linked #1787 from [`GOVERNANCE_PROPOSAL_FIXTURE_HANDOFF.md`](../demo/GOVERNANCE_PROPOSAL_FIXTURE_HANDOFF.md); allowlisted `docs/handoffs/` in [`registry.toml`](../registry.toml); aligned `GOVERNANCE` YAML Status with `docs/demo/` registry defaults.

## Canonical constraints (edit pass)

- Phase 0–1 complete; Phase 2 in progress / partner-bound (not “complete”).
- NYCN = partnership track unless canonical doc explicitly says “pilot”.
- No overclaim: live federation, production readiness, full mobile UX, Tool Commons live, service hosting implemented, formal pilot, fully fixture-backed demo — unless code/tests back it.
- Vocabulary: obligation, allocation, settlement, unit, position, receipt, provenance; avoid misleading payment/currency/balance/wallet in **current-facing** copy.

## Final disposition summary

| PR | Recommendation |
|----|----------------|
| **#1787** | **Merge** when maintainers accept benchmark disposition (PR comment + optional `perf-regression-ok`, or workflow fix follow-up). Runtime verification passed locally on `baseline-lock-loop`. |
| **#1782–#1785, #1781, #1786, #1780** | **Close as superseded** by single PR from branch `docs/open-pr-fanin-1780-1786` after review (link this handoff + new PR). #1780 content folded into short `remote-work-plan.md` pointer. |
| **Fan-in PR (new)** | Open from `docs/open-pr-fanin-1780-1786` against `main` with four commits (see git log). Do **not** push until you run `/push` or your usual gate. |

**Merge order suggestion:** land **#1787** first if you want demo docs to cite merged baseline-lock on `main`; otherwise merge fan-in first and rebase #1787 doc links—either order works because links use PR URL for #1787 until merged.
