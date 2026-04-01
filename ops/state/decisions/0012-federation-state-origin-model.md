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
- `GET /federation/coops*` → prefers `FederationService` (Step 1a); falls back to `FederationManager`
- `GET /federation/clearing` → prefers `FederationService` (Step 1b); falls back to `FederationManager`
- `GET /federation/clearing/{id}` → prefers `FederationService` (Step 1b); falls back to `FederationManager`
- `GET /federation/clearing/{id}/position` → `FederationService` only (ADR 0011) — shows supervisor state

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
| Read list | `GET /federation/clearing` (fallback only) | ✅ via `FederationService::list_agreements` (Step 1b) |
| Read by ID | `GET /federation/clearing/{id}` (fallback only) | ✅ via `FederationService::get_agreement` (Step 1b) |
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

4. **Read APIs reflect their write path**. `GET /federation/coops` returns gateway-local state
   in standalone mode, governance-registered coops in daemon mode (Step 1 implemented below).

5. **Position reads are already unified** (ADR 0011). This is the original crossing point.

---

## Phase 4: Precise Design Artifact — What Full Unification Requires

## Phase 4a: Read Unification — Step 1 Implementation (2026-03-31)

### Implemented: Cooperative Registry Read Surface

Added the following to `FederationService` (icn-kernel-api) and implemented in
`FederationServiceImpl` (icn-core):

```rust
fn list_cooperatives(&self) -> Result<Vec<CooperativeView>>;
fn get_cooperative(&self, coop_id: &str) -> Result<Option<CooperativeView>>;
fn get_vouches_for(&self, coop_id: &str) -> Result<Vec<String>>;
```

New view DTO in `icn-kernel-api`:
```rust
pub struct CooperativeView {
    pub coop_id: String,
    pub name: String,
    pub public_did: String,    // DID as string — no icn-identity dep in kernel-api
    pub gateway_endpoints: Vec<String>,
    pub capabilities: Vec<String>,
    pub last_seen: u64,
    pub origin: String,        // "governance" | "direct-management" (added Step 2)
}
```

Gateway handlers updated to prefer service in daemon mode:
- `GET /federation/coops` — prefers `FederationService::list_cooperatives()`
- `GET /federation/coops/{coop_id}` — prefers `FederationService::get_cooperative()`
- `GET /federation/coops/{coop_id}/vouches` — prefers `FederationService::get_vouches_for()`

In standalone mode (no service injected), all three fall back to `FederationManager`.

**Response shape note**: In daemon mode, these endpoints return `CooperativeView` (pruned DTO)
not `CooperativeInfo` (internal type). Fields omitted from the view: `FederationPolicy`,
`CurrencyInfo`, `signature`. These are federation-internal and should not cross the boundary.

**Tests added** (4 new in `api/federation.rs`):
- `test_list_coops_prefers_federation_service` — proves service path taken over mgr
- `test_list_coops_falls_back_to_fed_mgr_when_no_service` — proves fallback path
- `test_get_coop_prefers_federation_service` — proves get + 404 behavior
- `test_get_vouches_prefers_federation_service` — proves vouch path

---

## Phase 4b: Remaining Read Unification — What Full Unification Requires

The cooperative registry reads are now unified. The following remain gateway-only reads:

```
GET /federation/clearing          → FederationManager only (no service equivalent)
GET /federation/clearing/{id}     → FederationManager only
```

### Prerequisite: ClearingAgreementView DTO

**Blocking issue**: `BilateralClearingAgreement` is in `icn-federation` (not `icn-kernel-api`).
The trait needs a `ClearingAgreementView` DTO similar to `ClearingPositionView`.

Required new service methods:
```rust
fn list_agreements(&self) -> Result<Vec<ClearingAgreementView>>;
fn get_agreement(&self, agreement_id: &str) -> Result<Option<ClearingAgreementView>>;
```

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

