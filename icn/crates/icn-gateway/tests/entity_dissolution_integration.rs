//! Entity Dissolution Workflow Integration Tests
#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App, HttpMessage};
use icn_entity::{CooperativeEntity, EntityId, Membership, MembershipRole};
use icn_gateway::api::entity;
use icn_gateway::entity_audit::{EntityAuditManager, EntityOperation};
use icn_gateway::entity_mgr::EntityManager;
use icn_gateway::governance_mgr::GovernanceManager;
use icn_gateway::TokenClaims;
use icn_governance::{GovernanceDomainId, Proposal, ProposalId, ProposalPayload, ProposalState};
use icn_identity::IdentityBundle;
use icn_store::SledStore;
use serde_json::json;
use std::sync::{Arc, Once};

static INIT: Once = Once::new();

fn init_logging() {
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("debug")
            .with_test_writer()
            .try_init();
    });
}

/// Create test claims with specified scopes (bypasses actual JWT auth)
fn create_test_claims(did: &str, scopes: Vec<&str>) -> TokenClaims {
    TokenClaims {
        sub: did.to_string(),
        iat: 1000000000,
        coop_id: "test-coop".to_string(),
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
        exp: 9999999999,
    }
}

/// Test proposal ID used for dissolution tests
const TEST_PROPOSAL_ID: &str = "proposal-123";

/// Create a test proposal with Accepted state
fn create_test_proposal(proposal_id: &str) -> Proposal {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Generate a test identity for the proposer
    let test_proposer = IdentityBundle::generate().unwrap();

    Proposal {
        id: ProposalId::new(proposal_id),
        domain_id: GovernanceDomainId("test-domain".to_string()),
        proposer: test_proposer.did().clone(),
        title: "Dissolution Proposal".to_string(),
        description: "Test dissolution proposal".to_string(),
        payload: ProposalPayload::Text {
            body: "Entity dissolution proposal for testing".to_string(),
        },
        state: ProposalState::Accepted { closed_at: now },
        created_at: now - 3600,
        updated_at: now,
    }
}

/// Create a test application with entity routes configured
async fn create_test_app() -> (
    impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    Arc<EntityManager>,
    Arc<EntityAuditManager>,
    Arc<GovernanceManager>,
) {
    let store = Arc::new(SledStore::temporary().unwrap());
    let entity_mgr = Arc::new(EntityManager::new());
    let audit_mgr = Arc::new(EntityAuditManager::new(store));
    let governance_mgr = Arc::new(GovernanceManager::new());

    // Pre-populate governance with accepted proposals for dissolution tests
    // Each test may use a different proposal ID
    governance_mgr.insert_test_proposal(create_test_proposal(TEST_PROPOSAL_ID)); // proposal-123
    governance_mgr.insert_test_proposal(create_test_proposal("proposal-202"));
    governance_mgr.insert_test_proposal(create_test_proposal("proposal-303"));
    governance_mgr.insert_test_proposal(create_test_proposal("proposal-456"));
    governance_mgr.insert_test_proposal(create_test_proposal("proposal-complete"));
    governance_mgr.insert_test_proposal(create_test_proposal("proposal-success"));
    governance_mgr.insert_test_proposal(create_test_proposal("proposal-wait"));
    governance_mgr.insert_test_proposal(create_test_proposal("proposal-founder"));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(entity_mgr.clone()))
            .app_data(web::Data::new(audit_mgr.clone()))
            .app_data(web::Data::new(governance_mgr.clone()))
            .service(web::scope("/entities").configure(entity::configure)),
    )
    .await;

    (app, entity_mgr, audit_mgr, governance_mgr)
}

/// Generate a test identity bundle
fn test_identity() -> IdentityBundle {
    IdentityBundle::generate().unwrap()
}

// ============================================================================
// Entity Dissolution Tests
// ============================================================================

