//! Integration tests for Commons Evolution flow
//!
//! Tests the complete enrollment → PersonhoodAnchor → CommonsHolderRecord flow.

use icn_gateway::commons_mgr::CommonsManager;
use icn_identity::KeyPair;

/// Test that CommonsManager creates anchors and holders correctly
#[actix_web::test]
async fn test_create_anchor_and_holder() {
    let commons_mgr = CommonsManager::new();

    // Generate test keypair
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    // Create anchor without steward (genesis enrollment)
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&did, None)
        .await
        .unwrap();

    assert!(anchor.is_active());
    assert_eq!(anchor.pop_attestations.len(), 1);

    // Get anchor by DID
    let found = commons_mgr.get_anchor_by_did(&did).await.unwrap();
    assert!(found.is_some());
    let found_anchor = found.unwrap();
    assert_eq!(found_anchor.id(), anchor.id());

    // Create holder from anchor
    let anchor_id = hex::encode(anchor.id());
    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &did)
        .await
        .unwrap();

    assert!(holder.is_active());
    assert!(holder.affiliations.is_empty());

    // Get holder by DID
    let found = commons_mgr.get_holder_by_did(&did).await.unwrap();
    assert!(found.is_some());
    let found_holder = found.unwrap();
    assert_eq!(found_holder.id(), holder.id());
}

/// Test anchor creation with steward vouch
#[actix_web::test]
async fn test_create_anchor_with_steward() {
    let commons_mgr = CommonsManager::new();

    let member_keypair = KeyPair::generate().unwrap();
    let steward_keypair = KeyPair::generate().unwrap();
    let member_did = member_keypair.did().clone();
    let steward_did = steward_keypair.did().clone();

    // Create anchor with steward vouch
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&member_did, Some(&steward_did))
        .await
        .unwrap();

    assert!(anchor.is_active());
    // Should have Strong POP level from steward vouch
    assert_eq!(anchor.pop_level(), Some(icn_identity::POPLevel::Strong));
}

/// Test joining and leaving jurisdictions
#[actix_web::test]
async fn test_jurisdiction_membership() {
    use icn_identity::{JurisdictionId, MembershipStatus};

    let commons_mgr = CommonsManager::new();

    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    // Create anchor and holder
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&did, None)
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());
    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &did)
        .await
        .unwrap();
    let holder_id = hex::encode(holder.id());

    // Join a jurisdiction
    let jurisdiction = JurisdictionId::new("coop:test-coop");
    let affiliation = commons_mgr
        .join_jurisdiction(&holder_id, jurisdiction.clone(), vec![])
        .await
        .unwrap();

    assert_eq!(affiliation.membership_status, MembershipStatus::Candidate);

    // Verify affiliation exists
    let affiliations = commons_mgr.list_affiliations(&holder_id).await.unwrap();
    assert_eq!(affiliations.len(), 1);

    // Update status to Member
    commons_mgr
        .update_affiliation_status(&holder_id, &jurisdiction, MembershipStatus::Member)
        .await
        .unwrap();

    let affiliations = commons_mgr.list_affiliations(&holder_id).await.unwrap();
    assert_eq!(affiliations[0].membership_status, MembershipStatus::Member);

    // Leave jurisdiction
    commons_mgr
        .leave_jurisdiction(&holder_id, &jurisdiction)
        .await
        .unwrap();

    let affiliations = commons_mgr.list_affiliations(&holder_id).await.unwrap();
    assert_eq!(affiliations[0].membership_status, MembershipStatus::Exited);
}

/// Test charter creation and management
#[actix_web::test]
async fn test_charter_lifecycle() {
    use icn_governance::{
        Charter, CharterStatus, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType,
    };

    let commons_mgr = CommonsManager::new();

    // Create a cooperative charter
    let charter = Charter::new(
        OrgType::Cooperative,
        "coop:test-coop".to_string(),
        "Test Cooperative".to_string(),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );

    let charter_id = charter.charter_id.to_hex();
    let domain_id = charter.domain_id.clone();

    // Store charter
    commons_mgr.store_charter(charter).await.unwrap();

    // Get by ID
    let found = commons_mgr.get_charter(&charter_id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "Test Cooperative");

    // Get by domain
    let found = commons_mgr.get_charter_by_domain(&domain_id).await.unwrap();
    assert!(found.is_some());

    // List all charters
    let all = commons_mgr.list_charters(None, None).await.unwrap();
    assert_eq!(all.len(), 1);

    // List by org type
    let coops = commons_mgr
        .list_charters(Some(OrgType::Cooperative), None)
        .await
        .unwrap();
    assert_eq!(coops.len(), 1);

    // Update status
    commons_mgr
        .update_charter_status(&charter_id, CharterStatus::Active)
        .await
        .unwrap();

    let updated = commons_mgr.get_charter(&charter_id).await.unwrap().unwrap();
    assert!(matches!(updated.status, CharterStatus::Active));
}

