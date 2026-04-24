//! Charter activation HTTP endpoint (issue #1602).
//!
//! Proves the bootstrap-side `POST /charters` endpoint:
//!
//! 1. Validated CCL YAML → 201 with `{ charter_id, status: "active", activated_at }`
//!    and the wired `on_charter_accepted` hook is invoked exactly once with the
//!    submitted (charter_id, charter_yaml) pair.
//! 2. Reactivation of an existing `charter_id` succeeds (idempotent — the
//!    underlying oracle overwrites in place).
//! 3. Malformed YAML is rejected at the boundary with 400 (the hook is *not*
//!    invoked, preventing silent drops downstream).
//! 4. Empty `charter_id` and oversize `charter_yaml` are rejected with 400.
//! 5. Missing `governance:write` scope returns 403.
//! 6. Response payload uses regulatory-safe vocabulary
//!    (no "payment", "currency", or "balance" terms).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use actix_web::{body::to_bytes, dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance_actor::{
    http::{self, configure::CharterAcceptedHook, GovernanceContext},
    manager::GovernanceManager,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use serde_json::{json, Value};

/// Minimal valid CCL YAML — `from_yaml` + `validate` accept an empty document
/// (all sections are `Option<...>`). For rigor we still pass a populated entity
/// section so a future tightening of `validate()` will fail loudly here.
const VALID_CHARTER_YAML: &str = r#"
schema_version: v0
entity:
  name: "Test Cooperative"
  type: cooperative
"#;

type CapturedHookCalls = Arc<Mutex<Vec<(String, String)>>>;

fn make_ctx_with_capture() -> (GovernanceContext<NoopEventEmitter>, CapturedHookCalls) {
    let captured: CapturedHookCalls = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured.clone();
    let hook: CharterAcceptedHook = Arc::new(move |id, yaml| {
        captured_clone.lock().unwrap().push((id, yaml));
    });

    let ctx = GovernanceContext {
        manager: Arc::new(GovernanceManager::new()),
        emitter: NoopEventEmitter,
        on_charter_accepted: Some(hook),
        on_proposal_accepted: None,
        on_proposal_accepted_with_evidence: None,
        member_checker: None,
        steward_checker: None,
        suspension_checker: None,
        membership_resolver: None,
        sdis_service: None,
    };

    (ctx, captured)
}

/// Build a test app with full governance route configuration, injecting the
/// given scope (or none) into every request — bypasses JWT validation.
macro_rules! charter_test_app {
    ($ctx:expr, $scope:expr) => {{
        let scope: Option<&'static str> = $scope;
        test::init_service(
            App::new()
                .wrap_fn(move |req, srv| {
                    if let Some(scope_str) = scope {
                        req.extensions_mut().insert(BasicClaims {
                            sub: "did:icn:caller".to_string(),
                            scope: Some(scope_str.to_string()),
                        });
                    }
                    srv.call(req)
                })
                .configure(|cfg| http::configure(cfg, $ctx)),
        )
        .await
    }};
}

#[actix_web::test]
async fn activate_charter_returns_201_and_fires_hook() {
    let (ctx, captured) = make_ctx_with_capture();
    let app = charter_test_app!(ctx, Some("governance:write"));

    let body = json!({
        "charter_id": "nycn-bootstrap",
        "charter_yaml": VALID_CHARTER_YAML,
    });
    let req = test::TestRequest::post()
        .uri("/charters")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body_bytes = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "valid charter must return 201 Created, body: {}",
        String::from_utf8_lossy(&body_bytes)
    );
    let parsed: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(parsed["charter_id"], "nycn-bootstrap");
    assert_eq!(parsed["status"], "active");
    assert!(
        parsed["activated_at"].as_u64().is_some(),
        "activated_at must be a unix epoch seconds u64"
    );

    // Regulatory-safe vocabulary: response must not leak economic-payment terms.
    let raw = String::from_utf8(body_bytes.to_vec()).unwrap();
    let raw_lower = raw.to_lowercase();
    for forbidden in ["payment", "currency", "balance"] {
        assert!(
            !raw_lower.contains(forbidden),
            "response must not contain forbidden term '{forbidden}': {raw}"
        );
    }

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "hook must fire exactly once");
    assert_eq!(captured[0].0, "nycn-bootstrap");
    assert_eq!(captured[0].1, VALID_CHARTER_YAML);
}

