//! Charter handler-family capability migration (#1868 step 3).
//!
//! Proves the four charter-family mutation handlers (`create_domain`,
//! `add_domain_member`, `remove_domain_member`, `activate_charter`) accept the
//! narrowed `governance:charter:write` class scope while still accepting the
//! legacy broad `governance:write` scope as an accepted-also fallback, and
//! reject an unrelated scope.
//!
//! Scope of the proof:
//! - `governance:charter:write` is accepted (the migration's distinguishing
//!   effect — before the migration this class scope would have been rejected by
//!   the broad-only gate).
//! - legacy `governance:write` is still accepted (no token-compat regression).
//! - an unrelated scope (`governance:read`) is rejected with 403.
//!
//! The scope-matching logic itself is unit-tested in
//! `icn-http-kit::auth` (`require_any_scope_*`); this file pins the handler
//! wiring. It does NOT exercise mandate gating (unbuilt), receipt-body scope
//! recording (a later slice), or charter bootstrap labeling (#1869).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{body::to_bytes, dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use serde_json::json;

/// Minimal valid CCL document — mirrors `charter_activation.rs`. With the
/// `on_charter_accepted` hook unwired (see `make_ctx`), a scope-accepted
/// activation proceeds past the gate and fails at the unwired hook (500),
/// which is a non-auth status and therefore evidence the gate let it through.
const VALID_CHARTER_YAML: &str = r#"
schema_version: v0
entity:
  name: "Test Cooperative"
  type: cooperative
"#;

const CHARTER_CLASS: &str = "governance:charter:write";
const LEGACY_BROAD: &str = "governance:write";
const UNRELATED: &str = "governance:read";

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
        mandate_gate: None,
        build_mode: icn_governance_actor::http::GovernanceContextBuildMode::Test,
    }
}

/// Build a test app injecting the given caller + scope on every request.
macro_rules! charter_app {
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

fn create_domain_body(caller: &Did, domain_id: &str) -> serde_json::Value {
    json!({
        "id": domain_id,
        "name": "Charter Scope Migration Test",
        "profile": "cooperative_default",
        "quorum_percent": 50,
        "approval_percent": 50,
        "voting_period_days": 7,
        "members": [caller.to_string()],
    })
}

#[actix_web::test]
async fn create_domain_accepts_charter_class_scope() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let app = charter_app!(ctx, &caller, CHARTER_CLASS);

    let req = test::TestRequest::post()
        .uri("/domains")
        .set_json(create_domain_body(&caller, "csm-charter-class"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "governance:charter:write must be accepted on create_domain, got {status}, body: {}",
        String::from_utf8_lossy(&bytes)
    );
}

#[actix_web::test]
async fn create_domain_accepts_legacy_broad_scope() {
    // Accepted-also fallback: tokens minted before the migration still work.
    let ctx = make_ctx();
    let caller = fresh_did();
    let app = charter_app!(ctx, &caller, LEGACY_BROAD);

    let req = test::TestRequest::post()
        .uri("/domains")
        .set_json(create_domain_body(&caller, "csm-legacy-broad"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "legacy governance:write must remain accepted on create_domain, got {status}, body: {}",
        String::from_utf8_lossy(&bytes)
    );
}

#[actix_web::test]
async fn create_domain_rejects_unrelated_scope() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let app = charter_app!(ctx, &caller, UNRELATED);

    let req = test::TestRequest::post()
        .uri("/domains")
        .set_json(create_domain_body(&caller, "csm-unrelated"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "an unrelated scope (governance:read) must be rejected with 403 on create_domain"
    );
}

/// Each of the four charter-family handlers must accept the class scope and
/// reach handler logic. Asserting the *specific* post-gate status and body
/// (not merely "not 401/403") is deliberate: a router-level 404 from a
/// mis-registered route would also be "not 401/403" and would falsely pass.
/// The expected post-gate outcomes are:
/// - `create_domain` → 201 (valid body, domain created).
/// - `add`/`remove` member → 404 with the handler's "Domain not found" body
///   (targets a non-existent domain, so the handler's own lookup miss — not the
///   later membership 403, and not a router 404 which carries no such body).
/// - `activate_charter` → 500 with the handler's "hook is not wired" body
///   (the unwired `on_charter_accepted` in `make_ctx`).
#[actix_web::test]
async fn all_charter_family_handlers_accept_charter_class_scope() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let member = fresh_did();
    let app = charter_app!(ctx, &caller, CHARTER_CLASS);

    // create_domain → 201 Created (gate passed, handler ran to completion).
    let req = test::TestRequest::post()
        .uri("/domains")
        .set_json(create_domain_body(&caller, "csm-all-create"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "POST /domains under governance:charter:write must reach the handler and create the domain"
    );

    // add_domain_member → 404 with the handler's own "Domain not found" body.
    let req = test::TestRequest::post()
        .uri("/domains/csm-missing/members")
        .set_json(json!({ "did": member.to_string() }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND, "POST /members status");
    let body = to_bytes(resp.into_body()).await.unwrap();
    assert!(
        String::from_utf8_lossy(&body).contains("Domain not found"),
        "POST /members must reach the handler's domain-lookup miss (not a router 404), body: {}",
        String::from_utf8_lossy(&body)
    );

    // remove_domain_member → 404 with the handler's own "Domain not found" body.
    let req = test::TestRequest::delete()
        .uri("/domains/csm-missing/members")
        .set_json(json!({ "did": member.to_string() }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "DELETE /members status"
    );
    let body = to_bytes(resp.into_body()).await.unwrap();
    assert!(
        String::from_utf8_lossy(&body).contains("Domain not found"),
        "DELETE /members must reach the handler's domain-lookup miss (not a router 404), body: {}",
        String::from_utf8_lossy(&body)
    );

    // activate_charter → 500 with the handler's own "hook is not wired" body.
    let req = test::TestRequest::post()
        .uri("/charters")
        .set_json(json!({ "charter_id": "csm-charter", "charter_yaml": VALID_CHARTER_YAML }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "POST /charters status"
    );
    let body = to_bytes(resp.into_body()).await.unwrap();
    assert!(
        String::from_utf8_lossy(&body).contains("hook is not wired"),
        "POST /charters must reach the handler's unwired-hook path (not a router 404), body: {}",
        String::from_utf8_lossy(&body)
    );
}
