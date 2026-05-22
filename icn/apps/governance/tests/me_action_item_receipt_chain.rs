//! End-to-end proof-loop integration tests for the `action_item` /
//! `complete` action-card source path.
//!
//! Pins the runtime proof loop:
//!
//!   `standing → action card → authorized action → receipt`
//!
//! is honest for the `action_item` / `complete` source path — i.e. the
//! card's `receipt_expected: true` is not a lie. Companion to
//! `me_action_card_receipt_chain.rs` (which pins the same loop for the
//! `proposal` / `vote` source path).
//!
//! ## What this test pins
//!
//! 1. A holder with standing in a domain that has an open action item
//!    assigned to them sees a `source_kind=action_item`,
//!    `action_kind=complete`, `receipt_expected=true` card on
//!    `GET /me/action-cards`.
//! 2. Marking the action item complete via the same path the
//!    `update_action_item_status` handler exercises persists an
//!    [`ActionItemCompletionReceipt`] (ADR-0026 Layer 2) keyed by
//!    `item_id`.
//! 3. The receipt's `item_id` equals the action card's `source_id`, the
//!    `actor_did` equals the caller's DID, the `transition` is
//!    `Completed`, and the `record_hash` is a real cryptographic
//!    binding (not a zero placeholder).
//! 4. Re-completing an already-completed item is idempotent — no second
//!    receipt is emitted.
//! 5. Once the item is completed, the action card disappears from the
//!    next `GET /me/action-cards` response (derived view freshness).
//! 6. A different DID does not see another holder's action-item card or
//!    receive a card pointing at their item, regardless of the
//!    receipt's existence in the shared store.
//! 7. The full-update handler (`update_action_item`) routes status
//!    changes through `update_action_item_status` so a `status=Completed`
//!    full update cannot bypass receipt emission.
//! 8. A receipt backend that rejects `put_action_item_completion`
//!    aborts the status transition — the item does not silently
//!    commit `Completed` without provenance.
//! 9. A reopen / re-complete cycle (Completed → Open → Completed)
//!    preserves the prior completion receipt and adds a new one;
//!    the audit chain is append-only.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use actix_web::{body::to_bytes, dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance::{
    ActionItemCompletionReceipt, ActionItemPriority, ActionItemStatus, ActionItemTransition,
    AuthorityGrant, GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams,
    MembershipConfig, MembershipSource,
};
use icn_governance_actor::{
    dispatch_evidence::EffectDispatchEvidence,
    http::{self, GovernanceContext},
    institutional_effect::InstitutionalEffectRecord,
    manager::GovernanceManager,
    receipt_backend::GovernanceReceiptBackend,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};
use serde_json::Value;

// ============================================================================
// Test receipt backend — persists ActionItemCompletionReceipt + lookups.
// ============================================================================

#[derive(Default)]
struct TestReceiptStore {
    governance: Mutex<Vec<GovernanceDecisionReceipt>>,
    action_item_completions: Mutex<Vec<ActionItemCompletionReceipt>>,
}

impl GovernanceReceiptBackend for TestReceiptStore {
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String> {
        self.governance.lock().unwrap().push(receipt.clone());
        Ok(())
    }

    fn get_governance_by_proposal(
        &self,
        _proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }

    fn put_allocation(&self, _receipt: &AllocationReceipt) -> Result<Hash, String> {
        Ok([0u8; 32])
    }

    fn get_governance_by_decision(
        &self,
        _decision_hash: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }

    fn list_allocations_by_decision(
        &self,
        _decision_hash: &Hash,
    ) -> Result<Vec<AllocationReceipt>, String> {
        Ok(vec![])
    }

    fn put_institutional_effect(&self, _record: &InstitutionalEffectRecord) -> Result<(), String> {
        Ok(())
    }

    fn list_institutional_effects_by_proposal(
        &self,
        _proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        Ok(vec![])
    }

    fn put_effect_dispatch_evidence(
        &self,
        _evidence: &EffectDispatchEvidence,
    ) -> Result<(), String> {
        Ok(())
    }

    fn list_effect_dispatch_evidence_by_record(
        &self,
        _effect_record_id: &str,
    ) -> Result<Vec<EffectDispatchEvidence>, String> {
        Ok(vec![])
    }

    fn put_mandate(&self, _mandate: &icn_governance::Mandate) -> Result<(), String> {
        Ok(())
    }

