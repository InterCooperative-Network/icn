//! Integration tests for Commons Evolution governance flows
//!
//! Tests end-to-end flows for:
//! - Enrollment with auto-affiliation
//! - Charter creation to activation
//! - Membership lifecycle

use icn_gateway::commons_mgr::CommonsManager;
use icn_governance::{
    Charter, CharterStatus, DisputePolicy, FounderSignature, GovernanceConfig, MembershipPolicy,
    OrgType,
};
use icn_identity::{
    commons::{JurisdictionId, MembershipCapability, MembershipStatus},
    KeyPair,
};

/// Helper to create a FounderSignature
fn create_founder_signature(did: icn_identity::Did, sig_byte: u8) -> FounderSignature {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    FounderSignature {
        did,
        signature: vec![sig_byte; 64],
        timestamp: now,
        role: Some("founder".to_string()),
    }
}

/// Test that enrollment creates holder with auto-affiliation
#[actix_web::test]
async fn test_enrollment_creates_affiliated_holder() {
    let commons_mgr = CommonsManager::new();
    let steward_keypair = KeyPair::generate().unwrap();
    let steward_did = steward_keypair.did().clone();

    // Simulate enrollment completion
    let member_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();
    let coop_id = "test-coop";

    // Create anchor with steward vouch (sponsored enrollment)
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&member_did, Some(&steward_did))
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());

    // Create holder
    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &member_did)
        .await
        .unwrap();
    let holder_id = hex::encode(holder.id());

    // Auto-affiliate with coop (simulating what simple_enrollment does)
    let jurisdiction = JurisdictionId::new(format!("coop:{coop_id}"));
    let initial_capabilities = vec![MembershipCapability::Transact, MembershipCapability::Vote];

    let affiliation = commons_mgr
        .join_jurisdiction(&holder_id, jurisdiction.clone(), initial_capabilities)
        .await
        .unwrap();

    // Starts as Candidate
    assert_eq!(affiliation.membership_status, MembershipStatus::Candidate);

    // Steward vouched = auto-approve to Provisional
    commons_mgr
        .approve_membership(&holder_id, &jurisdiction)
        .await
        .unwrap();

    // Verify final state
    let affiliations = commons_mgr.list_affiliations(&holder_id).await.unwrap();
    assert_eq!(affiliations.len(), 1);
    assert_eq!(
        affiliations[0].membership_status,
        MembershipStatus::Provisional
    );
    assert!(affiliations[0]
        .capabilities
        .contains(&MembershipCapability::Transact));
    assert!(affiliations[0]
        .capabilities
        .contains(&MembershipCapability::Vote));
}

/// Test charter creation to activation flow
#[actix_web::test]
async fn test_charter_creation_to_activation() {
    let commons_mgr = CommonsManager::new();

    // Create founding members
    let founder1 = KeyPair::generate().unwrap();
    let founder2 = KeyPair::generate().unwrap();
    let founder3 = KeyPair::generate().unwrap();

    // Create a draft charter
    let mut charter = Charter::new(
        OrgType::Cooperative,
        "coop:test-governance-coop".to_string(),
        "Test Governance Cooperative".to_string(),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );

    let charter_id = charter.charter_id.to_hex();

    // Add founder signatures
    charter.add_founder(create_founder_signature(founder1.did().clone(), 1));
    charter.add_founder(create_founder_signature(founder2.did().clone(), 2));
    charter.add_founder(create_founder_signature(founder3.did().clone(), 3));

    // Store as draft
    commons_mgr.store_charter(charter).await.unwrap();

    // Verify draft status
    let stored = commons_mgr.get_charter(&charter_id).await.unwrap().unwrap();
    assert!(matches!(stored.status, CharterStatus::Draft));
    assert_eq!(stored.founders.len(), 3);

    // Activate the charter
    commons_mgr
        .update_charter_status(&charter_id, CharterStatus::Active)
        .await
        .unwrap();

    // Verify active status
    let ratified = commons_mgr.get_charter(&charter_id).await.unwrap().unwrap();
    assert!(matches!(ratified.status, CharterStatus::Active));

    // Can list by status
    let active_charters = commons_mgr
        .list_charters(None, Some(CharterStatus::Active))
        .await
        .unwrap();
    assert_eq!(active_charters.len(), 1);
}

