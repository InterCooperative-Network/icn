//! Proof for the dev/demo-only standing bootstrap bridge
//! (`POST /v1/commons/dev/bootstrap-standing`).
//!
//! This is the live/HTTP counterpart to PR #1980's in-process helper. It lets a
//! local secured-gateway demo (e.g. a NYCN v4 bash flow) establish commons
//! `Member` standing for the organizer's own DID over HTTP, so the governed
//! proposal lifecycle (`create → open → vote → close`, all already present) can
//! produce a receipt chain that `icnctl audit verify --token` can check.
//!
//! ## What it proves (the six required cases)
//!
//! 1. The endpoint is **disabled by default** (no env) → 403.
//! 2. It is **refused in Production posture** even with the opt-in flag set → 403.
//! 3. It is **available only** when `ICN_ENABLE_ADMIN_ENDPOINTS=true` AND posture
//!    is non-Production → 200, and the caller then holds `Member` standing.
//! 4. A governed proposal is **rejected before** the bootstrap → 403.
//! 5. The same governed proposal **succeeds after** the bootstrap → 201.
//! 6. A *different* authenticated, domain-listed DID that did NOT bootstrap is
//!    **still rejected** → 403 (the `member_checker` gate is not bypassed).
//!
//! It bypasses the multi-steward SDIS proof-of-personhood ceremony, so it is
//! double dev-gated and confined to a local operator-controlled node. It grants
//! standing for the caller's own DID only and weakens no production check.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    // The env guard below serializes process-global env vars across these async
    // tests; its lock is intentionally held across the request `.await`.
    clippy::await_holding_lock
)]

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
use std::sync::{Arc, Mutex, MutexGuard};

const DOMAIN_ID: &str = "dev-standing-bridge-coop";
const BOOTSTRAP_PATH: &str = "/v1/commons/dev/bootstrap-standing";

/// Serializes mutation of the process-global env vars the gate reads.
static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Sets `ICN_ENABLE_ADMIN_ENDPOINTS` / `ICN_GOVERNANCE_BUILD_MODE` for the test's
/// lifetime and restores the previous values on drop, under a shared lock so
/// parallel tests don't race on this process-wide state.
struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    prior_admin: Option<String>,
    prior_mode: Option<String>,
}

impl EnvGuard {
    fn acquire(admin: Option<&str>, mode: Option<&str>) -> Self {
        let lock = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prior_admin = std::env::var("ICN_ENABLE_ADMIN_ENDPOINTS").ok();
        let prior_mode = std::env::var("ICN_GOVERNANCE_BUILD_MODE").ok();
        set_or_clear("ICN_ENABLE_ADMIN_ENDPOINTS", admin);
        set_or_clear("ICN_GOVERNANCE_BUILD_MODE", mode);
        Self {
            _lock: lock,
            prior_admin,
            prior_mode,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        set_or_clear("ICN_ENABLE_ADMIN_ENDPOINTS", self.prior_admin.as_deref());
        set_or_clear("ICN_GOVERNANCE_BUILD_MODE", self.prior_mode.as_deref());
    }
}

fn set_or_clear(key: &str, value: Option<&str>) {
    match value {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

/// Commons-backed `MemberStandingChecker`, identical to the gateway's production
/// wiring (`server.rs`) and to PR #1980's gate test.
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

/// Build a test gateway mounting `/v1/commons` (with the dev endpoint) and
/// `/v1/gov` (with the live `member_checker` gate), sharing one `CommonsManager`.
/// `members` seed the domain's `StaticList` so the governance layer admits them —
/// the only gate left for proposals is the commons checker.
async fn build_app(
    commons_mgr: Arc<CommonsManager>,
    governance_manager: Arc<GovernanceManager>,
    members: &[Did],
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    governance_manager
        .create_domain(
            GovernanceDomainId(DOMAIN_ID.to_string()),
            "Dev Standing Bridge Coop".to_string(),
            "cooperative".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(members.to_vec()),
            },
        )
        .await
        .expect("create_domain");

    let auth_manager = Arc::new(
        // Dev/demo self-service issuance: caller supplies its own coop_id (#2075).
        AuthManager::new(b"dev-standing-bridge-jwt-secret-32".to_vec())
            .with_self_asserted_coop(true),
    );
    let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

    let gov_ctx = GovernanceContext {
        manager: governance_manager.clone(),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        on_proposal_accepted_with_evidence: None,
        member_checker: Some(make_checker(commons_mgr.clone())),
        steward_checker: None,
        suspension_checker: None,
        membership_resolver: None,
        sdis_service: None,
        mandate_gate: None,
        build_mode: icn_governance_actor::http::GovernanceContextBuildMode::Test,
    };

    test::init_service(
        App::new()
            .app_data(web::Data::new(auth_manager.clone()))
            .app_data(web::Data::new(ip_limiter))
            .app_data(web::Data::new(commons_mgr))
            .service(
                web::scope("/v1")
                    .service(api::auth::challenge)
                    .service(api::auth::verify)
                    .service(
                        web::scope("/commons")
                            .configure(api::commons::configure)
                            .wrap(HttpAuthentication::bearer(jwt_auth)),
                    )
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
                            .wrap(HttpAuthentication::bearer(jwt_auth)),
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
            "coop_id": "dev-standing-bridge",
            "scopes": ["governance:read", "governance:write"]
        }))
        .to_request();
    let token_resp: Value = test::call_and_read_body_json(app, verify_req).await;
    token_resp["token"].as_str().expect("token").to_string()
}

fn bootstrap_request(token: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri(BOOTSTRAP_PATH)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "jurisdiction_id": DOMAIN_ID }))
}

