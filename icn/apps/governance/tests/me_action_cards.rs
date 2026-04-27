//! `GET /v1/gov/me/action-cards` integration tests (issue icn#1646).
//!
//! Proves the first runtime slice of the action-cards surface defined by
//! ADR-0027 and seated on top of the `/me/standing` activation chain
//! recorded in ADR-0020 (step 7).
//!
//! 1. A holder with mixed standing inputs — one open proposal in their
//!    domain, one scheduled meeting where they are an attendee, one
//!    assigned action item — receives three cards with the expected
//!    `source_kind` and `action_kind` values, the underlying `source_id`,
//!    and a deterministic id.
//! 2. The response payload uses regulatory-safe vocabulary (no payment /
//!    currency / balance / wallet terms).
//! 3. A different DID does not see the first DID's cards.
//! 4. The reserved `signal_rule` and `obligation_lifecycle` source kinds
//!    are not emitted today.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{body::to_bytes, dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance::{
    ActionItemPriority, AttendanceStatus, GovernanceDomainId, GovernanceParams, MeetingAttendee,
    MeetingRole, MembershipConfig, MembershipSource, ProposalId, ProposalPayload, ProposalScope,
};
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use serde_json::Value;

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

fn make_ctx() -> GovernanceContext<NoopEventEmitter> {
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
    }
}

/// Stand up a domain with `caller` in the static member list. Returns the
/// domain id so callers can pin proposals/action items into it.
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

macro_rules! action_cards_test_app {
    ($ctx:expr, $caller_did:expr, $scope:expr) => {{
        let scope: Option<&'static str> = $scope;
        let caller = $caller_did.to_string();
        test::init_service(
            App::new()
                .wrap_fn(move |req, srv| {
                    if let Some(scope_str) = scope {
                        req.extensions_mut().insert(BasicClaims {
                            sub: caller.clone(),
                            scope: Some(scope_str.to_string()),
                        });
                    }
                    srv.call(req)
                })
                .configure(|cfg| http::configure(cfg, $ctx)),
        )
        .await
    }};
}

macro_rules! fetch_action_cards {
    ($app:expr, $uri:expr) => {{
        let req = test::TestRequest::get().uri($uri).to_request();
        let resp = test::call_service(&$app, req).await;
        let status = resp.status();
        let bytes = to_bytes(resp.into_body()).await.unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).expect("response is valid JSON");
        (status, parsed)
    }};
}

#[actix_web::test]
async fn caller_with_no_inputs_returns_well_formed_empty_card_set() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let app = action_cards_test_app!(ctx, &caller, Some("governance:read"));

    let (status, body) = fetch_action_cards!(app, "/me/action-cards");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["did"], caller.to_string());
    assert!(
        body["cards"].as_array().unwrap().is_empty(),
        "no inputs should produce no cards, not an error"
    );
    assert!(body["generated_at"].as_u64().is_some());
}

