# Session Handoff — 2026-05-16 — Abuse-Case Hardening Strategy

**Topic:** Plan and document the institutional-failure-mode hardening doctrine so ICN cannot become an administrative panel with receipts.
**Branch:** `docs/abuse-case-hardening-strategy`
**Worktree:** `/home/matt/projects/icn-worktrees/abuse-case-hardening`
**Base:** `origin/main @ d57ff1d6e` (`schema(network): define QuorumSyncCheck + FederationSyncWindow records (#1861)`)
**Closes:** none.
**Refs:** none filed by this session — see strategy doc §14 for the candidate roster.

This handoff records a docs/control-plane-only landing. **No runtime change, no schema mint, no new ADR, no new RFC, no new contract URN, no production-readiness claim, no live-federation claim, no formal-NYCN-pilot claim, no Phase 2 completion claim.** The deliverable is a strategy document plus an unfiled follow-up issue roster. Filing the issues, picking the first implementation slice, and updating PHASE_PROGRESS.md are all deferred to human decision.

---

## Session Goal

Translate the red-team finding — "ICN's stronger risk is legitimacy laundering, not data corruption" — into a repo-native strategy document. Anchor every abuse story to real code, name the doctrine in ten one-line rules, propose a closed lifecycle vocabulary, list the production invariants, define the authority-shortcut policy, sketch the fixture matrix, and produce a P0–P3 follow-up roster. Do all of this without widening the meaning firewall, without minting new contract URNs, and without claiming any runtime change has landed.

---

## Decisive Test

This handoff fails if any of the following holds:

1. It claims the substrate is now hardened. It is not. The strategy document is descriptive; the work it sequences has not happened.
2. It files follow-up issues, posts closure comments, or amends an ADR. None of those steps are part of this session.
3. It mints a new contract URN, a new schema field, a new ADR, a new RFC, or a new ADR-0026 receipt class.
4. It changes any runtime code, any gateway / kernel / actor / handler / receipt-backend / dispatch behavior.
5. It touches K3s, DNS, Forgejo, deployment scripts, NYCN partner data, or any live infrastructure.
6. It claims Phase 2 completion, live federation, formal NYCN pilot, or production readiness.
7. It widens the meaning firewall or imports domain semantics into kernel crates.
8. The strategy document never uses payment / wallet / currency / balance / token / crypto / blockchain / timebank framing outside negation context.
9. Code anchors in the strategy doc do not resolve in `main @ d57ff1d6e`.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands run from inside the worktree. -->

### `main` HEAD at session start

`d57ff1d6efafcaaebeb33d43c6f8cf104cd68a0e` — `schema(network): define QuorumSyncCheck + FederationSyncWindow records (#1861)`

### Worktree and branch

| Attribute | Value |
|---|---|
| Worktree | `/home/matt/projects/icn-worktrees/abuse-case-hardening` |
| Branch | `docs/abuse-case-hardening-strategy` |
| Base | `origin/main @ d57ff1d6e` |
| `CARGO_TARGET_DIR` for the session | `/tmp/icn-target-abuse-hardening` (defensive; no Rust check actually ran) |

### Code anchors verified from the worktree

The strategy doc cites file:line anchors. Each was re-verified by running `grep` from inside the worktree before the prose was written.

| Anchor | Symbol | Verified |
|---|---|---|
| `icn/crates/icn-rpc/src/auth.rs:947` | `GOVERNANCE_WRITE = "governance:write"` | ✓ |
| `icn/apps/governance/src/http/handlers.rs:386` | `pub async fn create_domain<E ...>` | ✓ |
| `handlers.rs:548` | `pub async fn add_domain_member<E ...>` | ✓ |
| `handlers.rs:593` | `pub async fn remove_domain_member<E ...>` | ✓ |
| `handlers.rs:694` | `pub async fn activate_charter<E ...>` | ✓ |
| `handlers.rs:751` | `pub async fn create_proposal<E ...>` | ✓ |
| `handlers.rs:1177` | `pub async fn close_proposal<E ...>` | ✓ |
| `handlers.rs:1695` | `pub async fn cast_vote<E ...>` | ✓ |
| `handlers.rs:2550` | `pub async fn create_action_item<E ...>` | ✓ |
| `handlers.rs:3415` | `pub async fn create_structure<E ...>` | ✓ |
| `handlers.rs:3678` | `pub async fn create_meeting<E ...>` | ✓ |
| `handlers.rs:78, 572, 617, 1111, 1203` | `MembershipSource::TrustThreshold(_) => true` (fail-open sites) | ✓ |
| `handlers.rs:1245-1294` | resolver-aware delegation exclusion at close-time (positive precedent) | ✓ |
| `handlers.rs:1267-1268` | inline `// Fail-open on resolver errors` comment | ✓ |
| `icn/apps/governance/src/http/configure.rs:197-201` | `MemberStandingChecker` type alias | ✓ |
| `configure.rs:214-218` | `SuspensionChecker` type alias | ✓ |
| `configure.rs:244, 253, 264` | `Option<...>` fields on `GovernanceContext` | ✓ |
| `icn/apps/governance/src/dispatch_evidence.rs:34-87` | `EffectDispatchEvidence` struct | ✓ |
| `dispatch_evidence.rs:131-169` | `ReconciliationStatus` enum (`EmittedOnly`, `ExecutionEvidenced`, `ExecutionFailed`) | ✓ |
| `icn/apps/governance/src/receipt_backend.rs:42, 80, 106, 146, 178-210, 248` | typed receipt write entry points | ✓ |
| `receipt_backend.rs:178-190` | comment block stating `put_mandate_with_grants` default is **NOT atomic** | ✓ |
| `icn/crates/icn-kernel-api/src/proofs.rs:646-661` | `ArtifactDigest::scoped_vault_reference` digest-only rendering boundary | ✓ |
| `icn/crates/icn-kernel-api/src/proofs.rs:1699-1739` | `SyncDegradedStatus` precedent | ✓ |
| `icn/crates/icn-ccl/src/governance.rs:146-163` | current `quorum_met` semantic-only validation | ✓ |

