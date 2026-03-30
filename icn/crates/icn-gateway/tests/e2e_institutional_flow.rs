//! E2E institutional lifecycle proof: enrollment → commons standing →
//! governance action → effect execution → commons state mutation persists.
//!
//! ## What this proves
//!
//! A cooperative member is enrolled in CommonsManager (anchor → holder →
//! affiliated with a jurisdiction). A FreezeMember governance proposal is
//! submitted, opened, voted for, and closed via the live HTTP governance API.
//! The `on_proposal_accepted` hook fires and mutates commons affiliation to
//! `MembershipStatus::Suspended`.
//!
//! The test wires a minimal `on_proposal_accepted` hook that performs only the
//! commons affiliation update, mirroring the commons side-effect of the hook in
//! `server.rs`. The ledger freeze side-effect used in production is intentionally
//! excluded here: this test focuses solely on the commons affiliation suspension
//! path, which is the novel seam being proved. The ledger freeze path has
//! separate coverage in ledger tests.
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
    sdis::SdisProposal, GovernanceDomainId, GovernanceParams, MembershipConfig, MembershipSource,
    ProposalId, ProposalPayload, ProposalScope,
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

/// Drive the open → vote → close lifecycle for an already-created proposal.
///
/// This is purely structural boilerplate shared across all three E2E tests:
/// the HTTP calls are identical regardless of proposal payload type.
async fn run_proposal_lifecycle(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    proposal_id: &str,
    token: &str,
) {
    let open_resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/open"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    assert_eq!(
        open_resp.status().as_u16(),
        200,
        "open_proposal {proposal_id} must return 200"
    );

    let vote_resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/vote"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "choice": "for" }))
            .to_request(),
    )
    .await;
    assert_eq!(
        vote_resp.status().as_u16(),
        200,
        "cast_vote {proposal_id} must return 200"
    );

    let close_resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/close"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(
        close_resp.status().as_u16(),
        200,
        "close_proposal {proposal_id} must return 200"
    );
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

    // ── Wire the commons side-effect hook (mirrors server.rs commons path) ───
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
        GovernanceEffect::UnfreezeMember { .. }
        | GovernanceEffect::DeployCharter { .. }
        | GovernanceEffect::AppointSteward { .. }
        | GovernanceEffect::RevokeSteward { .. }
        | GovernanceEffect::Unhandled { .. } => {}
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
        member_checker: None,
        steward_checker: None,
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

    // ── Step 6: Poll until the hook's tokio::spawn mutates affiliation ───────
    // The hook fires tokio::spawn — we must wait for the spawned future to
    // acquire the CommonsManager write lock and mutate affiliation. Poll with
    // a bounded 5s timeout so the test is deterministic even on slow CI.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let affiliations = commons_mgr
            .list_affiliations(&holder_id)
            .await
            .expect("list_affiliations after freeze");
        let aff = affiliations
            .iter()
            .find(|a| a.jurisdiction_id == JurisdictionId::new(DOMAIN_ID))
            .expect("affiliation must still exist after FreezeMember");

        if aff.membership_status == MembershipStatus::Suspended {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for commons affiliation to become Suspended \
                 after FreezeMember proposal accepted; last seen status: {:?}",
                aff.membership_status
            );
        }
        tokio::task::yield_now().await;
    }
}

