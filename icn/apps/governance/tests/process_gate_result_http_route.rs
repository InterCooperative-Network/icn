//! HTTP route proof for the first `ProcessTransitionReceipt` class
//! ([`ProcessGateResultReceipt`]) — issue #2144.
//!
//! The receipt machinery (type, `GovernanceManager::record_process_gate_result`,
//! and the receipt-store record kind) already exists and is proven at the
//! manager layer by `process_gate_result_receipt_runtime_slice.rs`. This
//! test pins the missing surface: **one governance HTTP route that drives
//! that existing path end-to-end**:
//!
//!   `POST /gov/domains/{domain_id}/process-sessions/{session_id}/gate-results`
//!     → governance handler
//!     → GovernanceManager::record_process_gate_result
//!     → persisted ProcessGateResultReceipt
//!     → response body carries the persisted receipt (deterministic record_hash)
//!
//! It does NOT add a new receipt type, a process runtime, or a read-back
//! manager getter. The POST response IS the retrieval/verification surface.
//!
//! Pins:
//!
//! 1. The route is mounted and reachable (not 404).
//! 2. The handler calls `record_process_gate_result` and returns the
//!    persisted receipt: returned fields match the request, `recorded_by`
//!    is the authenticated caller, and `record_hash` is a real blake3
//!    binding (not a zero placeholder) that matches
//!    `ProcessGateResultReceipt::compute_record_hash` for the returned
//!    fields (deterministic).
//! 3. The receipt is actually persisted through the HTTP path — the wired
//!    backend captures it and a `(session_id, gate_kind)` lookup returns
//!    the same `record_hash`.
//! 4. Authorization reuses the existing action-item write surface gate:
//!    a caller who is not a member of the domain receives 403 and no
//!    receipt is persisted (auth decisions are unchanged by this slice).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use actix_web::{body::to_bytes, http::StatusCode, test, App};
use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams, MembershipConfig,
    MembershipSource, ProcessGateKind, ProcessGateResult, ProcessGateResultReceipt,
};
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    receipt_backend::GovernanceReceiptBackend,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Test receipt backend — captures ProcessGateResultReceipt writes + lookups.
// Mirrors the minimal backend in process_gate_result_receipt_runtime_slice.rs:
// overrides only the methods that have no trait default plus the three
// process-gate methods.
// ============================================================================

#[derive(Default)]
struct TestReceiptStore {
    persisted: Mutex<Vec<ProcessGateResultReceipt>>,
}

impl GovernanceReceiptBackend for TestReceiptStore {
    fn put_governance(&self, _r: &GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _p: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn put_allocation(&self, _r: &AllocationReceipt) -> Result<Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _h: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(&self, _h: &Hash) -> Result<Vec<AllocationReceipt>, String> {
        Ok(vec![])
    }

    fn put_process_gate_result(&self, receipt: &ProcessGateResultReceipt) -> Result<(), String> {
        self.persisted.lock().unwrap().push(receipt.clone());
        Ok(())
    }

    fn get_latest_process_gate_result(
        &self,
        session_id: &str,
        gate_kind: ProcessGateKind,
    ) -> Result<Option<ProcessGateResultReceipt>, String> {
        Ok(self
            .persisted
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.session_id == session_id && r.gate_kind == gate_kind)
            .max_by_key(|r| r.recorded_at)
            .cloned())
    }
}

impl TestReceiptStore {
    fn total_count(&self) -> usize {
        self.persisted.lock().unwrap().len()
    }
}

// ============================================================================
// Scaffolding (mirrors me_action_item_receipt_chain.rs)
// ============================================================================

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

struct Harness {
    ctx: GovernanceContext<NoopEventEmitter>,
    receipts: Arc<TestReceiptStore>,
}

fn make_harness() -> Harness {
    let receipts = Arc::new(TestReceiptStore::default());
    let manager = GovernanceManager::new()
        .with_receipt_store(receipts.clone() as Arc<dyn GovernanceReceiptBackend>);
    let ctx = GovernanceContext {
        manager: Arc::new(manager),
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
    };
    Harness { ctx, receipts }
}

