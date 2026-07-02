//! HTTP route proof for the fourth `ProcessTransitionReceipt` class
//! ([`DecisionRecordedReceipt`]) — the #2280 contract + #2281 Q4 decision.
//!
//! The receipt machinery (type, manager method, session precondition,
//! atomic insert-if-absent persistence) is proven at the manager layer by
//! `decision_recorded_receipt_runtime_slice.rs`. This test pins the HTTP
//! surface:
//!
//!   `POST /gov/domains/{domain_id}/process-sessions/{session_id}/decisions/{decision_id}/record`
//!     → governance handler (governance:write + domain membership)
//!     → GovernanceManager::record_decision
//!     → persisted DecisionRecordedReceipt
//!     → response body carries the persisted receipt (body_hash only —
//!       never a decision body, and none of the proposal/vote lineage's
//!       fields)
//!
//! Pins:
//!
//! 1. The route is mounted and reachable; the response carries the
//!    persisted receipt with `recorded_by` = authenticated caller, a
//!    deterministic blake3 `record_hash`, and NO body content field and NO
//!    outcome/tally/vote/proposal/mandate field (#2281 Axes A/B).
//! 2. Same-identity retry over HTTP returns the ORIGINAL receipt with
//!    200 — idempotent, byte-stable `record_hash`/`recorded_at`.
//! 3. A mismatched duplicate (different recorder) gets 409 and the
//!    original stays untouched.
//! 4. A non-member caller receives 403 and nothing is persisted.
//! 5. Whitespace `decision_id` → 400; malformed/short `body_hash` → 400.
//! 6. Recording against a session with no recorded opening → 404
//!    (`decision_recorded_session_not_opened`; absent referenced anchor,
//!    mirroring the missing-domain mapping) and nothing is persisted —
//!    no silent session creation.
//!
//! This surface is parallel to and never touches the proposal/vote
//! decision endpoints, their typed store, effect dispatch, or action
//! cards. A receipt records an institutional fact and grants zero
//! authority; `recorded_by` is recorder evidence, not decider identity.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use actix_web::{body::to_bytes, http::StatusCode, test, App};
use icn_governance::{
    DecisionRecordedReceipt, GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams,
    MembershipConfig, MembershipSource,
};
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    receipt_backend::{decision_recorded_composite_key1, GovernanceReceiptBackend},
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Opaque-capable test backend — implements the opaque primitives
// (including the atomic put_opaque_if_absent) so the trait's typed
// decision-recorded defaults drive persistence, mirroring the production
// gateway ReceiptStore posture.
// ============================================================================

type ChainKey = (String, String, Option<String>);

/// One persisted opaque entry: `(recorded_at, record_hash, payload)`.
type ChainEntry = (u64, [u8; 32], Vec<u8>);

#[derive(Default)]
struct OpaqueUniqueBackend {
    chains: Mutex<HashMap<ChainKey, Vec<ChainEntry>>>,
    unique: Mutex<HashMap<ChainKey, [u8; 32]>>,
}

impl OpaqueUniqueBackend {
    fn decision_count(&self, domain_id: &str, session_id: &str, decision_id: &str) -> usize {
        self.chains
            .lock()
            .unwrap()
            .get(&(
                "decision_recorded".to_string(),
                decision_recorded_composite_key1(domain_id, session_id),
                Some(decision_id.to_string()),
            ))
            .map_or(0, Vec::len)
    }
}

impl GovernanceReceiptBackend for OpaqueUniqueBackend {
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

    fn put_opaque_if_absent(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
        recorded_at: u64,
        record_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<Option<[u8; 32]>, String> {
        let key = (class.to_string(), key1.to_string(), key2.map(String::from));
        let mut unique = self.unique.lock().unwrap();
        if let Some(winner) = unique.get(&key) {
            return Ok(Some(*winner));
        }
        unique.insert(key.clone(), record_hash);
        self.chains.lock().unwrap().entry(key).or_default().push((
            recorded_at,
            record_hash,
            payload.to_vec(),
        ));
        Ok(None)
    }

    fn get_latest_opaque(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
    ) -> Result<Option<Vec<u8>>, String> {
        let key = (class.to_string(), key1.to_string(), key2.map(String::from));
        Ok(self.chains.lock().unwrap().get(&key).and_then(|chain| {
            chain
                .iter()
                .max_by_key(|(t, h, _)| (*t, *h))
                .map(|(_, _, p)| p.clone())
        }))
    }
}

// ============================================================================
// Scaffolding (mirrors deliberation_entry_record_http_route.rs)
// ============================================================================

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

struct Harness {
    ctx: GovernanceContext<NoopEventEmitter>,
    receipts: Arc<OpaqueUniqueBackend>,
}

fn make_harness() -> Harness {
    let receipts = Arc::new(OpaqueUniqueBackend::default());
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

async fn seed_domain_with_members(
    mgr: &GovernanceManager,
    members: &[Did],
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
            source: MembershipSource::StaticList(members.to_vec()),
        },
    )
    .await
    .expect("create_domain");
    domain
}