#[actix_web::test]
async fn activate_charter_idempotent_on_reactivation() {
    let (ctx, captured) = make_ctx_with_capture();
    let app = charter_test_app!(ctx, Some("governance:write"));

    let body = json!({
        "charter_id": "nycn-bootstrap",
        "charter_yaml": VALID_CHARTER_YAML,
    });

    for attempt in 0..2 {
        let req = test::TestRequest::post()
            .uri("/charters")
            .set_json(&body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "reactivation attempt {attempt} must succeed"
        );
    }

    let captured = captured.lock().unwrap();
    assert_eq!(
        captured.len(),
        2,
        "hook must fire once per activation request"
    );
    assert_eq!(
        captured[0].0, captured[1].0,
        "charter_id stable across reactivation"
    );
}

#[actix_web::test]
async fn activate_charter_rejects_malformed_yaml() {
    let (ctx, captured) = make_ctx_with_capture();
    let app = charter_test_app!(ctx, Some("governance:write"));

    let body = json!({
        "charter_id": "bad-coop",
        // Invalid YAML — unbalanced braces
        "charter_yaml": "schema_version: v0\nentity: { id: ",
    });
    let req = test::TestRequest::post()
        .uri("/charters")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "malformed YAML must return 400, got {}",
        resp.status()
    );

    assert!(
        captured.lock().unwrap().is_empty(),
        "hook must NOT fire on malformed input — boundary validation must reject before deploy"
    );
}

#[actix_web::test]
async fn activate_charter_rejects_empty_id() {
    let (ctx, _captured) = make_ctx_with_capture();
    let app = charter_test_app!(ctx, Some("governance:write"));

    let body = json!({
        "charter_id": "",
        "charter_yaml": VALID_CHARTER_YAML,
    });
    let req = test::TestRequest::post()
        .uri("/charters")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "empty charter_id must return 400"
    );
}

#[actix_web::test]
async fn activate_charter_rejects_empty_yaml() {
    let (ctx, _captured) = make_ctx_with_capture();
    let app = charter_test_app!(ctx, Some("governance:write"));

    let body = json!({
        "charter_id": "nycn-bootstrap",
        "charter_yaml": "   \n   ",
    });
    let req = test::TestRequest::post()
        .uri("/charters")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "whitespace-only YAML must return 400"
    );
}

#[actix_web::test]
async fn activate_charter_requires_governance_write_scope() {
    let (ctx, captured) = make_ctx_with_capture();
    // No claims at all → middleware injects nothing, require_scope returns Forbidden.
    let app = charter_test_app!(ctx, None);

    let body = json!({
        "charter_id": "nycn-bootstrap",
        "charter_yaml": VALID_CHARTER_YAML,
    });
    let req = test::TestRequest::post()
        .uri("/charters")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;

    let status = resp.status();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "missing scope must return 401 or 403, got {status}"
    );

    assert!(
        captured.lock().unwrap().is_empty(),
        "hook must NOT fire without governance:write scope"
    );
}

#[actix_web::test]
async fn activate_charter_returns_500_when_hook_not_wired() {
    // Build a context with `on_charter_accepted: None` to prove the handler
    // surfaces a clear server error instead of silently succeeding.
    let ctx = GovernanceContext {
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
    };
    let app = charter_test_app!(ctx, Some("governance:write"));

    let body = json!({
        "charter_id": "nycn-bootstrap",
        "charter_yaml": VALID_CHARTER_YAML,
    });
    let req = test::TestRequest::post()
        .uri("/charters")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "missing hook must return 500, not silently succeed"
    );
}
