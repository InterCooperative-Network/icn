//! Proof: accepted FreezeMember proposal → `on_proposal_accepted` fires →
//! member marked suspended → subsequent `open_proposal` call on a
//! pre-existing proposal is denied 403.
//!
//! ## Chain under test
//!
//! ```text
//! close_proposal(FreezeMember) (HTTP 200)
//!   └─► on_proposal_accepted(GovernanceEffect::FreezeMember)
//!         └─► tokio::spawn → suspended_set.insert(member_did)
//!               └─► open_proposal (HTTP 403) for suspended member
//!                     └─► suspension_checker returns true
//!                           └─► err_forbidden("suspended members may not open proposals")
//! ```
//!
//! ## What this proves
//!
//! 1. A suspended member who authored a proposal before being frozen cannot
//!    call `open_proposal` after the FreezeMember proposal is accepted.
//! 2. A non-suspended member can open the same proposal normally.
//! 3. The gate is scoped — only the suspended member is denied.
//!
//! ## What this is NOT
//!
//! - Vote-casting enforcement (covered in e2e_vote_standing_gate.rs).
//! - Ledger enforcement (covered in e2e_ledger_freeze_enforcement.rs).
//! - Proposal-creation enforcement (covered in e2e_proposal_submission_gate.rs).
//! - Auto-expiration testing (separate gap, not in scope here).
//! - Full atomic suspension (best-effort hook, not transactional).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use icn_gateway::{api, auth::AuthManager, middleware::jwt_auth, rate_limit::IpRateLimiter};
use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams, MembershipConfig,
    MembershipSource, ProposalId, ProposalPayload, ProposalScope,
};
use icn_governance_actor::{
    events::NoopEventEmitter,
    http::configure::{
        GovernanceContext, GovernanceEffect, ProposalAcceptedHook, SuspensionChecker,
    },
    manager::GovernanceManager,
    receipt_backend::GovernanceReceiptBackend,
};
use icn_identity::{Did, IdentityBundle, KeyPair};
use icn_kernel_api::receipts::CanonicalReceipt;
use icn_kernel_api::{AllocationReceipt, Hash};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::RwLock;

#[derive(Default)]
struct TestReceiptBackend {
    governance_by_proposal: Mutex<HashMap<String, GovernanceDecisionReceipt>>,
    governance_by_decision: Mutex<HashMap<Hash, GovernanceDecisionReceipt>>,
    allocations_by_decision: Mutex<HashMap<Hash, Vec<AllocationReceipt>>>,
}

impl GovernanceReceiptBackend for TestReceiptBackend {
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String> {
        self.governance_by_proposal
            .lock()
            .map_err(|e| e.to_string())?
            .insert(receipt.proposal_id.clone(), receipt.clone());
        self.governance_by_decision
            .lock()
            .map_err(|e| e.to_string())?
            .insert(receipt.decision_hash, receipt.clone());
        Ok(())
    }

    fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance_by_proposal
            .lock()
            .map_err(|e| e.to_string())?
            .get(proposal_id)
            .cloned())
    }

    fn put_allocation(&self, receipt: &AllocationReceipt) -> Result<Hash, String> {
        self.allocations_by_decision
            .lock()
            .map_err(|e| e.to_string())?
            .entry(receipt.decision_hash)
            .or_default()
            .push(receipt.clone());
        Ok(receipt.canonical_hash())
    }

    fn get_governance_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance_by_decision
            .lock()
            .map_err(|e| e.to_string())?
            .get(decision_hash)
            .cloned())
    }

    fn list_allocations_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<AllocationReceipt>, String> {
        Ok(self
            .allocations_by_decision
            .lock()
            .map_err(|e| e.to_string())?
            .get(decision_hash)
            .cloned()
            .unwrap_or_default())
    }
}

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
            "coop_id": "open-gate-test-coop",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

