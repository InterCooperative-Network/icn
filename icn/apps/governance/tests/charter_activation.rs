//! Charter activation HTTP endpoint (issue #1602).
//!
//! Proves the bootstrap-side `POST /charters` endpoint:
//!
//! 1. Validated CCL YAML → 201 with `{ charter_id, status: "active", activated_at }`
//!    and the wired `on_charter_accepted` hook is invoked exactly once with the
//!    submitted (charter_id, charter_yaml) pair.
//! 2. The same activation also dispatches `GovernanceEffect::DeployCharter` via
//!    `on_proposal_accepted`, mirroring the accepted-proposal close path so
//!    commons / SDIS jurisdiction-binding flows see the domain.
//! 3. Reactivation of an existing `charter_id` succeeds (idempotent — the
//!    underlying oracle overwrites in place).
//! 4. Malformed YAML is rejected at the boundary with 400 (neither hook fires).
//! 5. Empty `charter_id` and whitespace-only `charter_yaml` are rejected with 400.
//! 6. Oversize `charter_yaml` (exceeding `MAX_CHARTER_YAML_BYTES`) is rejected
//!    with 400 by the validator before downstream dispatch.
//! 7. Missing `governance:write` scope returns 401 or 403 (auth layer choice;
//!    both are accepted as evidence of unauthorized rejection).
//! 8. If the proposal-acceptance hook is unwired, the endpoint returns 500
//!    rather than silently succeeding with commons unaware of the domain.
//! 9. Response payload uses regulatory-safe vocabulary
//!    (no "payment", "currency", or "balance" terms).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use actix_web::{body::to_bytes, dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance_actor::{
    http::{
        self,
        configure::{CharterAcceptedHook, GovernanceEffect, ProposalAcceptedHook},
        GovernanceContext,
    },
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

type CapturedCharterCalls = Arc<Mutex<Vec<(String, String)>>>;
type CapturedEffectCalls = Arc<Mutex<Vec<GovernanceEffect>>>;

struct CapturedHooks {
    charter_calls: CapturedCharterCalls,
    effect_calls: CapturedEffectCalls,
}

/// Build a context with both the charter-accepted and proposal-accepted hooks
/// wired to in-memory captures. The handler invokes both hooks on success, so
/// every test that exercises the success path must wire both.
fn make_ctx_with_capture() -> (GovernanceContext<NoopEventEmitter>, CapturedHooks) {
    let charter_calls: CapturedCharterCalls = Arc::new(Mutex::new(Vec::new()));
    let effect_calls: CapturedEffectCalls = Arc::new(Mutex::new(Vec::new()));

    let charter_calls_clone = charter_calls.clone();
    let charter_hook: CharterAcceptedHook = Arc::new(move |id, yaml| {
        charter_calls_clone.lock().unwrap().push((id, yaml));
    });

    let effect_calls_clone = effect_calls.clone();
    let proposal_hook: ProposalAcceptedHook = Arc::new(move |effect| {
        effect_calls_clone.lock().unwrap().push(effect);
    });

    let ctx = GovernanceContext {
        manager: Arc::new(GovernanceManager::new()),
        emitter: NoopEventEmitter,
        on_charter_accepted: Some(charter_hook),
        on_proposal_accepted: Some(proposal_hook),
        on_proposal_accepted_with_evidence: None,
        member_checker: None,
        steward_checker: None,
        suspension_checker: None,
        membership_resolver: None,
        sdis_service: None,
    };

    (
        ctx,
        CapturedHooks {
            charter_calls,
            effect_calls,
        },
    )
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

    let charter = captured.charter_calls.lock().unwrap();
    assert_eq!(charter.len(), 1, "charter hook must fire exactly once");
    assert_eq!(charter[0].0, "nycn-bootstrap");
    assert_eq!(charter[0].1, VALID_CHARTER_YAML);

    // Equivalence with the accepted-proposal close path: the same activation
    // must dispatch DeployCharter so commons registers the domain.
    let effects = captured.effect_calls.lock().unwrap();
    assert_eq!(
        effects.len(),
        1,
        "proposal-accepted hook must fire exactly once"
    );
    match &effects[0] {
        GovernanceEffect::DeployCharter {
            charter_id,
            proposal_id,
        } => {
            assert_eq!(charter_id, "nycn-bootstrap");
            assert!(
                proposal_id.starts_with("direct-activation:"),
                "synthetic proposal_id must mark the direct-activation origin, got {proposal_id}"
            );
        }
        other => panic!("expected GovernanceEffect::DeployCharter, got {other:?}"),
    }
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

    let charter = captured.charter_calls.lock().unwrap();
    assert_eq!(
        charter.len(),
        2,
        "charter hook must fire once per activation request"
    );
    assert_eq!(
        charter[0].0, charter[1].0,
        "charter_id stable across reactivation"
    );

    let effects = captured.effect_calls.lock().unwrap();
    assert_eq!(
        effects.len(),
        2,
        "proposal-accepted hook must fire once per activation"
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
        captured.charter_calls.lock().unwrap().is_empty(),
        "charter hook must NOT fire on malformed input"
    );
    assert!(
        captured.effect_calls.lock().unwrap().is_empty(),
        "proposal-accepted hook must NOT fire on malformed input"
    );
}

