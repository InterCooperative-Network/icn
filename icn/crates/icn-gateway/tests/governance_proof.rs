//! Governance proof artifact — end-to-end lifecycle through the live HTTP API
//! with real Ed25519 auth signing.
//!
//! ## What this proves
//!
//! - Auth challenge/verify with a real `IdentityBundle` Ed25519 signature
//!   yields a valid JWT accepted by the gateway middleware.
//! - Governance HTTP handlers (create_domain, create_proposal, open_proposal,
//!   cast_vote, close_proposal, get_proposal) correctly route, authenticate,
//!   and process requests through the canonical apps/governance layer.
//! - The full proposal lifecycle transitions state as expected:
//!   Draft → Open → Accepted (when quorum and approval thresholds are met).
//! - Proposal state is readable back after each mutation step.
//!
//! ## Persistence boundary
//!
//! `GovernanceManager::new()` stores proposals, domains, and votes in
//! in-memory `RwLock<HashMap>s`.  This test therefore proves the live HTTP path
//! and full lifecycle, **not** cross-process restart persistence.
//!
//! True restart persistence is proven separately via the actor-backed path:
//! `GovernanceManager::with_handle(Arc<dyn GovernanceOps>)` where the handle
//! wraps a `GovernanceActor` backed by `SledStore`. See:
//! `apps/governance/tests/persistence_proof.rs` and
//! `docs/development/testing/governance-proof-layers.md`.
//!
//! ## Routes exercised (mirrors server.rs)
//!
//! ```text
//! POST /v1/auth/challenge
//! POST /v1/auth/verify
//! POST /v1/gov/domains
//! GET  /v1/gov/domains/{id}
//! POST /v1/gov/proposals
//! GET  /v1/gov/proposals/{id}
//! POST /v1/gov/proposals/{id}/open
//! POST /v1/gov/proposals/{id}/vote
//! POST /v1/gov/proposals/{id}/close
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use icn_gateway::{api, auth::AuthManager, middleware::jwt_auth, rate_limit::IpRateLimiter};
use icn_governance_actor::{
    events::NoopEventEmitter, http::configure::GovernanceContext, manager::GovernanceManager,
};
use icn_identity::IdentityBundle;
use serde_json::{json, Value};
use std::sync::Arc;

