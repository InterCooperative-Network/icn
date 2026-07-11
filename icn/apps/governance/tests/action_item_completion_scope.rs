//! #2400 — completion-only action-item capability
//! (`governance:action-item:complete`).
//!
//! Value-sensitive authorization on
//! `PUT /gov/domains/{domain}/action-items/{item}/status`: the completion-only
//! scope authorizes **only** the `completed` transition and nothing else, while
//! the broad `governance:meeting:write` / `governance:write` scopes keep
//! full-range access (backward compatible). The completion evidence
//! (`capability_scope_presented`) must record the narrow scope truthfully.
//!
//! Gate-semantics (authorization) assertions follow the `*_scope_migration`
//! family: acceptance = "not 401 and not 403" (the request passes the gate; its
//! downstream status depends on domain/item state, not under test here);
//! rejection = exactly 403 at the gate. Malformed transitions fail closed (400).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use actix_web::{dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance::{
    ActionItemCompletionReceipt, ActionItemCompletionReceiptV2, ActionItemPriority,
    GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams, MembershipConfig,
    MembershipSource,
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

const COMPLETE: &str = "governance:action-item:complete";
const MEETING_CLASS: &str = "governance:meeting:write";
const LEGACY_BROAD: &str = "governance:write";
const READ: &str = "governance:read";

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

/// Minimal receipt backend that captures the v2 completion receipts so a test
/// can assert the `capability_scope_presented` evidence. All other trait methods
/// use no-op/None defaults (v1 persistence and opaque routing are not under test
/// here).
#[derive(Default)]
struct V2CapturingStore {
    v2: Mutex<Vec<ActionItemCompletionReceiptV2>>,
}

impl GovernanceReceiptBackend for V2CapturingStore {
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
    fn put_action_item_completion(&self, _r: &ActionItemCompletionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn put_action_item_completion_v2(
        &self,
        receipt: &ActionItemCompletionReceiptV2,
    ) -> Result<(), String> {
        self.v2.lock().unwrap().push(receipt.clone());
        Ok(())
    }
}

fn make_ctx() -> GovernanceContext<NoopEventEmitter> {
    ctx_from_manager(GovernanceManager::new())
}

fn ctx_from_manager(manager: GovernanceManager) -> GovernanceContext<NoopEventEmitter> {
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
        build_mode: icn_governance_actor::http::GovernanceContextBuildMode::Test,
    }
}

async fn seed_domain(
    mgr: &GovernanceManager,
    members: Vec<Did>,
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
            source: MembershipSource::StaticList(members),
        },
    )
    .await
    .expect("create_domain");
    domain
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

/// Drive a single request against a fresh (unseeded) context with the given
/// scope, returning the HTTP status. Used for gate-semantics assertions.
async fn status_for(method: &str, path: &str, body: &Value, scope: &'static str) -> StatusCode {
    let ctx = make_ctx();
    let caller = fresh_did();
    let app = gov_app!(ctx, &caller, scope);
    let builder = match method {
        "POST" => test::TestRequest::post(),
        "PUT" => test::TestRequest::put(),
        other => panic!("unsupported method {other}"),
    };
    let req = builder.uri(path).set_json(body).to_request();
    test::call_service(&app, req).await.status()
}

const STATUS_ROUTE: &str = "/domains/test-domain/action-items/test-item/status";

fn assert_gate_accepted(status: StatusCode, scope: &str, ctx: &str) {
    assert!(
        status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
        "{scope} must be accepted at the gate ({ctx}); {status} is a downstream \
         outcome, not an auth rejection"
    );
}

// ── The completion-only scope authorizes exactly the `completed` transition ──

#[actix_web::test]
async fn complete_scope_accepts_completed_transition() {
    let status = status_for(
        "PUT",
        STATUS_ROUTE,
        &json!({ "status": "completed" }),
        COMPLETE,
    )
    .await;
    assert_gate_accepted(status, COMPLETE, "completed transition");
}

#[actix_web::test]
async fn complete_scope_rejects_in_progress_transition() {
    let status = status_for(
        "PUT",
        STATUS_ROUTE,
        &json!({ "status": "in_progress" }),
        COMPLETE,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "completion-only scope must NOT authorize the in_progress transition"
    );
}

#[actix_web::test]
async fn complete_scope_rejects_cancelled_transition() {
    let status = status_for(
        "PUT",
        STATUS_ROUTE,
        &json!({ "status": "cancelled" }),
        COMPLETE,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "completion-only scope must NOT authorize the cancelled transition"
    );
}

#[actix_web::test]
async fn complete_scope_rejects_deferred_transition() {
    let status = status_for(
        "PUT",
        STATUS_ROUTE,
        &json!({ "status": "deferred" }),
        COMPLETE,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "completion-only scope must NOT authorize the deferred transition"
    );
}

// ── The completion-only scope cannot create items/meetings or add notes ──

#[actix_web::test]
async fn complete_scope_rejects_create_action_item() {
    let status = status_for(
        "POST",
        "/domains/test-domain/action-items",
        &json!({ "title": "nope" }),
        COMPLETE,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "completion-only scope must NOT authorize creating an action item"
    );
}

