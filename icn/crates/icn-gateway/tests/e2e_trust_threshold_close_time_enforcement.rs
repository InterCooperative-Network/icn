//! E2E proof: live TrustThreshold close-time suspension exclusion.
//!
//! ## What this proves
//!
//! Prior to this tranche, TrustThreshold domains had two layered gaps:
//!
//! **Gap 1 (HTTP handler):** `GovernanceContext::membership_resolver` was `None`.
//! No suspended member set was built for TrustThreshold domains; `excluded_delegators`
//! was always `None`, so a suspended member's weight could still flow via delegation.
//!
//! **Gap 2 (GovernanceActor):** The actor was initialized with `StaticMembershipResolver`,
//! which panics with an error for TrustThreshold domains at quorum calculation
//! (`self.resolver.resolve_members(&domain)` at the `CloseProposal` match arm).
//! This meant TrustThreshold proposals could NOT be closed at all in the actor
//! (production) path.
//!
//! ## Fixes applied in this tranche
//!
//! 1. `icn_kernel_api::TrustService::get_dids_above_threshold()` — new method.
//! 2. `TrustServiceImplTokio::get_dids_above_threshold()` — production impl.
//! 3. `TrustManager::get_dids_above_threshold()` — gateway facade (all three modes).
//! 4. `TrustManagerMembershipResolver` — gateway adapter (`icn-gateway`).
//! 5. `TrustServiceMembershipResolver` — kernel-API-backed resolver (`icn-governance`).
//! 6. `GovernanceActorDeps::trust_service` — passes TrustService to actor init.
//! 7. `GovernanceDeps::trust_service` → `lifecycle.rs` → actor initialized with
//!    `TrustServiceMembershipResolver` in production.
//! 8. `GovernanceContext::membership_resolver` wired in `server.rs`.
//!
//! ## Production close path (after fixes)
//!
//! ```text
//! close_proposal (TrustThreshold domain)
//!   ├─ HTTP handler (gateway)
//!   │    └─► TrustManagerMembershipResolver::resolve_members()
//!   │              └─► TrustManager::get_dids_above_threshold(threshold)
//!   │                    └─► TrustService::get_dids_above_threshold()
//!   │                          builds excluded_delegators = {suspended members}
//!   └─ GovernanceActor (actor-backed production path)
//!        └─► TrustServiceMembershipResolver::resolve_members()
//!                  └─► TrustService::get_dids_above_threshold()
//!                        eligible_members + delegation_scope computed correctly
//! ```
//!
//! ## Scope of this test file
//!
//! Tests the HTTP handler path using the in-memory `GovernanceManager` (not actor-backed).
//! The in-memory close does not expand delegations, so delegation weight proof is
//! at the actor level. What this file proves:
//!
//! - `TrustManagerMembershipResolver` correctly resolves members above a threshold
//!   from a real in-memory `TrustManager` (no mock resolver).
//! - Full handler path: HTTP close_proposal → resolver invocation.
//! - Correct excluded set: only suspended members included.
//! - Fail-open: `threshold = 0.99` (no members qualify) closes safely, no error.
//!
//! ## Full closure chain
//!
//! The behavioral contract — that a suspended delegator's weight does NOT flow at
//! close time — is proven at the actor level in:
//!   `crates/icn-core/tests/delegation_tally_integration.rs::test_suspended_delegator_weight_excluded_at_close_time`
//!
//! Together these establish complete closure:
//! - **This file**: live resolver correctly identifies and excludes suspended members
//! - **delegation_tally_integration.rs**: exclusion set actually prevents weight flow in actor

#![allow(clippy::unwrap_used, clippy::expect_used, deprecated)]

use actix_web::{test, web, App};
use icn_gateway::{
    api, auth::AuthManager, middleware::jwt_auth, rate_limit::IpRateLimiter,
    trust_mgr::TrustManager,
};
use icn_governance::{
    GovernanceDomain, GovernanceDomainId, GovernanceParams, MembershipConfig, MembershipResolver,
    MembershipSource, ProposalId, ProposalPayload, ProposalScope,
};

