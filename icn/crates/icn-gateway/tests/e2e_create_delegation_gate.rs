//! Proof: accepted FreezeMember proposal → `on_proposal_accepted` fires →
//! member marked suspended → subsequent `create_delegation` call with a
//! domain-scoped scope is denied 403.
//!
//! ## Chain under test
//!
//! ```text
//! close_proposal(FreezeMember) (HTTP 200)
//!   └─► on_proposal_accepted(GovernanceEffect::FreezeMember)
//!         └─► tokio::spawn → suspended_set.insert(member_did)
//!               └─► create_delegation (HTTP 403) for suspended member
//!                     └─► suspension_checker returns true
//!                           └─► err_forbidden("suspended members may not create delegations")
//! ```
//!
//! ## What this proves
//!
//! 1. A suspended member cannot create a Domain-scoped delegation in the domain
//!    where they are frozen, preventing proxy voting via delegation.
//! 2. A non-suspended member can still create delegations normally.
//! 3. The gate is scoped — only the suspended member is denied.
//! 4. The target member can create a delegation before being frozen (precondition).
//!
//! ## What this is NOT
//!
//! - Vote-casting enforcement (covered in e2e_vote_standing_gate.rs).
//! - Ledger enforcement (covered in e2e_ledger_freeze_enforcement.rs).
//! - Proposal-creation enforcement (covered in e2e_proposal_submission_gate.rs).
//! - Open-proposal enforcement (covered in e2e_open_proposal_gate.rs).
//! - Full atomic suspension (best-effort hook, not transactional).

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
            "coop_id": "delegation-gate-test-coop",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

/// Helper: attempt to create a Domain-scoped delegation via HTTP; return status.
async fn try_create_delegation(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    delegate_did: &str,
    domain_id: &str,
) -> u16 {
    test::call_service(
        app,
        test::TestRequest::post()
            .uri("/v1/gov/delegations")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "delegate": delegate_did,
                "scope": format!("domain:{domain_id}")
            }))
            .to_request(),
    )
    .await
    .status()
    .as_u16()
}

/// Helper: attempt to create a Blanket-scoped delegation via HTTP; return status.
async fn try_create_blanket_delegation(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    delegate_did: &str,
) -> u16 {
    test::call_service(
        app,
        test::TestRequest::post()
            .uri("/v1/gov/delegations")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "delegate": delegate_did,
                "scope": "blanket"
            }))
            .to_request(),
    )
    .await
    .status()
    .as_u16()
}

