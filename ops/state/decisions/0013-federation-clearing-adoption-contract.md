---
id: "0013"
title: "Federation Clearing Adoption Contract — Step 3 Architecture Design"
status: "proposed"
date: "2026-04-01"
context: "federation-clearing-position-api / ADR 0012 Step 3 design pass"
deciders: ["Matt Faherty"]
tags: ["gateway", "architecture", "federation", "clearing", "governance", "adoption", "ccl"]
---

# ADR 0013: Federation Clearing Adoption Contract

## Status

Proposed (design-only, 2026-04-01) — no implementation yet.

## Context

ADR 0012 established Model C (Explicit Parallel): two independent clearing paths —
gateway/direct-management and governance/supervisor-owned — intentionally separate, with
origin labeling at the read surface. Steps 1a, 1b, and 2 are complete.

Step 3 was described as "CCL adoption contract for promotion path — future work, high
complexity." This ADR derives the exact Step 3 architecture from code reality, identifies
a prerequisite capability gap in the existing governance path, and produces the design
artifact that the next implementation pass can execute from.

---

## Phase 1: Post-Merge Origin Path Map (as of 2026-04-01)

### Path 1: Direct-Management (gateway-local)

| Dimension | Value |
|-----------|-------|
| Creation trigger | `POST /v1/federation/clearing` (admin API caller) |
| Handler | `create_agreement()` in `FederationManager` |
| Canonical owner | `FederationManager` (gateway-local) |
| Persistence | `data_dir/federation_store` — Sled, per-gateway instance |
| Object created | `BilateralClearingAgreement` with REAL terms (settlement_interval, max_imbalance from request) |
| Provenance | None — no `decision_receipt_id`, no `decision_hash` |
| Read surface | `GET /v1/federation/clearing`, `/clearing/{id}` — fallback path (origin = "direct-management") |
| Position read | 404 in daemon mode (ADR 0011) — agreement unknown to FederationService |
| Settlement | `POST /clearing/{id}/settle` — gateway-local only |
| Governance-actionable | No |

### Path 2: Governance (supervisor-owned)

| Dimension | Value |
|-----------|-------|
| Creation trigger | CCL governance proposal → vote → `KernelEffect::Federation(FederationEffect::EstablishClearing)` |
| Handler | `FederationServiceImpl::establish_clearing(FederationClearingRequest)` |
| Canonical owner | `FederationServiceImpl` (supervisor-owned) |
| Persistence | `store_path/clearing` — Sled, supervisor-controlled |
| Object created | `BilateralClearingAgreement` with **HARDCODED DEFAULTS** (see critical gap below) |
| Provenance | `decision_receipt_id` + `decision_hash` — stored in `FederationProvenance` in-memory map |
| Read surface | `GET /v1/federation/clearing`, `/clearing/{id}` — preferred path (origin = "governance") |
| Position read | `GET /v1/federation/clearing/{id}/position` — supervisor-owned, works correctly |
| Settlement | `FederationEffect::SettleClearing` — governance execution path |
| Governance-actionable | Yes — full audit trail |

### Path 3: Compute/Receipt-Derived

| Dimension | Value |
|-----------|-------|
| Creation trigger | Cross-coop compute task completion → `ComputeActor` emits `ClearingReceipt` |
| Handler | `ReceiptClearingManager::flush_to_clearing()` |
| Canonical owner | `ReceiptClearingManager` (supervisor-owned, fed by compute actor) |
| Persistence | `store_path/clearing` — SAME ClearingManager as governance path |
| Object created | Position delta (`CrossCoopTransfer`) on existing agreement |
| Provenance | `receipt_hash` from compute execution |
| Read surface | Via `FederationService::get_clearing_position()` — accumulates into governance-origin positions |
| Requires | Governance-established agreement must already exist (agreement ID match) |
| Governance-actionable | Position is observable; receipt is backing evidence |

### Critical Gap: Terms Propagation (PREREQUISITE for Step 3)

**Discovery**: `FederationEffect::EstablishClearing` carries only:
```rust
EstablishClearing {
    coop_a_did: String,
    coop_b_did: String,
    agreement_hash: String,  // agreement_id, despite the name
}
```

`FederationClearingRequest` carries only:
```rust
pub struct FederationClearingRequest {
    pub coop_a_did: String,
    pub coop_b_did: String,
    pub agreement_id: String,
    pub decision_receipt_id: String,
    pub decision_hash: String,
}
```

