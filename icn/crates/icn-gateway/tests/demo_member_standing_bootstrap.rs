//! Gap B proof: a LOCAL/DEMO-only Member-standing bootstrap unlocks governed
//! proposal submission through the live `member_checker` gate.
//!
//! ## What this proves
//!
//! The gateway gates governed proposal submission on commons Member standing
//! (`GovernanceContext::member_checker`, wired in `server.rs` to `CommonsManager`).
//! A freshly generated organizer DID is rejected with 403 until it holds active
//! `MembershipStatus::Member` standing in the target domain's jurisdiction.
//!
//! [`bootstrap_demo_member_standing`] establishes that standing for a single DID
//! using the *existing* internal `CommonsManager` enrollment + jurisdiction APIs —
//! the exact sequence already proven in `e2e_member_standing_gate.rs`. It is the
//! reusable precondition a future NYCN v4 receipt-chain demo needs.
//!
//! Specifically:
//! - `POST /v1/gov/proposals` (budget payload) before bootstrap → 403 Forbidden
//! - `POST /v1/gov/proposals` (budget payload) after  bootstrap → 201 Created
//!
//! ## Why this is NOT production enrollment
//!
//! The real production path to `Member` standing is the multi-steward SDIS
//! proof-of-personhood ceremony (`/v1/sdis/enrollment/{start,finalize,approve}`),
//! whose `approve` step is itself already dev-gated by `ICN_ENABLE_ADMIN_ENDPOINTS`.
//! This helper deliberately bypasses that ceremony and is therefore confined to
//! test/demo code (it lives in `tests/`, ships in no library, and adds no HTTP
//! route). It does not weaken production auth, SDIS, or membership semantics, and
//! it introduces no public self-service "make me a Member" route.

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

/// Jurisdiction id that doubles as the governance domain id (same as the
/// `member_checker` contract: the proposal's `domain_id` is matched against the
/// caller's commons affiliations).
const DOMAIN_ID: &str = "demo-standing-bootstrap-coop";

/// DEMO/TEST-ONLY: establish active commons `Member` standing for `did` in
/// `jurisdiction`, using the existing internal `CommonsManager` API.
///
/// This is the reusable Gap B mechanism. It performs, in order, the same four
/// steps `e2e_member_standing_gate.rs` performs by hand:
///   1. create a personhood anchor (demo: self-vouched, no steward ceremony),
///   2. create the commons holder record bound to `did`,
///   3. join the jurisdiction (lands at `Candidate`),
///   4. advance `Candidate` → `Member` (the state the gate requires).
///
/// It is intentionally NOT a production enrollment path: it bypasses the
/// multi-steward SDIS proof-of-personhood ceremony and is only safe on a local
/// demo node a single operator controls. A future NYCN v4 loop mirrors this
/// sequence to put its organizer DID into a governed-proposal-eligible state.
pub async fn bootstrap_demo_member_standing(
    commons: &CommonsManager,
    did: &Did,
    jurisdiction: &str,
) -> anyhow::Result<()> {
    // 1. Personhood anchor. The voucher is a throwaway demo identity; production
    //    enrollment instead requires steward attestations through the ceremony.
    let voucher = KeyPair::generate()?;
    let anchor = commons
        .create_anchor_from_enrollment(did, Some(voucher.did()))
        .await?;
    let anchor_id = hex::encode(anchor.id());

    // 2. Commons holder record bound to the caller's own signing DID (the DID
    //    that mints JWTs and casts votes) — not an anchor-derived DID.
    let holder = commons.create_holder_from_anchor(&anchor_id, did).await?;
    let holder_id = hex::encode(holder.id());

    // 3. Join the jurisdiction. `join_jurisdiction` lands the affiliation at
    //    `Candidate`.
    commons
        .join_jurisdiction(
            &holder_id,
            JurisdictionId::new(jurisdiction),
            vec![MembershipCapability::Vote],
        )
        .await?;

    // 4. Advance to `Member` — the only status the gateway `member_checker`
    //    accepts. In production this transition is the outcome of a governed
    //    admission decision, not a direct call.
    commons
        .update_affiliation_status(
            &holder_id,
            &JurisdictionId::new(jurisdiction),
            MembershipStatus::Member,
        )
        .await?;

    Ok(())
}

