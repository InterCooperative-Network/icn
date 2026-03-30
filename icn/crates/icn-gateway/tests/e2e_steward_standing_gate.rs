//! E2E proof: steward standing gates SDIS proposal submission.
//!
//! ## What this proves
//!
//! Governance authority is now differentiated by institutional role:
//! - Generic Member standing gates proposal submission and voting (PRs #1457, #1458)
//! - Active steward standing gates SDIS proposal types (AppointSteward, RemoveSteward)
//!
//! A plain Member — even one with valid JWT and active commons affiliation — cannot
//! submit an AppointSteward or RemoveSteward proposal. Only active stewards may
//! propose changes to the steward office.
//!
//! Three paths are exercised for AppointSteward:
//! - `POST /v1/gov/proposals/sdis/appoint-steward` with active steward standing → 201 Created
//! - `POST /v1/gov/proposals/sdis/appoint-steward` from plain Member (no steward record) → 403
//! - `POST /v1/gov/proposals/sdis/appoint-steward` from no-commons actor → 403
//!
//! RemoveSteward is covered by the same gate; one additional 403 test exercises it.

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
    http::configure::{GovernanceContext, MemberStandingChecker, StewardStandingChecker},
    manager::GovernanceManager,
};
use icn_identity::{
    commons::{JurisdictionId, MembershipCapability, MembershipStatus},
    Did, IdentityBundle, KeyPair,
};
use serde_json::{json, Value};
use std::sync::Arc;

const DOMAIN_ID: &str = "steward-gate-test-coop";

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
            "coop_id": "steward-gate-test",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

fn make_member_checker(commons: Arc<CommonsManager>) -> MemberStandingChecker {
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

fn make_steward_checker(commons: Arc<CommonsManager>) -> StewardStandingChecker {
    Arc::new(move |did: Did| {
        let c = commons.clone();
        Box::pin(async move { c.is_active_steward(&did).await.unwrap_or(false) })
    })
}

async fn build_app(
    commons_mgr: Arc<CommonsManager>,
    actor_did: &Did,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let governance_manager = Arc::new(GovernanceManager::new());

    governance_manager
        .create_domain(
            GovernanceDomainId(DOMAIN_ID.to_string()),
            "Steward Gate Test Coop".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![actor_did.clone()]),
            },
        )
        .await
        .expect("create_domain");

    let jwt_secret = b"steward-gate-test-jwt-secret-32b".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

    let gov_ctx = GovernanceContext {
        manager: governance_manager,
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        member_checker: Some(make_member_checker(commons_mgr.clone())),
        steward_checker: Some(make_steward_checker(commons_mgr)),
    };

    let auth_mw = HttpAuthentication::bearer(jwt_auth);
    test::init_service(
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
    .await
}

/// Helper: enroll actor in commons with Member standing in DOMAIN_ID.
async fn enroll_as_member(commons_mgr: &CommonsManager, actor_did: &Did) {
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
        .expect("update to Member");
}

/// Helper: register actor as an active steward in commons.
///
/// Stores a minimal charter for DOMAIN_ID first (required by register_steward
/// jurisdiction validation), then creates the steward record.
async fn enroll_as_steward(commons_mgr: &CommonsManager, actor_did: &Did) -> String {
    // The actor must be a holder first.
    enroll_as_member(commons_mgr, actor_did).await;

    // register_steward validates jurisdiction against the commons charter store.
    // Store a minimal stub charter before calling it.
    use icn_governance::{Charter, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType};
    let charter = Charter::new(
        OrgType::Cooperative,
        DOMAIN_ID.to_string(),
        DOMAIN_ID.to_string(),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );
    commons_mgr
        .store_charter(charter)
        .await
        .expect("store_charter for steward jurisdiction");

    let proposal_id = format!("test-appoint-{}", hex::encode([1u8; 4]));
    commons_mgr
        .register_steward(
            actor_did,
            actor_did,
            365, // term_days
            100, // bond
            proposal_id,
            Some(DOMAIN_ID.to_string()),
            vec![],
        )
        .await
        .expect("register_steward");

    // Return the holder_id for convenience (unused here but available if needed).
    let holder = commons_mgr
        .get_holder_by_did(actor_did)
        .await
        .expect("get_holder")
        .expect("holder must exist");
    hex::encode(holder.id())
}

