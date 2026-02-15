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
use icn_kernel_api::receipts::{AllocationReceipt, Hash};
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
    use icn_kernel_api::receipts::CanonicalReceipt;

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
            receipt
                .provenance
                .upstream_hashes
                .contains(&TEST_DECISION_HASH),
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
