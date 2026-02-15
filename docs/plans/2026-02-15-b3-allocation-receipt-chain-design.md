# B3: Allocation Receipt Chain Design

**Date**: 2026-02-15
**Issue**: #1141
**Status**: Approved

## Problem

Governance decisions produce economic effects, but there is no structured provenance chain linking decisions to ledger mutations. The vertical slice test uses ad-hoc JSON audit records. The existing `AllocationReceipt` and `SettlementIntent` types in `icn-kernel-api` are defined but not wired into the governance → economics pipeline.

## Existing Infrastructure

Already implemented (no changes needed):

| Component | Location | Purpose |
|-----------|----------|---------|
| `AllocationReceipt` | `icn-kernel-api/src/receipts.rs` | Links decision_hash → SettlementIntents |
| `SettlementIntent` | `icn-kernel-api/src/economics.rs` | Declarative economic action |
| `CanonicalReceipt` trait | `icn-kernel-api/src/receipts.rs` | Cross-node deterministic hashing |
| `ReceiptStore` | `icn-gateway/src/receipt_store.rs` | Sled-backed receipt persistence |
| Gateway endpoints | `icn-gateway/src/api/receipts.rs` | 6 REST endpoints for receipt chain queries |
| `GovernanceDecisionReceipt` | `icn-governance/src/proof.rs` | Canonical decision hash |

## Design

### Approach: Pure Conversion Function

Add `create_budget_allocation()` in `icn-ledger/src/allocations.rs` — a pure function that converts governance budget decision parameters into a structured `AllocationReceipt` containing `SettlementIntent`(s).

No new actors. No new traits. The caller (event handler) is responsible for storing the receipt.

### New Module: `icn-ledger/src/allocations.rs`

```rust
pub fn create_budget_allocation(
    decision_hash: Hash,
    proposal_id: &str,
    treasury_did: &str,
    recipient_did: &str,
    amount: u64,
    currency: &str,
    purpose: &str,
) -> AllocationReceipt
```

Produces:
- One `SettlementIntent` (treasury → recipient, amount, currency)
- One `AllocationReceipt` wrapping the intent, linked to `decision_hash`
- Provenance anchors set to `[decision_hash]`

### Vertical Slice Test Changes

**Step 6** (ProposalAccepted handler): Replace ad-hoc JSON audit with:
1. Build `GovernanceDecisionReceipt` from proposal data
2. Call `create_budget_allocation()` to produce `AllocationReceipt`
3. Create `JournalEntry` as before (actual ledger mutation)
4. Store receipt in KV keyed by hex-encoded canonical hash

**Step 9** (Audit verification): Replace raw KV JSON check with:
1. Load `AllocationReceipt` from KV by canonical hash
2. Verify `decision_hash` linkage
3. Verify `SettlementIntent` fields match ledger entry
4. Assert complete provenance chain

### Provenance Chain (After B3)

```
GovernanceDecisionReceipt.decision_hash
    ↓ (same hash)
AllocationReceipt.decision_hash
    ↓ (same hash, via provenance anchors)
SettlementIntent.decision_hash
    ↓ (ledger mutation)
JournalEntry (debit/credit)
```

Each link is verifiable via canonical hashing. Cross-node deterministic.

### Files Changed

| File | Change |
|------|--------|
| `icn-ledger/src/allocations.rs` | **New** — `create_budget_allocation()` + 4 unit tests |
| `icn-ledger/src/lib.rs` | Add `pub mod allocations` + re-export |
| `icn-core/tests/vertical_slice_integration.rs` | Wire receipt chain in Steps 6 and 9 |

### What Does NOT Change

- `AllocationReceipt` / `SettlementIntent` types (kernel-api)
- Gateway receipt endpoints
- `ReceiptStore`
- `GovernanceDecisionReceipt`
- `JournalEntry` creation (still via builder)

## Test Plan

1. `cargo test -p icn-ledger --lib allocations` — 4 unit tests
2. `cargo test -p icn-core --test vertical_slice_integration` — full chain verification
3. fmt + clippy clean

## Risk

Low — additive module in icn-ledger, uses existing kernel-api types. Vertical slice test changes are in the event handler and verification steps only.