async fn seed_domain_with_member(
    mgr: &GovernanceManager,
    member: &Did,
    domain_id: &str,
) -> GovernanceDomainId {
    let domain = GovernanceDomainId::new(domain_id);
    mgr.create_domain(
        domain.clone(),
        "Test Coop".to_string(),
        "default".to_string(),
        GovernanceParams {
            quorum_percentage: 1,
            approval_threshold_percentage: 51,
            voting_period_seconds: 86_400,
            require_deliberation: false,
            ..GovernanceParams::default()
        },
        MembershipConfig {
            source: MembershipSource::StaticList(vec![member.clone()]),
        },
    )
    .await
    .expect("create_domain");
    domain
}

/// Build an actix app for the governance routes, injecting `BasicClaims`
/// for `caller` with the `governance:write` scope (the same scope the
/// action-item write surface requires).
macro_rules! gate_app {
    ($ctx:expr, $caller:expr) => {{
        use actix_web::dev::Service as _;
        use actix_web::HttpMessage as _;
        let caller = $caller.to_string();
        test::init_service(
            App::new()
                .wrap_fn(move |req, srv| {
                    req.extensions_mut().insert(BasicClaims {
                        sub: caller.clone(),
                        scope: Some("governance:write".to_string()),
                    });
                    srv.call(req)
                })
                .configure(|cfg| http::configure(cfg, $ctx)),
        )
        .await
    }};
}

fn gate_results_uri(domain_id: &str, session_id: &str) -> String {
    format!("/domains/{domain_id}/process-sessions/{session_id}/gate-results")
}

// ============================================================================
// Tests
// ============================================================================

