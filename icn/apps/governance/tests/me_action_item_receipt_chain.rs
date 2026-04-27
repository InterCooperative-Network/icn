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
        Ok(self
            .action_item_completions
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.item_id == item_id)
            .cloned())
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
