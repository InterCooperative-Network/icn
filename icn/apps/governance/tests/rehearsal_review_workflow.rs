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
    ActionItem, ActionItemFilter, ActionItemId, GovernanceDecisionReceipt, GovernanceDomain,
    GovernanceDomainId, GovernanceParams, InMemoryActionItemStore, MembershipConfig,
    MembershipResolver, MembershipSource, StaticMembershipResolver,
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
const SETUP: &str = "governance:rehearsal:setup";
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
    /// Failure injection: `put_opaque_if_absent` fails for these classes.
    fail_classes: Mutex<std::collections::HashSet<String>>,
}

impl OpaqueUniqueBackend {
    fn fail_class(&self, class: &str) {
        self.fail_classes.lock().unwrap().insert(class.to_string());
    }
    fn clear_failures(&self) {
        self.fail_classes.lock().unwrap().clear();
    }
    /// Total persisted entries of one receipt class (append-only chains +
    /// unique inserts both land in `chains`).
    fn count_entries(&self, class: &str) -> usize {
        self.chains
            .lock()
            .unwrap()
            .iter()
            .filter(|((c, _, _), _)| c == class)
            .map(|(_, chain)| chain.len())
            .sum()
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
        if self.fail_classes.lock().unwrap().contains(class) {
            return Err(format!("injected failure for class {class}"));
        }
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

fn make_ctx_with_backend(
    mode: GovernanceContextBuildMode,
) -> (
    GovernanceContext<NoopEventEmitter>,
    Arc<OpaqueUniqueBackend>,
) {
    let backend = Arc::new(OpaqueUniqueBackend::default());
    let ctx = make_ctx_on_backend(mode, backend.clone());
    (ctx, backend)
}

fn make_ctx(mode: GovernanceContextBuildMode) -> GovernanceContext<NoopEventEmitter> {
    make_ctx_with_backend(mode).0
}

/// Share one in-memory action-item store across "restarted" managers, the
/// way the gateway's Sled store survives a daemon restart.
struct SharedItemStore(Arc<InMemoryActionItemStore>);

impl icn_governance::ActionItemStoreBackend for SharedItemStore {
    fn save(&self, item: &ActionItem) -> icn_governance::Result<()> {
        self.0.save(item)
    }
    fn get(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
    ) -> icn_governance::Result<Option<ActionItem>> {
        self.0.get(domain_id, id)
    }
    fn list(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> icn_governance::Result<Vec<ActionItem>> {
        self.0.list(domain_id, filter)
    }
    fn delete(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
    ) -> icn_governance::Result<bool> {
        self.0.delete(domain_id, id)
    }
    fn count(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> icn_governance::Result<usize> {
        self.0.count(domain_id, filter)
    }
    fn delete_all(&self, domain_id: &GovernanceDomainId) -> icn_governance::Result<usize> {
        self.0.delete_all(domain_id)
    }
}

/// Like [`make_ctx_on_backend`], but with a durable (shared) action-item
/// store, so a "restart" test can observe items persisted by a prior manager.
fn make_ctx_on_backend_with_items(
    mode: GovernanceContextBuildMode,
    backend: Arc<OpaqueUniqueBackend>,
    items: Arc<InMemoryActionItemStore>,
) -> GovernanceContext<NoopEventEmitter> {
    let mut ctx = make_ctx_on_backend(mode, backend.clone());
    let mut manager = GovernanceManager::new().with_receipt_store(backend);
    manager.set_action_item_store(Box::new(SharedItemStore(items)));
    ctx.manager = Arc::new(manager);
    ctx
}

fn make_ctx_on_backend(
    mode: GovernanceContextBuildMode,
    backend: Arc<OpaqueUniqueBackend>,
) -> GovernanceContext<NoopEventEmitter> {
    let manager = GovernanceManager::new().with_receipt_store(backend);
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

/// A `TrustThreshold` domain: membership is NOT enumerable from the source;
/// standing exists only if a wired resolver affirms it.
async fn seed_trust_domain(mgr: &GovernanceManager, name: &str) -> GovernanceDomainId {
    let domain = GovernanceDomainId::new(name);
    mgr.create_domain(
        domain.clone(),
        format!("Rehearsal Trust Coop ({name})"),
        "default".to_string(),
        GovernanceParams {
            quorum_percentage: 1,
            approval_threshold_percentage: 51,
            voting_period_seconds: 86_400,
            require_deliberation: false,
            ..GovernanceParams::default()
        },
        MembershipConfig {
            source: MembershipSource::TrustThreshold(0.5),
        },
    )
    .await
    .expect("create_domain");
    domain
}

/// Test resolver with a fixed member set, optionally erroring for one DID
/// (an INDETERMINATE standing, distinct from a provable non-member).
struct FixedResolver {
    members: Vec<Did>,
    error_for: Option<Did>,
}

impl MembershipResolver for FixedResolver {
    fn resolve_members(&self, _domain: &GovernanceDomain) -> anyhow::Result<Vec<Did>> {
        Ok(self.members.clone())
    }
    fn is_member(&self, _domain: &GovernanceDomain, did: &Did) -> anyhow::Result<bool> {
        if self.error_for.as_ref() == Some(did) {
            anyhow::bail!("trust graph unavailable for this identity");
        }
        Ok(self.members.contains(did))
    }
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
        // First initialization (designation) is a SETUP act; the app under
        // test then carries only the scopes the test declares.
        let setup_app = gov_app!(ctx.clone(), &organizer, format!("{READ} {SETUP}"));
        let reset = test::TestRequest::post()
            .uri(&format!("/domains/{DOMAIN}/rehearsal/reset"))
            .set_json(json!({}))
            .to_request();
        let resp = test::call_service(&setup_app, reset).await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "designation reset must succeed"
        );
        let app = gov_app!(ctx, &organizer, $scope);
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
    // Each request carries a schema-valid body for its route, so the
    // rejection under test is the capability gate (403), not body parsing —
    // actix extractors run before handler bodies by repo convention.
    for (uri, body) in [
        (format!("/domains/{DOMAIN}/rehearsal/reset"), json!({})),
        (review_uri(ROW_ACTION), json!({"decision": "approve"})),
        (
            confirm_uri(ROW_ACTION),
            json!({"preview_digest": "0".repeat(64)}),
        ),
        (
            format!("/domains/{DOMAIN}/rehearsal/bindings"),
            json!({"label": "Example member (fictional)", "did": "did:icn:example-not-live"}),
        ),
    ] {
        let resp = post!(&app, &uri, &body);
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
    let app = gov_app!(ctx, &outsider, format!("{READ} {REVIEW} {CONFIRM} {SETUP}"));
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
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {SETUP}"));

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
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM} {SETUP}"));

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
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM} {SETUP}"));

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
    assert_eq!(packet["packet_hash"].as_str().map(str::len), Some(64));
    assert_eq!(
        packet["packet_hash_sha256"].as_str().map(str::len),
        Some(64)
    );

    // The reusable validator accepts the pristine packet and rejects EVERY
    // semantically meaningful tamper — including generated_at, which is
    // bound by the hashes like every other exported field.
    use icn_governance_actor::rehearsal_workspace::validate_evidence_packet;
    validate_evidence_packet(&packet).expect("pristine packet must validate");
    let tamper = |mutate: &dyn Fn(&mut Value)| {
        let mut copy = packet.clone();
        mutate(&mut copy);
        assert!(
            validate_evidence_packet(&copy).is_err(),
            "tampered packet must fail validation"
        );
    };
    tamper(&|p| p["rows"][0]["outcome"] = json!("deferred"));
    tamper(&|p| p["decisions"][0]["record_hash"] = json!("00".repeat(32)));
    tamper(&|p| p["generation"] = json!(999));
    tamper(&|p| p["generated_at"] = json!(0));
    tamper(&|p| p["non_claims"] = json!([]));
    tamper(&|p| p["run_id"] = json!("forged"));

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
    let app = gov_app!(ctx, &outsider, format!("{READ} {REVIEW} {CONFIRM} {SETUP}"));
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
    let resp = post!(
        &app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve"})
    );
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn rebinding_a_label_between_preview_and_confirm_invalidates_digest() {
    // Both identities are domain members (a binding target must hold
    // standing), so the rebinding itself is legal — what must fail closed is
    // the STALE PREVIEW after it.
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let organizer = fresh_did();
    let other = fresh_did();
    seed_domain(&ctx.manager, vec![organizer.clone(), other.clone()]).await;
    let app = gov_app!(
        ctx,
        &organizer,
        format!("{READ} {REVIEW} {CONFIRM} {SETUP}")
    );
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);

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
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM} {SETUP}"));

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

// ═══════════════════════════════════════════════════════════════════════════
// 8. Design-audit regressions: capability separation, summary isolation,
//    restart safety, confirm atomicity, reset retirement, broad fallback
// ═══════════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn review_scope_cannot_designate_but_can_re_reset() {
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let organizer = fresh_did();
    seed_domain(&ctx.manager, vec![organizer.clone()]).await;

    // Designation (first reset) is a setup act: a review-scoped member
    // cannot turn an arbitrary domain into a rehearsal workspace.
    let review_app = gov_app!(ctx.clone(), &organizer, format!("{READ} {REVIEW}"));
    let resp = post!(
        &review_app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "review scope must not designate a rehearsal domain"
    );

    // Setup designates; the organizer may then repeat the rehearsal.
    let setup_app = gov_app!(ctx.clone(), &organizer, format!("{READ} {SETUP}"));
    let resp = post!(
        &setup_app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "setup designates the workspace"
    );
    let resp = post!(
        &review_app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "review scope re-resets an already-designated workspace"
    );
}

#[actix_web::test]
async fn organizer_shape_cannot_bind_and_setup_cannot_review_or_confirm() {
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM}"));

    // The organizer browser shape never binds identities.
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": organizer.to_string()})
    );
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "organizer credential must not bind identities"
    );

    // A setup-only credential can bind a MEMBER identity but cannot review
    // or confirm anything.
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let operator = fresh_did();
    seed_domain(&ctx.manager, vec![operator.clone()]).await;
    let setup_app = gov_app!(ctx, &operator, format!("{READ} {SETUP}"));
    let resp = post!(
        &setup_app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = post!(
        &setup_app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": operator.to_string()})
    );
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "setup binds a member identity"
    );
    let resp = post!(
        &setup_app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve"})
    );
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "setup must not review"
    );
    let resp = post!(
        &setup_app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": "0".repeat(64)})
    );
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "setup must not confirm"
    );
}