`FederationServiceImpl::establish_clearing()` calls `BilateralClearingAgreement::new()` which
produces defaults:
- `settlement_interval: SettlementInterval::Weekly` (hardcoded)
- `max_imbalance: 10000` (hardcoded)
- `exchange_rates: HashMap::new()` (empty — no exchange rates)

**Consequence**: Every governance-established clearing agreement has identical hardcoded terms,
making governance clearing non-functional for production bilateral agreements. This is an
existing correctness gap, not introduced by this design pass.

**This gap MUST be fixed before or alongside Step 3** — adoption cannot carry real terms
through a path that ignores terms.

---

## Phase 2: Adoption Model Selection

### The question

When a direct-management agreement is "adopted" into governance, what changes?

### Options considered

**Option A: In-place promotion**

The gateway-local agreement is mutated/moved to become the governance-owned canonical record.

- **Rejected**: Violates ADR 0011/0012. The supervisor store must not be contaminated by
  gateway-local writes. Cross-store mutation creates split-brain risk and invalidates the
  "canonical owner" invariant. The gateway cannot be a write path to the supervisor store.

**Option B: Ratified re-instantiation** ← CHOSEN

The direct-management agreement is **input material** for a governance proposal. If governance
ratifies the terms, `FederationEffect::EstablishClearing` executes, creating a NEW canonical
agreement in the supervisor store with governance provenance. Two records coexist:

- Source record: original direct-management agreement in gateway store (unchanged)
- Canonical record: new governance agreement in supervisor store with full provenance

The canonical record carries `source_agreement_id` (referring to the source record for audit).
The position read surface (`GET /clearing/{id}/position`) becomes live for the canonical record.
Compute receipts can now accumulate against the canonical agreement's position.
The read surface (`GET /clearing/{id}`) shows the governance version (origin = "governance").

