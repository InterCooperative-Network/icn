#![allow(unused_variables)]
#![allow(clippy::unwrap_used)]
//! Integration tests for agreement workflows
//!
//! These tests verify complete agreement lifecycle workflows including:
//! - Happy path: create → propose → sign → active
//! - Rejection: propose → reject → draft
//! - Amendment: active → propose amendment → sign → ratify
//! - Suspension: active → suspend → resume
//! - Termination: active → terminate
//! - Multi-party: 3+ cooperatives must all sign
//! - Permission checks: unauthorized operations fail
//!
//! Note: Cross-node gossip sync tests are blocked on #655 (AgreementGossipHandler wiring)

use icn_federation::agreement::{
    AgreementManager, AgreementParty, AgreementStatus, AgreementType, AmendmentChange,
    AmendmentStatus, CompensationModel, FederationMembershipTerms, InMemoryAgreementStore,
    PartyRole, ResourceType, TerminationReason, TradeItem,
};
use icn_identity::KeyPair;
use std::sync::Arc;

/// Test helper: create a manager with a keypair
fn create_manager(
    coop_id: &str,
) -> (
    AgreementManager<InMemoryAgreementStore>,
    Arc<KeyPair>,
    Arc<InMemoryAgreementStore>,
) {
    let store = Arc::new(InMemoryAgreementStore::new());
    let keypair = Arc::new(KeyPair::generate().unwrap());
    let did = keypair.did().clone();
    let manager = AgreementManager::new(store.clone(), coop_id.to_string(), did)
        .with_keypair(keypair.clone());
    (manager, keypair, store)
}

/// Test helper: create a manager sharing an existing store
fn create_manager_with_store(
    coop_id: &str,
    store: Arc<InMemoryAgreementStore>,
) -> (AgreementManager<InMemoryAgreementStore>, Arc<KeyPair>) {
    let keypair = Arc::new(KeyPair::generate().unwrap());
    let did = keypair.did().clone();
    let manager =
        AgreementManager::new(store, coop_id.to_string(), did).with_keypair(keypair.clone());
    (manager, keypair)
}

// =============================================================================
// Happy Path Tests
// =============================================================================