| Step | Prerequisite | Risk | Scope | Status |
|------|-------------|------|-------|--------|
| 1a. `list/get_cooperative`, `get_vouches_for`, `CooperativeView` DTO | None | Low | `icn-kernel-api` + `icn-core` + `icn-gateway` | ✅ Done (2026-03-31) |
| 1b. `list/get_agreement`, `ClearingAgreementView` DTO | None | Low | `icn-kernel-api` + `icn-core` + `icn-gateway` | ✅ Done (2026-04-01) |
| 2. Add origin labeling to response DTOs | Steps 1a+1b | Medium | Cross-crate | ✅ Done (2026-04-01) |
| 3a. Terms propagation fix + source reference fields | Steps 1-2 + store-isolation tests | Low | `icn-kernel-api` + `icn-core` | ✅ Done (2026-04-01) |
| 3b. Adoption provenance persistence (Sled) | Step 3a | Low | `icn-core` | ✅ Done (2026-04-01) |
| 3c. `source_agreement_id` in ClearingAgreementView | Step 3b | Low | `icn-kernel-api` + `icn-gateway` | ✅ Done (2026-04-01) |
| 3d. Adoption proposal endpoint | Steps 3a-3c + governance plumbing | High | `icn-gateway` + governance | ⏳ Future (ADR 0013) |

Step 2 is the origin labeling pass — adds `origin: "governance" | "direct-management"` to view DTOs and gateway fallback paths. See Phase 4d below.

Step 3 (ADR 0013) is the adoption contract / lifecycle unification. Steps 3a-3c are
independently implementable (terms fix + reference fields). Step 3d (adoption proposal
endpoint) requires governance plumbing and is the final piece. See ADR 0013 for full design.

---

## Phase 4c: Read Unification — Step 1b Implementation (2026-04-01)

### Implemented: Clearing Agreement Read Surface

Added the following to `FederationService` (icn-kernel-api) and implemented in
`FederationServiceImpl` (icn-core):

```rust
fn list_agreements(&self) -> Result<Vec<ClearingAgreementView>>;
fn get_agreement(&self, agreement_id: &str) -> Result<Option<ClearingAgreementView>>;
```

New view DTO in `icn-kernel-api`:
```rust
pub struct ClearingAgreementView {
    pub agreement_id: String,
    pub coop_a: String,
    pub coop_a_did: String,   // Did as string — no icn-identity dep in kernel-api
    pub coop_b: String,
    pub coop_b_did: String,
    pub settlement_interval: String, // "daily" | "weekly" | "monthly" | "manual"
    pub max_imbalance: i64,
    pub created_at: u64,
    pub exchange_rates: std::collections::HashMap<String, f64>,
    pub origin: String,       // "governance" | "direct-management" (added Step 2)
}
```

**Boundary notes**:
- `Did` mapped to `String` — `icn-kernel-api` has no `icn-identity` dependency
- `SettlementInterval` mapped to string — avoids cross-crate enum duplication, safe for future variants
- `signatures` field omitted — raw cryptographic bytes have no API utility and must not cross boundary
- `exchange_rates: HashMap<String, f64>` crosses freely (std primitives)
- `None` clearing manager: `list_agreements` returns empty vec, `get_agreement` returns `Ok(None)`

Gateway handlers updated to prefer service in daemon mode:
- `GET /federation/clearing` — prefers `FederationService::list_agreements()`
- `GET /federation/clearing/{id}` — prefers `FederationService::get_agreement()`

In standalone mode (no service injected), both fall back to `FederationManager`.

**Tests added** (3 new in `api/federation.rs`):
- `test_list_agreements_prefers_federation_service` — proves service path taken over mgr, correct shape
- `test_get_agreement_prefers_federation_service` — proves get + 404 behavior
- `test_list_agreements_falls_back_to_fed_mgr_when_no_service` — proves fallback path fires

---

## Phase 4d: Origin Labeling — Step 2 Implementation (2026-04-01)

### Why origin labeling was needed after Step 1a/1b

After Steps 1a and 1b, the 5 federation GET endpoints all have two possible code paths:
- **Service path** — reads from the supervisor's store (governance-originated state)
- **Fallback path** — reads from `FederationManager` (direct-management state)

Before this step:
1. The `origin` was entirely implicit — clients had no in-band signal indicating which path was taken
2. The fallback paths serialized raw `icn-federation` domain types (`CooperativeInfo`,
   `BilateralClearingAgreement`) with different JSON shapes than the service-path view DTOs —
   creating an undocumented shape inconsistency between daemon mode and standalone mode

### Labeling model chosen: Option A (add `origin` field to view DTOs)

Considered:
- **Option B (envelope `{ origin, data }`)**: body-breaking change, requires unwrap by clients
- **Option C (response header)**: silently ignorable, not reflected in OpenAPI
- **Option A (field on DTOs)**: consistent with existing DTO pattern, visible in body, type-safe