/// E2E create_delegation suspension gate:
///   member creates delegation → FreezeMember accepted → member suspended →
///   member's Domain-scoped create_delegation returns 403,
///   non-suspended member's returns 201.
#[actix_web::test]
async fn test_suspended_member_cannot_create_delegation() {
    const DOMAIN_ID: &str = "test-coop-delegation-gate";

    // ── In-memory suspension store ───────────────────────────────────────────
    let suspended: Arc<RwLock<HashSet<(String, String)>>> = Arc::new(RwLock::new(HashSet::new()));

    // ── Member identities ────────────────────────────────────────────────────
    let target_kp = KeyPair::generate().expect("target KeyPair");
    let target_did: Did = target_kp.did().clone();

    let bystander_kp = KeyPair::generate().expect("bystander KeyPair");
    let bystander_did: Did = bystander_kp.did().clone();

    let proxy_kp = KeyPair::generate().expect("proxy KeyPair");
    let proxy_did: Did = proxy_kp.did().clone();

    // ── Wire on_proposal_accepted to mark the target suspended ───────────────
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

    // ── Build test app ────────────────────────────────────────────────────────
    let jwt_secret = b"delegation-gate-enforcement-sec32".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
    let governance_manager = Arc::new(GovernanceManager::new());

    let gov_ctx = GovernanceContext {
        manager: governance_manager.clone(),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: Some(on_proposal_accepted),
        on_proposal_accepted_with_evidence: None,
        member_checker: None,
        steward_checker: None,
        suspension_checker: Some(suspension_checker),
        membership_resolver: None,
        sdis_service: None,
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

    // proxy_did is the delegation target (the member who receives voting power)
    let proxy_bundle = IdentityBundle::from_keypair(proxy_kp).expect("proxy IdentityBundle");
    let proxy_did_str = proxy_did.to_string();
    // proxy only needs to exist as a DID — no JWT required for delegation target
    let _ = get_jwt(&app, &proxy_did_str, &proxy_bundle).await;

    // ── Create governance domain ──────────────────────────────────────────────
    let actor_governance_did: Did = actor_did_str.parse().expect("actor DID parse");
    // All members must be in the StaticList so membership checks pass before
    // reaching the suspension gate.
    let target_governance_did: Did = target_did_str.parse().expect("target DID parse");
    let bystander_governance_did: Did = bystander_did_str.parse().expect("bystander DID parse");
    let domain_id_gov = GovernanceDomainId(DOMAIN_ID.to_string());

    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "Delegation Gate Test Cooperative".to_string(),
            "cooperative".to_string(),
            // 1% quorum/approval so the actor's single vote out of 3 passes.
            GovernanceParams::new(1, 1, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![
                    actor_governance_did.clone(),
                    target_governance_did,
                    bystander_governance_did,
                ]),
            },
        )
        .await
        .expect("create_domain");

    // ── Precondition: target (not yet suspended) can create a delegation ──────
    let pre_freeze_status =
        try_create_delegation(&app, &target_token, &proxy_did_str, DOMAIN_ID).await;
    assert_eq!(
        pre_freeze_status, 201,
        "pre-condition: unsuspended target must be able to create delegations (got {pre_freeze_status})"
    );

    // ── Create and run FreezeMember proposal ─────────────────────────────────
    let freeze_proposal_id = ProposalId("delegation-gate-freeze-prop".to_string());
    governance_manager
        .create_proposal(
            freeze_proposal_id.clone(),
            domain_id_gov,
            actor_governance_did,
            "Freeze target member".to_string(),
            "Suspend target from governance actions.".to_string(),
            ProposalPayload::FreezeMember {
                member: target_did.clone(),
                reason: "governance abuse".to_string(),
                duration_seconds: None,
            },
            ProposalScope::Local,
        )
        .await
        .expect("create FreezeMember proposal");

    let freeze_id = freeze_proposal_id.0.clone();

    let open_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{freeze_id}/open"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    assert_eq!(open_resp.status().as_u16(), 200, "open freeze proposal");

    let vote_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{freeze_id}/vote"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "choice": "for" }))
            .to_request(),
    )
    .await;
    assert_eq!(vote_resp.status().as_u16(), 200, "vote on freeze proposal");

    let close_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{freeze_id}/close"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(close_resp.status().as_u16(), 200, "close freeze proposal");

    // ── Wait deterministically for the hook to complete ───────────────────────
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
    .expect("timed out waiting for FreezeMember acceptance hook");

    // ── Assert: suspended member cannot create a Domain-scoped delegation ────
    let post_freeze_status =
        try_create_delegation(&app, &target_token, &proxy_did_str, DOMAIN_ID).await;
    assert_eq!(
        post_freeze_status, 403,
        "suspended member must be denied create_delegation (got {post_freeze_status})"
    );

    // ── Assert: non-suspended bystander can still create delegations ──────────
    let bystander_status =
        try_create_delegation(&app, &bystander_token, &proxy_did_str, DOMAIN_ID).await;
    assert_eq!(
        bystander_status, 201,
        "non-suspended bystander must be able to create delegations (got {bystander_status})"
    );
}