/// Test multiple holders with different POP levels
#[actix_web::test]
async fn test_multiple_holders_different_pop_levels() {
    use icn_identity::POPLevel;

    let commons_mgr = CommonsManager::new();
    let steward_keypair = KeyPair::generate().unwrap();
    let steward_did = steward_keypair.did().clone();

    // Create genesis anchor (Weak POP)
    let genesis_keypair = KeyPair::generate().unwrap();
    let genesis_did = genesis_keypair.did().clone();
    let genesis_anchor = commons_mgr
        .create_anchor_from_enrollment(&genesis_did, None)
        .await
        .unwrap();
    assert_eq!(genesis_anchor.pop_level(), Some(POPLevel::Weak));

    // Create sponsored anchor (Strong POP)
    let sponsored_keypair = KeyPair::generate().unwrap();
    let sponsored_did = sponsored_keypair.did().clone();
    let sponsored_anchor = commons_mgr
        .create_anchor_from_enrollment(&sponsored_did, Some(&steward_did))
        .await
        .unwrap();
    assert_eq!(sponsored_anchor.pop_level(), Some(POPLevel::Strong));

    // Create holders and verify they inherit the POP level
    let genesis_anchor_id = hex::encode(genesis_anchor.id());
    let genesis_holder = commons_mgr
        .create_holder_from_anchor(&genesis_anchor_id, &genesis_did)
        .await
        .unwrap();
    assert_eq!(genesis_holder.personhood_level, POPLevel::Weak);

    let sponsored_anchor_id = hex::encode(sponsored_anchor.id());
    let sponsored_holder = commons_mgr
        .create_holder_from_anchor(&sponsored_anchor_id, &sponsored_did)
        .await
        .unwrap();
    assert_eq!(sponsored_holder.personhood_level, POPLevel::Strong);
}

/// Test duplicate prevention
#[actix_web::test]
async fn test_duplicate_prevention() {
    use icn_identity::JurisdictionId;

    let commons_mgr = CommonsManager::new();

    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    // Create first anchor - should succeed
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&did, None)
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());

    // Create holder - should succeed
    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &did)
        .await
        .unwrap();
    let holder_id = hex::encode(holder.id());

    // Try to create duplicate holder - should fail
    let result = commons_mgr
        .create_holder_from_anchor(&anchor_id, &did)
        .await;
    assert!(result.is_err());

    // Join jurisdiction - should succeed
    let jurisdiction = JurisdictionId::new("coop:test");
    commons_mgr
        .join_jurisdiction(&holder_id, jurisdiction.clone(), vec![])
        .await
        .unwrap();

    // Try to join same jurisdiction again - should fail
    let result = commons_mgr
        .join_jurisdiction(&holder_id, jurisdiction, vec![])
        .await;
    assert!(result.is_err());
}

/// Test anchor status updates
#[actix_web::test]
async fn test_anchor_status_updates() {
    use icn_identity::AnchorStatus;

    let commons_mgr = CommonsManager::new();

    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();
    let authority_did = KeyPair::generate().unwrap().did().clone();

    let anchor = commons_mgr
        .create_anchor_from_enrollment(&did, None)
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());

    // Initially active
    let found = commons_mgr.get_anchor(&anchor_id).await.unwrap().unwrap();
    assert!(matches!(found.status, AnchorStatus::Active));

    // Suspend
    commons_mgr
        .update_anchor_status(
            &anchor_id,
            AnchorStatus::Suspended {
                reason: "Test suspension".to_string(),
                suspended_at: 12345,
                until: None,
                authority: authority_did.clone(),
            },
        )
        .await
        .unwrap();

    let found = commons_mgr.get_anchor(&anchor_id).await.unwrap().unwrap();
    assert!(matches!(found.status, AnchorStatus::Suspended { .. }));

    // Reactivate
    commons_mgr
        .update_anchor_status(&anchor_id, AnchorStatus::Active)
        .await
        .unwrap();

    let found = commons_mgr.get_anchor(&anchor_id).await.unwrap().unwrap();
    assert!(matches!(found.status, AnchorStatus::Active));
}
