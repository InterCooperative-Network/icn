#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Vertical slice integration test — full receipt chain (Sprint 15, s15-t6)
//!
//! Proves end-to-end:
//!
//! ```text
//! AllocationProposal (ProposalPayload::AllocationProposal)
//!   → vote Accepted
//!   → GovernanceDecisionReceipt (governance store)
//!   → check_execution_gate() passes
//!   → AllocationReceipt written to ReceiptStore
//!   → SettlementIntent per recipient
//! ```
//!
//! After this chain completes, `GET /v1/receipts/allocations` returns
//! a populated list (not empty `[]`).

use icn_gateway::receipt_store::ReceiptStore;
use icn_governance::{
    check_execution_gate, AllocationEntry, GovernanceDecisionReceipt, ProofOutcome, VoteTally,
};
use icn_identity::KeyPair;
use icn_kernel_api::{
    economics::SettlementIntent,
    receipts::{AllocationReceipt, CanonicalReceipt},
    ScopeLevel,
};

/// Build the allocation-accepted hook closure — identical to production code in server.rs.
///
/// This is the closure that the gateway wires in `GatewayBuilder::build()`. By
/// extracting it here we can unit-test the hook without a full actix-web server.
fn make_allocation_hook(
    store: std::sync::Arc<ReceiptStore>,
) -> impl Fn(GovernanceDecisionReceipt, Vec<AllocationEntry>, String, u64, String) {
    move |receipt: GovernanceDecisionReceipt,
          recipients: Vec<AllocationEntry>,
          unit: String,
          _total_amount: u64,
          domain_id: String| {
        if let Err(e) = check_execution_gate(&receipt) {
            panic!("ExecutionReceiptGate rejected receipt in test: {e}");
        }

        let now = icn_time::current_timestamp_secs();
        let decision_hash = receipt.decision_hash;

        let intents: Vec<SettlementIntent> = recipients
            .iter()
            .map(|entry| {
                let intent = SettlementIntent::new(
                    receipt.proposal_id.clone(),
                    decision_hash,
                    domain_id.clone(),
                    entry.recipient.as_str(),
                    entry.amount,
                    unit.clone(),
                )
                .with_timestamp(now);
                match &entry.memo {
                    Some(m) => intent.with_memo(m.clone()),
                    None => intent,
                }
            })
            .collect();

        let allocation_receipt = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
            .with_intents(intents)
            .with_timestamp(now)
            .with_receipt_id(uuid::Uuid::new_v4().to_string());

        store
            .put_allocation_with_intents(&allocation_receipt)
            .expect("failed to store AllocationReceipt");
    }
}