#[test]
fn test_trade_agreement_happy_path() {
    // Setup: Food coop and tech coop share a store (simulating synced state)
    let (food_coop, food_keypair, store) = create_manager("food-coop");
    let (tech_coop, tech_keypair) = create_manager_with_store("tech-coop", store);

    // 1. Food coop creates draft trade agreement
    let agreement = food_coop
        .create_draft(
            "Produce Exchange",
            "Monthly vegetables for tech support",
            AgreementType::Trade {
                items: vec![TradeItem::new("organic vegetables", 100, "kg", 500, "USD")],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    assert!(agreement.status.is_draft());
    assert_eq!(agreement.parties.len(), 1);

    // 2. Add tech coop as counterparty
    let tech_party = AgreementParty::new(
        tech_keypair.did().clone(),
        "tech-coop",
        PartyRole::Counterparty,
    );
    let agreement = food_coop.add_party(&agreement.id, tech_party).unwrap();
    assert_eq!(agreement.parties.len(), 2);

    // 3. Propose agreement
    let agreement = food_coop.propose(&agreement.id).unwrap();
    assert!(agreement.status.is_proposed());

    // 4. Both parties can see the proposed agreement
    let agreement_from_tech = tech_coop.get_agreement(&agreement.id).unwrap().unwrap();
    assert!(agreement_from_tech.status.is_proposed());

    // 5. Food coop signs
    let agreement = food_coop.sign(&agreement.id).unwrap();
    assert!(agreement.has_signed(food_keypair.did()));
    assert!(agreement.status.is_proposed()); // Still proposed, waiting for tech coop

    // 6. Tech coop signs
    let agreement = tech_coop.sign(&agreement.id).unwrap();
    assert!(agreement.has_signed(tech_keypair.did()));

    // 7. Agreement should now be active
    assert!(
        agreement.status.is_active(),
        "Agreement should be active after all parties sign"
    );

    // Verify both parties see active status
    let final_food = food_coop.get_agreement(&agreement.id).unwrap().unwrap();
    assert!(final_food.status.is_active());
}

#[test]
fn test_credit_line_agreement_happy_path() {
    let (lender, lender_keypair, store) = create_manager("lender-coop");
    let (borrower, borrower_keypair) = create_manager_with_store("borrower-coop", store);

    // Create credit line agreement
    let agreement = lender
        .create_draft(
            "Credit Line",
            "Emergency credit facility",
            AgreementType::Credit {
                credit_limit: 50000,
                interest_rate_bps: 350, // 3.5%
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    // Add borrower
    let borrower_party = AgreementParty::new(
        borrower_keypair.did().clone(),
        "borrower-coop",
        PartyRole::Counterparty,
    );
    lender.add_party(&agreement.id, borrower_party).unwrap();

    // Propose and sign
    lender.propose(&agreement.id).unwrap();
    lender.sign(&agreement.id).unwrap();
    let agreement = borrower.sign(&agreement.id).unwrap();

    assert!(agreement.status.is_active());

    // Verify agreement type preserved
    match &agreement.agreement_type {
        AgreementType::Credit {
            credit_limit,
            interest_rate_bps,
            currency,
        } => {
            assert_eq!(*credit_limit, 50000);
            assert_eq!(*interest_rate_bps, 350);
            assert_eq!(currency, "USD");
        }
        _ => panic!("Expected Credit agreement type"),
    }
}

// =============================================================================
// Rejection Tests
// =============================================================================

#[test]
fn test_agreement_rejection() {
    let (proposer, _, store) = create_manager("proposer-coop");
    let (counterparty, counterparty_keypair) =
        create_manager_with_store("counterparty-coop", store);

    // Create and propose
    let agreement = proposer
        .create_draft(
            "Rejected Agreement",
            "This will be rejected",
            AgreementType::Trade {
                items: vec![TradeItem::new("widgets", 50, "units", 100, "USD")],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    let counterparty_party = AgreementParty::new(
        counterparty_keypair.did().clone(),
        "counterparty-coop",
        PartyRole::Counterparty,
    );
    proposer
        .add_party(&agreement.id, counterparty_party)
        .unwrap();
    proposer.propose(&agreement.id).unwrap();

    // Counterparty rejects
    let agreement = counterparty.reject(&agreement.id).unwrap();

    // Verify reverted to draft
    assert!(
        agreement.status.is_draft(),
        "Agreement should revert to draft after rejection"
    );
    assert!(
        agreement.signatures.is_empty(),
        "Signatures should be cleared on rejection"
    );

    // Both parties see draft status
    let proposer_view = proposer.get_agreement(&agreement.id).unwrap().unwrap();
    assert!(proposer_view.status.is_draft());
}

#[test]
fn test_reject_after_partial_signatures() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());
    let (coop_c, keypair_c) = create_manager_with_store("coop-c", store);

    // Create 3-party agreement
    let agreement = coop_a
        .create_draft(
            "Three Party Deal",
            "A, B, and C",
            AgreementType::ResourceSharing {
                resource_type: ResourceType::Facility,
                duration_days: 365,
                compensation: CompensationModel::Reciprocal {
                    description: "Equal access sharing".to_string(),
                },
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_c.did().clone(), "coop-c", PartyRole::Counterparty),
        )
        .unwrap();

    // Propose and partially sign
    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    // C rejects before signing
    let agreement = coop_c.reject(&agreement.id).unwrap();

    assert!(agreement.status.is_draft());
    assert!(agreement.signatures.is_empty());
}

// =============================================================================
// Amendment Tests
// =============================================================================

#[test]
fn test_agreement_amendment() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store);

    // Create and activate agreement
    let agreement = coop_a
        .create_draft(
            "Original Agreement",
            "Will be amended",
            AgreementType::Credit {
                credit_limit: 10000,
                interest_rate_bps: 500,
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();

    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    let agreement = coop_b.sign(&agreement.id).unwrap();

    assert!(agreement.status.is_active());
    assert_eq!(agreement.version, 1);

    // Propose amendment to extend agreement duration
    let new_expiration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 365 * 24 * 3600; // 1 year from now
    let amendment = coop_a
        .propose_amendment(
            &agreement.id,
            "Extend agreement duration",
            vec![AmendmentChange::ExtendDuration { new_expiration }],
        )
        .unwrap();

    assert_eq!(amendment.status, AmendmentStatus::Proposed);

    // Both parties sign amendment
    coop_a.sign_amendment(&agreement.id, &amendment.id).unwrap();
    let amendment = coop_b.sign_amendment(&agreement.id, &amendment.id).unwrap();

    // Verify amendment is ratified
    assert_eq!(
        amendment.status,
        AmendmentStatus::Ratified,
        "Amendment should be ratified after all signatures"
    );

    // Verify agreement version incremented
    let updated = coop_a.get_agreement(&agreement.id).unwrap().unwrap();
    assert_eq!(
        updated.version, 2,
        "Version should increment after amendment"
    );
}

// =============================================================================
// Suspension Tests
// =============================================================================

#[test]
fn test_agreement_suspension_and_resumption() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store);

    // Create and activate agreement
    let agreement = coop_a
        .create_draft(
            "Suspendable Agreement",
            "Can be suspended",
            AgreementType::Trade {
                items: vec![TradeItem::new("goods", 100, "units", 50, "USD")],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();

    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    // Suspend
    let agreement = coop_a
        .suspend(&agreement.id, "Temporary pause for inventory")
        .unwrap();

    assert!(
        agreement.status.is_suspended(),
        "Agreement should be suspended"
    );

    // Both parties see suspended status
    let coop_b_view = coop_b.get_agreement(&agreement.id).unwrap().unwrap();
    assert!(coop_b_view.status.is_suspended());

    // Resume (only the party who suspended can resume)
    let agreement = coop_a.resume(&agreement.id).unwrap();

    assert!(
        agreement.status.is_active(),
        "Agreement should be active after resumption"
    );
}

// =============================================================================
// Termination Tests
// =============================================================================

#[test]
fn test_agreement_termination() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store);

    // Create and activate
    let agreement = coop_a
        .create_draft(
            "Terminatable Agreement",
            "Will be terminated",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();

    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    // Terminate
    let agreement = coop_a
        .terminate(
            &agreement.id,
            TerminationReason::MutualConsent { explanation: None },
        )
        .unwrap();

    assert!(
        agreement.status.is_terminated(),
        "Agreement should be terminated"
    );

    // Verify cannot be resumed
    let resume_result = coop_a.resume(&agreement.id);
    assert!(resume_result.is_err(), "Cannot resume terminated agreement");

    // Verify cannot be amended
    let amend_result = coop_a.propose_amendment(&agreement.id, "Invalid", vec![]);
    assert!(amend_result.is_err(), "Cannot amend terminated agreement");
}

#[test]
fn test_termination_reasons() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store);

    // Create keypairs for breach test
    let breach_did = keypair_a.did().clone();

    // Test different termination reasons
    let reasons = vec![
        TerminationReason::MutualConsent { explanation: None },
        TerminationReason::Breach {
            breaching_party: breach_did.clone(),
            breach_description: "Contract violation".to_string(),
        },
        TerminationReason::Expired,
    ];
    for reason in reasons {
        let agreement = coop_a
            .create_draft(
                "Test",
                "Test",
                AgreementType::Trade {
                    items: vec![],
                    currency: "USD".to_string(),
                },
            )
            .unwrap();

        coop_a
            .add_party(
                &agreement.id,
                AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
            )
            .unwrap();

        coop_a.propose(&agreement.id).unwrap();
        coop_a.sign(&agreement.id).unwrap();
        coop_b.sign(&agreement.id).unwrap();

        let agreement = coop_a.terminate(&agreement.id, reason.clone()).unwrap();

        match &agreement.status {
            AgreementStatus::Terminated {
                reason: stored_reason,
                ..
            } => match (&reason, stored_reason) {
                (
                    TerminationReason::MutualConsent { explanation: a },
                    TerminationReason::MutualConsent { explanation: b },
                ) => {
                    assert_eq!(a, b);
                }
                (TerminationReason::Expired, TerminationReason::Expired) => {}
                (
                    TerminationReason::Breach {
                        breaching_party: a,
                        breach_description: desc_a,
                    },
                    TerminationReason::Breach {
                        breaching_party: b,
                        breach_description: desc_b,
                    },
                ) => {
                    assert_eq!(a, b);
                    assert_eq!(desc_a, desc_b);
                }
                _ => panic!("Termination reason mismatch"),
            },
            _ => panic!("Expected Terminated status"),
        }
    }
}

// =============================================================================
// Multi-Party Tests
// =============================================================================

#[test]
fn test_three_party_agreement() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());
    let (coop_c, keypair_c) = create_manager_with_store("coop-c", store);

    // Create federation membership with 3 parties
    let agreement = coop_a
        .create_draft(
            "Three-Way Federation",
            "A, B, and C join forces",
            AgreementType::FederationMembership {
                federation_id: "fed-123".to_string(),
                terms: FederationMembershipTerms::default(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_c.did().clone(), "coop-c", PartyRole::Counterparty),
        )
        .unwrap();

    coop_a.propose(&agreement.id).unwrap();

    // A signs - still proposed
    let agreement = coop_a.sign(&agreement.id).unwrap();
    assert!(agreement.status.is_proposed());

    // B signs - still proposed (waiting for C)
    let agreement = coop_b.sign(&agreement.id).unwrap();
    assert!(
        agreement.status.is_proposed(),
        "Should still be proposed with 2/3 signatures"
    );

    // C signs - now active
    let agreement = coop_c.sign(&agreement.id).unwrap();
    assert!(
        agreement.status.is_active(),
        "Should be active with all 3 signatures"
    );

    // Verify all parties see the active agreement
    for (name, mgr) in [("A", &coop_a), ("B", &coop_b), ("C", &coop_c)] {
        let view = mgr.get_agreement(&agreement.id).unwrap().unwrap();
        assert!(
            view.status.is_active(),
            "Coop {name} should see active status"
        );
        assert_eq!(view.signatures.len(), 3);
    }
}

