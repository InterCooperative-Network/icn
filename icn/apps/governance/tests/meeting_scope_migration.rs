//! Meeting handler-family capability migration (#1868 step 9).
//!
//! Migrates the twelve meeting / action-item mutation handlers (`create_meeting`,
//! `start_meeting`, `end_meeting`, `add_agenda_item`, `update_agenda_item`,
//! `add_attendee`, `mark_attendance`, `create_action_item`, `update_action_item`,
//! `update_action_item_status`, `delete_action_item`, `add_action_item_note`)
//! from the broad `governance:write` gate to `governance:meeting:write` with the
//! legacy broad scope as an accepted-also fallback. No shared helper — each is an
//! independent one-line gate.
//!
//! Assertion semantics (authorization test): acceptance is "not 401 and not 403"
//! (the request passes the gate and continues into the handler; its eventual
//! status depends on meeting/domain state, not under test here); rejection is
//! exactly 403 at the gate, before any handler logic.

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

const MEETING_CLASS: &str = "governance:meeting:write";
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

/// (method, path, deserializable body) for each meeting-family route.
fn meeting_routes() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "POST",
            "/domains/test-domain/meetings",
            json!({ "title": "scope test" }),
        ),
        ("POST", "/meetings/test-meeting/start", json!({})),
        ("POST", "/meetings/test-meeting/end", json!({})),
        (
            "POST",
            "/meetings/test-meeting/attendees",
            json!({ "did": "did:icn:test" }),
        ),
        (
            "PUT",
            "/meetings/test-meeting/attendance",
            json!({ "did": "did:icn:test", "status": "present" }),
        ),
        (
            "POST",
            "/meetings/test-meeting/agenda",
            json!({ "title": "scope test" }),
        ),
        ("PUT", "/meetings/test-meeting/agenda/test-item", json!({})),
        (
            "POST",
            "/domains/test-domain/action-items",
            json!({ "title": "scope test" }),
        ),
        (
            "PUT",
            "/domains/test-domain/action-items/test-item",
            json!({}),
        ),
        (
            "PUT",
            "/domains/test-domain/action-items/test-item/status",
            json!({ "status": "in_progress" }),
        ),
        (
            "DELETE",
            "/domains/test-domain/action-items/test-item",
            json!({}),
        ),
        (
            "POST",
            "/domains/test-domain/action-items/test-item/notes",
            json!({ "content": "scope test note" }),
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
async fn create_meeting_accepts_meeting_class_scope() {
    let (m, p, b) = &meeting_routes()[0];
    assert_auth_accepted(status_for(m, p, b, MEETING_CLASS).await, MEETING_CLASS, p);
}

#[actix_web::test]
async fn create_meeting_accepts_legacy_broad_scope() {
    let (m, p, b) = &meeting_routes()[0];
    assert_auth_accepted(status_for(m, p, b, LEGACY_BROAD).await, LEGACY_BROAD, p);
}

#[actix_web::test]
async fn all_meeting_routes_reject_unrelated_read_scope() {
    for (m, p, b) in meeting_routes() {
        let status = status_for(m, p, &b, UNRELATED_READ).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "governance:read must be rejected at {m} {p}, got {status}"
        );
    }
}

#[actix_web::test]
async fn all_meeting_routes_reject_sibling_write_class_scope() {
    for (m, p, b) in meeting_routes() {
        let status = status_for(m, p, &b, SIBLING_CLASS).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "sibling class {SIBLING_CLASS} must be rejected at {m} {p}, got {status}"
        );
    }
}
