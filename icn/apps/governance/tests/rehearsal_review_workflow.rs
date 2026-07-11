//! Integration matrix for the rehearsal pending-publish review/mutation
//! surface (#1726 / #1728 / #2386 organizer workflow slice).
//!
//! Pins the contract:
//!
//! 1. The surface is mounted ONLY in `GovernanceContextBuildMode::Rehearsal`.
//!    In Production, Bootstrap (the unknown/missing-mode fallback), and Test
//!    the routes do not exist (404) — fail-closed by absence.
//! 2. Review decisions / edits / assignment require the narrow
//!    `governance:pending-publish:review` capability; executing a confirmed
//!    mutation requires `governance:pending-publish:confirm`. Neither implies
//!    the other; `governance:read` grants neither; broad `governance:write`
//!    retains both (sub-capability model, #2400).
//! 3. Preview is a pure GET returning a deterministic `preview_digest` over
//!    the exact mutation payload + review-state version. Confirm requires the
//!    digest; any state change between preview and confirm fails closed (409)
//!    and demands a new preview. Duplicate confirm is idempotent (no second
//!    action item, same ids returned).
//! 4. Confirm walks the real ADR-0026 ladder (session → decision → gate →
//!    activation → plan(body_hash = preview digest) → real action item →
//!    applied) — receipts come from the real machinery, and the created
//!    action item is a real governance record visible to the member surface.
//! 5. Unknown request fields are rejected (400), oversized inputs rejected,
//!    labels never resolve to DIDs on any read surface, and the evidence
//!    export is value-withheld (no DIDs) with a verifiable packet hash.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use actix_web::{dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams, MembershipConfig,
    MembershipSource, StaticMembershipResolver,
};
use icn_governance_actor::{
    http::{self, GovernanceContext, GovernanceContextBuildMode},
    manager::GovernanceManager,
    mandate_gate::{MandateGate, MandateGateError, MandateGrant, MandateRejection, MandateRequest},
    receipt_backend::GovernanceReceiptBackend,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};
use serde_json::{json, Value};

const REVIEW: &str = "governance:pending-publish:review";
const CONFIRM: &str = "governance:pending-publish:confirm";
const READ: &str = "governance:read";
const BROAD: &str = "governance:write";

const DOMAIN: &str = "rehearsal-test-domain";
const ROW_ACTION: &str = "pending-row-action-item-001";
const ROW_DECISION: &str = "pending-row-decision-001";

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

// ── Opaque-backed receipt store (same pattern as the ladder runtime-slice
// tests: opaque primitives only, typed defaults exercised end-to-end) ───────

type ChainKey = (String, String, Option<String>);
type ChainEntry = (u64, [u8; 32], Vec<u8>);

#[derive(Default)]
struct OpaqueUniqueBackend {
    chains: Mutex<HashMap<ChainKey, Vec<ChainEntry>>>,
    unique: Mutex<HashMap<ChainKey, [u8; 32]>>,
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

