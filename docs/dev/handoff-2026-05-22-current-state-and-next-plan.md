---
Status: descriptive
Authority: session
Canonical: no
Last verified: 2026-05-22
---

# Session Handoff — 2026-05-22 — current state and next plan

## Session Goal

Verify the merged state of `origin/main` against an external review brief that asserted recent landings (#1878 → #1901), produce a current-state record, and propose the next PR sequence. Land the verification as a docs-only truth-sync rather than letting the doc layer continue to lag the merged code.

## Decisive Test

Does `docs/STATE.md` plus `docs/PHASE_PROGRESS.md` plus `docs/status.toml` describe the state a reader would find on `origin/main` today, without overstating what is true and without omitting what merged? If a reader trusted those docs to plan the next PR, would they end up planning the right PR or the wrong PR?

Before this session: no — STATE.md ended at #1833 (2026-05-15), status.toml's `total_crates` literal undercounted workspace members by three, and PHASE_PROGRESS.md did not mention #1880/#1881/#1899/#1900/#1901.

After this PR: STATE.md, PHASE_PROGRESS.md, and status.toml all describe `origin/main` through #1901.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`eadd5b0d0 docs(spec): define ICN Civic Shell v0 composition surface (#1899)`

### Branch this PR opens on
`docs/truth-sync-2026-05-22` — created off `origin/main` at `eadd5b0d0`. No prior worktree branch history.

### Open PRs at session start

| PR | Title | Class |
|----|-------|-------|
| #1898 | deps(ts-sdk): bump fast-uri 3.1.0 → 3.1.2 | security patch (GHSA-v39h-62p7-jpjc) |
| #1897 | deps(ts-sdk): bump picomatch 2.3.1 → 4.0.4 | major + two CVE fixes |
| #1896 | deps(ts-sdk): bump minimatch 3.1.2 → 10.2.5 | major jump; needs Node ≥20; adds install-time `prepare` script |
| #1895 | deps(ts-sdk): dev-dependencies group (4 updates) | minor/patch dev-only |
| #1894 | deps(ts-sdk): bump flatted 3.3.3 → 3.4.2 | CWE-1321 patch |

No human-author PRs open.

### Verified high-claim items on `origin/main`

| Claim | Code anchor | Verdict |
|-------|-------------|---------|
| #1901 production guard | `icn/apps/governance/src/http/configure.rs:58` defines `GovernanceContextBuildMode { Bootstrap, Production, Test }`; line 95 reads `ICN_GOVERNANCE_BUILD_MODE`; line 455 returns `Err(GovernanceContextValidationError)` in Production; line 459 emits `tracing::warn!` in Bootstrap/Test; tests at line 802+ | Present |
| #1881 class-level scope constants | `icn/crates/icn-rpc/src/auth.rs:966-972` defines seven constants (`GOVERNANCE_CHARTER_WRITE`, `GOVERNANCE_PROPOSAL_WRITE`, `GOVERNANCE_STEWARD_WRITE`, `GOVERNANCE_FEDERATION_WRITE`, `GOVERNANCE_MEETING_WRITE`, `GOVERNANCE_ACTIVITY_WRITE`, `GOVERNANCE_COMMENT_WRITE`); line 979 defines `GOVERNANCE_CLASS_WRITE` slice; tests at line 1509+ | Present, but not enforced — 41 broad `require_scope::<BasicClaims>(&http_req, "governance:write")` call sites remain in `icn/apps/governance/src/http/handlers.rs`. No handler uses the new constants. In-code comment at `auth.rs:963` says broad scope "remains in place during the migration." |
| MandateGate | `git grep MandateGate origin/main` — no matches in source | Absent — concept named in `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md` and `docs/design/governance/governance-write-decomposition.md` only |
| Civic Shell implementation | `git grep 'civic_shell\|CivicShell\|headquarters' origin/main -- ':!docs'` — no matches | Absent — `docs/spec/icn-civic-shell-v0.md` is a composition spec, not an implementation |
| Appliance negative firstboot test | `git ls-tree -r origin/main -- deploy/appliance/smoke/` — only `smoke-local.sh` (happy path) and `cloud-init/` examples | Absent — #1900 verified positive path only |
| Workspace member count | `icn/Cargo.toml` `[workspace].members` enumerates 37 crates + 3 bins + 4 apps = 44 declared members | 44 (not 41 as `docs/status.toml` line 241 still says) |

### Issue states at session end (via `gh`)

- #1868 — **OPEN** — umbrella decomposition issue; step 1 done by #1881, step 2 (handler migration) pending.
- #1870 — CLOSED.
- #1871 — CLOSED (implemented by #1901).

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. Verification only — no code touched

Read-only inspection of `origin/main` via `git show` and `gh`. The branch `docs/truth-sync-2026-05-22` is created off `origin/main` for the docs-only PR; no Rust source, no schema, no contract URN, no ADR, no RFC, no kernel/gateway/runtime change.

### 2. Docs-only truth-sync (this PR)

- `docs/STATE.md` — append `[sync edit] 2026-05-22` block covering #1875 → #1901 (21 PR merges + 1 bare handoff commit = 22 commits in the 2026-05-17 → 2026-05-22 window), label each PR as docs vs code, repeat the verbatim non-claim block. Refresh YAML `Last Reviewed: 2026-05-22`.
- `docs/PHASE_PROGRESS.md` — append matching `[sync edit] 2026-05-22` block; keep Phase 2 ⏳ status; update the `Last Updated` date and the `Current Phase` paragraph to mention the 2026-05-22 governance-hardening cycle + Civic Shell + Debian-13 appliance smoke + Claude Design landings.
- `docs/status.toml` — fix one literal: `total_crates` on line 241 from `"34 crates + 4 apps + 3 binaries = 41 workspace members"` to `"37 crates + 4 apps + 3 binaries = 44 workspace members"`. Leave `last_full_verification` alone (that's a different attestation).
- `docs/registry.toml` — refresh the `[docs."docs/STATE.md"]` row's `last_updated` and `last_reviewed` to `2026-05-22` so `doc_control_check.py --strict` agrees with the YAML header. Required by the strict gate; not a content change.
- `docs/dev/handoff-2026-05-22-current-state-and-next-plan.md` (this file).

That's the entire diff. Five files, all under `docs/`.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [ ] **Governance handler migration** (#1868 step 2): 41 broad `governance:write` call sites in `icn/apps/governance/src/http/handlers.rs` still ignore the class-level scopes minted in #1881. The plan picks the *charter* handler family first.
- [ ] **MandateGate scaffold**: named in two design docs, not in code. Defer until handler migration is far enough along that the gating need is concrete.
- [ ] **Appliance negative firstboot smoke**: positive path verified in #1900; fail-closed path (firstboot bails when marker present, `icnd.service` does not start) is not a script. The `10-firstboot-gate.conf` drop-in exists but is not exercised.
- [ ] **Dependabot triage**: five open SDK security/dev-deps PRs. Fast-track #1898/#1894/#1895 after CI validation; review #1897 (picomatch major + CVE) carefully; defer #1896 (minimatch 3→10, Node ≥20).
- [ ] **`docs/strategy/ICN-Technical-Whitepaper.md` line 207** still says `38 Rust workspace crates`. Defer — strategy doc may be intentionally frozen.
- [ ] **`docs/deployment/DEPLOYMENT_READY.md`** uses wallet/balance/payment language (lines 26-27, 130-144). Not in `[control].canonical_doc_paths`. Recommend explicit archive move or a regulatory-vocabulary banner in a follow-up; not silently delete.
- [ ] **`docs/INDEX.md` Last Updated header is 2026-04-15** but the file has had row updates since (e.g., `icn-civic-shell-v0.md` row at line 181 added 2026-05-21). Refresh the header date only when the next session does a full INDEX review pass.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- I rely on the `gh` PR/issue states snapshot taken at session start. If anything merged between that snapshot and PR open, the STATE.md sync block may need an additional entry. The next session should re-run `git log origin/main --oneline -20` before opening the PR.
- I did not run `cargo metadata` end-to-end against `origin/main`; the 44-member count comes from parsing `git show origin/main:icn/Cargo.toml` and counting `[workspace].members` entries. Re-verify with `cargo metadata --manifest-path icn/Cargo.toml --format-version 1 --no-deps | jq '.workspace_members | length'` if the literal goes into release-bearing docs.
- The Civic Shell spec file `docs/spec/icn-civic-shell-v0.md` is named in the merged #1899 commit. I did not read the full file; the registry row description is the canonical summary.
- `docs/registry.toml` `[control].canonical_doc_paths` listing is the authority for what counts as control-plane canonical. I trust that list rather than YAML headers in individual docs.
- I did not run the docs control-plane checks against this branch yet; I will run them before push, but any earlier-introduced drift will surface as a side effect.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. **This session**: finalize this handoff + the three doc edits, run `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict`, `git diff --check`, push `docs/truth-sync-2026-05-22`.
2. **PR 2 — Dependabot fast-track triage**: validate (`cd sdk/typescript && npm ci && npm test && npm run build && npm run typecheck`) and merge in order: #1898 → #1894 → #1895. Hold #1897 (picomatch major + CVE) for a careful read of the changelog. Defer #1896 (minimatch major) pending a Node ≥20 decision and a `prepare`-script review.
3. **PR 3 — Charter handler migration (#1868 step 2, batch 1)**: migrate the charter handler family in `icn/apps/governance/src/http/handlers.rs` from broad `"governance:write"` to `GOVERNANCE_CHARTER_WRITE`. Keep broad scope as an accepted-also fallback per #1880's hybrid design and the comment in `auth.rs:963`. Add unit tests asserting (a) class-level scope is required and (b) broad scope is still accepted. One handler family per PR. Charter → proposal → steward → federation → meeting → activity → comment.
4. **PR 4 — Receipt evidence carries presented scope + mandate grant**: gated by the design pick in #1880 / open question about administrative receipt class. Out of scope until PR 3's first slice lands.
5. **PR 5 — Appliance negative firstboot smoke**: add a `smoke/smoke-local.sh --fail-closed` mode or sibling script that exercises the `10-firstboot-gate.conf` drop-in. Real fail-closed proof, not docs.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

None ratified this session. The PR is docs-only. No new ADR, no new RFC, no new contract URN, no new schema field, no kernel/app boundary widening.

The Civic Shell v0 spec (#1899) is the most recent declared truth change but landed before this session. The governance:write hybrid decomposition design (#1880) and the abuse-case hardening strategy doctrine (#1816, landed earlier as the 2026-05-16 strategy doc) are likewise prior landings, not this session's work.

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
# Confirm tip
git fetch origin
git rev-parse origin/main
# Expect a hash at or after eadd5b0d0; any later commits should be folded into the sync block.

# Confirm #1901 production guard is on main
git show origin/main:icn/apps/governance/src/http/configure.rs | grep -n GovernanceContextBuildMode | head -5

# Confirm #1881 class-level scopes are on main
git show origin/main:icn/crates/icn-rpc/src/auth.rs | grep -n GOVERNANCE_CLASS_WRITE | head -3

# Confirm handler migration has not happened
git grep -c '"governance:write"' origin/main -- 'icn/apps/governance/src/http/handlers.rs'
# Expect: 41 (or close — if it drops materially, batch 1 of the migration started elsewhere)

# Confirm MandateGate still absent from source
git grep MandateGate origin/main -- ':!docs'
# Expect: no output

# Workspace member count
python3 -c "
import re
with open('icn/Cargo.toml') as f: s = f.read()
m = re.search(r'^members\s*=\s*\[(.*?)^\]', s, re.M|re.S)
print(len(re.findall(r'\"[^\"]+\"', m.group(1))))
"
# Expect: 44

# Doc control-plane check (before push)
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict

# Whitespace / file-mode sanity
git diff --check
```

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth** (`STATE.md` 2026-05-16 + `PHASE_PROGRESS.md` 2026-05-15): lagged actual main by ~10 merged PRs. This PR closes that gap.
- **Implementation truth** (`origin/main` source): verified for #1901 (configure.rs), #1881 (auth.rs), absent for MandateGate and Civic Shell code.
- **Execution truth** (gh PR/issue snapshot): five Dependabot PRs open; #1868 open, #1870 closed, #1871 closed.
- **Narrative truth**: `docs/strategy/ICN-Technical-Whitepaper.md` line 207 says "38 Rust workspace crates"; the canonical workspace count is 37 lib crates + 3 bins + 4 apps = 44 declared members. Defer correction to a strategy-doc refresh PR; this PR does not modify strategy docs.
- **Known conflicts**:
  - `docs/status.toml` line 241 literal vs `icn/Cargo.toml` `[workspace].members` count → this PR fixes.
  - `docs/INDEX.md` Last Updated header is older than its last content edit → this PR does not refresh; out of scope to verify every row.
  - `docs/deployment/DEPLOYMENT_READY.md` regulatory vocabulary regression (wallet/balance/payment) → this PR does not touch; recommend follow-up to archive or banner the file. The file is not in `[control].canonical_doc_paths`, so the regression is not a control-plane violation by `doc_control_check.py`'s lights.
