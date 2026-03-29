//! Integration tests for Commons Evolution flow
#![allow(clippy::unwrap_used, clippy::expect_used)]
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
    let steward_keypair = KeyPair::generate().unwrap();
    let steward_did = steward_keypair.did().clone();

    // Create anchor and holder with steward vouch (Strong POP required for join)
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&did, Some(&steward_did))
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
    let steward_keypair = KeyPair::generate().unwrap();
    let steward_did = steward_keypair.did().clone();

    // Create first anchor with steward vouch (Strong POP for join_jurisdiction)
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&did, Some(&steward_did))
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

    // Join jurisdiction - should succeed (Strong POP)
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

/// Test that get_or_create_holder is idempotent
///
/// This is a key integration test for S2.1: SDIS → CommonsHolder Wiring
/// Verifies that:
/// 1. get_or_create_holder creates a holder if none exists
/// 2. Calling it again returns the same holder (idempotent)
/// 3. The holder_id is deterministically derived from anchor_id
#[actix_web::test]
async fn test_enrollment_creates_holder_atomically() {
    let commons_mgr = CommonsManager::new();

    // Generate test keypair and enroll
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    // Create anchor (simulating enrollment)
    let anchor = commons_mgr
        .create_anchor_from_enrollment(&did, None)
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());

    // First call: should create new holder
    let holder1 = commons_mgr
        .get_or_create_holder(&anchor_id, &did, None)
        .await
        .unwrap();
    assert!(holder1.is_active());
    let holder1_id = hex::encode(holder1.id());

    // Second call: should return same holder (idempotent)
    let holder2 = commons_mgr
        .get_or_create_holder(&anchor_id, &did, None)
        .await
        .unwrap();
    let holder2_id = hex::encode(holder2.id());

    // Holder IDs must match
    assert_eq!(
        holder1_id, holder2_id,
        "get_or_create_holder must be idempotent"
    );

    // Verify holder is also findable by DID
    let holder_by_did = commons_mgr.get_holder_by_did(&did).await.unwrap();
    assert!(holder_by_did.is_some());
    assert_eq!(hex::encode(holder_by_did.unwrap().id()), holder1_id);
}

/// Test that duplicate create_holder_from_anchor fails but get_or_create_holder succeeds
#[actix_web::test]
async fn test_get_or_create_vs_create_holder() {
    let commons_mgr = CommonsManager::new();

    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    let anchor = commons_mgr
        .create_anchor_from_enrollment(&did, None)
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());

    // First create succeeds
    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &did)
        .await
        .unwrap();
    let holder_id = hex::encode(holder.id());

    // Second create_holder_from_anchor FAILS (not idempotent)
    let result = commons_mgr
        .create_holder_from_anchor(&anchor_id, &did)
        .await;
    assert!(
        result.is_err(),
        "create_holder_from_anchor should reject duplicates"
    );

    // But get_or_create_holder SUCCEEDS and returns same holder
    let holder2 = commons_mgr
        .get_or_create_holder(&anchor_id, &did, None)
        .await
        .unwrap();
    assert_eq!(
        hex::encode(holder2.id()),
        holder_id,
        "get_or_create should return existing"
    );
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

// ============================================================================
// Layer 1 — Sled persistence proof tests (CommonsManager drop-and-reopen)
// ============================================================================

/// Prove that PersonhoodAnchor survives a CommonsManager drop-and-reopen boundary.
#[actix_web::test]
async fn test_commons_anchor_survives_sled_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("commons.sled");

    let keypair = icn_identity::KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    // Phase 1: write
    let anchor_id = {
        let mgr = CommonsManager::with_sled_path(&sled_path).expect("open sled");
        let anchor = mgr.create_anchor_from_enrollment(&did, None).await.unwrap();
        hex::encode(anchor.id())
        // mgr drops here — sled lock released
    };

    // Phase 2: reopen fresh instance
    let mgr2 = CommonsManager::with_sled_path(&sled_path).expect("reopen sled");
    let found = mgr2.get_anchor(&anchor_id).await.unwrap();
    assert!(
        found.is_some(),
        "PersonhoodAnchor must survive sled drop-and-reopen"
    );
    // Verify the DID-to-anchor index also survived
    let by_did = mgr2.get_anchor_by_did(&did).await.unwrap();
    assert!(
        by_did.is_some(),
        "DID index must survive sled drop-and-reopen"
    );
}

/// Prove that Charter survives a CommonsManager drop-and-reopen boundary.
#[actix_web::test]
async fn test_commons_charter_survives_sled_drop_and_reopen() {
    use icn_governance::{Charter, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType};

    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("commons.sled");

    // Phase 1: write
    let charter_id = {
        let mgr = CommonsManager::with_sled_path(&sled_path).expect("open sled");
        let charter = Charter::new(
            OrgType::Cooperative,
            "coop:persist-test".to_string(),
            "Persistence Test Coop".to_string(),
            GovernanceConfig::cooperative_default(),
            MembershipPolicy::default(),
            DisputePolicy::default(),
        );
        let id = charter.charter_id.to_hex();
        mgr.store_charter(charter).await.unwrap();
        id
        // mgr drops here
    };

    // Phase 2: reopen fresh instance
    let mgr2 = CommonsManager::with_sled_path(&sled_path).expect("reopen sled");
    let found = mgr2.get_charter(&charter_id).await.unwrap();
    assert!(found.is_some(), "Charter must survive sled drop-and-reopen");
    assert_eq!(
        found.unwrap().name,
        "Persistence Test Coop",
        "name must survive round-trip"
    );
}

