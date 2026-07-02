//! HTTP route proof for the third `ProcessTransitionReceipt` class
//! ([`DeliberationEntryRecordedReceipt`]) — the #2277 contract + #2278 Q3
//! taxonomy decision.
//!
//! The receipt machinery (type, manager method, session precondition,
//! atomic insert-if-absent persistence) is proven at the manager layer by
//! `deliberation_entry_recorded_receipt_runtime_slice.rs`. This test pins
//! the HTTP surface:
//!
//!   `POST /gov/domains/{domain_id}/process-sessions/{session_id}/deliberation-entries/{entry_id}/record`
//!     → governance handler (governance:write + domain membership)
//!     → GovernanceManager::record_deliberation_entry
//!     → persisted DeliberationEntryRecordedReceipt
//!     → response body carries the persisted receipt (body_hash only —
//!       never a deliberation body)
//!
//! Pins:
//!
//! 1. The route is mounted and reachable; the response carries the
//!    persisted receipt with `author` = authenticated caller, the closed
//!    taxonomy kind, a deterministic blake3 `record_hash`, and NO body
//!    content field.
//! 2. Same-identity retry over HTTP returns the ORIGINAL receipt with
//!    200 — idempotent, byte-stable `record_hash`/`recorded_at`.
//! 3. A mismatched duplicate (different entry_kind; different author)
//!    gets 409 and the original stays untouched.
//! 4. A non-member caller receives 403 and nothing is persisted.
//! 5. Whitespace `entry_id` → 400; out-of-taxonomy `entry_kind` → 400
//!    (serde fail-closed); malformed `body_hash` → 400.
//! 6. Recording against a session with no recorded opening → 404
//!    (`deliberation_entry_session_not_opened`; absent referenced anchor,
//!    mirroring the missing-domain mapping) and nothing is persisted —
//!    no silent session creation.
//!
//! No stored `DeliberationThread`, no discussion system, no process
//! runtime. A receipt records an institutional fact and grants zero
//! authority.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use actix_web::{body::to_bytes, http::StatusCode, test, App};
use icn_governance::{
    DeliberationEntryKind, DeliberationEntryRecordedReceipt, GovernanceDecisionReceipt,
    GovernanceDomainId, GovernanceParams, MembershipConfig, MembershipSource,
};
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    receipt_backend::{deliberation_entry_composite_key1, GovernanceReceiptBackend},
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Opaque-capable test backend — implements the opaque primitives
// (including the atomic put_opaque_if_absent) so the trait's typed
// deliberation-entry defaults drive persistence, mirroring the production
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
    fn entry_count(&self, domain_id: &str, session_id: &str, entry_id: &str) -> usize {
        self.chains
            .lock()
            .unwrap()
            .get(&(
                "deliberation_entry_recorded".to_string(),
                deliberation_entry_composite_key1(domain_id, session_id),
                Some(entry_id.to_string()),
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
// Scaffolding (mirrors process_session_open_http_route.rs)
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

fn record_uri(domain_id: &str, session_id: &str, entry_id: &str) -> String {
    format!(
        "/domains/{domain_id}/process-sessions/{session_id}/deliberation-entries/{entry_id}/record"
    )
}

/// 64-hex-char body fingerprint for tests: `byte` repeated 32 times.
fn hex_body_hash(byte: u8) -> String {
    format!("{byte:02x}").repeat(32)
}

fn record_body(kind: &str, body_hash: &str) -> serde_json::Value {
    serde_json::json!({ "entry_kind": kind, "body_hash": body_hash })
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
        .uri(&record_uri(&domain.0, "session-http", "entry-http"))
        .set_json(record_body("question", &hex_body_hash(7)))
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

    let receipt: DeliberationEntryRecordedReceipt = serde_json::from_slice(&bytes)
        .expect("response body is a DeliberationEntryRecordedReceipt");
    assert_eq!(receipt.domain_id, domain.0);
    assert_eq!(receipt.session_id, "session-http");
    assert_eq!(receipt.entry_id, "entry-http");
    assert_eq!(receipt.author, caller.to_string());
    assert_eq!(receipt.entry_kind, DeliberationEntryKind::Question);
    assert_eq!(receipt.body_hash, [7u8; 32]);
    // Deterministic binding over the returned fields.
    let expected = DeliberationEntryRecordedReceipt::compute_record_hash(
        &receipt.domain_id,
        &receipt.session_id,
        &receipt.entry_id,
        &receipt.author,
        receipt.entry_kind,
        receipt.recorded_at,
        &receipt.body_hash,
    );
    assert_eq!(receipt.record_hash, expected);
    // Persisted through the HTTP path.
    assert_eq!(
        h.receipts
            .entry_count(&domain.0, "session-http", "entry-http"),
        1
    );
    // Privacy discipline: the response carries body_hash only — no body
    // content field of any name.
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let obj = value.as_object().unwrap();
    assert!(obj.contains_key("body_hash"));
    for forbidden in ["body", "content", "text", "message"] {
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
            .uri(&record_uri(&domain.0, "session-retry", "entry-r"))
            .set_json(record_body("concern", &hex_body_hash(9)))
            .to_request()
    };

    let first = test::call_service(&app, make_req()).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first: DeliberationEntryRecordedReceipt =
        serde_json::from_slice(&to_bytes(first.into_body()).await.unwrap()).unwrap();

    let second = test::call_service(&app, make_req()).await;
    assert_eq!(second.status(), StatusCode::OK, "retry is idempotent");
    let second: DeliberationEntryRecordedReceipt =
        serde_json::from_slice(&to_bytes(second.into_body()).await.unwrap()).unwrap();

    assert_eq!(
        second.record_hash, first.record_hash,
        "original, never restamped"
    );
    assert_eq!(second.recorded_at, first.recorded_at);
    assert_eq!(
        h.receipts
            .entry_count(&domain.0, "session-retry", "entry-r"),
        1
    );
}

#[actix_web::test]
async fn record_route_different_kind_conflicts_409() {
    // entry_kind participates in duplicate identity (#2278): the same
    // caller re-posting the same entry_id with a different kind is a
    // conflict, never a silent original-receipt return.
    let h = make_harness();
    let caller = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&caller), "test-coop").await;
    open_session(&h.ctx.manager, &domain, "session-kind", &caller);
    let app = gate_app!(h.ctx.clone(), &caller);

    let first = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&record_uri(&domain.0, "session-kind", "entry-k"))
            .set_json(record_body("question", &hex_body_hash(1)))
            .to_request(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    let second = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&record_uri(&domain.0, "session-kind", "entry-k"))
            .set_json(record_body("blocker", &hex_body_hash(1)))
            .to_request(),
    )
    .await;
    assert_eq!(
        second.status(),
        StatusCode::CONFLICT,
        "different entry_kind must be refused with 409"
    );
    assert_eq!(
        h.receipts.entry_count(&domain.0, "session-kind", "entry-k"),
        1,
        "original untouched"
    );
}