// =============================================================================
// Permission Tests
// =============================================================================

#[test]
fn test_only_parties_can_sign() {
    let (proposer, _, store) = create_manager("proposer-coop");
    let (counterparty, counterparty_keypair) =
        create_manager_with_store("counterparty-coop", store.clone());
    let (outsider, _) = create_manager_with_store("outsider-coop", store);

    let agreement = proposer
        .create_draft(
            "Private Deal",
            "Between proposer and counterparty only",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    proposer
        .add_party(
            &agreement.id,
            AgreementParty::new(
                counterparty_keypair.did().clone(),
                "counterparty-coop",
                PartyRole::Counterparty,
            ),
        )
        .unwrap();

    proposer.propose(&agreement.id).unwrap();

    // Outsider cannot sign
    let result = outsider.sign(&agreement.id);
    assert!(result.is_err(), "Non-party should not be able to sign");
}

#[test]
fn test_only_proposer_can_propose() {
    let (proposer, _, store) = create_manager("proposer-coop");
    let (counterparty, counterparty_keypair) =
        create_manager_with_store("counterparty-coop", store);

    let agreement = proposer
        .create_draft(
            "Test",
            "Test",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    proposer
        .add_party(
            &agreement.id,
            AgreementParty::new(
                counterparty_keypair.did().clone(),
                "counterparty-coop",
                PartyRole::Counterparty,
            ),
        )
        .unwrap();

    // Counterparty cannot propose
    let result = counterparty.propose(&agreement.id);
    assert!(result.is_err(), "Only proposer should be able to propose");
}

#[test]
fn test_cannot_sign_twice() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store);

    let agreement = coop_a
        .create_draft(
            "Test",
            "Test",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();

    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();

    // A tries to sign again
    let result = coop_a.sign(&agreement.id);
    assert!(result.is_err(), "Should not be able to sign twice");
}

#[test]
fn test_only_parties_can_suspend() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());
    let (outsider, _) = create_manager_with_store("outsider-coop", store);

    let agreement = coop_a
        .create_draft(
            "Test",
            "Test",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();

    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    // Outsider cannot suspend
    let result = outsider.suspend(&agreement.id, "Malicious suspend");
    assert!(result.is_err(), "Non-party should not be able to suspend");
}