/// E2E open_proposal suspension gate:
///   member creates proposal → FreezeMember accepted → member suspended →
///   member's open_proposal call returns 403, non-suspended member's returns 200.
#[actix_web::test]
async fn test_suspended_member_cannot_open_proposal() {
    const DOMAIN_ID: &str = "test-coop-open-proposal-gate";

    // ── In-memory suspension store ───────────────────────────────────────────
    let suspended: Arc<RwLock<HashSet<(String, String)>>> = Arc::new(RwLock::new(HashSet::new()));

    // ── Member identities ────────────────────────────────────────────────────
    let target_kp = KeyPair::generate().expect("target KeyPair");
    let target_did: Did = target_kp.did().clone();

    let bystander_kp = KeyPair::generate().expect("bystander KeyPair");

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

    // ── Build test app ────────────────────────────────────────────────────────
    let jwt_secret = b"open-proposal-gate-enforcement-sec32".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
    let governance_manager = Arc::new(
        GovernanceManager::new().with_receipt_store(Arc::new(TestReceiptBackend::default())),
    );

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
        mandate_gate: None,
        build_mode: icn_governance_actor::http::GovernanceContextBuildMode::Test,
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
    let bystander_did_str = bystander_bundle.did().to_string();
    let bystander_token = get_jwt(&app, &bystander_did_str, &bystander_bundle).await;

    // ── Create governance domain ──────────────────────────────────────────────
    let actor_governance_did: Did = actor_did_str.parse().expect("actor DID parse");
    // Target and bystander must be domain members so the membership check passes
    // and the tests reach the suspension gate. Bystander is not suspended and
    // proves the gate is scoped to the suspended member only.
    let target_governance_did: Did = target_did_str.parse().expect("target DID parse");
    let bystander_governance_did: Did = bystander_did_str.parse().expect("bystander DID parse");
    let domain_id_gov = GovernanceDomainId(DOMAIN_ID.to_string());

    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "Open Proposal Gate Test Cooperative".to_string(),
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

    // ── Create the subject proposal (Text) that the target will try to open ──
    // Created directly via manager before the freeze so it exists pre-suspension.
    let subject_proposal_id = ProposalId("open-gate-subject-prop".to_string());
    governance_manager
        .create_proposal(
            subject_proposal_id.clone(),
            domain_id_gov.clone(),
            actor_governance_did.clone(),
            "Subject proposal".to_string(),
            "This is the proposal the suspended member will try to open.".to_string(),
            ProposalPayload::Text {
                body: "Proposal body for open_proposal gate test.".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create subject proposal");

    // ── Precondition: target (not yet suspended) can open the proposal ────────
    let pre_freeze_open = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{}/open", subject_proposal_id.0))
            .insert_header(("Authorization", format!("Bearer {target_token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    // `member_checker` is None so any JWT holder can open; expect 200.
    assert_eq!(
        pre_freeze_open.status().as_u16(),
        200,
        "pre-condition: unsuspended target must be able to open proposals"
    );

    // ── Create a second subject proposal (the freeze target will try to open post-freeze) ──
    let post_freeze_subject_id = ProposalId("open-gate-post-freeze-prop".to_string());
    governance_manager
        .create_proposal(
            post_freeze_subject_id.clone(),
            domain_id_gov.clone(),
            actor_governance_did.clone(),
            "Post-freeze subject proposal".to_string(),
            "This proposal exists for the post-freeze open attempt.".to_string(),
            ProposalPayload::Text {
                body: "Post-freeze proposal body.".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create post-freeze subject proposal");

    // ── Create and run FreezeMember proposal ─────────────────────────────────
    let freeze_proposal_id = ProposalId("open-gate-freeze-prop".to_string());
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

    // ── Assert: suspended member cannot open the post-freeze subject proposal ─
    let post_freeze_open = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/v1/gov/proposals/{}/open",
                post_freeze_subject_id.0
            ))
            .insert_header(("Authorization", format!("Bearer {target_token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    assert_eq!(
        post_freeze_open.status().as_u16(),
        403,
        "suspended member must be denied open_proposal (got {})",
        post_freeze_open.status().as_u16()
    );

    // ── Assert: non-suspended bystander can still open the same proposal ───��──
    let bystander_open = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/v1/gov/proposals/{}/open",
                post_freeze_subject_id.0
            ))
            .insert_header(("Authorization", format!("Bearer {bystander_token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    assert_eq!(
        bystander_open.status().as_u16(),
        200,
        "non-suspended bystander must be able to open proposals (got {})",
        bystander_open.status().as_u16()
    );
}