#[actix_web::test]
async fn binding_a_non_member_identity_fails_closed() {
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let operator = fresh_did();
    let stranger = fresh_did();
    seed_domain(&ctx.manager, vec![operator.clone()]).await;
    let setup_app = gov_app!(ctx, &operator, format!("{READ} {SETUP}"));
    let resp = post!(
        &setup_app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = post!(
        &setup_app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": stranger.to_string()})
    );
    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "a binding target must already hold domain membership"
    );
}

#[actix_web::test]
async fn summary_is_isolated_to_the_callers_own_domains() {
    // Two domains, two members. A workspace exists ONLY in DOMAIN (member:
    // insider). The outsider — a member of other-domain only — must not
    // observe DOMAIN's rows, review state, or even that a workspace exists.
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let insider = fresh_did();
    let outsider = fresh_did();
    seed_domain(&ctx.manager, vec![insider.clone()]).await;
    seed_named_domain(&ctx.manager, vec![outsider.clone()], "other-domain").await;

    let insider_app = gov_app!(ctx.clone(), &insider, format!("{READ} {REVIEW} {SETUP}"));
    let resp = post!(
        &insider_app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = post!(
        &insider_app,
        review_uri(ROW_ACTION),
        &json!({"decision": "approve"})
    );
    assert_eq!(resp.status(), StatusCode::OK);

    // Insider sees their live workspace.
    let body = body_json(get!(&insider_app, "/me/pending-publish-summary")).await;
    assert_eq!(body["origin"], "rehearsal_runtime");

    // Outsider gets the static fixture — indistinguishable from "no
    // workspace exists anywhere", with no approved row state leaking.
    let outsider_app = gov_app!(ctx, &outsider, format!("{READ} {REVIEW}"));
    let body = body_json(get!(&outsider_app, "/me/pending-publish-summary")).await;
    assert_eq!(
        body["origin"], "committed_fixture",
        "another domain's workspace must be unobservable"
    );
    let rows = body["rows"].as_array().unwrap();
    let action = rows.iter().find(|r| r["id"] == ROW_ACTION).unwrap();
    assert_eq!(
        action["status"], "pending_review",
        "review state from a foreign domain must not leak into the fixture view"
    );
}

#[actix_web::test]
async fn restart_with_persisted_receipts_does_not_collide_or_resolve_to_prior_run() {
    // Run 1 on manager A; then a "daemon restart": fresh manager B (fresh
    // in-memory rehearsal state) sharing the SAME persistent receipt
    // backend. Every persistent identifier is run-scoped, so run 2 must
    // complete end-to-end without colliding with — or silently resolving to
    // — run 1's receipts.
    let (ctx1, backend) = make_ctx_with_backend(GovernanceContextBuildMode::Rehearsal);
    let organizer = fresh_did();
    seed_domain(&ctx1.manager, vec![organizer.clone()]).await;
    let app1 = gov_app!(
        ctx1,
        &organizer,
        format!("{READ} {REVIEW} {CONFIRM} {SETUP}")
    );
    let resp = post!(
        &app1,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    post!(
        &app1,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": organizer.to_string()})
    );
    post!(
        &app1,
        format!("/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}/assign"),
        &json!({"assignee_label": "Example member (fictional)"})
    );
    let preview1 = approve_and_preview!(&app1);
    let d1 = preview1["preview_digest"].as_str().unwrap().to_string();
    let resp = post!(
        &app1,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": d1})
    );
    assert_eq!(resp.status(), StatusCode::CREATED);
    let confirm1 = body_json(resp).await;

    // "Restart": new manager, same receipt store.
    let ctx2 = make_ctx_on_backend(GovernanceContextBuildMode::Rehearsal, backend);
    seed_domain(&ctx2.manager, vec![organizer.clone()]).await;
    let app2 = gov_app!(
        ctx2,
        &organizer,
        format!("{READ} {REVIEW} {CONFIRM} {SETUP}")
    );
    let resp = post!(
        &app2,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "post-restart designation succeeds"
    );
    post!(
        &app2,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": organizer.to_string()})
    );
    post!(
        &app2,
        format!("/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}/assign"),
        &json!({"assignee_label": "Example member (fictional)"})
    );
    let preview2 = approve_and_preview!(&app2);
    let d2 = preview2["preview_digest"].as_str().unwrap().to_string();
    let resp = post!(
        &app2,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": d2})
    );
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "post-restart confirm must succeed with fresh run-scoped receipt ids"
    );
    let confirm2 = body_json(resp).await;

    // Fresh receipts, not silent resolutions of run 1's.
    for key in ["session_id", "decision_id", "plan_id", "application_id"] {
        assert_ne!(
            confirm2[key], confirm1[key],
            "{key} must be run-scoped, never reused after restart"
        );
    }
    assert_ne!(confirm2["plan_record_hash"], confirm1["plan_record_hash"]);
    assert_ne!(confirm2["action_item_id"], confirm1["action_item_id"]);
}

#[actix_web::test]
async fn confirm_failure_before_item_creation_leaves_no_item_and_retry_succeeds() {
    let (ctx, backend) = make_ctx_with_backend(GovernanceContextBuildMode::Rehearsal);
    let organizer = fresh_did();
    seed_domain(&ctx.manager, vec![organizer.clone()]).await;
    let app = gov_app!(
        ctx,
        &organizer,
        format!("{READ} {REVIEW} {CONFIRM} {SETUP}")
    );
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
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

    // Plan-receipt failure happens BEFORE the item exists: no item, clean
    // retry. (An action-item creation failure is the same class: nothing to
    // resume, nothing duplicated.)
    backend.fail_class("mutation_plan_recorded");
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let listing = body_json(get!(&app, format!("/domains/{DOMAIN}/action-items"))).await;
    assert_eq!(
        listing.as_array().map(Vec::len),
        Some(0),
        "no action item may exist after a pre-creation failure"
    );

    backend.clear_failures();
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(resp.status(), StatusCode::CREATED, "clean retry succeeds");
    let listing = body_json(get!(&app, format!("/domains/{DOMAIN}/action-items"))).await;
    assert_eq!(
        listing.as_array().map(Vec::len),
        Some(1),
        "exactly one item"
    );
}

#[actix_web::test]
async fn confirm_retry_after_applied_failure_resumes_same_item_never_duplicates() {
    let (ctx, backend) = make_ctx_with_backend(GovernanceContextBuildMode::Rehearsal);
    let organizer = fresh_did();
    seed_domain(&ctx.manager, vec![organizer.clone()]).await;
    let app = gov_app!(
        ctx,
        &organizer,
        format!("{READ} {REVIEW} {CONFIRM} {SETUP}")
    );
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
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

    // The item is created, then the mutation-applied receipt fails.
    backend.fail_class("mutation_applied");
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let listing = body_json(get!(&app, format!("/domains/{DOMAIN}/action-items"))).await;
    assert_eq!(
        listing.as_array().map(Vec::len),
        Some(1),
        "the real item exists after the interrupted confirm"
    );
    let interrupted_id = listing[0]["id"].as_str().unwrap().to_string();

    // Review mutations are blocked while the interrupted execution exists.
    let resp = post!(&app, review_uri(ROW_ACTION), &json!({"decision": "reject"}));
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "review is blocked while an interrupted execution exists"
    );

    // The identical retry RESUMES the same item — never a second one.
    backend.clear_failures();
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "retry completes the execution"
    );
    let confirm = body_json(resp).await;
    assert_eq!(confirm["action_item_id"], interrupted_id.as_str());
    let listing = body_json(get!(&app, format!("/domains/{DOMAIN}/action-items"))).await;
    assert_eq!(
        listing.as_array().map(Vec::len),
        Some(1),
        "an interrupted-then-retried confirm must never duplicate the item"
    );

    // The retry REUSED the original gate receipt instead of appending a
    // second observation. This is what keeps the retry working at all: gate
    // record hashes include recorded_at, so a fresh gate in a later second
    // would change the activation's gate basis and hard-conflict with the
    // already-recorded activation (same activation_id, different basis).
    assert_eq!(
        backend.count_entries("process_gate_result"),
        1,
        "an identical retry must reuse the recorded gate receipt, not append a second"
    );
    assert_eq!(
        backend.count_entries("activation_crossed"),
        1,
        "exactly one activation crossing exists after the retry"
    );
}

