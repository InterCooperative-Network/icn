//! Tests for `icnctl audit verify` receipt chain verification logic.
//!
//! These tests construct typed `ReceiptChainResponse` payloads and verify
//! the check logic without requiring a live gateway.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use icn_gateway::api::receipts::{
    AllocationReceiptResponse, GovernanceReceiptResponse, GovernanceVoteTallyResponse,
    ReceiptChainResponse, SettlementIntentResponse,
};

const DECISION_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn make_governance() -> GovernanceReceiptResponse {
    GovernanceReceiptResponse {
        decision_hash: DECISION_HASH.to_string(),
        proposal_id: "prop-1".to_string(),
        domain_id: "test-domain".to_string(),
        outcome: "Accepted".to_string(),
        vote_tally: GovernanceVoteTallyResponse {
            for_votes: 3,
            against_votes: 0,
            abstain_votes: 0,
        },
        vote_hash: "bb".repeat(32),
    }
}

fn make_intent(canonical_hash: &str) -> SettlementIntentResponse {
    SettlementIntentResponse {
        canonical_hash: canonical_hash.to_string(),
        decision_receipt_id: "receipt-1".to_string(),
        decision_hash: DECISION_HASH.to_string(),
        asset_type: "MutualCredit".to_string(),
        from: "did:icn:alice".to_string(),
        to: "did:icn:bob".to_string(),
        amount: 100,
        unit: "credits".to_string(),
        scope: "Local".to_string(),
        memo: None,
        created_at: 1000,
    }
}

fn make_allocation(intent_hashes: Vec<String>) -> AllocationReceiptResponse {
    AllocationReceiptResponse {
        canonical_hash: "cc".repeat(32),
        decision_hash: DECISION_HASH.to_string(),
        scope: "Local".to_string(),
        created_at: 1000,
        intent_count: intent_hashes.len(),
        intent_hashes,
    }
}

fn make_complete_chain() -> ReceiptChainResponse {
    let intent_hash = "dd".repeat(32);
    ReceiptChainResponse {
        decision_hash: DECISION_HASH.to_string(),
        governance: Some(make_governance()),
        allocations: vec![make_allocation(vec![intent_hash.clone()])],
        intents: vec![make_intent(&intent_hash)],
        chain_complete: true,
    }
}

/// Replicates the verification logic from main.rs for testing.
/// This is a direct copy of `verify_receipt_chain` — kept in sync manually
/// because icnctl's binary doesn't export it.
fn verify_receipt_chain(
    chain: &ReceiptChainResponse,
    decision_hash: &str,
) -> Vec<(String, bool, String)> {
    use std::collections::HashSet;

    let mut checks = Vec::new();

    // 1: Governance
    let has_gov = chain.governance.is_some();
    checks.push((
        "Governance receipt present".to_string(),
        has_gov,
        if let Some(ref gov) = chain.governance {
            format!("Proposal: {}", gov.proposal_id)
        } else {
            "No governance decision receipt found".to_string()
        },
    ));

    // 2: Hash consistency
    let hash_ok = chain.decision_hash == decision_hash;
    checks.push((
        "Decision hash consistent".to_string(),
        hash_ok,
        String::new(),
    ));

    // 3: Allocations present
    let has_alloc = !chain.allocations.is_empty();
    checks.push((
        "Allocation receipts present".to_string(),
        has_alloc,
        String::new(),
    ));

    // 4: Allocation provenance
    let alloc_linked = chain
        .allocations
        .iter()
        .all(|a| a.decision_hash == decision_hash);
    checks.push((
        "Allocation provenance linked".to_string(),
        !has_alloc || alloc_linked,
        String::new(),
    ));

    // 5: Intents present
    let has_intents = !chain.intents.is_empty();
    checks.push((
        "Settlement intents present".to_string(),
        has_intents,
        String::new(),
    ));

    // 6: Intent provenance
    let intents_linked = chain
        .intents
        .iter()
        .all(|i| i.decision_hash == decision_hash);
    checks.push((
        "Intent provenance linked".to_string(),
        !has_intents || intents_linked,
        String::new(),
    ));

    // 7: No orphaned intents
    let claimed: HashSet<&str> = chain
        .allocations
        .iter()
        .flat_map(|a| a.intent_hashes.iter().map(String::as_str))
        .collect();
    let orphaned_count = chain
        .intents
        .iter()
        .filter(|i| !claimed.contains(i.canonical_hash.as_str()))
        .count();
    checks.push((
        "No orphaned intents".to_string(),
        !has_intents || orphaned_count == 0,
        String::new(),
    ));

    // 8: Chain complete
    checks.push((
        "Chain complete".to_string(),
        chain.chain_complete,
        String::new(),
    ));

    checks
}

