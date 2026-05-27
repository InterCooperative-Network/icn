//! Steward handler capability migration (#1868 step 4).
//!
//! Proves the single direct-mutation steward act (`assign_role`) accepts the
//! narrowed `governance:steward:write` class scope while still accepting the
//! legacy broad `governance:write` scope as an accepted-also fallback, and
//! rejects scopes outside that set.
//!
//! `assign_role` is the only steward act in this rung: it directly grants
//! steward authority, so it carries its own narrow scope. Steward *proposals*
//! (`create_appoint_steward_proposal`, `create_remove_steward_proposal`) stay
//! under the proposal-write family — they create a proposal that still must
//! pass voting before authority is granted — and are migrated in a later rung.
//!
//! Scope of the proof:
//! - `governance:steward:write` is accepted (the migration's distinguishing
//!   effect — before the migration this class scope would have been rejected by
//!   the broad-only gate).
//! - legacy `governance:write` is still accepted (no token-compat regression).
//! - an unrelated read scope (`governance:read`) is rejected with 403.
//! - a sibling write class (`governance:charter:write`) is rejected with 403 —
//!   the point of decomposition is that one class scope does not satisfy
//!   another.
//!
//! With no structure created, a scope-accepted request reaches the handler and
//! fails downstream with 500 (Structure not found via `anyhow_to_api` →
//! `Internal`) — a non-auth status, and therefore evidence the gate let it
//! through. A scope-rejected request is blocked at the gate with 403 before any
//! manager logic runs.
//!
//! The scope-matching logic itself is unit-tested in `icn-http-kit::auth`
//! (`require_any_scope_*`); this file pins the handler wiring. It does NOT
//! exercise mandate gating (unbuilt) or receipt-body scope recording (a later
//! slice).

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

const STEWARD_CLASS: &str = "governance:steward:write";
const LEGACY_BROAD: &str = "governance:write";
const UNRELATED_READ: &str = "governance:read";
const SIBLING_CLASS: &str = "governance:charter:write";

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
macro_rules! roles_app {
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

/// POST a role assignment to a (non-existent) structure under `scope`, and
/// return the resulting HTTP status. The structure does not exist, so a
/// scope-accepted request reaches the manager and fails with 500; a
/// scope-rejected request is blocked at the auth gate with 403.
async fn assign_role_status(scope: &'static str) -> StatusCode {
    let ctx = make_ctx();
    let caller = fresh_did();
    let target = fresh_did();
    let app = roles_app!(ctx, &caller, scope);

    let body = json!({
        "did": target.to_string(),
        "role": "coordinator",
        "authority_scope": [],
    });
    let req = test::TestRequest::post()
        .uri("/structures/test-structure/roles")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    // Drain the body so a failed assertion can surface it via the caller.
    let _ = to_bytes(resp.into_body()).await;
    status
}

#[actix_web::test]
async fn assign_role_accepts_steward_class_scope() {
    let status = assign_role_status(STEWARD_CLASS).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "governance:steward:write must be accepted on assign_role (reaches the \
         handler; 500 = structure-not-found downstream, a non-auth status), got {status}"
    );
}

#[actix_web::test]
async fn assign_role_accepts_legacy_broad_scope() {
    let status = assign_role_status(LEGACY_BROAD).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "legacy governance:write must remain accepted on assign_role, got {status}"
    );
}

#[actix_web::test]
async fn assign_role_rejects_unrelated_read_scope() {
    let status = assign_role_status(UNRELATED_READ).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "governance:read must be rejected on assign_role, got {status}"
    );
}

#[actix_web::test]
async fn assign_role_rejects_sibling_write_class_scope() {
    // Class isolation: a different governance write class must not satisfy
    // steward:write — this is the whole point of the decomposition.
    let status = assign_role_status(SIBLING_CLASS).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a sibling class scope (governance:charter:write) must not satisfy \
         steward:write on assign_role, got {status}"
    );
}
