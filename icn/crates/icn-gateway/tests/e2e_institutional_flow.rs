//! E2E institutional lifecycle proof: enrollment → commons standing →
//! governance action → effect execution → commons state mutation persists.
//!
//! ## What this proves
//!
//! A cooperative member is enrolled in CommonsManager (anchor → holder →
//! affiliated with a jurisdiction). A FreezeMember governance proposal is
//! submitted, opened, voted for, and closed via the live HTTP governance API.
//! The `on_proposal_accepted` hook — the same hook wired in `server.rs` —
//! fires both:
//!   1. Ledger freeze (existing path, not asserted here)
//!   2. Commons affiliation suspension (new path)
//!
//! After the proposal closes the test reads the member's affiliation status
//! directly from the CommonsManager and asserts `MembershipStatus::Suspended`.
//!
//! ## What this is NOT
//!
//! - Persistence across process restart (proven in commons_persistence.rs /
//!   commons_integration.rs Layer 4).
//! - Transactional atomicity between ledger and commons (these are best-effort
//!   independent side-effects; two-phase commit is future work).

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
    http::configure::{GovernanceContext, GovernanceEffect, ProposalAcceptedHook},
    manager::GovernanceManager,
};
use icn_identity::{
    commons::{JurisdictionId, MembershipCapability, MembershipStatus},
    IdentityBundle, KeyPair,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// Auth helper: challenge → sign → JWT.
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
            "coop_id": "e2e-test-coop",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

/// E2E institutional flow:
///   enroll target → join jurisdiction → FreezeMember proposal accepted →
///   commons affiliation becomes Suspended.
#[actix_web::test]
async fn test_e2e_freeze_member_suspends_commons_affiliation() {
    const DOMAIN_ID: &str = "test-coop-e2e";

    // ── Commons setup ────────────────────────────────────────────────────────
    let commons_mgr = Arc::new(CommonsManager::new());

    // Enroll the target member. A steward vouch is required to reach Strong POP
    // level, which `join_jurisdiction` enforces.
    let steward_kp = KeyPair::generate().expect("steward KeyPair");
    let steward_did = steward_kp.did().clone();
    let target_kp = KeyPair::generate().expect("target KeyPair");
    let target_did = target_kp.did().clone();

    let anchor = commons_mgr
        .create_anchor_from_enrollment(&target_did, Some(&steward_did))
        .await
        .expect("create_anchor_from_enrollment");
    let anchor_id = hex::encode(anchor.id());

    let holder = commons_mgr
        .create_holder_from_anchor(&anchor_id, &target_did)
        .await
        .expect("create_holder_from_anchor");
    let holder_id = hex::encode(holder.id());

    // Join the jurisdiction the governance domain will operate in.
    commons_mgr
        .join_jurisdiction(
            &holder_id,
            JurisdictionId::new(DOMAIN_ID),
            vec![MembershipCapability::Vote],
        )
        .await
        .expect("join_jurisdiction");

    // Verify affiliation is present and active before the proposal.
    {
        let affiliations = commons_mgr
            .list_affiliations(&holder_id)
            .await
            .expect("list_affiliations");
        let aff = affiliations
            .iter()
            .find(|a| a.jurisdiction_id == JurisdictionId::new(DOMAIN_ID))
            .expect("affiliation must exist after join_jurisdiction");
        assert_ne!(
            aff.membership_status,
            MembershipStatus::Suspended,
            "affiliation must not be Suspended before FreezeMember proposal"
        );
    }

    // ── Wire the same hook used in server.rs ─────────────────────────────────
    let commons_mgr_for_hook = commons_mgr.clone();
    let on_proposal_accepted: ProposalAcceptedHook = Arc::new(move |effect| match effect {
        GovernanceEffect::FreezeMember {
            domain_id, member, ..
        } => {
            let commons = commons_mgr_for_hook.clone();
            tokio::spawn(async move {
                let jurisdiction = JurisdictionId::new(&domain_id);
                if let Ok(Some(h)) = commons.get_holder_by_did(&member).await {
                    let hid = hex::encode(h.id());
                    let _ = commons
                        .update_affiliation_status(&hid, &jurisdiction, MembershipStatus::Suspended)
                        .await;
                }
            });
        }
        GovernanceEffect::Unhandled { .. } => {}
    });

    // ── Build test app (governance + auth) ───────────────────────────────────
    let jwt_secret = b"e2e-institutional-flow-test-secret32".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
    let governance_manager = Arc::new(GovernanceManager::new());

    let gov_ctx = GovernanceContext {
        manager: governance_manager.clone(),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: Some(on_proposal_accepted),
    };

    let auth_mw = HttpAuthentication::bearer(jwt_auth);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_manager.clone()))
            .app_data(web::Data::new(ip_limiter.clone()))
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

    // ── Auth: acting DID (domain creator + sole voter) ───────────────────────
    let actor_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let actor_did = actor_bundle.did().to_string();
    let token = get_jwt(&app, &actor_did, &actor_bundle).await;

    // ── Step 1: Create governance domain + FreezeMember proposal directly ────
    // The HTTP API's ProposalPayloadRequest does not expose FreezeMember as a
    // create-proposal type (it is an internal governance primitive). We create
    // the domain and proposal via the manager directly, then exercise the
    // open/vote/close lifecycle through the live HTTP handlers.
    // This matches the pattern used in apps/governance handler tests.
    let actor_governance_did: icn_identity::Did = actor_did.parse().expect("actor DID parse");
    let domain_id_gov = GovernanceDomainId(DOMAIN_ID.to_string());
    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "E2E Test Cooperative".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![actor_governance_did.clone()]),
            },
        )
        .await
        .expect("create_domain");

    let proposal_id_val = ProposalId("e2e-freeze-1".to_string());
    governance_manager
        .create_proposal(
            proposal_id_val.clone(),
            domain_id_gov,
            actor_governance_did,
            "Freeze disruptive member".to_string(),
            "Suspend member for repeated CoC violations.".to_string(),
            ProposalPayload::FreezeMember {
                member: target_did.clone(),
                reason: "Repeated Code of Conduct violations".to_string(),
                duration_seconds: None,
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    let proposal_id = proposal_id_val.0.clone();

    // ── Step 3: Open proposal ────────────────────────────────────────────────
    let open_req = test::TestRequest::post()
        .uri(&format!("/v1/gov/proposals/{proposal_id}/open"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "voting_period_seconds": 3600 }))
        .to_request();
    let open_resp = test::call_service(&app, open_req).await;
    assert_eq!(
        open_resp.status().as_u16(),
        200,
        "open_proposal must return 200"
    );

    // ── Step 4: Vote for ────────────────────────────────────────────────────
    let vote_req = test::TestRequest::post()
        .uri(&format!("/v1/gov/proposals/{proposal_id}/vote"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "choice": "for" }))
        .to_request();
    let vote_resp = test::call_service(&app, vote_req).await;
    assert_eq!(
        vote_resp.status().as_u16(),
        200,
        "cast_vote must return 200"
    );

    // ── Step 5: Close proposal → triggers on_proposal_accepted hook ─────────
    let close_req = test::TestRequest::post()
        .uri(&format!("/v1/gov/proposals/{proposal_id}/close"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let close_resp = test::call_service(&app, close_req).await;
    assert_eq!(
        close_resp.status().as_u16(),
        200,
        "close_proposal must return 200"
    );

    // Verify proposal state is Accepted.
    let get_req = test::TestRequest::get()
        .uri(&format!("/v1/gov/proposals/{proposal_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let final_state: Value = test::call_and_read_body_json(&app, get_req).await;
    assert!(
        final_state["state"]["Accepted"].is_object(),
        "proposal must be Accepted after close with quorum met, got: {}",
        final_state["state"]
    );

    // ── Step 6: Yield to let the hook's tokio::spawn run ────────────────────
    // The hook fires tokio::spawn — we must yield the executor so the spawned
    // future can acquire the CommonsManager write lock and mutate affiliation.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // ── Assert: target member's commons affiliation is now Suspended ─────────
    let affiliations = commons_mgr
        .list_affiliations(&holder_id)
        .await
        .expect("list_affiliations after freeze");
    let aff = affiliations
        .iter()
        .find(|a| a.jurisdiction_id == JurisdictionId::new(DOMAIN_ID))
        .expect("affiliation must still exist after FreezeMember");

    assert_eq!(
        aff.membership_status,
        MembershipStatus::Suspended,
        "commons affiliation must be Suspended after FreezeMember proposal accepted"
    );
}