- **Accepted for these reasons**:
  1. No cross-store mutation — ADR 0011 preserved
  2. Governance always creates its own canonical record — ADR 0012 Model C preserved
  3. Full provenance on the canonical record (decision_receipt_id, decision_hash)
  4. The source record survives as historical context — audit trail preserved
  5. No split-brain: two different agreement_ids OR the canonical agreement explicitly
     references the source by ID
  6. Read surface naturally shows governance version because FederationService is preferred
  7. Clearing positions start fresh for the canonical agreement — no implicit position
     inheritance (which would blend two stores' state)

**Option C: Reference-link adoption**

Governance "blesses" the existing direct-management agreement by governance decision, leaving
the record in the gateway store but marking it as ratified.

- **Rejected**: The read surface (FederationService) cannot see gateway-local records. The
  position query (ADR 0011) still 404s for gateway-local agreements. Compute receipts still
  cannot accumulate. The blessed record is invisible to the canonical read path. This solves
  paperwork (provenance) but not operational correctness.

**Option D: Shared identity (same agreement_id)**

The governance-established agreement takes the same agreement_id as the direct-management
source. Clients see a single ID, but under two different stores.

- **Rejected**: Creates exactly the split-brain scenario ADR 0011/0012 guard against. Which
  record is canonical when both stores have the same ID? The read preference (FederationService
  over FederationManager) masks the conflict rather than resolving it. The two positions would
  accumulate separately and diverge.

### Chosen model: B — Ratified Re-instantiation

**State transition**:

```
Before adoption:
  gateway store:     agreement_id = "agr-001" (direct-management, no provenance)
  supervisor store:  [no entry for "agr-001"]

After adoption:
  gateway store:     agreement_id = "agr-001" (unchanged, origin = "direct-management")
  supervisor store:  agreement_id = "agr-gov-001" (governance-canonical,
                                                    decision_receipt_id = "rcpt-XYZ",
                                                    source_agreement_id = "agr-001")
```

The canonical agreement gets a new ID (governance assigns it) OR the same ID as source
(see §Clearing Position Continuity below for the tradeoff).

**Clearing position continuity**: Positions do NOT carry over. The governance-canonical
agreement starts with a clean position. If position transfer is needed, it requires explicit
governance action (`FederationEffect::SettleClearing` on the source, then a new canonical
agreement starts at zero). Silent position inheritance would violate the store-isolation
invariant.

---

## Phase 3: Contract Boundary Design

### Prerequisite 3a: Terms Propagation Fix

Before adoption can work, `FederationEffect::EstablishClearing` must carry agreement terms.
These are the minimum term fields, validated against the direct-management creation path:

**New fields for `FederationEffect::EstablishClearing`** (all with `#[serde(default)]` for
backward wire compat):

```rust
EstablishClearing {
    coop_a_did: String,
    coop_b_did: String,
    agreement_hash: String,                     // Keep for compat (= agreement_id)
    // NEW with serde defaults:
    #[serde(default = "default_settlement_interval")]
    settlement_interval: String,                // "weekly" | "daily" | "monthly" | "manual"
    #[serde(default = "default_max_imbalance")]
    max_imbalance: i64,                         // = 10000 (existing runtime default)
    #[serde(default)]
    exchange_rates: HashMap<String, f64>,       // "from:to" -> rate
    #[serde(default)]
    source_agreement_id: Option<String>,        // set only for adoption, None for fresh
}
```

**New fields for `FederationClearingRequest`** (mirroring the effect):

```rust
pub struct FederationClearingRequest {
    pub coop_a_did: String,
    pub coop_b_did: String,
    pub agreement_id: String,
    pub decision_receipt_id: String,
    pub decision_hash: String,
    // NEW:
    pub settlement_interval: String,
    pub max_imbalance: i64,
    pub exchange_rates: HashMap<String, f64>,
    pub source_agreement_id: Option<String>,    // None = fresh establishment
}
```

**`FederationServiceImpl::establish_clearing()`** must be updated to use provided terms:
```rust
let agreement = BilateralClearingAgreement::new(...)
    .with_interval(parse_settlement_interval(&request.settlement_interval)?)
    .with_max_imbalance(request.max_imbalance)
// exchange_rates applied via iterator
```

### Step 3b: Source Reference in Provenance

`FederationProvenance` (or a new `ClearingProvenance` struct) must be extended:

```rust
struct ClearingProvenance {
    decision_receipt_id: String,
    decision_hash: String,
    source_agreement_id: Option<String>,    // for adopted agreements
}
```

This is persisted alongside the clearing agreement in the supervisor store. The provenance
map key is `agreement_id` (canonical id of the governance record).

### Step 3c: Read Surface Honesty

`ClearingAgreementView` gains one new field:

```rust
pub struct ClearingAgreementView {
    // ... existing fields ...
    pub origin: String,                         // "governance" | "direct-management"
    pub source_agreement_id: Option<String>,    // Some("agr-001") for adopted, None otherwise
}
```

When `origin == "governance"` and `source_agreement_id == Some(...)`, clients know this is
an adopted agreement and can trace back to the source.

### Step 3d: Adoption Proposal Endpoint

A gateway API endpoint that reads the direct-management agreement and forms a governance
adoption proposal. This does NOT execute the effect directly — it prepares the proposal for
the governance voting flow.

**Candidate endpoint**: `POST /v1/federation/clearing/{id}/propose-adoption`

Request (minimal):
```json
{
  "proposed_canonical_id": "agr-gov-001",   // governance ID for the new canonical record
  "rationale": "Formalizing bilateral clearing agreement for institutional use"
}
```

The handler:
1. Reads the source agreement from `FederationManager` (gateway-local)
2. Extracts: `coop_a_did`, `coop_b_did`, settlement_interval, max_imbalance, exchange_rates
3. Constructs a governance proposal payload with these terms
4. Sets `source_agreement_id` to the source agreement's ID
5. Submits to the governance voting path (returns proposal_id)

The actual `FederationEffect::EstablishClearing` fires after governance approval — not
immediately on proposal submission.

### Lifecycle Sequence

```
1. Direct-management agreement exists:
   POST /v1/federation/clearing → "agr-001" in gateway store
   (origin: "direct-management", no provenance, no position query)

2. Adoption proposed:
   POST /v1/federation/clearing/agr-001/propose-adoption
   → governance proposal created with terms from "agr-001" + proposed canonical id "agr-gov-001"
   → proposal_id returned to caller

3. Governance votes:
   Members vote on the proposal through normal governance flow
   → decision_receipt_id and decision_hash assigned

4. Effect executes:
   FederationEffect::EstablishClearing {
     coop_a_did: ...,
     coop_b_did: ...,
     agreement_hash: "agr-gov-001",
     settlement_interval: "weekly",    // from source
     max_imbalance: 50000,             // from source
     exchange_rates: {...},            // from source
     source_agreement_id: Some("agr-001"),
   }
   → FederationServiceImpl::establish_clearing() creates canonical agreement in supervisor store
   → ClearingProvenance stored with decision_receipt_id + source reference

5. Read surface updated:
   GET /v1/federation/clearing → shows "agr-gov-001" (origin: "governance", source: "agr-001")
   GET /v1/federation/clearing/agr-gov-001/position → live (supervisor store)
   GET /v1/federation/clearing/agr-001 → still shows source (origin: "direct-management")
     (fallback path, gateway store, 404 on position)

6. Compute receipts begin accumulating:
   Cross-coop tasks reference "agr-gov-001"
   → ReceiptClearingManager flush writes to supervisor store under "agr-gov-001"
   → GET /v1/federation/clearing/agr-gov-001/position reflects accumulated transfers

7. Settlement:
   FederationEffect::SettleClearing { agreement_id: "agr-gov-001", ... }
   → nets position, emits ledger entry with full provenance
```

---

## Phase 4: Minimum Implementation-Ready Slice

### What is NOT yet ripe for implementation

**Step 3d (adoption proposal endpoint)** requires:
- The governance voting flow to accept a clearing adoption proposal type
- The proposal payload to map cleanly to `FederationEffect::EstablishClearing`
- Tested round-trip from proposal → vote → effect execution
- These are non-trivial and should not be rushed

### What IS implementation-ready now

**Step 3a (terms propagation fix)** is independently valuable and low-risk:
- Fix `FederationEffect::EstablishClearing` to carry terms (`#[serde(default)]` for compat)
- Fix `FederationClearingRequest` with same terms
- Fix `FederationServiceImpl::establish_clearing()` to use provided terms
- This alone makes the existing governance clearing path correct for production use
- No new API surface, no governance flow changes, no adoption mechanics needed
- Can be a standalone PR without Step 3 adoption

**Step 3b (source reference in provenance)** adds 1 optional field — can ship alongside 3a
as part of the same minimal PR.

**Step 3c (read surface)** adds 1 optional field to `ClearingAgreementView` — can ship
alongside 3a/3b.

**Recommended implementation order**:

| Slice | Name | Prerequisite | Risk | Can ship standalone? |
|-------|------|-------------|------|----------------------|
| 3a | Terms propagation fix | Store-isolation tests (verify baseline) | Low | ✅ Yes |
| 3b | Source reference in provenance | 3a | Low | ✅ Yes (same PR as 3a) |
| 3c | ClearingAgreementView.source_agreement_id | 3b | Low | ✅ Yes (same PR as 3a) |
| 3d | Adoption proposal endpoint | 3a+3b+3c + governance plumbing | High | Separate PR |

**The minimal PR ready to write today**: 3a+3b+3c together — "terms propagation + adoption
reference fields." No governance flow changes, no new API endpoints, no CCL contract types.
~5 files changed.

---

## Phase 5: Verification Strategy

### Store-isolation tests (land BEFORE Step 3 implementation)

These tests must be green before any adoption mechanics are written. They establish the
invariants that adoption must not violate:

**`test_gateway_clearing_and_governance_clearing_do_not_share_positions`**
- Create agreement "agr-dm" via `FederationManager` (direct-management)
- Create agreement "agr-gov" via `FederationServiceImpl::establish_clearing()` (governance)
- Assert `FederationService::get_clearing_position("agr-dm")` returns Err (not found in supervisor)
- Assert `FederationService::get_clearing_position("agr-gov")` returns Ok (found in supervisor)
- Assert `FederationManager::get_position("agr-gov")` returns Err (not found in gateway store)
- Proves: stores are isolated — governance agreement invisible to gateway manager and vice versa

**`test_gateway_coop_list_reflects_only_gateway_registered_coops`**
- Register "coop-a" via `FederationManager::register_own_info()` (gateway path)
- Register "coop-b" via `FederationServiceImpl::join_federation()` (governance path)
- Assert `FederationManager::list_cooperatives()` returns only "coop-a"
- Assert `FederationServiceImpl::list_cooperatives()` returns only "coop-b"
- Proves: cooperative registries are isolated — not merged, not visible cross-path

### Step 3 adoption-specific tests (land with 3a/3b/3c PR)

**`test_establish_clearing_uses_provided_terms_not_defaults`**
- Call `FederationServiceImpl::establish_clearing()` with a request carrying explicit terms
  (Monthly, max_imbalance=25000, custom exchange_rates)
- Assert the stored agreement has `settlement_interval = Monthly`, `max_imbalance = 25000`
- Proves: terms propagation fix works — defaults no longer hardcoded

**`test_establish_clearing_compat_with_missing_terms`**
- Deserialize a legacy `FederationEffect::EstablishClearing` JSON that lacks the new fields
- Assert deserialization succeeds with defaults applied
- Proves: `#[serde(default)]` backward compat preserved

**`test_establish_clearing_with_source_agreement_id_stores_provenance`**
- Call `establish_clearing()` with `source_agreement_id: Some("agr-001")`
- Assert provenance for the new agreement carries `source_agreement_id = Some("agr-001")`
- Proves: adoption reference chain is persisted

**`test_adopted_agreement_position_starts_fresh`**
- Create source agreement via FederationManager, record some mock transfers
- Create governance canonical agreement via establish_clearing with source_agreement_id
- Assert canonical agreement's initial position is zero (no inherited transfers)
- Proves: position isolation — no silent position blending across stores

**`test_read_surface_shows_source_reference_for_adopted_agreement`**
- Use FederationServiceImpl in a gateway test stub
- GET /v1/federation/clearing/{canonical_id}
- Assert response has `origin: "governance"` and `source_agreement_id: "agr-001"`
- Proves: read surface exposes adoption lineage honestly

### Timing of store-isolation tests vs. Step 3

Store-isolation tests MUST land before any Step 3 implementation begins. They are
standalone tests against existing code — they require no new types or endpoints. If they
fail, something is already broken and must be fixed before proceeding.

Step 3a/b/c tests land in the same PR as the implementation changes.

Step 3d tests land when the adoption proposal endpoint is implemented.

---

## Phase 6: Open Questions Before Implementation

These must be resolved before writing Step 3a:

**Q1: Agreement ID for adopted record**
Should the governance-canonical agreement use:
- A) A new ID assigned by governance (e.g., a UUID or a hash of the proposal)
- B) The same ID as the source agreement