#[test]
fn test_only_parties_can_terminate() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());
    let (outsider, _) = create_manager_with_store("outsider-coop", store);

    let agreement = coop_a
        .create_draft(
            "Test",
            "Test",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();

    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    // Outsider cannot terminate
    let result = outsider.terminate(
        &agreement.id,
        TerminationReason::MutualConsent { explanation: None },
    );
    assert!(result.is_err(), "Non-party should not be able to terminate");
}

// =============================================================================
// State Transition Tests
// =============================================================================

#[test]
fn test_cannot_sign_draft() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store);

    let agreement = coop_a
        .create_draft(
            "Test",
            "Test",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();

    // Cannot sign draft (must be proposed first)
    let result = coop_a.sign(&agreement.id);
    assert!(
        result.is_err(),
        "Should not be able to sign draft agreement"
    );
}

#[test]
fn test_cannot_suspend_draft() {
    let (coop_a, _, _store) = create_manager("coop-a");

    let agreement = coop_a
        .create_draft(
            "Test",
            "Test",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    let result = coop_a.suspend(&agreement.id, "Invalid suspend");
    assert!(result.is_err(), "Should not be able to suspend draft");
}

#[test]
fn test_cannot_amend_draft() {
    let (coop_a, _, _store) = create_manager("coop-a");

    let agreement = coop_a
        .create_draft(
            "Test",
            "Test",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    let result = coop_a.propose_amendment(&agreement.id, "Invalid", vec![]);
    assert!(result.is_err(), "Should not be able to amend draft");
}
