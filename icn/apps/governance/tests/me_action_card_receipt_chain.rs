//! End-to-end proof-loop integration tests for issue icn#1646 follow-up.
//!
//! Proves the runtime proof loop:
//!
//!   `standing → action card → authorized action → receipt`
//!
//! is honest for the `proposal` / `vote` action-card source path.
//!
//! ## What this test pins
//!
//! 1. A holder with standing in a domain that has an open proposal sees a
//!    `source_kind=proposal`, `action_kind=vote`, `receipt_expected=true`
//!    card on `GET /me/action-cards`.
//! 2. Casting the vote (the authorized action) and closing the proposal
//!    produces a `GovernanceDecisionReceipt` per ADR-0026 Layer 2 in the
//!    receipt store.
//! 3. The receipt's `proposal_id` equals the action card's `source_id` —
//!    i.e. the link between the card and its receipt is real, not implied.
//! 4. Once the proposal closes, the action card disappears from the next
//!    `GET /me/action-cards` (derived view freshness, per ADR-0027).
//! 5. A different DID does not see another holder's cards or receipts.
//!
//! Source paths still without a receipt path verified here:
//!   - `meeting` / `attend` — meeting-attendance write path is not yet
//!     receipt-bearing in the runtime and currently surfaces
//!     `receipt_expected: false`; tracked separately.
//!   - `action_item` / `complete` — action-item completion is currently a
//!     status update; the card advertises `receipt_expected: true`
//!     because that is the intended target, but no receipt envelope is
//!     emitted today.
//!
//! These paths are out of scope for the slice this test pins. ADR-0020
//! row 7 records the coverage status.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use actix_web::{body::to_bytes, dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance::{
    AuthorityGrant, GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams,
    MembershipConfig, MembershipSource, ProofOutcome, ProposalId, ProposalPayload, ProposalScope,
    VoteChoice,
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
// Test receipt backend — stores governance receipts in memory so tests can
// observe what the close-path emitted.
// ============================================================================

/// Minimal receipt backend that actually persists `GovernanceDecisionReceipt`
/// values so the test can observe the receipt the close path produced. Only
/// the methods the proof-loop test needs are non-trivial; the rest fall back
/// to the trait defaults or no-op stubs.
#[derive(Default)]
struct TestReceiptStore {
    governance: Mutex<Vec<GovernanceDecisionReceipt>>,
}

impl GovernanceReceiptBackend for TestReceiptStore {
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String> {
        self.governance.lock().unwrap().push(receipt.clone());
        Ok(())
    }

    fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.proposal_id == proposal_id)
            .cloned())
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
}

// ============================================================================
// Test scaffolding
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
            // Quorum 1 / approval 51 keeps the close path deterministic on a
            // single-member ballot — one yes is enough to accept.
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