fn governed_proposal_request(token: &str, recipient: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/v1/gov/proposals")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "domain_id": DOMAIN_ID,
            "title": "Q1 infrastructure budget",
            "description": "Allocate compute-hours to cluster maintenance.",
            "payload": {
                "type": "budget",
                "amount": 6000,
                "recipient": recipient,
                "currency": "compute-hours",
                "purpose": "q1-infra"
            }
        }))
}

// ── Case 1: disabled by default ──────────────────────────────────────────────
#[actix_web::test]
async fn dev_bootstrap_disabled_by_default() {
    let _env = EnvGuard::acquire(None, None);
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();

    let app = build_app(commons, gov, std::slice::from_ref(&did)).await;
    let token = get_jwt(&app, &did.to_string(), &bundle).await;

    let resp = test::call_service(&app, bootstrap_request(&token).to_request()).await;
    assert_eq!(
        resp.status().as_u16(),
        403,
        "endpoint must be disabled by default (no ICN_ENABLE_ADMIN_ENDPOINTS)"
    );
}

// ── Case 2: refused in Production posture even with the opt-in flag ───────────
#[actix_web::test]
async fn dev_bootstrap_refused_in_production_posture() {
    let _env = EnvGuard::acquire(Some("true"), Some("production"));
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();

    let app = build_app(commons, gov, std::slice::from_ref(&did)).await;
    let token = get_jwt(&app, &did.to_string(), &bundle).await;

    let resp = test::call_service(&app, bootstrap_request(&token).to_request()).await;
    assert_eq!(
        resp.status().as_u16(),
        403,
        "endpoint must be refused in Production posture even if the opt-in flag is set"
    );
}

// ── Case 3: available only when dev-gated, and grants Member standing ─────────
#[actix_web::test]
async fn dev_bootstrap_enabled_grants_member_standing() {
    let _env = EnvGuard::acquire(Some("true"), Some("test"));
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();

    let app = build_app(commons.clone(), gov, std::slice::from_ref(&did)).await;
    let token = get_jwt(&app, &did.to_string(), &bundle).await;

    let resp = test::call_service(&app, bootstrap_request(&token).to_request()).await;
    let status = resp.status().as_u16();
    let body = test::read_body(resp).await;
    assert_eq!(
        status,
        200,
        "endpoint must succeed when dev-gated and non-production; body={}",
        String::from_utf8_lossy(&body)
    );

    // The caller now holds active Member standing in the jurisdiction.
    let holder = commons
        .get_holder_by_did(&did)
        .await
        .unwrap()
        .expect("holder created by bootstrap");
    let affiliations = commons
        .list_affiliations(&hex::encode(holder.id()))
        .await
        .unwrap();
    assert!(
        affiliations.iter().any(|a| {
            a.jurisdiction_id == JurisdictionId::new(DOMAIN_ID)
                && a.membership_status == MembershipStatus::Member
        }),
        "caller must hold Member standing after bootstrap"
    );
}

