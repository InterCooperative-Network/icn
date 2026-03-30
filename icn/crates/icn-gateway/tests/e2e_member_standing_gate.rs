//! E2E proof: commons Member standing gates governance proposal submission.
//!
//! ## What this proves
//!
//! The commons-to-governance direction of institutional authority binding:
//! a caller with a valid JWT but no active Member standing in the target domain's
//! jurisdiction is rejected with 403 Forbidden before the proposal reaches the
//! governance manager.
//!
//! Specifically:
//! - `POST /v1/gov/proposals` with Member standing → 201 Created
//! - `POST /v1/gov/proposals` without commons standing → 403 Forbidden
//!
//! This is the reverse of the governance→commons direction proven in
//! `e2e_institutional_flow.rs`. Together they close the bidirectional loop:
//! governance effects mutate commons state, and commons state constrains
//! governance authority.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use icn_gateway::{
    api, auth::AuthManager, commons_mgr::CommonsManager, middleware::jwt_auth,
    rate_limit::IpRateLimiter,
};
use icn_governance::{GovernanceDomainId, GovernanceParams, MembershipConfig, MembershipSource};
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

const DOMAIN_ID: &str = "standing-gate-test-coop";

/// Auth helper reused from e2e_institutional_flow.
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
            "coop_id": "standing-gate-test",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

/// Build a commons-backed MemberStandingChecker for the test.
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

/// Shared test app builder used by both tests.
///
/// `actor_did` is added to the governance domain's StaticList so governance
/// itself never rejects them. The only gate under test is the commons checker.
async fn build_app(
    commons_mgr: Arc<CommonsManager>,
    governance_manager: Arc<GovernanceManager>,
    actor_did: &Did,
) -> (
    impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    Arc<AuthManager>,
) {
    governance_manager
        .create_domain(
            GovernanceDomainId(DOMAIN_ID.to_string()),
            "Standing Gate Test Coop".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![actor_did.clone()]),
            },
        )
        .await
        .expect("create_domain");

    let jwt_secret = b"standing-gate-test-jwt-secret-32b".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

    let gov_ctx = GovernanceContext {
        manager: governance_manager.clone(),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        member_checker: Some(make_checker(commons_mgr)),
        steward_checker: None,
    };

    let auth_mw = HttpAuthentication::bearer(jwt_auth);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_manager.clone()))
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

    (app, auth_manager)
}

/// A proposer with active Member standing may submit proposals.
#[actix_web::test]
async fn test_member_with_standing_can_submit_proposal() {
    let commons_mgr = Arc::new(CommonsManager::new());
    let governance_manager = Arc::new(GovernanceManager::new());

    // Enroll the actor in commons and join the jurisdiction as Member.
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

    // join_jurisdiction starts at Candidate. Advance to Member to simulate
    // a fully onboarded cooperative member (the state FreezeMember targets,
    // and the state UnfreezeMember restores).
    commons_mgr
        .update_affiliation_status(
            &holder_id,
            &JurisdictionId::new(DOMAIN_ID),
            MembershipStatus::Member,
        )
        .await
        .expect("update_affiliation_status to Member");

    let (app, _) = build_app(commons_mgr, governance_manager, &actor_did).await;
    let token = get_jwt(&app, &actor_did.to_string(), &actor_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "domain_id": DOMAIN_ID,
                "title": "Motion to adopt cooperative principles",
                "description": "Adopt the ICA cooperative principles as governance baseline.",
                "payload": { "type": "text", "body": "We adopt the ICA principles." }
            }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        201,
        "Member with active standing must be able to submit a proposal"
    );
}

/// A caller authenticated via JWT but with no commons standing is rejected 403.
#[actix_web::test]
async fn test_non_member_without_standing_is_rejected() {
    let commons_mgr = Arc::new(CommonsManager::new());
    let governance_manager = Arc::new(GovernanceManager::new());

    // Actor has a valid identity and JWT but is NOT enrolled in commons.
    let actor_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let actor_did = actor_bundle.did().clone();

    let (app, _) = build_app(commons_mgr, governance_manager, &actor_did).await;
    let token = get_jwt(&app, &actor_did.to_string(), &actor_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "domain_id": DOMAIN_ID,
                "title": "Illegitimate motion",
                "description": "Submitted by a non-member attempting to influence governance.",
                "payload": { "type": "text", "body": "This should be rejected." }
            }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        403,
        "Actor without commons Member standing must be rejected with 403 Forbidden"
    );
}

/// A formerly-Member actor whose affiliation is Suspended is also rejected.
#[actix_web::test]
async fn test_suspended_member_is_rejected() {
    let commons_mgr = Arc::new(CommonsManager::new());
    let governance_manager = Arc::new(GovernanceManager::new());

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

    // Simulate a FreezeMember outcome: affiliation becomes Suspended.
    commons_mgr
        .update_affiliation_status(
            &holder_id,
            &JurisdictionId::new(DOMAIN_ID),
            MembershipStatus::Suspended,
        )
        .await
        .expect("update_affiliation_status to Suspended");

    let (app, _) = build_app(commons_mgr, governance_manager, &actor_did).await;
    let token = get_jwt(&app, &actor_did.to_string(), &actor_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "domain_id": DOMAIN_ID,
                "title": "Motion from a suspended member",
                "description": "This should not be accepted.",
                "payload": { "type": "text", "body": "Suspended members cannot submit proposals." }
            }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        403,
        "Suspended member must be rejected with 403 Forbidden"
    );
}
