//! Proof: accepted FreezeMember proposal → `on_proposal_accepted` hook fires →
//! `ledger.freeze_member_with_metadata()` called → frozen member's ledger
//! entries are quarantined (rejected by `validate_entry`).
//!
//! ## Chain under test
//!
//! ```text
//! close_proposal (HTTP 200)
//!   └─► on_proposal_accepted(GovernanceEffect::FreezeMember)
//!         └─► tokio::spawn → ledger.freeze_member_with_metadata()
//!               └─► FreezeManager::freeze_with_metadata() [persisted to sled]
//!                     └─► Ledger::validate_entry() checks is_frozen()
//!                           └─► entry quarantined / Error returned
//! ```
//!
//! ## What this proves
//!
//! 1. `on_proposal_accepted` fires and calls ledger freeze after proposal close.
//! 2. `ledger.is_member_frozen()` is true for the target member post-freeze.
//! 3. A ledger entry authored by the frozen member is quarantined.
//! 4. A ledger entry authored by a non-frozen member is accepted.
//!
//! ## What this is NOT
//!
//! - Full atomic freeze + commit (two effects are best-effort, not transactional).
//! - Auto-expiration testing (separate gap, not in scope here).
//! - Proposal-submission gating (separate gap).

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
    http::configure::{GovernanceContext, GovernanceEffect, ProposalAcceptedHook},
    manager::GovernanceManager,
};
use icn_identity::{IdentityBundle, KeyPair};
use icn_ledger::{entry::JournalEntryBuilder, Ledger};
use icn_store::SledStore;
use serde_json::{json, Value};
use std::sync::Arc;
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
            "coop_id": "ledger-freeze-test-coop",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