async fn open_text_proposal(
    mgr: &GovernanceManager,
    domain: GovernanceDomainId,
    proposer: Did,
    title: &str,
) -> ProposalId {
    let proposal_id = ProposalId(format!("prop-{}", uuid::Uuid::new_v4()));
    mgr.create_proposal(
        proposal_id.clone(),
        domain,
        proposer,
        title.to_string(),
        "A test proposal awaiting the holder's vote.".to_string(),
        ProposalPayload::Text {
            body: "We endorse this fictional statement.".to_string(),
        },
        ProposalScope::Local,
    )
    .await
    .expect("create_proposal");
    mgr.open_proposal(proposal_id.clone(), 86_400)
        .await
        .expect("open_proposal");
    proposal_id
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
async fn vote_action_card_receipt_chain_end_to_end() {
    // standing → action card → authorized action → receipt
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;

    let proposal_id = open_text_proposal(
        &h.ctx.manager,
        domain.clone(),
        caller.clone(),
        "Adopt fictional statement",
    )
    .await;

    // ── 1. Standing: holder is a domain member (not asserted via /me/standing
    //       here — covered by me_standing.rs; we rely on it as a pre-condition
    //       and assert the action-card surface that depends on it).

    // ── 2. Action card: the proposal/vote card exists with receipt_expected=true.
    let app = action_cards_app!(h.ctx.clone(), &caller);
    let (status, body) = fetch_action_cards!(app);
    assert_eq!(status, StatusCode::OK);
    let cards = body["cards"].as_array().expect("cards is an array");
    let vote_card = cards
        .iter()
        .find(|c| c["source_kind"] == "proposal" && c["action_kind"] == "vote")
        .expect("expected a proposal/vote card before the action runs");
    assert_eq!(
        vote_card["source_id"], proposal_id.0,
        "card source_id must point at the proposal whose receipt the holder will receive"
    );
    assert_eq!(
        vote_card["receipt_expected"], true,
        "vote cards advertise that completion produces a receipt — this is the contract under test"
    );
    let card_source_id = vote_card["source_id"].as_str().unwrap().to_string();

    // ── 3. Authorized action: the holder casts a vote.
    h.ctx
        .manager
        .cast_vote(proposal_id.clone(), caller.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote (authorized action)");

    // Closing the proposal is the receipt-bearing transition. In the runtime
    // the close happens via the close handler; the manager call covers the
    // same finalization path used by that handler. The receipt store is
    // wired on the manager, so a successful close persists a
    // GovernanceDecisionReceipt.
    h.ctx
        .manager
        .close_proposal(proposal_id.clone())
        .await
        .expect("close_proposal (receipt-bearing transition)");

    // ── 4. Receipt: the receipt store now contains a GovernanceDecisionReceipt
    //       whose proposal_id is the same string the action card carried as
    //       source_id. This is the link the proof loop pins.
    let receipt = h
        .receipts
        .get_governance_by_proposal(&card_source_id)
        .expect("receipt lookup")
        .expect(
            "expected a GovernanceDecisionReceipt for the proposal — \
             the action card's receipt_expected:true would otherwise be a lie",
        );
    assert_eq!(
        receipt.proposal_id, card_source_id,
        "receipt.proposal_id must equal the card's source_id"
    );
    assert_eq!(
        receipt.domain_id, domain.0,
        "receipt domain must match the domain the card lives under"
    );
    assert_eq!(
        receipt.outcome,
        ProofOutcome::Accepted,
        "single yes-vote with quorum=1 / approval=51 must accept"
    );
    // proof_hash and decision_hash must be populated — these are the layer-1
    // proof binding fields per ADR-0026 §Layer 1.
    assert_ne!(
        receipt.decision_hash, [0u8; 32],
        "decision_hash must be a real cryptographic binding, not a zero placeholder"
    );

    // ── 5. Card freshness: after close, the card derives away.
    let app = action_cards_app!(h.ctx.clone(), &caller);
    let (_, body_after) = fetch_action_cards!(app);
    let post_cards = body_after["cards"].as_array().unwrap();
    let still_voting = post_cards
        .iter()
        .any(|c| c["source_kind"] == "proposal" && c["source_id"] == card_source_id);
    assert!(
        !still_voting,
        "after close, the proposal/vote card for this proposal must not appear — \
         derived views must reflect closed state"
    );
}

#[actix_web::test]
async fn another_did_does_not_see_first_did_receipt_via_action_cards() {
    let h = make_harness();
    let alice = fresh_did();
    let bob = fresh_did();
    assert_ne!(alice, bob);

    let domain = seed_domain_with_member(&h.ctx.manager, &alice, "test-coop").await;
    let proposal_id = open_text_proposal(
        &h.ctx.manager,
        domain.clone(),
        alice.clone(),
        "Alice-domain proposal",
    )
    .await;

    // Alice acts.
    h.ctx
        .manager
        .cast_vote(proposal_id.clone(), alice.clone(), VoteChoice::For, None)
        .await
        .expect("alice cast_vote");
    h.ctx
        .manager
        .close_proposal(proposal_id.clone())
        .await
        .expect("close_proposal");

    // Bob has no membership in alice's domain. His /me/action-cards must not
    // surface a card pointing at alice's proposal id, regardless of the
    // post-close receipt's existence in the shared store.
    let bob_app = action_cards_app!(h.ctx.clone(), &bob);
    let (status, body) = fetch_action_cards!(bob_app);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["did"], bob.to_string());
    let bob_cards = body["cards"].as_array().unwrap();
    let leaks = bob_cards.iter().any(|c| c["source_id"] == proposal_id.0);
    assert!(
        !leaks,
        "bob must not see any card referencing alice's proposal — \
         derived views must respect /me/standing scoping"
    );
}

#[actix_web::test]
async fn vote_card_response_uses_regulatory_safe_vocabulary_through_close() {
    // Whole-loop vocabulary check: card payload before the action and the
    // close path both must avoid payment / wallet / balance / currency.
    let h = make_harness();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&h.ctx.manager, &caller, "test-coop").await;
    let proposal_id = open_text_proposal(
        &h.ctx.manager,
        domain.clone(),
        caller.clone(),
        "Vocabulary-discipline proposal",
    )
    .await;

    let app = action_cards_app!(h.ctx.clone(), &caller);
    let (status, body) = fetch_action_cards!(app);
    assert_eq!(status, StatusCode::OK);
    let raw_lower = body.to_string().to_lowercase();
    for forbidden in ["payment", "wallet", "balance", "currency"] {
        assert!(
            !raw_lower.contains(forbidden),
            "/me/action-cards response must not contain forbidden term '{forbidden}': {raw_lower}"
        );
    }

    h.ctx
        .manager
        .cast_vote(proposal_id.clone(), caller, VoteChoice::For, None)
        .await
        .expect("cast_vote");
    h.ctx
        .manager
        .close_proposal(proposal_id.clone())
        .await
        .expect("close_proposal");
    let receipt = h
        .receipts
        .get_governance_by_proposal(&proposal_id.0)
        .unwrap()
        .expect("receipt expected");
    let serialized = serde_json::to_string(&receipt).unwrap().to_lowercase();
    for forbidden in ["payment", "wallet", "balance", "currency"] {
        assert!(
            !serialized.contains(forbidden),
            "GovernanceDecisionReceipt must not contain forbidden term '{forbidden}': {serialized}"
        );
    }
}
