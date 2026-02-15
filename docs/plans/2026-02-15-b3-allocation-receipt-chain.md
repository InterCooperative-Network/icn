# B3: Allocation Receipt Chain — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Wire governance budget decisions to structured AllocationReceipts with verifiable provenance chain, replacing ad-hoc audit JSON in the vertical slice test.

**Architecture:** Pure conversion function `create_budget_allocation()` in `icn-ledger` bridges governance decisions to `AllocationReceipt` + `SettlementIntent` types from `icn-kernel-api`. The vertical slice integration test is updated to produce and verify these receipts end-to-end. No new actors or traits.

**Tech Stack:** Rust, `icn-kernel-api` (AllocationReceipt, SettlementIntent, CanonicalReceipt), `icn-governance` (GovernanceDecisionReceipt), `icn-ledger` (new allocations module), `icn-time` (timestamps).

**Design Doc:** `docs/plans/2026-02-15-b3-allocation-receipt-chain-design.md`

---

## Pre-flight

Before starting, create the feature branch:

```bash
git checkout main && git pull origin main
git checkout -b feat/allocation-receipt-chain
```

---

### Task 1: Create allocations module with unit tests

**Files:**
- Create: `icn/crates/icn-ledger/src/allocations.rs`
- Modify: `icn/crates/icn-ledger/src/lib.rs` (add module + re-exports)

**Step 1: Write the allocations module with tests**

Create `icn/crates/icn-ledger/src/allocations.rs`:

```rust
//! Allocation receipt creation for governance → economics bridge.
//!
//! Converts governance budget decision parameters into structured
//! `AllocationReceipt` objects with `SettlementIntent` entries.
//! These receipts form the provenance chain:
//!
//! ```text
//! GovernanceDecisionReceipt.decision_hash
//!     → AllocationReceipt.decision_hash
//!         → SettlementIntent.decision_hash
//!             → JournalEntry (ledger mutation)
//! ```

use icn_kernel_api::economics::SettlementIntent;
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};
use icn_kernel_api::ScopeLevel;

