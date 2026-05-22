//! End-to-end HTTP round-trip for `authority_scope` on role assignment (#1629).
//!
//! Proves the new field on `AssignRoleRequest` lands on the persisted
//! `RoleAssignment`, surfaces on the `RoleAssignmentResponse`, and rolls up
//! into `/me/standing.authority_scopes`. Backward-compat is pinned by a
//! second request that omits the field entirely.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{body::to_bytes, dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance::StructureKind;
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use serde_json::{json, Value};

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

fn make_ctx() -> GovernanceContext<NoopEventEmitter> {
    GovernanceContext {
        manager: Arc::new(GovernanceManager::new()),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        on_proposal_accepted_with_evidence: None,
        member_checker: None,
        steward_checker: None,
        suspension_checker: None,
        membership_resolver: None,
        sdis_service: None,
        build_mode: icn_governance_actor::http::GovernanceContextBuildMode::Test,
    }
}

/// Build a test app injecting the given caller + scope on every request.
macro_rules! role_test_app {
    ($ctx:expr, $caller_did:expr, $scope:expr) => {{
        let scope: &'static str = $scope;
        let caller = $caller_did.to_string();
        test::init_service(
            App::new()
                .wrap_fn(move |req, srv| {
                    req.extensions_mut().insert(BasicClaims {
                        sub: caller.clone(),
                        scope: Some(scope.to_string()),
                    });
                    srv.call(req)
                })
                .configure(|cfg| http::configure(cfg, $ctx)),
        )
        .await
    }};
}

#[actix_web::test]
async fn assign_role_with_authority_scope_round_trips_to_me_standing() {
    let ctx = make_ctx();
    let caller = fresh_did();

    // Pre-create a structure the role will attach to. parent_entity_id is
    // arbitrary — the test pins behavior, not NYCN topology.
    let structure = ctx
        .manager
        .create_structure(
            "entity-a".to_string(),
            StructureKind::Committee,
            "Finance Committee".to_string(),
            None,
        )
        .expect("create_structure");

    // The handler registry differs across requests because /me/standing needs
    // governance:read while POST /roles needs governance:write. Build two
    // apps from the same context — manager state is shared via Arc.
    let writer_app = role_test_app!(ctx.clone(), &caller, "governance:write");
    let reader_app = role_test_app!(ctx.clone(), &caller, "governance:read");

    // 1) POST /structures/{id}/roles with explicit authority_scope.
    let assign_body = json!({
        "did": caller.to_string(),
        "role": "treasurer",
        "authority_scope": [
            "approve_budget_within_policy",
            "confirm_settlement_within_policy",
        ],
    });
    let req = test::TestRequest::post()
        .uri(&format!("/structures/{}/roles", structure.id.0))
        .set_json(&assign_body)
        .to_request();
    let resp = test::call_service(&writer_app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "POST /roles must return 201, got {status}, body: {}",
        String::from_utf8_lossy(&bytes)
    );
    let role_resp: Value = serde_json::from_slice(&bytes).unwrap();
    let returned_scope: Vec<String> = role_resp["authority_scope"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        returned_scope,
        vec![
            "approve_budget_within_policy".to_string(),
            "confirm_settlement_within_policy".to_string(),
        ],
        "AssignRoleRequest.authority_scope must round-trip on the response"
    );

    // 2) GET /me/standing returns the same scope on the role row and in the
    // top-level union.
    let req = test::TestRequest::get().uri("/me/standing").to_request();
    let resp = test::call_service(&reader_app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(status, StatusCode::OK);
    let standing: Value = serde_json::from_slice(&bytes).unwrap();
    let roles = standing["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 1);
    let row_scope: Vec<String> = roles[0]["authority_scope"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        row_scope,
        vec![
            "approve_budget_within_policy".to_string(),
            "confirm_settlement_within_policy".to_string(),
        ],
        "/me/standing roles[].authority_scope must match the assigned scope"
    );
    let rollup: Vec<String> = standing["authority_scopes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    // Sorted/deduped union — same two entries, alphabetical order.
    assert_eq!(
        rollup,
        vec![
            "approve_budget_within_policy".to_string(),
            "confirm_settlement_within_policy".to_string(),
        ],
        "/me/standing.authority_scopes union must include all per-role scopes"
    );
}

#[actix_web::test]
async fn assign_role_without_authority_scope_remains_valid_and_empty() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let structure = ctx
        .manager
        .create_structure(
            "entity-a".to_string(),
            StructureKind::Committee,
            "Steering".to_string(),
            None,
        )
        .expect("create_structure");

    let writer_app = role_test_app!(ctx.clone(), &caller, "governance:write");
    let reader_app = role_test_app!(ctx.clone(), &caller, "governance:read");

    // Backward-compat: POST without `authority_scope` must still succeed.
    // The serde default keeps this an empty vector, not a missing-field
    // error — old clients keep working unchanged.
    let assign_body = json!({
        "did": caller.to_string(),
        "role": "coordinator",
    });
    let req = test::TestRequest::post()
        .uri(&format!("/structures/{}/roles", structure.id.0))
        .set_json(&assign_body)
        .to_request();
    let resp = test::call_service(&writer_app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    let role_resp: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        role_resp["authority_scope"].as_array().unwrap().is_empty(),
        "missing authority_scope must default to empty, not error"
    );

    // /me/standing rolls up empty.
    let req = test::TestRequest::get().uri("/me/standing").to_request();
    let resp = test::call_service(&reader_app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    let standing: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(standing["authority_scopes"].as_array().unwrap().is_empty());
    let roles = standing["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 1);
    assert!(roles[0]["authority_scope"].as_array().unwrap().is_empty());
}
