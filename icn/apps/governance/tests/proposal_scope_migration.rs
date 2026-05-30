//! Proposal-family capability migration (#1868 slices A0 + A1c).
//!
//! A0 migrated the normal scoped HTTP proposal-close gate
//! (`POST /proposals/{proposal_id}/close`, `handlers::close_proposal`) from the
//! broad `governance:write` to `governance:proposal:write` (legacy broad scope
//! kept as an accepted-also fallback). Close is the one proposal-lifecycle path
//! that emits a `GovernanceDecisionReceiptV3`, so it additionally records the
//! actually-matched scope as evidence.
//!
//! A1c migrates the remaining HTTP proposal-family gates the same way —
//! `create_proposal`, `open_proposal`, `cast_vote`, `create_delegation`,
//! `revoke_delegation`, `create_appoint_steward_proposal`,
//! `create_remove_steward_proposal` — to
//! `["governance:proposal:write", "governance:write"]`. These paths emit no v3,
//! so they use `require_any_scope` (no matched-scope evidence threaded).
//!
//! Two properties are pinned:
//! - **Authorization** — the close gate accepts `governance:proposal:write`,
//!   still accepts legacy `governance:write` as a fallback, and rejects an
//!   unrelated read scope and a sibling write class (403 at the gate).
//! - **Honest evidence** — the emitted v3 records the scope the auth layer
//!   *actually matched*: the narrow class when presented (including when both
//!   are present, since it is listed first), the broad scope only when the
//!   request was accepted via the fallback. Never a fabricated/desired value.
//!
//! Scheduler/timer (unscoped) and forced-accept (Bootstrap) close paths are not
//! exercised here — they emit no v3 and are pinned by
//! `governance_decision_receipt_v3_emission.rs`, unchanged by this slice.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use actix_web::{dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDecisionReceiptV3, GovernanceDomainId, GovernanceParams,
    MembershipConfig, ProofOutcome, ProposalId, ProposalPayload, ProposalScope,
    ReceiptMandateAttestation, VoteChoice,
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
use serde_json::{json, Value};

const PROPOSAL_CLASS: &str = "governance:proposal:write";
const LEGACY_BROAD: &str = "governance:write";
const BOTH: &str = "governance:proposal:write governance:write";
const UNRELATED_READ: &str = "governance:read";
const SIBLING_CLASS: &str = "governance:charter:write";

const DECISION_V3_CLASS: &str = "governance_decision_v3";

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

// Capturing backend: v3 is persisted through the opaque seam
// (`put_governance_decision_v3` → `put_opaque`), so capturing opaque writes is
// how we read the emitted v3 receipt back. Mirrors the backend in
// `governance_decision_receipt_v3_emission.rs`, trimmed to what this slice needs.
#[derive(Default)]
struct CapturingBackend {
    opaque: Mutex<Vec<(String, Vec<u8>)>>, // (class, payload)
}

impl CapturingBackend {
    fn v3_receipts(&self) -> Vec<GovernanceDecisionReceiptV3> {
        self.opaque
            .lock()
            .unwrap()
            .iter()
            .filter(|(class, _)| class == DECISION_V3_CLASS)
            .map(|(_, payload)| serde_json::from_slice(payload).expect("opaque v3 deserializes"))
            .collect()
    }
}

