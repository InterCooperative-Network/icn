//! Integration tests for Steward management
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Tests the complete steward lifecycle including registration, status changes,
//! attestation tracking, and bond management.

use icn_gateway::commons_mgr::CommonsManager;
use icn_governance::StewardStatus;
use icn_identity::{KeyPair, POPLevel};

/// Helper to create a holder with Strong POP level (required for steward registration)
async fn create_holder_with_strong_pop(
    commons_mgr: &CommonsManager,
    member_keypair: &KeyPair,
    steward_keypair: &KeyPair,
) -> String {
    let member_did = member_keypair.did();
    let steward_did = steward_keypair.did();

    // Create anchor with steward vouch (Strong POP)
    let anchor = commons_mgr
        .create_anchor_from_enrollment(member_did, Some(steward_did))
        .await
        .unwrap();
    assert_eq!(anchor.pop_level(), Some(POPLevel::Strong));

    // Create holder
    let anchor_id = hex::encode(anchor.id());
    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, member_did)
        .await
        .unwrap();
    assert_eq!(holder.personhood_level, POPLevel::Strong);

    hex::encode(holder.id())
}

/// Test basic steward registration
#[actix_web::test]
async fn test_steward_registration() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    // Create holder with Strong POP
    let _holder_id =
        create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    // Register as steward
    let steward = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,  // 1 year term
            1000, // bond amount
            "governance-proposal-123".to_string(),
            Some("coop:test-coop".to_string()),
            vec!["enrollment".to_string(), "dispute-resolution".to_string()],
        )
        .await
        .unwrap();

    assert!(steward.is_active());
    assert!(steward.can_attest());
    assert_eq!(steward.bond_amount, 1000);
    assert_eq!(steward.jurisdiction, Some("coop:test-coop".to_string()));
    assert!(steward.specializations.contains(&"enrollment".to_string()));
    assert!(steward
        .specializations
        .contains(&"dispute-resolution".to_string()));
}

/// Test that weak POP level cannot become steward
#[actix_web::test]
async fn test_weak_pop_cannot_become_steward() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    // Create anchor without steward vouch (Weak POP)
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&member_did, None)
        .await
        .unwrap();
    assert_eq!(anchor.pop_level(), Some(POPLevel::Weak));

    let anchor_id = hex::encode(anchor.id());
    let _holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &member_did)
        .await
        .unwrap();

    // Try to register as steward - should fail
    let result = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Strong POP level"));
}

/// Test steward lookup by ID and DID
#[actix_web::test]
async fn test_steward_lookup() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    let steward = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    let steward_id = steward.steward_id.to_hex();

    // Lookup by ID
    let found = commons_mgr.get_steward(&steward_id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().steward_id.to_hex(), steward_id);

    // Lookup by DID
    let found = commons_mgr.get_steward_by_did(&member_did).await.unwrap();
    assert!(found.is_some());
    assert_eq!(
        found.unwrap().steward_did.to_string(),
        member_did.to_string()
    );
}

/// Test steward suspension and reinstatement
#[actix_web::test]
async fn test_steward_suspend_reinstate() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    let steward = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    let steward_id = steward.steward_id.to_hex();

    // Initially active
    assert!(commons_mgr.is_active_steward(&member_did).await.unwrap());

    // Suspend
    commons_mgr
        .suspend_steward(&steward_id, "Policy violation".to_string())
        .await
        .unwrap();

    let suspended = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert!(matches!(suspended.status, StewardStatus::Suspended { .. }));
    assert!(!commons_mgr.is_active_steward(&member_did).await.unwrap());
    assert!(!suspended.can_attest());

    // Reinstate
    let reinstated = commons_mgr.reinstate_steward(&steward_id).await.unwrap();
    assert!(reinstated);

    let active = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert!(matches!(active.status, StewardStatus::Active));
    assert!(commons_mgr.is_active_steward(&member_did).await.unwrap());
}

/// Test steward retirement
#[actix_web::test]
async fn test_steward_retirement() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    let steward = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    let steward_id = steward.steward_id.to_hex();

    // Retire
    commons_mgr.retire_steward(&steward_id).await.unwrap();

    let retired = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert!(matches!(retired.status, StewardStatus::Retired));
    assert!(!retired.can_attest());
}

/// Test steward revocation
#[actix_web::test]
async fn test_steward_revocation() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    let steward = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    let steward_id = steward.steward_id.to_hex();

    // Revoke with evidence
    let evidence_hash = [0u8; 32];
    commons_mgr
        .revoke_steward(&steward_id, "Fraud".to_string(), vec![evidence_hash])
        .await
        .unwrap();

    let revoked = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert!(matches!(revoked.status, StewardStatus::Revoked { .. }));
    assert!(!revoked.can_attest());
}

/// Test attestation tracking
#[actix_web::test]
async fn test_attestation_tracking() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    let steward = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    let steward_id = steward.steward_id.to_hex();
    assert_eq!(steward.attestations_issued, 0);

    // Record some attestations
    commons_mgr
        .record_steward_attestation(&steward_id)
        .await
        .unwrap();
    commons_mgr
        .record_steward_attestation(&steward_id)
        .await
        .unwrap();
    commons_mgr
        .record_steward_attestation(&steward_id)
        .await
        .unwrap();

    let updated = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert_eq!(updated.attestations_issued, 3);
}

