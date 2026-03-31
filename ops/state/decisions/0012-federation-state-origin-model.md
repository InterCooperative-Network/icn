---
id: "0012"
title: "Federation State Origin Model — Gateway vs Governance vs Compute"
status: "accepted"
date: "2026-03-31"
context: "federation-clearing-position-api / post-ADR-0011 architecture pass"
deciders: ["Matt Faherty"]
tags: ["gateway", "architecture", "federation", "clearing", "state-origins", "parallel-model"]
---

# ADR 0012: Federation State Origin Model

## Status

Accepted (2026-03-31)

## Context

ADR 0011 established the canonical truth invariant: no gateway-local state for supervisor-owned
domains. It identified that federation clearing position reads must go through the supervisor's
`FederationService`. It left open the question: **how do the gateway API write paths, governance
execution write paths, and compute receipt paths relate — and what is the correct long-term model?**

This ADR answers that question by tracing every federation write path from code, performing a
per-concept convergence analysis, and defining the minimal correct architecture for the current
phase with a precise roadmap for unification.

---

## Phase 1: Federation State Origin Map

### Gateway-Originated State

**Owner**: `FederationManager` (gateway-local)
**Store**: `data_dir/federation_store` (persistent sled, per ADR 0011 fix)
**Provenance**: None — no `decision_receipt_id`, no `decision_hash`, no settlement attribution
**Guarantees**: Durable across gateway restarts, isolated from supervisor state

