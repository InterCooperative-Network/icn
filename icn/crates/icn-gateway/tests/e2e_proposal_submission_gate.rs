//! Proof: accepted FreezeMember proposal → `on_proposal_accepted` hook fires →
//! member marked suspended → subsequent `create_proposal` call is denied 403.
//!
//! ## Chain under test
//!
//! ```text
//! close_proposal (HTTP 200)
//!   └─► on_proposal_accepted(GovernanceEffect::FreezeMember)
//!         └─► tokio::spawn → suspended_set.insert(member_did)
//!               └─► create_proposal (HTTP 403) for suspended member
//!                     └─► suspension_checker returns true
//!                           └─► err_forbidden("suspended members may not submit proposals")
//! ```
//!
//! ## What this proves
//!
//! 1. `on_proposal_accepted` fires and updates suspension state after proposal close.
//! 2. A suspended member's `create_proposal` call is denied 403.
//! 3. A non-suspended member can still submit proposals normally.
//! 4. The gate is scoped: only the suspended member is denied.
//!
//! ## What this is NOT
//!
//! - Full atomic suspension (best-effort hook, not transactional).
//! - Auto-expiration testing (separate gap, not in scope here).
//! - Ledger enforcement (covered in e2e_ledger_freeze_enforcement.rs).
//! - Vote-casting enforcement (covered in e2e_vote_standing_gate.rs).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use icn_gateway::{api, auth::AuthManager, middleware::jwt_auth, rate_limit::IpRateLimiter};
use icn_governance::{
    GovernanceDomainId, GovernanceParams, MembershipConfig, MembershipSource, ProposalId,
    ProposalPayload, ProposalScope,
};
use icn_governance_actor::{
    events::NoopEventEmitter,
    http::configure::{
        GovernanceContext, GovernanceEffect, ProposalAcceptedHook, SuspensionChecker,
    },
    manager::GovernanceManager,
};
use icn_identity::{Did, IdentityBundle, KeyPair};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::RwLock;

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
            "coop_id": "proposal-gate-test-coop",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

/// Helper: submit a Text proposal via HTTP and return the status code.
async fn try_create_proposal(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    domain_id: &str,
    token: &str,
    title: &str,
) -> u16 {
    let resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/v1/gov/proposals")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "domain_id": domain_id,
                "title": title,
                "description": "Test proposal description for gate verification",
                "payload": { "type": "text", "body": "This proposal tests the submission gate." }
            }))
            .to_request(),
    )
    .await;
    resp.status().as_u16()
}