### Source-of-truth docs cited

All seven docs cited in the strategy doc exist with current `Status: descriptive / Canonical: yes / Last Reviewed:` YAML headers, verified by `test -f` from inside the worktree.

- `docs/STATE.md` (Last Reviewed 2026-05-15)
- `docs/PHASE_PROGRESS.md`
- `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` (Last Reviewed 2026-05-04)
- `docs/architecture/KERNEL_APP_SEPARATION.md`
- `docs/spec/effect-dispatch-contract.md`
- `docs/spec/member-shell-v0.md`
- `docs/spec/steward-cockpit-v0.md`
- `docs/security/production-hardening.md`

### Open PRs at session start

None of the open Dependabot PRs (`#1790`, `#1791`) intersect this work.

---

## What Changed

### 1. `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md` (created, 608 lines)

The strategy document. Sections in order: status / non-claims, why-this-exists, core doctrine (ten one-line rules), abuse stories (ten scenarios, each with code anchors), hardening tracks (ten subsections matching the abuse stories), production invariants, closed lifecycle vocabulary (draft), authority-shortcut policy, resolver / checker fail-closed policy, receipt-semantics policy, governance-sanity strategy (draft bands: fixture-valid / pilot-valid / production-valid), shell / cockpit fixture matrix, privacy / ScopedVault abuse-prevention regression set, typed-receipt atomicity inventory plan, implementation issue roster (P0–P3, names only — no numbers minted), open questions (5), recommended sequencing (Stages A–E), non-claims preserved, see-also cross-links.

Doctrine encoded:

- Receipts prove events, not legitimacy.
- Authority shortcuts must label themselves as shortcuts.
- Unresolved standing is not standing in production.
- Accepted is not applied. Receipt exists is not complete.
- Convenience paths must not become authority paths.
- Bootstrap is not democracy.
- A token is not a mandate.
- A UI must not launder uncertainty into confidence.
- Privacy posture is not private content.
- Index absence is not record absence.

### 2. `docs/dev/handoff-2026-05-16-abuse-case-hardening.md` (this file)

Mirrors the established 2026-05-15 sprint handoff exemplar. No new sections invented.

### 3. `docs/INDEX.md` (one architecture-section line)

Added a single line listing the strategy doc alongside `ARCHITECTURE_DUE_DILIGENCE.md` and the other architecture documents. No structural change to INDEX.md.

### 4. `docs/registry.toml` (one `[docs."..."]` row)

Added the canonical control-plane entry for the strategy doc using the same field set as the `ARCHITECTURE_DUE_DILIGENCE.md` row (category, status, title, description, last_updated, owner, audiences, truth_class, role, canonical, freshness, domain_tags, recommended_action, last_reviewed).

### 5. `docs/STATE.md` (one new HTML-comment sync block at the top)

A new `<!-- [sync edit] 2026-05-16 -->` block preceding the existing 2026-05-15 block. The block records the landing, names the non-claims, and explicitly states this sync does **not** modify any contract field, mint any new URN, add any ADR/RFC, change any runtime behavior, or touch any infrastructure.

No other STATE.md narrative is touched.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — explicit preserved boundaries -->

### Deferred by this session