#[actix_web::test]
async fn rebinding_is_refused_while_an_interrupted_execution_references_the_label() {
    // If the mutation-applied receipt fails after the item exists, the row
    // holds pending_item_id and the ONLY forward path is the identical
    // retry. Rebinding the assignee label now would change the recomputed
    // digest, turn that retry into a stale-preview 409, and strand the
    // created item without its applied receipt — so rebinding is refused
    // (409) until the retry completes or the rehearsal is reset.
    let (ctx, backend) = make_ctx_with_backend(GovernanceContextBuildMode::Rehearsal);
    let organizer = fresh_did();
    let other = fresh_did();
    seed_domain(&ctx.manager, vec![organizer.clone(), other.clone()]).await;
    let app = gov_app!(
        ctx,
        &organizer,
        format!("{READ} {REVIEW} {CONFIRM} {SETUP}")
    );
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
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

    backend.fail_class("mutation_applied");
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Rebinding the label referenced by the interrupted row: refused.
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": other.to_string()})
    );
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "rebinding must be refused while an interrupted execution references the label"
    );

    // An unrelated label may still be bound during the pending state.
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Another member (fictional)", "did": other.to_string()})
    );
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "labels not referenced by the interrupted row stay bindable"
    );

    // The identical retry still resumes and completes.
    backend.clear_failures();
    let resp = post!(
        &app,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "the identical retry must still complete after the refused rebinding"
    );
}