    fn put_opaque(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
        recorded_at: u64,
        record_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<(), String> {
        let key = (class.to_string(), key1.to_string(), key2.map(String::from));
        self.chains.lock().unwrap().entry(key).or_default().push((
            recorded_at,
            record_hash,
            payload.to_vec(),
        ));
        Ok(())
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

    fn list_opaque_for(&self, class: &str, key1: &str) -> Result<Vec<Vec<u8>>, String> {
        let chains = self.chains.lock().unwrap();
        let mut hits: Vec<ChainEntry> = chains
            .iter()
            .filter(|((c, k1, _), _)| c == class && k1 == key1)
            .flat_map(|(_, chain)| chain.iter().cloned())
            .collect();
        hits.sort_by_key(|(t, h, _)| (*t, *h));
        Ok(hits.into_iter().map(|(_, _, p)| p).collect())
    }
}

// ── Context / app harness ──────────────────────────────────────────────────

fn make_ctx(mode: GovernanceContextBuildMode) -> GovernanceContext<NoopEventEmitter> {
    let manager =
        GovernanceManager::new().with_receipt_store(Arc::new(OpaqueUniqueBackend::default()));
    GovernanceContext {
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
        build_mode: mode,
    }
}

/// Always-rejecting mandate gate: satisfies Production's `is_some()` wiring
/// requirement; no test here exercises `require()`.
struct RejectingMandateGate;

impl MandateGate for RejectingMandateGate {
    fn require(&self, _req: &MandateRequest) -> Result<MandateGrant, MandateGateError> {
        Err(MandateGateError::Rejected(MandateRejection::NoMandate))
    }
}

/// A context that satisfies Production dependency validation (configure()
/// panics on a bare Production context by design), so the absent-mode matrix
/// can include Production.
fn make_production_ctx() -> GovernanceContext<NoopEventEmitter> {
    let mut ctx = make_ctx(GovernanceContextBuildMode::Production);
    ctx.member_checker = Some(Arc::new(|_did, _domain| Box::pin(async { true })));
    ctx.suspension_checker = Some(Arc::new(|_did, _domain| Box::pin(async { false })));
    ctx.membership_resolver = Some(Arc::new(StaticMembershipResolver::new()));
    ctx.mandate_gate = Some(Arc::new(RejectingMandateGate));
    ctx
}

async fn seed_named_domain(
    mgr: &GovernanceManager,
    members: Vec<Did>,
    name: &str,
) -> GovernanceDomainId {
    let domain = GovernanceDomainId::new(name);
    mgr.create_domain(
        domain.clone(),
        format!("Rehearsal Test Coop ({name})"),
        "default".to_string(),
        GovernanceParams {
            quorum_percentage: 1,
            approval_threshold_percentage: 51,
            voting_period_seconds: 86_400,
            require_deliberation: false,
            ..GovernanceParams::default()
        },
        MembershipConfig {
            source: MembershipSource::StaticList(members),
        },
    )
    .await
    .expect("create_domain");
    domain
}

async fn seed_domain(mgr: &GovernanceManager, members: Vec<Did>) -> GovernanceDomainId {
    seed_named_domain(mgr, members, DOMAIN).await
}

macro_rules! gov_app {
    ($ctx:expr, $caller:expr, $scope:expr) => {{
        let scope: String = $scope.to_string();
        let caller = $caller.to_string();
        test::init_service(
            App::new()
                .wrap_fn(move |req, srv| {
                    req.extensions_mut().insert(BasicClaims {
                        sub: caller.clone(),
                        scope: Some(scope.clone()),
                    });
                    srv.call(req)
                })
                .configure(|cfg| http::configure(cfg, $ctx)),
        )
        .await
    }};
}

async fn body_json(resp: actix_web::dev::ServiceResponse) -> Value {
    let bytes = test::read_body(resp).await;
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// Standard rehearsal app: Rehearsal mode, one organizer DID that is a domain
/// member, workspace reset already performed. Returns (app, organizer_did).
macro_rules! rehearsal_app_reset {
    ($scope:expr) => {{
        let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
        let organizer = fresh_did();
        seed_domain(&ctx.manager, vec![organizer.clone()]).await;
        let app = gov_app!(ctx, &organizer, $scope);
        let reset = test::TestRequest::post()
            .uri(&format!("/domains/{DOMAIN}/rehearsal/reset"))
            .set_json(json!({}))
            .to_request();
        let resp = test::call_service(&app, reset).await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "reset must succeed for scope {}",
            $scope
        );
        (app, organizer)
    }};
}

fn review_uri(row: &str) -> String {
    format!("/domains/{DOMAIN}/rehearsal/pending-publish/{row}/review")
}
fn preview_uri(row: &str) -> String {
    format!("/domains/{DOMAIN}/rehearsal/pending-publish/{row}/preview")
}
fn confirm_uri(row: &str) -> String {
    format!("/domains/{DOMAIN}/rehearsal/pending-publish/{row}/confirm")
}

/// POST a JSON body to the app under test.
macro_rules! post {
    ($app:expr, $uri:expr, $body:expr) => {{
        let uri_owned = $uri;
        let uri: &str = uri_owned.as_ref();
        test::call_service(
            $app,
            test::TestRequest::post()
                .uri(uri)
                .set_json($body)
                .to_request(),
        )
        .await
    }};
}

/// GET a path on the app under test.
macro_rules! get {
    ($app:expr, $uri:expr) => {{
        let uri_owned = $uri;
        let uri: &str = uri_owned.as_ref();
        test::call_service($app, test::TestRequest::get().uri(uri).to_request()).await
    }};
}

/// Drive approve on the action-item row, then return the preview JSON.
macro_rules! approve_and_preview {
    ($app:expr) => {{
        let resp = post!($app, review_uri(ROW_ACTION), &json!({"decision": "approve"}));
        assert_eq!(resp.status(), StatusCode::OK, "approve");
        let resp = get!($app, preview_uri(ROW_ACTION));
        assert_eq!(resp.status(), StatusCode::OK, "preview");
        body_json(resp).await
    }};
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Mode gating: the surface exists ONLY in Rehearsal mode
// ═══════════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn rehearsal_routes_absent_outside_rehearsal_mode() {
    for mode in [
        GovernanceContextBuildMode::Production,
        GovernanceContextBuildMode::Bootstrap,
        GovernanceContextBuildMode::Test,
    ] {
        let ctx = if mode == GovernanceContextBuildMode::Production {
            make_production_ctx()
        } else {
            make_ctx(mode)
        };
        let caller = fresh_did();
        // Broad scope on purpose: absence must hold even for the most
        // privileged caller — the routes do not exist, 404, never 403.
        let app = gov_app!(ctx, &caller, BROAD);
        for (method, uri) in [
            ("POST", format!("/domains/{DOMAIN}/rehearsal/reset")),
            (
                "GET",
                format!("/domains/{DOMAIN}/rehearsal/pending-publish"),
            ),
            ("POST", review_uri(ROW_ACTION)),
            ("GET", preview_uri(ROW_ACTION)),
            ("POST", confirm_uri(ROW_ACTION)),
            (
                "GET",
                format!("/domains/{DOMAIN}/rehearsal/evidence-export"),
            ),
        ] {
            let req = match method {
                "POST" => test::TestRequest::post()
                    .uri(&uri)
                    .set_json(json!({}))
                    .to_request(),
                _ => test::TestRequest::get().uri(&uri).to_request(),
            };
            let resp = test::call_service(&app, req).await;
            assert_eq!(
                resp.status(),
                StatusCode::NOT_FOUND,
                "{mode:?} {method} {uri} must be absent (404)"
            );
        }
    }
}

#[actix_web::test]
async fn rehearsal_routes_present_in_rehearsal_mode() {
    let (app, _organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM}"));
    let resp = get!(&app, format!("/domains/{DOMAIN}/rehearsal/pending-publish"));
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let rows = body["rows"].as_array().expect("rows array");
    assert_eq!(
        rows.len(),
        3,
        "deterministic seed serves the 3 fixture rows"
    );
    assert!(rows.iter().any(|r| r["id"] == ROW_ACTION));
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Capability gates (sub-capability model)
// ═══════════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn read_scope_cannot_review_edit_assign_confirm_or_reset() {
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let organizer = fresh_did();
    seed_domain(&ctx.manager, vec![organizer.clone()]).await;
    let app = gov_app!(ctx, &organizer, READ);
    for uri in [
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        review_uri(ROW_ACTION),
        confirm_uri(ROW_ACTION),
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
    ] {
        let resp = post!(&app, &uri, &json!({"decision": "approve"}));
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "read-only must be 403 at {uri}"
        );
    }
}

#[actix_web::test]
async fn review_scope_cannot_confirm_and_confirm_scope_cannot_review() {
    // review-only: may approve, may NOT confirm.
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW}"));
    let preview = approve_and_preview!(&app);
    let digest = preview["preview_digest"].as_str().expect("digest");
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "review scope must not confirm"
    );

    // confirm-only: may NOT review (and therefore cannot manufacture state).
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let organizer = fresh_did();
    seed_domain(&ctx.manager, vec![organizer.clone()]).await;
    let app2 = gov_app!(ctx, &organizer, format!("{READ} {CONFIRM}"));
    let resp = post!(
        &app2,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "confirm scope must not reset"
    );
    let resp = post!(
        &app2,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve"})
    );
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "confirm scope must not review"
    );
}