impl GovernanceReceiptBackend for CapturingBackend {
    fn put_governance(&self, _: &GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn put_allocation(&self, _: &AllocationReceipt) -> Result<Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(&self, _: &Hash) -> Result<Vec<AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn put_opaque(
        &self,
        class: &str,
        _key1: &str,
        _key2: Option<&str>,
        _recorded_at: u64,
        _record_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<(), String> {
        self.opaque
            .lock()
            .unwrap()
            .push((class.to_string(), payload.to_vec()));
        Ok(())
    }
}

/// Seed a single-member domain with one open, For-voted Text proposal behind a
/// capturing backend, then drive the **HTTP** close route with `scope` as the
/// caller's token. Returns the close response status and any captured v3
/// receipts. The caller DID is both the domain member and the only voter, so a
/// single For vote crosses the 50/50 thresholds → Accepted → v3 emitted (when a
/// scope is matched).
async fn http_close_with_scope(
    scope: &'static str,
) -> (StatusCode, Vec<GovernanceDecisionReceiptV3>) {
    let backend = Arc::new(CapturingBackend::default());
    let member = fresh_did();

    let mgr = Arc::new(
        GovernanceManager::new()
            .with_receipt_store(backend.clone() as Arc<dyn GovernanceReceiptBackend>),
    );

    let domain = GovernanceDomainId("proposal-scope-a0".to_string());
    mgr.create_domain(
        domain.clone(),
        "Proposal Close Scope (A0)".to_string(),
        "cooperative_default".to_string(),
        GovernanceParams::new(50, 50, 3600),
        MembershipConfig::static_list(vec![member.clone()]),
    )
    .await
    .expect("create_domain");

    let proposal_id = ProposalId("proposal-scope-a0-prop".to_string());
    mgr.create_proposal(
        proposal_id.clone(),
        domain.clone(),
        member.clone(),
        "Endorse a fictional statement".to_string(),
        "A declarative text proposal — no execution closure.".to_string(),
        ProposalPayload::Text {
            body: "We endorse this fictional statement.".to_string(),
        },
        ProposalScope::Local,
    )
    .await
    .expect("create_proposal");
    mgr.open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");
    mgr.cast_vote(proposal_id.clone(), member.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");

    let ctx: GovernanceContext<NoopEventEmitter> = GovernanceContext {
        manager: mgr.clone(),
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

    let caller = member.to_string();
    let app = test::init_service(
        App::new()
            .wrap_fn(move |req, srv| {
                req.extensions_mut().insert(BasicClaims {
                    sub: caller.clone(),
                    scope: Some(scope.to_string()),
                });
                srv.call(req)
            })
            .configure(|cfg| http::configure(cfg, ctx)),
    )
    .await;

    let uri = format!("/proposals/{}/close", proposal_id.0);
    let req = test::TestRequest::post().uri(&uri).to_request();
    let status = test::call_service(&app, req).await.status();

    (status, backend.v3_receipts())
}

/// The emitted v3 must carry ProcessAuthorized, the Accepted outcome, the given
/// matched scope, and must verify. Shared assertion for the acceptance cases.
fn assert_v3_records_scope(v3s: &[GovernanceDecisionReceiptV3], matched_scope: &str) {
    assert_eq!(
        v3s.len(),
        1,
        "scoped normal close must emit exactly one v3 decision receipt"
    );
    let v3 = &v3s[0];
    assert_eq!(
        v3.capability_scope_presented, matched_scope,
        "v3 must record the scope the auth layer actually matched, not a fabricated/desired value"
    );
    assert_eq!(v3.outcome, ProofOutcome::Accepted);
    assert!(
        matches!(
            v3.mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ),
        "a normal democratic close is process-authorized, got {:?}",
        v3.mandate_attestation
    );
    assert!(v3.verify(), "emitted v3 receipt must verify");
}

// --- Authorization + honest-evidence: acceptance cases ---------------------

#[actix_web::test]
async fn close_accepts_and_records_proposal_class_scope() {
    let (status, v3s) = http_close_with_scope(PROPOSAL_CLASS).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "governance:proposal:write must authorize normal HTTP proposal close, got {status}"
    );
    assert_v3_records_scope(&v3s, PROPOSAL_CLASS);
}

#[actix_web::test]
async fn close_accepts_and_records_legacy_broad_scope() {
    // Accepted-also fallback: tokens minted before the migration still close,
    // and the receipt honestly records the broad scope that was actually used.
    let (status, v3s) = http_close_with_scope(LEGACY_BROAD).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "legacy governance:write must still authorize normal HTTP proposal close, got {status}"
    );
    assert_v3_records_scope(&v3s, LEGACY_BROAD);
}

#[actix_web::test]
async fn close_with_both_scopes_records_narrow_proposal_class() {
    // Both present → the helper prefers the narrowed class (listed first), so
    // the receipt records the sharper permission that was genuinely held.
    let (status, v3s) = http_close_with_scope(BOTH).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "both scopes present must authorize, got {status}"
    );
    assert_v3_records_scope(&v3s, PROPOSAL_CLASS);
}

// --- Authorization: rejection cases ----------------------------------------

#[actix_web::test]
async fn close_rejects_unrelated_read_scope() {
    let (status, v3s) = http_close_with_scope(UNRELATED_READ).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "governance:read must be rejected at the close gate, got {status}"
    );
    assert!(
        v3s.is_empty(),
        "a rejected close must emit no v3 (the close never proceeds)"
    );
}

#[actix_web::test]
async fn close_rejects_sibling_write_class_scope() {
    let (status, v3s) = http_close_with_scope(SIBLING_CLASS).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "sibling class {SIBLING_CLASS} must be rejected at the close gate, got {status}"
    );
    assert!(v3s.is_empty(), "a rejected close must emit no v3");
}

