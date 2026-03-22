# Meaning Firewall Phase 2 Audit

**Status:** Sprint 21 remediations complete — `icn-compute` and `icn-security` HIGH violations resolved
**Audited:** Sprint 20 (2026-03-21) · **Remediation:** Sprint 21 (2026-03-21)
**Issue:** [#1370](https://github.com/InterCooperative-Network/icn/issues/1370)
**Prior work:** Phase 1 CI enforcement (#916, #871 — closed/complete)

---

## Executive Summary

> **The kernel enforces constraints WITHOUT understanding their semantic origin.**

The Phase 1 CI gate prevents new *import* violations. This audit identifies **semantic violations** — places where kernel crates make domain-specific decisions (encode business rules, governance policies, trust thresholds) rather than consuming generic constraints from PolicyOracle app crates.

**Original result: 8 significant violations across 4 crates. 7 crates clean.**
**Post-Sprint 21: 5 violations remediated. 3 violations remaining (deferred to Sprint 22+).**

---

## Crate-by-Crate Findings

### 1. `icn-compute` — HIGH · ✅ Partially remediated (Sprint 21)

Domain-specific policy logic embedded directly in compute kernel code.

| File | Lines | Violation | Severity | Sprint 21 Status |
|------|-------|-----------|----------|-----------------|
| `src/policy.rs` | ~98 | `min_standing: 0.3` — hardcoded trust threshold for commons pool access | HIGH | ✅ Config-driven via `ComputePolicyConfig.min_standing` (PR #1384) |
| `src/policy.rs` | ~176–184 | `check_standing()` calls trust score against hardcoded policy threshold | HIGH | ✅ Config-driven via `ComputePolicyConfig.min_standing` (PR #1384) |
| `src/policy.rs` | ~142–150 | `estimate_task_cost()` contains domain payment logic (fuel → credits, hardcoded `1000` divisor) | MEDIUM | ✅ Config-driven via `CommonsPoolPolicy.fuel_cost_divisor` (PR #1384) |
| `src/policy.rs` | ~113–137 | Credit ceiling validation against cooperative-specific rules | MEDIUM | ⏳ Deferred to Sprint 22 |
| `src/policy.rs` | ~157–173 | `CharterPriority::UbsFirst`, `EmergencyFirst` as kernel-level preemption arms | MEDIUM | ⏳ Deferred to Sprint 22 |
| `src/commons_pool.rs` | ~156–182 | `try_add_participant()` enforces sybil policy with hardcoded `min_trust_score: 0.1` | HIGH | ✅ Config-driven via `ComputePolicyConfig.min_trust_score` (PR #1384) |

**Pattern**: `icn-compute` implements admission, scheduling, and cost decisions using trust scores and charter priorities directly — domain knowledge that belongs in a `ComputePolicyOracle` app.

**Sprint 21 remediation (PR #1384)**: Extracted `min_standing`, `min_trust_score`, and `fuel_cost_divisor` into `ComputePolicyConfig` in `icn-core/src/config/compute.rs`. Wired from `lifecycle.rs` into `CommonsPoolPolicy` and `SybilPolicy` on startup. Defaults match previously-hardcoded values (zero-config-change upgrade).

**Remaining**: `CharterPriority` preemption routing and credit ceiling validation remain hardcoded. Full `ComputePolicyOracle` extraction deferred to Sprint 22.

---

### 2. `icn-security` — HIGH · ✅ Partially remediated (Sprint 21)

Reputation management encoded as hardcoded algorithms.

| File | Lines | Violation | Severity | Sprint 21 Status |
|------|-------|-----------|----------|-----------------|
| `src/misbehavior.rs` | ~100–120 | `Violation::severity()` returns hardcoded scores: Critical=10, Major=5, Minor=1 | HIGH | ✅ Config-driven via `SeverityWeights` + `severity_with_weights()` (PR #1385) |
| `src/misbehavior.rs` | ~123–139 | `StorageFailureReason::severity()`: InvalidMerkleProof=8, DataMismatch=5, NoResponse=1 | HIGH | ✅ Config-driven via `SeverityWeights.storage_*` fields (PR #1385) |
| `src/misbehavior.rs` | ~259–277 | `apply_penalty()`: `penalty = severity * 0.05` (5% per point — governance choice) | MEDIUM | ✅ Config-driven via `MisbehaviorThresholds.penalty_rate` + `ReputationPolicyConfig` (PR #1385) |
| `src/misbehavior.rs` | ~313–314 | `max_violations_per_hour: 10` — hardcoded quarantine trigger | MEDIUM | ⏳ Deferred to Sprint 22 |
| `src/misbehavior.rs` | ~318–319 | `violation_retention_secs: 7 * 24 * 3600` — hardcoded 7-day retention | MEDIUM | ⏳ Deferred to Sprint 22 |

**Pattern**: Severity scores, decay rates, quarantine thresholds, and retention policies are governance decisions hardcoded as kernel constants.

**Sprint 21 remediation (PR #1385)**: Extracted severity weights into `SeverityWeights` struct; added `severity_with_weights()` to `Violation` and `StorageFailureReason`; made `penalty_rate` a field on `MisbehaviorThresholds`; added `ReputationPolicyConfig` and `SecurityConfig` in `icn-core/src/config/security.rs`; wired from `lifecycle.rs` via `set_severity_weights()` and `set_penalty_rate()` on startup. Defaults match previously-hardcoded values.

**Remaining**: `max_violations_per_hour` and `violation_retention_secs` remain in `MisbehaviorThresholds` but are not yet exposed in `SecurityConfig`. Full `ReputationPolicyOracle` extraction deferred to Sprint 22.

---

### 3. `icn-ledger` — MEDIUM · Extract policy

Credit policy calculations use trust score inputs directly.

| File | Lines | Violation | Severity |
|------|-------|-----------|----------|
| `src/credit_policy.rs` | ~88–101 | `calculate_limit()`: `limit = baseline + (baseline * trust_score * trust_multiplier) + (cleared_volume * history_bonus_rate)` | MEDIUM |
| `src/credit_policy.rs` | ~30 | `trust_multiplier: f64` presets encode governance choices: conservative=0.3, permissive=0.5 | MEDIUM |
| `src/credit_policy.rs` | ~59–73 | `conservative()` and `permissive()` presets bake policy choices as code constants | MEDIUM |
| `src/credit_policy.rs` | ~119–144 | `NewMemberPolicy` encodes onboarding semantics (time-based ramp vs. cleared-volume bypass) | MEDIUM |

**Pattern**: `CreditPolicy` implements cooperative financial governance (who gets how much credit, on what basis) inside the ledger kernel crate. The `trust_multiplier`, ramp schedule, and baseline limits are all governance decisions.

**Remediation**: Move `CreditPolicy` to `apps/ledger`. The kernel's `SettlementEngine` receives `ConstraintSet { credit_limit, credit_multiplier }` from `LedgerPolicyOracle` per-cooperative. Remove `conservative()`/`permissive()` from kernel code.

---

### 4. `icn-core` — MEDIUM · Minor cleanup

Effect dispatcher encodes knowledge of domain effect semantics.

| File | Lines | Violation | Severity |
|------|-------|-----------|----------|
| `src/supervisor/effect_dispatcher.rs` | ~143–154 | `effect_type_label()` maps effect variants to domain labels: "treasury", "governance", "membership" | LOW |
| `src/supervisor/effect_dispatcher.rs` | ~60–125 | `execute_effects()` routes on `TreasuryEffect::Spend`, `ProtocolEffect::SetParameter` — kernel knows effect semantics | MEDIUM |

**Pattern**: The dispatcher knows what makes a "treasury" effect vs. a "governance" effect. This is semantic knowledge; a pure kernel would route to a registered handler by opaque type ID.

**Remediation**: This is lower priority. Effect routing is an intentional design decision documented in KERNEL_APP_SEPARATION.md. The label function could be removed or moved to an app-layer metric emitter. Open question: whether `execute_effects()` routing constitutes a real violation or acceptable kernel infrastructure.

---

### 5. `icn-obs` — LOW · Move thresholds

Observability layer exposes domain thresholds as constants.

| File | Lines | Violation | Severity |
|------|-------|-----------|----------|
| `src/attestation.rs` | ~82–85 | `CONTRIBUTION_THRESHOLD`, `MIN_TRUST_TO_ATTEST`, `MIN_MEMBERSHIP_AGE_SECS`, `ORG_ATTESTATION_THRESHOLD` hardcoded | MEDIUM |

**Pattern**: Governance-specific thresholds are in the observability substrate. These should come from config or app layer.

**Remediation**: Expose these as config fields on `ObsConfig`, not compiled constants. Let the attestation oracle provide them.

---

### 6–11. Clean Crates ✅

| Crate | Notes |
|-------|-------|
| `icn-gossip` | Rate limiting is generic token bucket; semantics stay in `icn-trust` app layer |
| `icn-net` | Trust checks occur via app-layer callbacks, not in transport code |
| `icn-gateway` | Domain logic correctly isolated in `governance_mgr`, `ledger_mgr`, `trust_mgr` |
| `icn-store` | Pure storage abstraction |
| `icn-rpc` | Generic JSON-RPC transport; semantics delegated to handlers |
| `icn-federation` | Topic constants are identifiers only; no semantic decisions |

---

## Summary

| Crate | Original Violations | Severity | Sprint 21 Status |
|-------|---------------------|----------|-----------------|
| `icn-compute` | 6 | HIGH | ✅ 3 remediated (min_standing, min_trust_score, fuel_cost_divisor) · ⏳ 3 deferred |
| `icn-security` | 5 | HIGH | ✅ 3 remediated (severity weights, penalty_rate) · ⏳ 2 deferred |
| `icn-ledger` | 4 | MEDIUM | ⏳ Deferred to Sprint 22 |
| `icn-core` | 2 | MEDIUM | ⏳ Deferred to Sprint 23+ |
| `icn-obs` | 1 | LOW | ⏳ Deferred to Sprint 22 |
| `icn-gossip` | 0 | — | ✅ Clean |
| `icn-net` | 0 | — | ✅ Clean |
| `icn-gateway` | 0 | — | ✅ Clean |
| `icn-store` | 0 | — | ✅ Clean |
| `icn-rpc` | 0 | — | ✅ Clean |
| `icn-federation` | 0 | — | ✅ Clean |

**Sprint 21 result**: 5 of 8 original HIGH/MEDIUM violations resolved. All HIGH violations in `icn-compute` and `icn-security` are now config-driven with governance-safe defaults.

---

## Remediation Priority Order

1. **`icn-compute` → `ComputePolicyOracle`** (Sprint 21 — partial ✅)
   - ✅ `min_standing`, `min_trust_score`, `fuel_cost_divisor` now in `ComputePolicyConfig` (PR #1384)
   - ⏳ `CharterPriority` preemption routing and credit ceiling still hardcoded
   - Full oracle extraction (full `ComputePolicyOracle`) deferred to Sprint 22

2. **`icn-security` → `ReputationPolicyOracle`** (Sprint 21 — partial ✅)
   - ✅ `SeverityWeights`, `penalty_rate` now in `ReputationPolicyConfig` (PR #1385)
   - ⏳ `max_violations_per_hour`, `violation_retention_secs` not yet in `SecurityConfig`
   - Full oracle extraction deferred to Sprint 22

3. **`icn-ledger` → `apps/ledger`** (Sprint 22)
   - Move `CreditPolicy` struct and presets to `apps/ledger`
   - `SettlementEngine` receives `ConstraintSet { credit_limit, credit_multiplier }` per cooperative
   - Remove `conservative()` / `permissive()` from kernel code

4. **`icn-obs` attestation thresholds** (Sprint 22)
   - Replace compiled constants with `ObsConfig` fields
   - Defaults match current values for backward compatibility

5. **`icn-core` effect labels** (Low priority, Sprint 23+)
   - Move `effect_type_label()` to app-layer metrics emitter
   - Or document as acceptable kernel infrastructure if effect routing is load-bearing

---

## CI Ratchet Proposal

Add a Phase 2 semantic ratchet test alongside the existing `strict_*_reference_ratchet()` tests in `icn-core/src/meaning_firewall.rs`:

```rust
// PROPOSED — not yet implemented
#[test]
fn phase_2_semantic_violation_ratchet() {
    // Pin counts of domain-semantic strings in kernel crates.
    // Can only decrease (ratchet down), never increase.
    let ratchets: &[(&str, &str, usize)] = &[
        // (crate_path, grep_pattern, max_allowed_occurrences)
        ("crates/icn-compute/src", "min_standing|min_trust_score", 2),
        ("crates/icn-security/src", r"severity\(\)|penalty|quarantine_threshold", 5),
        ("crates/icn-ledger/src", "trust_multiplier|conservative\(\)|permissive\(\)", 5),
    ];
    // Assert actual count <= max. Fail if count increases.
}
```

This ratchet prevents regression while remediation proceeds incrementally.

---

## Related Documents

- [KERNEL_APP_SEPARATION.md](./KERNEL_APP_SEPARATION.md) — architecture principles and PolicyOracle pattern
- [docs/spec/federation-settlement-finality.md](../spec/federation-settlement-finality.md) — finality spec (Sprint 20)
- Issue #1370 — tracking issue for this remediation
- Issues #916, #871 — Phase 1 CI enforcement (complete)
- PR #1384 — Sprint 21 s21-t1: `ComputePolicyConfig` extraction (merged)
- PR #1385 — Sprint 21 s21-t2: `ReputationPolicyConfig` / `SeverityWeights` extraction