/// An active steward can submit an AppointSteward proposal.
#[actix_web::test]
async fn test_active_steward_can_submit_appoint_proposal() {
    let commons_mgr = Arc::new(CommonsManager::new());

    let proposer_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let proposer_did = proposer_bundle.did().clone();
    let candidate_bundle = IdentityBundle::generate().expect("candidate bundle");
    let candidate_did = candidate_bundle.did().clone();

    enroll_as_steward(&commons_mgr, &proposer_did).await;

    let app = build_app(commons_mgr, &proposer_did).await;
    let token = get_jwt(&app, &proposer_did.to_string(), &proposer_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals/sdis/appoint-steward")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "domain_id": DOMAIN_ID,
                "title": "Appoint Alice as steward",
                "description": "Alice meets all requirements for the northeast region.",
                "candidate": candidate_did.to_string(),
                "region": "northeast",
                "bond_amount": 100,
                "term_length_seconds": 31_536_000,
                "sponsors": []
            }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        201,
        "Active steward must be able to submit AppointSteward proposal"
    );
}

/// A plain Member (no steward record) is rejected when submitting AppointSteward.
#[actix_web::test]
async fn test_plain_member_cannot_submit_appoint_proposal() {
    let commons_mgr = Arc::new(CommonsManager::new());

    let proposer_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let proposer_did = proposer_bundle.did().clone();
    let candidate_bundle = IdentityBundle::generate().expect("candidate bundle");
    let candidate_did = candidate_bundle.did().clone();

    // Enroll as Member only — not a steward.
    enroll_as_member(&commons_mgr, &proposer_did).await;

    let app = build_app(commons_mgr, &proposer_did).await;
    let token = get_jwt(&app, &proposer_did.to_string(), &proposer_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals/sdis/appoint-steward")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "domain_id": DOMAIN_ID,
                "title": "Unauthorized appointment attempt",
                "description": "Plain members should not be able to do this.",
                "candidate": candidate_did.to_string(),
                "region": "northeast",
                "bond_amount": 100,
                "term_length_seconds": 31_536_000,
                "sponsors": []
            }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        403,
        "Plain Member must be rejected with 403 Forbidden"
    );
}

/// An actor with no commons record is rejected when submitting AppointSteward.
#[actix_web::test]
async fn test_non_member_cannot_submit_appoint_proposal() {
    let commons_mgr = Arc::new(CommonsManager::new());

    let proposer_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let proposer_did = proposer_bundle.did().clone();
    let candidate_bundle = IdentityBundle::generate().expect("candidate bundle");
    let candidate_did = candidate_bundle.did().clone();

    // No commons enrollment at all.
    let app = build_app(commons_mgr, &proposer_did).await;
    let token = get_jwt(&app, &proposer_did.to_string(), &proposer_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals/sdis/appoint-steward")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "domain_id": DOMAIN_ID,
                "title": "No-standing appointment attempt",
                "description": "No commons standing at all.",
                "candidate": candidate_did.to_string(),
                "region": "northeast",
                "bond_amount": 100,
                "term_length_seconds": 31_536_000,
                "sponsors": []
            }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        403,
        "Actor without commons standing must be rejected with 403 Forbidden"
    );
}

/// A plain Member is also rejected for RemoveSteward proposals.
#[actix_web::test]
async fn test_plain_member_cannot_submit_remove_proposal() {
    let commons_mgr = Arc::new(CommonsManager::new());

    let proposer_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let proposer_did = proposer_bundle.did().clone();
    let target_bundle = IdentityBundle::generate().expect("target bundle");
    let target_did = target_bundle.did().clone();

    enroll_as_member(&commons_mgr, &proposer_did).await;

    let app = build_app(commons_mgr, &proposer_did).await;
    let token = get_jwt(&app, &proposer_did.to_string(), &proposer_bundle).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals/sdis/remove-steward")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "domain_id": DOMAIN_ID,
                "title": "Unauthorized removal attempt",
                "description": "Plain members should not be able to remove stewards.",
                "steward": target_did.to_string(),
                "reason": "Unauthorized test",
                "return_bond": false
            }))
            .to_request(),
    )
    .await;

    assert_eq!(
        resp.status().as_u16(),
        403,
        "Plain Member must be rejected for RemoveSteward proposal"
    );
}