#[actix_web::test]
async fn complete_scope_rejects_create_meeting() {
    let status = status_for(
        "POST",
        "/domains/test-domain/meetings",
        &json!({ "title": "nope" }),
        COMPLETE,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "completion-only scope must NOT authorize creating a meeting"
    );
}

#[actix_web::test]
async fn complete_scope_rejects_add_action_item_note() {
    let status = status_for(
        "POST",
        "/domains/test-domain/action-items/test-item/notes",
        &json!({ "content": "nope" }),
        COMPLETE,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "completion-only scope must NOT authorize adding an action-item note"
    );
}

// ── Backward compatibility: broad scopes keep full-range access ──

#[actix_web::test]
async fn meeting_class_still_accepts_completed_transition() {
    let status = status_for(
        "PUT",
        STATUS_ROUTE,
        &json!({ "status": "completed" }),
        MEETING_CLASS,
    )
    .await;
    assert_gate_accepted(status, MEETING_CLASS, "completed transition");
}

#[actix_web::test]
async fn meeting_class_still_accepts_in_progress_transition() {
    let status = status_for(
        "PUT",
        STATUS_ROUTE,
        &json!({ "status": "in_progress" }),
        MEETING_CLASS,
    )
    .await;
    assert_gate_accepted(status, MEETING_CLASS, "in_progress transition");
}

#[actix_web::test]
async fn legacy_broad_still_accepts_completed_transition() {
    let status = status_for(
        "PUT",
        STATUS_ROUTE,
        &json!({ "status": "completed" }),
        LEGACY_BROAD,
    )
    .await;
    assert_gate_accepted(status, LEGACY_BROAD, "completed transition");
}

#[actix_web::test]
async fn read_scope_rejects_completed_transition() {
    let status = status_for("PUT", STATUS_ROUTE, &json!({ "status": "completed" }), READ).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "governance:read must NOT authorize completing an action item"
    );
}

// ── Unknown transitions fail closed (400) before any state mutation, and
// before scope selection, so the value-sensitive gate can never see an
// untrusted transition value. ──

#[actix_web::test]
async fn unknown_transition_fails_closed_for_complete_scope() {
    let status = status_for("PUT", STATUS_ROUTE, &json!({ "status": "bogus" }), COMPLETE).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an unknown transition must fail closed (400), not be authorized"
    );
}

#[actix_web::test]
async fn unknown_transition_fails_closed_for_meeting_scope() {
    let status = status_for(
        "PUT",
        STATUS_ROUTE,
        &json!({ "status": "bogus" }),
        MEETING_CLASS,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an unknown transition must fail closed (400) regardless of scope"
    );
}

// ── Positive end-to-end: the completion-only scope completes an assigned item,
// and the evidence records the narrow scope that authorized it. ──