/// Create an `AllocationReceipt` from a governance budget decision.
///
/// This is the bridge between governance decisions and economic settlement.
/// The receipt links `decision_hash` to a `SettlementIntent` describing
/// the treasury → recipient transfer.
///
/// # Arguments
///
/// * `decision_hash` — Canonical hash of the `GovernanceDecisionReceipt`
/// * `proposal_id` — Governance proposal ID (used as `decision_receipt_id` on the intent)
/// * `treasury_did` — Source account DID (treasury paying out)
/// * `recipient_did` — Destination account DID (receiving the allocation)
/// * `amount` — Transfer amount in smallest unit
/// * `currency` — Currency/unit symbol (e.g. "HOURS")
/// * `purpose` — Human-readable memo (excluded from canonical hash)
pub fn create_budget_allocation(
    decision_hash: Hash,
    proposal_id: &str,
    treasury_did: &str,
    recipient_did: &str,
    amount: u64,
    currency: &str,
    purpose: &str,
) -> AllocationReceipt {
    let now = icn_time::current_timestamp_secs();

    let intent = SettlementIntent::new(
        proposal_id,
        decision_hash,
        treasury_did,
        recipient_did,
        amount,
        currency,
    )
    .with_memo(purpose)
    .with_timestamp(now);

    AllocationReceipt::new(decision_hash, ScopeLevel::Org)
        .with_timestamp(now)
        .add_intent(intent)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DECISION_HASH: Hash = [42u8; 32];

    fn make_test_allocation() -> AllocationReceipt {
        create_budget_allocation(
            TEST_DECISION_HASH,
            "prop-budget-001",
            "did:icn:treasury",
            "did:icn:supplier",
            30,
            "HOURS",
            "Purchase drill press",
        )
    }

    #[test]
    fn test_creates_valid_receipt_with_one_intent() {
        let receipt = make_test_allocation();
        assert_eq!(receipt.intents.len(), 1);
        assert_eq!(receipt.decision_hash, TEST_DECISION_HASH);
        assert_eq!(receipt.scope, ScopeLevel::Org);
        assert!(receipt.created_at > 0);
    }

    #[test]
    fn test_decision_hash_propagates_to_intent() {
        let receipt = make_test_allocation();
        let intent = &receipt.intents[0];
        assert_eq!(intent.decision_hash, TEST_DECISION_HASH);
        assert_eq!(receipt.decision_hash, intent.decision_hash);
    }

    #[test]
    fn test_intent_fields_match_inputs() {
        let receipt = make_test_allocation();
        let intent = &receipt.intents[0];

        assert_eq!(intent.from, "did:icn:treasury");
        assert_eq!(intent.to, "did:icn:supplier");
        assert_eq!(intent.amount, 30);
        assert_eq!(intent.unit, "HOURS");
        assert_eq!(intent.decision_receipt_id, "prop-budget-001");
        assert_eq!(intent.memo.as_deref(), Some("Purchase drill press"));
    }

    #[test]
    fn test_provenance_chain_includes_decision_hash() {
        let receipt = make_test_allocation();
        assert!(
            receipt.provenance.upstream_hashes.contains(&TEST_DECISION_HASH),
            "provenance must link to decision_hash"
        );
    }

    #[test]
    fn test_canonical_hash_is_stable() {
        let r1 = create_budget_allocation(
            TEST_DECISION_HASH,
            "prop-001",
            "did:icn:a",
            "did:icn:b",
            100,
            "HOURS",
            "test",
        );
        let r2 = create_budget_allocation(
            TEST_DECISION_HASH,
            "prop-001",
            "did:icn:a",
            "did:icn:b",
            100,
            "HOURS",
            "test",
        );

        // Same inputs within same second → same canonical hash
        // (canonical hash excludes memo, so purpose doesn't matter)
        assert_eq!(r1.canonical_hash(), r2.canonical_hash());
    }

    #[test]
    fn test_different_amounts_produce_different_hashes() {
        let r1 = create_budget_allocation(
            TEST_DECISION_HASH,
            "prop-001",
            "did:icn:a",
            "did:icn:b",
            100,
            "HOURS",
            "test",
        );
        let r2 = create_budget_allocation(
            TEST_DECISION_HASH,
            "prop-001",
            "did:icn:a",
            "did:icn:b",
            200,
            "HOURS",
            "test",
        );

        assert_ne!(r1.canonical_hash(), r2.canonical_hash());
    }
}
```

**Step 2: Register module in lib.rs**

In `icn/crates/icn-ledger/src/lib.rs`, add after `pub mod asset_types;`:

```rust
pub mod allocations;
```

And add to re-exports:

```rust
pub use allocations::create_budget_allocation;
```

**Step 3: Run tests to verify**

```bash
cargo test -p icn-ledger --lib allocations -- --quiet
```

Expected: 6 tests pass.

**Step 4: Run fmt + clippy**

```bash
cargo fmt -p icn-ledger -- --check
cargo clippy -p icn-ledger -- -D warnings
```

Expected: Clean.

**Step 5: Commit**

```bash
git add icn/crates/icn-ledger/src/allocations.rs icn/crates/icn-ledger/src/lib.rs
git commit -m "feat(ledger): add allocation receipt creation for budget decisions (#1141)"
```

---

### Task 2: Update vertical slice test — receipt production in Step 6

**Files:**
- Modify: `icn/crates/icn-core/tests/vertical_slice_integration.rs`

The `ProposalAccepted` event handler (lines ~204-252) currently creates an ad-hoc JSON audit record. Replace it with structured receipt production.

**Step 1: Add imports at top of test file**

Add these imports (some may already be present):

```rust
use icn_governance::{GovernanceDecisionReceipt, ProofOutcome, VoteTally};
use icn_kernel_api::receipts::CanonicalReceipt;
use icn_ledger::create_budget_allocation;
```

**Step 2: Update the ProposalAccepted event handler**

Replace the event handler closure (the `event_bus.subscribe(...)` block around lines 203-252) so that after creating the `JournalEntry` it also:

1. Creates a `GovernanceDecisionReceipt` from the proposal outcome
2. Calls `create_budget_allocation()` to produce an `AllocationReceipt`
3. Stores both the receipt canonical hash and the serialized receipt in the KV store

The handler should store:
- Key `"receipt:allocation:<hex_hash>"` → serialized `AllocationReceipt`
- Key `"receipt:decision:<proposal_id>"` → serialized `GovernanceDecisionReceipt`

**Step 3: Run the test**

```bash
cargo test -p icn-core --test vertical_slice_integration -- --quiet
```

Expected: Pass (receipt production is additive, doesn't change existing behavior).

**Step 4: Commit**

```bash
git add icn/crates/icn-core/tests/vertical_slice_integration.rs
git commit -m "feat(test): produce allocation receipts in vertical slice event handler (#1141)"
```

---

### Task 3: Update vertical slice test — receipt verification in Step 9

**Files:**
- Modify: `icn/crates/icn-core/tests/vertical_slice_integration.rs`

**Step 1: Replace the ad-hoc audit verification in Step 9**

The current Step 9 loads `gov:audit:<proposal_id>` and checks JSON fields. Replace with:

1. Load the `AllocationReceipt` from KV by scanning `"receipt:allocation:"` prefix
2. Verify `receipt.decision_hash` is non-zero
3. Verify the `SettlementIntent` inside has correct from/to/amount/unit
4. Verify provenance chain: `receipt.provenance.upstream_hashes` contains the decision hash
5. Load the `GovernanceDecisionReceipt` and verify its `decision_hash` matches

Keep the existing cooperative state and member count assertions — those are still valid.

**Step 2: Run the full test**

```bash
cargo test -p icn-core --test vertical_slice_integration -- --nocapture
```

Expected: All 9 steps pass with receipt chain verification logged.

**Step 3: Commit**

```bash
git add icn/crates/icn-core/tests/vertical_slice_integration.rs
git commit -m "feat(test): verify allocation receipt chain end-to-end in vertical slice (#1141)"
```

---

### Task 4: Final verification and cleanup

**Step 1: Run full workspace lib tests**

```bash
cargo test --workspace --lib -- --quiet 2>&1 | tail -5
```

Expected: All pass (4400+ tests).

**Step 2: Run the specific integration test**

```bash
cargo test -p icn-core --test vertical_slice_integration -- --nocapture
```

Expected: 9-step vertical slice passes with receipt chain output.

**Step 3: Run fmt + clippy on workspace**

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Expected: Clean.

**Step 4: Commit any remaining changes and push**

Use `/push` skill to push the branch.

**Step 5: Create PR**

Use `/pr-create` skill targeting `main`. PR title: `feat(ledger): allocation receipt chain (#1141)`

---

## Verification Checklist

- [ ] `cargo test -p icn-ledger --lib allocations` — 6+ unit tests pass
- [ ] `cargo test -p icn-core --test vertical_slice_integration` — full chain verified
- [ ] `cargo fmt --all -- --check` — clean
- [ ] `cargo clippy --all-targets -- -D warnings` — clean
- [ ] Provenance chain: decision_hash propagates through AllocationReceipt → SettlementIntent
- [ ] No new dependencies added (uses existing icn-kernel-api types)
- [ ] No panics in non-test code (all Result returns)