    fn put_authority_grant(&self, _grant: &AuthorityGrant) -> Result<(), String> {
        Ok(())
    }

    fn put_action_item_completion(
        &self,
        receipt: &ActionItemCompletionReceipt,
    ) -> Result<(), String> {
        self.action_item_completions
            .lock()
            .unwrap()
            .push(receipt.clone());
        Ok(())
    }

    fn get_action_item_completion_by_item(
        &self,
        item_id: &str,
    ) -> Result<Option<ActionItemCompletionReceipt>, String> {
        // Latest = receipt with the largest `completed_at`.
        Ok(self
            .list_action_item_completions_by_item(item_id)?
            .into_iter()
            .next_back())
    }

    fn list_action_item_completions_by_item(
        &self,
        item_id: &str,
    ) -> Result<Vec<ActionItemCompletionReceipt>, String> {
        let mut hits: Vec<ActionItemCompletionReceipt> = self
            .action_item_completions
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.item_id == item_id)
            .cloned()
            .collect();
        hits.sort_by_key(|r| r.completed_at);
        Ok(hits)
    }
}

impl TestReceiptStore {
    fn completion_count_for(&self, item_id: &str) -> usize {
        self.action_item_completions
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.item_id == item_id)
            .count()
    }
}

/// A backend whose `put_action_item_completion` always rejects, used to
/// prove the manager's "persist before commit" guarantee.
#[derive(Default)]
struct FailingCompletionStore;

impl GovernanceReceiptBackend for FailingCompletionStore {
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
    fn put_action_item_completion(
        &self,
        _receipt: &ActionItemCompletionReceipt,
    ) -> Result<(), String> {
        Err("simulated receipt backend failure".to_string())
    }
}

// ============================================================================
// Scaffolding
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
        build_mode: icn_governance_actor::http::GovernanceContextBuildMode::Test,
    };
    Harness { ctx, receipts }
}

async fn seed_domain_with_member(
    mgr: &GovernanceManager,
    caller: &Did,
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
            source: MembershipSource::StaticList(vec![caller.clone()]),
        },
    )
    .await
    .expect("create_domain");
    domain
}

macro_rules! action_cards_app {
    ($ctx:expr, $caller_did:expr) => {{
        let caller = $caller_did.to_string();
        test::init_service(
            App::new()
                .wrap_fn(move |req, srv| {
                    req.extensions_mut().insert(BasicClaims {
                        sub: caller.clone(),
                        scope: Some("governance:read".to_string()),
                    });
                    srv.call(req)
                })
                .configure(|cfg| http::configure(cfg, $ctx)),
        )
        .await
    }};
}

macro_rules! fetch_action_cards {
    ($app:expr) => {{
        let req = test::TestRequest::get()
            .uri("/me/action-cards")
            .to_request();
        let resp = test::call_service(&$app, req).await;
        let status = resp.status();
        let bytes = to_bytes(resp.into_body()).await.unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).expect("response is valid JSON");
        (status, parsed)
    }};
}

// ============================================================================
// Tests
// ============================================================================

