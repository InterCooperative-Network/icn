//! Comment handler-family capability migration (#1868 step 11).
//!
//! Migrates the five comment/reaction mutation handlers (`add_comment`,
//! `edit_comment`, `delete_comment`, `add_reaction`, `remove_reaction`) from the
//! broad `governance:write` gate to `governance:comment:write` with the legacy
//! broad scope as an accepted-also fallback. No shared helper — each handler is
//! an independent one-line gate.
//!
//! Assertion semantics (authorization test): acceptance is "not 401 and not
//! 403" (the request passes the gate and continues into the handler; its
//! eventual status depends on proposal/comment state, not under test here);
//! rejection is exactly 403 at the gate, before any handler logic.
//!
//! `edit_comment` / `delete_comment` enforce an author-only app-level check
//! *after* the gate, so an accepted non-author still gets 403 there. We
//! therefore prove ACCEPTANCE on `add_comment` (no author check) and prove
//! REJECTION (unrelated scope -> 403, which fires at the gate, before the
//! author check) across all five routes.

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

const COMMENT_CLASS: &str = "governance:comment:write";
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

macro_rules! gov_app {
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

/// (method, path, deserializable body) for each comment-family route. All five
/// share the migrated gate; bodies deserialize so the request reaches it.
fn comment_routes() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "POST",
            "/proposals/test-proposal/discussion/comments",
            json!({ "content": "scope test" }),
        ),
        (
            "PUT",
            "/proposals/test-proposal/discussion/comments/test-comment",
            json!({ "content": "scope test edit" }),
        ),
        (
            "DELETE",
            "/proposals/test-proposal/discussion/comments/test-comment",
            json!({}),
        ),
        (
            "POST",
            "/proposals/test-proposal/discussion/comments/test-comment/reactions",
            json!({ "emoji": "thumbsup" }),
        ),
        (
            "DELETE",
            "/proposals/test-proposal/discussion/comments/test-comment/reactions",
            json!({ "emoji": "thumbsup" }),
        ),
    ]
}

async fn status_for(method: &str, path: &str, body: &Value, scope: &'static str) -> StatusCode {
    let ctx = make_ctx();
    let caller = fresh_did();
    let app = gov_app!(ctx, &caller, scope);
    let builder = match method {
        "POST" => test::TestRequest::post(),
        "PUT" => test::TestRequest::put(),
        "PATCH" => test::TestRequest::patch(),
        "DELETE" => test::TestRequest::delete(),
        other => panic!("unsupported method {other}"),
    };
    let req = builder.uri(path).set_json(body).to_request();
    test::call_service(&app, req).await.status()
}

fn assert_auth_accepted(status: StatusCode, scope: &str, route: &str) {
    assert!(
        status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
        "{scope} must be accepted at {route} (auth passes; {status} is the downstream \
         outcome, not an auth rejection)"
    );
}

#[actix_web::test]
async fn add_comment_accepts_comment_class_scope() {
    // add_comment has no author check, so a scope-accepted request reaches the
    // handler body (not 401/403).
    let (m, p, b) = &comment_routes()[0];
    assert_auth_accepted(status_for(m, p, b, COMMENT_CLASS).await, COMMENT_CLASS, p);
}

#[actix_web::test]
async fn add_comment_accepts_legacy_broad_scope() {
    let (m, p, b) = &comment_routes()[0];
    assert_auth_accepted(status_for(m, p, b, LEGACY_BROAD).await, LEGACY_BROAD, p);
}

#[actix_web::test]
async fn all_comment_routes_reject_unrelated_read_scope() {
    // Rejection fires at the gate, before the edit/delete author check, so it
    // holds for all five routes.
    for (m, p, b) in comment_routes() {
        let status = status_for(m, p, &b, UNRELATED_READ).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "governance:read must be rejected at {m} {p}, got {status}"
        );
    }
}

#[actix_web::test]
async fn all_comment_routes_reject_sibling_write_class_scope() {
    // Class isolation: a sibling write class never satisfies comment:write.
    for (m, p, b) in comment_routes() {
        let status = status_for(m, p, &b, SIBLING_CLASS).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "sibling class {SIBLING_CLASS} must be rejected at {m} {p}, got {status}"
        );
    }
}