/// Commons-backed `MemberStandingChecker`, identical in shape to the gateway's
/// production wiring (`server.rs`) and to `e2e_member_standing_gate.rs`.
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

/// Build a test gateway wired with the commons `member_checker` gate. `actor_did`
/// is placed in the governance domain's `StaticList` so the governance layer never
/// rejects them — the only gate under test is the commons checker, exactly as in
/// `e2e_member_standing_gate.rs`.
async fn build_app(
    commons_mgr: Arc<CommonsManager>,
    governance_manager: Arc<GovernanceManager>,
    actor_did: &Did,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    governance_manager
        .create_domain(
            GovernanceDomainId(DOMAIN_ID.to_string()),
            "Demo Standing Bootstrap Coop".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![actor_did.clone()]),
            },
        )
        .await
        .expect("create_domain");

    let jwt_secret = b"demo-standing-bootstrap-jwt-secret".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret).with_self_asserted_coop(true));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

    let gov_ctx = GovernanceContext {
        manager: governance_manager.clone(),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        on_proposal_accepted_with_evidence: None,
        member_checker: Some(make_checker(commons_mgr)),
        steward_checker: None,
        suspension_checker: None,
        membership_resolver: None,
        sdis_service: None,
        mandate_gate: None,
        build_mode: icn_governance_actor::http::GovernanceContextBuildMode::Test,
    };

    let auth_mw = HttpAuthentication::bearer(jwt_auth);
    test::init_service(
        App::new()
            .app_data(web::Data::new(auth_manager.clone()))
            // Authority composition boundary (issues #2436/#2437): this test
            // builds its own router, so it must install the same authority the
            // production composition installs — `jwt_auth` fails closed without it.
            .app_data(web::Data::new(std::sync::Arc::new(
                icn_gateway::session_authority::SessionAuthority::evaluator(auth_manager.clone()),
            )))
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

/// Mint a JWT for `did` via the challenge/verify handshake.
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
            "coop_id": "demo-standing-bootstrap",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

/// The Gap B proof: a governed (budget) proposal is rejected before standing and
/// accepted after the demo standing bootstrap.
#[actix_web::test]
async fn demo_standing_bootstrap_unlocks_governed_proposal() {
    let commons_mgr = Arc::new(CommonsManager::new());
    let governance_manager = Arc::new(GovernanceManager::new());

    let actor_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let actor_did = actor_bundle.did().clone();

    let app = build_app(commons_mgr.clone(), governance_manager, &actor_did).await;
    let token = get_jwt(&app, &actor_did.to_string(), &actor_bundle).await;

    let governed_proposal = json!({
        "domain_id": DOMAIN_ID,
        "title": "Q1 infrastructure budget",
        "description": "Allocate compute-hours to cluster maintenance.",
        "payload": {
            "type": "budget",
            "amount": 6000,
            "recipient": actor_did.to_string(),
            "currency": "compute-hours",
            "purpose": "q1-infra"
        }
    });

    // Before bootstrap: the member_checker gate rejects the governed proposal.
    let before = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(&governed_proposal)
            .to_request(),
    )
    .await;
    assert_eq!(
        before.status().as_u16(),
        403,
        "without Member standing the governed proposal must be rejected with 403"
    );

    // Establish demo/local Member standing via the reusable bootstrap helper.
    bootstrap_demo_member_standing(&commons_mgr, &actor_did, DOMAIN_ID)
        .await
        .expect("bootstrap_demo_member_standing");

    // After bootstrap: the same governed proposal is admitted.
    let after = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(&governed_proposal)
            .to_request(),
    )
    .await;
    assert_eq!(
        after.status().as_u16(),
        201,
        "after the demo standing bootstrap the governed proposal must be accepted with 201"
    );
}