#[actix_web::test]
async fn reset_after_restart_retires_the_prior_runs_persisted_items() {
    // Action items are DURABLE (Sled in the gateway path) while the review
    // workspace is node-lifetime memory. After a daemon restart the first
    // reset has no in-memory record of the prior run — the durable
    // meeting_context marker is what lets it retire the prior run's
    // fictional items instead of leaving them active in the member's queue.
    let backend = Arc::new(OpaqueUniqueBackend::default());
    let items = Arc::new(InMemoryActionItemStore::new());
    let organizer = fresh_did();

    // Run 1: full confirm on manager A.
    let ctx1 = make_ctx_on_backend_with_items(
        GovernanceContextBuildMode::Rehearsal,
        backend.clone(),
        items.clone(),
    );
    seed_domain(&ctx1.manager, vec![organizer.clone()]).await;
    let app1 = gov_app!(
        ctx1,
        &organizer,
        format!("{READ} {REVIEW} {CONFIRM} {SETUP}")
    );
    let resp = post!(
        &app1,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    post!(
        &app1,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": organizer.to_string()})
    );
    post!(
        &app1,
        format!("/domains/{DOMAIN}/rehearsal/pending-publish/{ROW_ACTION}/assign"),
        &json!({"assignee_label": "Example member (fictional)"})
    );
    let preview = approve_and_preview!(&app1);
    let digest = preview["preview_digest"].as_str().unwrap().to_string();
    let resp = post!(
        &app1,
        confirm_uri(ROW_ACTION),
        &json!({"preview_digest": digest})
    );
    assert_eq!(resp.status(), StatusCode::CREATED);
    let item_id = body_json(resp).await["action_item_id"]
        .as_str()
        .unwrap()
        .to_string();

    // "Restart": manager B shares the durable stores but has NO in-memory
    // workspace. The first reset (a fresh designation) must find and retire
    // the prior run's persisted item through the durable marker.
    let ctx2 =
        make_ctx_on_backend_with_items(GovernanceContextBuildMode::Rehearsal, backend, items);
    seed_domain(&ctx2.manager, vec![organizer.clone()]).await;
    let app2 = gov_app!(ctx2, &organizer, format!("{READ} {SETUP}"));
    let resp = post!(
        &app2,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["prior_item_scan"], "complete");
    assert!(
        body["cancelled_action_items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == item_id.as_str()),
        "post-restart reset must retire the prior run's persisted item: {body}"
    );
    let item = body_json(get!(
        &app2,
        format!("/domains/{DOMAIN}/action-items/{item_id}")
    ))
    .await;
    assert_eq!(
        item["status"], "cancelled",
        "the orphaned fictional item is cancelled, not left active"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Binding-target membership must be AFFIRMATIVE (fail-closed)
// ═══════════════════════════════════════════════════════════════════════════

const TRUST_DOMAIN: &str = "rehearsal-trust-domain";

/// The standard deny body: identical for every deny cause, so the response
/// never distinguishes "provable non-member" from "membership unknowable"
/// and leaks nothing about hidden identities or foreign domains.
fn assert_binding_denied_opaquely(status: StatusCode, body: &Value, did: &Did) {
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "binding must be denied: {body}"
    );
    assert!(
        !body.to_string().contains(&did.to_string()),
        "the deny must not echo the DID: {body}"
    );
    assert_eq!(
        body["error"]
            .as_str()
            .map(|s| s.contains("membership standing")),
        Some(true),
        "the deny carries only the generic standing message: {body}"
    );
}

#[actix_web::test]
async fn trust_threshold_binding_denies_when_no_resolver_is_wired() {
    // Rehearsal keeps the permissive CALLER posture for TrustThreshold
    // domains without a resolver (matching Bootstrap), so the setup caller
    // reaches the handler — but the BINDING TARGET invariant must still
    // fail closed: with no resolver, standing cannot be affirmatively
    // established, so no DID whatsoever may be bound.
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let operator = fresh_did();
    let target = fresh_did();
    seed_trust_domain(&ctx.manager, TRUST_DOMAIN).await;
    let app = gov_app!(ctx, &operator, format!("{READ} {SETUP}"));
    let resp = post!(
        &app,
        format!("/domains/{TRUST_DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "caller-side permissive posture still designates the workspace"
    );
    let resp = post!(
        &app,
        format!("/domains/{TRUST_DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": target.to_string()})
    );
    let status = resp.status();
    let body = body_json(resp).await;
    assert_binding_denied_opaquely(status, &body, &target);
}

#[actix_web::test]
async fn trust_threshold_binding_requires_affirmative_resolver_confirmation() {
    let mut ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let operator = fresh_did();
    let member_target = fresh_did();
    let non_member = fresh_did();
    let indeterminate = fresh_did();
    ctx.membership_resolver = Some(Arc::new(FixedResolver {
        members: vec![operator.clone(), member_target.clone()],
        error_for: Some(indeterminate.clone()),
    }));
    seed_trust_domain(&ctx.manager, TRUST_DOMAIN).await;
    let app = gov_app!(ctx, &operator, format!("{READ} {SETUP}"));
    let resp = post!(
        &app,
        format!("/domains/{TRUST_DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);

    // Resolver AFFIRMS membership → binding proceeds.
    let resp = post!(
        &app,
        format!("/domains/{TRUST_DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": member_target.to_string()})
    );
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "resolver-confirmed membership must permit the binding"
    );
    assert_eq!(body_json(resp).await["bound"], true);

    // Resolver PROVES non-membership → deny.
    let resp = post!(
        &app,
        format!("/domains/{TRUST_DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Second member (fictional)", "did": non_member.to_string()})
    );
    let status = resp.status();
    let deny_non_member = body_json(resp).await;
    assert_binding_denied_opaquely(status, &deny_non_member, &non_member);

    // Resolver CANNOT DETERMINE standing → deny, indistinguishable from the
    // provable non-member deny (same status, same error text).
    let resp = post!(
        &app,
        format!("/domains/{TRUST_DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Second member (fictional)", "did": indeterminate.to_string()})
    );
    let status = resp.status();
    let deny_indeterminate = body_json(resp).await;
    assert_binding_denied_opaquely(status, &deny_indeterminate, &indeterminate);
    assert_eq!(
        deny_non_member["error"], deny_indeterminate["error"],
        "non-member and indeterminate denies must be indistinguishable"
    );
}

#[actix_web::test]
async fn membership_in_another_domain_does_not_permit_binding() {
    // The target holds REAL standing — in a different domain. The binding
    // domain's own membership is the only authority context, and the deny
    // must not reveal that the target exists elsewhere.
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let operator = fresh_did();
    let foreigner = fresh_did();
    seed_domain(&ctx.manager, vec![operator.clone()]).await;
    seed_named_domain(&ctx.manager, vec![foreigner.clone()], "other-domain").await;
    let app = gov_app!(ctx, &operator, format!("{READ} {SETUP}"));
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": foreigner.to_string()})
    );
    let status = resp.status();
    let body = body_json(resp).await;
    assert_binding_denied_opaquely(status, &body, &foreigner);
    assert!(
        !body.to_string().contains("other-domain"),
        "the deny must not reveal foreign-domain standing: {body}"
    );
}