#[test]
fn test_complete_chain_passes_all_checks() {
    let chain = make_complete_chain();
    let checks = verify_receipt_chain(&chain, DECISION_HASH);

    assert_eq!(checks.len(), 8);
    for (name, passed, _) in &checks {
        assert!(passed, "Check '{}' should pass on complete chain", name);
    }
}

#[test]
fn test_missing_governance_fails() {
    let intent_hash = "dd".repeat(32);
    let chain = ReceiptChainResponse {
        decision_hash: DECISION_HASH.to_string(),
        governance: None,
        allocations: vec![make_allocation(vec![intent_hash.clone()])],
        intents: vec![make_intent(&intent_hash)],
        chain_complete: false,
    };
    let checks = verify_receipt_chain(&chain, DECISION_HASH);

    assert!(!checks[0].1, "Governance check should fail");
    assert!(!checks[7].1, "Chain complete should fail");
}

#[test]
fn test_wrong_decision_hash_fails() {
    let chain = make_complete_chain();
    let wrong_hash = "ff".repeat(32);
    let checks = verify_receipt_chain(&chain, &wrong_hash);

    assert!(!checks[1].1, "Decision hash consistency should fail");
    assert!(!checks[3].1, "Allocation provenance should fail");
    assert!(!checks[5].1, "Intent provenance should fail");
}

#[test]
fn test_orphaned_intent_detected() {
    let intent_hash = "dd".repeat(32);
    let orphan_hash = "ee".repeat(32);
    // Allocation only claims intent_hash, but we also have orphan_hash
    let chain = ReceiptChainResponse {
        decision_hash: DECISION_HASH.to_string(),
        governance: Some(make_governance()),
        allocations: vec![make_allocation(vec![intent_hash.clone()])],
        intents: vec![make_intent(&intent_hash), make_intent(&orphan_hash)],
        chain_complete: true,
    };
    let checks = verify_receipt_chain(&chain, DECISION_HASH);

    assert!(!checks[6].1, "Orphaned intent check should fail");
}

#[test]
fn test_empty_chain_reports_missing() {
    let chain = ReceiptChainResponse {
        decision_hash: DECISION_HASH.to_string(),
        governance: None,
        allocations: vec![],
        intents: vec![],
        chain_complete: false,
    };
    let checks = verify_receipt_chain(&chain, DECISION_HASH);

    assert!(!checks[0].1, "Governance should fail");
    assert!(checks[1].1, "Hash should still be consistent");
    assert!(!checks[2].1, "Allocations should fail");
    // Provenance/orphan checks pass vacuously when empty
    assert!(checks[3].1, "Allocation provenance vacuously passes");
    assert!(!checks[4].1, "Intents should fail");
    assert!(checks[5].1, "Intent provenance vacuously passes");
    assert!(checks[6].1, "Orphan check vacuously passes");
    assert!(!checks[7].1, "Chain complete should fail");
}

#[test]
fn test_serde_roundtrip_of_chain_response() {
    let chain = make_complete_chain();
    let json = serde_json::to_string(&chain).expect("serialize");
    let deserialized: ReceiptChainResponse = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(chain.decision_hash, deserialized.decision_hash);
    assert_eq!(chain.chain_complete, deserialized.chain_complete);
    assert_eq!(chain.allocations.len(), deserialized.allocations.len());
    assert_eq!(chain.intents.len(), deserialized.intents.len());
    assert!(deserialized.governance.is_some());
}