// ============================================================================
// #1868 A1c — remaining HTTP proposal-family handler gates.
//
// These paths emit no v3, so the gate uses `require_any_scope` (no matched-scope
// evidence). Auth-only coverage in the meeting_scope_migration.rs route-table
// style: acceptance = "not 401/403" (the gate passes; the downstream status
// reflects missing domain/proposal state — every optional member/steward/
// suspension checker is None in the test ctx, so the scope gate is the only
// 403 source); rejection = 403 at the gate, before any handler logic.
// ============================================================================

fn make_proposal_ctx() -> GovernanceContext<NoopEventEmitter> {
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

/// (method, path, optional JSON body) for each migrated proposal-family route.
/// Bodies are minimal-but-deserializable so the request reaches the in-handler
/// scope gate (actix extracts `web::Json` before the handler body runs).
fn proposal_family_routes() -> Vec<(&'static str, &'static str, Option<Value>)> {
    vec![
        (
            "POST",
            "/proposals",
            Some(json!({
                "domain_id": "test-domain",
                "title": "Scope migration proposal",
                "description": "A1c proposal-family auth test.",
                "payload": { "type": "text", "body": "endorse" }
            })),
        ),
        ("POST", "/proposals/test-proposal/open", Some(json!({}))),
        (
            "POST",
            "/proposals/test-proposal/vote",
            Some(json!({ "choice": "for" })),
        ),
        (
            "POST",
            "/delegations",
            Some(json!({ "delegate": "did:icn:delegate", "scope": "all" })),
        ),
        ("DELETE", "/delegations/test-delegation", None),
        (
            "POST",
            "/proposals/sdis/appoint-steward",
            Some(json!({
                "domain_id": "test-domain",
                "title": "Appoint steward",
                "description": "A1c proposal-family auth test.",
                "candidate": "did:icn:candidate",
                "region": "region-1",
                "bond_amount": 0,
                "term_length_seconds": 3600
            })),
        ),
        (
            "POST",
            "/proposals/sdis/remove-steward",
            Some(json!({
                "domain_id": "test-domain",
                "title": "Remove steward",
                "description": "A1c proposal-family auth test.",
                "steward": "did:icn:steward",
                "reason": "scope migration test"
            })),
        ),
    ]
}

async fn proposal_family_status(
    method: &str,
    path: &str,
    body: &Option<Value>,
    scope: &'static str,
) -> StatusCode {
    let ctx = make_proposal_ctx();
    let caller = fresh_did().to_string();
    let app = test::init_service(
        App::new()
            .wrap_fn(move |req, srv| {
                req.extensions_mut().insert(BasicClaims {
                    sub: caller.clone(),
                    scope: Some(scope.to_string()),
                });
                srv.call(req)
            })
            .configure(|cfg| http::configure(cfg, ctx)),
    )
    .await;

    let builder = match method {
        "POST" => test::TestRequest::post(),
        "DELETE" => test::TestRequest::delete(),
        other => panic!("unsupported method {other}"),
    };
    let builder = builder.uri(path);
    let req = match body {
        Some(b) => builder.set_json(b).to_request(),
        None => builder.to_request(),
    };
    test::call_service(&app, req).await.status()
}

fn assert_proposal_auth_accepted(status: StatusCode, scope: &str, method: &str, path: &str) {
    assert!(
        status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
        "{scope} must be accepted at {method} {path} (gate passes; {status} is the downstream \
         outcome, not an auth rejection)"
    );
}

#[actix_web::test]
async fn proposal_family_accepts_proposal_class_scope() {
    for (m, p, b) in proposal_family_routes() {
        let status = proposal_family_status(m, p, &b, PROPOSAL_CLASS).await;
        assert_proposal_auth_accepted(status, PROPOSAL_CLASS, m, p);
    }
}

#[actix_web::test]
async fn proposal_family_accepts_legacy_broad_scope() {
    // Accepted-also fallback: tokens minted before the migration still work.
    for (m, p, b) in proposal_family_routes() {
        let status = proposal_family_status(m, p, &b, LEGACY_BROAD).await;
        assert_proposal_auth_accepted(status, LEGACY_BROAD, m, p);
    }
}

#[actix_web::test]
async fn proposal_family_rejects_unrelated_read_scope() {
    for (m, p, b) in proposal_family_routes() {
        let status = proposal_family_status(m, p, &b, UNRELATED_READ).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "governance:read must be rejected at the {m} {p} gate, got {status}"
        );
    }
}

#[actix_web::test]
async fn proposal_family_rejects_sibling_write_class_scope() {
    for (m, p, b) in proposal_family_routes() {
        let status = proposal_family_status(m, p, &b, SIBLING_CLASS).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "sibling class {SIBLING_CLASS} must be rejected at the {m} {p} gate, got {status}"
        );
    }
}
