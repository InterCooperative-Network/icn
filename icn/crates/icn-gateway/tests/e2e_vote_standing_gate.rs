//! E2E proof: commons Member standing gates governance voting.
//!
//! ## What this proves
//!
//! Voting is now standing-gated in the same way as proposal submission.
//! A caller with a valid JWT but no active Member affiliation in the proposal's
//! domain jurisdiction is rejected with 403 Forbidden before the vote reaches
//! the governance manager.
//!
//! Three paths are exercised:
//! - `POST /v1/gov/proposals/{id}/vote` with Member standing → 200 OK
//! - `POST /v1/gov/proposals/{id}/vote` with no commons standing → 403 Forbidden
//! - `POST /v1/gov/proposals/{id}/vote` while Suspended → 403 Forbidden
//!
//! Proposals are created via the GovernanceManager directly (bypassing the
//! submission gate). This isolates the vote gate as the sole seam under test.

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

const DOMAIN_ID: &str = "vote-gate-test-coop";

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
            "coop_id": "vote-gate-test",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

fn make_checker(commons: Arc<CommonsManager>) -> MemberStandingChecker {
    Arc::new(move |did: Did, domain_id: String| {
        use icn_identity::commons::{JurisdictionId, MembershipStatus};
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

/// Build the test app and seed a single open proposal for the given actor.
///
/// The proposal is created via the manager (not HTTP) so the submission gate
/// is bypassed. Returns (app, proposal_id).
async fn build_app_with_open_proposal(
    commons_mgr: Arc<CommonsManager>,
    actor_did: &Did,
) -> (
    impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    String,
) {
    let governance_manager = Arc::new(GovernanceManager::new());

    governance_manager
        .create_domain(
            GovernanceDomainId(DOMAIN_ID.to_string()),
            "Vote Gate Test Coop".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![actor_did.clone()]),
            },
        )
        .await
        .expect("create_domain");

    let proposal_id_val = ProposalId("vote-gate-prop-1".to_string());
    governance_manager
        .create_proposal(
            proposal_id_val.clone(),
            GovernanceDomainId(DOMAIN_ID.to_string()),
            actor_did.clone(),
            "Motion under vote-gate test".to_string(),
            "Testing that voting is standing-gated.".to_string(),
            ProposalPayload::Text {
                body: "Vote gate test proposal body.".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    // Open the proposal via the manager so votes can be cast.
    governance_manager
        .open_proposal(proposal_id_val.clone(), 3600)
        .await
        .expect("open_proposal");

    let jwt_secret = b"vote-gate-test-jwt-secret-32bytes".to_vec();
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

    (app, proposal_id_val.0)
}

/// A voter with active Member standing can cast a vote.
#[actix_web::test]
async fn test_member_with_standing_can_vote() {
    let commons_mgr = Arc::new(CommonsManager::new());

    let actor_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let actor_did = actor_bundle.did().clone();

    let voucher_kp = KeyPair::generate().expect("voucher KeyPair");
    let voucher_did = voucher_kp.did().clone();

    let anchor = commons_mgr
        .create_anchor_from_enrollment(&actor_did, Some(&voucher_did))
        .await
        .expect("create_anchor_from_enrollment");
    let anchor_id = hex::encode(anchor.id());

    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &actor_did)
        .await
        .expect("create_holder_from_anchor");
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
        .expect("update_affiliation_status to Member");

    let (app, proposal_id) = build_app_with_open_proposal(commons_mgr, &actor_did).await;
    let token = get_jwt(&app, &actor_did.to_string(), &actor_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/vote"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "choice": "for" }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        200,
        "Member with active standing must be able to vote"
    );
}

/// An authenticated actor with no commons standing cannot vote.
#[actix_web::test]
async fn test_non_member_without_standing_cannot_vote() {
    let commons_mgr = Arc::new(CommonsManager::new());

    let actor_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let actor_did = actor_bundle.did().clone();

    // Actor has JWT but no commons record.
    let (app, proposal_id) = build_app_with_open_proposal(commons_mgr, &actor_did).await;
    let token = get_jwt(&app, &actor_did.to_string(), &actor_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/vote"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "choice": "for" }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        403,
        "Actor without commons standing must be rejected with 403"
    );
}

/// A suspended member cannot vote.
#[actix_web::test]
async fn test_suspended_member_cannot_vote() {
    let commons_mgr = Arc::new(CommonsManager::new());

    let actor_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let actor_did = actor_bundle.did().clone();

    let voucher_kp = KeyPair::generate().expect("voucher KeyPair");
    let voucher_did = voucher_kp.did().clone();

    let anchor = commons_mgr
        .create_anchor_from_enrollment(&actor_did, Some(&voucher_did))
        .await
        .expect("create_anchor_from_enrollment");
    let anchor_id = hex::encode(anchor.id());

    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &actor_did)
        .await
        .expect("create_holder_from_anchor");
    let holder_id = hex::encode(holder.id());

    commons_mgr
        .join_jurisdiction(
            &holder_id,
            JurisdictionId::new(DOMAIN_ID),
            vec![MembershipCapability::Vote],
        )
        .await
        .expect("join_jurisdiction");

    // Simulate FreezeMember outcome.
    commons_mgr
        .update_affiliation_status(
            &holder_id,
            &JurisdictionId::new(DOMAIN_ID),
            MembershipStatus::Suspended,
        )
        .await
        .expect("update_affiliation_status to Suspended");

    let (app, proposal_id) = build_app_with_open_proposal(commons_mgr, &actor_did).await;
    let token = get_jwt(&app, &actor_did.to_string(), &actor_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/vote"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "choice": "for" }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        403,
        "Suspended member must be rejected with 403"
    );
}