/// Test full membership lifecycle
#[actix_web::test]
async fn test_membership_lifecycle() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();

    // Create anchor and holder
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&member_did, None)
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());
    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &member_did)
        .await
        .unwrap();
    let holder_id = hex::encode(holder.id());

    // Join a coop
    let jurisdiction = JurisdictionId::new("coop:lifecycle-test-coop");
    let affiliation = commons_mgr
        .join_jurisdiction(&holder_id, jurisdiction.clone(), vec![])
        .await
        .unwrap();

    // Starts as Candidate
    assert_eq!(affiliation.membership_status, MembershipStatus::Candidate);

    // Approve to Provisional
    commons_mgr
        .approve_membership(&holder_id, &jurisdiction)
        .await
        .unwrap();

    let affiliations = commons_mgr.list_affiliations(&holder_id).await.unwrap();
    assert_eq!(
        affiliations[0].membership_status,
        MembershipStatus::Provisional
    );

    // Promote to Member
    commons_mgr
        .promote_member(&holder_id, &jurisdiction)
        .await
        .unwrap();

    let affiliations = commons_mgr.list_affiliations(&holder_id).await.unwrap();
    assert_eq!(affiliations[0].membership_status, MembershipStatus::Member);

    // Suspend
    commons_mgr
        .update_affiliation_status(&holder_id, &jurisdiction, MembershipStatus::Suspended)
        .await
        .unwrap();

    let affiliations = commons_mgr.list_affiliations(&holder_id).await.unwrap();
    assert_eq!(
        affiliations[0].membership_status,
        MembershipStatus::Suspended
    );

    // Reinstate
    commons_mgr
        .update_affiliation_status(&holder_id, &jurisdiction, MembershipStatus::Member)
        .await
        .unwrap();

    let affiliations = commons_mgr.list_affiliations(&holder_id).await.unwrap();
    assert_eq!(affiliations[0].membership_status, MembershipStatus::Member);

    // Exit
    commons_mgr
        .leave_jurisdiction(&holder_id, &jurisdiction)
        .await
        .unwrap();

    let affiliations = commons_mgr.list_affiliations(&holder_id).await.unwrap();
    assert_eq!(affiliations[0].membership_status, MembershipStatus::Exited);
}

/// Test end-to-end coop formation flow
#[actix_web::test]
async fn test_e2e_coop_formation() {
    let commons_mgr = CommonsManager::new();
    let steward_keypair = KeyPair::generate().unwrap();
    let steward_did = steward_keypair.did().clone();

    // 1. Create charter for new coop
    let mut charter = Charter::new(
        OrgType::Cooperative,
        "coop:e2e-test-coop".to_string(),
        "E2E Test Cooperative".to_string(),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );
    let charter_id = charter.charter_id.to_hex();

    // 2. Create three founders with full enrollment
    let mut founders = Vec::new();
    for i in 0..3u8 {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create anchor with steward vouch
        let anchor = commons_mgr
            .create_anchor_from_enrollment(&did, Some(&steward_did))
            .await
            .unwrap();
        let anchor_id = hex::encode(anchor.id());

        // Create holder
        let holder = commons_mgr
            .create_holder_from_anchor(&anchor_id, &did)
            .await
            .unwrap();
        let holder_id = hex::encode(holder.id());

        // Add founder signature to charter
        charter.add_founder(create_founder_signature(did.clone(), i));

        founders.push((keypair, holder_id));
    }

    // 3. Store and activate charter
    commons_mgr.store_charter(charter).await.unwrap();
    commons_mgr
        .update_charter_status(&charter_id, CharterStatus::Active)
        .await
        .unwrap();

    // 4. Founders join the coop as members
    let jurisdiction = JurisdictionId::new("coop:e2e-test-coop");
    for (_keypair, holder_id) in &founders {
        commons_mgr
            .join_jurisdiction(
                holder_id,
                jurisdiction.clone(),
                vec![
                    MembershipCapability::Vote,
                    MembershipCapability::Propose,
                    MembershipCapability::Transact,
                    MembershipCapability::HoldOffice,
                ],
            )
            .await
            .unwrap();

        // Auto-approve and promote founders
        commons_mgr
            .approve_membership(holder_id, &jurisdiction)
            .await
            .unwrap();
        commons_mgr
            .promote_member(holder_id, &jurisdiction)
            .await
            .unwrap();

        // Verify full member status
        let affiliations = commons_mgr.list_affiliations(holder_id).await.unwrap();
        assert_eq!(affiliations[0].membership_status, MembershipStatus::Member);
    }

    // 5. Enroll a new member (not a founder)
    let new_member_keypair = KeyPair::generate().unwrap();
    let new_member_did = new_member_keypair.did().clone();

    let anchor = commons_mgr
        .create_anchor_from_enrollment(&new_member_did, Some(&steward_did))
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());
    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &new_member_did)
        .await
        .unwrap();
    let new_member_holder_id = hex::encode(holder.id());

    // 6. New member applies to join
    commons_mgr
        .join_jurisdiction(
            &new_member_holder_id,
            jurisdiction.clone(),
            vec![MembershipCapability::Transact, MembershipCapability::Vote],
        )
        .await
        .unwrap();

    // New member starts as Candidate
    let affiliations = commons_mgr
        .list_affiliations(&new_member_holder_id)
        .await
        .unwrap();
    assert_eq!(
        affiliations[0].membership_status,
        MembershipStatus::Candidate
    );

    // 7. Steward vouched, so auto-approve to Provisional
    commons_mgr
        .approve_membership(&new_member_holder_id, &jurisdiction)
        .await
        .unwrap();

    let affiliations = commons_mgr
        .list_affiliations(&new_member_holder_id)
        .await
        .unwrap();
    assert_eq!(
        affiliations[0].membership_status,
        MembershipStatus::Provisional
    );

    // 8. After probation period, promote to full Member
    commons_mgr
        .promote_member(&new_member_holder_id, &jurisdiction)
        .await
        .unwrap();

    let affiliations = commons_mgr
        .list_affiliations(&new_member_holder_id)
        .await
        .unwrap();
    assert_eq!(affiliations[0].membership_status, MembershipStatus::Member);

    // Verify coop now has 4 members total
    for (_, holder_id) in &founders {
        let affiliations = commons_mgr.list_affiliations(holder_id).await.unwrap();
        assert_eq!(affiliations[0].membership_status, MembershipStatus::Member);
    }
}

