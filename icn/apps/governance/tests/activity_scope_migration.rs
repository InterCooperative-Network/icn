//! Activity handler-family capability migration (#1868 step 10).
//!
//! Migrates the eight activity / program / structure / milestone mutation
//! handlers (`create_activity`, `create_program`, `create_structure`,
//! `create_milestone`, `link_activity_to_program`, `unlink_activity_from_program`,
//! `update_milestone_status`, `update_program_status`) from the broad
//! `governance:write` gate to `governance:activity:write` with the legacy broad
//! scope as an accepted-also fallback. No shared helper — each is an independent
//! one-line gate.
//!
//! Note: `update_milestone_status` and `update_program_status` are status
//! transitions flagged in the decomposition design (§6) as medium-blast and
//! will require the mandate gate in a later step. This PR only narrows the
//! technical capability scope; the mandate-check is a separate future layer.
//!
//! Assertion semantics (authorization test): acceptance is "not 401 and not 403"
//! (the request passes the gate and continues into the handler; its eventual
//! status depends on entity/program state, not under test here); rejection is
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

const ACTIVITY_CLASS: &str = "governance:activity:write";
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
        mandate_gate: None,
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

/// (method, path, deserializable body) for each activity-family route.
/// CreateProgram/CreateMilestone/UpdateMilestoneStatus/UpdateProgramStatus use
/// `#[serde(deny_unknown_fields)]`, so their bodies carry only known fields.
fn activity_routes() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "POST",
            "/entities/test-entity/activities",
            json!({ "kind": "event", "name": "scope test" }),
        ),
        (
            "POST",
            "/domains/test-domain/programs",
            json!({ "parent_entity_id": "test-entity", "kind": "cycle", "name": "scope test" }),
        ),
        (
            "POST",
            "/entities/test-entity/structures",
            json!({ "kind": "committee", "name": "scope test" }),
        ),
        (
            "POST",
            "/programs/test-program/milestones",
            json!({ "name": "scope test" }),
        ),
        (
            "PUT",
            "/programs/test-program/activities/test-activity",
            json!({}),
        ),
        (
            "DELETE",
            "/programs/test-program/activities/test-activity",
            json!({}),
        ),
        (
            "PATCH",
            "/milestones/test-milestone",
            json!({ "status": "in_progress" }),
        ),
        (
            "PATCH",
            "/programs/test-program/status",
            json!({ "status": "active_planning" }),
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
async fn create_activity_accepts_activity_class_scope() {
    let (m, p, b) = &activity_routes()[0];
    assert_auth_accepted(status_for(m, p, b, ACTIVITY_CLASS).await, ACTIVITY_CLASS, p);
}

#[actix_web::test]
async fn create_activity_accepts_legacy_broad_scope() {
    let (m, p, b) = &activity_routes()[0];
    assert_auth_accepted(status_for(m, p, b, LEGACY_BROAD).await, LEGACY_BROAD, p);
}

#[actix_web::test]
async fn all_activity_routes_reject_unrelated_read_scope() {
    for (m, p, b) in activity_routes() {
        let status = status_for(m, p, &b, UNRELATED_READ).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "governance:read must be rejected at {m} {p}, got {status}"
        );
    }
}

#[actix_web::test]
async fn all_activity_routes_reject_sibling_write_class_scope() {
    for (m, p, b) in activity_routes() {
        let status = status_for(m, p, &b, SIBLING_CLASS).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "sibling class {SIBLING_CLASS} must be rejected at {m} {p}, got {status}"
        );
    }
}
