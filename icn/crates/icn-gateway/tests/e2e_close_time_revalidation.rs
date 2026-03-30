//! E2E proof: close-time standing revalidation (lifecycle legitimacy).
//!
//! ## What this proves
//!
//! Prior to this change, standing gates were snapshot-only: commons membership
//! was checked at vote-cast time, but never revalidated when the proposal
//! resolved. A member suspended *after* voting still had their vote counted in
//! the final tally — a constitutional contradiction (governance resolving with
//! evidence from actors who were no longer institutionally legitimate).
//!
//! This file proves that `close_proposal` now revalidates active commons Member
//! standing for every voter at resolution time. Votes from suspended or expelled
//! members are excluded from the effective tally.
//!
//! ## Setup
//!
//! Three-member domain (Alice, Bob, Charlie):
//! - quorum = 67% (requires ≥ 2 of 3 members to vote)
//! - approval = 50%
//!
//! ## Cases exercised
//!
//! 1. **All standing valid at close** (`test_valid_standing_throughout_resolves_normally`)
//!    Alice:FOR, Bob:FOR, Charlie:AGAINST — all still Member at close.
//!    Effective tally: 3 votes, 100% quorum, 66% approval ≥ 50% → Accepted.
//!
//! 2. **Voter suspended before close** (`test_suspended_voter_excluded_at_close`)
//!    Same votes cast. Bob gets Suspended before close.
//!    Effective tally: Alice:FOR + Charlie:AGAINST = 2 votes, 2/3 = 66% quorum < 67% → NoQuorum.
//!    Bob's vote is excluded. The proposal fails to meet quorum because its effective
//!    participant set shrank when institutional legitimacy was revoked mid-flight.
//!
//! Together these two tests establish the first lifecycle legitimacy seam:
//! standing is a continuous predicate across the full decision arc, not a
//! one-time entry gate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use icn_gateway::{
    api, auth::AuthManager, commons_mgr::CommonsManager, middleware::jwt_auth,
    rate_limit::IpRateLimiter,
};
use icn_governance::{
    GovernanceDomainId, GovernanceParams, MembershipConfig, MembershipSource, ProposalId,
    ProposalPayload, ProposalScope,
};
use icn_governance_actor::{
    events::NoopEventEmitter,
    http::configure::{GovernanceContext, MemberStandingChecker},
    manager::GovernanceManager,
};
use icn_identity::{
    commons::{JurisdictionId, MembershipCapability, MembershipStatus},
    Did, IdentityBundle, KeyPair,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// Cooperative domain ID used across all tests.
const DOMAIN_ID: &str = "close-revalidation-test-coop";

/// Obtain a signed JWT for `did` via the test app's auth endpoints.
async fn get_jwt(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    did: &str,
    bundle: &IdentityBundle,
) -> String {
    let challenge_req = test::TestRequest::post()
        .uri("/v1/auth/challenge")
        .set_json(json!({ "did": did }))
        .to_request();
    let challenge_resp: Value = test::call_and_read_body_json(app, challenge_req).await;
    let nonce = challenge_resp["nonce"].as_str().expect("nonce").to_string();

    let nonce_bytes = hex::decode(&nonce).expect("hex nonce");
    let signature = bundle.sign(&nonce_bytes).expect("sign");
    let sig_hex = hex::encode(signature.to_bytes());

    let verify_req = test::TestRequest::post()
        .uri("/v1/auth/verify")
        .set_json(json!({
            "did": did,
            "signature": sig_hex,
            "coop_id": "close-revalidation-test",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

/// Build a commons-backed MemberStandingChecker.
fn make_checker(commons: Arc<CommonsManager>) -> MemberStandingChecker {
    Arc::new(move |did: Did, domain_id: String| {
        let c = commons.clone();
        Box::pin(async move {
            let Ok(Some(holder)) = c.get_holder_by_did(&did).await else {
                return false;
            };
            let holder_id = hex::encode(holder.id());
            let Ok(affiliations) = c.list_affiliations(&holder_id).await else {
                return false;
            };
            affiliations.iter().any(|a| {
                a.jurisdiction_id == JurisdictionId::new(&domain_id)
                    && a.membership_status == MembershipStatus::Member
            })
        })
    })
}

/// Enroll `actor_did` in commons as an active Member in DOMAIN_ID.
///
/// Returns the holder_id (hex-encoded) for subsequent standing updates.
async fn enroll_as_member(commons_mgr: &CommonsManager, actor_did: &Did) -> String {
    let voucher_kp = KeyPair::generate().expect("voucher KeyPair");
    let voucher_did = voucher_kp.did().clone();

    let anchor = commons_mgr
        .create_anchor_from_enrollment(actor_did, Some(&voucher_did))
        .await
        .expect("create_anchor");
    let anchor_id = hex::encode(anchor.id());

    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, actor_did)
        .await
        .expect("create_holder");
    let holder_id = hex::encode(holder.id());

    commons_mgr
        .join_jurisdiction(
            &holder_id,
            JurisdictionId::new(DOMAIN_ID),
            vec![MembershipCapability::Vote],
        )
        .await
        .expect("join_jurisdiction");

    commons_mgr
        .update_affiliation_status(
            &holder_id,
            &JurisdictionId::new(DOMAIN_ID),
            MembershipStatus::Member,
        )
        .await
        .expect("advance to Member");

    holder_id
}

/// Build the test app with a commons-backed standing checker.
///
/// The governance domain has 3 members in a StaticList so the governance layer
/// itself never rejects anyone. The commons checker is the sole gate under test.
///
/// `all_members` must be the complete StaticList for the governance domain.
async fn build_app_with_proposal(
    commons_mgr: Arc<CommonsManager>,
    governance_manager: Arc<GovernanceManager>,
    all_members: Vec<Did>,
    proposal_id_str: &str,
    proposer_did: Did,
) -> (
    impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    String, // proposal_id
) {
    // quorum=67 means ≥2/3 voters must participate.
    // approval=50 means ≥50% of deciding votes must be FOR.
    governance_manager
        .create_domain(
            GovernanceDomainId(DOMAIN_ID.to_string()),
            "Close Revalidation Test Coop".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(67, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(all_members),
            },
        )
        .await
        .expect("create_domain");

    let proposal_id = ProposalId(proposal_id_str.to_string());
    governance_manager
        .create_proposal(
            proposal_id.clone(),
            GovernanceDomainId(DOMAIN_ID.to_string()),
            proposer_did,
            "Motion under lifecycle legitimacy test".to_string(),
            "Testing that vote tally is revalidated at close time.".to_string(),
            ProposalPayload::Text {
                body: "Test proposal body.".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    governance_manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    let jwt_secret = b"close-revalidation-jwt-secret-32b".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

    let gov_ctx = GovernanceContext {
        manager: governance_manager,
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        member_checker: Some(make_checker(commons_mgr)),
        steward_checker: None,
    };

    let auth_mw = HttpAuthentication::bearer(jwt_auth);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_manager))
            .app_data(web::Data::new(ip_limiter))
            .service(
                web::scope("/v1")
                    .service(api::auth::challenge)
                    .service(api::auth::verify)
                    .service(
                        web::scope("/gov")
                            .configure({
                                let ctx = gov_ctx.clone();
                                move |cfg| {
                                    icn_governance_actor::http::configure::configure(
                                        cfg,
                                        ctx.clone(),
                                    )
                                }
                            })
                            .wrap(auth_mw),
                    ),
            ),
    )
    .await;

    (app, proposal_id.0)
}

/// Cast a vote via HTTP. Returns the response status.
async fn cast_vote(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    proposal_id: &str,
    token: &str,
    choice: &str,
) -> u16 {
    let resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/vote"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "choice": choice }))
            .to_request(),
    )
    .await;
    resp.status().as_u16()
}

/// When all voters retain Member standing through resolution, the proposal
/// resolves on its full effective tally.
///
/// Setup: Alice:FOR, Bob:FOR, Charlie:AGAINST. All still Member at close.
/// Tally: 3 votes / 3 members = 100% quorum (≥67%). FOR=2, AGAINST=1 → 66%
/// approval (≥50%). Expected outcome: Accepted.
#[actix_web::test]
async fn test_valid_standing_throughout_resolves_normally() {
    let commons_mgr = Arc::new(CommonsManager::new());

    let alice_bundle = IdentityBundle::generate().expect("alice");
    let alice_did = alice_bundle.did().clone();
    let bob_bundle = IdentityBundle::generate().expect("bob");
    let bob_did = bob_bundle.did().clone();
    let charlie_bundle = IdentityBundle::generate().expect("charlie");
    let charlie_did = charlie_bundle.did().clone();

    enroll_as_member(&commons_mgr, &alice_did).await;
    enroll_as_member(&commons_mgr, &bob_did).await;
    enroll_as_member(&commons_mgr, &charlie_did).await;

    let governance_manager = Arc::new(GovernanceManager::new());
    let all_members = vec![alice_did.clone(), bob_did.clone(), charlie_did.clone()];
    let (app, proposal_id) = build_app_with_proposal(
        commons_mgr.clone(),
        governance_manager,
        all_members,
        "lifecycle-valid-prop-1",
        alice_did.clone(),
    )
    .await;

    let alice_token = get_jwt(&app, &alice_did.to_string(), &alice_bundle).await;
    let bob_token = get_jwt(&app, &bob_did.to_string(), &bob_bundle).await;
    let charlie_token = get_jwt(&app, &charlie_did.to_string(), &charlie_bundle).await;

    assert_eq!(
        cast_vote(&app, &proposal_id, &alice_token, "for").await,
        200
    );
    assert_eq!(cast_vote(&app, &proposal_id, &bob_token, "for").await, 200);
    assert_eq!(
        cast_vote(&app, &proposal_id, &charlie_token, "against").await,
        200
    );

    // All three still have Member standing — no standing change before close.
    let close_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/close"))
            .insert_header(("Authorization", format!("Bearer {alice_token}")))
            .to_request(),
    )
    .await;

    assert_eq!(
        close_resp.status().as_u16(),
        200,
        "close_proposal must succeed"
    );
    let body: Value = test::read_body_json(close_resp).await;
    // ProposalState uses external tagging: {"Accepted": {"closed_at": ...}}
    assert!(
        body["state"].get("Accepted").is_some(),
        "Proposal must be Accepted when all voters retain standing: \
         3/3 quorum (100% ≥ 67%), 2:1 FOR:AGAINST (66% ≥ 50%). \
         Actual state: {}",
        body["state"]
    );
}

/// When a voter loses commons Member standing before `close_proposal`, their
/// vote is excluded from the effective tally, potentially changing the outcome.
///
/// Setup: Alice:FOR, Bob:FOR, Charlie:AGAINST. Bob is Suspended before close.
/// Effective tally: Alice:FOR + Charlie:AGAINST = 2 votes / 3 members = 66%
/// quorum. 66 < 67 → NoQuorum. Bob's FOR vote does not count.
///
/// Without lifecycle revalidation, the outcome would be Accepted (100% quorum,
/// 66% approval). With revalidation it becomes NoQuorum — proving that the
/// system no longer gives the benefit of stale standing to suspended actors.
#[actix_web::test]
async fn test_suspended_voter_excluded_at_close() {
    let commons_mgr = Arc::new(CommonsManager::new());

    let alice_bundle = IdentityBundle::generate().expect("alice");
    let alice_did = alice_bundle.did().clone();
    let bob_bundle = IdentityBundle::generate().expect("bob");
    let bob_did = bob_bundle.did().clone();
    let charlie_bundle = IdentityBundle::generate().expect("charlie");
    let charlie_did = charlie_bundle.did().clone();

    let alice_holder = enroll_as_member(&commons_mgr, &alice_did).await;
    let bob_holder = enroll_as_member(&commons_mgr, &bob_did).await;
    let charlie_holder = enroll_as_member(&commons_mgr, &charlie_did).await;
    // suppress unused warning — charlie_holder used only to confirm enrollment
    let _ = (alice_holder, charlie_holder);

    let governance_manager = Arc::new(GovernanceManager::new());
    let all_members = vec![alice_did.clone(), bob_did.clone(), charlie_did.clone()];
    let (app, proposal_id) = build_app_with_proposal(
        commons_mgr.clone(),
        governance_manager,
        all_members,
        "lifecycle-suspended-prop-1",
        alice_did.clone(),
    )
    .await;

    let alice_token = get_jwt(&app, &alice_did.to_string(), &alice_bundle).await;
    let bob_token = get_jwt(&app, &bob_did.to_string(), &bob_bundle).await;
    let charlie_token = get_jwt(&app, &charlie_did.to_string(), &charlie_bundle).await;

    // All three cast votes while Bob still has Member standing.
    assert_eq!(
        cast_vote(&app, &proposal_id, &alice_token, "for").await,
        200
    );
    assert_eq!(cast_vote(&app, &proposal_id, &bob_token, "for").await, 200);
    assert_eq!(
        cast_vote(&app, &proposal_id, &charlie_token, "against").await,
        200
    );

    // Bob loses standing before close (e.g., FreezeMember proposal accepted
    // on a concurrent governance thread). The close handler must revalidate.
    commons_mgr
        .update_affiliation_status(
            &bob_holder,
            &JurisdictionId::new(DOMAIN_ID),
            MembershipStatus::Suspended,
        )
        .await
        .expect("suspend Bob");

    let close_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/close"))
            .insert_header(("Authorization", format!("Bearer {alice_token}")))
            .to_request(),
    )
    .await;

    assert_eq!(
        close_resp.status().as_u16(),
        200,
        "close_proposal must succeed even when some votes are excluded"
    );
    let body: Value = test::read_body_json(close_resp).await;
    // ProposalState uses external tagging: {"NoQuorum": {"closed_at": ...}}
    assert!(
        body["state"].get("NoQuorum").is_some(),
        "Proposal must resolve as NoQuorum when Bob's vote is excluded: \
         2 eligible votes / 3 members = 66% quorum < 67% threshold. \
         Without lifecycle revalidation this would incorrectly be Accepted. \
         Actual state: {}",
        body["state"]
    );
}