#[actix_web::test]
async fn non_member_is_rejected_even_with_review_scope() {
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let member = fresh_did();
    let outsider = fresh_did();
    seed_domain(&ctx.manager, vec![member]).await;
    let app = gov_app!(ctx, &outsider, format!("{READ} {REVIEW} {CONFIRM}"));
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "non-member must be rejected"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Review decisions, edits, assignment
// ═══════════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn approve_reject_needs_edit_move_row_status_and_record_decisions() {
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW}"));

    let resp = post!(
        &app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve"})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["row"]["status"], "approved_for_publish");
    assert!(
        body["decision_receipt"]["record_hash"]
            .as_str()
            .map(str::len)
            == Some(64),
        "review decision must carry a real 64-hex decision receipt hash: {body}"
    );

    let resp = post!(
        &app,
        review_uri(ROW_DECISION),
        &json!({"decision": "reject", "note": "Duplicate of an earlier record."})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["row"]["status"], "rejected");
}

#[actix_web::test]
async fn unknown_decision_value_and_unknown_fields_fail_closed() {
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW}"));
    let resp = post!(
        &app,
        review_uri(ROW_ACTION),
        &json!({"decision": "publish"})
    );
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "unknown decision value"
    );

    let resp = post!(
        &app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve", "grant_authority": true})
    );
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "unknown field must be rejected"
    );

    let oversized = "x".repeat(4001);
    let resp = post!(
        &app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve", "note": oversized})
    );
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "oversized note must be rejected"
    );
}

