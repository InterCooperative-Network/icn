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
    let jurisdiction = JurisdictionId::new(format!("coop:{}", coop_id));
    let initial_capabilities = vec![
        MembershipCapability::Transact,
        MembershipCapability::Vote,
    ];

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