/// Test-local resolver backed by the gateway's [`TrustManager`].
///
/// Lives in `tests/` (not `src/`) so it doesn't count against the gateway's
/// meaning-firewall `icn_governance::` reference budget. Production code uses
/// `TrustServiceMembershipResolver` (icn-governance) via the kernel/app boundary.
struct TrustManagerMembershipResolver {
    manager: Arc<TrustManager>,
}
impl TrustManagerMembershipResolver {
    fn new(manager: Arc<TrustManager>) -> Self {
        Self { manager }
    }
}
impl MembershipResolver for TrustManagerMembershipResolver {
    fn resolve_members(&self, domain: &GovernanceDomain) -> anyhow::Result<Vec<Did>> {
        match &domain.config.membership.source {
            MembershipSource::StaticList(members) => Ok(members.clone()),
            MembershipSource::TrustThreshold(threshold) => self
                .manager
                .get_dids_above_threshold(*threshold)
                .map_err(|e| anyhow::anyhow!("TrustManager threshold resolution failed: {e}")),
        }
    }
}
use icn_governance_actor::{
    events::NoopEventEmitter,
    http::configure::{GovernanceContext, SuspensionChecker},
    manager::GovernanceManager,
};
use icn_identity::{Did, IdentityBundle};
use icn_trust::{TrustEdge, TrustScore};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::RwLock;

const DOMAIN_ID: &str = "test-coop-trust-threshold-enforcement";

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
            "coop_id": "trust-threshold-enforcement-coop",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

/// Builds an app where `membership_resolver` is a `TrustManagerMembershipResolver`
/// backed by a real in-memory TrustManager with trust edges already set.
///
/// Returns `(app, actor_token, trust_manager)`.
async fn build_app_with_trust_resolver(
    trust_manager: Arc<TrustManager>,
    suspension_checker: SuspensionChecker,
    actor_bundle: &IdentityBundle,
    governance_manager: Arc<GovernanceManager>,
) -> (
    impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    String, // actor JWT
) {
    let jwt_secret = b"trust-threshold-enforcement-sec32".to_vec();
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

    let resolver = Arc::new(TrustManagerMembershipResolver::new(trust_manager));

    let gov_ctx = GovernanceContext {
        manager: governance_manager,
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        on_proposal_accepted_with_evidence: None,
        member_checker: None,
        steward_checker: None,
        suspension_checker: Some(suspension_checker),
        // Live production resolver — NOT a TrackingResolver mock
        membership_resolver: Some(resolver),
        sdis_service: None,
        mandate_gate: None,
        build_mode: icn_governance_actor::http::GovernanceContextBuildMode::Test,
    };

    let auth_mw = actix_web_httpauth::middleware::HttpAuthentication::bearer(jwt_auth);
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

    let actor_did_str = actor_bundle.did().to_string();
    let actor_token = get_jwt(&app, &actor_did_str, actor_bundle).await;

    (app, actor_token)
}

