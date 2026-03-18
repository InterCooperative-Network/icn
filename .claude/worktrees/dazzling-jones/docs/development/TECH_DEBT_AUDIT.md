# ICN Tech Debt Audit

**Last Updated**: 2026-03-05
**Audited Against**: `main` @ `c7773390` (Sprint 14 complete)

## Summary Dashboard

| # | Category | P0 | P1 | P2 | Total | Resolved |
|---|----------|----|----|----|----|----------|
| 1 | [Large Module Decomposition](#1-large-module-decomposition) | 0 | 5 | 8 | 13 | 0 |
| 2 | [Error Module Gaps](#2-error-module-gaps) | 0 | 7 | 7 | 14 | 0 |
| 3 | [Test Coverage Gaps](#3-test-coverage-gaps) | 2 | 5 | 6 | 13 | 0 |
| 4 | [TODO/FIXME Markers](#4-todofixme-markers) | 3 | 12 | 36 | 51 | 0 |
| 5 | [Ignored Tests](#5-ignored-tests) | 0 | 6 | 13 | 19 | 0 |
| 6 | [Testkit Adoption](#6-testkit-adoption) | 0 | 1 | 2 | 3 | 0 |
| 7 | [Meaning Firewall Ratchets](#7-meaning-firewall-ratchets) | 0 | 2 | 0 | 2 | 2 |
| 8 | [Documentation Gaps](#8-documentation-gaps) | 0 | 1 | 3 | 4 | 0 |
| 9 | [Benchmark Gaps](#9-benchmark-gaps) | 0 | 0 | 4 | 4 | 0 |
| 10 | [Structural Debt](#10-structural-debt) | 0 | 2 | 1 | 3 | 0 |
| | **Totals** | **5** | **41** | **80** | **126** | **2** |

**Priority definitions**:
- **P0**: Affects runtime correctness, security, or data integrity
- **P1**: Blocks or significantly impedes development velocity
- **P2**: Improves quality, developer experience, or documentation

---

## 1. Large Module Decomposition

8 files exceed 2000 LOC; 5 more are in the 1900-2000 danger zone. Large modules hurt readability, increase merge conflicts, and make targeted testing difficult.

**Detailed plans**: [module-splitting-analysis.md](module-splitting-analysis.md) | [module-splitting-implementation-guide.md](module-splitting-implementation-guide.md)

### Files >2000 LOC (non-test, non-binary)

| File | LOC | Priority | Recommendation | Status |
|------|-----|----------|----------------|--------|
| `crates/icn-obs/src/metrics_legacy.rs` | 5,149 | P2 | Mark deprecated, migrate to per-domain metrics modules | [ ] |
| `crates/icn-ledger/src/ledger.rs` | 4,630 | P1 | Split into queries, balances, fork resolution, witness, validation (see analysis doc) | [ ] |
| `crates/icn-gossip/src/gossip.rs` | 3,157 | P1 | Split by protocol phase (push/pull/anti-entropy) | [ ] |
| `crates/icn-compute/src/scheduler.rs` | 2,714 | P1 | Extract placement, lifecycle, policy into submodules | [ ] |
| `crates/icn-ledger/src/use_access.rs` | 2,699 | P2 | Extract access control subsystems | [ ] |
| `crates/icn-ccl/src/disputes.rs` | 2,656 | P2 | Well-structured internally; defer unless actively changing | [ ] |
| `apps/governance/src/actor.rs` | 2,615 | P1 | Extract message handlers, state management, gossip sync | [ ] |
| `crates/icn-governance/src/proposal.rs` | 2,589 | P2 | Extract validation, execution, cleanup logic | [ ] |
| `crates/icn-gateway/src/commons_mgr.rs` | 2,383 | P2 | Extract charter, amendment, steward operations | [ ] |
| `crates/icn-net/src/protocol.rs` | 2,297 | P2 | Stable; defer unless actively changing | [ ] |
| `crates/icn-core/src/supervisor/governance_executor.rs` | 2,123 | P1 | Extract per-proposal-type execution handlers | [ ] |
| `crates/icn-gateway/src/api/entity.rs` | 2,048 | P2 | Split by entity type (individual, coop, federation) | [ ] |
| `crates/icn-net/src/actor/mod.rs` | 1,998 | P2 | Approaching threshold; monitor | [ ] |

### Binary

| File | LOC | Priority | Recommendation | Status |
|------|-----|----------|----------------|--------|
| `bins/icnctl/src/main.rs` | 10,246 | P1 | See [Section 10: Structural Debt](#10-structural-debt) | [ ] |

---

## 2. Error Module Gaps

14 non-facade crates lack a dedicated `error.rs` with `thiserror`-derived error types. These crates use ad-hoc error handling (`anyhow`, string errors, or `Box<dyn Error>`), which makes error recovery and API contracts unclear.

**Related**: [code-quality-improvements.md](code-quality-improvements.md) (Error Handling Audit section)

### Crates Without Dedicated Error Module

| Crate | Src LOC | Priority | Effort | Status |
|-------|---------|----------|--------|--------|
| `icn-identity` | 14,522 | P1 | Medium | [ ] |
| `icn-trust` | 11,818 | P1 | Medium | [ ] |
| `icn-obs` | 11,792 | P1 | Small | [ ] |
| `icn-rpc` | 10,498 | P1 | Medium | [ ] |
| `icn-steward` | 6,218 | P1 | Medium | [ ] |
| `icn-zkp` | 5,843 | P1 | Medium | [ ] |
| `icn-coop` | 5,354 | P1 | Medium | [ ] |
| `icn-store` | 4,508 | P2 | Small | [ ] |
| `icn-crypto-pq` | 3,707 | P2 | Small | [ ] |
| `icn-snapshot` | 2,910 | P2 | Small | [ ] |
| `icn-security` | 1,782 | P2 | Small | [ ] |
| `icn-testkit` | 1,818 | P2 | Small | [ ] |
| `icn-http-kit` | 1,049 | P2 | Small | [ ] |
| `icn-naming` | 849 | P2 | Small | [ ] |

**Excluded** (thin facades, <150 LOC): `icn-crypto` (42), `icn-protocol` (47), `icn-services` (60), `icn-encoding` (138)

---

## 3. Test Coverage Gaps

13 crates and 3 apps have zero integration tests. Some have inline `#[cfg(test)]` unit tests but no `tests/` directory for integration testing.

### Crates With No Integration Tests

| Crate | Src LOC | Inline Unit Tests | Priority | Status |
|-------|---------|-------------------|----------|--------|
| `icn-kernel-api` | 15,592 | Yes | P0 | [ ] |
| `icn-obs` | 11,792 | No | P0 | [ ] |
| `icn-entity` | 7,138 | Yes | P1 | [ ] |
| `icn-zkp` | 5,843 | Yes | P1 | [ ] |
| `icn-coop` | 5,354 | No | P1 | [ ] |
| `icn-snapshot` | 2,910 | Yes | P1 | [ ] |
| `icn-security` | 1,782 | Yes | P1 | [ ] |
| `icn-api` | 1,286 | Yes | P2 | [ ] |
| `icn-http-kit` | 1,049 | Yes | P2 | [ ] |
| `icn-naming` | 849 | Yes | P2 | [ ] |
| `icn-encoding` | 138 | Yes | P2 | [ ] |
| `icn-protocol` | 47 | No | P2 | [ ] |
| `icn-services` | 60 | No | P2 | [ ] |

### Apps With No Integration Tests

| App | Src LOC | Priority | Status |
|-----|---------|----------|--------|
| `apps/membership` | 17,752 | P1 | [ ] |
| `apps/governance` | 9,410 | P1 | [ ] |
| `apps/ledger` | 404 | P2 | [ ] |

### Test-to-Code Ratios (Low Coverage Crates With Tests)

| Crate | Src LOC | Test LOC | Ratio | Note |
|-------|---------|----------|-------|------|
| `icn-gateway` | 58,480 | 9,036 | 15.5% | Largest crate, needs more |
| `icn-governance` | 25,245 | 1,481 | 5.9% | Complex proposal logic |
| `icn-compute` | 24,419 | 1,601 | 6.6% | Scheduler/placement logic |
| `icn-ledger` | 27,572 | 1,970 | 7.1% | Double-entry invariants |

---

## 4. TODO/FIXME Markers

51 markers across production code (non-test). Each represents deferred work or a known gap.

### P0: `unimplemented!()` in Production Paths

These will panic at runtime if reached.

| File | Line(s) | Context |
|------|---------|---------|
| `crates/icn-core/src/resource_enforcer_actor.rs` | 511, 582, 671 | Resource enforcement match arms — will panic on certain resource types |
| `crates/icn-gateway/src/api/names.rs` | 199, 203, 215, 219, 223, 227, 231, 235 | 8 naming API endpoints — all `unimplemented!()` |

### P1: Phase-Gated TODOs (Blocking Future Work)

| File | Line | Marker | Phase/Issue |
|------|------|--------|-------------|
| `crates/icn-rpc/src/server.rs` | 204 | `TODO(Phase 2.3): Implement with PolicyOracle` | Phase 2.3 |
| `crates/icn-core/src/supervisor/init_rpc.rs` | 111 | `TODO(Phase 2.3): Replace with PolicyOracle-based rate limiting` | Phase 2.3 |
| `crates/icn-core/src/supervisor/init_rpc.rs` | 117 | `TODO: rpc_server.set_policy_oracle(...)` | Phase 2.3 |
| `crates/icn-net/src/handlers/handshake.rs` | 42 | `TODO(Phase 2.3): Get trust score from PolicyOracle` | Phase 2.3 |
| `crates/icn-net/src/handlers/hello.rs` | 230 | `TODO(Phase 2.3): Get trust score from PolicyOracle` | Phase 2.3 |
| `crates/icn-core/src/supervisor/governance_executor.rs` | 752 | `TODO: Query actual treasury balance from ledger` | Pilot |
| `crates/icn-core/src/supervisor/effect_dispatcher.rs` | 583 | `TODO: When treasury executor is wired to real ledger` | Pilot |
| `crates/icn-core/src/supervisor/init_notifications.rs` | 835 | `TODO: When a real ResourceAccessStore backend is integrated` | Pilot |
| `crates/icn-core/src/apps/dispatcher.rs` | 185 | `TODO(#873): Implement copy-on-write for StateSnapshot` | #873 |
| `crates/icn-core/src/replication/adjuster.rs` | 233 | `TODO(#924): For Federation+ scopes, integrate with gossip peer discovery` | #924 |
| `crates/icn-federation/src/receipt_clearing.rs` | 371 | `TODO(Epic 6, #925): Route to commons credit pool` | #925 |
| `crates/icn-compute/src/actor/placement.rs` | 1125 | `TODO: Trigger payment settlement via ClearingManager` | Settlement |

### P2: Enhancement TODOs

| File | Line | Marker |
|------|------|--------|
| `crates/icn-compute/src/commons_pool.rs` | 84 | `TODO(#925): Implement stale participant expiry` |
| `crates/icn-compute/src/actor/lifecycle.rs` | 1357-1359 | 3x `TODO: populate from task constraints` (max_scope, cell_affinity, allowed_scopes) |
| `crates/icn-compute/src/actor/placement.rs` | 1084-1086 | 3x `TODO: populate from federated task constraints` |
| `crates/icn-compute/src/scheduler.rs` | 444 | `TODO: Implement actual storage sensing via statvfs` |
| `crates/icn-compute/src/scheduler.rs` | 446 | `TODO: Implement actual network bandwidth measurement` |
| `crates/icn-kernel-api/src/proofs.rs` | 40 | `TODO: Converge with canonical ScopeId type` |
| `crates/icn-identity/src/bundle.rs` | 286 | `TODO: Support PQ keys for hardware backends` |
| `crates/icn-identity/src/keystore_tpm.rs` | 749 | `TODO: Implement TPM attestation in future phase` |
| `crates/icn-gateway/src/api/ledger.rs` | 42 | `TODO: Retrieve actual credit limits from CreditPolicy` |
| `crates/icn-gateway/src/api/ledger.rs` | 440 | `TODO: Add index for decision_hash in ledger` |
| `crates/icn-gateway/src/api/sdis/simple_enrollment.rs` | 74 | `TODO(#396): Replace with threshold PRF computation` |
| `crates/icn-gateway/src/api/sdis/simple_enrollment.rs` | 477 | `TODO(#396): Enforce steward vouch rate limiting` |
| `crates/icn-gateway/src/api/compute.rs` | 167 | `TODO: Wire WasmRegistry into ComputeManager` |
| `crates/icn-gateway/src/api/contracts.rs` | 528 | `TODO: Extract required capabilities from contract` |
| `crates/icn-gateway/src/api/registry.rs` | 781 | `TODO: populate from canonical receipt graph` |
| `crates/icn-trust/src/lib.rs` | 908 | `TODO: Could optimize with a scoped pathfinder` |
| `crates/icn-trust/src/lib.rs` | 927 | `TODO: Optimize with batch edge fetching` |
| `crates/icn-ledger/src/ledger_impl/witness_ops.rs` | 184 | `TODO: compute trust from each party's perspective` |
| `crates/icn-ledger/src/ledger.rs` | 467 | `TODO: implement From<&icn_identity::Did> for kernel Did` |
| `crates/icn-ledger/src/commons_credits.rs` | 26, 56 | `TODO(governance): Make configurable via CCL governance` |
| `crates/icn-api/src/compute.rs` | 168 | `TODO: Add to API params when policy support is added` |
| `crates/icn-governance/src/proposal_cleanup.rs` | 334 | `TODO: Once store_archive is fully implemented` |
| `apps/membership/src/membership.rs` | 254 | `TODO: Query trust PolicyOracle for actual trust score` |
| `apps/membership/src/coop.rs` | 131 | `TODO: MemberRole::Officer doesn't carry a title` |
| `bins/icnd/src/main.rs` | 139 | `TODO(hardware-keys): refactor keystore init` |

---

## 5. Ignored Tests

19 tests are marked `#[ignore]`. Each test that's ignored is a gap in CI coverage.

### Stress Tests (Intentionally Ignored — Run Manually)

| File | Test | Reason |
|------|------|--------|
| `crates/icn-testkit/tests/chaos_tests.rs:279` | `test_message_burst` | Long-running stress test |
| `crates/icn-core/tests/graceful_restart_stress.rs:223` | `test_high_message_volume_restart` | Stress test |
| `crates/icn-core/tests/graceful_restart_stress.rs:336` | `test_many_peers_restart` | Stress test |
| `crates/icn-core/tests/graceful_restart_stress.rs:491` | `test_multi_topic_high_subscription_restart` | Stress test |
| `crates/icn-gossip/src/gossip.rs:1928` | `test_subscribe_limit_enforcement_full` | Slow (fills 10K slots) |

### Hardware-Dependent Tests (Require TPM 2.0)

| File | Test | Reason |
|------|------|--------|
| `crates/icn-identity/src/keystore_tpm.rs:957` | `test_tpm_seal_unseal_cycle` | Requires TPM 2.0 or swtpm |
| `crates/icn-identity/src/keystore_tpm.rs:1019` | `test_tpm_with_pcr_binding` | Requires TPM 2.0 or swtpm |
| `crates/icn-identity/src/keystore_tpm.rs:1062` | `test_tpm_persistent_across_restarts` | Requires TPM 2.0 or swtpm |
| `crates/icn-identity/src/keystore_tpm.rs:1163` | `test_unseal_key_from_blob_wrong_size` | Requires TPM 2.0 or swtpm |
| `crates/icn-identity/src/keystore_tpm.rs:1484` | `test_same_handle_overwrites_previous` | Requires TPM 2.0 or swtpm |

### Flaky Tests (Need Fix or Redesign)

| File | Test | Reason | Priority |
|------|------|--------|----------|
| `crates/icn-core/tests/network_gossip_integration.rs:101` | `test_two_node_gossip_flow` | QUIC handshake timing in CI | P1 |
| `crates/icn-core/tests/multi_node_gossip_convergence.rs:264` | `test_response_handler_triggers_notifications_across_nodes` | Connection timing issues | P1 |
| `crates/icn-core/tests/multi_node_gossip_convergence.rs:423` | `test_response_handler_enforces_max_entries_across_nodes` | Connection timing issues | P1 |
| `crates/icn-core/tests/governance_integration.rs:508` | `test_governance_proposal_lifecycle` | Domain propagation timeout | P1 |
| `crates/icn-core/tests/entity_gossip_convergence.rs:260` | `test_entity_creation_syncs_across_nodes` | Needs investigation | P1 |
| `crates/icn-core/tests/entity_gossip_convergence.rs:325` | `test_entity_update_syncs_with_last_write_wins` | Needs investigation | P1 |
| `crates/icn-core/tests/entity_gossip_convergence.rs:389` | `test_membership_syncs_across_nodes` | Needs investigation | P1 |
| `crates/icn-core/tests/entity_gossip_convergence.rs:449` | `test_entity_deletion_syncs_across_nodes` | Needs investigation | P1 |

**NOTE**: Stress and TPM tests are expected to be ignored in CI. The 8 flaky tests are the ones that need attention — they represent real integration scenarios that aren't being validated.

---

## 6. Testkit Adoption

`icn-testkit` provides `TestCluster`, `TestNode`, and `poll_with_backoff` but they're underutilized. Integration tests build custom node setups instead, and use fixed `tokio::time::sleep` calls (171 occurrences across test files) which cause flakiness.

### Action Items

- [ ] **P1**: Audit the 8 flaky ignored tests (Section 5) — most could be fixed by switching to `poll_with_backoff` instead of fixed sleeps
- [ ] **P2**: Create a migration guide for existing integration tests to use `TestCluster` where appropriate
- [ ] **P2**: Replace `tokio::time::sleep` with `poll_with_backoff` in high-value integration tests (start with `icn-core/tests/`)

**Reference**: `crates/icn-testkit/src/lib.rs` — `TestCluster::new(n)`, `poll_with_backoff()`, `BackoffConfig`

---

## 7. Meaning Firewall Ratchets

The meaning firewall prevents kernel crates from importing domain types. Violations are pinned in `crates/icn-core/src/meaning_firewall.rs` — CI fails on regressions, forces count updates on progress.

### Current Ratchet Status

| Crate | Cargo.toml Deps | Source Imports | Total Refs | Status |
|-------|-----------------|----------------|------------|--------|
| `icn-gossip` | 0 | 0 | 0 | Clean |
| `icn-net` | 0 | 0 | 0 | Clean |
| `icn-gateway` | 2 (icn-trust, icn-governance) | Tracked | Tracked | Intentional (API boundary) |
| `icn-ledger` | 0 (domain crates are dev-deps only) | 0 | 0 | Clean |

**Note**: `icn-gateway` violations are architecturally intentional — the gateway is the API boundary layer that bridges kernel and domain. `icn-core` domain deps are for supervisor actor wiring. Both are tracked by ratchet tests and on the extraction roadmap.

**Detailed documentation**: [../architecture/KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md)

---

## 8. Documentation Gaps

All 34 crates have crate-level `//!` documentation in `lib.rs`, but public APIs (structs, traits, functions) have near-zero doc comments.

### Highest-Value Targets

| Crate | Public Items (approx) | Priority | Reason |
|-------|-----------------------|----------|--------|
| `icn-kernel-api` | ~484 | P1 | Trait definitions used by all apps; most critical for onboarding |
| `icn-gateway` | ~962 | P2 | Largest API surface; REST endpoint docs needed |
| `icn-ledger` | ~571 | P2 | Complex financial types need clear contracts |
| `icn-core` | ~470 | P2 | Supervisor, actor patterns need explanation |

**Recommendation**: Start with `icn-kernel-api` — it defines `PolicyOracle`, `ConstraintSet`, `PolicyDecision`, and other types that every app developer needs to understand.

---

## 9. Benchmark Gaps

6 crates have Criterion benchmarks; 4 notable crates are missing them.

### Current Benchmark Coverage

| Crate | Has Benchmarks | File |
|-------|---------------|------|
| `icn-ledger` | Yes | `benches/ledger_bench.rs` |
| `icn-trust` | Yes | `benches/trust_bench.rs` |
| `icn-gossip` | Yes | `benches/gossip_bench.rs` |
| `icn-net` | Yes | `benches/net_bench.rs` |
| `icn-gateway` | Yes | `benches/commons_bench.rs` |
| `icn-compute` | Yes | `benches/compute_bench.rs` |

### Missing Benchmarks

| Crate | Priority | What to Benchmark |
|-------|----------|-------------------|
| `icn-core` | P2 | Supervisor startup, actor spawn/shutdown, message dispatch |
| `icn-ccl` | P2 | Interpreter throughput, fuel metering overhead |
| `icn-identity` | P2 | Key generation, signing, verification, keystore unlock |
| `icn-federation` | P2 | Agreement negotiation, receipt clearing |

---

## 10. Structural Debt

### icnctl Monolith

`bins/icnctl/src/main.rs` is **10,246 lines** in a single file. It should be split into subcommand modules:

- [ ] **P1**: Extract identity commands (`id init`, `id show`, `id rotate`, `id export/import`)
- [ ] **P1**: Extract daemon control commands (`status`, `start`, `stop`)
- [ ] **P2**: Extract ledger/receipts commands, governance commands, compute commands

### Unwrap/Expect Audit

Production code enforces `#![deny(clippy::unwrap_used, clippy::expect_used)]` in 6 core crates. 5 crates remain:

| Crate | Status | Note |
|-------|--------|------|
| `icn-core` | Done | Denied since 2025-12-21 |
| `icn-net` | Done | Denied since 2025-12-21 |
| `icn-gateway` | Done | Denied since 2025-12-21 |
| `icn-gossip` | Done | Denied since 2025-12-21 |
| `icn-ledger` | Done | Denied since 2025-12-21 |
| `icnd` | Done | Denied since 2025-12-21 |
| `icnctl` | **TODO** | Not yet enforced |
| `icn-rpc` | **TODO** | Not yet enforced |
| `icn-trust` | **TODO** | Not yet enforced |
| `icn-identity` | **TODO** | Not yet enforced |
| `icn-compute` | **TODO** | Not yet enforced |

**Detailed tracking**: [code-quality-improvements.md](code-quality-improvements.md) (Error Handling Audit section)

---

## Cross-Reference Index

| Document | Relationship |
|----------|-------------|
| [code-quality-improvements.md](code-quality-improvements.md) | Error handling audit, property-based testing plans, benchmark plans |
| [module-splitting-analysis.md](module-splitting-analysis.md) | Detailed analysis of 9 largest modules with splitting recommendations |
| [module-splitting-implementation-guide.md](module-splitting-implementation-guide.md) | Step-by-step guide for splitting modules |
| [../architecture/KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md) | Meaning firewall architecture, migration roadmap |
| [../STATE.md](../STATE.md) | Current project state, code review findings |
| [../TODO.md](../TODO.md) | Active sprint work items (items graduate from this audit to TODO when active) |

---

## Maintenance

This document is a **living tracker**. Update it when:
- Items are resolved: check off the item, update the dashboard counts
- New debt is discovered: add to the appropriate section, assess priority
- Priorities change: re-assess based on current sprint needs

Review quarterly per [DOCUMENTATION_MAINTENANCE.md](../DOCUMENTATION_MAINTENANCE.md).