#[actix_web::test]
async fn record_process_gate_result_route_persists_and_returns_receipt() {
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;
    let session_id = "session-http-001";

    let app = gate_app!(h.ctx.clone(), &caller);
    let req = test::TestRequest::post()
        .uri(&gate_results_uri(&domain.0, session_id))
        .set_json(serde_json::json!({
            "gate_kind": "privacy_review",
            "result": "pass",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();

    assert_eq!(
        status,
        StatusCode::OK,
        "POST gate-results must mount and succeed; body: {}",
        String::from_utf8_lossy(&bytes)
    );

    let receipt: ProcessGateResultReceipt =
        serde_json::from_slice(&bytes).expect("response body is a ProcessGateResultReceipt");

    // Returned fields match the request + authenticated caller.
    assert_eq!(receipt.session_id, session_id);
    assert_eq!(receipt.domain_id, domain.0);
    assert_eq!(receipt.gate_kind, ProcessGateKind::PrivacyReview);
    assert_eq!(receipt.result, ProcessGateResult::Pass);
    assert_eq!(receipt.recorded_by, caller.to_string());
    assert_ne!(
        receipt.record_hash, [0u8; 32],
        "record_hash must be a real blake3 binding, not a zero placeholder"
    );

    // Deterministic: the returned record_hash binds exactly the returned fields.
    let expected = ProcessGateResultReceipt::compute_record_hash(
        &receipt.session_id,
        &receipt.domain_id,
        receipt.gate_kind,
        receipt.result,
        &receipt.recorded_by,
        receipt.recorded_at,
    );
    assert_eq!(
        receipt.record_hash, expected,
        "record_hash must equal compute_record_hash over the returned fields"
    );

    // Persisted through the HTTP path: the wired backend captured it and a
    // (session_id, gate_kind) lookup returns the same receipt.
    let latest = h
        .receipts
        .get_latest_process_gate_result(session_id, ProcessGateKind::PrivacyReview)
        .expect("lookup")
        .expect("a receipt must be persisted before the route responds");
    assert_eq!(latest.record_hash, receipt.record_hash);
}

#[actix_web::test]
async fn record_process_gate_result_route_rejects_non_member() {
    // The domain exists and has a member, but the authenticated caller is
    // NOT that member. The route must reuse the existing domain-membership
    // gate and return 403 without persisting a receipt.
    let h = make_harness();
    let member = fresh_did();
    let outsider = fresh_did();
    assert_ne!(member, outsider);
    let domain = seed_domain_with_member(&h.ctx.manager, &member, "test-coop").await;

    let app = gate_app!(h.ctx.clone(), &outsider);
    let req = test::TestRequest::post()
        .uri(&gate_results_uri(&domain.0, "session-http-403"))
        .set_json(serde_json::json!({
            "gate_kind": "scope_confirmation",
            "result": "pass",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a non-member must be rejected by the existing domain-membership gate"
    );
    assert_eq!(
        h.receipts.total_count(),
        0,
        "no receipt may be persisted when authorization fails"
    );
}

#[actix_web::test]
async fn record_process_gate_result_route_rejects_unknown_result() {
    // A malformed `result` value must be rejected by JSON deserialization
    // (400), never coerced — the result taxonomy is closed.
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let app = gate_app!(h.ctx.clone(), &caller);
    let req = test::TestRequest::post()
        .uri(&gate_results_uri(&domain.0, "session-http-400"))
        .set_json(serde_json::json!({
            "gate_kind": "privacy_review",
            "result": "maybe",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "an out-of-taxonomy result must be a 400, not silently accepted"
    );
    let _ = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(h.receipts.total_count(), 0);
}

#[actix_web::test]
async fn record_process_gate_result_route_rejects_unknown_fields() {
    // The request contract carries only `gate_kind`/`result`; the path and
    // token supply everything else. `#[serde(deny_unknown_fields)]` makes an
    // extra field fail closed with 400 rather than be silently discarded, so
    // the surface's contract is enforced by rejection, not omission.
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;
    let app = gate_app!(h.ctx.clone(), &caller);

    for extra in [
        serde_json::json!({ "gate_kind": "privacy_review", "result": "pass", "outcome": "accepted" }),
        serde_json::json!({ "gate_kind": "privacy_review", "result": "pass", "foo": 1 }),
    ] {
        let req = test::TestRequest::post()
            .uri(&gate_results_uri(&domain.0, "session-http-deny"))
            .set_json(&extra)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "unknown request field must 400, got {} for {extra}",
            resp.status()
        );
    }
    assert_eq!(
        h.receipts.total_count(),
        0,
        "nothing persisted for rejected unknown-field requests"
    );
}

#[actix_web::test]
async fn record_process_gate_result_route_rejects_whitespace_session_id() {
    // The handler's `session_id.trim().is_empty()` guard must reject a
    // whitespace-only session id (`%20`, which actix percent-decodes to a
    // single space) with 400 and persist nothing.
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let app = gate_app!(h.ctx.clone(), &caller);
    let req = test::TestRequest::post()
        .uri(&gate_results_uri(&domain.0, "%20"))
        .set_json(serde_json::json!({
            "gate_kind": "privacy_review",
            "result": "pass",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "a whitespace-only session_id must be rejected with 400"
    );
    let _ = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(h.receipts.total_count(), 0);
}

#[actix_web::test]
async fn record_process_gate_result_route_rejects_unknown_gate_kind() {
    // Symmetric to the unknown-`result` case: an out-of-taxonomy
    // `gate_kind` must be rejected by JSON deserialization (400), never
    // coerced -- the gate-kind taxonomy is closed.
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let app = gate_app!(h.ctx.clone(), &caller);
    let req = test::TestRequest::post()
        .uri(&gate_results_uri(&domain.0, "session-http-400-gate"))
        .set_json(serde_json::json!({
            "gate_kind": "not_a_real_gate",
            "result": "pass",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "an out-of-taxonomy gate_kind must be a 400, not silently accepted"
    );
    let _ = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(h.receipts.total_count(), 0);
}