#[actix_web::test]
async fn reset_retires_prior_fictional_items_from_the_active_queue() {
    let (app, organizer) = rehearsal_app_reset!(format!("{READ} {REVIEW} {CONFIRM} {SETUP}"));
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
    let item_id = body_json(resp).await["action_item_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Reset = "start a new rehearsal generation": the prior run's fictional
    // item leaves the active queue through the normal status machinery.
    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert!(
        body["cancelled_action_items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == item_id.as_str()),
        "reset must report the retired item: {body}"
    );
    let item = body_json(get!(
        &app,
        format!("/domains/{DOMAIN}/action-items/{item_id}")
    ))
    .await;
    assert_eq!(
        item["status"], "cancelled",
        "the prior fictional item is cancelled, not erased"
    );
}

#[actix_web::test]
async fn broad_governance_write_retains_setup_review_and_confirm() {
    // Operator-compatibility fallback (sub-capability doctrine): broad
    // governance:write may drive the whole surface. Browser credentials
    // never hold it — pinned by the icn-http-kit boundary tests.
    let ctx = make_ctx(GovernanceContextBuildMode::Rehearsal);
    let operator = fresh_did();
    seed_domain(&ctx.manager, vec![operator.clone()]).await;
    let app = gov_app!(ctx, &operator, format!("{READ} {BROAD}"));

    let resp = post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/reset"),
        &json!({})
    );
    assert_eq!(resp.status(), StatusCode::OK, "broad scope designates");
    post!(
        &app,
        format!("/domains/{DOMAIN}/rehearsal/bindings"),
        &json!({"label": "Example member (fictional)", "did": operator.to_string()})
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
    assert_eq!(resp.status(), StatusCode::CREATED, "broad scope confirms");
}