/// E2E proposal submission gate:
///   FreezeMember proposal accepted → on_proposal_accepted → member suspended →
///   subsequent create_proposal is denied 403.
///
/// Proves that `create_proposal` now enforces the suspension gate added in
/// this tranche. Complements vote-casting denial (#1490) and ledger enforcement
/// (#1491): a suspended member cannot initiate governance action either.
#[actix_web::test]
async fn test_suspended_member_cannot_submit_proposals() {
    const DOMAIN_ID: &str = "test-coop-proposal-gate";

    // ── In-memory suspension store ───────────────────────────────────────────
    // Key: (did_string, domain_id_string). Simulates what CoopStore tracks in
    // production. The on_proposal_accepted hook populates this; the
    // suspension_checker reads it.
    let suspended: Arc<RwLock<HashSet<(String, String)>>> = Arc::new(RwLock::new(HashSet::new()));

    // ── Member identities ────────────────────────────────────────────────────
    let target_kp = KeyPair::generate().expect("target KeyPair");
    let target_did: Did = target_kp.did().clone();

    let bystander_kp = KeyPair::generate().expect("bystander KeyPair");
    let bystander_did: Did = bystander_kp.did().clone();

    // ── Wire on_proposal_accepted to mark the target suspended ────────────────
    let suspended_for_hook = suspended.clone();
    let on_proposal_accepted: ProposalAcceptedHook = Arc::new(move |effect| {
        if let GovernanceEffect::FreezeMember {
            member, domain_id, ..
        } = effect
        {
            let s = suspended_for_hook.clone();
            tokio::spawn(async move {
                s.write().await.insert((member.to_string(), domain_id));
            });
        }
    });

    // ── Wire suspension_checker to read from the in-memory store ─────────────
    let suspended_for_checker = suspended.clone();
    let suspension_checker: SuspensionChecker = Arc::new(move |did: Did, domain_id: String| {
        let s = suspended_for_checker.clone();
        Box::pin(async move { s.read().await.contains(&(did.to_string(), domain_id)) })
    });

    // ── Build test app (governance + auth) ───────────────────────────────────
    let jwt_secret = b"proposal-gate-enforcement-test-sec32".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
    let governance_manager = Arc::new(GovernanceManager::new());

    let gov_ctx = GovernanceContext {
        manager: governance_manager.clone(),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: Some(on_proposal_accepted),
        member_checker: None,
        steward_checker: None,
        suspension_checker: Some(suspension_checker),
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

    // ── Auth: actor (domain creator + sole voter), target, bystander ─────────
    let actor_bundle = IdentityBundle::generate().expect("actor IdentityBundle");
    let actor_did_str = actor_bundle.did().to_string();
    let actor_token = get_jwt(&app, &actor_did_str, &actor_bundle).await;

    let target_bundle = IdentityBundle::from_keypair(target_kp).expect("target IdentityBundle");
    let target_did_str = target_did.to_string();
    let target_token = get_jwt(&app, &target_did_str, &target_bundle).await;

    let bystander_bundle =
        IdentityBundle::from_keypair(bystander_kp).expect("bystander IdentityBundle");
    let bystander_did_str = bystander_did.to_string();
    let bystander_token = get_jwt(&app, &bystander_did_str, &bystander_bundle).await;

    // ── Create governance domain ──────────────────────────────────────────────
    let actor_governance_did: Did = actor_did_str.parse().expect("actor DID parse");
    let domain_id_gov = GovernanceDomainId(DOMAIN_ID.to_string());

    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "Proposal Gate Test Cooperative".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![actor_governance_did.clone()]),
            },
        )
        .await
        .expect("create_domain");

    // ── Precondition: target can submit proposals before freeze ───────────────
    let pre_freeze_status =
        try_create_proposal(&app, DOMAIN_ID, &target_token, "Pre-freeze proposal").await;
    assert_eq!(
        pre_freeze_status, 201,
        "pre-condition: unsuspended member must be able to submit proposals (got {pre_freeze_status})"
    );

    // ── Create FreezeMember proposal (via manager — no HTTP endpoint for this type) ──
    let proposal_id_val = ProposalId("gate-freeze-prop-001".to_string());
    governance_manager
        .create_proposal(
            proposal_id_val.clone(),
            domain_id_gov,
            actor_governance_did,
            "Freeze target for governance abuse".to_string(),
            "Policy violation detected; suspend governance access.".to_string(),
            ProposalPayload::FreezeMember {
                member: target_did.clone(),
                reason: "governance abuse".to_string(),
                duration_seconds: None,
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    let proposal_id = proposal_id_val.0.clone();

    // ── Open → vote → close via HTTP ─────────────────────────────────────────
    let open_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/open"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    assert_eq!(
        open_resp.status().as_u16(),
        200,
        "open_proposal must return 200"
    );

    let vote_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/vote"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "choice": "for", "comment": "Violation confirmed" }))
            .to_request(),
    )
    .await;
    assert_eq!(
        vote_resp.status().as_u16(),
        200,
        "cast_vote must return 200"
    );

    let close_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/close"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(
        close_resp.status().as_u16(),
        200,
        "close_proposal must return 200"
    );

    // ── Wait deterministically for the async acceptance hook to complete ─────
    // The hook spawns a tokio task; poll with a short sleep rather than a
    // fixed 200ms so the test doesn't fail on a loaded CI runner.
    let expected_suspension = (target_did_str.clone(), DOMAIN_ID.to_string());
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if suspended.read().await.contains(&expected_suspension) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for FreezeMember acceptance hook to suspend target");

    // ── Assert: target is now in the suspended set ─────────────────────��──────
    {
        let s = suspended.read().await;
        assert!(
            s.contains(&(target_did_str.clone(), DOMAIN_ID.to_string())),
            "target must be in suspended set after FreezeMember proposal accepted"
        );
    }

    // ── Assert: suspended member's create_proposal is denied 403 ─────────────
    let post_freeze_status = try_create_proposal(
        &app,
        DOMAIN_ID,
        &target_token,
        "Post-freeze proposal attempt",
    )
    .await;
    assert_eq!(
        post_freeze_status, 403,
        "suspended member must be denied proposal submission (got {post_freeze_status})"
    );

    // ── Assert: non-suspended bystander can still submit proposals ────────────
    let bystander_status =
        try_create_proposal(&app, DOMAIN_ID, &bystander_token, "Bystander proposal").await;
    assert_eq!(
        bystander_status, 201,
        "non-suspended bystander must still be able to submit proposals (got {bystander_status})"
    );
}