/// Open the session at the manager layer so the route's precondition is
/// satisfied where a test wants it satisfied.
fn open_session(mgr: &GovernanceManager, domain: &GovernanceDomainId, session: &str, by: &Did) {
    mgr.record_process_session_opened(domain, session, by)
        .expect("session open must succeed");
}

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

fn record_uri(domain_id: &str, session_id: &str, decision_id: &str) -> String {
    format!("/domains/{domain_id}/process-sessions/{session_id}/decisions/{decision_id}/record")
}

/// 64-hex-char body fingerprint for tests: `byte` repeated 32 times.
fn hex_body_hash(byte: u8) -> String {
    format!("{byte:02x}").repeat(32)
}

fn record_body(body_hash: &str) -> serde_json::Value {
    serde_json::json!({ "body_hash": body_hash })
}

// ============================================================================
// Tests
// ============================================================================

#[actix_web::test]
async fn record_route_persists_and_returns_receipt() {
    let h = make_harness();
    let caller = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&caller), "test-coop").await;
    open_session(&h.ctx.manager, &domain, "session-http", &caller);

    let app = gate_app!(h.ctx.clone(), &caller);
    let req = test::TestRequest::post()
        .uri(&record_uri(&domain.0, "session-http", "decision-http"))
        .set_json(record_body(&hex_body_hash(9)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();

    assert_eq!(
        status,
        StatusCode::OK,
        "POST record must mount and succeed; body: {}",
        String::from_utf8_lossy(&bytes)
    );

    let receipt: DecisionRecordedReceipt =
        serde_json::from_slice(&bytes).expect("response body is a DecisionRecordedReceipt");
    assert_eq!(receipt.domain_id, domain.0);
    assert_eq!(receipt.session_id, "session-http");
    assert_eq!(receipt.decision_id, "decision-http");
    assert_eq!(receipt.recorded_by, caller.to_string());
    assert_eq!(receipt.body_hash, [9u8; 32]);
    // Deterministic binding over the returned fields.
    let expected = DecisionRecordedReceipt::compute_record_hash(
        &receipt.domain_id,
        &receipt.session_id,
        &receipt.decision_id,
        &receipt.recorded_by,
        receipt.recorded_at,
        &receipt.body_hash,
    );
    assert_eq!(receipt.record_hash, expected);
    // Persisted through the HTTP path.
    assert_eq!(
        h.receipts
            .decision_count(&domain.0, "session-http", "decision-http"),
        1
    );
    // Privacy + lineage discipline: the response carries body_hash only —
    // no body content field of any name, and none of the proposal/vote
    // lineage's fields (#2281 Axes A/B).
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let obj = value.as_object().unwrap();
    assert!(obj.contains_key("body_hash"));
    for forbidden in [
        "body",
        "content",
        "text",
        "message",
        "outcome",
        "vote_tally",
        "vote_hash",
        "proposal_id",
        "mandate_attestation",
        "capability_scope_presented",
    ] {
        assert!(
            !obj.contains_key(forbidden),
            "response must not carry a `{forbidden}` field"
        );
    }
}

#[actix_web::test]
async fn record_route_same_identity_retry_is_idempotent() {
    let h = make_harness();
    let caller = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&caller), "test-coop").await;
    open_session(&h.ctx.manager, &domain, "session-retry", &caller);
    let app = gate_app!(h.ctx.clone(), &caller);

    let make_req = || {
        test::TestRequest::post()
            .uri(&record_uri(&domain.0, "session-retry", "decision-r"))
            .set_json(record_body(&hex_body_hash(9)))
            .to_request()
    };

    let first = test::call_service(&app, make_req()).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first: DecisionRecordedReceipt =
        serde_json::from_slice(&to_bytes(first.into_body()).await.unwrap()).unwrap();

    let second = test::call_service(&app, make_req()).await;
    assert_eq!(second.status(), StatusCode::OK, "retry is idempotent");
    let second: DecisionRecordedReceipt =
        serde_json::from_slice(&to_bytes(second.into_body()).await.unwrap()).unwrap();

    assert_eq!(
        second.record_hash, first.record_hash,
        "original, never restamped"
    );
    assert_eq!(second.recorded_at, first.recorded_at);
    assert_eq!(
        h.receipts
            .decision_count(&domain.0, "session-retry", "decision-r"),
        1
    );
}

