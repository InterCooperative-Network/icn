//! `/v1/sdis/recovery/{id}/approve` route-authority enforcement.
//!
//! # What these tests exercise
//!
//! Recovery steward-approval is a steward institutional act: advancing a recovery
//! ceremony to its approval threshold authorizes rotating an Anchor's keys (the
//! public `start` -> steward `approve` -> public `complete` chain). Historically
//! `approve_recovery` was mounted as an **unwrapped** sibling under `/sdis`,
//! authenticated nothing, and was reachable by any caller whenever
//! `ICN_ENABLE_ADMIN_ENDPOINTS=true` — the same defect class PR #2446 fixed for
//! enrollment vouch, omitted for recovery.
//!
//! These cases drive `approve_recovery` through the **real `jwt_auth` middleware**
//! over the **same `SessionAuthority`** production registers, mounted the way
//! `GatewayServer::run` mounts it: the public recovery surface unwrapped, the
//! steward approval behind `jwt_auth` via `recovery::configure_protected`.
//!
//! # What they do NOT prove
//!
//! They do not construct the full production router (#2421), and — because the
//! recovery ceremony carries no cooperative binding — they do not prove
//! cooperative-scoped authorization (an owed institutional decision; see #2447 /
//! ADR-0085). What they prove is that the approval is unreachable without a
//! verified steward credential and stays fail-closed as a dev/test simulation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex, MutexGuard};

use actix_web::{test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use icn_gateway::api::sdis::recovery::{self, RecoveryStore};
use icn_gateway::auth::AuthManager;
use icn_gateway::middleware::jwt_auth;
use icn_gateway::session_authority::{
    AuthorityProfile, InMemoryRevocationAuthority, SessionAuthority, TokenLifetimePolicy,
};
use icn_identity::Did;

const SECRET: &[u8] = b"sdis-recovery-authority-test-secret-32b";

/// The canonical steward capability, and the broad governance scope accepted
/// alongside it during the #1868 decomposition (mirrors `STEWARD_ACT_SCOPES`).
const STEWARD_SCOPE: &str = "governance:steward:write";
const BROAD_GOVERNANCE_SCOPE: &str = "governance:write";

/// Serializes mutation of the process-global `ICN_ENABLE_ADMIN_ENDPOINTS` the
/// approval gate reads (same approach as `dev_standing_bootstrap_gate.rs`).
static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Sets `ICN_ENABLE_ADMIN_ENDPOINTS` for the test's lifetime and restores the
/// previous value on drop, under a shared lock so parallel tests don't race.
struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    prior_admin: Option<String>,
}