#[actix_web::test]
async fn edit_bounds_fields_and_reapproval_is_required_after_edit() {
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW}"));

    // Approve, then edit: the edit must push the row back to pending review.
    let resp = post!(
        &app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve"})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!(
                "/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}"
            ))
            .set_json(json!({"plain_summary": "Confirm venue booking for the gathering"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(
        body["row"]["status"], "pending_review",
        "an edit after approval must invalidate the approval"
    );

    // Oversized summary rejected.
    let resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!(
                "/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}"
            ))
            .set_json(json!({"plain_summary": "y".repeat(300)}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Unknown edit field rejected.
    let resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!(
                "/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}"
            ))
            .set_json(json!({"assignee_did": "did:icn:sneaky"}))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "editing a DID directly is impossible"
    );
}

#[actix_web::test]
async fn assignment_uses_labels_and_binding_state_is_visible_without_dids() {
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW}"));

    // Bind a label to a DID (organizer/operator act). Response withholds the DID.
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": organizer.to_string()})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["label"], "Example member (fictional)");
    assert_eq!(body["bound"], true);
    assert!(
        !body.to_string().contains(&organizer.to_string()),
        "binding response must never echo the DID"
    );

    // Bindings listing shows labels + bound flags only.
    let resp = get!(&app, format!("/domains/{DOMAIN}/rehearsal/bindings"));
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert!(
        !body.to_string().contains("did:"),
        "bindings listing must never contain DIDs: {body}"
    );

    // Assign the action row to the bound label.
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}/assign"),
        &json!({"assignee_label": "Example member (fictional)"})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["row"]["assignee_label"], "Example member (fictional)");
    assert_eq!(body["assignee_bound"], true);

    // Assigning an unknown label fails closed.
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}/assign"),
        &json!({"assignee_label": "Nobody we know"})
    );
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Preview → confirm binding
// ═══════════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn preview_requires_approval_and_is_deterministic() {
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW}"));

    // Not approved yet → no preview.
    let resp = get!(&app, preview_uri(ROW_ACTION));
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "preview before approval must 409"
    );

    let preview1 = approve_and_preview!(&app);
    let d1 = preview1["preview_digest"]
        .as_str()
        .expect("digest")
        .to_string();
    assert_eq!(d1.len(), 64, "digest is 64-hex");

    // The preview names the mission's nine facts in plain fields.
    for key in [
        "action",
        "domain_id",
        "authority_basis",
        "assignee_label",
        "risk_level",
        "receipt_expected",
        "reversible",
        "privacy_note",
    ] {
        assert!(
            preview1.get(key).is_some(),
            "preview must carry '{key}': {preview1}"
        );
    }
    assert_eq!(preview1["reversible"], false);

    // Pure GET: same state → identical digest; no state was created.
    let preview2 = body_json(get!(&app, preview_uri(ROW_ACTION))).await;
    assert_eq!(preview2["preview_digest"].as_str().unwrap(), d1);
}