/// Test dispute tracking
#[actix_web::test]
async fn test_dispute_tracking() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    let steward = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    let steward_id = steward.steward_id.to_hex();
    assert_eq!(steward.attestations_disputed, 0);
    assert_eq!(steward.disputes_won, 0);

    // Record disputes
    commons_mgr
        .record_steward_dispute(&steward_id)
        .await
        .unwrap();
    commons_mgr
        .record_steward_dispute(&steward_id)
        .await
        .unwrap();

    // Record a dispute win
    commons_mgr
        .record_steward_dispute_won(&steward_id)
        .await
        .unwrap();

    let updated = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert_eq!(updated.attestations_disputed, 2);
    assert_eq!(updated.disputes_won, 1);
}

/// Test bond management
#[actix_web::test]
async fn test_bond_management() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    let steward = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    let steward_id = steward.steward_id.to_hex();
    assert_eq!(steward.bond_amount, 1000);

    // Add bond
    commons_mgr
        .add_steward_bond(&steward_id, 500)
        .await
        .unwrap();

    let updated = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert_eq!(updated.bond_amount, 1500);

    // Slash bond - returns the amount slashed, not remaining
    let slashed = commons_mgr
        .slash_steward_bond(&steward_id, 300)
        .await
        .unwrap();
    assert_eq!(slashed, 300);

    let updated = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert_eq!(updated.bond_amount, 1200);

    // Slash more than available - only slashes what's available
    let slashed = commons_mgr
        .slash_steward_bond(&steward_id, 2000)
        .await
        .unwrap();
    assert_eq!(slashed, 1200); // Only 1200 was available

    let updated = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert_eq!(updated.bond_amount, 0);
}

/// Test term extension
#[actix_web::test]
async fn test_term_extension() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    let steward = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    let steward_id = steward.steward_id.to_hex();
    let original_term_end = steward.term_end;

    // Extend term by 6 months
    let new_term_end = original_term_end + (180 * 24 * 60 * 60);
    commons_mgr
        .extend_steward_term(&steward_id, new_term_end)
        .await
        .unwrap();

    let updated = commons_mgr.get_steward(&steward_id).await.unwrap().unwrap();
    assert_eq!(updated.term_end, new_term_end);
}

/// Test listing stewards with filters
#[actix_web::test]
async fn test_list_stewards() {
    let commons_mgr = CommonsManager::new();

    // Create two stewards in different jurisdictions
    let member1 = KeyPair::generate().unwrap();
    let member2 = KeyPair::generate().unwrap();
    let voucher = KeyPair::generate().unwrap();

    create_holder_with_strong_pop(&commons_mgr, &member1, &voucher).await;
    create_holder_with_strong_pop(&commons_mgr, &member2, &voucher).await;

    let steward1 = commons_mgr
        .register_steward(
            member1.did(),
            member1.did(),
            365,
            1000,
            "gov-123".to_string(),
            Some("coop:alpha".to_string()),
            vec![],
        )
        .await
        .unwrap();

    let steward2 = commons_mgr
        .register_steward(
            member2.did(),
            member2.did(),
            365,
            2000,
            "gov-456".to_string(),
            Some("coop:beta".to_string()),
            vec![],
        )
        .await
        .unwrap();

    // List all stewards
    let all = commons_mgr.list_stewards(false, None).await.unwrap();
    assert_eq!(all.len(), 2);

    // List by jurisdiction
    let alpha = commons_mgr
        .list_stewards(false, Some("coop:alpha"))
        .await
        .unwrap();
    assert_eq!(alpha.len(), 1);
    assert_eq!(alpha[0].steward_id.to_hex(), steward1.steward_id.to_hex());

    // Suspend one steward
    commons_mgr
        .suspend_steward(&steward2.steward_id.to_hex(), "test".to_string())
        .await
        .unwrap();

    // List only active
    let active = commons_mgr.list_stewards(true, None).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].steward_id.to_hex(), steward1.steward_id.to_hex());
}

/// Test list attesters (stewards who can issue attestations)
#[actix_web::test]
async fn test_list_attesters() {
    let commons_mgr = CommonsManager::new();

    let member1 = KeyPair::generate().unwrap();
    let member2 = KeyPair::generate().unwrap();
    let voucher = KeyPair::generate().unwrap();

    create_holder_with_strong_pop(&commons_mgr, &member1, &voucher).await;
    create_holder_with_strong_pop(&commons_mgr, &member2, &voucher).await;

    let steward1 = commons_mgr
        .register_steward(
            member1.did(),
            member1.did(),
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    let steward2 = commons_mgr
        .register_steward(
            member2.did(),
            member2.did(),
            365,
            1000,
            "gov-456".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    // Both should be able to attest initially
    let attesters = commons_mgr.list_attesters().await.unwrap();
    assert_eq!(attesters.len(), 2);

    // Retire one
    commons_mgr
        .retire_steward(&steward1.steward_id.to_hex())
        .await
        .unwrap();

    // Only one attester now
    let attesters = commons_mgr.list_attesters().await.unwrap();
    assert_eq!(attesters.len(), 1);
    assert_eq!(
        attesters[0].steward_id.to_hex(),
        steward2.steward_id.to_hex()
    );
}

/// Test duplicate steward registration prevention
#[actix_web::test]
async fn test_duplicate_steward_prevention() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let voucher_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    create_holder_with_strong_pop(&commons_mgr, &member_keypair, &voucher_keypair).await;

    // First registration should succeed
    commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await
        .unwrap();

    // Second registration should fail
    let result = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-456".to_string(),
            None,
            vec![],
        )
        .await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Already registered"));
}

/// Test steward without holder fails
#[actix_web::test]
async fn test_steward_without_holder_fails() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    // Try to register without creating holder first
    let result = commons_mgr
        .register_steward(
            &member_did,
            &member_did,
            365,
            1000,
            "gov-123".to_string(),
            None,
            vec![],
        )
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Holder not found"));
}