#[actix_web::test]
async fn record_route_mismatched_recorder_conflicts_409() {
    let h = make_harness();
    let caller = fresh_did();
    let other = fresh_did();
    let domain = seed_domain_with_members(
        &h.ctx.manager,
        &[caller.clone(), other.clone()],
        "test-coop",
    )
    .await;
    open_session(&h.ctx.manager, &domain, "session-conflict", &caller);

    let app = gate_app!(h.ctx.clone(), &caller);
    let req = test::TestRequest::post()
        .uri(&record_uri(&domain.0, "session-conflict", "decision-c"))
        .set_json(record_body(&hex_body_hash(9)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let original: DecisionRecordedReceipt =
        serde_json::from_slice(&to_bytes(resp.into_body()).await.unwrap()).unwrap();

    // A different member retrying the same decision_id (same body hash) is
    // a different recorder — stable identity mismatch, fail-closed 409.
    let app_other = gate_app!(h.ctx.clone(), &other);
    let req = test::TestRequest::post()
        .uri(&record_uri(&domain.0, "session-conflict", "decision-c"))
        .set_json(record_body(&hex_body_hash(9)))
        .to_request();
    let resp = test::call_service(&app_other, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "different recorder must 409"
    );

    // Original untouched; still exactly one persisted decision.
    let read = h
        .ctx
        .manager
        .get_decision_recorded(&domain, "session-conflict", "decision-c")
        .unwrap()
        .unwrap();
    assert_eq!(read, original);
    assert_eq!(
        h.receipts
            .decision_count(&domain.0, "session-conflict", "decision-c"),
        1
    );
}

#[actix_web::test]
async fn record_route_non_member_403_nothing_persisted() {
    let h = make_harness();
    let member = fresh_did();
    let outsider = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&member), "test-coop").await;
    open_session(&h.ctx.manager, &domain, "session-authz", &member);

    let app = gate_app!(h.ctx.clone(), &outsider);
    let req = test::TestRequest::post()
        .uri(&record_uri(&domain.0, "session-authz", "decision-x"))
        .set_json(record_body(&hex_body_hash(9)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "non-member must be refused"
    );
    assert_eq!(
        h.receipts
            .decision_count(&domain.0, "session-authz", "decision-x"),
        0,
        "nothing persisted for a refused caller"
    );
}

#[actix_web::test]
async fn record_route_bad_inputs_400() {
    let h = make_harness();
    let caller = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&caller), "test-coop").await;
    open_session(&h.ctx.manager, &domain, "session-bad", &caller);
    let app = gate_app!(h.ctx.clone(), &caller);

    // Whitespace decision_id path segment → 400.
    let req = test::TestRequest::post()
        .uri(&record_uri(&domain.0, "session-bad", "%20%20"))
        .set_json(record_body(&hex_body_hash(9)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "whitespace decision_id must 400"
    );

    // Non-hex body_hash → 400.
    let req = test::TestRequest::post()
        .uri(&record_uri(&domain.0, "session-bad", "decision-bad"))
        .set_json(record_body("zz-not-hex"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "non-hex body_hash must 400"
    );

    // Wrong-length hex body_hash → 400.
    let req = test::TestRequest::post()
        .uri(&record_uri(&domain.0, "session-bad", "decision-bad"))
        .set_json(record_body("abcd"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "short body_hash must 400"
    );

    assert_eq!(
        h.receipts
            .decision_count(&domain.0, "session-bad", "decision-bad"),
        0,
        "nothing persisted by rejected requests"
    );
}

#[actix_web::test]
async fn record_route_unopened_session_404_nothing_persisted() {
    let h = make_harness();
    let caller = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&caller), "test-coop").await;
    // Session deliberately NOT opened.

    let app = gate_app!(h.ctx.clone(), &caller);
    let req = test::TestRequest::post()
        .uri(&record_uri(&domain.0, "session-ghost", "decision-g"))
        .set_json(record_body(&hex_body_hash(9)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "unopened session anchor must 404; body: {}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(
        String::from_utf8_lossy(&bytes).contains("decision_recorded_session_not_opened"),
        "stable precondition prefix in error body"
    );
    assert_eq!(
        h.receipts
            .decision_count(&domain.0, "session-ghost", "decision-g"),
        0,
        "nothing persisted; no session silently created"
    );
}