#[actix_web::test]
async fn action_item_completion_receipt_chain_end_to_end() {
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let item = h
        .ctx
        .manager
        .create_action_item(
            domain.clone(),
            "Draft fictional onboarding note".to_string(),
            Some("Background: a fictional onboarding example.".to_string()),
            caller.clone(),
            Some(caller.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec!["fictional".to_string()],
        )
        .expect("create_action_item");
    let item_id_str = item.id.to_string();

    // ── 2. Action card: action_item / complete with receipt_expected=true.
    let app = action_cards_app!(h.ctx.clone(), &caller);
    let (status, body) = fetch_action_cards!(app);
    assert_eq!(status, StatusCode::OK);
    let cards = body["cards"].as_array().expect("cards is an array");
    let item_card = cards
        .iter()
        .find(|c| c["source_kind"] == "action_item" && c["action_kind"] == "complete")
        .expect("expected an action_item/complete card before the action runs");
    assert_eq!(item_card["source_id"], item_id_str);
    assert_eq!(item_card["receipt_expected"], true);
    let card_source_id = item_card["source_id"].as_str().unwrap().to_string();

    // ── 3. Authorized action: the assignee marks the item complete via the
    //       same code path the HTTP handler exercises.
    let updated = h
        .ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &caller)
        .expect("update_action_item_status -> Completed");
    assert_eq!(updated.status, ActionItemStatus::Completed);

    // ── 4. Receipt: store contains an ActionItemCompletionReceipt keyed by
    //       the same string the action card carried as source_id.
    let receipt = h
        .receipts
        .get_action_item_completion_by_item(&card_source_id)
        .expect("receipt lookup")
        .expect(
            "expected an ActionItemCompletionReceipt — the action card's \
             receipt_expected:true would otherwise be a lie",
        );
    assert_eq!(receipt.item_id, card_source_id);
    assert_eq!(receipt.domain_id, domain.0);
    assert_eq!(receipt.actor_did, caller.to_string());
    assert_eq!(receipt.transition, ActionItemTransition::Completed);
    assert_ne!(
        receipt.record_hash, [0u8; 32],
        "record_hash must be a real cryptographic binding, not a zero placeholder"
    );
    assert_eq!(
        receipt.completed_at, updated.updated_at,
        "completed_at must equal the action item's post-transition updated_at"
    );

    // ── 5. Card freshness: derived view drops the completed item.
    let app2 = action_cards_app!(h.ctx.clone(), &caller);
    let (_, body_after) = fetch_action_cards!(app2);
    let post_cards = body_after["cards"].as_array().unwrap();
    assert!(
        !post_cards.iter().any(|c| c["source_id"] == card_source_id),
        "completed action item must not appear as an action card"
    );

    // ── 6. Idempotence: re-saving Completed must not double-emit.
    let _ = h
        .ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &caller)
        .expect("idempotent re-complete");
    assert_eq!(
        h.receipts.completion_count_for(&item_id_str),
        1,
        "re-completing an already-completed item must not emit a second receipt"
    );
}

#[actix_web::test]
async fn another_did_does_not_see_first_did_completion_via_action_cards() {
    let h = make_harness();
    let alice = fresh_did();
    let bob = fresh_did();
    assert_ne!(alice, bob);

    let domain = seed_domain_with_member(&h.ctx.manager, &alice, "test-coop").await;

    let item = h
        .ctx
        .manager
        .create_action_item(
            domain.clone(),
            "Alice's private fictional task".to_string(),
            None,
            alice.clone(),
            Some(alice.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec!["fictional".to_string()],
        )
        .expect("create_action_item");

    h.ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &alice)
        .expect("alice complete");

    // Bob: not a member, never assigned. Must not see a card pointing at
    // alice's item id, regardless of the receipt's existence in the shared
    // store.
    let app = action_cards_app!(h.ctx.clone(), &bob);
    let (status, body) = fetch_action_cards!(app);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["did"], bob.to_string());
    let bob_cards = body["cards"].as_array().unwrap();
    let item_id_str = item.id.to_string();
    assert!(
        !bob_cards.iter().any(|c| c["source_id"] == item_id_str),
        "bob must not see any card referencing alice's action item"
    );
}

#[actix_web::test]
async fn completion_receipt_uses_regulatory_safe_vocabulary() {
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let item = h
        .ctx
        .manager
        .create_action_item(
            domain.clone(),
            "Vocabulary-discipline task (fictional)".to_string(),
            Some(
                "Make sure the completion receipt stays away from regulated-finance vocabulary."
                    .to_string(),
            ),
            caller.clone(),
            Some(caller.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec!["fictional".to_string()],
        )
        .expect("create_action_item");

    h.ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &caller)
        .expect("complete");

    let receipt = h
        .receipts
        .get_action_item_completion_by_item(&item.id.to_string())
        .unwrap()
        .expect("receipt expected");
    let serialized = serde_json::to_string(&receipt).unwrap().to_lowercase();
    for forbidden in ["payment", "wallet", "balance", "currency"] {
        assert!(
            !serialized.contains(forbidden),
            "ActionItemCompletionReceipt must not contain forbidden term '{forbidden}': {serialized}"
        );
    }
}