#[actix_web::test]
async fn mixed_inputs_produce_one_card_per_source() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&ctx.manager, &caller, "test-coop").await;

    // ── Source 1: open proposal in caller's domain ────────────────────────
    let proposal_id = ProposalId(format!("prop-{}", uuid::Uuid::new_v4()));
    ctx.manager
        .create_proposal(
            proposal_id.clone(),
            domain.clone(),
            caller.clone(),
            "Adopt fictional statement".to_string(),
            "A test proposal awaiting the holder's vote.".to_string(),
            ProposalPayload::Text {
                body: "We endorse this fictional statement.".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");
    ctx.manager
        .open_proposal(proposal_id.clone(), 86_400)
        .await
        .expect("open_proposal");

    // ── Source 2: meeting in next 48h with caller on attendee list ───────
    // The digest helper looks at the meeting store's `list_upcoming` window
    // measured against the wall clock; scheduling at +1h covers any test
    // jitter without overshooting the 48h bound.
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let meeting = ctx
        .manager
        .create_meeting(
            domain.0.clone(),
            "Quarterly review (fictional)".to_string(),
            None,
            Some(now_secs + 3_600),
            "did:icn:organizer".to_string(),
        )
        .expect("create_meeting");
    let mut updated = meeting.clone();
    updated.attendees = vec![MeetingAttendee {
        did: caller.as_str().to_owned(),
        status: AttendanceStatus::Invited,
        meeting_role: MeetingRole::Participant,
    }];
    ctx.manager
        .update_meeting(&updated)
        .expect("update_meeting");

    // ── Source 3: action item assigned to caller ─────────────────────────
    let item = ctx
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

    // ── Exercise the endpoint ────────────────────────────────────────────
    let app = action_cards_test_app!(ctx, &caller, Some("governance:read"));
    let (status, body) = fetch_action_cards!(app, "/me/action-cards");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["did"], caller.to_string());

    let cards = body["cards"].as_array().expect("cards is an array");
    assert_eq!(
        cards.len(),
        3,
        "expected one card per source (proposal + meeting + action_item); got {cards:?}"
    );

    // Index by source_kind so order-independence is explicit.
    let by_source: std::collections::HashMap<&str, &Value> = cards
        .iter()
        .map(|c| (c["source_kind"].as_str().unwrap(), c))
        .collect();

    let proposal_card = by_source
        .get("proposal")
        .expect("expected a proposal-source card");
    assert_eq!(proposal_card["action_kind"], "vote");
    assert_eq!(proposal_card["scope"], "entity");
    assert_eq!(proposal_card["source_id"], proposal_id.0);
    assert_eq!(proposal_card["domain_id"], domain.0);
    assert_eq!(proposal_card["receipt_expected"], true);
    assert_eq!(proposal_card["risk_level"], "normal");
    assert_eq!(
        proposal_card["id"],
        format!("card-proposal-{}-vote", proposal_id.0),
        "proposal-card id must be deterministic per ADR-0027"
    );

    let meeting_card = by_source
        .get("meeting")
        .expect("expected a meeting-source card");
    assert_eq!(meeting_card["action_kind"], "attend");
    assert_eq!(meeting_card["scope"], "structure");
    assert_eq!(meeting_card["source_id"], meeting.id.0);
    assert!(
        meeting_card["deadline"].as_u64().unwrap() >= now_secs,
        "scheduled meeting deadline must be in the future"
    );
    assert_eq!(
        meeting_card["receipt_expected"], true,
        "meeting/attend now emits a MeetingAttendanceReceipt on Present/Remote transitions"
    );

    let item_card = by_source
        .get("action_item")
        .expect("expected an action_item-source card");
    assert_eq!(item_card["action_kind"], "complete");
    assert_eq!(item_card["scope"], "individual");
    assert_eq!(item_card["source_id"], item.id.to_string());
    assert_eq!(item_card["receipt_expected"], true);

    // Reserved variants must NOT appear in this slice (icn#1646 leaves them
    // unimplemented; icn#1631 and icn#1634 are the gating issues).
    for source in ["signal_rule", "obligation_lifecycle"] {
        assert!(
            !by_source.contains_key(source),
            "source_kind '{source}' is reserved by icn#1646 and must not be emitted today"
        );
    }
}

#[actix_web::test]
async fn response_uses_regulatory_safe_vocabulary() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let domain = seed_domain_with_member(&ctx.manager, &caller, "test-coop").await;

    // One proposal is enough to populate the response with prose fields
    // (title, summary, authority_basis, accessibility_hint).
    let proposal_id = ProposalId(format!("prop-{}", uuid::Uuid::new_v4()));
    ctx.manager
        .create_proposal(
            proposal_id.clone(),
            domain.clone(),
            caller.clone(),
            "Vocabulary check (fictional)".to_string(),
            "Make sure the action-cards surface stays away from regulated-finance vocabulary."
                .to_string(),
            ProposalPayload::Text {
                body: "endorse".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");
    ctx.manager
        .open_proposal(proposal_id, 86_400)
        .await
        .expect("open_proposal");

    let app = action_cards_test_app!(ctx, &caller, Some("governance:read"));
    let (status, body) = fetch_action_cards!(app, "/me/action-cards");
    assert_eq!(status, StatusCode::OK);
    let raw_lower = body.to_string().to_lowercase();
    for forbidden in ["payment", "wallet", "balance", "currency"] {
        assert!(
            !raw_lower.contains(forbidden),
            "/me/action-cards response must not contain forbidden term '{forbidden}': {raw_lower}"
        );
    }
}

#[actix_web::test]
async fn caller_does_not_see_another_dids_cards() {
    let ctx = make_ctx();
    let alice = fresh_did();
    let bob = fresh_did();
    assert_ne!(alice, bob);

    let domain = seed_domain_with_member(&ctx.manager, &alice, "test-coop").await;
    let _ = ctx
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

    // Bob hits the endpoint; he gets *his* (empty) card set, not Alice's.
    let app = action_cards_test_app!(ctx, &bob, Some("governance:read"));
    let (status, body) = fetch_action_cards!(app, "/me/action-cards");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["did"], bob.to_string());
    assert!(
        body["cards"].as_array().unwrap().is_empty(),
        "bob must not inherit alice's assigned cards"
    );
}
