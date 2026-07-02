//! HTTP route proof for the second `ProcessTransitionReceipt` class
//! ([`ProcessSessionOpenedReceipt`]) — the #2275 session anchor.
//!
//! The receipt machinery (type, manager method, atomic insert-if-absent
//! persistence) is proven at the manager layer by
//! `process_session_opened_receipt_runtime_slice.rs`. This test pins the
//! HTTP surface:
//!
//!   `POST /gov/domains/{domain_id}/process-sessions/{session_id}/open`
//!     → governance handler (governance:write + domain membership)
//!     → GovernanceManager::record_process_session_opened
//!     → persisted ProcessSessionOpenedReceipt
//!     → response body carries the persisted receipt
//!
//! Pins:
//!
//! 1. The route is mounted and reachable; the response carries the
//!    persisted receipt with `opened_by` = authenticated caller and a
//!    deterministic blake3 `record_hash`.
//! 2. Same-opener retry over HTTP returns the ORIGINAL receipt
//!    (identical `record_hash`/`opened_at`) with 200 — idempotent.
//! 3. A different authenticated member opening the same session gets
//!    409 and the original stays untouched (fail-closed conflict).
//! 4. A caller who is not a member of the domain receives 403 and no
//!    opening is persisted — the route mirrors the gate-result write
//!    surface's authorization posture exactly (per #2275: sticky
//!    duplicate-opens must not let an outsider preempt a session id).
//! 5. Whitespace `session_id` is rejected with 400.
//!
//! No stored `ProcessSession`, no lifecycle, no process runtime. A
//! receipt records an institutional fact and grants zero authority.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use actix_web::{body::to_bytes, http::StatusCode, test, App};
use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams, MembershipConfig,
    MembershipSource, ProcessSessionOpenedReceipt,
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
// Opaque-capable test backend — implements the opaque primitives
// (including the atomic put_opaque_if_absent) so the trait's typed
// session-open defaults drive persistence, mirroring the production
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
    fn open_count(&self, domain_id: &str, session_id: &str) -> usize {
        self.chains
            .lock()
            .unwrap()
            .get(&(
                "process_session_opened".to_string(),
                domain_id.to_string(),
                Some(session_id.to_string()),
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
// Scaffolding (mirrors process_gate_result_http_route.rs)
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

fn open_uri(domain_id: &str, session_id: &str) -> String {
    format!("/domains/{domain_id}/process-sessions/{session_id}/open")
}

// ============================================================================
// Tests
// ============================================================================

#[actix_web::test]
async fn open_route_persists_and_returns_receipt() {
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let app = gate_app!(h.ctx.clone(), &caller);
    let req = test::TestRequest::post()
        .uri(&open_uri(&domain.0, "session-http-open"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();

    assert_eq!(
        status,
        StatusCode::OK,
        "POST open must mount and succeed; body: {}",
        String::from_utf8_lossy(&bytes)
    );

    let receipt: ProcessSessionOpenedReceipt =
        serde_json::from_slice(&bytes).expect("response body is a ProcessSessionOpenedReceipt");
    assert_eq!(receipt.session_id, "session-http-open");
    assert_eq!(receipt.domain_id, domain.0);
    assert_eq!(receipt.opened_by, caller.to_string());
    assert_ne!(receipt.record_hash, [0u8; 32]);
    // Deterministic binding over the returned fields.
    let expected = ProcessSessionOpenedReceipt::compute_record_hash(
        &receipt.session_id,
        &receipt.domain_id,
        &receipt.opened_by,
        receipt.opened_at,
    );
    assert_eq!(receipt.record_hash, expected);
    // Persisted through the HTTP path.
    assert_eq!(h.receipts.open_count(&domain.0, "session-http-open"), 1);
}

#[actix_web::test]
async fn open_route_same_opener_retry_is_idempotent() {
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;
    let app = gate_app!(h.ctx.clone(), &caller);

    let first = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&open_uri(&domain.0, "session-retry"))
            .to_request(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let first: ProcessSessionOpenedReceipt =
        serde_json::from_slice(&to_bytes(first.into_body()).await.unwrap()).unwrap();

    let second = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&open_uri(&domain.0, "session-retry"))
            .to_request(),
    )
    .await;
    assert_eq!(second.status(), StatusCode::OK, "retry is idempotent");
    let second: ProcessSessionOpenedReceipt =
        serde_json::from_slice(&to_bytes(second.into_body()).await.unwrap()).unwrap();

    assert_eq!(
        second.record_hash, first.record_hash,
        "original, never restamped"
    );
    assert_eq!(second.opened_at, first.opened_at);
    assert_eq!(h.receipts.open_count(&domain.0, "session-retry"), 1);
}

#[actix_web::test]
async fn open_route_different_opener_conflicts_409() {
    let h = make_harness();
    let opener_a = fresh_did();
    let opener_b = fresh_did();
    // Both are members of the domain; only the OPENER identity differs.
    let domain = GovernanceDomainId::new("test-coop");
    h.ctx
        .manager
        .create_domain(
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
                source: MembershipSource::StaticList(vec![opener_a.clone(), opener_b.clone()]),
            },
        )
        .await
        .expect("create_domain");

    let app_a = gate_app!(h.ctx.clone(), &opener_a);
    let first = test::call_service(
        &app_a,
        test::TestRequest::post()
            .uri(&open_uri(&domain.0, "session-conflict"))
            .to_request(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let original: ProcessSessionOpenedReceipt =
        serde_json::from_slice(&to_bytes(first.into_body()).await.unwrap()).unwrap();

    let app_b = gate_app!(h.ctx.clone(), &opener_b);
    let second = test::call_service(
        &app_b,
        test::TestRequest::post()
            .uri(&open_uri(&domain.0, "session-conflict"))
            .to_request(),
    )
    .await;
    assert_eq!(
        second.status(),
        StatusCode::CONFLICT,
        "different opener must be refused with 409"
    );

    // Original untouched; still exactly one persisted opening.
    assert_eq!(h.receipts.open_count(&domain.0, "session-conflict"), 1);
    let read = h
        .ctx
        .manager
        .get_process_session_opened(&domain, "session-conflict")
        .unwrap()
        .unwrap();
    assert_eq!(read, original);
}

#[actix_web::test]
async fn open_route_non_member_rejected_403_and_nothing_persisted() {
    let h = make_harness();
    let member = fresh_did();
    let outsider = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &member, "test-coop").await;

    // The outsider is authenticated with governance:write but is NOT a
    // member of the domain — the membership gate must refuse (mirroring
    // the gate-result write surface), otherwise a stranger could preempt
    // a session id in someone else's domain.
    let app = gate_app!(h.ctx.clone(), &outsider);
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&open_uri(&domain.0, "session-outsider"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "non-member must be rejected with 403"
    );
    assert_eq!(
        h.receipts.open_count(&domain.0, "session-outsider"),
        0,
        "no opening persisted for a rejected caller"
    );
}

#[actix_web::test]
async fn open_route_whitespace_session_id_rejected_400() {
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let app = gate_app!(h.ctx.clone(), &caller);
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&open_uri(&domain.0, "%20%20"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "whitespace session_id must be rejected"
    );
    assert_eq!(h.receipts.open_count(&domain.0, "  "), 0);
}