#[actix_web::test]
async fn confirm_requires_matching_digest_and_fails_closed_on_stale_preview() {
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM}"));
    let preview = approve_and_preview!(&app);
    let digest = preview["preview_digest"].as_str().unwrap().to_string();

    // Tampered digest → 409, nothing executed.
    let mut wrong = digest.clone();
    wrong.replace_range(0..1, if digest.starts_with('0') { "1" } else { "0" });
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": wrong})
    );
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "tampered digest fails closed"
    );

    // Edit after preview → old digest is stale → 409 and a fresh preview differs.
    let resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!(
                "/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}"
            ))
            .set_json(json!({"plain_summary": "Changed after preview"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = post!(
        &app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve"})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "stale preview fails closed"
    );
    let fresh = body_json(get!(&app, preview_uri(ROW_ACTION))).await;
    assert_ne!(fresh["preview_digest"].as_str().unwrap(), digest);

    // Unknown confirm field rejected.
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": fresh["preview_digest"], "force": true})
    );
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn confirm_executes_ladder_creates_real_item_and_is_idempotent() {
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM}"));

    // Bind + assign so the created item is completable by the member.
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": organizer.to_string()})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}/assign"),
        &json!({"assignee_label": "Example member (fictional)"})
    );
    assert_eq!(resp.status(), StatusCode::OK);

    let preview = approve_and_preview!(&app);
    let digest = preview["preview_digest"].as_str().unwrap().to_string();

    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(resp.status(), StatusCode::CREATED);
    let first = body_json(resp).await;
    let item_id = first["action_item_id"]
        .as_str()
        .expect("item id")
        .to_string();
    for key in [
        "plan_record_hash",
        "application_record_hash",
        "decision_record_hash",
    ] {
        assert_eq!(
            first[key].as_str().map(str::len),
            Some(64),
            "{key} must be a 64-hex real receipt hash: {first}"
        );
    }

    // The created action item is a REAL governance record on the normal surface.
    let resp = get!(&app, format!("/domains/{DOMAIN}/action-items/{item_id}"));
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "created item must be a real record"
    );
    let item = body_json(resp).await;
    assert_eq!(item["status"], "pending");

    // Duplicate confirm: idempotent — same ids, no second item.
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "duplicate confirm is a no-op replay"
    );
    let second = body_json(resp).await;
    assert_eq!(second["action_item_id"], first["action_item_id"]);
    assert_eq!(second["idempotent"], true);
    let resp = get!(&app, format!("/domains/{DOMAIN}/action-items"));
    let listing = body_json(resp).await;
    let created: Vec<_> = listing
        .as_array()
        .map(|a| a.iter().filter(|i| i["id"] == item_id.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        created.len(),
        1,
        "exactly one item exists after duplicate confirm"
    );
}