#[actix_web::test]
async fn record_route_different_author_conflicts_409() {
    let h = make_harness();
    let author_a = fresh_did();
    let author_b = fresh_did();
    // Both are members; only the AUTHOR identity differs.
    let domain = seed_domain_with_members(
        &h.ctx.manager,
        &[author_a.clone(), author_b.clone()],
        "test-coop",
    )
    .await;
    open_session(&h.ctx.manager, &domain, "session-author", &author_a);

    let app_a = gate_app!(h.ctx.clone(), &author_a);
    let first = test::call_service(
        &app_a,
        test::TestRequest::post()
            .uri(&record_uri(&domain.0, "session-author", "entry-a"))
            .set_json(record_body("objection", &hex_body_hash(3)))
            .to_request(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let original: DeliberationEntryRecordedReceipt =
        serde_json::from_slice(&to_bytes(first.into_body()).await.unwrap()).unwrap();

    let app_b = gate_app!(h.ctx.clone(), &author_b);
    let second = test::call_service(
        &app_b,
        test::TestRequest::post()
            .uri(&record_uri(&domain.0, "session-author", "entry-a"))
            .set_json(record_body("objection", &hex_body_hash(3)))
            .to_request(),
    )
    .await;
    assert_eq!(
        second.status(),
        StatusCode::CONFLICT,
        "different author must be refused with 409"
    );
    let read = h
        .ctx
        .manager
        .get_deliberation_entry(&domain, "session-author", "entry-a")
        .unwrap()
        .unwrap();
    assert_eq!(read, original, "original untouched");
}

#[actix_web::test]
async fn record_route_non_member_rejected_403_and_nothing_persisted() {
    let h = make_harness();
    let member = fresh_did();
    let outsider = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&member), "test-coop").await;
    open_session(&h.ctx.manager, &domain, "session-outsider", &member);

    let app = gate_app!(h.ctx.clone(), &outsider);
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&record_uri(&domain.0, "session-outsider", "entry-o"))
            .set_json(record_body("question", &hex_body_hash(1)))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "non-member must be rejected with 403"
    );
    assert_eq!(
        h.receipts
            .entry_count(&domain.0, "session-outsider", "entry-o"),
        0,
        "no entry persisted for a rejected caller"
    );
}

