//! Federation handler-family capability migration (#1868 step 5).
//!
//! All seven federation-proposal handlers (`create_join_federation_proposal`,
//! `create_leave_federation_proposal`, `create_establish_clearing_proposal`,
//! `create_terminate_clearing_proposal`, `create_vouch_proposal`,
//! `create_revoke_vouch_proposal`, `create_update_federation_policy_proposal`)
//! share a single auth gate via `extract_federation_common`. Narrowing that one
//! helper to the `governance:federation:write` class scope (while still
//! accepting the legacy broad `governance:write` as an accepted-also fallback)
//! migrates the whole family at once. This file pins that behavior across all
//! seven routes.
//!
//! Assertion semantics: this is an *authorization* test, so it asserts the auth
//! outcome, not a downstream status. A scope-accepted request passes the shared
//! gate and continues into the handler (its eventual status — 400/500/201 —
//! depends on field validity and domain state, which are not under test here),
//! so acceptance is asserted as "not 401 and not 403". A scope-rejected request
//! is blocked at the gate with 403 before any handler logic runs. Deliberately
//! not coupling to a specific downstream code keeps the test robust to unrelated
//! handler changes.
//!
//! Each body is the minimal *deserializable* shape for its route: a malformed
//! body would 400 at `web::Json` extraction before reaching the gate, so the
//! required fields are present (values are dummy — auth runs before field
//! validation). The scope-matching logic itself is unit-tested in
//! `icn-http-kit::auth` (`require_any_scope_*`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use serde_json::{json, Value};

const FEDERATION_CLASS: &str = "governance:federation:write";
const LEGACY_BROAD: &str = "governance:write";
const UNRELATED_READ: &str = "governance:read";
const SIBLING_CLASS: &str = "governance:charter:write";

const DOMAIN_ID: &str = "coop:test";
const TITLE: &str = "Federation Scope Migration";
const DESCRIPTION: &str = "Scope migration test proposal";

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
macro_rules! fed_app {
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

/// The seven federation-proposal routes, each with a minimally deserializable
/// body. All funnel through `extract_federation_common`.
fn federation_routes() -> Vec<(&'static str, Value)> {
    let common = |extra: Value| {
        let mut base = json!({
            "domain_id": DOMAIN_ID,
            "title": TITLE,
            "description": DESCRIPTION,
        });
        if let (Value::Object(b), Value::Object(e)) = (&mut base, extra) {
            b.extend(e);
        }
        base
    };
    vec![
        (
            "/proposals/federation/join",
            common(json!({
                "federation_id": "fed-1",
                "terms": {
                    "min_trust_threshold": 0.5,
                    "governance_binding": false,
                    "data_sharing_level": "summary",
                    "dispute_resolution": "federation_mediation"
                }
            })),
        ),
        (
            "/proposals/federation/leave",
            common(json!({
                "federation_id": "fed-1",
                "reason": "scope test",
                "grace_period_days": 30
            })),
        ),
        (
            "/proposals/federation/clearing/establish",
            common(json!({
                "partner_coop_id": "coop-2",
                "partner_coop_did": "did:icn:test",
                "max_imbalance": 1000,
                "settlement_interval": "weekly",
                "currency": "credits"
            })),
        ),
        (
            "/proposals/federation/clearing/terminate",
            common(json!({
                "partner_coop_id": "coop-2",
                "reason": "scope test"
            })),
        ),
        (
            "/proposals/federation/vouch",
            common(json!({
                "target_coop_id": "coop-2",
                "target_coop_did": "did:icn:test",
                "trust_score": 0.5,
                "context": "scope test"
            })),
        ),
        (
            "/proposals/federation/vouch/revoke",
            common(json!({
                "target_coop_id": "coop-2",
                "reason": "scope test"
            })),
        ),
        ("/proposals/federation/policy", common(json!({}))),
    ]
}

/// POST `body` to `route` under `scope`, returning the HTTP status.
async fn status_for(route: &str, body: &Value, scope: &'static str) -> StatusCode {
    let ctx = make_ctx();
    let caller = fresh_did();
    let app = fed_app!(ctx, &caller, scope);
    let req = test::TestRequest::post()
        .uri(route)
        .set_json(body)
        .to_request();
    test::call_service(&app, req).await.status()
}

/// An accepted scope passes the auth gate; the request continues into the
/// handler and yields some non-auth status.
fn assert_auth_accepted(status: StatusCode, scope: &str, route: &str) {
    assert!(
        status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
        "{scope} must be accepted at {route} (auth passes; {status} is the downstream \
         outcome, not an auth rejection)"
    );
}

#[actix_web::test]
async fn federation_join_accepts_federation_class_scope() {
    let routes = federation_routes();
    let (route, body) = &routes[0];
    let status = status_for(route, body, FEDERATION_CLASS).await;
    assert_auth_accepted(status, FEDERATION_CLASS, route);
}

#[actix_web::test]
async fn federation_join_accepts_legacy_broad_scope() {
    let routes = federation_routes();
    let (route, body) = &routes[0];
    let status = status_for(route, body, LEGACY_BROAD).await;
    assert_auth_accepted(status, LEGACY_BROAD, route);
}

#[actix_web::test]
async fn federation_join_rejects_unrelated_read_scope() {
    let routes = federation_routes();
    let (route, body) = &routes[0];
    let status = status_for(route, body, UNRELATED_READ).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "governance:read must be rejected at {route}, got {status}"
    );
}

#[actix_web::test]
async fn federation_join_rejects_sibling_write_class_scope() {
    // Class isolation: a different governance write class must not satisfy
    // federation:write.
    let routes = federation_routes();
    let (route, body) = &routes[0];
    let status = status_for(route, body, SIBLING_CLASS).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a sibling class scope (governance:charter:write) must not satisfy \
         federation:write at {route}, got {status}"
    );
}

#[actix_web::test]
async fn all_federation_handlers_reject_unrelated_scope() {
    // Proves the single migrated gate covers every federation route: each
    // rejects an unrelated scope at the shared `extract_federation_common`.
    for (route, body) in federation_routes() {
        let status = status_for(route, &body, UNRELATED_READ).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "unrelated scope must be rejected at {route}, got {status}"
        );
    }
}

#[actix_web::test]
async fn all_federation_handlers_accept_federation_class_scope() {
    // The other half of the same claim: every federation route accepts the
    // narrowed class scope (none is blocked at the gate).
    for (route, body) in federation_routes() {
        let status = status_for(route, &body, FEDERATION_CLASS).await;
        assert_auth_accepted(status, FEDERATION_CLASS, route);
    }
}