#[actix_web::test]
async fn test_initiate_dissolution_success() {
    init_logging();

    let (app, entity_mgr, audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create a test entity with governance domain matching the test proposals
    let mut entity = CooperativeEntity::cooperative("dissolution-test", "Test Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active; // Set to Active so it can be dissolved
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    // Register alice as an individual entity first
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Initiate dissolution
    let req_body = json!({
        "proposal_id": "proposal-123",
        "reason": "Test dissolution",
        "waiting_period_seconds": 10  // Short period for testing
    });

    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&req_body)
        .to_request();
    req.extensions_mut().insert(claims);

    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body_bytes = test::read_body(resp).await;
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert!(
        status.is_success(),
        "Expected success, got {status:?} with body: {body_str}"
    );

    let response: serde_json::Value = serde_json::from_str(&body_str).unwrap();
    assert_eq!(response["status"], "dissolving");
    assert_eq!(response["proposal_id"], "proposal-123");

    // Verify entity status changed
    let entity = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert!(matches!(
        entity.status,
        icn_entity::EntityStatus::Dissolving { .. }
    ));

    // Verify audit record
    let audit_trail = audit_mgr.get_audit_trail(&entity_id, 10, 0).unwrap();
    let dissolution_record = audit_trail
        .records
        .iter()
        .find(|r| matches!(r.operation, EntityOperation::DissolutionInitiated { .. }));
    assert!(dissolution_record.is_some());
}

#[actix_web::test]
async fn test_initiate_dissolution_requires_active_status() {
    let (app, entity_mgr, _audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create a test entity with Suspended status
    let mut entity = CooperativeEntity::cooperative("suspended-test", "Suspended Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Suspended {
        reason: "Test suspension".to_string(),
        suspended_at: 1000,
    };
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    // Register alice as an individual entity first
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Try to initiate dissolution on suspended entity
    let req_body = json!({
        "proposal_id": "proposal-456",
        "reason": "Test dissolution"
    });

    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&req_body)
        .to_request();
    req.extensions_mut().insert(claims);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let body_bytes = test::read_body(resp).await;
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(body_str.contains("Cannot dissolve entity with status"));
}

#[actix_web::test]
async fn test_cancel_dissolution_success() {
    let (app, entity_mgr, audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create and set up entity with governance domain matching test proposals
    let mut entity = CooperativeEntity::cooperative("cancel-test", "Cancel Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active; // Set to Active so it can be dissolved
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    let alice_id = EntityId::from_did(alice.did());
    // Register alice as an individual entity first
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Initiate dissolution
    let req_body = json!({
        "proposal_id": "proposal-202",
        "reason": "Test cancellation"
    });

    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&req_body)
        .to_request();
    req.extensions_mut().insert(claims.clone());

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // Verify entity is Dissolving
    let entity = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert!(matches!(
        entity.status,
        icn_entity::EntityStatus::Dissolving { .. }
    ));

    // Cancel dissolution
    let req = test::TestRequest::delete()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .to_request();
    req.extensions_mut().insert(claims);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // Verify entity is back to Active
    let entity = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert!(matches!(entity.status, icn_entity::EntityStatus::Active));

    // Verify audit record
    let audit_trail = audit_mgr.get_audit_trail(&entity_id, 10, 0).unwrap();
    let cancel_record = audit_trail
        .records
        .iter()
        .find(|r| matches!(r.operation, EntityOperation::DissolutionCancelled { .. }));
    assert!(cancel_record.is_some());
}

#[actix_web::test]
async fn test_dissolution_requires_authorization() {
    let (app, entity_mgr, _audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();
    let bob = test_identity();

    // Create entity with alice as founder and governance domain
    let entity = CooperativeEntity::cooperative("auth-test", "Auth Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    let alice_id = EntityId::from_did(alice.did());
    // Register alice as an individual entity first
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Bob (not a founder) tries to initiate dissolution
    let req_body = json!({
        "proposal_id": "proposal-303",
        "reason": "Unauthorized attempt"
    });

    let claims = create_test_claims(&bob.did().to_string(), vec!["entity:write"]);
    let req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&req_body)
        .to_request();
    req.extensions_mut().insert(claims);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_complete_dissolution_requires_no_members() {
    let (app, entity_mgr, _audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity with governance domain
    let mut entity = CooperativeEntity::cooperative("complete-test", "Complete Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Initiate dissolution with very short waiting period (1 second)
    let init_body = json!({
        "proposal_id": "proposal-complete",
        "reason": "Test dissolution",
        "waiting_period_seconds": 1
    });

    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let init_req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&init_body)
        .to_request();
    init_req.extensions_mut().insert(claims.clone());

    let init_resp = test::call_service(&app, init_req).await;
    assert!(
        init_resp.status().is_success(),
        "Expected success, got {:?}",
        init_resp.status()
    );

    // Wait for the waiting period
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Try to complete without removing members - should fail
    let complete_req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution/complete", entity_id))
        .to_request();
    complete_req.extensions_mut().insert(claims.clone());

    let complete_resp = test::call_service(&app, complete_req).await;
    assert_eq!(complete_resp.status(), 400);

    let body_bytes = test::read_body(complete_resp).await;
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(body_str.contains("still has") || body_str.contains("members"));
}

#[actix_web::test]
async fn test_complete_dissolution_success() {
    let (app, entity_mgr, audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity with governance domain
    let mut entity = CooperativeEntity::cooperative("complete-success", "Complete Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Initiate dissolution with very short waiting period
    let init_body = json!({
        "proposal_id": "proposal-success",
        "reason": "Test dissolution",
        "waiting_period_seconds": 1
    });

    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let init_req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&init_body)
        .to_request();
    init_req.extensions_mut().insert(claims.clone());

    let init_resp = test::call_service(&app, init_req).await;
    assert!(
        init_resp.status().is_success(),
        "Expected success, got {:?}",
        init_resp.status()
    );

    // Remove all members (dissolution allows removing last founder)
    entity_mgr
        .remove_membership(&entity_id, &alice_id)
        .await
        .unwrap();

    // Wait for the waiting period
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Complete dissolution
    let complete_req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution/complete", entity_id))
        .to_request();
    complete_req.extensions_mut().insert(claims.clone());

    let complete_resp = test::call_service(&app, complete_req).await;
    assert_eq!(complete_resp.status(), 204);

    // Verify entity is deleted
    let entity = entity_mgr.get(&entity_id).await.unwrap();
    assert!(entity.is_none());

    // Verify audit record exists
    let audit_trail = audit_mgr.get_audit_trail(&entity_id, 10, 0).unwrap();
    let completed_record = audit_trail
        .records
        .iter()
        .find(|r| matches!(r.operation, EntityOperation::DissolutionCompleted { .. }));
    assert!(completed_record.is_some());
}

#[actix_web::test]
async fn test_complete_dissolution_requires_waiting_period() {
    let (app, entity_mgr, _audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity with governance domain
    let mut entity = CooperativeEntity::cooperative("wait-test", "Wait Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Initiate dissolution with long waiting period (1 day)
    let init_body = json!({
        "proposal_id": "proposal-wait",
        "reason": "Test dissolution",
        "waiting_period_seconds": 86400
    });

    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let init_req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&init_body)
        .to_request();
    init_req.extensions_mut().insert(claims.clone());

    let init_resp = test::call_service(&app, init_req).await;
    assert!(
        init_resp.status().is_success(),
        "Expected success, got {:?}",
        init_resp.status()
    );

    // Try to complete immediately - should fail
    let complete_req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution/complete", entity_id))
        .to_request();
    complete_req.extensions_mut().insert(claims.clone());

    let complete_resp = test::call_service(&app, complete_req).await;
    assert_eq!(complete_resp.status(), 400);

    let body_bytes = test::read_body(complete_resp).await;
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(body_str.contains("Waiting period"));
}

#[actix_web::test]
async fn test_dissolving_entity_allows_removing_last_founder() {
    let (app, entity_mgr, _audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity with governance domain
    let mut entity = CooperativeEntity::cooperative("founder-test", "Founder Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as the only founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // First verify we can't remove last founder from active entity
    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let remove_req = test::TestRequest::delete()
        .uri(&format!("/entities/{}/members/{}", entity_id, alice_id))
        .to_request();
    remove_req.extensions_mut().insert(claims.clone());

    let remove_resp = test::call_service(&app, remove_req).await;
    assert_eq!(remove_resp.status(), 400); // Should fail - last founder

    let body_bytes = test::read_body(remove_resp).await;
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(body_str.contains("Cannot remove the last founder"));

    // Initiate dissolution
    let init_body = json!({
        "proposal_id": "proposal-founder",
        "reason": "Test dissolution"
    });

    let init_req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&init_body)
        .to_request();
    init_req.extensions_mut().insert(claims.clone());

    let init_resp = test::call_service(&app, init_req).await;
    assert!(
        init_resp.status().is_success(),
        "Expected success, got {:?}",
        init_resp.status()
    );

    // Now we should be able to remove the last founder since entity is dissolving
    let remove_req2 = test::TestRequest::delete()
        .uri(&format!("/entities/{}/members/{}", entity_id, alice_id))
        .to_request();
    remove_req2.extensions_mut().insert(claims.clone());

    let remove_resp2 = test::call_service(&app, remove_req2).await;
    assert_eq!(remove_resp2.status(), 204); // Should succeed - entity is dissolving (204 No Content)
}

#[actix_web::test]
async fn test_dissolving_entity_allows_removing_regular_members() {
    let (app, entity_mgr, _audit_mgr, governance_mgr) = create_test_app().await;
    let alice = test_identity();
    let bob = test_identity();

    // Create an active entity with governance domain
    let mut entity = CooperativeEntity::cooperative("member-test", "Member Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let alice_membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(alice_membership).await.unwrap();

    // Add bob as regular member (not founder)
    let bob_id = EntityId::from_did(bob.did());
    let bob_entity = CooperativeEntity::individual(bob.did(), bob_id.to_string());
    entity_mgr.register(bob_entity).await.unwrap();
    let bob_membership =
        Membership::active(bob_id.clone(), entity_id.clone(), MembershipRole::Member);
    entity_mgr.add_membership(bob_membership).await.unwrap();

    // Pre-populate a proposal for this test
    governance_mgr.insert_test_proposal(create_test_proposal("proposal-member"));

    // Initiate dissolution
    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let init_body = json!({
        "proposal_id": "proposal-member",
        "reason": "Test dissolution"
    });

    let init_req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&init_body)
        .to_request();
    init_req.extensions_mut().insert(claims.clone());

    let init_resp = test::call_service(&app, init_req).await;
    assert!(
        init_resp.status().is_success(),
        "Expected success, got {:?}",
        init_resp.status()
    );

    // Remove regular member bob during dissolution - should succeed
    let remove_req = test::TestRequest::delete()
        .uri(&format!("/entities/{}/members/{}", entity_id, bob_id))
        .to_request();
    remove_req.extensions_mut().insert(claims.clone());

    let remove_resp = test::call_service(&app, remove_req).await;
    assert_eq!(
        remove_resp.status(),
        204,
        "Should succeed removing regular member during dissolution"
    );

    // Verify bob was removed
    let members = entity_mgr.get_members(&entity_id).await.unwrap();
    assert_eq!(members.len(), 1, "Should have only alice left");
    assert!(members.iter().any(|m| m.member_id == alice_id));
}

#[actix_web::test]
async fn test_dissolving_entity_allows_removing_multiple_founders() {
    let (app, entity_mgr, _audit_mgr, governance_mgr) = create_test_app().await;
    let alice = test_identity();
    let bob = test_identity();
    let carol = test_identity();

    // Create an active entity with governance domain
    let mut entity = CooperativeEntity::cooperative("multi-founder-test", "Multi Founder Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice, bob, and carol as founders
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let alice_membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(alice_membership).await.unwrap();

    let bob_id = EntityId::from_did(bob.did());
    let bob_entity = CooperativeEntity::individual(bob.did(), bob_id.to_string());
    entity_mgr.register(bob_entity).await.unwrap();
    let bob_membership =
        Membership::active(bob_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(bob_membership).await.unwrap();

    let carol_id = EntityId::from_did(carol.did());
    let carol_entity = CooperativeEntity::individual(carol.did(), carol_id.to_string());
    entity_mgr.register(carol_entity).await.unwrap();
    let carol_membership =
        Membership::active(carol_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(carol_membership).await.unwrap();

    // Pre-populate a proposal for this test
    governance_mgr.insert_test_proposal(create_test_proposal("proposal-multi"));

    // Initiate dissolution
    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let init_body = json!({
        "proposal_id": "proposal-multi",
        "reason": "Test dissolution"
    });

    let init_req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&init_body)
        .to_request();
    init_req.extensions_mut().insert(claims.clone());

    let init_resp = test::call_service(&app, init_req).await;
    assert!(
        init_resp.status().is_success(),
        "Expected success, got {:?}",
        init_resp.status()
    );

    // Remove bob (first founder removal during dissolution) - should succeed
    let remove_req1 = test::TestRequest::delete()
        .uri(&format!("/entities/{}/members/{}", entity_id, bob_id))
        .to_request();
    remove_req1.extensions_mut().insert(claims.clone());

    let remove_resp1 = test::call_service(&app, remove_req1).await;
    assert_eq!(
        remove_resp1.status(),
        204,
        "Should succeed removing first founder during dissolution"
    );

    // Remove carol (second founder removal, leaving only alice) - should succeed
    let remove_req2 = test::TestRequest::delete()
        .uri(&format!("/entities/{}/members/{}", entity_id, carol_id))
        .to_request();
    remove_req2.extensions_mut().insert(claims.clone());

    let remove_resp2 = test::call_service(&app, remove_req2).await;
    assert_eq!(
        remove_resp2.status(),
        204,
        "Should succeed removing second founder during dissolution"
    );

    // Remove alice (last founder) - should succeed because entity is dissolving
    let remove_req3 = test::TestRequest::delete()
        .uri(&format!("/entities/{}/members/{}", entity_id, alice_id))
        .to_request();
    remove_req3.extensions_mut().insert(claims.clone());

    let remove_resp3 = test::call_service(&app, remove_req3).await;
    assert_eq!(
        remove_resp3.status(),
        204,
        "Should succeed removing last founder during dissolution"
    );

    // Verify all members removed
    let members = entity_mgr.get_members(&entity_id).await.unwrap();
    assert_eq!(members.len(), 0, "Should have no members left");
}

#[actix_web::test]
async fn test_dissolution_rejects_zero_waiting_period() {
    let (app, entity_mgr, _audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity with governance domain
    let mut entity = CooperativeEntity::cooperative("zero-wait-test", "Zero Wait Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Pre-populate a proposal for this test
    _governance_mgr.insert_test_proposal(create_test_proposal("proposal-zero-wait"));

    // Try to initiate dissolution with zero waiting period - should fail
    let req_body = json!({
        "proposal_id": "proposal-zero-wait",
        "reason": "Test zero waiting period",
        "waiting_period_seconds": 0
    });

    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&req_body)
        .to_request();
    req.extensions_mut().insert(claims);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let body_bytes = test::read_body(resp).await;
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(
        body_str.contains("minimum") || body_str.contains("below"),
        "Error should mention minimum waiting period, got: {body_str}"
    );

    // Verify entity is still Active (not Dissolving)
    let entity = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert!(matches!(entity.status, icn_entity::EntityStatus::Active));
}

#[actix_web::test]
async fn test_dissolution_rejects_concurrent_initiation() {
    let (app, entity_mgr, _audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity with governance domain
    let mut entity = CooperativeEntity::cooperative("concurrent-test", "Concurrent Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Pre-populate proposals for this test
    _governance_mgr.insert_test_proposal(create_test_proposal("proposal-concurrent-1"));
    _governance_mgr.insert_test_proposal(create_test_proposal("proposal-concurrent-2"));

    // First initiation should succeed
    let req_body1 = json!({
        "proposal_id": "proposal-concurrent-1",
        "reason": "First dissolution attempt",
        "waiting_period_seconds": 3600
    });

    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let req1 = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&req_body1)
        .to_request();
    req1.extensions_mut().insert(claims.clone());

    let resp1 = test::call_service(&app, req1).await;
    assert!(
        resp1.status().is_success(),
        "First dissolution should succeed"
    );

    // Verify entity is now Dissolving
    let entity = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert!(matches!(
        entity.status,
        icn_entity::EntityStatus::Dissolving { .. }
    ));

    // Second initiation should fail - entity already dissolving
    let req_body2 = json!({
        "proposal_id": "proposal-concurrent-2",
        "reason": "Second dissolution attempt",
        "waiting_period_seconds": 3600
    });

    let req2 = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&req_body2)
        .to_request();
    req2.extensions_mut().insert(claims.clone());

    let resp2 = test::call_service(&app, req2).await;
    assert_eq!(resp2.status(), 400);

    let body_bytes = test::read_body(resp2).await;
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(
        body_str.contains("Cannot dissolve") || body_str.contains("status"),
        "Error should mention invalid status, got: {body_str}"
    );
}

#[actix_web::test]
async fn test_concurrent_dissolution_initiation_detected() {
    init_logging();

    // This test verifies that optimistic locking prevents race conditions
    // by simulating a scenario where the entity is modified between read and write

    let entity_mgr = Arc::new(EntityManager::new());
    let alice = test_identity();

    // Create and register entity
    let mut entity = CooperativeEntity::cooperative("concurrent-test", "Concurrent Test")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity.clone()).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership = Membership::active(alice_id, entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Read entity and capture version (simulating first request's read)
    let entity_v0 = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert_eq!(entity_v0.version, 0, "Initial version should be 0");

    // Simulate concurrent modification: another process updates the entity using
    // optimistic locking. This increments the version from 0 to 1.
    // (Regular update() does not increment version - only update_if_version does)
    let mut entity_modified = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    let expected_version = entity_modified.version;
    entity_modified.description = Some("Modified by concurrent process".to_string());
    entity_mgr
        .update_if_version(entity_modified, expected_version)
        .await
        .unwrap();

    // Verify version was incremented
    let entity_v1 = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert_eq!(entity_v1.version, 1, "Version should be 1 after update");

    // Now try to update with stale version (0) - should fail
    let mut entity_stale = entity_v0.clone(); // This has version 0
    entity_stale.status = icn_entity::EntityStatus::Dissolving {
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    // Attempt to update with stale version
    let result = entity_mgr.update_if_version(entity_stale, 0).await;

    // Should fail due to version mismatch
    assert!(result.is_err(), "Update with stale version should fail");

    let error_str = result.unwrap_err().to_string();
    assert!(
        error_str.contains("Concurrent modification")
            || error_str.contains("ConcurrentModification"),
        "Error should indicate concurrent modification, got: {error_str}"
    );

    // Verify entity is still Active (status update did not proceed)
    let final_entity = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert!(
        matches!(final_entity.status, icn_entity::EntityStatus::Active),
        "Entity should still be Active after failed update"
    );
    assert_eq!(final_entity.version, 1, "Version should still be 1");
}

#[actix_web::test]
async fn test_dissolution_initiation_succeeds_with_correct_version() {
    init_logging();

    let (app, entity_mgr, audit_mgr, _governance_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create a test entity
    let mut entity = CooperativeEntity::cooperative("version-test", "Version Test Coop")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership =
        Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Verify initial version is 0
    let entity_before = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert_eq!(entity_before.version, 0, "Initial version should be 0");

    // Initiate dissolution (no concurrent modifications)
    let req_body = json!({
        "proposal_id": "proposal-123",
        "reason": "Test version-tracked dissolution",
        "waiting_period_seconds": 10
    });

    let claims = create_test_claims(&alice.did().to_string(), vec!["entity:write"]);
    let req = test::TestRequest::post()
        .uri(&format!("/entities/{}/dissolution", entity_id))
        .set_json(&req_body)
        .to_request();
    req.extensions_mut().insert(claims);

    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body_bytes = test::read_body(resp).await;
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert!(
        status.is_success(),
        "Expected success, got {status:?} with body: {body_str}"
    );

    // Verify entity status changed to Dissolving
    let entity_after = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert!(
        matches!(
            entity_after.status,
            icn_entity::EntityStatus::Dissolving { .. }
        ),
        "Entity should be in Dissolving status"
    );

    // Verify version was incremented
    assert_eq!(
        entity_after.version, 1,
        "Version should be incremented to 1 after update"
    );

    // Verify audit record was created
    let audit_trail = audit_mgr.get_audit_trail(&entity_id, 10, 0).unwrap();
    let dissolution_record = audit_trail
        .records
        .iter()
        .find(|r| matches!(r.operation, EntityOperation::DissolutionInitiated { .. }));
    assert!(
        dissolution_record.is_some(),
        "Audit trail should contain dissolution initiation record"
    );
}

#[actix_web::test]
async fn test_optimistic_locking_prevents_double_initiation() {
    init_logging();

    // This test simulates two concurrent requests attempting to initiate dissolution
    // The second request should be rejected due to version mismatch

    let entity_mgr = Arc::new(EntityManager::new());
    let alice = test_identity();

    // Create and register entity
    let mut entity = CooperativeEntity::cooperative("double-init-test", "Double Init Test")
        .unwrap()
        .with_governance_domain("test-domain");
    entity.status = icn_entity::EntityStatus::Active;
    let entity_id = entity.id.clone();
    entity_mgr.register(entity.clone()).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    let alice_entity = CooperativeEntity::individual(alice.did(), alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership = Membership::active(alice_id, entity_id.clone(), MembershipRole::Founder);
    entity_mgr.add_membership(membership).await.unwrap();

    // Get entity and capture version (simulating first request)
    let mut entity1 = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    let version1 = entity1.version;

    // Get entity again (simulating second concurrent request)
    let mut entity2 = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    let version2 = entity2.version;

    assert_eq!(
        version1, version2,
        "Both requests should see same initial version"
    );

    // First request updates status
    entity1.status = icn_entity::EntityStatus::Dissolving {
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    // First update succeeds
    let result1 = entity_mgr.update_if_version(entity1, version1).await;
    assert!(result1.is_ok(), "First update should succeed");

    // Second request tries to update with stale version
    entity2.status = icn_entity::EntityStatus::Dissolving {
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    // Second update should fail due to version mismatch
    let result2 = entity_mgr.update_if_version(entity2, version2).await;
    assert!(result2.is_err(), "Second update should fail");

    // Verify error is about concurrent modification
    let error_str = result2.unwrap_err().to_string();
    assert!(
        error_str.contains("Concurrent modification")
            || error_str.contains("ConcurrentModification"),
        "Error should indicate concurrent modification, got: {error_str}"
    );

    // Verify final entity state
    let final_entity = entity_mgr.get(&entity_id).await.unwrap().unwrap();
    assert_eq!(
        final_entity.version, 1,
        "Version should be 1 after single successful update"
    );
    assert!(
        matches!(
            final_entity.status,
            icn_entity::EntityStatus::Dissolving { .. }
        ),
        "Entity should be in Dissolving status"
    );
}
