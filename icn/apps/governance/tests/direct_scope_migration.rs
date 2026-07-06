//! Focused authorization wiring for the six governance handlers that remained
//! broad-scope-only after the first class-scope migrations.
//!
//! The four process-recording routes now accept `governance:process:write`
//! first, while the two constitutional/domain routes accept
//! `governance:charter:write` first. All six retain `governance:write` as a
//! compatibility fallback. These tests use a missing domain (or an unwired
//! mandate backend for declaration): a 404/500 proves the request passed scope
//! admission and reached handler logic without mutating state.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{dev::Service as _, http::StatusCode, test, App, HttpMessage as _};
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};

const PROCESS_CLASS: &str = "governance:process:write";
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
        build_mode: http::GovernanceContextBuildMode::Test,
    }
}

macro_rules! scope_app {
    ($ctx:expr, $caller:expr, $scope:expr) => {{
        let caller = $caller.to_string();
        let scope: Option<String> = $scope.map(str::to_owned);
        test::init_service(
            App::new()
                .wrap_fn(move |req, srv| {
                    req.extensions_mut().insert(BasicClaims {
                        sub: caller.clone(),
                        scope: scope.clone(),
                    });
                    srv.call(req)
                })
                .configure(|cfg| http::configure(cfg, $ctx)),
        )
        .await
    }};
}

macro_rules! assert_direct_handler_statuses {
    ($scope:expr, $expected:expr) => {{
        let ctx = make_ctx();
        let caller = fresh_did();
        let app = scope_app!(ctx, &caller, $scope);
        let hash = "09".repeat(32);
        let expected: [StatusCode; 6] = $expected;
        let cases = vec![
            (
                test::TestRequest::post()
                    .uri("/domains/missing/process-sessions/session/gate-results")
                    .set_json(serde_json::json!({
                        "gate_kind": "privacy_review",
                        "result": "pass"
                    })),
                "record_process_gate_result",
            ),
            (
                test::TestRequest::post()
                    .uri("/domains/missing/process-sessions/session/open"),
                "open_process_session",
            ),
            (
                test::TestRequest::post()
                    .uri("/domains/missing/process-sessions/session/deliberation-entries/entry/record")
                    .set_json(serde_json::json!({
                        "entry_kind": "question",
                        "body_hash": hash
                    })),
                "record_deliberation_entry",
            ),
            (
                test::TestRequest::post()
                    .uri("/domains/missing/process-sessions/session/decisions/decision/record")
                    .set_json(serde_json::json!({ "body_hash": hash })),
                "record_decision",
            ),
            (
                test::TestRequest::post()
                    .uri("/domains/missing/domain-policy/adopt")
                    .set_json(serde_json::json!({ "policy_content": "policy v1" })),
                "adopt_domain_policy",
            ),
            (
                test::TestRequest::post()
                    .uri("/domains/missing/institutional-domain/declare")
                    .set_json(serde_json::json!({ "entity_type": "cooperative" })),
                "declare_institutional_domain",
            ),
        ];

        for ((request, handler), expected_status) in cases.into_iter().zip(expected) {
            let response = test::call_service(&app, request.to_request()).await;
            assert_eq!(
                response.status(),
                expected_status,
                "unexpected scope-admission outcome for {handler}"
            );
        }
    }};
}

#[actix_web::test]
async fn process_class_is_accepted_only_by_the_four_process_handlers() {
    assert_direct_handler_statuses!(
        Some(PROCESS_CLASS),
        [
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND,
            StatusCode::FORBIDDEN,
            StatusCode::FORBIDDEN,
        ]
    );
}

#[actix_web::test]
async fn charter_class_is_accepted_only_by_the_two_constitutional_handlers() {
    assert_direct_handler_statuses!(
        Some(CHARTER_CLASS),
        [
            StatusCode::FORBIDDEN,
            StatusCode::FORBIDDEN,
            StatusCode::FORBIDDEN,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::INTERNAL_SERVER_ERROR,
        ]
    );
}

#[actix_web::test]
async fn broad_fallback_remains_accepted_by_all_six_handlers() {
    assert_direct_handler_statuses!(
        Some(LEGACY_BROAD),
        [
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND,
            StatusCode::INTERNAL_SERVER_ERROR,
        ]
    );
}

#[actix_web::test]
async fn unrelated_scope_is_rejected_by_all_six_handlers() {
    assert_direct_handler_statuses!(Some(UNRELATED), [StatusCode::FORBIDDEN; 6]);
}

#[actix_web::test]
async fn missing_scope_is_rejected_by_all_six_handlers() {
    assert_direct_handler_statuses!(None, [StatusCode::FORBIDDEN; 6]);
}