// ── Cases 4 + 5: governed proposal rejected before, accepted after bootstrap ──
#[actix_web::test]
async fn governed_proposal_gated_before_and_unlocked_after_bootstrap() {
    let _env = EnvGuard::acquire(Some("true"), Some("test"));
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();

    let app = build_app(commons, gov, std::slice::from_ref(&did)).await;
    let token = get_jwt(&app, &did.to_string(), &bundle).await;

    // Case 4: before bootstrap the governed proposal is rejected by member_checker.
    let before = test::call_service(
        &app,
        governed_proposal_request(&token, &did.to_string()).to_request(),
    )
    .await;
    assert_eq!(
        before.status().as_u16(),
        403,
        "governed proposal must be rejected before standing is established"
    );

    // Establish standing via the dev-gated bridge.
    let boot = test::call_service(&app, bootstrap_request(&token).to_request()).await;
    assert_eq!(boot.status().as_u16(), 200, "bootstrap must succeed");

    // Case 5: after bootstrap the same governed proposal is accepted.
    let after = test::call_service(
        &app,
        governed_proposal_request(&token, &did.to_string()).to_request(),
    )
    .await;
    assert_eq!(
        after.status().as_u16(),
        201,
        "governed proposal must be accepted after the dev-gated standing bootstrap"
    );
}

// ── Case 6: a different domain-listed DID that didn't bootstrap is still 403 ──
#[actix_web::test]
async fn other_did_without_bootstrap_still_rejected() {
    let _env = EnvGuard::acquire(Some("true"), Some("test"));
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());

    let bundle_a = IdentityBundle::generate().unwrap();
    let did_a = bundle_a.did().clone();
    let bundle_b = IdentityBundle::generate().unwrap();
    let did_b = bundle_b.did().clone();

    // Both DIDs are in the governance StaticList, so the only gate is the commons
    // checker — proving the dev bootstrap grants standing for A only, and that B
    // (which never bootstrapped) is still rejected by member_checker.
    let app = build_app(commons, gov, &[did_a.clone(), did_b.clone()]).await;
    let token_a = get_jwt(&app, &did_a.to_string(), &bundle_a).await;
    let token_b = get_jwt(&app, &did_b.to_string(), &bundle_b).await;

    // A bootstraps its own standing and can propose.
    let boot = test::call_service(&app, bootstrap_request(&token_a).to_request()).await;
    assert_eq!(boot.status().as_u16(), 200);
    let a_prop = test::call_service(
        &app,
        governed_proposal_request(&token_a, &did_a.to_string()).to_request(),
    )
    .await;
    assert_eq!(
        a_prop.status().as_u16(),
        201,
        "bootstrapped DID A may propose"
    );

    // B never bootstrapped → member_checker still rejects it (no blanket bypass).
    let b_prop = test::call_service(
        &app,
        governed_proposal_request(&token_b, &did_b.to_string()).to_request(),
    )
    .await;
    assert_eq!(
        b_prop.status().as_u16(),
        403,
        "DID B (no bootstrap) must still be rejected — member_checker is not bypassed"
    );
}