/// Governance lifecycle proof:
/// create domain → create proposal → open → vote (for) → close → assert Accepted.
///
/// Uses a single `IdentityBundle` as both domain creator and sole voter so that
/// one vote satisfies quorum.
#[actix_web::test]
async fn test_governance_proposal_full_lifecycle_with_real_auth() {
    // ── Setup ────────────────────────────────────────────────────────────────
    let jwt_secret = b"governance-proof-test-secret-min32!".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

    // In-memory governance manager.
    // Proposals, domains, votes are in RwLock<HashMap> — not sled-backed in
    // standalone mode. See persistence boundary note at the top of this file.
    let governance_manager = Arc::new(GovernanceManager::new());

    let gov_ctx = GovernanceContext {
        manager: governance_manager.clone(),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
    };

    // Build test app — mirrors the governance-relevant subset of server.rs:
    //   /v1/auth/challenge and /v1/auth/verify (public)
    //   /v1/gov/** (JWT-authenticated via HttpAuthentication::bearer wrapper)
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
                                // Use the fully qualified path to unambiguously call
                                // the function `configure` inside the `configure` module.
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

    // ── Auth: challenge → sign → verify → JWT ───────────────────────────────
    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate must succeed");
    let did = bundle.did().to_string();

    // Challenge
    let challenge_req = test::TestRequest::post()
        .uri("/v1/auth/challenge")
        .set_json(json!({ "did": &did }))
        .to_request();
    let challenge_resp: Value = test::call_and_read_body_json(&app, challenge_req).await;
    let nonce = challenge_resp["nonce"]
        .as_str()
        .expect("challenge response must have nonce field")
        .to_string();
    assert_eq!(
        nonce.len(),
        64,
        "nonce must be 32 bytes hex-encoded (64 chars)"
    );

    // Sign nonce with real Ed25519 key
    let nonce_bytes = hex::decode(&nonce).expect("nonce must be valid hex");
    let signature = bundle
        .sign(&nonce_bytes)
        .expect("IdentityBundle::sign must succeed");
    let sig_hex = hex::encode(signature.to_bytes());

    // Verify → JWT
    let verify_req = test::TestRequest::post()
        .uri("/v1/auth/verify")
        .set_json(json!({
            "did": &did,
            "signature": sig_hex,
            "coop_id": "proof-coop",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(&app, verify_req).await;
    let token = token_resp["token"]
        .as_str()
        .expect("verify response must have token field")
        .to_string();
    assert!(!token.is_empty(), "JWT token must not be empty");

    // ── Step 1: Create governance domain ────────────────────────────────────
    // The acting DID is in `members` so it satisfies membership checks for
    // create_proposal, open_proposal, cast_vote, close_proposal.
    let create_domain_req = test::TestRequest::post()
        .uri("/v1/gov/domains")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "id": "proof-domain",
            "name": "Proof Domain",
            "profile": "cooperative",
            "quorum_percent": 50,
            "approval_percent": 50,
            "voting_period_days": 1,
            "members": [&did]
        }))
        .to_request();
    let create_domain_resp = test::call_service(&app, create_domain_req).await;
    assert_eq!(
        create_domain_resp.status().as_u16(),
        201,
        "create_domain must return 201 Created"
    );

    // ── Step 2: Read domain back ─────────────────────────────────────────────
    let get_domain_req = test::TestRequest::get()
        .uri("/v1/gov/domains/proof-domain")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let domain: Value = test::call_and_read_body_json(&app, get_domain_req).await;
    assert_eq!(
        domain["id"].as_str().unwrap_or(""),
        "proof-domain",
        "domain id must match"
    );
    assert_eq!(
        domain["name"].as_str().unwrap_or(""),
        "Proof Domain",
        "domain name must match"
    );

    // ── Step 3: Create proposal ──────────────────────────────────────────────
    let create_proposal_req = test::TestRequest::post()
        .uri("/v1/gov/proposals")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "domain_id": "proof-domain",
            "title": "Governance Proof Proposal",
            "description": "This proposal exercises the full governance lifecycle.",
            "payload": {
                "type": "text",
                "body": "The ICN governance layer is real and verifiable."
            }
        }))
        .to_request();
    let proposal_create_resp: Value =
        test::call_and_read_body_json(&app, create_proposal_req).await;
    let proposal_id = proposal_create_resp["id"]
        .as_str()
        .expect("create_proposal response must have id field")
        .to_string();
    assert!(!proposal_id.is_empty(), "proposal_id must not be empty");
    // ProposalState::Draft is a unit variant → serializes as the string "Draft"
    assert_eq!(
        proposal_create_resp["state"].as_str().unwrap_or(""),
        "Draft",
        "newly created proposal must be in Draft state"
    );

    // ── Step 4: Open proposal for voting ────────────────────────────────────
    let open_req = test::TestRequest::post()
        .uri(&format!("/v1/gov/proposals/{proposal_id}/open"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "voting_period_seconds": 3600 }))
        .to_request();
    let open_resp = test::call_service(&app, open_req).await;
    assert_eq!(
        open_resp.status().as_u16(),
        200,
        "open_proposal must return 200 OK"
    );
    let opened: Value = test::read_body_json(open_resp).await;
    // ProposalState::Open is a struct variant → serializes as {"Open": {"opened_at": ..., "closes_at": ...}}
    assert!(
        opened["state"]["Open"].is_object(),
        "proposal state must be {{Open: {{...}}}} after opening, got: {}",
        opened["state"]
    );

    // ── Step 5: Cast vote (for) ──────────────────────────────────────────────
    let vote_req = test::TestRequest::post()
        .uri(&format!("/v1/gov/proposals/{proposal_id}/vote"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "choice": "for", "comment": "The proof is sound." }))
        .to_request();
    let vote_resp = test::call_service(&app, vote_req).await;
    assert_eq!(
        vote_resp.status().as_u16(),
        200,
        "cast_vote must return 200 OK"
    );

    // ── Step 6: Read proposal mid-lifecycle ─────────────────────────────────
    let get_mid_req = test::TestRequest::get()
        .uri(&format!("/v1/gov/proposals/{proposal_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let mid_state: Value = test::call_and_read_body_json(&app, get_mid_req).await;
    // ProposalState::Open is a struct variant → serializes as {"Open": {"opened_at": ..., "closes_at": ...}}
    assert!(
        mid_state["state"]["Open"].is_object(),
        "proposal must still be {{Open: {{...}}}} before close, got: {}",
        mid_state["state"]
    );

    // ── Step 7: Close proposal ───────────────────────────────────────────────
    let close_req = test::TestRequest::post()
        .uri(&format!("/v1/gov/proposals/{proposal_id}/close"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let close_resp = test::call_service(&app, close_req).await;
    assert_eq!(
        close_resp.status().as_u16(),
        200,
        "close_proposal must return 200 OK"
    );

    // ── Step 8: Read final state ─────────────────────────────────────────────
    // With 1 member (our DID), quorum_percent=50, approval_percent=50:
    // - quorum_percentage = (1 vote * 100) / 1 member = 100 ≥ 50 → quorum met
    // - approval_percentage = 100 (sole vote is 'for') ≥ 50 → Accepted
    let get_final_req = test::TestRequest::get()
        .uri(&format!("/v1/gov/proposals/{proposal_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let final_state: Value = test::call_and_read_body_json(&app, get_final_req).await;
    // ProposalState::Accepted is a struct variant → serializes as {"Accepted": {"closed_at": ...}}
    assert!(
        final_state["state"]["Accepted"].is_object(),
        "proposal must be {{Accepted: {{...}}}} after close with quorum met, got: {}",
        final_state["state"]
    );
    assert_eq!(
        final_state["id"].as_str().unwrap_or(""),
        proposal_id,
        "returned proposal id must match"
    );
    assert_eq!(
        final_state["title"].as_str().unwrap_or(""),
        "Governance Proof Proposal",
        "returned title must match"
    );

    // ── Direct manager read ──────────────────────────────────────────────────
    // The manager is the same Arc<GovernanceManager> wired into the HTTP
    // handlers above. Reading it directly confirms the handler's write path
    // reaches the in-memory state that backs the HTTP API.
    //
    // Persistence claim: the state is in-memory only. It would be lost on
    // process restart without daemon mode (GovernanceHandle + GovernanceActor).
    let proposal_direct = governance_manager
        .get_proposal(&icn_governance::ProposalId(proposal_id.clone()))
        .await
        .expect("get_proposal must not fail")
        .expect("proposal must exist in manager after lifecycle");

    assert!(
        matches!(
            proposal_direct.state,
            icn_governance::ProposalState::Accepted { .. }
        ),
        "proposal state in manager must be Accepted, got: {:?}",
        proposal_direct.state
    );
}

/// Verify that auth rejects a tampered signature.
/// This proves the auth path is a real cryptographic check, not a no-op.
#[actix_web::test]
async fn test_auth_rejects_invalid_signature() {
    let jwt_secret = b"governance-proof-test-secret-min32!".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_manager.clone()))
            .app_data(web::Data::new(ip_limiter))
            .service(
                web::scope("/v1")
                    .service(api::auth::challenge)
                    .service(api::auth::verify),
            ),
    )
    .await;

    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().to_string();

    // Get a valid challenge
    let challenge_req = test::TestRequest::post()
        .uri("/v1/auth/challenge")
        .set_json(json!({ "did": &did }))
        .to_request();
    let _challenge: Value = test::call_and_read_body_json(&app, challenge_req).await;

    // Submit an all-zero (invalid) signature — must be rejected with 401
    let bad_verify_req = test::TestRequest::post()
        .uri("/v1/auth/verify")
        .set_json(json!({
            "did": &did,
            "signature": hex::encode([0u8; 64]),
            "coop_id": "proof-coop",
            "scopes": ["governance:read"]
        }))
        .to_request();
    let bad_resp = test::call_service(&app, bad_verify_req).await;
    assert_eq!(
        bad_resp.status().as_u16(),
        401,
        "tampered signature must be rejected with 401"
    );
}

/// Verify that governance endpoints reject unauthenticated requests.
#[actix_web::test]
async fn test_governance_endpoints_require_auth() {
    let jwt_secret = b"governance-proof-test-secret-min32!".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
    let governance_manager = Arc::new(GovernanceManager::new());
    let gov_ctx = GovernanceContext {
        manager: governance_manager,
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
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

    // Unauthenticated request to governance endpoint must return 401
    let unauth_req = test::TestRequest::post()
        .uri("/v1/gov/domains")
        .set_json(json!({
            "id": "unauth-domain",
            "name": "Unauth",
            "profile": "cooperative",
            "quorum_percent": 50,
            "approval_percent": 50,
            "voting_period_days": 1,
            "members": ["did:icn:some-did"]
        }))
        .to_request();
    let unauth_resp = test::call_service(&app, unauth_req).await;
    assert_eq!(
        unauth_resp.status().as_u16(),
        401,
        "unauthenticated governance request must return 401"
    );
}
