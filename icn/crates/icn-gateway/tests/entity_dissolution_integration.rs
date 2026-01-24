//! Entity Dissolution Workflow Integration Tests
#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App, HttpMessage};
use icn_entity::{CooperativeEntity, EntityId, Membership, MembershipRole};
use icn_gateway::api::entity;
use icn_gateway::entity_audit::{EntityAuditManager, EntityOperation};
use icn_gateway::entity_mgr::EntityManager;
use icn_gateway::TokenClaims;
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

/// Create a test application with entity routes configured
async fn create_test_app() -> (
    impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    Arc<EntityManager>,
    Arc<EntityAuditManager>,
) {
    let store = Arc::new(SledStore::temporary().unwrap());
    let entity_mgr = Arc::new(EntityManager::new());
    let audit_mgr = Arc::new(EntityAuditManager::new(store));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(entity_mgr.clone()))
            .app_data(web::Data::new(audit_mgr.clone()))
            .service(web::scope("/entities").configure(entity::configure)),
    )
    .await;

    (app, entity_mgr, audit_mgr)
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

    let (app, entity_mgr, audit_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create a test entity
    let mut entity = CooperativeEntity::cooperative("dissolution-test", "Test Coop").unwrap();
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
    let (app, entity_mgr, _audit_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create a test entity with Suspended status
    let mut entity = CooperativeEntity::cooperative("suspended-test", "Suspended Coop").unwrap();
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
    let (app, entity_mgr, audit_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create and set up entity
    let mut entity = CooperativeEntity::cooperative("cancel-test", "Cancel Coop").unwrap();
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
    let (app, entity_mgr, _audit_mgr) = create_test_app().await;
    let alice = test_identity();
    let bob = test_identity();

    // Create entity with alice as founder
    let entity = CooperativeEntity::cooperative("auth-test", "Auth Coop").unwrap();
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
    let (app, entity_mgr, _audit_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity
    let mut entity = CooperativeEntity::cooperative("complete-test", "Complete Coop").unwrap();
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
    let (app, entity_mgr, audit_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity
    let mut entity = CooperativeEntity::cooperative("complete-success", "Complete Coop").unwrap();
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
    let (app, entity_mgr, _audit_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity
    let mut entity = CooperativeEntity::cooperative("wait-test", "Wait Coop").unwrap();
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
    let (app, entity_mgr, _audit_mgr) = create_test_app().await;
    let alice = test_identity();

    // Create an active entity
    let mut entity = CooperativeEntity::cooperative("founder-test", "Founder Coop").unwrap();
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