// ── Review fix (codex P2): a removed/blocked affiliation is NOT reactivated ──
#[actix_web::test]
async fn dev_bootstrap_refuses_to_reactivate_removed_member() {
    let _env = EnvGuard::acquire(Some("true"), Some("test"));
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();
    let jurisdiction = JurisdictionId::new(DOMAIN_ID);

    // Pre-existing affiliation in a governance-imposed `Banned` state (the shape a
    // FreezeMember/removal decision leaves behind).
    let voucher = KeyPair::generate().unwrap();
    let anchor = commons
        .create_anchor_from_enrollment(&did, Some(voucher.did()))
        .await
        .unwrap();
    let holder = commons
        .create_holder_from_anchor(&hex::encode(anchor.id()), &did)
        .await
        .unwrap();
    let holder_id = hex::encode(holder.id());
    commons
        .join_jurisdiction(
            &holder_id,
            jurisdiction.clone(),
            vec![MembershipCapability::Vote],
        )
        .await
        .unwrap();
    commons
        .update_affiliation_status(&holder_id, &jurisdiction, MembershipStatus::Banned)
        .await
        .unwrap();

    let app = build_app(commons.clone(), gov, std::slice::from_ref(&did)).await;
    let token = get_jwt(&app, &did.to_string(), &bundle).await;

    let resp = test::call_service(&app, bootstrap_request(&token).to_request()).await;
    assert_eq!(
        resp.status().as_u16(),
        403,
        "the dev bridge must refuse to reactivate a Banned affiliation"
    );

    // The affiliation must remain `Banned` — the bridge did not override it.
    let affiliations = commons.list_affiliations(&holder_id).await.unwrap();
    assert!(
        affiliations.iter().any(|a| {
            a.jurisdiction_id == jurisdiction && a.membership_status == MembershipStatus::Banned
        }),
        "Banned affiliation must be left unchanged by the dev bridge"
    );
}

// ── Review fix (Copilot): idempotent when an anchor exists but no holder ──
#[actix_web::test]
async fn dev_bootstrap_idempotent_when_anchor_exists_without_holder() {
    let _env = EnvGuard::acquire(Some("true"), Some("test"));
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();

    // Simulate a prior partial run / earlier SDIS enrollment: an anchor exists for
    // the DID but no commons holder record yet.
    let voucher = KeyPair::generate().unwrap();
    commons
        .create_anchor_from_enrollment(&did, Some(voucher.did()))
        .await
        .unwrap();

    let app = build_app(commons.clone(), gov, std::slice::from_ref(&did)).await;
    let token = get_jwt(&app, &did.to_string(), &bundle).await;

    // Must reuse the existing anchor rather than erroring on "Anchor already exists".
    let resp = test::call_service(&app, bootstrap_request(&token).to_request()).await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "bootstrap must reuse an existing anchor and succeed (idempotent enrollment)"
    );

    let holder = commons
        .get_holder_by_did(&did)
        .await
        .unwrap()
        .expect("holder created from the reused anchor");
    let affiliations = commons
        .list_affiliations(&hex::encode(holder.id()))
        .await
        .unwrap();
    assert!(
        affiliations.iter().any(|a| {
            a.jurisdiction_id == JurisdictionId::new(DOMAIN_ID)
                && a.membership_status == MembershipStatus::Member
        }),
        "caller must hold Member standing after bootstrap reuses the anchor"
    );
}

// ── Review fix (codex P2): a holder removed at the holder level is rejected ──
#[actix_web::test]
async fn dev_bootstrap_refuses_inactive_holder() {
    let _env = EnvGuard::acquire(Some("true"), Some("test"));
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();

    // Pre-existing holder removed at the commons-holder level (`Suspended`) — the
    // shape a holder-level governance removal leaves behind. There is NO blocked
    // affiliation in the target jurisdiction, so only the holder-status guard can
    // catch this; `member_checker` would otherwise honor a fresh Member affiliation.
    let voucher = KeyPair::generate().unwrap();
    let anchor = commons
        .create_anchor_from_enrollment(&did, Some(voucher.did()))
        .await
        .unwrap();
    let holder = commons
        .create_holder_from_anchor(&hex::encode(anchor.id()), &did)
        .await
        .unwrap();
    let holder_id = hex::encode(holder.id());
    let mut removed = commons.get_holder(&holder_id).await.unwrap().unwrap();
    removed.suspend("test removal".to_string(), 0);
    commons
        .update_holder_status(&holder_id, removed.status)
        .await
        .unwrap();

    let app = build_app(commons.clone(), gov, std::slice::from_ref(&did)).await;
    let token = get_jwt(&app, &did.to_string(), &bundle).await;

    let resp = test::call_service(&app, bootstrap_request(&token).to_request()).await;
    assert_eq!(
        resp.status().as_u16(),
        403,
        "the dev bridge must refuse a holder that is not active (holder-level removal)"
    );

    // No Member affiliation was created for the removed holder.
    let affiliations = commons.list_affiliations(&holder_id).await.unwrap();
    assert!(
        !affiliations.iter().any(|a| {
            a.jurisdiction_id == JurisdictionId::new(DOMAIN_ID)
                && a.membership_status == MembershipStatus::Member
        }),
        "an inactive holder must not gain a Member affiliation"
    );
}