/// Prove that CommonsHolderRecord survives a CommonsManager drop-and-reopen boundary.
#[actix_web::test]
async fn test_commons_holder_survives_sled_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("commons.sled");

    let keypair = icn_identity::KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    // Phase 1: write anchor + holder
    let holder_id = {
        let mgr = CommonsManager::with_sled_path(&sled_path).expect("open sled");
        let anchor = mgr.create_anchor_from_enrollment(&did, None).await.unwrap();
        let anchor_id = hex::encode(anchor.id());
        let holder = mgr
            .create_holder_from_anchor(&anchor_id, &did)
            .await
            .unwrap();
        hex::encode(holder.id())
        // mgr drops here
    };

    // Phase 2: reopen fresh instance
    let mgr2 = CommonsManager::with_sled_path(&sled_path).expect("reopen sled");
    let found = mgr2.get_holder(&holder_id).await.unwrap();
    assert!(
        found.is_some(),
        "CommonsHolderRecord must survive sled drop-and-reopen"
    );
    let found_holder = found.unwrap();
    assert_eq!(
        found_holder.holder_did, did,
        "holder_did must survive round-trip"
    );
}

// ============================================================================
// Layer 4 — Daemon restart simulation
//
// Proves that commons state written through the production daemon wiring
// (`CommonsManager::with_handle(CommonsHandle::with_sled_path(...))`)
// survives a full daemon shutdown + restart simulation: all manager and
// handle references drop, then a fresh daemon path reopens the same data dir.
//
// This is the highest-fidelity same-process proof. It exercises the exact
// code path that `icn-core/src/supervisor/lifecycle.rs` uses:
//   CommonsHandle::with_sled_path(data_dir/commons.sled) → CommonsManager::with_handle(handle)
//
// What it proves:
// - CommonsManager::with_handle() is a thin facade — writes go through to sled
// - Dropping both CommonsManager and CommonsHandle releases the sled lock
// - A fresh handle + manager pair recovers exactly the persisted state
//
// What it does NOT prove:
// - Cross-process durability (see Layer 3 in icn-commons/tests/commons_persistence.rs)
// ============================================================================

/// Layer 4 — Daemon restart simulation: anchor + holder survive full production
/// CommonsManager+CommonsHandle lifecycle boundary.
#[actix_web::test]
async fn test_l4_daemon_restart_simulation_anchor_and_holder() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path();
    let commons_path = data_dir.join("commons.sled");

    let keypair = KeyPair::generate().expect("KeyPair::generate");
    let did = keypair.did().clone();

    // ── Daemon startup + write ────────────────────────────────────────────────
    // Mirrors lifecycle.rs: open CommonsHandle, inject into CommonsManager.
    let (anchor_id, holder_id) = {
        let handle =
            icn_commons::CommonsHandle::with_sled_path(&commons_path).expect("open CommonsHandle");
        let mgr = CommonsManager::with_handle(handle);
        // Write anchor
        let anchor = mgr
            .create_anchor_from_enrollment(&did, None)
            .await
            .expect("create_anchor_from_enrollment");
        let anchor_id = hex::encode(anchor.id());
        // Write holder
        let holder = mgr
            .create_holder_from_anchor(&anchor_id, &did)
            .await
            .expect("create_holder_from_anchor");
        let holder_id = hex::encode(holder.id());
        // Flush before drop — simulates clean daemon shutdown
        mgr.flush().await.expect("flush");
        (anchor_id, holder_id)
        // mgr drops here, then handle drops — sled lock released.
        // This simulates the daemon exiting: all Arc refs go to zero.
    };

    // ── Daemon restart ────────────────────────────────────────────────────────
    // Fresh CommonsHandle + CommonsManager from the same data_dir.
    // No shared memory with the "previous daemon" — disk is the only bridge.
    let handle2 =
        icn_commons::CommonsHandle::with_sled_path(&commons_path).expect("reopen CommonsHandle");
    let mgr2 = CommonsManager::with_handle(handle2);

    // Assert 1: anchor present by ID after restart.
    let found_anchor = mgr2
        .get_anchor(&anchor_id)
        .await
        .expect("get_anchor after daemon restart");
    assert!(
        found_anchor.is_some(),
        "PersonhoodAnchor must survive daemon restart simulation (Layer 4)"
    );

    // Assert 2: DID reverse-index survives restart.
    let by_did = mgr2
        .get_anchor_by_did(&did)
        .await
        .expect("get_anchor_by_did after daemon restart");
    assert!(
        by_did.is_some(),
        "DID reverse-index must survive daemon restart simulation (Layer 4)"
    );

    // Assert 3: holder present by ID after restart.
    let found_holder = mgr2
        .get_holder(&holder_id)
        .await
        .expect("get_holder after daemon restart");
    assert!(
        found_holder.is_some(),
        "CommonsHolderRecord must survive daemon restart simulation (Layer 4)"
    );
    assert_eq!(
        found_holder.unwrap().holder_did,
        did,
        "holder_did must survive daemon restart round-trip (Layer 4)"
    );
}