/// Test charter signing flow (signatures added one at a time)
#[actix_web::test]
async fn test_charter_signing_flow() {
    use icn_governance::FounderSignature;

    let commons_mgr = CommonsManager::new();

    // Create founding members
    let founder1 = KeyPair::generate().unwrap();
    let founder2 = KeyPair::generate().unwrap();
    let founder3 = KeyPair::generate().unwrap();

    // Create a draft charter with one founder
    let mut charter = Charter::new(
        OrgType::Cooperative,
        "coop:signing-test-coop".to_string(),
        "Signing Test Cooperative".to_string(),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );

    // Add first founder during creation
    charter.add_founder(create_founder_signature(founder1.did().clone(), 1));
    let charter_id = charter.charter_id.to_hex();

    // Store as draft
    commons_mgr.store_charter(charter).await.unwrap();

    // Verify initial state
    let stored = commons_mgr.get_charter(&charter_id).await.unwrap().unwrap();
    assert!(matches!(stored.status, CharterStatus::Draft));
    assert_eq!(stored.founders.len(), 1);

    // Add second founder signature using the new method
    let sig2 = FounderSignature {
        did: founder2.did().clone(),
        signature: vec![2u8; 64],
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        role: Some("founder".to_string()),
    };
    let updated = commons_mgr
        .add_charter_signature(&charter_id, sig2)
        .await
        .unwrap();
    assert_eq!(updated.founders.len(), 2);
    assert!(matches!(updated.status, CharterStatus::Draft));

    // Add third founder signature
    let sig3 = FounderSignature {
        did: founder3.did().clone(),
        signature: vec![3u8; 64],
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        role: Some("founder".to_string()),
    };
    let updated = commons_mgr
        .add_charter_signature(&charter_id, sig3)
        .await
        .unwrap();
    assert_eq!(updated.founders.len(), 3);

    // Now has enough founders to activate (min 3)
    commons_mgr
        .update_charter_status(&charter_id, CharterStatus::Active)
        .await
        .unwrap();

    let final_charter = commons_mgr.get_charter(&charter_id).await.unwrap().unwrap();
    assert!(matches!(final_charter.status, CharterStatus::Active));
    assert_eq!(final_charter.founders.len(), 3);
}

/// Test duplicate signature is rejected
#[actix_web::test]
async fn test_charter_duplicate_signature_rejected() {
    use icn_governance::FounderSignature;

    let commons_mgr = CommonsManager::new();

    let founder1 = KeyPair::generate().unwrap();

    // Create a draft charter with one founder
    let mut charter = Charter::new(
        OrgType::Cooperative,
        "coop:dup-sig-test".to_string(),
        "Duplicate Signature Test".to_string(),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );
    charter.add_founder(create_founder_signature(founder1.did().clone(), 1));
    let charter_id = charter.charter_id.to_hex();
    commons_mgr.store_charter(charter).await.unwrap();

    // Try to add duplicate signature (same DID)
    let dup_sig = FounderSignature {
        did: founder1.did().clone(),
        signature: vec![99u8; 64],
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        role: Some("founder".to_string()),
    };

    let result = commons_mgr
        .add_charter_signature(&charter_id, dup_sig)
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already signed"));
}