impl EnvGuard {
    fn acquire(admin: Option<&str>) -> Self {
        let lock = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prior_admin = std::env::var("ICN_ENABLE_ADMIN_ENDPOINTS").ok();
        set_or_clear("ICN_ENABLE_ADMIN_ENDPOINTS", admin);
        Self {
            _lock: lock,
            prior_admin,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        set_or_clear("ICN_ENABLE_ADMIN_ENDPOINTS", self.prior_admin.as_deref());
    }
}

fn set_or_clear(key: &str, value: Option<&str>) {
    match value {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

fn generated_did() -> Did {
    icn_identity::IdentityBundle::generate()
        .expect("generate test identity")
        .did()
        .clone()
}

fn scopes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn authority(ttl_hours: u64) -> Arc<SessionAuthority> {
    let lifetime = TokenLifetimePolicy::from_hours(ttl_hours).unwrap();
    let auth = Arc::new(AuthManager::new(SECRET.to_vec()).with_token_ttl(lifetime.ttl()));
    Arc::new(
        SessionAuthority::new(
            auth,
            Arc::new(InMemoryRevocationAuthority::new()),
            lifetime,
            AuthorityProfile::PortableEvaluator,
        )
        .expect("authority assembles"),
    )
}

fn mint(authority: &SessionAuthority, did: &Did, scope_list: &[&str]) -> String {
    authority
        .auth_manager()
        .issue_token(did, "test-coop", scopes(scope_list))
        .expect("mint")
}

/// Mount `/sdis` recovery the way production does: the public recovery surface
/// (start/status/complete) unwrapped, steward approval behind `jwt_auth`.
macro_rules! recovery_app {
    ($authority:expr) => {{
        let auth_mw = HttpAuthentication::bearer(jwt_auth);
        test::init_service(
            App::new()
                .app_data(web::Data::new($authority))
                .app_data(web::Data::new(Arc::new(RecoveryStore::new())))
                .service(
                    web::scope("/v1/sdis")
                        .configure(recovery::configure)
                        .service(
                            web::scope("")
                                .wrap(auth_mw)
                                .configure(recovery::configure_protected),
                        ),
                ),
        )
        .await
    }};
}

/// Start a real recovery ceremony through the public route and return its id.
async fn start_recovery_ceremony(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
) -> String {
    let start = test::TestRequest::post()
        .uri("/v1/sdis/recovery/start")
        .set_json(serde_json::json!({
            "anchor_id": "anchor-under-recovery",
            "verification_data": { "type": "selfie" },
            "new_keybundle": {
                "ed25519_pub": "ed25519-new",
                "ml_dsa_pub": "ml-dsa-new",
                "x25519_pub": "x25519-new",
            },
        }))
        .to_request();
    let started: serde_json::Value = test::call_and_read_body_json(app, start).await;
    started["recovery_id"]
        .as_str()
        .expect("recovery_id in start response")
        .to_string()
}

async fn post_approve(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    recovery_id: &str,
    token: Option<&str>,
) -> u16 {
    let mut req =
        test::TestRequest::post().uri(&format!("/v1/sdis/recovery/{recovery_id}/approve"));
    if let Some(t) = token {
        req = req.insert_header(("Authorization", format!("Bearer {t}")));
    }
    test::call_service(app, req.to_request())
        .await
        .status()
        .as_u16()
}

// ---------------------------------------------------------------------------
// 1. Authentication: the approval surface refuses anonymous callers
// ---------------------------------------------------------------------------

/// The core defect: an anonymous POST could advance a steward-approval counter.
/// It must be refused before the handler — and, crucially, refused EVEN WITH the
/// admin flag enabled, proving the fix is authentication, not merely the env gate.
#[actix_web::test]
async fn anonymous_caller_cannot_approve_recovery_even_with_admin_enabled() {
    let _env = EnvGuard::acquire(Some("true"));
    let authority = authority(1);
    let app = recovery_app!(authority.clone());

    assert_eq!(
        post_approve(&app, "any-recovery-id", None).await,
        401,
        "an unauthenticated caller must not be able to record a steward approval"
    );
}

/// The recovery approval route inherits the shared authority's revocation: a
/// credential withdrawn through `SessionAuthority` stops working on the next call.
#[actix_web::test]
async fn revoked_credential_is_refused_on_recovery_approval() {
    let _env = EnvGuard::acquire(Some("true"));
    let authority = authority(1);
    let app = recovery_app!(authority.clone());
    let did = generated_did();
    let token = mint(&authority, &did, &[STEWARD_SCOPE]);

    // Before revoking, the credential authenticates + authorizes + passes the
    // admin gate, so only the missing ceremony refuses it (404). Pinning 404
    // keeps the revocation assertion below from passing vacuously on a 500.
    assert_eq!(
        post_approve(&app, "no-such-ceremony", Some(&token)).await,
        404,
        "a live steward credential should reach the handler before revocation"
    );

    let claims = authority.auth_manager().verify_token(&token).unwrap();
    authority.revoke(&claims).expect("revoke");

    assert_eq!(
        post_approve(&app, "no-such-ceremony", Some(&token)).await,
        401,
        "a revoked credential must not approve recovery"
    );
}

// ---------------------------------------------------------------------------
// 2. Authorization: authentication alone is not steward authority
// ---------------------------------------------------------------------------

/// A valid member credential with no steward capability must be refused (403),
/// even with admin endpoints enabled — authentication is not authorization.
#[actix_web::test]
async fn authenticated_member_without_steward_capability_is_refused() {
    let _env = EnvGuard::acquire(Some("true"));
    let authority = authority(1);
    let app = recovery_app!(authority.clone());
    let token = mint(
        &authority,
        &generated_did(),
        &["coop:read", "governance:read"],
    );

    assert_eq!(
        post_approve(&app, "any-recovery-id", Some(&token)).await,
        403,
        "any authenticated user is NOT a steward"
    );
}

/// Both the narrow steward capability and the broad governance scope carry
/// steward authority (mirrors the enrollment surface — one notion of steward
/// authority, not a second divergent one).
#[actix_web::test]
async fn steward_capability_and_broad_governance_scope_are_both_accepted() {
    let _env = EnvGuard::acquire(Some("true"));
    let authority = authority(1);
    let app = recovery_app!(authority.clone());

    for scope in [STEWARD_SCOPE, BROAD_GOVERNANCE_SCOPE] {
        let token = mint(&authority, &generated_did(), &[scope]);
        // Nonexistent ceremony: 404 is the precise success signal — reaching the
        // handler's own NotFound proves authentication, authorization, AND the
        // admin gate all passed. "not 401/403" would also pass on a 500.
        assert_eq!(
            post_approve(&app, "no-such-ceremony", Some(&token)).await,
            404,
            "scope {scope} must authenticate and carry steward authority"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Dev/test simulation stays fail-closed
// ---------------------------------------------------------------------------

/// Even a genuine steward cannot use this dev/test simulation when admin
/// endpoints are disabled: the endpoint is fail-closed by default. (Auth is
/// checked first, so this 403 is the admin gate, reached only by a real steward.)
#[actix_web::test]
async fn steward_is_refused_when_admin_endpoints_disabled() {
    let _env = EnvGuard::acquire(None); // admin flag unset -> disabled
    let authority = authority(1);
    let app = recovery_app!(authority.clone());
    let token = mint(&authority, &generated_did(), &[STEWARD_SCOPE]);

    assert_eq!(
        post_approve(&app, "no-such-ceremony", Some(&token)).await,
        403,
        "the dev/test approval simulation must be fail-closed by default"
    );
}

// ---------------------------------------------------------------------------
// 4. The success path, and what it records
// ---------------------------------------------------------------------------

/// An authorized steward, with admin enabled, on a real ceremony started through
/// the public route: the approval SUCCEEDS, the counter advances, and the
/// recorded approver is the VERIFIED credential subject (there is no body field
/// that could redirect attribution).
#[actix_web::test]
async fn authorized_steward_approval_succeeds_and_records_verified_subject() {
    let _env = EnvGuard::acquire(Some("true"));
    let authority = authority(1);
    let app = recovery_app!(authority.clone());

    let recovery_id = start_recovery_ceremony(&app).await;

    let steward = generated_did();
    let token = mint(&authority, &steward, &[STEWARD_SCOPE]);
    let req = test::TestRequest::post()
        .uri(&format!("/v1/sdis/recovery/{recovery_id}/approve"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "a steward with the capability must be able to approve when enabled"
    );

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        body["approved_by"].as_str().unwrap_or_default(),
        steward.to_string(),
        "the recorded approver must be the verified credential subject"
    );
    assert_eq!(
        body["approvals"].as_u64().unwrap_or_default(),
        1,
        "the approval must advance the ceremony's steward counter"
    );
}

// ---------------------------------------------------------------------------
// 5. Scope ordering: the public surface is preserved, approval is not exposed
// ---------------------------------------------------------------------------

/// The scope split must not break the holder-side flow: starting a recovery has
/// no credential yet and must stay anonymous — while steward approval on the same
/// `/recovery` prefix is unreachable without a credential (no unwrapped path).
#[actix_web::test]
async fn public_recovery_flow_preserved_and_approval_not_exposed() {
    let _env = EnvGuard::acquire(Some("true"));
    let authority = authority(1);
    let app = recovery_app!(authority.clone());

    // Public initiation stays anonymous AND working (pinned 200, not "not 401").
    let start = test::TestRequest::post()
        .uri("/v1/sdis/recovery/start")
        .set_json(serde_json::json!({
            "anchor_id": "anchor-x",
            "verification_data": {},
            "new_keybundle": {
                "ed25519_pub": "e", "ml_dsa_pub": "m", "x25519_pub": "x"
            },
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, start).await.status().as_u16(),
        200,
        "public recovery initiation must remain anonymous and working"
    );

    // Steward approval on the same prefix is NOT reachable through the public
    // scope: refused with 401, not merely 404'd away.
    assert_eq!(
        post_approve(&app, "any-recovery-id", None).await,
        401,
        "steward approval must be refused without a credential"
    );
}

/// The off-loopback / opt-out composition: when `recovery::configure_protected`
/// is NOT mounted (the server's `mount_admin_recovery` gate is false — see
/// `admin_endpoints_allowed` in server.rs, unit-tested there), the approve route
/// is absent entirely and cannot reach `approve_by_steward`. This asserts the
/// safe-by-construction claim end-to-end: the public routes still resolve, but
/// approval returns 404 (no such route) rather than existing-but-forbidden.
#[actix_web::test]
async fn approve_route_absent_when_protected_config_not_mounted() {
    let authority = authority(1);
    // Public recovery routes only — mirrors the mounting when the loopback+opt-in
    // gate is not satisfied, so `configure_protected` is never applied.
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(authority))
            .app_data(web::Data::new(Arc::new(RecoveryStore::new())))
            .service(web::scope("/v1/sdis").configure(recovery::configure)),
    )
    .await;

    // A public route still resolves (200) — the composition is otherwise intact.
    let start = test::TestRequest::post()
        .uri("/v1/sdis/recovery/start")
        .set_json(serde_json::json!({
            "anchor_id": "anchor-y",
            "verification_data": {},
            "new_keybundle": { "ed25519_pub": "e", "ml_dsa_pub": "m", "x25519_pub": "x" },
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, start).await.status().as_u16(),
        200,
        "public recovery routes must still work when the admin route is unmounted"
    );

    // The approve route does not exist in this composition -> 404, never reaching
    // the steward-approval handler. The route is absent regardless of credential,
    // so no token is needed to demonstrate it.
    assert_eq!(
        post_approve(&app, "any-recovery-id", None).await,
        404,
        "an unmounted approve route must be absent (404), not reachable"
    );
}