Chose Option A. The `origin` field is a stable string at the `CooperativeView` and
`ClearingAgreementView` boundaries. It is not a full provenance record — that requires
`decision_receipt_id` / `decision_hash`, which belong in a future promotion-path design.

### DTOs changed

`CooperativeView` in `icn-kernel-api`:
- Added `pub origin: String` — `"governance"` or `"direct-management"`

`ClearingAgreementView` in `icn-kernel-api`:
- Added `pub origin: String` — `"governance"` or `"direct-management"`

`GET /federation/coops/{id}/vouches` response (raw JSON object):
- Added `"origin"` key directly — not a typed DTO

### Handler changes

**Service path** (daemon mode): `origin = "governance"` — set in `FederationServiceImpl` mappers
in `icn-core`

**Fallback path** (standalone mode): `origin = "direct-management"` — set in gateway handler
adapters. **Important**: the fallback paths now also adapt the raw `icn-federation` types
(`CooperativeInfo`, `BilateralClearingAgreement`) to the view DTOs. This:
- Unifies the response shape regardless of which path was taken
- Eliminates the hidden shape inconsistency between daemon and standalone mode
- Ensures `signatures`, `FederationPolicy`, `CurrencyInfo` fields never reach API consumers

`ClearingPositionView` was NOT labeled — this endpoint only has one meaningful path
(`FederationService`) and the fallback already builds the same DTO from gateway state.
The handler comments explain the distinction.

### Tests added (5 new)

- `test_list_coops_origin_governance_when_service_present`
- `test_get_coop_origin_governance_when_service_present`
- `test_get_vouches_origin_governance_when_service_present`
- `test_list_agreements_origin_governance_when_service_present`
- `test_get_agreement_origin_governance_when_service_present`

Each asserts `body["origin"] == "governance"` when the stub service is injected.
Fallback path (direct-management) origin is covered implicitly by the existing fallback tests
(`test_list_coops_falls_back_to_fed_mgr_when_no_service`, etc.) which now produce labeled
`ClearingAgreementView` / `CooperativeView` responses.

### What the next slice should be

Steps 3a/3b/3c (terms propagation, adoption provenance persistence, read surface exposure) are
complete as of 2026-04-01. See Phase 4e below for details.

Step 3d (CCL adoption proposal endpoint: `POST /v1/federation/clearing/{id}/propose-adoption`) is
the remaining item for the Step 3 adoption contract sequence. It requires wiring the gateway
endpoint that accepts a direct-management agreement ID and emits an `EstablishClearing` effect with
the stored terms and `source_agreement_id` set. The governance execution path (Step 3b) is already
wired to carry these fields through to `BilateralClearingAgreement`.

Before Step 3d: consider whether origin labeling should be extended to include
`decision_receipt_id` and `decision_hash` on `"governance"` records — currently these fields are
available in `FederationProvenance` (stored in `FederationServiceImpl`) but not exposed in the
view DTOs. Exposing them would allow clients to verify governance provenance directly from the API.

---

## Phase 4e: Adoption Contract Foundation — Steps 3a/3b/3c (2026-04-01)

### What was implemented

Steps 3a, 3b, and 3c close the terms-carry-through gap and establish adoption provenance
persistence across the governance execution path.

**Step 3a — Terms propagation fix**

Extended the full effect chain to carry agreement terms from proposal through to the stored
`BilateralClearingAgreement`. Four types updated:

- `FederationEffect::EstablishClearing` (in `icn-kernel-api::effects`): added
  `settlement_interval: Option<String>`, `max_imbalance: Option<i64>`,
  `source_agreement_id: Option<String>` — all with `#[serde(default)]` for backward compat
- `FederationOperation` (in `icn-kernel-api::governance`): same three fields added with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`
- `federation_effect_to_operation()`: `EstablishClearing` arm updated to carry all three fields
- `FederationClearingRequest` (in `icn-kernel-api::services`): same three fields added

`establish_clearing()` in `icn-core::services::federation_service` updated to apply the provided
`settlement_interval` and `max_imbalance` instead of relying on `BilateralClearingAgreement::new()`
defaults (which were hardcoded to `Weekly` / `10000`).

**Step 3b — Adoption provenance persistence**

`BilateralClearingAgreement` (in `icn-federation::clearing`) extended with
`source_agreement_id: Option<String>` using `#[serde(default, skip_serializing_if = "Option::is_none")]`
for Sled backward compat. The field is populated in `establish_clearing()` from
`request.source_agreement_id` before `ClearingManager::create_agreement()` persists it.