#[actix_web::test]
async fn record_route_session_not_opened_404_and_nothing_persisted() {
    let h = make_harness();
    let caller = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&caller), "test-coop").await;
    // Deliberately NO session open.

    let app = gate_app!(h.ctx.clone(), &caller);
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&record_uri(&domain.0, "session-unopened", "entry-u"))
            .set_json(record_body("question", &hex_body_hash(1)))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "an unopened session is an absent referenced anchor → 404"
    );
    assert_eq!(
        h.receipts
            .entry_count(&domain.0, "session-unopened", "entry-u"),
        0
    );
    // No silent session creation through the failed record.
    assert!(h
        .ctx
        .manager
        .get_process_session_opened(&domain, "session-unopened")
        .unwrap()
        .is_none());
}

#[actix_web::test]
async fn record_route_bad_inputs_rejected_400() {
    let h = make_harness();
    let caller = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&caller), "test-coop").await;
    open_session(&h.ctx.manager, &domain, "session-bad", &caller);
    let app = gate_app!(h.ctx.clone(), &caller);

    // Whitespace entry_id.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&record_uri(&domain.0, "session-bad", "%20%20"))
            .set_json(record_body("question", &hex_body_hash(1)))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "whitespace entry_id must be rejected"
    );

    // Out-of-taxonomy entry_kind — serde fails closed (deferred
    // `resolution` included).
    for bad_kind in ["resolution", "approve", "vote", "chat_message"] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&record_uri(&domain.0, "session-bad", "entry-bk"))
                .set_json(record_body(bad_kind, &hex_body_hash(1)))
                .to_request(),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "kind `{bad_kind}` must be rejected"
        );
    }

    // Malformed body_hash: not hex / wrong length.
    for bad_hash in ["zz", "abcd", ""] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&record_uri(&domain.0, "session-bad", "entry-bh"))
                .set_json(record_body("question", bad_hash))
                .to_request(),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "body_hash `{bad_hash}` must be rejected"
        );
    }

    // Nothing persisted by any rejected request.
    assert_eq!(
        h.receipts.entry_count(&domain.0, "session-bad", "entry-bk"),
        0
    );
    assert_eq!(
        h.receipts.entry_count(&domain.0, "session-bad", "entry-bh"),
        0
    );
}

#[actix_web::test]
async fn record_route_rejects_unknown_fields_400() {
    // The request contract carries only `entry_kind`/`body_hash`; the path and
    // token supply everything else, and the body itself never crosses this
    // surface. `#[serde(deny_unknown_fields)]` makes an extra field (a raw
    // body, or smuggled decision semantics) fail closed with 400 rather than
    // be silently discarded, so the contract is enforced by rejection.
    let h = make_harness();
    let caller = fresh_did();
    let domain =
        seed_domain_with_members(&h.ctx.manager, std::slice::from_ref(&caller), "test-coop").await;
    open_session(&h.ctx.manager, &domain, "session-deny", &caller);
    let app = gate_app!(h.ctx.clone(), &caller);

    for extra in [
        serde_json::json!({ "entry_kind": "question", "body_hash": hex_body_hash(1), "body": "raw text" }),
        serde_json::json!({ "entry_kind": "question", "body_hash": hex_body_hash(1), "outcome": "accepted" }),
        serde_json::json!({ "entry_kind": "question", "body_hash": hex_body_hash(1), "foo": 1 }),
    ] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&record_uri(&domain.0, "session-deny", "entry-deny"))
                .set_json(&extra)
                .to_request(),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "unknown request field must 400, got {} for {extra}",
            resp.status()
        );
    }
    assert_eq!(
        h.receipts
            .entry_count(&domain.0, "session-deny", "entry-deny"),
        0,
        "nothing persisted for rejected unknown-field requests"
    );
}