#[actix_web::test]
async fn full_update_to_completed_emits_completion_receipt() {
    // Pins that the full-update handler does NOT bypass the receipt path:
    // a request that sets status=Completed via PUT
    // /v1/gov/domains/{domain_id}/action-items/{item_id} routes through
    // update_action_item_status and persists a receipt.
    use actix_web::http::StatusCode;
    use serde_json::json;

    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let item = h
        .ctx
        .manager
        .create_action_item(
            domain.clone(),
            "Full-update completion (fictional)".to_string(),
            None,
            caller.clone(),
            Some(caller.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec!["fictional".to_string()],
        )
        .expect("create_action_item");
    let item_id_str = item.id.to_string();

    // Build a write-scoped app — the full-update endpoint requires
    // governance:write, unlike the read-only `/me/action-cards` macro.
    let caller_str = caller.to_string();
    let ctx_clone = h.ctx.clone();
    let app = test::init_service(
        App::new()
            .wrap_fn(move |req, srv| {
                req.extensions_mut().insert(BasicClaims {
                    sub: caller_str.clone(),
                    scope: Some("governance:write governance:read".to_string()),
                });
                srv.call(req)
            })
            .configure(|cfg| http::configure(cfg, ctx_clone)),
    )
    .await;

    let body = json!({ "status": "completed" });
    let req = test::TestRequest::put()
        .uri(&format!(
            "/domains/{}/action-items/{}",
            domain.0, item_id_str
        ))
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "full-update to status=completed should succeed"
    );

    // The full-update handler must have routed through
    // update_action_item_status, so a completion receipt must exist.
    let receipt = h
        .receipts
        .get_action_item_completion_by_item(&item_id_str)
        .unwrap();
    assert!(
        receipt.is_some(),
        "full-update setting status=completed must emit a completion receipt — \
         it must not bypass the receipt-bearing status path"
    );
    let r = receipt.unwrap();
    assert_eq!(r.item_id, item_id_str);
    assert_eq!(r.actor_did, caller.to_string());
    assert_eq!(r.transition, ActionItemTransition::Completed);
}

#[actix_web::test]
async fn receipt_backend_failure_prevents_completed_status_commit() {
    // Pins the manager's "persist before commit" guarantee: when the
    // receipt backend rejects put_action_item_completion, the action
    // item save does NOT run — the item must not silently advance into
    // Completed without provenance.
    let receipts = Arc::new(FailingCompletionStore);
    let manager = GovernanceManager::new()
        .with_receipt_store(receipts.clone() as Arc<dyn GovernanceReceiptBackend>);
    let manager = Arc::new(manager);

    let caller = fresh_did();
    let domain = GovernanceDomainId::new("test-coop");
    manager
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
                source: MembershipSource::StaticList(vec![caller.clone()]),
            },
        )
        .await
        .expect("create_domain");

    let item = manager
        .create_action_item(
            domain.clone(),
            "Will fail to complete (fictional)".to_string(),
            None,
            caller.clone(),
            Some(caller.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec!["fictional".to_string()],
        )
        .expect("create_action_item");
    let prior_status = item.status;

    let result =
        manager.update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &caller);
    assert!(
        result.is_err(),
        "completion must fail when the receipt backend rejects the receipt"
    );

    // The item's stored state must not have advanced into Completed —
    // the receipt store's rejection is an abort, not a warning.
    let after = manager
        .get_action_item(&domain, &item.id)
        .expect("get_action_item")
        .expect("item still present");
    assert_eq!(
        after.status, prior_status,
        "status must not commit Completed when receipt persistence failed"
    );
}