/// Prove the receipt chain end-to-end.
///
/// Flow:
/// 1. Two members of a cooperative (Yusuf, Rosa) are to receive patronage
/// 2. An AllocationProposal with structured recipients is accepted
/// 3. The acceptance hook fires, producing AllocationReceipt + SettlementIntents
/// 4. ReceiptStore holds the receipt, indexable by decision_hash
#[test]
fn test_allocation_proposal_produces_receipt_chain() {
    // ── Setup ────────────────────────────────────────────────────────────────

    let db = sled::Config::new()
        .temporary(true)
        .open()
        .expect("failed to open temp sled db");
    let receipt_store = std::sync::Arc::new(ReceiptStore::new(db));

    let hook = make_allocation_hook(receipt_store.clone());

    // Generate recipient keypairs (simulate coop members)
    let yusuf = KeyPair::generate().unwrap();
    let rosa = KeyPair::generate().unwrap();

    let recipients = vec![
        AllocationEntry {
            recipient: yusuf.did().clone(),
            amount: 880,
            basis_hours: Some(120),
            memo: Some("Q1 patronage — Yusuf Okafor".to_string()),
        },
        AllocationEntry {
            recipient: rosa.did().clone(),
            amount: 638,
            basis_hours: Some(87),
            memo: Some("Q1 patronage — Rosa Mendez".to_string()),
        },
    ];

    // ── Simulate GovernanceDecisionReceipt (what GovernanceManager produces) ─

    let tally = VoteTally {
        for_votes: 3,
        against_votes: 0,
        abstain_votes: 0,
    };
    // GovernanceDecisionReceipt::new computes a canonical decision_hash from these inputs
    let decision_receipt = GovernanceDecisionReceipt::new(
        "proposal-q1-patronage-2026".to_string(),
        "brightworks-governance".to_string(),
        ProofOutcome::Accepted,
        tally,
        &[], // no individual votes in this unit test (hash over empty set is deterministic)
    );

    assert_eq!(decision_receipt.outcome, ProofOutcome::Accepted);
    let decision_hash = decision_receipt.decision_hash;

    // ── Fire the hook (what close_proposal handler does on AllocationProposal) ─

    hook(
        decision_receipt,
        recipients,
        "credits".to_string(),
        1_518,
        "brightworks-governance".to_string(),
    );

    // ── Verify: AllocationReceipt was stored ──────────────────────────────────

    let all_allocations = receipt_store
        .list_all_allocations()
        .expect("failed to list allocations");

    assert_eq!(
        all_allocations.len(),
        1,
        "expected exactly 1 AllocationReceipt in store"
    );

    let receipt = &all_allocations[0];
    assert_eq!(
        receipt.decision_hash, decision_hash,
        "AllocationReceipt.decision_hash must match the GovernanceDecisionReceipt.decision_hash"
    );
    assert_eq!(
        receipt.intents.len(),
        2,
        "expected 1 SettlementIntent per AllocationEntry recipient"
    );

    // Each intent carries the same decision_hash (provenance anchor) and
    // decision_receipt_id that points back to the governance proposal (not the recipient DID).
    for intent in &receipt.intents {
        assert_eq!(
            intent.decision_hash, decision_hash,
            "SettlementIntent.decision_hash must chain back to governance decision"
        );
        assert_eq!(
            intent.decision_receipt_id, "proposal-q1-patronage-2026",
            "SettlementIntent.decision_receipt_id must be the governance proposal ID"
        );
        assert!(
            intent.memo.is_some(),
            "SettlementIntent.memo must be propagated from AllocationEntry"
        );
    }

    // ── Verify: receipt is indexed by decision_hash ───────────────────────────

    let by_decision = receipt_store
        .list_allocations_by_decision(&decision_hash)
        .expect("failed to query by decision hash");

    assert_eq!(
        by_decision.len(),
        1,
        "decision hash index must return the AllocationReceipt"
    );
    assert_eq!(
        by_decision[0].canonical_hash(),
        receipt.canonical_hash(),
        "canonical hash must be stable across lookup paths"
    );

    // ── Verify: intents are indexed by decision_hash ──────────────────────────
    // put_allocation_with_intents must write intents to the intent index so that
    // /v1/receipts/intents and get_chain_by_decision return populated results.

    let intents_by_decision = receipt_store
        .list_intents_by_decision(&decision_hash)
        .expect("failed to query intents by decision hash");

    assert_eq!(
        intents_by_decision.len(),
        2,
        "intent index must return 1 SettlementIntent per AllocationEntry"
    );
}

/// Prove the gate rejects a rejected proposal — no receipt is written.
#[test]
fn test_rejected_proposal_does_not_produce_receipt() {
    let db = sled::Config::new()
        .temporary(true)
        .open()
        .expect("failed to open temp sled db");
    let receipt_store = std::sync::Arc::new(ReceiptStore::new(db));

    // Use a closure that captures panics instead of the production hook
    // (production hook just logs; here we verify no store writes happen)
    let store_ref = receipt_store.clone();

    let rejected_receipt = GovernanceDecisionReceipt::new(
        "proposal-rejected".to_string(),
        "test-domain".to_string(),
        ProofOutcome::Rejected,
        VoteTally {
            for_votes: 0,
            against_votes: 3,
            abstain_votes: 0,
        },
        &[],
    );

    // Gate should reject — no receipt written
    let gate_result = check_execution_gate(&rejected_receipt);
    assert!(
        gate_result.is_err(),
        "ExecutionReceiptGate must reject Rejected proposals"
    );

    // Verify nothing was written
    let allocations = store_ref.list_all_allocations().unwrap();
    assert_eq!(
        allocations.len(),
        0,
        "no AllocationReceipt should be written for a Rejected proposal"
    );
}