| Endpoint | Object Created | Store Key |
|----------|---------------|-----------|
| `POST /federation/init` | Own `CooperativeInfo` | `CooperativeRegistry` |
| `POST /federation/coops` | Peer `CooperativeInfo` | `CooperativeRegistry` |
| `POST /federation/connect` | Peer `CooperativeInfo` | `CooperativeRegistry` |
| `POST /federation/coops/{id}/vouch` | `Vouch` | `CooperativeRegistry` |
| `POST /federation/attestations` | `FederatedTrustAttestation` | `AttestationStore` |
| `POST /federation/clearing` | `BilateralClearingAgreement` | `ClearingManager` |
| `POST /federation/clearing/{id}/settle` | Settlement (gateway's ClearingManager) | `ClearingManager` |
| `POST /federation/clearing/settle-scheduled` | Multiple settlements | `ClearingManager` |
| `POST /federation/clearing/netting/{unit}/apply` | Position adjustments | `ClearingManager` |

**Visibility through gateway reads**:
- `GET /federation/coops*` → reads from `FederationManager` — shows only gateway-API-registered coops
- `GET /federation/clearing*` → reads from `FederationManager` — shows only gateway-API-created agreements
- `GET /federation/clearing/{id}/position` → reads from `FederationService` (ADR 0011) — shows supervisor state

### Supervisor/Governance-Originated State

**Owner**: `FederationServiceImpl` (supervisor-owned)
**Store**: `store_path/{federation,clearing}` (persistent sled, supervisor-controlled)
**Provenance**: Full — carries `decision_receipt_id`, `decision_hash`, `state_change_hash` on every operation
**Guarantees**: Durable, audit-attributed, settlement-linked, governance-ratified

| Origin | Kernel Effect | Object Created | Store Path |
|--------|--------------|----------------|-----------|
| CCL governance → kernel executor | `FederationEffect::JoinFederation` | `CooperativeInfo` with provenance | `store_path/federation` |
| CCL governance → kernel executor | `FederationEffect::LeaveFederation` | Departure record | `store_path/federation` |
| CCL governance → kernel executor | `FederationEffect::VouchForCoop` | `Vouch` with provenance | `store_path/federation` |
| CCL governance → kernel executor | `FederationEffect::EstablishClearing` | `BilateralClearingAgreement` with provenance | `store_path/clearing` |
| CCL governance → kernel executor | `FederationEffect::SettleClearing` | Settlement + ledger entry | `store_path/clearing` |

**Execution chain**:
```
CCL contract execution
  → KernelEffect::Federation(FederationEffect)
  → federation_effect_to_operation()
  → KernelFederationExecutor::execute_federation_operation()
  → FederationService::join_federation / vouch_for_cooperative / establish_clearing / settle_clearing
  → FederationServiceImpl (adapter)
  → CooperativeRegistry / ClearingManager at store_path/{federation,clearing}
```

### Compute-Originated Clearing State

**Owner**: `ReceiptClearingManager` (supervisor-owned, fed by compute actor)
**Store**: `store_path/clearing` (same physical store as governance-originated clearing)
**Provenance**: Attestation hash from compute receipt
**Guarantees**: Durable, per-task attributed, flushed periodically

```
Compute task completion (cross-coop)
  → ComputeActor emits clearing receipt
  → receipt_clearing_handle queue
  → periodic flush task (spawn_clearing_receipt_flush_task)
  → ReceiptClearingManager::flush_to_clearing()
  → ClearingManager::record_transfer() at store_path/clearing
```

**Note**: Compute receipts and governance-established agreements share the **same** `ClearingManager` instance at `store_path/clearing`. A compute receipt for agreement `X` accumulates into the same position that governance's `get_clearing_position("X")` reads. This is the intended design.

### Agreements API (Separate Plane)

`/v1/agreements/...` via `AgreementManagerHandle` manages inter-cooperative agreements with full
lifecycle (draft, propose, sign, suspend, resume, terminate). These are **not** bilateral clearing
agreements — they are structured documents (Trade, Credit, ResourceSharing, FederationMembership,
Custom). They do not overlap with `/federation/clearing`. Omitted from convergence analysis below.

---

## Phase 2: Per-Concept Convergence Analysis

### Cooperative Registry (coops, vouches)

| Attribute | Gateway Path | Governance Path |
|-----------|-------------|-----------------|
| Type | `CooperativeInfo`, `Vouch` | `CooperativeInfo`, `Vouch` (same types) |
| Store | `data_dir/federation_store` | `store_path/federation` |
| Provenance | None | `decision_receipt_id`, `decision_hash` |
| Read API | `GET /federation/coops` | Not exposed via gateway |
| Relationship | **Transitional** | **Canonical** |

**Gap**: Governance-registered cooperatives are invisible to `GET /federation/coops`. The read API
only queries the gateway's `FederationManager`. This is not a correctness bug — it is a read-plane
gap that will require `FederationService` read method expansion to close.

**Verdict**: **Parallel** for now. Gateway path is direct-management / exploratory / standalone
tooling. Governance path is institutional / ratified. Neither replaces the other in the current
phase.

### Attestations

| Attribute | Gateway Path | Governance Path |
|-----------|-------------|-----------------|
| Type | `FederatedTrustAttestation` | No governance effect path |
| Store | `data_dir/federation_store` | N/A |
| Provenance | None | N/A |

**Verdict**: **Standalone** — gateway is the only write path. No convergence needed.

### Bilateral Clearing Agreements

| Attribute | Gateway Path | Governance Path |
|-----------|-------------|-----------------|
| Type | `BilateralClearingAgreement` | `BilateralClearingAgreement` (same type) |
| Store | `data_dir/federation_store` | `store_path/clearing` |
| Provenance | None | `decision_receipt_id`, `decision_hash` |
| Position query | 404 in daemon mode (ADR 0011) | ✅ via `FederationService::get_clearing_position` |
| Read list | `GET /federation/clearing` (gateway-only) | Not exposed |
| Relationship | **Transitional** | **Canonical** |

**Gap (documented, acceptable)**: A clearing agreement created via `POST /federation/clearing` in
daemon mode will return 404 from `GET /federation/clearing/{id}/position` because position reads
go through the supervisor's service (ADR 0011) which only knows about governance-established
agreements. This is documented and expected: direct-API agreements are for standalone operation.

**Verdict**: **Parallel** for now. Same analysis as cooperative registry.

### Clearing Positions

| Attribute | Value |
|-----------|-------|
| Write sources | Governance (`SettleClearing`) + Compute (receipt flush) |
| Store | `store_path/clearing` — single ClearingManager instance |
| Read path | `FederationService::get_clearing_position` (supervisor) |
| Gateway fallback | `FederationManager::get_position` (standalone only) |

**Verdict**: **Already correctly unified** under the supervisor's store. Governance and compute
both write to the same `ClearingManager`. ADR 0011 fixed the read path. No further action needed.

### Settlement

| Attribute | Gateway Path | Governance Path |
|-----------|-------------|-----------------|
| Endpoint | `POST /clearing/{id}/settle` | `FederationEffect::SettleClearing` |
| Target store | `FederationManager`'s ClearingManager | `FederationServiceImpl`'s ClearingManager |
| Can settle | Only gateway-API-created agreements | Only governance-established agreements |

**Gap**: Each settlement path can only settle agreements it owns. Gateway settlement of a
governance-established agreement will return "not found"; governance settlement of a gateway-API
agreement is not possible (no governance path to the gateway's store).

**Verdict**: **Parallel** — acceptable given the transitional status of the two paths.

### Netting

| Attribute | Gateway Path | Governance Path |
|-----------|-------------|-----------------|
| Endpoint | `POST /clearing/netting/{unit}` and `/apply` | No governance netting path |
| Scope | Gateway's ClearingManager only | N/A |

**Verdict**: **Standalone** — netting only operates on gateway-API-created positions. No convergence needed or possible in current phase.

---

## Phase 3: Chosen Model

### Decision: **Model C (Explicit Parallel) with Targeted Promotion Path Design**

The two paths serve different institutional roles:

| Dimension | Gateway API Path | Governance Path |
|-----------|----------------|-----------------|
| Use case | Direct management, admin tooling, standalone ops, test setup | Democratic ratification, institutional decisions |
| Actor | API caller (admin) | Governance body (votes) |
| Provenance | None | Full audit trail (decision_receipt_id, decision_hash) |
| Guarantees | Durable state, isolated | Durable, attributed, verifiable |
| Appropriate for | Exploratory federation, direct bilateral agreements, standalone nodes | Production institutional federation, cross-coop credit, compliance-visible settlement |

**Why not Option B (Submission/Proxy)**:
- Gateway writes like `POST /federation/clearing` would need to create a governance proposal, wait for a vote, and return the agreement ID from governance execution
- This breaks the direct management use case entirely (no sync response possible)
- Governance quorum is inappropriate overhead for local administrative operations

**Why not Option A (Promotion)**:
- Gateway-created objects can't be "proposed to governance" without a CCL contract that accepts `BilateralClearingAgreement` as a proposal payload — this infrastructure doesn't exist
- Even if it did, the promotion flow is async and requires governance participation from both parties

**Why not Option D (Hybrid unification of clearing)**:
- The `FederationService` trait currently has no read methods beyond `get_clearing_position`
- Unifying read paths requires adding `list_agreements`, `list_coops`, `get_vouches` to the trait
- This is non-trivial and out of scope for the current phase

### Model C Rules (Explicit Parallel)

1. **Gateway API path = direct management / standalone tooling**. Objects created here are valid for
   standalone and direct-bilateral use. They are NOT institutional state.

2. **Governance path = canonical institutional state**. Production clearing agreements, cross-coop
   credit, and federation membership records that are compliance-visible MUST originate from
   governance execution.

3. **No silent mixing**. The API contract must be clear which path a caller is on. Currently:
   - `POST /federation/clearing` → direct-management (no provenance)
   - CCL governance execution → institutional (full provenance)
   These are not interchangeable.

4. **Read APIs reflect their write path**. `GET /federation/coops` returns gateway-local state.
   This is correct behavior, not a bug. The limitation must be documented.

5. **Position reads are already unified** (ADR 0011). This is the one crossing point.

---

## Phase 4: Precise Design Artifact — What Full Unification Requires

Full unification is **not yet implementable** without the following components:

### Prerequisite A: FederationService Read API Expansion

`FederationService` (icn-kernel-api) currently exposes only:
- `join_federation`, `vouch_for_cooperative`, `establish_clearing`, `settle_clearing` (writes)
- `get_clearing_position` (one read, added in ADR 0011)
- `is_registered`, `get_registration_provenance` (existence checks)

To unify read paths, the following methods are required:
```rust
fn list_cooperatives(&self) -> Result<Vec<CooperativeInfo>>;
fn get_cooperative(&self, coop_id: &str) -> Result<Option<CooperativeInfo>>;
fn list_agreements(&self) -> Result<Vec<BilateralClearingAgreement>>;
fn get_agreement(&self, agreement_id: &str) -> Result<Option<BilateralClearingAgreement>>;
fn get_vouches(&self, coop_id: &str) -> Result<Vec<String>>;
```

Once these exist, the gateway route handlers can be updated to prefer the service (same pattern
as `get_clearing_position` in ADR 0011).

**Blocking issue**: `BilateralClearingAgreement` is in `icn-federation` (not `icn-kernel-api`).
The trait would need to either re-export this type from kernel-api or introduce a
`ClearingAgreementView` DTO similar to `ClearingPositionView`.

### Prerequisite B: Origin Labeling

Before mixed-origin reads can be served from the gateway without confusion, objects need an
origin tag:
```rust
pub enum FederationStateOrigin {
    DirectManagement,           // gateway API, no provenance
    GovernanceRatified {        // governance execution, full provenance
        decision_receipt_id: String,
        decision_hash: String,
    },
    ComputeReceipt {            // compute task attribution
        attestation_hash: String,
    },
}
```

This allows the API to return both governance-ratified and direct-management coops/agreements
in a single list with clear provenance markers.

### Prerequisite C: Promotion Path (Optional, Long-Term)

If gateway-created objects should be promotable to governance-ratified state:
1. A CCL contract type for "adopt_direct_agreement" would accept a `BilateralClearingAgreement`
   payload and, on approval, execute `FederationEffect::EstablishClearing` with the same terms
2. The governance execution path would write to the supervisor's store with full provenance
3. The direct-management record would be superseded by the governance-ratified record

This is a long-term path, not a current requirement.

### Sequencing Plan

| Step | Prerequisite | Risk | Scope |
|------|-------------|------|-------|
| 1. Add `FederationService` read methods | None | Low | `icn-kernel-api` + `icn-core` |
| 2. Add `ClearingAgreementView` DTO | Step 1 | Low | `icn-kernel-api` |
| 3. Update gateway route handlers to prefer service for list/get | Step 1+2 | Low | `icn-gateway` |
| 4. Add origin labeling to DTOs | Step 1-3 | Medium | Cross-crate |
| 5. Implement CCL adoption contract | Steps 1-4 + CCL work | High | `icn-ccl`, `icn-governance` |

Steps 1-3 are the read unification path (no semantic risk, no write path changes).
Steps 4-5 are the full lifecycle unification (future work).

---

## Phase 5: Tests and Documentation

### Invariant Tests

The following invariants are already tested or should be added:

**Existing** (from ADR 0011 work):
- `test_get_position_prefers_federation_service_over_local_manager` — proves ADR 0011 read
  preference is enforced for position queries
- `test_persistent_storage_survives_manager_reconstruction` — proves gateway-API state is durable

**Required** (not yet added):
- **`test_gateway_clearing_and_governance_clearing_do_not_share_positions`**: create an agreement
  via gateway API path, create a different agreement via `FederationServiceImpl::establish_clearing`,
  assert `get_clearing_position` returns the governance-path agreement and 404s on the gateway-path
  agreement — proving the stores are isolated as intended.
- **`test_gateway_coop_list_reflects_only_gateway_registered_coops`**: register a coop via
  `FederationManager`, register a different coop via `FederationServiceImpl::join_federation`,
  assert `list_cooperatives` on `FederationManager` returns only the first — proving read API
  accurately reflects its write path, not governance state.

### Documentation Updates

This ADR serves as the design artifact. Gateway API documentation should note:
- "Objects created via direct management APIs are for standalone and direct-bilateral use. Production
  institutional federation should originate from CCL governance execution."
- `GET /federation/coops` and related list endpoints return only direct-management state, not
  governance-ratified state.
- `GET /federation/clearing/{id}/position` returns supervisor-owned state (governance + compute)
  and will 404 for agreements created via direct management API in daemon mode.

---

## Consequences

### What This ADR Locks In

1. **Model C is the current architecture**: two parallel paths, intentionally separate.
2. **Gateway read APIs reflect their write path** — this is correct behavior, documented as such.
3. **No state copying** between paths — divergence is the intended design.
4. **Read unification requires FederationService expansion** — do not attempt partial unification
   before Steps 1-3 above are complete.
5. **Position reads (ADR 0011) are the only crossing point** — do not add more crossing points
   without following the full wiring chain from ADR 0011.

### What This ADR Leaves Open

1. FederationService read method expansion (Step 1-3 above) — future PR, low risk.
2. Origin labeling for mixed-origin reads (Step 4) — future design.
3. CCL adoption contract for promotion path (Step 5) — future work, high complexity.

---

## References

- ADR 0011: Canonical Truth Ownership — Gateway vs Supervisor
- `crates/icn-kernel-api/src/services.rs` — FederationService trait (current + needed methods)
- `crates/icn-kernel-api/src/effects.rs` — FederationEffect enum (5 governance effect variants)
- `crates/icn-core/src/supervisor/governance_executor.rs` — KernelFederationExecutor, effect→operation bridge
- `crates/icn-core/src/services/federation_service.rs` — FederationServiceImpl adapter
- `crates/icn-core/src/supervisor/init_federation.rs` — ReceiptClearingManager setup
- `crates/icn-gateway/src/api/federation.rs` — All gateway write endpoints
- `crates/icn-gateway/src/federation_mgr.rs` — FederationManager (gateway's state layer)