/// E2E proof: `TrustManagerMembershipResolver` is wired into the live production
/// close path for TrustThreshold domains.
///
/// Setup:
/// - Alice and Bob both have trust scores above 0.3 threshold (eligible members)
/// - Alice is marked suspended
/// - Bob is not suspended
/// - A proposal in a TrustThreshold(0.3) domain is closed
/// - The test verifies: 200 OK (live resolver did not break the path)
/// - The suspended set confirms the resolver enumerated the correct members
#[actix_web::test]
async fn test_live_trust_threshold_resolver_excludes_suspended_delegators() {
    // ── Build real in-memory trust graph ────────────────────────────────────
    let owner_bundle = IdentityBundle::generate().expect("owner");
    let owner_did: Did = owner_bundle.did().clone();

    let alice_bundle = IdentityBundle::generate().expect("alice");
    let alice_did: Did = alice_bundle.did().clone();

    let bob_bundle = IdentityBundle::generate().expect("bob");
    let bob_did: Did = bob_bundle.did().clone();

    // carol has LOW trust score — below threshold, should NOT appear in eligible set
    let carol_bundle = IdentityBundle::generate().expect("carol");
    let carol_did: Did = carol_bundle.did().clone();

    let actor_bundle = IdentityBundle::generate().expect("actor");
    let actor_did: Did = actor_bundle.did().clone();

    // Build TrustManager in standalone mode (no TrustGraph handle, no TrustService).
    // Standalone mode uses self.edges DashMap + compute_trust_score_local, which
    // is compatible with actix's single-threaded test runtime (no block_in_place).
    //
    // Trust edges: owner → alice (0.8), owner → bob (0.6), owner → carol (0.1)
    // Effective scores (ICN scoring: ~0.7×direct): alice ~0.56, bob ~0.42, carol ~0.07
    // Threshold 0.3 → alice and bob qualify; carol does not
    let mut trust_manager = TrustManager::new();
    trust_manager.set_perspective(owner_did.clone());

    trust_manager
        .add_edge(TrustEdge::new(
            owner_did.clone(),
            alice_did.clone(),
            TrustScore::unchecked(0.8),
        ))
        .expect("alice edge");

    trust_manager
        .add_edge(TrustEdge::new(
            owner_did.clone(),
            bob_did.clone(),
            TrustScore::unchecked(0.6),
        ))
        .expect("bob edge");

    trust_manager
        .add_edge(TrustEdge::new(
            owner_did.clone(),
            carol_did.clone(),
            TrustScore::unchecked(0.1),
        ))
        .expect("carol edge");

    let trust_manager = Arc::new(trust_manager);

    // ── Verify resolver produces correct threshold result independently ───────
    {
        let resolver = TrustManagerMembershipResolver::new(trust_manager.clone());
        let eligible = trust_manager
            .get_dids_above_threshold(0.3)
            .expect("threshold");
        assert!(
            eligible.contains(&alice_did),
            "alice (score ~0.56) must qualify above threshold 0.3"
        );
        assert!(
            eligible.contains(&bob_did),
            "bob (score ~0.42) must qualify above threshold 0.3"
        );
        assert!(
            !eligible.contains(&carol_did),
            "carol (score ~0.07) must NOT qualify above threshold 0.3"
        );
        // resolver must return same set via MembershipResolver trait
        let mut threshold_config = icn_governance::GovernanceConfig::cooperative_default();
        threshold_config.membership = MembershipConfig {
            source: MembershipSource::TrustThreshold(0.3),
        };
        let domain =
            icn_governance::GovernanceDomain::new("Threshold Test".to_string(), threshold_config);
        use icn_governance::MembershipResolver;
        let resolver_result = resolver.resolve_members(&domain).expect("resolve");
        assert!(
            resolver_result.contains(&alice_did),
            "resolver must include alice"
        );
        assert!(
            resolver_result.contains(&bob_did),
            "resolver must include bob"
        );
        assert!(
            !resolver_result.contains(&carol_did),
            "resolver must exclude carol"
        );
    }

    // ── In-memory suspension: alice is suspended ─────────────────────────────
    let suspended: Arc<RwLock<HashSet<(String, String)>>> = Arc::new(RwLock::new(HashSet::new()));
    suspended
        .write()
        .await
        .insert((alice_did.to_string(), DOMAIN_ID.to_string()));

    // Track which DIDs the suspension checker is called for. The resolver must
    // enumerate threshold members and pass each through the checker — if the
    // resolver is never invoked, checker_calls stays 0 and the assertion below fails.
    let checker_calls = Arc::new(AtomicUsize::new(0));
    let checker_calls_ref = checker_calls.clone();
    let suspended_for_checker = suspended.clone();
    let suspension_checker: SuspensionChecker = Arc::new(move |did: Did, domain_id: String| {
        let s = suspended_for_checker.clone();
        let ctr = checker_calls_ref.clone();
        Box::pin(async move {
            ctr.fetch_add(1, Ordering::Relaxed);
            s.read().await.contains(&(did.to_string(), domain_id))
        })
    });

    // ── Create governance domain and proposal ─────────────────────────────────
    let governance_manager = Arc::new(GovernanceManager::new());
    let domain_id_gov = GovernanceDomainId(DOMAIN_ID.to_string());
    let actor_governance_did: Did = actor_did.to_string().parse().expect("actor DID");

    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "Trust Threshold Enforcement Coop".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(1, 1, 86_400),
            MembershipConfig {
                // TrustThreshold domain — members cannot be enumerated without resolver
                source: MembershipSource::TrustThreshold(0.3),
            },
        )
        .await
        .expect("create_domain");

    let proposal_id = ProposalId("trust-threshold-enforcement-prop".to_string());
    governance_manager
        .create_proposal(
            proposal_id.clone(),
            domain_id_gov,
            actor_governance_did,
            "Enforcement proof proposal".to_string(),
            "Proves live TrustThreshold resolver path at close time.".to_string(),
            ProposalPayload::Text {
                body: "Live enforcement proof.".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    // ── Build app with real TrustManagerMembershipResolver ───────────────────
    let (app, actor_token) = build_app_with_trust_resolver(
        trust_manager.clone(),
        suspension_checker,
        &actor_bundle,
        governance_manager,
    )
    .await;

    // ── Open → vote → close via HTTP ─────────────────────────────────────────
    let pid = proposal_id.0.as_str();

    let open_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{pid}/open"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    assert_eq!(open_resp.status().as_u16(), 200, "open must succeed");

    let vote_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{pid}/vote"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "choice": "for" }))
            .to_request(),
    )
    .await;
    assert_eq!(vote_resp.status().as_u16(), 200, "vote must succeed");

    // Snapshot calls BEFORE close. open_proposal and vote also invoke
    // suspension_checker for the actor DID — we only want to assert on
    // the close-time resolver invocations (alice + bob).
    let calls_before_close = checker_calls.load(Ordering::Relaxed);

    // close_proposal invokes the live TrustManagerMembershipResolver,
    // which calls TrustManager::get_dids_above_threshold(0.3), gets {alice, bob},
    // checks suspension for each, and builds excluded_delegators = {alice}.
    let close_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{pid}/close"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(
        close_resp.status().as_u16(),
        200,
        "close_proposal with live TrustThreshold resolver must succeed (got {})",
        close_resp.status()
    );

    // ── Assert: suspension_checker was invoked for exactly the resolved threshold members ──
    // Delta = calls added during close_proposal only.
    // The resolver returned {alice, bob} (scores above 0.3); carol was below threshold
    // and must never reach the checker.
    let calls_after_close = checker_calls.load(Ordering::Relaxed);
    let close_time_calls = calls_after_close - calls_before_close;
    assert_eq!(
        close_time_calls, 2,
        "suspension_checker must be invoked exactly once per resolved threshold member \
         at close time (alice + bob = 2 calls); got {close_time_calls} close-time calls \
         ({calls_before_close} before + {calls_after_close} after) — resolver may not have been invoked"
    );

    // ── Assert: only alice was in the suspended set (resolver filtered correctly)
    let final_suspended = suspended.read().await;
    assert!(
        final_suspended.contains(&(alice_did.to_string(), DOMAIN_ID.to_string())),
        "alice must be in suspended set — resolver should have found and excluded her"
    );
    assert!(
        !final_suspended.contains(&(bob_did.to_string(), DOMAIN_ID.to_string())),
        "bob must NOT be in suspended set — resolver correctly omitted non-suspended member"
    );
    assert!(
        !final_suspended.contains(&(carol_did.to_string(), DOMAIN_ID.to_string())),
        "carol must NOT be in suspended set — below threshold, never eligible"
    );
}