/// E2E institutional flow — UnfreezeMember:
///   member suspended in commons → UnfreezeMember proposal accepted →
///   commons affiliation reinstated to Member.
///
/// Proves the symmetric restorative path: `GovernanceEffect::UnfreezeMember`
/// → `CommonsManager::update_affiliation_status(..., Member)`.
#[actix_web::test]
async fn test_e2e_unfreeze_member_reinstates_commons_affiliation() {
    const DOMAIN_ID: &str = "test-coop-unfreeze-e2e";

    // ── Commons setup ────────────────────────────────────────────────────────
    let commons_mgr = Arc::new(CommonsManager::new());

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

    commons_mgr
        .join_jurisdiction(
            &holder_id,
            JurisdictionId::new(DOMAIN_ID),
            vec![MembershipCapability::Vote],
        )
        .await
        .expect("join_jurisdiction");

    // Pre-condition: manually set affiliation to Suspended (simulates a prior FreezeMember).
    commons_mgr
        .update_affiliation_status(
            &holder_id,
            &JurisdictionId::new(DOMAIN_ID),
            MembershipStatus::Suspended,
        )
        .await
        .expect("pre-condition: suspend affiliation");

    {
        let affiliations = commons_mgr
            .list_affiliations(&holder_id)
            .await
            .expect("list");
        let aff = affiliations
            .iter()
            .find(|a| a.jurisdiction_id == JurisdictionId::new(DOMAIN_ID))
            .expect("affiliation exists");
        assert_eq!(
            aff.membership_status,
            MembershipStatus::Suspended,
            "pre-condition: affiliation must be Suspended before UnfreezeMember"
        );
    }

    // ── Wire commons-only hook ────────────────────────────────────────────────
    let commons_mgr_for_hook = commons_mgr.clone();
    let on_proposal_accepted: ProposalAcceptedHook = Arc::new(move |effect| {
        if let GovernanceEffect::UnfreezeMember {
            domain_id, member, ..
        } = effect
        {
            let commons = commons_mgr_for_hook.clone();
            tokio::spawn(async move {
                let jurisdiction = JurisdictionId::new(&domain_id);
                if let Ok(Some(h)) = commons.get_holder_by_did(&member).await {
                    let hid = hex::encode(h.id());
                    let _ = commons
                        .update_affiliation_status(&hid, &jurisdiction, MembershipStatus::Member)
                        .await;
                }
            });
        }
    });

    // ── Build test app ────────────────────────────────────────────────────────
    let jwt_secret = b"e2e-unfreeze-flow-test-secret-min32".to_vec();
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

    let actor_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let actor_did = actor_bundle.did().to_string();
    let token = get_jwt(&app, &actor_did, &actor_bundle).await;

    // ── Create governance domain + UnfreezeMember proposal via manager ────────
    let actor_governance_did: icn_identity::Did = actor_did.parse().expect("actor DID parse");
    let domain_id_gov = GovernanceDomainId(DOMAIN_ID.to_string());
    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "E2E Unfreeze Test Cooperative".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![actor_governance_did.clone()]),
            },
        )
        .await
        .expect("create_domain");

    let proposal_id_val = ProposalId("e2e-unfreeze-1".to_string());
    governance_manager
        .create_proposal(
            proposal_id_val.clone(),
            domain_id_gov,
            actor_governance_did,
            "Reinstate suspended member".to_string(),
            "Investigation complete; member cleared.".to_string(),
            ProposalPayload::UnfreezeMember {
                member: target_did.clone(),
                reason: "Investigation complete, no wrongdoing found".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    let proposal_id = proposal_id_val.0.clone();

    // ── Open → Vote → Close ───────────────────────────────────────────────────
    let open_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/open"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    assert_eq!(open_resp.status().as_u16(), 200, "open_proposal");

    let vote_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/vote"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "choice": "for" }))
            .to_request(),
    )
    .await;
    assert_eq!(vote_resp.status().as_u16(), 200, "cast_vote");

    let close_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{proposal_id}/close"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(close_resp.status().as_u16(), 200, "close_proposal");

    // ── Poll until affiliation is reinstated ──────────────────────────────────
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let affiliations = commons_mgr
            .list_affiliations(&holder_id)
            .await
            .expect("list_affiliations after unfreeze");
        let aff = affiliations
            .iter()
            .find(|a| a.jurisdiction_id == JurisdictionId::new(DOMAIN_ID))
            .expect("affiliation must still exist after UnfreezeMember");

        if aff.membership_status == MembershipStatus::Member {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for commons affiliation to become Member \
                 after UnfreezeMember proposal accepted; last seen status: {:?}",
                aff.membership_status
            );
        }
        tokio::task::yield_now().await;
    }
}

