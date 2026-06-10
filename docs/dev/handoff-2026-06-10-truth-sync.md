---
Status: descriptive
Authority: session
Canonical: no
Last verified: 2026-06-10
---

# Session Handoff — 2026-06-10 — canonical truth-sync through #2016

## Session Goal

Bring `docs/STATE.md`, `docs/PHASE_PROGRESS.md`, and `docs/status.toml` back to describing `origin/main` truthfully. The canonical docs were last truth-synced by #1902 (2026-05-22, through #1901); 91 commits merged since, plus a previously unrecorded 17-PR window from 2026-05-15 → 2026-05-17 that #1902 missed because it verified an external review brief (#1875 → #1901) rather than sweeping the full git window.

## Decisive Test

Same as the 2026-05-22 handoff: if a reader trusted STATE.md + PHASE_PROGRESS.md to plan the next PR, would they plan the right PR or the wrong PR?

Before this session: the wrong PR, demonstrably. A candidate evaluation run against the 2026-05-22 docs produced "migrate the charter handler family off broad governance:write (#1868 step 2)" and "first fixture rehearsal (#1838/#1839/#1840)" as live candidates — both already merged on main (#1918 et al.; #1845/#1846/#1848).

After this PR: STATE.md, PHASE_PROGRESS.md describe `origin/main` through `9012ba5c` (#2016); status.toml re-verified as still accurate without changes.

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection this session -->

### `main` HEAD at session time
`9012ba5c deps(security): bump shell-quote 1.8.3 -> 1.8.4 in examples/mobile-app (#2016)`

### Branch this PR opens on
`docs/truth-sync-2026-06-10` — created off `origin/main` at `9012ba5c` via the icn-dev worktree model (docs/dev/AGENT_WORKTREE_POLICY.md).

### Verified high-claim items on `origin/main` at `9012ba5c`

| Claim | Verification | Verdict |
|-------|-------------|---------|
| governance:write decomposition executed | `grep -c require_any_scope icn/apps/governance/src/http/handlers.rs` → 42; seven class-scope/broad-fallback pairs (charter 4, proposal 8, steward 1, federation 1, meeting 12, activity 8, comment 5); zero bare `require_scope(..., "governance:write")` mutation gates | Present; broad fallback still accepted everywhere — NOT retired |
| MandateGate | `icn/apps/governance/src/mandate_gate.rs` exists; wired in `configure.rs`, `manager.rs`, `icn-gateway/src/server.rs`; PRs resolved via `gh api commits/<sha>/pulls`: #1927 (context guard), #1928 (MandateGrantRef), #1929 (decision-receipt attestation) | Present as code; authority migration NOT complete |
| EffectDispatchEvidence durability | #1990 commit `f4c69ed7` — dispatcher ExecutionRecord as durable write-ahead recovery handle; closes #1987 (CLOSED, verified via `gh`) | Present |
| Live 13/13 receipt chain | #1985 commit `b3248133` — `icnctl audit verify --token` 13/13 on live local daemon; dev-gated `ICN_DEV_SELF_TRUST` seed; one-command rehearsal at `demo/scripts/local_receipt_chain_13of13_rehearsal.sh` (#1997) | Present; local/dev-gated proof, not a partner deployment |
| Slice A fixture rehearsals | #1838/#1839/#1840 all CLOSED (verified via `gh`); closed by #1845/#1848/#1846 (2026-05-15 → 2026-05-16) | Complete — was misrecorded as open candidate in the 2026-05-22 sync block |
| TrustThreshold fail-open | #1911, #1916 (closes #1913 — CLOSED, verified) | Closed |
| Workspace member count | `tomllib` parse of `icn/Cargo.toml`: 44 members = 37 crates + 3 bins + 4 apps; `icn-baseline-lock-guest` excluded | status.toml "44" still correct; whitepaper "38 crates" stale → fixed to 37 in this PR |
| Appliance negative firstboot smoke | `git log --since=2026-05-22 -- deploy/appliance/` → empty; no tracking issue found | STILL UNVERIFIED — gap unchanged since #1900 |

### Files changed in this PR

- `docs/STATE.md` — new top sync-edit block (Part A gap correction #1843–#1874; Part B #1903–#2016 in ten stacks); `Last Reviewed` → 2026-06-10.
- `docs/PHASE_PROGRESS.md` — header date + paragraph refresh (stale "MandateGate not implied" non-claim corrected to "mandate-authority migration not complete"); condensed mirror sync block.
- `docs/registry.toml` — STATE.md row `last_updated`/`last_reviewed` → 2026-06-10 (mirrors the #1902 precedent).
- `docs/INDEX.generated.md` — regenerated drift mirror (one date line).
- `docs/strategy/ICN-Technical-Whitepaper.md` — line 207 crate count 38 → 37 (the correction the 2026-05-22 sync deferred); LOC/test figures on that line NOT re-verified (still attributed "As of March 2026").
- `docs/dev/handoff-2026-06-10-truth-sync.md` — this file.
- `docs/status.toml` — NOT changed; `total_crates` re-verified as still correct.

## Checks run (with results)

| Check | Command | Result |
|-------|---------|--------|
| Doc control (strict) | `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict` | see PR test plan — run pre- and post-edit |
| Compliance linter | `python3 .github/scripts/compliance_linter.py` | see PR test plan |
| Readiness overclaim linter | `python3 .github/scripts/readiness_overclaim_linter.py` | see PR test plan |
| INDEX drift mirror | `python3 docs/scripts/generate-index.py --registry docs/registry.toml` vs `docs/INDEX.generated.md` | clean after regeneration |
| Docs-only diff | `git diff --stat` | 6 files, all under `docs/` |

## Checks NOT run

- No cargo commands (docs-only change; no Rust file touched).
- Whitepaper LOC (~414K) and test-count (2,287) figures not re-verified — named as candidate (f) in the STATE.md sync block.
- The full STATE.md / PHASE_PROGRESS.md historical blocks below the new sync edits were not re-audited; this sync corrects forward, it does not re-litigate prior blocks beyond the named corrections.

## Deferred items (named, not started)

Carried in the STATE.md sync block's candidate enumeration: (a) appliance negative firstboot smoke; (b) Dependabot queue triage (hold #1944 rand major + #1942 ml-dsa); (c) #1868 broad-fallback retirement criteria; (d) #1703 human gate; (e) issue hygiene #1704/#1727/#1728; (f) strategy-doc deep refresh.

## Non-claims

This session does NOT claim Phase 2 completion, formal NYCN pilot, production readiness, live federation, implemented Civic Shell / member shell / steward cockpit, retirement of broad `governance:write`, a verified appliance fail-closed firstboot path, signed appliance images, or resolved licensing. No private partner / member / organizer data was read or handled.