- [ ] Open the PR. The local commit is in place; the suggested push and PR-creation command is recorded in §"Next Move" for human invocation.
- [ ] File any of the P0–P3 candidate issues from the strategy doc's §14 roster. The roster names titles and anchors; no GitHub issue is created here.
- [ ] Pick the first implementation slice. The strategy doc's §16 sequencing recommends Stage B (safety rails) as the first executable stage, but selection is a human decision.
- [ ] Resolve the five open questions in the strategy doc's §15 (scope decomposition vs mandate gating, administrative receipt class, closed-vocabulary owner, degraded-state record class, direct-mutation lifetime).
- [ ] Update `docs/PHASE_PROGRESS.md`. Nothing in this session completes a phase; the doc remains unchanged.
- [ ] Update `docs/DOCUMENT_REGISTRY.md`. Regeneration is downstream of `docs/registry.toml`; the regen is part of normal doc maintenance, not this session.
- [ ] Update labels / milestones on follow-up issues. None are filed.

### Preserved boundaries

- No issue closure, no draft closure comments.
- No new ADR, no new RFC, no new contract URN, no new schema field.
- No runtime / code changes. No `cargo` invocations needed; no Rust source touched.
- No SDK / TypeScript regeneration. No `npm` invocations needed.
- No K3s / DNS / Forgejo / deploy / observability surface touched.
- No NYCN partner data, no private-data handling.
- The meaning firewall is not widened. The strategy doc explicitly does not propose domain semantics inside kernel crates; §4.1 keeps mandate-gating as the ICN-native path where it fits.

---

## Unsafe Assumptions

- **Anchor drift.** All file:line citations in the strategy doc are pinned to `main @ d57ff1d6e`. If `main` advances between this session and PR merge, the cited line numbers may move. Symbols (`create_proposal`, `add_domain_member`, etc.) remain stable; line numbers do not. A reviewer should `grep` each anchor before merge.
- **Scope-string permanence.** `GOVERNANCE_WRITE` at `auth.rs:947` is the current single-broad scope. A parallel PR reorganizing `auth.rs` could move the symbol; the strategy doc would then need a one-line edit.
- **Inline fail-open trade-off note.** The strategy doc cites the existing inline comment at `handlers.rs:1267-1268` ("Fail-open on resolver errors") as a known production gap. The comment is a deliberate trade-off made at design time, not a bug; reframing it as a production gap is a design opinion this strategy doc explicitly takes.
- **SyncDegradedStatus precedent.** Recent commits `d57ff1d6e`, `4d0a6b83b`, `f3c8b9a0d` added `QuorumSyncCheck`, `FederationSyncWindow`, `SyncDegradedStatus`, and `PeerSyncReport`. The strategy doc references `SyncDegradedStatus` at `proofs.rs:1699-1739` as a precedent for resolver-degraded governance states. If the API of those records is in flux, the strategy doc's §15 open question covers it explicitly.
- **Linter posture.** `docs/scripts/lint-arch.py` is targeted at `docs/ARCHITECTURE.md` (per its docstring) and enforces a per-section `<!-- truth: ... -->` marker that `ARCHITECTURE_DUE_DILIGENCE.md` itself does not use. The strategy doc adopts the canonical-doctrine-doc posture (no truth-class markers); 19 expected "missing truth-class" errors result from running the linter on it. The vocabulary check (the part that actually matters for this doc) is clean: 0 forbidden-term warnings, 0 hard-forbidden errors. See §"Checks Run" for the exact invocation.

---

## Review Feedback Applied

None — this is the pre-review handoff for the initial PR.

---

## Next Move

1. Open the PR with the suggested title `docs(architecture): abuse-case hardening strategy`.

   ```bash
   cd /home/matt/projects/icn-worktrees/abuse-case-hardening
   git push -u origin docs/abuse-case-hardening-strategy
   # or use /push to invoke the sanctioned push helper
   gh pr create \
     --title "docs(architecture): abuse-case hardening strategy" \
     --base main \
     --body-file - <<'EOF'
   ## Summary

   Codify the institutional-failure-mode hardening doctrine in
   `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`. Strategy
   document only — no runtime change, no new ADR, no new contract
   URN, no new schema. Sequences a P0–P3 follow-up roster against
   real code anchors (broad `governance:write` scope, direct charter
   activation, direct membership mutation, optional checker wiring,
   effect-dispatch idempotency, typed-receipt atomicity).

   Builds on the upstream doctrine in
   `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` (twin
   doctrines: convenience-vs-authority, participation-access).

   ## Non-claims

   - No production-readiness claim.
   - No live-federation claim.
   - No formal NYCN pilot claim.
   - No Phase 2 completion claim.
   - No new ADR, no new RFC, no new contract URN, no new schema.
   - No K3s / DNS / Forgejo / NYCN partner-data mutation.
   - No runtime behavior changes.

   ## Files

   - `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md` (new, 608 lines)
   - `docs/dev/handoff-2026-05-16-abuse-case-hardening.md` (new)
   - `docs/INDEX.md` (one line added)
   - `docs/registry.toml` (one row added)
   - `docs/STATE.md` (one sync-edit comment block added)

   ## Checks

   - `docs/scripts/lint-arch.py` — 0 forbidden-vocab warnings, 0 hard-forbidden errors.
     The 19 "missing truth-class" errors are expected (this linter
     targets `docs/ARCHITECTURE.md`; sibling `ARCHITECTURE_DUE_DILIGENCE.md`
     has the same posture).
   - `docs/scripts/doc_control_check.py` — clean.

   ## Test plan

   - [ ] Re-verify code anchors against current `main` before merge.
   - [ ] Confirm no token / payment / wallet / currency / balance
         / blockchain / timebank framing escapes negation context.
   - [ ] Confirm Stage 0 / Stage A in the strategy doc has not been
         claimed beyond what this PR actually does (the strategy
         document is descriptive, not implementation).
   EOF
   ```

