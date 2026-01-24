//! Entity Dissolution Workflow Integration Tests
#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App, HttpMessage};
use icn_entity::{CooperativeEntity, EntityId, Membership, MembershipRole};
use icn_gateway::api::entity;
use icn_gateway::entity_audit::{EntityAuditManager, EntityOperation};
use icn_gateway::entity_mgr::EntityManager;
use icn_gateway::TokenClaims;
use icn_identity::{Did, IdentityBundle};
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
    entity.status = icn_entity::EntityStatus::Active;  // Set to Active so it can be dissolved
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    // Add alice as founder
    let alice_id = EntityId::from_did(alice.did());
    // Register alice as an individual entity first
    let alice_entity = CooperativeEntity::individual(alice.did(), &alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership = Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
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
    let alice_entity = CooperativeEntity::individual(alice.did(), &alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership = Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
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
    entity.status = icn_entity::EntityStatus::Active;  // Set to Active so it can be dissolved
    let entity_id = entity.id.clone();
    entity_mgr.register(entity).await.unwrap();

    let alice_id = EntityId::from_did(alice.did());
    // Register alice as an individual entity first
    let alice_entity = CooperativeEntity::individual(alice.did(), &alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership = Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
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
    let alice_entity = CooperativeEntity::individual(alice.did(), &alice_id.to_string());
    entity_mgr.register(alice_entity).await.unwrap();
    let membership = Membership::active(alice_id.clone(), entity_id.clone(), MembershipRole::Founder);
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