// ── Review fix (codex P2): a suspended/revoked anchor is rejected on reuse ──
#[actix_web::test]
async fn dev_bootstrap_refuses_inactive_anchor() {
    let _env = EnvGuard::acquire(Some("true"), Some("test"));
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();

    // Pre-existing personhood anchor for the DID, suspended at the SDIS-anchor
    // level, with NO holder record yet — only the anchor-active guard can catch
    // this (create_holder_from_anchor would otherwise build a fresh active holder).
    let voucher = KeyPair::generate().unwrap();
    let anchor = commons
        .create_anchor_from_enrollment(&did, Some(voucher.did()))
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());
    let mut removed = commons.get_anchor(&anchor_id).await.unwrap().unwrap();
    removed.suspend("test removal".to_string(), voucher.did().clone(), None);
    commons
        .update_anchor_status(&anchor_id, removed.status)
        .await
        .unwrap();

    let app = build_app(commons.clone(), gov, std::slice::from_ref(&did)).await;
    let token = get_jwt(&app, &did.to_string(), &bundle).await;

    let resp = test::call_service(&app, bootstrap_request(&token).to_request()).await;
    assert_eq!(
        resp.status().as_u16(),
        403,
        "the dev bridge must refuse a suspended/revoked personhood anchor"
    );

    // No holder (and thus no Member standing) was created from the inactive anchor.
    assert!(
        commons.get_holder_by_did(&did).await.unwrap().is_none(),
        "no holder may be created from an inactive anchor"
    );
}

// ── Review fix (codex P2): existing holder with a revoked backing anchor → 403 ─
#[actix_web::test]
async fn dev_bootstrap_refuses_holder_with_inactive_backing_anchor() {
    let _env = EnvGuard::acquire(Some("true"), Some("test"));
    let commons = Arc::new(CommonsManager::new());
    let gov = Arc::new(GovernanceManager::new());
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();

    // An ACTIVE holder whose backing anchor is suspended AFTER holder creation.
    // Anchor status does not cascade to the holder, so the holder stays `Active`
    // and only the backing-anchor guard can catch this (the no-holder anchor guard
    // does not run because a holder already exists).
    let voucher = KeyPair::generate().unwrap();
    let anchor = commons
        .create_anchor_from_enrollment(&did, Some(voucher.did()))
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());
    let holder = commons
        .create_holder_from_anchor(&anchor_id, &did)
        .await
        .unwrap();
    let holder_id = hex::encode(holder.id());
    let mut removed_anchor = commons.get_anchor(&anchor_id).await.unwrap().unwrap();
    removed_anchor.suspend("test removal".to_string(), voucher.did().clone(), None);
    commons
        .update_anchor_status(&anchor_id, removed_anchor.status)
        .await
        .unwrap();

    // Precondition: the holder is still `Active` (anchor status did not cascade).
    assert!(
        commons
            .get_holder(&holder_id)
            .await
            .unwrap()
            .unwrap()
            .is_active(),
        "precondition: holder stays Active when its backing anchor is suspended"
    );

    let app = build_app(commons.clone(), gov, std::slice::from_ref(&did)).await;
    let token = get_jwt(&app, &did.to_string(), &bundle).await;

    let resp = test::call_service(&app, bootstrap_request(&token).to_request()).await;
    assert_eq!(
        resp.status().as_u16(),
        403,
        "an Active holder backed by a suspended/revoked anchor must be refused"
    );

    // No Member affiliation was granted.
    let affiliations = commons.list_affiliations(&holder_id).await.unwrap();
    assert!(
        !affiliations.iter().any(|a| {
            a.jurisdiction_id == JurisdictionId::new(DOMAIN_ID)
                && a.membership_status == MembershipStatus::Member
        }),
        "no Member affiliation may be granted when the backing anchor is inactive"
    );
}