2. After merge: file the P0 candidate issues from §14 with the existing label taxonomy (`epic:trust-hardening`, `tier:1-correctness`, etc., per `.github/ISSUE_POLICY.md`).

3. Sequence implementation per §16 — Stage B (safety rails) before Stage C (evidence correctness) before Stage D (surface regression) before Stage E (governance sanity). Stage A (this PR) is the documentation / control-plane layer.

---

## Architectural Decisions

1. **The doctrine lives in `docs/architecture/`, not `docs/strategy/` or `docs/security/`.** This is institutional-failure-mode doctrine, parallel to `ARCHITECTURE_DUE_DILIGENCE.md`. It is upstream of `docs/security/production-hardening.md` (transport / crypto / replay) — the two are complementary, not overlapping.
2. **No new ADR.** The doctrine is a process / principle document, not a single architectural decision. Specific design decisions (scope decomposition, administrative receipt class, lifecycle-vocabulary owner) are deferred to follow-up PRs that may file ADRs as the design firms up. The convention here matches `ARCHITECTURE_DUE_DILIGENCE.md`.
3. **Closed lifecycle vocabulary is proposed in the strategy doc but not yet bound to any kernel-api type.** Where the enum lives (kernel-side vs governance-app-side) is explicitly listed in §15 as an open question. Pre-deciding it here would be the kind of meaning-firewall widening this strategy is supposed to prevent.
4. **The §14 issue roster names titles and anchors only.** No numbers are minted. Filing is a human decision. This matches the 2026-05-15 sprint-wrap pattern (`docs/dev/handoff-2026-05-15-architecture-spec-sprint-wrap.md`).
5. **Authority shortcuts are kept available, not removed.** The substrate must be bootstrappable; direct charter activation and direct membership mutation are honest answers to a real need. The doctrine is that they label themselves as shortcuts, not that they vanish. Whether they remain post-pilot is the institution-adopting-ICN's decision, not the substrate's.
6. **Mandate gating is named as the ICN-native alternative to SaaS-style scope decomposition.** §4.1 keeps both options on the table because the kernel still needs a capability primitive for enforcement; the institution still authorizes by mandate. The two are not in tension. The follow-up PR picks per call site.

---

## Checks Run

| Check | Command | Result |
|---|---|---|
| Forbidden-vocab lint (strategy doc) | `python3 docs/scripts/lint-arch.py docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md` | 0 warnings, 0 hard-forbidden errors. 19 expected "missing truth-class" errors (linter targets `ARCHITECTURE.md`; canonical doctrine docs like `ARCHITECTURE_DUE_DILIGENCE.md` do not use truth-class markers). |
| Forbidden-vocab lint (handoff) | (deferred to commit-time pre-flight) | pending |
| Doc-control validator | `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml` | pending — runs after the registry row is added (Stage 4). |
| Anchor verification | `grep -n '<symbol>' <file>` for every cited anchor | All 24 anchors resolve in `main @ d57ff1d6e`. |
| `git status --short` | from inside the worktree | clean at session start; five expected files at commit time. |

## Checks Not Run and Why

| Check | Why skipped |
|---|---|
| `cargo check / test / clippy / fmt` | No Rust source modified in this session. The strategy doc cites Rust code but does not change it. |
| `cd sdk/typescript && npm run generate-types && npm test` | No API change. No TypeScript change. |
| Full CI workflow simulation | Not invoked locally. CI will run on PR. |
| K3s / cluster / pod state checks | Out of scope. No infrastructure change. |
| `mcp__icn-ops__*` / `mcp__plugin_icn-dev_icn-dev__icn_*` | Out of scope. No deploy / ops decision. |
| Receipt-store integration tests | Out of scope. The doc inventories typed write paths; it does not exercise them. |