#[actix_web::test]
async fn activate_charter_rejects_oversize_yaml() {
    let (ctx, captured) = make_ctx_with_capture();
    let app = charter_test_app!(ctx, Some("governance:write"));

    // MAX_CHARTER_YAML_BYTES is 256 KiB (matched to the gateway-wide JsonConfig
    // limit). Build a YAML body that is structurally valid but exceeds that
    // bound, by inflating the entity name with filler text. The validator
    // must reject this with 400 before any hook fires. (The init_service test
    // harness used here has no JsonConfig limit wired, so the request reaches
    // the validator — proving the validator alone enforces the cap.)
    let filler = "x".repeat(300_000);
    let oversize_yaml =
        format!("schema_version: v0\nentity:\n  name: \"{filler}\"\n  type: cooperative\n");
    let body = json!({
        "charter_id": "oversize-coop",
        "charter_yaml": oversize_yaml,
    });
    let req = test::TestRequest::post()
        .uri("/charters")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "oversize charter_yaml must return 400 from the boundary validator"
    );
    assert!(
        captured.charter_calls.lock().unwrap().is_empty(),
        "charter hook must NOT fire on oversize input"
    );
    assert!(
        captured.effect_calls.lock().unwrap().is_empty(),
        "proposal-accepted hook must NOT fire on oversize input"
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
    // No claims at all → middleware injects nothing, require_scope returns
    // an unauthorized error. The exact code (401 vs 403) is the auth layer's
    // choice; both are accepted as evidence of unauthorized rejection.
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
        captured.charter_calls.lock().unwrap().is_empty(),
        "charter hook must NOT fire without governance:write scope"
    );
    assert!(
        captured.effect_calls.lock().unwrap().is_empty(),
        "proposal-accepted hook must NOT fire without governance:write scope"
    );
}

#[actix_web::test]
async fn activate_charter_returns_500_when_charter_hook_not_wired() {
    // `on_charter_accepted: None` — the handler surfaces 500 rather than
    // silently succeeding (the kernel would never start enforcing the charter).
    let ctx = GovernanceContext {
        manager: Arc::new(GovernanceManager::new()),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: Some(Arc::new(|_| {})),
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
        "missing charter hook must return 500, not silently succeed"
    );
}

#[actix_web::test]
async fn activate_charter_returns_500_when_proposal_hook_not_wired() {
    // `on_proposal_accepted: None` — without it commons never registers the
    // domain, so steward / jurisdiction-binding flows would fail downstream.
    // Returning 500 keeps the contract honest: a 201 response means the
    // charter is fully activated, not just half-activated.
    let charter_calls: CapturedCharterCalls = Arc::new(Mutex::new(Vec::new()));
    let charter_calls_clone = charter_calls.clone();
    let charter_hook: CharterAcceptedHook = Arc::new(move |id, yaml| {
        charter_calls_clone.lock().unwrap().push((id, yaml));
    });
    let ctx = GovernanceContext {
        manager: Arc::new(GovernanceManager::new()),
        emitter: NoopEventEmitter,
        on_charter_accepted: Some(charter_hook),
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
        "missing proposal-accepted hook must return 500"
    );
    // Note: the charter hook fires before the proposal-hook check, so the
    // CharterPolicyOracle has already deployed the document. That is
    // acceptable — the kernel is now enforcing the charter — and is the
    // narrowest behavior to assert without a synchronous rollback path.
    assert_eq!(
        charter_calls.lock().unwrap().len(),
        1,
        "charter hook fires before the proposal-hook check"
    );
}