Recommendation: **Option A (new ID)**. Same-ID risks split-brain (two records, one ID, two
stores). A new ID makes both records independently addressable and unambiguous. The
`source_agreement_id` field in provenance provides the link.

**Q2: What happens to positions when adoption occurs**
Should accumulated direct-management positions carry over?

Answer: **No — positions start fresh**. Position carryover would blend two stores' state.
If parties need to account for pre-adoption transfers, that requires an explicit governance
settlement of the source agreement first (resetting it to zero), followed by a fresh
canonical agreement. The governance settlement produces a ledger entry. This is the
correct audit-safe path.

**Q3: Should the source agreement remain actionable after adoption?**
Can both parties continue settling the direct-management agreement after the governance
canonical agreement exists?

Answer: **The source agreement remains in the gateway store and is not voided by adoption**.
ADR 0011 prohibits gateway mutation from governance execution. The source agreement's
operational status is a policy decision above the kernel layer. The API can add a
"superseded_by" field to the view DTO as a hint, but enforcement is the caller's
responsibility.

**Q4: Provenance storage gap**
Currently `FederationProvenance` is stored in a `RwLock<HashMap<...>>` — in-memory, not
persisted to Sled. This means provenance is lost on daemon restart. This gap exists today
for join_federation provenance too. Step 3 should persist provenance to Sled alongside the
agreement as part of the 3b implementation.