`governance_executor.rs` updated to forward `operation.source_agreement_id` into the
`FederationClearingRequest` it constructs, completing the governance → persistence chain.

**Step 3c — Read surface exposure**

`ClearingAgreementView` (in `icn-kernel-api::services`) extended with
`source_agreement_id: Option<String>`. Both `list_agreements()` and `get_agreement()` in
`federation_service.rs` now populate this field from the stored `BilateralClearingAgreement`.

Gateway direct-management handlers in `icn-gateway::api::federation` set `source_agreement_id: None`
(direct-management agreements are not adopted from anywhere).

### Deviation from ADR 0013 design

ADR 0013 specified `exchange_rates: HashMap<String, f64>` as a term to carry through the effect
chain. This field was **deferred**: `FederationEffect` derives `PartialEq + Eq`, and `HashMap<String,
f64>` cannot satisfy `Eq` because `f64` does not implement `Eq`. Only `settlement_interval`,
`max_imbalance`, and `source_agreement_id` were added.

Resolution path when needed: wrap exchange rates in a newtype that implements `Eq` by approximate
comparison, or relax the `Eq` derive on `FederationEffect` variants that don't need it.

### Tests added (5 new in `icn-core`)

- `test_establish_clearing_terms_propagate` — verifies `settlement_interval` and `max_imbalance`
  from `FederationClearingRequest` are applied to the stored `BilateralClearingAgreement`
- `test_establish_clearing_backward_compat_no_terms` — verifies a request without terms fields
  creates an agreement with default `Weekly` / `10000` (backward compat)
- `test_source_agreement_id_persisted_via_clearing_manager` — verifies `source_agreement_id` round-
  trips through `ClearingManager::create_agreement()` and is returned by `get_agreement()`
- `test_establish_clearing_creates_fresh_positions` — verifies governance-path agreement starts
  with zero credit balances regardless of source provenance
- `test_list_agreements_returns_source_agreement_id` — verifies `list_agreements()` exposes
  `source_agreement_id` in the returned `ClearingAgreementView`

---

## Phase 5: Tests and Documentation

### Invariant Tests

The following invariants are already tested or should be added:

**Existing** (from ADR 0011 work):
- `test_get_position_prefers_federation_service_over_local_manager` — proves ADR 0011 read
  preference is enforced for position queries
- `test_persistent_storage_survives_manager_reconstruction` — proves gateway-API state is durable

**Required** (now confirmed present in `icn-gateway/src/federation_mgr.rs`):
- **`test_gateway_clearing_and_governance_clearing_do_not_share_positions`**: exists and passes.
  Proves gateway and supervisor Sled stores are isolated — creating an agreement via one path does
  not appear in the other's position reads.
- **`test_gateway_coop_list_reflects_only_gateway_registered_coops`**: exists and passes. Proves
  `FederationManager::list_cooperatives` reflects only direct-management registrations, not
  governance-ratified ones.

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

1. Step 3d: `POST /v1/federation/clearing/{id}/propose-adoption` endpoint — emits `EstablishClearing`
   effect with stored terms + `source_agreement_id` from an existing direct-management agreement.
   The governance execution and Sled persistence are already wired (Steps 3a/3b/3c); only the
   gateway intake endpoint remains.
2. Provenance field exposure on governance-origin reads: `decision_receipt_id` / `decision_hash` are
   stored in `FederationProvenance` but not yet exposed in view DTOs. Clients cannot verify
   governance origin directly from the API without this. Low-complexity follow-up.
3. `exchange_rates: HashMap<String, f64>` carry-through: deferred because `FederationEffect` derives
   `Eq` and `f64` does not implement `Eq`. Exchange rate terms do not flow through the effect chain
   today. When needed: either wrap in a newtype that implements `Eq`, or relax the `Eq` derive on
   `FederationEffect`.

Steps 1a, 1b (FederationService read expansion), Step 2 (origin labeling), and Steps 3a/3b/3c
(terms propagation, adoption provenance persistence, read surface exposure) are complete as of
2026-04-01. See Phase 4a, 4c, 4d, 4e below for details.

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