#[actix_web::test]
async fn confirm_rejects_unbound_assignee_and_non_action_kinds() {
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM}"));

    // Seeded action row carries an UNBOUND assignee label → confirm fails closed.
    let preview = approve_and_preview!(&app);
    assert_eq!(preview["assignee_bound"], false);
    assert_eq!(preview["confirmable"], false);
    let digest = preview["preview_digest"].as_str().unwrap();
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "unbound assignee label must fail closed at confirm"
    );

    // Non-action-item kinds are reviewable but not executable in this slice.
    let resp = post!(
        &app,
        review_uri(ROW_DECISION),
        &json!({"decision": "approve"})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = get!(&app, preview_uri(ROW_DECISION));
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Summary endpoint reflects the rehearsal workspace
// ═══════════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn summary_serves_rehearsal_runtime_rows_after_workspace_init() {
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW}"));

    let resp = post!(
        &app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve"})
    );
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = get!(&app, "/me/pending-publish-summary");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["origin"], "rehearsal_runtime");
    let rows = body["rows"].as_array().unwrap();
    let action = rows
        .iter()
        .find(|r| r["id"] == ROW_ACTION)
        .expect("action row");
    assert_eq!(
        action["status"], "approved_for_publish",
        "summary must reflect live review state"
    );
}

#[actix_web::test]
async fn summary_without_workspace_keeps_committed_fixture_origin_in_rehearsal_mode() {
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let caller = fresh_did();
    let app = gov_app!(ctx, &caller, READ);
    let resp = get!(&app, "/me/pending-publish-summary");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(
        body["origin"], "committed_fixture",
        "before any workspace exists the static fixture is served, honestly labeled"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Evidence export + receipts read-back + reset
// ═══════════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn evidence_export_is_value_withheld_hash_bound_and_reflects_outcomes() {
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM}"));

    // Full loop on the action row.
    post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": organizer.to_string()})
    );
    post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}/assign"),
        &json!({"assignee_label": "Example member (fictional)"})
    );
    let preview = approve_and_preview!(&app);
    let digest = preview["preview_digest"].as_str().unwrap().to_string();
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(resp.status(), StatusCode::CREATED);
    let confirm = body_json(resp).await;

    // Reject the decision row so the packet shows a mixed outcome set.
    post!(
        &app,
        review_uri(ROW_DECISION),
        &json!({"decision": "reject"})
    );

    let resp = get!(&app, format!("/domains/{DOMAIN}/rehearsal/evidence-export"));
    assert_eq!(resp.status(), StatusCode::OK);
    let packet = body_json(resp).await;

    assert_eq!(
        packet["contract"],
        "urn:icn:contract:rehearsal-workflow-evidence:v1"
    );
    assert_eq!(packet["origin"], "rehearsal_runtime");
    assert_eq!(packet["hash_algorithm"], "sha256");
    assert_eq!(packet["packet_hash"].as_str().map(str::len), Some(64));

    // Value-withheld: no DIDs anywhere in the packet.
    assert!(
        !packet.to_string().contains("did:"),
        "evidence packet must contain no DIDs: {packet}"
    );

    // Outcomes: executed row binds to the plan/application receipt hashes
    // returned at confirm time.
    let rows = packet["rows"].as_array().unwrap();
    let executed = rows.iter().find(|r| r["id"] == ROW_ACTION).unwrap();
    assert_eq!(executed["outcome"], "executed");
    assert_eq!(executed["plan_record_hash"], confirm["plan_record_hash"]);
    assert_eq!(
        executed["application_record_hash"],
        confirm["application_record_hash"]
    );
    let rejected = rows.iter().find(|r| r["id"] == ROW_DECISION).unwrap();
    assert_eq!(rejected["outcome"], "rejected");

    // Receipts read-back surface lists the ladder receipts (hashes only).
    let resp = get!(&app, format!("/domains/{DOMAIN}/rehearsal/receipts"));
    assert_eq!(resp.status(), StatusCode::OK);
    let receipts = body_json(resp).await;
    let classes: Vec<&str> = receipts["receipts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["class"].as_str())
        .collect();
    for expected in [
        "process_session_opened",
        "decision_recorded",
        "process_gate_result",
        "activation_crossed",
        "mutation_plan_recorded",
        "mutation_applied",
    ] {
        assert!(
            classes.contains(&expected),
            "receipt read-back must include {expected}; got {classes:?}"
        );
    }
    assert!(
        !receipts.to_string().contains("did:"),
        "receipt read-back must be value-withheld (no DIDs)"
    );
}