---

## Consequences

### What Step 3a/b/c locks in

1. `FederationEffect::EstablishClearing` is the canonical governance effect for creating
   clearing agreements — fresh AND adopted. No new effect variant needed for adoption.
2. `source_agreement_id` is the adoption audit link — required when adoption is the trigger.
3. Clearing positions start fresh at governance establishment. No implicit inheritance.
4. Provenance is persisted to Sled (Step 3b fixes the in-memory gap).

### What Step 3a/b/c does NOT do

1. Does not change the gateway write path (direct-management still works as-is).
2. Does not add an adoption proposal endpoint (that is Step 3d).
3. Does not modify the governance voting flow.
4. Does not add CCL contract types.
5. Does not void or supersede source agreements.
6. Does not merge stores.

### What Step 3d adds (future)

1. `POST /v1/federation/clearing/{id}/propose-adoption` endpoint
2. Proposal payload mapping to `FederationEffect::EstablishClearing` with terms + source_id
3. Round-trip: proposal → vote → effect → canonical agreement in supervisor store
4. Full lifecycle test

---

## References

- ADR 0011: Canonical Truth Ownership — Gateway vs Supervisor
- ADR 0012: Federation State Origin Model (Step 1a/1b/2 complete; Step 3 here)
- `crates/icn-kernel-api/src/effects.rs` — `FederationEffect::EstablishClearing` (needs terms extension)
- `crates/icn-kernel-api/src/services.rs` — `FederationClearingRequest`, `ClearingAgreementView`
- `crates/icn-core/src/services/federation_service.rs` — `establish_clearing()` (uses hardcoded defaults)
- `crates/icn-federation/src/clearing.rs` — `BilateralClearingAgreement::new()` (Weekly/10000 defaults)
- `crates/icn-core/src/supervisor/governance_executor.rs` — federation_effect_to_operation()