#[actix_web::test]
async fn reopen_and_recomplete_preserves_completion_history() {
    // Pins the append-only contract: a Completed → Open → Completed
    // cycle on the same item must produce two distinct receipts; the
    // first must not be overwritten.
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let item = h
        .ctx
        .manager
        .create_action_item(
            domain.clone(),
            "Reopenable task (fictional)".to_string(),
            None,
            caller.clone(),
            Some(caller.clone()),
            None,
            ActionItemPriority::Medium,
            None,
            None,
            vec!["fictional".to_string()],
        )
        .expect("create_action_item");
    let item_id_str = item.id.to_string();

    // First completion.
    h.ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &caller)
        .expect("first complete");
    let first_receipt = h
        .receipts
        .get_action_item_completion_by_item(&item_id_str)
        .unwrap()
        .expect("first receipt");

    // Sleep enough that `completed_at` (Unix-seconds) advances when we
    // re-complete, so the two receipts are distinct on
    // (item_id, completed_at) and produce distinct record_hashes. The
    // canonical hash is over (item_id, domain_id, actor_did,
    // transition, completed_at), so a strict +1s wall-clock advance is
    // sufficient.
    std::thread::sleep(std::time::Duration::from_millis(1_100));

    // Reopen.
    h.ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::InProgress, &caller)
        .expect("reopen via InProgress");

    // Second completion.
    h.ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &caller)
        .expect("second complete");

    let chain = h
        .receipts
        .list_action_item_completions_by_item(&item_id_str)
        .unwrap();
    assert_eq!(
        chain.len(),
        2,
        "reopen / re-complete cycle must produce two completion receipts; the chain is append-only"
    );
    assert_eq!(
        chain[0].record_hash, first_receipt.record_hash,
        "first receipt in the chain must be the original (oldest-first ordering)"
    );
    assert!(
        chain[0].completed_at < chain[1].completed_at,
        "second receipt's completed_at must be strictly later"
    );
    assert_ne!(
        chain[0].record_hash, chain[1].record_hash,
        "the two receipts must have distinct record_hashes — append-only history"
    );

    // The "latest" lookup returns the most recent.
    let latest = h
        .receipts
        .get_action_item_completion_by_item(&item_id_str)
        .unwrap()
        .expect("latest receipt");
    assert_eq!(latest.record_hash, chain[1].record_hash);
}

// ============================================================================
// HTTP endpoint: GET .../action-items/{item_id}/completion-receipt
// ============================================================================