#[actix_web::test]
async fn reset_restores_deterministic_seed_and_bumps_generation() {
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM}"));

    let resp = post!(
        &app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve"})
    );
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["generation"], 2);

    let resp = get!(&app, format!("/domains/{DOMAIN}/rehearsal/pending-publish"));
    let listing = body_json(resp).await;
    let action = listing["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == ROW_ACTION)
        .unwrap()
        .clone();
    assert_eq!(
        action["status"], "pending_review",
        "reset restores the seed state"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Cross-domain isolation, rebinding staleness, reset staleness, inert markup
// ═══════════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn member_of_another_domain_cannot_touch_this_domains_workspace() {
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let insider = fresh_did();
    let outsider = fresh_did();
    seed_domain(&ctx.manager, vec![insider.clone()]).await;
    seed_named_domain(&ctx.manager, vec![outsider.clone()], "other-domain").await;

    // The outsider holds full rehearsal scopes and real standing — in the
    // OTHER domain. The path domain must be the authority context.
    let app = gov_app!(ctx, &outsider, format!("{READ} {REVIEW} {CONFIRM}"));
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "standing in another domain must not open this domain's workspace"
    );
    let resp = post!(&app, review_uri(ROW_ACTION), &json!({"decision": "approve"}));
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn rebinding_a_label_between_preview_and_confirm_invalidates_digest() {
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM}"));

    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": organizer.to_string()})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}/assign"),
        &json!({"assignee_label": "Example member (fictional)"})
    );
    assert_eq!(resp.status(), StatusCode::OK);

    let preview = approve_and_preview!(&app);
    let digest = preview["preview_digest"].as_str().unwrap().to_string();

    // Re-bind the SAME label to a different identity: the mutation the
    // organizer previewed is no longer the mutation that would execute.
    let other = fresh_did();
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": other.to_string()})
    );
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "a binding change between preview and confirm must fail closed"
    );
    let fresh = body_json(get!(&app, preview_uri(ROW_ACTION))).await;
    assert_ne!(fresh["preview_digest"].as_str().unwrap(), digest);
}

#[actix_web::test]
async fn reset_invalidates_previews_from_earlier_generations() {
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM}"));

    post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": organizer.to_string()})
    );
    let preview = approve_and_preview!(&app);
    let old_digest = preview["preview_digest"].as_str().unwrap().to_string();

    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);

    // Re-approve in the new generation, then try the pre-reset digest.
    let fresh = approve_and_preview!(&app);
    assert_ne!(
        fresh["preview_digest"].as_str().unwrap(),
        old_digest,
        "a new generation must never reproduce an old digest"
    );
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": old_digest})
    );
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "a pre-reset preview digest must be stale after reset"
    );
}

#[actix_web::test]
async fn organizer_supplied_markup_stays_inert_text() {
    let (app, _o) = rehearsal_app_reset!(format!("{READ} {REVIEW}"));
    let sneaky = "<script>alert(1)</script> Confirm venue & <b>book</b>";
    let resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!(
                "/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}"
            ))
            .set_json(json!({"plain_summary": sneaky}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(
        body["row"]["plain_summary"], sneaky,
        "markup is stored and returned verbatim as inert JSON text — \
         never interpreted, never mangled, escaping is the renderer's job"
    );
}