#[actix_web::test]
async fn completion_scope_completes_assigned_item_and_records_evidence() {
    let store = Arc::new(V2CapturingStore::default());
    let manager = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    let ctx = ctx_from_manager(manager);

    let caller = fresh_did();
    let domain = seed_domain(&ctx.manager, vec![caller.clone()], "test-coop").await;
    let item = ctx
        .manager
        .create_action_item(
            domain.clone(),
            "complete me".to_string(),
            None,
            caller.clone(),
            Some(caller.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec![],
        )
        .expect("create_action_item");
    let item_id = item.id.to_string();

    let app = gov_app!(ctx, &caller, format!("{READ} {COMPLETE}"));
    let req = test::TestRequest::put()
        .uri(&format!(
            "/domains/{}/action-items/{}/status",
            domain.0, item_id
        ))
        .set_json(json!({ "status": "completed" }))
        .to_request();
    let status = test::call_service(&app, req).await.status();
    assert_eq!(
        status,
        StatusCode::OK,
        "the completion-only scope must complete an item assigned to its subject"
    );

    let v2 = store.v2.lock().unwrap();
    let receipt = v2
        .iter()
        .find(|r| r.item_id == item_id)
        .expect("a v2 completion receipt must be persisted");
    assert_eq!(
        receipt.capability_scope_presented, COMPLETE,
        "evidence must record the completion-only scope that authorized the request"
    );
}

// ── Ownership stays decisive: the completion scope cannot complete another
// member's item, even when the caller is a domain member. ──

#[actix_web::test]
async fn completion_scope_cannot_complete_another_members_item() {
    let alice = fresh_did();
    let bob = fresh_did();
    let manager = GovernanceManager::new();
    let ctx = ctx_from_manager(manager);
    let domain = seed_domain(&ctx.manager, vec![alice.clone(), bob.clone()], "test-coop").await;
    // Item created by and assigned to bob.
    let item = ctx
        .manager
        .create_action_item(
            domain.clone(),
            "bob's item".to_string(),
            None,
            bob.clone(),
            Some(bob.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec![],
        )
        .expect("create_action_item");
    let item_id = item.id.to_string();

    // Alice holds the completion scope and is a domain member, but is neither
    // the creator nor the assignee.
    let app = gov_app!(ctx, &alice, format!("{READ} {COMPLETE}"));
    let req = test::TestRequest::put()
        .uri(&format!(
            "/domains/{}/action-items/{}/status",
            domain.0, item_id
        ))
        .set_json(json!({ "status": "completed" }))
        .to_request();
    let status = test::call_service(&app, req).await.status();
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a completion-scoped member must NOT complete another member's item"
    );
}

// ── The completion-only capability completes an item ASSIGNED to the caller —
// not one they merely created. Creator-based completion is reserved for the
// broader meeting:write / write scopes (Codex #2400 review). ──

#[actix_web::test]
async fn completion_scope_cannot_complete_item_it_created_for_another_assignee() {
    let alice = fresh_did();
    let bob = fresh_did();
    let ctx = ctx_from_manager(GovernanceManager::new());
    let domain = seed_domain(&ctx.manager, vec![alice.clone(), bob.clone()], "test-coop").await;
    // Item created BY alice, assigned TO bob.
    let item = ctx
        .manager
        .create_action_item(
            domain.clone(),
            "alice's item for bob".to_string(),
            None,
            alice.clone(),
            Some(bob.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec![],
        )
        .expect("create_action_item");
    let item_id = item.id.to_string();

    // Alice holds the completion-only scope and is the creator, but NOT the assignee.
    let app = gov_app!(ctx, &alice, format!("{READ} {COMPLETE}"));
    let req = test::TestRequest::put()
        .uri(&format!(
            "/domains/{}/action-items/{}/status",
            domain.0, item_id
        ))
        .set_json(json!({ "status": "completed" }))
        .to_request();
    let status = test::call_service(&app, req).await.status();
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the completion-only scope must NOT complete an item the caller created but is not assigned"
    );
}

#[actix_web::test]
async fn meeting_scope_creator_can_still_complete_item_assigned_to_another() {
    let alice = fresh_did();
    let bob = fresh_did();
    let ctx = ctx_from_manager(GovernanceManager::new());
    let domain = seed_domain(&ctx.manager, vec![alice.clone(), bob.clone()], "test-coop").await;
    // Item created BY alice, assigned TO bob.
    let item = ctx
        .manager
        .create_action_item(
            domain.clone(),
            "alice's item for bob".to_string(),
            None,
            alice.clone(),
            Some(bob.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec![],
        )
        .expect("create_action_item");
    let item_id = item.id.to_string();

    // The broader meeting:write scope retains creator-or-assignee status updates:
    // alice (creator) may complete an item assigned to bob.
    let app = gov_app!(ctx, &alice, format!("{READ} {MEETING_CLASS}"));
    let req = test::TestRequest::put()
        .uri(&format!(
            "/domains/{}/action-items/{}/status",
            domain.0, item_id
        ))
        .set_json(json!({ "status": "completed" }))
        .to_request();
    let status = test::call_service(&app, req).await.status();
    assert_eq!(
        status,
        StatusCode::OK,
        "the broad meeting:write scope must retain creator-based completion"
    );
}

// ── A caller holding BOTH the completion-only scope AND a broad scope keeps
// broad-scope creator-completion; the recorded evidence is the broad scope that
// actually authorized it, not the completion-only capability (Codex #2400
// review — the ownership mode turns on whether a broad scope authorized, not on
// the narrowest scope matched). ──

#[actix_web::test]
async fn dual_scope_creator_completes_item_for_another_and_records_broad_scope() {
    let store = Arc::new(V2CapturingStore::default());
    let manager = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    let alice = fresh_did();
    let bob = fresh_did();
    let ctx = ctx_from_manager(manager);
    let domain = seed_domain(&ctx.manager, vec![alice.clone(), bob.clone()], "test-coop").await;
    // Item created BY alice, assigned TO bob.
    let item = ctx
        .manager
        .create_action_item(
            domain.clone(),
            "alice's item for bob".to_string(),
            None,
            alice.clone(),
            Some(bob.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec![],
        )
        .expect("create_action_item");
    let item_id = item.id.to_string();

    // Alice holds the completion-only scope AND the broad meeting:write scope.
    let app = gov_app!(ctx, &alice, format!("{READ} {COMPLETE} {MEETING_CLASS}"));
    let req = test::TestRequest::put()
        .uri(&format!(
            "/domains/{}/action-items/{}/status",
            domain.0, item_id
        ))
        .set_json(json!({ "status": "completed" }))
        .to_request();
    let status = test::call_service(&app, req).await.status();
    assert_eq!(
        status,
        StatusCode::OK,
        "a broad-scope creator must retain creator-based completion even if the \
         token also carries the completion-only scope"
    );

    let v2 = store.v2.lock().unwrap();
    let receipt = v2
        .iter()
        .find(|r| r.item_id == item_id)
        .expect("a v2 completion receipt must be persisted");
    assert_eq!(
        receipt.capability_scope_presented, MEETING_CLASS,
        "a creator-completion must record the broad scope that authorized it, \
         not the completion-only capability the caller also carries"
    );
}