/// Builds an actix-web app with the full governance routing surface and a
/// fixed `BasicClaims` injected for the supplied DID + scope. Used by the
/// completion-receipt HTTP tests below so they exercise the same handler
/// path a real gateway would route to.
macro_rules! gov_app_with_scope {
    ($ctx:expr, $caller_did:expr, $scope:expr) => {{
        let caller = $caller_did.to_string();
        let scope = $scope.to_string();
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

#[actix_web::test]
async fn completion_receipt_endpoint_returns_persisted_receipt() {
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    // Create + complete the item via the manager (same code path the
    // status-update HTTP handler exercises).
    let item = h
        .ctx
        .manager
        .create_action_item(
            domain.clone(),
            "Endpoint smoke item".to_string(),
            Some("Repo-safe placeholder for the completion-receipt endpoint.".to_string()),
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

    let updated = h
        .ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &caller)
        .expect("update_action_item_status -> Completed");

    // GET the new endpoint as the same caller (governance:read). The
    // receipt body must round-trip the persisted `ActionItemCompletionReceipt`.
    let app = gov_app_with_scope!(h.ctx.clone(), &caller, "governance:read");
    let req = test::TestRequest::get()
        .uri(&format!(
            "/domains/{}/action-items/{}/completion-receipt",
            domain.0, item_id
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).expect("response is valid JSON");

    // Field-level assertions: every field that survives the manager →
    // backend → handler → wire roundtrip must match the persisted record.
    assert_eq!(body["item_id"], item_id);
    assert_eq!(body["domain_id"], domain.0);
    assert_eq!(body["actor_did"], caller.to_string());
    assert_eq!(body["transition"], "completed");
    assert_eq!(body["completed_at"], updated.updated_at);
    let record_hash_hex = body["record_hash"]
        .as_str()
        .or_else(|| body["record_hash"].as_array().map(|_| "array"))
        .unwrap_or("");
    assert!(
        !record_hash_hex.is_empty() || body["record_hash"].is_array(),
        "record_hash must serialize as a non-empty hex string or byte array, not omitted"
    );
}

#[actix_web::test]
async fn completion_receipt_endpoint_404_when_no_receipt_yet() {
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    // Create but do NOT complete: no receipt should exist.
    let item = h
        .ctx
        .manager
        .create_action_item(
            domain.clone(),
            "Endpoint 404 item".to_string(),
            None,
            caller.clone(),
            Some(caller.clone()),
            None,
            ActionItemPriority::Low,
            None,
            None,
            vec![],
        )
        .expect("create_action_item");

    let app = gov_app_with_scope!(h.ctx.clone(), &caller, "governance:read");
    let req = test::TestRequest::get()
        .uri(&format!(
            "/domains/{}/action-items/{}/completion-receipt",
            domain.0, item.id
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "an item without a completed transition has no receipt; endpoint must 404"
    );
}

#[actix_web::test]
async fn completion_receipt_endpoint_does_not_leak_across_domains() {
    let h = make_harness();
    let caller = fresh_did();
    let dom_a = seed_domain_with_member(&h.ctx.manager, &caller, "domain-a").await;
    let dom_b = seed_domain_with_member(&h.ctx.manager, &caller, "domain-b").await;

    // Create + complete in domain A only.
    let item = h
        .ctx
        .manager
        .create_action_item(
            dom_a.clone(),
            "Domain-A item".to_string(),
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
    h.ctx
        .manager
        .update_action_item_status(&dom_a, &item.id, ActionItemStatus::Completed, &caller)
        .expect("complete in dom_a");
    let item_id = item.id.to_string();

    // GET via dom_b's path — must not surface dom_a's receipt.
    let app = gov_app_with_scope!(h.ctx.clone(), &caller, "governance:read");
    let req_wrong_domain = test::TestRequest::get()
        .uri(&format!(
            "/domains/{}/action-items/{}/completion-receipt",
            dom_b.0, item_id
        ))
        .to_request();
    let resp_wrong = test::call_service(&app, req_wrong_domain).await;
    assert_eq!(
        resp_wrong.status(),
        StatusCode::NOT_FOUND,
        "completion receipts must not be visible across governance domains"
    );

    // Sanity: same item id under the correct domain still resolves.
    let req_right_domain = test::TestRequest::get()
        .uri(&format!(
            "/domains/{}/action-items/{}/completion-receipt",
            dom_a.0, item_id
        ))
        .to_request();
    let resp_right = test::call_service(&app, req_right_domain).await;
    assert_eq!(resp_right.status(), StatusCode::OK);
}

#[actix_web::test]
async fn completion_receipt_endpoint_response_uses_regulatory_safe_vocabulary() {
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let item = h
        .ctx
        .manager
        .create_action_item(
            domain.clone(),
            "Vocabulary smoke item".to_string(),
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
    h.ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &caller)
        .expect("complete");

    let app = gov_app_with_scope!(h.ctx.clone(), &caller, "governance:read");
    let req = test::TestRequest::get()
        .uri(&format!(
            "/domains/{}/action-items/{}/completion-receipt",
            domain.0, item.id
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    let raw = std::str::from_utf8(&bytes)
        .expect("response bytes are valid UTF-8")
        .to_lowercase();
    for forbidden in [
        "payment", "currency", "wallet", "balance", "deposit", "withdraw",
    ] {
        assert!(
            !raw.contains(forbidden),
            "completion receipt response must not contain forbidden term '{forbidden}': {raw}"
        );
    }
}

#[actix_web::test]
async fn completion_receipt_endpoint_canonicalizes_non_canonical_uuids() {
    // Pins the fix for the reviewer-flagged issue on #1675: the receipt
    // store indexes by the canonical lowercase-hyphenated UUID string
    // the manager wrote at emission time, so the handler must canonicalize
    // the parsed id back to a string before looking it up. Without this,
    // a caller passing uppercase hex or URN-form UUIDs would parse OK
    // and then miss the index entry and receive a spurious 404.
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let item = h
        .ctx
        .manager
        .create_action_item(
            domain.clone(),
            "Canonical UUID smoke item".to_string(),
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
    h.ctx
        .manager
        .update_action_item_status(&domain, &item.id, ActionItemStatus::Completed, &caller)
        .expect("complete");

    let item_id_lower = item.id.to_string();
    let item_id_upper = item_id_lower.to_uppercase();
    let item_id_urn = format!("urn:uuid:{item_id_lower}");

    assert_ne!(
        item_id_upper, item_id_lower,
        "uppercase form must differ byte-wise from canonical form for this test to be meaningful"
    );

    let app = gov_app_with_scope!(h.ctx.clone(), &caller, "governance:read");

    for variant in [&item_id_lower, &item_id_upper, &item_id_urn] {
        let req = test::TestRequest::get()
            .uri(&format!(
                "/domains/{}/action-items/{}/completion-receipt",
                domain.0, variant
            ))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "non-canonical UUID variant {variant} must resolve to the same persisted receipt"
        );
        let bytes = to_bytes(resp.into_body()).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).expect("response is valid JSON");
        assert_eq!(
            body["item_id"], item_id_lower,
            "response item_id must be the canonical lowercase form regardless of input casing"
        );
    }
}