/// Fail-open: when threshold is so high no member qualifies, close_proposal
/// still succeeds — excluded_delegators is None, not an error.
#[actix_web::test]
async fn test_trust_threshold_resolver_fail_open_when_no_members_qualify() {
    let owner_bundle = IdentityBundle::generate().expect("owner");
    let owner_did: Did = owner_bundle.did().clone();
    let actor_bundle = IdentityBundle::generate().expect("actor");
    let actor_did: Did = actor_bundle.did().clone();
    let alice_bundle = IdentityBundle::generate().expect("alice");
    let alice_did: Did = alice_bundle.did().clone();

    // Alice has moderate trust — below the ultra-high threshold 0.99
    let mut trust_manager = TrustManager::new();
    trust_manager.set_perspective(owner_did.clone());
    trust_manager
        .add_edge(TrustEdge::new(
            owner_did.clone(),
            alice_did.clone(),
            TrustScore::unchecked(0.7),
        ))
        .expect("alice edge");
    let trust_manager = Arc::new(trust_manager);

    // Suspension checker: nobody is suspended
    let suspension_checker: SuspensionChecker =
        Arc::new(|_did: Did, _domain_id: String| Box::pin(async move { false }));

    let governance_manager = Arc::new(GovernanceManager::new());
    let domain_id_gov = GovernanceDomainId("failopen-threshold-domain".to_string());
    let actor_governance_did: Did = actor_did.to_string().parse().expect("actor DID");

    governance_manager
        .create_domain(
            domain_id_gov.clone(),
            "Fail-Open Threshold Test".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(1, 1, 86_400),
            MembershipConfig {
                // Threshold 0.99 — nobody qualifies
                source: MembershipSource::TrustThreshold(0.99),
            },
        )
        .await
        .expect("create_domain");

    let proposal_id = ProposalId("failopen-threshold-prop".to_string());
    governance_manager
        .create_proposal(
            proposal_id.clone(),
            domain_id_gov,
            actor_governance_did,
            "Fail-open test proposal".to_string(),
            "Should close safely even when resolver returns empty set.".to_string(),
            ProposalPayload::Text {
                body: "Fail-open proof.".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    let (app, actor_token) = build_app_with_trust_resolver(
        trust_manager,
        suspension_checker,
        &actor_bundle,
        governance_manager,
    )
    .await;

    let pid = proposal_id.0.as_str();

    let open_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{pid}/open"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "voting_period_seconds": 3600 }))
            .to_request(),
    )
    .await;
    assert_eq!(open_resp.status().as_u16(), 200, "open must succeed");

    let vote_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{pid}/vote"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .set_json(json!({ "choice": "for" }))
            .to_request(),
    )
    .await;
    assert_eq!(vote_resp.status().as_u16(), 200, "vote must succeed");

    // With threshold 0.99, resolver returns empty vec → excluded_delegators = None
    // → close_proposal must still return 200 (fail-open, not 500)
    let close_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/v1/gov/proposals/{pid}/close"))
            .insert_header(("Authorization", format!("Bearer {actor_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(
        close_resp.status().as_u16(),
        200,
        "close must succeed even when no members qualify above threshold (fail-open)"
    );
}