/// Test amendment add-change flow
#[actix_web::test]
async fn test_amendment_add_change_flow() {
    use icn_governance::{
        Amendment, AmendmentChange, AmendmentScope, AmendmentType, ChangeTarget, ChangeType,
    };

    let commons_mgr = CommonsManager::new();

    let proposer = KeyPair::generate().unwrap();
    let proposer_did = proposer.did().clone();

    // Create a draft amendment
    let amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:add-change-test".to_string(),
        },
        "Test Amendment".to_string(),
        "An amendment to test add-change functionality".to_string(),
        proposer_did,
    );
    let amendment_id = amendment.id.to_hex();

    // Store the amendment
    commons_mgr.store_amendment(amendment).await.unwrap();

    // Verify initial state - no changes yet
    let amendment_bytes: [u8; 32] = hex::decode(&amendment_id).unwrap().try_into().unwrap();
    let stored = commons_mgr
        .get_amendment(&icn_governance::AmendmentId::new(amendment_bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.changes.len(), 0);

    // Add first change
    let change1 = AmendmentChange {
        target: ChangeTarget::GovernanceRules,
        change_type: ChangeType::Modify,
        description: "Increase quorum requirement".to_string(),
        old_value: Some("50%".to_string()),
        new_value: "67%".to_string(),
    };
    let updated = commons_mgr
        .add_amendment_change(&amendment_id, change1)
        .await
        .unwrap();
    assert_eq!(updated.changes.len(), 1);
    assert_eq!(
        updated.changes[0].description,
        "Increase quorum requirement"
    );

    // Add second change
    let change2 = AmendmentChange {
        target: ChangeTarget::MembershipPolicy,
        change_type: ChangeType::Add,
        description: "Add probation period".to_string(),
        old_value: None,
        new_value: "90 days".to_string(),
    };
    let updated = commons_mgr
        .add_amendment_change(&amendment_id, change2)
        .await
        .unwrap();
    assert_eq!(updated.changes.len(), 2);
    assert_eq!(updated.changes[1].description, "Add probation period");

    // Add third change
    let change3 = AmendmentChange {
        target: ChangeTarget::EconomicPolicy,
        change_type: ChangeType::Modify,
        description: "Update fee structure".to_string(),
        old_value: Some("$10/month".to_string()),
        new_value: "$15/month".to_string(),
    };
    let updated = commons_mgr
        .add_amendment_change(&amendment_id, change3)
        .await
        .unwrap();
    assert_eq!(updated.changes.len(), 3);

    // Verify final state
    let final_amendment = commons_mgr
        .get_amendment(&icn_governance::AmendmentId::new(amendment_bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_amendment.changes.len(), 3);
}

/// Test add-change fails for non-draft amendments
#[actix_web::test]
async fn test_amendment_add_change_fails_after_submit() {
    use icn_governance::{
        Amendment, AmendmentChange, AmendmentScope, AmendmentType, ChangeTarget, ChangeType,
    };

    let commons_mgr = CommonsManager::new();

    let proposer = KeyPair::generate().unwrap();
    let proposer_did = proposer.did().clone();

    // Create amendment with a change (required for submission)
    let mut amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:submit-test".to_string(),
        },
        "Submitted Amendment".to_string(),
        "An amendment to test add-change after submit".to_string(),
        proposer_did.clone(),
    );
    amendment.add_change(AmendmentChange {
        target: ChangeTarget::GovernanceRules,
        change_type: ChangeType::Modify,
        description: "Initial change".to_string(),
        old_value: None,
        new_value: "new".to_string(),
    });
    amendment.requirements.review_period_secs = 0;
    let amendment_id = amendment.id.to_hex();

    // Store and submit
    commons_mgr.store_amendment(amendment).await.unwrap();
    let amendment_bytes: [u8; 32] = hex::decode(&amendment_id).unwrap().try_into().unwrap();
    commons_mgr
        .submit_amendment(
            &icn_governance::AmendmentId::new(amendment_bytes),
            &proposer_did,
        )
        .await
        .unwrap();

    // Try to add change after submission - should fail
    let new_change = AmendmentChange {
        target: ChangeTarget::MembershipPolicy,
        change_type: ChangeType::Add,
        description: "Late change".to_string(),
        old_value: None,
        new_value: "too late".to_string(),
    };
    let result = commons_mgr
        .add_amendment_change(&amendment_id, new_change)
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Draft"));
}