/// E2E ledger freeze enforcement:
///   FreezeMember proposal accepted → on_proposal_accepted → ledger frozen →
///   frozen member's ledger entries are quarantined.
///
/// Proves the governance → ledger enforcement connection: a FreezeMember
/// governance proposal that passes has real economic teeth, not just
/// governance-state and vote-blocking consequences.
#[actix_web::test]
async fn test_freeze_member_blocks_ledger_entries_after_governance_acceptance() {
    const DOMAIN_ID: &str = "test-coop-ledger-freeze";

    // ── Create ledger (temporary sled storage) ───────────────────────────────
    let store = Arc::new(SledStore::temporary().expect("temporary SledStore"));
    let ledger = Ledger::new(store).expect("Ledger::new");
    let ledger_arc = Arc::new(RwLock::new(ledger));

    // ── Member identities ────────────────────────────────────────────────────
    let target_kp = KeyPair::generate().expect("target KeyPair");
    let target_did = target_kp.did().clone();

    let bystander_kp = KeyPair::generate().expect("bystander KeyPair");
    let bystander_did = bystander_kp.did().clone();

    let counterpart_kp = KeyPair::generate().expect("counterpart KeyPair");
    let counterpart_did = counterpart_kp.did().clone();

    // ── Wire on_proposal_accepted to freeze the target in our ledger ─────────
    // This mirrors the production path in server.rs: GovernanceEffect::FreezeMember
    // → ledger.freeze_member_with_metadata().  We use tokio::spawn here (same as
    // production fire-and-forget pattern); the test waits 200ms after close.
    let ledger_for_hook = ledger_arc.clone();
    let on_proposal_accepted: ProposalAcceptedHook = Arc::new(move |effect| {
        if let GovernanceEffect::FreezeMember {
            member,
            reason,
            duration_seconds,
            proposal_id,
            ..
        } = effect
        {
            let ledger = ledger_for_hook.clone();
            tokio::spawn(async move {
                let mut l = ledger.write().await;
                l.freeze_member_with_metadata(
                    member,
                    reason,
                    duration_seconds,
                    Some(proposal_id),
                    None,
                );
            });
        }
    });

    // ── Build test app (governance + auth) ───────────────────────────────────
    let jwt_secret = b"ledger-freeze-enforcement-test-sec32".to_vec();
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
        suspension_checker: None,
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

    // ── Auth: acting DID (domain creator + sole voter) ───────────────────────
    let actor_bundle = IdentityBundle::generate().expect("IdentityBundle");
    let actor_did = actor_bundle.did().to_string();
    let token = get_jwt(&app, &actor_did, &actor_bundle).await;

    // ── Precondition: target is NOT frozen before the proposal ───────────────
    {
        let mut ledger = ledger_arc.write().await;
        assert!(
            !ledger.is_member_frozen(&target_did),
            "pre-condition: target must not be frozen before proposal"
        );
    }

    // ── Precondition: target can submit ledger entries before freeze ──────────
    {
        let entry = JournalEntryBuilder::new(target_did.clone())
            .debit(target_did.clone(), "credits".to_string(), 10)
            .credit(counterpart_did.clone(), "credits".to_string(), 10)
            .with_system_provenance("pre-freeze test")
            .build()
            .expect("build pre-freeze entry");

        let mut ledger = ledger_arc.write().await;
        let result = ledger.append_entry(entry).await;
        assert!(
            result.is_ok(),
            "pre-condition: unfrozen member must be able to submit entries"
        );
    }

    // ── Create governance domain + FreezeMember proposal via manager ─────────
    let actor_governance_did: icn_identity::Did = actor_did.parse().expect("actor DID parse");
    let domain_id_gov = GovernanceDomainId(DOMAIN_ID.to_string());

    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "Ledger Freeze Test Cooperative".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![actor_governance_did.clone()]),
            },
        )
        .await
        .expect("create_domain");

    let proposal_id_val = ProposalId("ledger-freeze-prop-001".to_string());
    governance_manager
        .create_proposal(
            proposal_id_val.clone(),
            domain_id_gov,
            actor_governance_did,
            "Freeze member for ledger abuse".to_string(),
            "Economic fraud detected; freeze ledger access.".to_string(),
            ProposalPayload::FreezeMember {
                member: target_did.clone(),
                reason: "ledger abuse".to_string(),
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
            .insert_header(("Authorization", format!("Bearer {token}")))
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
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "choice": "For", "comment": "Fraud confirmed" }))
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
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(
        close_resp.status().as_u16(),
        200,
        "close_proposal must return 200"
    );

    // ── Allow tokio::spawn in hook to complete ───────────────────────────────
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // ── Assert: target is now frozen in the ledger ────────────────────────────
    {
        let mut ledger = ledger_arc.write().await;
        assert!(
            ledger.is_member_frozen(&target_did),
            "target must be frozen after FreezeMember proposal accepted"
        );
    }

    // ── Assert: frozen member's ledger entry is quarantined ──────────────────
    {
        let frozen_entry = JournalEntryBuilder::new(target_did.clone())
            .debit(target_did.clone(), "credits".to_string(), 5)
            .credit(counterpart_did.clone(), "credits".to_string(), 5)
            .with_system_provenance("post-freeze attempt")
            .build()
            .expect("build frozen entry");

        let mut ledger = ledger_arc.write().await;
        let result = ledger.append_entry(frozen_entry).await;
        assert!(
            result.is_err(),
            "frozen member must not be able to submit ledger entries"
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("frozen"), "error must mention freeze: {err}");
    }

    // ── Assert: non-frozen member is unaffected ───────────────────────────────
    {
        let clean_entry = JournalEntryBuilder::new(bystander_did.clone())
            .debit(bystander_did.clone(), "credits".to_string(), 10)
            .credit(counterpart_did.clone(), "credits".to_string(), 10)
            .with_system_provenance("bystander post-freeze")
            .build()
            .expect("build bystander entry");

        let mut ledger = ledger_arc.write().await;
        let result = ledger.append_entry(clean_entry).await;
        assert!(
            result.is_ok(),
            "non-frozen member must still be able to submit ledger entries"
        );
    }
}