/// E2E Blanket-scope delegation suspension gate:
///   member is suspended in a StaticList domain →
///   subsequent Blanket create_delegation is denied 403,
///   non-suspended member's Blanket delegation succeeds,
///   unrelated member (not in any domain) is not denied.
///
/// This proves that the `list_domains()` → `contains_static()` walk correctly
/// identifies the delegator's member domains and denies the Blanket creation.
#[actix_web::test]
async fn test_suspended_member_cannot_create_blanket_delegation() {
    const DOMAIN_ID: &str = "blanket-gate-test-coop";

    // ── In-memory suspension store ───────────────────────────────────────────
    let suspended: Arc<RwLock<HashSet<(String, String)>>> = Arc::new(RwLock::new(HashSet::new()));

    // ── Member identities ────────────────────────────────────────────────────
    let target_kp = KeyPair::generate().expect("target KeyPair");
    let target_did: Did = target_kp.did().clone();

    let bystander_kp = KeyPair::generate().expect("bystander KeyPair");
    let bystander_did: Did = bystander_kp.did().clone();

    let proxy_kp = KeyPair::generate().expect("proxy KeyPair");
    let proxy_did: Did = proxy_kp.did().clone();

    // outsider is NOT in any domain — Blanket delegation must still succeed.
    let outsider_kp = KeyPair::generate().expect("outsider KeyPair");
    let outsider_did: Did = outsider_kp.did().clone();

    // ── Wire hook + checker ──────────────────────────────────────────────────
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

    let suspended_for_checker = suspended.clone();
    let suspension_checker: SuspensionChecker = Arc::new(move |did: Did, domain_id: String| {
        let s = suspended_for_checker.clone();
        Box::pin(async move { s.read().await.contains(&(did.to_string(), domain_id)) })
    });

    // ── Build test app ────────────────────────────────────────────────────────
    let jwt_secret = b"blanket-gate-enforcement-sec--32".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
    let governance_manager = Arc::new(GovernanceManager::new());

    let gov_ctx = GovernanceContext {
        manager: governance_manager.clone(),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: Some(on_proposal_accepted),
        on_proposal_accepted_with_evidence: None,
        member_checker: None,
        steward_checker: None,
        suspension_checker: Some(suspension_checker),
        membership_resolver: None,
        sdis_service: None,
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

    // ── Auth ─────────────────────────────────────────────────────────────────
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

    let outsider_bundle =
        IdentityBundle::from_keypair(outsider_kp).expect("outsider IdentityBundle");
    let outsider_did_str = outsider_did.to_string();
    let outsider_token = get_jwt(&app, &outsider_did_str, &outsider_bundle).await;

    let proxy_bundle = IdentityBundle::from_keypair(proxy_kp).expect("proxy IdentityBundle");
    let proxy_did_str = proxy_did.to_string();
    let _ = get_jwt(&app, &proxy_did_str, &proxy_bundle).await;

    // ── Create governance domain (StaticList: actor + target + bystander) ────
    let actor_governance_did: Did = actor_did_str.parse().expect("actor DID parse");
    let target_governance_did: Did = target_did_str.parse().expect("target DID parse");
    let bystander_governance_did: Did = bystander_did_str.parse().expect("bystander DID parse");
    let domain_id_gov = GovernanceDomainId(DOMAIN_ID.to_string());

    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "Blanket Gate Test Cooperative".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(1, 1, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![
                    actor_governance_did.clone(),
                    target_governance_did,
                    bystander_governance_did,
                ]),
            },
        )
        .await
        .expect("create_domain");

    // ── Precondition: target (not yet suspended) can create a Blanket delegation
    let pre_freeze_status =
        try_create_blanket_delegation(&app, &target_token, &proxy_did_str).await;
    assert_eq!(
        pre_freeze_status, 201,
        "pre-condition: unsuspended target must be able to create blanket delegations (got {pre_freeze_status})"
    );

    // ── Suspend target via FreezeMember proposal ─────────────────────────────
    let freeze_proposal_id = ProposalId("blanket-gate-freeze-prop".to_string());
    governance_manager
        .create_proposal(
            freeze_proposal_id.clone(),
            domain_id_gov,
            actor_governance_did,
            "Freeze target member".to_string(),
            "Suspend target from all governance actions.".to_string(),
            ProposalPayload::FreezeMember {
                member: target_did.clone(),
                reason: "governance abuse".to_string(),
                duration_seconds: None,
            },
            ProposalScope::Local,
        )
        .await
        .expect("create FreezeMember proposal");

    let freeze_id = freeze_proposal_id.0.clone();

    let open_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{freeze_id}/open"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    assert_eq!(open_resp.status().as_u16(), 200, "open freeze proposal");

    let vote_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{freeze_id}/vote"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "choice": "for" }))
            .to_request(),
    )
    .await;
    assert_eq!(vote_resp.status().as_u16(), 200, "vote on freeze proposal");

    let close_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{freeze_id}/close"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(close_resp.status().as_u16(), 200, "close freeze proposal");

    // ── Wait for hook ─────────────────────────────────────────────────────────
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
    .expect("timed out waiting for FreezeMember acceptance hook");

    // ── Assert: suspended member cannot create a Blanket delegation ───────────
    let post_freeze_status =
        try_create_blanket_delegation(&app, &target_token, &proxy_did_str).await;
    assert_eq!(
        post_freeze_status, 403,
        "suspended member must be denied blanket delegation creation (got {post_freeze_status})"
    );

    // ── Assert: non-suspended bystander can still create Blanket delegations ──
    let bystander_status =
        try_create_blanket_delegation(&app, &bystander_token, &proxy_did_str).await;
    assert_eq!(
        bystander_status, 201,
        "non-suspended bystander must be able to create blanket delegations (got {bystander_status})"
    );

    // ── Assert: outsider (not in any domain) is not falsely denied ────────────
    let outsider_status =
        try_create_blanket_delegation(&app, &outsider_token, &proxy_did_str).await;
    assert_eq!(
        outsider_status, 201,
        "outsider not in any domain must not be falsely denied (got {outsider_status})"
    );

    let _ = outsider_did_str;
}