/// E2E institutional flow — AppointSteward scoped to chartered domain:
///   charter proposal accepted → DeployCharter registers domain in commons →
///   AppointSteward proposal accepted → steward created with jurisdiction = domain_id.
///
/// Proves the scoped steward appointment path:
/// 1. `GovernanceEffect::DeployCharter` → `CommonsManager::store_charter()` (minimal stub)
/// 2. `GovernanceEffect::AppointSteward { domain_id }` → `CommonsManager::register_steward(..., Some(domain_id))`
/// 3. Resulting steward record has `jurisdiction == Some(domain_id)`
///
/// Before this, AppointSteward passed `None` for jurisdiction because commons validates
/// jurisdiction strings against its charter store, which was never populated by
/// governance acceptance.  The Charter proposal now emits `DeployCharter`, closing the gap.
#[actix_web::test]
async fn test_e2e_appoint_steward_scoped_to_chartered_domain() {
    const DOMAIN_ID: &str = "test-coop-scoped-steward-e2e";

    // ── Commons setup ────────────────────────────────────────────────────────
    let commons_mgr = Arc::new(CommonsManager::new());

    // Enroll the candidate with a steward vouch to reach Strong POP level,
    // which register_steward requires.
    let vouching_steward_kp = KeyPair::generate().expect("steward KeyPair");
    let vouching_steward_did = vouching_steward_kp.did().clone();
    let candidate_kp = KeyPair::generate().expect("candidate KeyPair");
    let candidate_did = candidate_kp.did().clone();

    let anchor = commons_mgr
        .create_anchor_from_enrollment(&candidate_did, Some(&vouching_steward_did))
        .await
        .expect("create_anchor_from_enrollment");
    let anchor_id = hex::encode(anchor.id());

    commons_mgr
        .create_holder_from_anchor(&anchor_id, &candidate_did)
        .await
        .expect("create_holder_from_anchor");

    // Pre-condition: no charter for this domain yet, candidate is not a steward.
    assert!(
        commons_mgr
            .get_charter_by_domain(DOMAIN_ID)
            .await
            .expect("pre-check charter")
            .is_none(),
        "charter must not exist before Charter proposal"
    );
    assert!(
        !commons_mgr
            .is_active_steward(&candidate_did)
            .await
            .expect("is_active_steward pre-check"),
        "candidate must not be a steward before AppointSteward proposal"
    );

    // ── Wire the hook: DeployCharter + AppointSteward (mirrors server.rs paths) ─
    let commons_mgr_for_hook = commons_mgr.clone();
    let on_proposal_accepted: ProposalAcceptedHook = Arc::new(move |effect| {
        let commons = commons_mgr_for_hook.clone();
        match effect {
            GovernanceEffect::DeployCharter { charter_id, .. } => {
                tokio::spawn(async move {
                    use icn_governance::{
                        Charter, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType,
                    };
                    let charter = Charter::new(
                        OrgType::Cooperative,
                        charter_id.clone(),
                        charter_id.clone(),
                        GovernanceConfig::cooperative_default(),
                        MembershipPolicy::default(),
                        DisputePolicy::default(),
                    );
                    let _ = commons.store_charter(charter).await;
                });
            }
            GovernanceEffect::AppointSteward {
                proposal_id,
                domain_id,
                candidate,
                bond_amount,
                term_length_seconds,
                ..
            } => {
                tokio::spawn(async move {
                    let term_days = term_length_seconds / 86_400;
                    let bond = bond_amount.max(0) as u64;
                    let _ = commons
                        .register_steward(
                            &candidate,
                            &candidate,
                            term_days,
                            bond,
                            proposal_id,
                            Some(domain_id),
                            vec![],
                        )
                        .await;
                });
            }
            _ => {}
        }
    });

    // ── Build test app ────────────────────────────────────────────────────────
    let jwt_secret = b"e2e-scoped-steward-test-secret-32bc".to_vec();
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

    let actor_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let actor_did = actor_bundle.did().to_string();
    let token = get_jwt(&app, &actor_did, &actor_bundle).await;

    let actor_governance_did: icn_identity::Did = actor_did.parse().expect("actor DID parse");
    let domain_id_gov = GovernanceDomainId(DOMAIN_ID.to_string());
    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "E2E Scoped Steward Test Cooperative".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![actor_governance_did.clone()]),
            },
        )
        .await
        .expect("create_domain");

    // ── Step 1: Charter proposal → DeployCharter → commons charter registered ─
    // charter_id equals domain_id by governance convention.
    let charter_proposal_id = ProposalId("e2e-charter-1".to_string());
    governance_manager
        .create_proposal(
            charter_proposal_id.clone(),
            domain_id_gov.clone(),
            actor_governance_did.clone(),
            "Ratify cooperative charter".to_string(),
            "Adopts the founding charter for this domain.".to_string(),
            ProposalPayload::Charter {
                charter_id: DOMAIN_ID.to_string(),
                charter_yaml: "schema_version: v0\ntype: cooperative\n".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create charter proposal");

    run_proposal_lifecycle(&app, &charter_proposal_id.0, &token).await;

    // Poll until the charter is registered in commons (DeployCharter hook fires async).
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if commons_mgr
            .get_charter_by_domain(DOMAIN_ID)
            .await
            .expect("charter poll")
            .is_some()
        {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for DeployCharter to register commons charter");
        }
        tokio::task::yield_now().await;
    }

    // ── Step 2: AppointSteward proposal → steward registered with jurisdiction ─
    let appoint_proposal_id = ProposalId("e2e-appoint-scoped-1".to_string());
    governance_manager
        .create_proposal(
            appoint_proposal_id.clone(),
            domain_id_gov,
            actor_governance_did,
            "Appoint scoped steward".to_string(),
            "Candidate meets all requirements.".to_string(),
            ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: candidate_did.clone(),
                    sponsors: vec![],
                    region: "northeast".to_string(),
                    bond_amount: 100,
                    term_length: 365 * 86_400, // 1 year
                },
            },
            ProposalScope::Local,
        )
        .await
        .expect("create appoint proposal");

    run_proposal_lifecycle(&app, &appoint_proposal_id.0, &token).await;

    // Poll until steward record is created.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if commons_mgr
            .is_active_steward(&candidate_did)
            .await
            .expect("is_active_steward poll")
        {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for steward record to be created \
                 after AppointSteward proposal accepted"
            );
        }
        tokio::task::yield_now().await;
    }

    // ── Assert: steward record is scoped to the governance domain ─────────────
    let record = commons_mgr
        .get_steward_by_did(&candidate_did)
        .await
        .expect("get_steward_by_did")
        .expect("steward record must exist");
    assert_eq!(
        record.jurisdiction,
        Some(DOMAIN_ID.to_string()),
        "steward must be scoped to the chartered domain, got: {:?}",
        record.jurisdiction
    );
}
