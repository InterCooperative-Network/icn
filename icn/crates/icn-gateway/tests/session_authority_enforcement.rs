//! Authority-spine enforcement tests (issues #2436, #2437).
//!
//! # What these tests exercise
//!
//! These are not unit tests around an isolated struct. Every case below drives
//! HTTP requests through the **real `jwt_auth` middleware** wrapping routes, with
//! the **same `SessionAuthority` value the production composition registers** as
//! `app_data` in `GatewayServer::run`. The middleware resolves the authority out
//! of `app_data` exactly as it does in production, so a regression that removed
//! revocation from the request path would fail here.
//!
//! # What they do NOT prove
//!
//! They do not construct the full production router — the gateway's route tree
//! is still assembled inside a closure in `GatewayServer::run` that no test can
//! call (issue #2421). What is shared with production is the authority object,
//! the middleware, and the enforcement ordering; what is not shared is the route
//! table. Closing that gap is #2421's job, and until it lands "the auth path is
//! tested" is a claim about this boundary, not about every mounted route.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{test, web, App, HttpResponse};
use actix_web_httpauth::middleware::HttpAuthentication;
use icn_gateway::auth::{AuthManager, TokenClaims};
use icn_gateway::middleware::jwt_auth;
use icn_gateway::session_authority::{
    attenuate_scopes, AuthorityProfile, InMemoryRevocationAuthority, RevocationAuthority,
    SessionAuthority, StoreRevocationAuthority, TokenLifetimePolicy,
};
use icn_identity::Did;

const SECRET: &[u8] = b"session-authority-enforcement-test-secret-32b";

/// A real generated DID — `Did` validates that the encoded key is a genuine
/// 32-byte Ed25519 public key, so a hand-written placeholder is not usable.
fn did() -> Did {
    use std::sync::OnceLock;
    static DID: OnceLock<Did> = OnceLock::new();
    DID.get_or_init(|| {
        icn_identity::IdentityBundle::generate()
            .expect("generate test identity")
            .did()
            .clone()
    })
    .clone()
}

fn scopes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Build a session authority the way production does, with a caller-chosen
/// revocation backend and profile.
fn authority(
    revocation: Arc<dyn RevocationAuthority>,
    profile: AuthorityProfile,
    ttl_hours: u64,
) -> Arc<SessionAuthority> {
    let auth = Arc::new(
        AuthManager::new(SECRET.to_vec())
            .with_token_ttl(TokenLifetimePolicy::from_hours(ttl_hours).unwrap().ttl()),
    );
    Arc::new(
        SessionAuthority::new(
            auth,
            revocation,
            TokenLifetimePolicy::from_hours(ttl_hours).unwrap(),
            profile,
        )
        .expect("authority assembles"),
    )
}

/// A protected app wired the way production wires authenticated scopes:
/// `jwt_auth` middleware reading `SessionAuthority` from `app_data`.
macro_rules! protected_app {
    ($authority:expr) => {{
        let auth_mw = HttpAuthentication::bearer(jwt_auth);
        test::init_service(
            App::new().app_data(web::Data::new($authority)).service(
                web::scope("/v1")
                    .route(
                        "/protected",
                        web::get().to(|| async { HttpResponse::Ok().body("ok") }),
                    )
                    .wrap(auth_mw),
            ),
        )
        .await
    }};
}

async fn get_protected(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
) -> u16 {
    let req = test::TestRequest::get()
        .uri("/v1/protected")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    test::call_service(app, req).await.status().as_u16()
}

// ---------------------------------------------------------------------------
// 1–2. Valid before revocation, rejected after — through the real middleware
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn credential_is_accepted_before_revocation_and_rejected_after() {
    let authority = authority(
        Arc::new(InMemoryRevocationAuthority::new()),
        AuthorityProfile::PortableEvaluator,
        1,
    );
    let app = protected_app!(authority.clone());

    let token = authority
        .auth_manager()
        .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
        .expect("mint");

    assert_eq!(
        get_protected(&app, &token).await,
        200,
        "valid before revoke"
    );

    let claims = authority.auth_manager().verify_token(&token).unwrap();
    authority.revoke(&claims).expect("revoke");

    assert_eq!(
        get_protected(&app, &token).await,
        401,
        "revoked credential must be rejected on the enforcing request path"
    );
}

// ---------------------------------------------------------------------------
// 3. Revocation survives application reconstruction over the same durable state
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn revocation_survives_restart_with_durable_state() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store: Arc<dyn icn_store::Store> =
        Arc::new(icn_store::SledStore::open(dir.path()).expect("open store"));

    let first = authority(
        Arc::new(StoreRevocationAuthority::new(store.clone()).unwrap()),
        AuthorityProfile::Institutional,
        1,
    );
    let token = first
        .auth_manager()
        .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
        .expect("mint");
    let claims = first.auth_manager().verify_token(&token).unwrap();
    first.revoke(&claims).expect("revoke");

    // Simulate a restart: a brand-new authority + app over the SAME store.
    drop(first);
    let restarted = authority(
        Arc::new(StoreRevocationAuthority::new(store).unwrap()),
        AuthorityProfile::Institutional,
        1,
    );
    let app = protected_app!(restarted);

    assert_eq!(
        get_protected(&app, &token).await,
        401,
        "durable revocation must still reject after reconstruction"
    );
}

/// The same scenario on a volatile authority: the credential comes back to life.
/// This is the degraded behavior the evaluator profile accepts and the
/// institutional profile refuses to assemble — asserted so the difference can
/// never be quietly erased.
#[actix_web::test]
async fn volatile_revocation_is_lost_across_restart_as_documented() {
    let first = authority(
        Arc::new(InMemoryRevocationAuthority::new()),
        AuthorityProfile::PortableEvaluator,
        1,
    );
    let token = first
        .auth_manager()
        .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
        .expect("mint");
    let claims = first.auth_manager().verify_token(&token).unwrap();
    first.revoke(&claims).unwrap();
    drop(first);

    let restarted = authority(
        Arc::new(InMemoryRevocationAuthority::new()),
        AuthorityProfile::PortableEvaluator,
        1,
    );
    let app = protected_app!(restarted.clone());
    assert_eq!(
        get_protected(&app, &token).await,
        200,
        "volatile revocation is lost on restart — the honest degraded behavior"
    );
    let caps = restarted.capabilities();
    assert_eq!(caps.revocation_durability, "volatile");
    assert!(caps.notes.iter().any(|n| n.contains("valid again after")));
}

// ---------------------------------------------------------------------------
// 5–6. Configured lifetime is the applied lifetime; expiry boundary
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn configured_lifetime_changes_actual_expiration() {
    for hours in [1u64, 24, 72] {
        let authority = authority(
            Arc::new(InMemoryRevocationAuthority::new()),
            AuthorityProfile::PortableEvaluator,
            hours,
        );
        let token = authority
            .auth_manager()
            .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
            .expect("mint");
        let claims = authority.auth_manager().verify_token(&token).unwrap();
        assert_eq!(
            claims.exp - claims.iat,
            hours * 3600,
            "issued credential must carry the CONFIGURED lifetime, not a hardcoded one"
        );
    }
}

#[actix_web::test]
async fn expired_credentials_are_rejected_at_the_boundary() {
    let authority = authority(
        Arc::new(InMemoryRevocationAuthority::new()),
        AuthorityProfile::PortableEvaluator,
        1,
    );
    let app = protected_app!(authority.clone());
    let now = icn_time::current_timestamp_secs();

    // Hand-mint claims around the expiry boundary using the same secret the
    // authority verifies with.
    let mint = |exp: u64| {
        let claims = TokenClaims {
            sub: did().to_string(),
            iat: now - 10,
            exp,
            coop_id: "test-coop".to_string(),
            scopes: scopes(&["coop:read"]),
            entity_id: None,
            entity_type: None,
            jti: Some("boundary-test-jti".to_string()),
        };
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(SECRET),
        )
        .expect("encode")
    };

    // Comfortably in the future: accepted.
    assert_eq!(get_protected(&app, &mint(now + 3600)).await, 200);
    // Already past: rejected. (jsonwebtoken applies a small default leeway, so
    // the "expired" case is placed clearly outside it rather than at exp-1,
    // which would assert against library leeway rather than our behavior.)
    assert_eq!(get_protected(&app, &mint(now - 3600)).await, 401);
}

// ---------------------------------------------------------------------------
// 7–8. Attenuation: a delegation cannot exceed its issuer
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn delegation_cannot_grant_more_than_the_issuer_holds() {
    let authority = authority(
        Arc::new(InMemoryRevocationAuthority::new()),
        AuthorityProfile::PortableEvaluator,
        1,
    );

    // The #2436 scenario: a narrowly-scoped member credential approving a
    // session that historically minted six broad scopes.
    let member = TokenClaims {
        sub: did().to_string(),
        iat: 0,
        exp: u64::MAX,
        coop_id: "test-coop".to_string(),
        scopes: scopes(&["governance:read", "governance:action-item:complete"]),
        entity_id: None,
        entity_type: None,
        jti: Some("member".to_string()),
    };

    let ceiling = [
        "coop:read",
        "coop:write",
        "ledger:read",
        "ledger:transact",
        "governance:read",
        "governance:write",
    ];

    let (token, granted) = authority
        .issue_delegated(&member, &did(), "test-coop", &ceiling, None)
        .expect("delegation of the issuer's own authority succeeds");

    assert_eq!(
        granted,
        scopes(&["governance:read"]),
        "only the issuer's own overlap with the flow ceiling may be delegated"
    );
    for forbidden in ["coop:write", "ledger:transact", "governance:write"] {
        assert!(
            !granted.contains(&forbidden.to_string()),
            "escalation to {forbidden} must be impossible"
        );
    }

    // And the minted credential really carries only the attenuated scopes.
    let claims = authority.auth_manager().verify_token(&token).unwrap();
    assert_eq!(claims.scopes, scopes(&["governance:read"]));
}

#[actix_web::test]
async fn issuer_with_no_delegable_authority_is_refused() {
    let authority = authority(
        Arc::new(InMemoryRevocationAuthority::new()),
        AuthorityProfile::PortableEvaluator,
        1,
    );
    let outsider = TokenClaims {
        sub: did().to_string(),
        iat: 0,
        exp: u64::MAX,
        coop_id: "test-coop".to_string(),
        scopes: scopes(&["some:unrelated:scope"]),
        entity_id: None,
        entity_type: None,
        jti: Some("outsider".to_string()),
    };
    assert!(
        authority
            .issue_delegated(&outsider, &did(), "test-coop", &["coop:write"], None)
            .is_err(),
        "an issuer holding none of the ceiling's scopes must be refused, not handed a token"
    );
}

#[actix_web::test]
async fn excess_scope_requests_are_narrowed_not_granted() {
    let issuer = scopes(&["governance:read"]);
    let granted = attenuate_scopes(
        &issuer,
        &["governance:read", "governance:write"],
        Some(&scopes(&["governance:read", "governance:write"])),
    )
    .expect("intersection is non-empty");
    assert_eq!(granted, scopes(&["governance:read"]));
}

// ---------------------------------------------------------------------------
// 9, 12. Missing required machinery fails startup rather than degrading
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn institutional_profile_refuses_to_assemble_without_durable_revocation() {
    let auth = Arc::new(AuthManager::new(SECRET.to_vec()));
    let result = SessionAuthority::new(
        auth,
        Arc::new(InMemoryRevocationAuthority::new()),
        TokenLifetimePolicy::default(),
        AuthorityProfile::Institutional,
    );
    let err = result.err().expect("must not assemble").to_string();
    assert!(err.contains("durable session revocation"), "{err}");
    assert!(err.contains("To fix"), "actionable message required: {err}");
}

#[actix_web::test]
async fn authenticated_routes_refuse_to_serve_without_the_authority_installed() {
    // The misassembly guard: if `SessionAuthority` is not registered, the
    // middleware must fail closed rather than fall back to signature-only
    // verification (which is exactly the pre-#2437 behavior).
    let auth_mw = HttpAuthentication::bearer(jwt_auth);
    let app = test::init_service(
        App::new().service(
            web::scope("/v1")
                .route(
                    "/protected",
                    web::get().to(|| async { HttpResponse::Ok().body("ok") }),
                )
                .wrap(auth_mw),
        ),
    )
    .await;

    let token = AuthManager::new(SECRET.to_vec())
        .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
        .expect("mint");

    let status = get_protected(&app, &token).await;
    assert_eq!(
        status, 500,
        "a cryptographically valid credential must NOT be honored when revocation \
         enforcement is not installed"
    );
}

// ---------------------------------------------------------------------------
// 11. The capability report agrees with what was actually assembled
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn capability_report_matches_the_assembled_dependencies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store: Arc<dyn icn_store::Store> =
        Arc::new(icn_store::SledStore::open(dir.path()).expect("open store"));

    let durable = authority(
        Arc::new(StoreRevocationAuthority::new(store).unwrap()),
        AuthorityProfile::Institutional,
        24,
    );
    let caps = durable.capabilities();
    assert_eq!(caps.profile, "institutional");
    assert_eq!(caps.revocation_durability, "durable");
    assert_eq!(caps.revocation_backend, "store");
    assert_eq!(caps.token_ttl_secs, 86_400);
    // Revocation is fully satisfied here, so no revocation-degradation note —
    // but attenuation is still partial and must say so even under the strongest
    // profile. A capability report that goes quiet when the profile is strong is
    // exactly the overclaim this subsystem exists to prevent.
    assert!(
        !caps.notes.iter().any(|n| n.contains("valid again after")),
        "durable revocation must not carry a volatility note: {:?}",
        caps.notes
    );
    assert!(
        caps.notes
            .iter()
            .any(|n| n.contains("enforced only for delegated session approval")),
        "partial attenuation must be reported even on the institutional profile: {:?}",
        caps.notes
    );

    let volatile = authority(
        Arc::new(InMemoryRevocationAuthority::new()),
        AuthorityProfile::PortableEvaluator,
        1,
    );
    let caps = volatile.capabilities();
    assert_eq!(caps.revocation_backend, "memory");
    assert!(!caps.notes.is_empty(), "degradation must be reported");
}

// ---------------------------------------------------------------------------
// 15. One surface cannot be used to bypass the other
// ---------------------------------------------------------------------------

/// The gateway and the RPC server are signed with the same secret and share one
/// revocation store, so a credential revoked on either surface must be rejected
/// on both. This pins the shared key format: writing the entry the way `icn-rpc`
/// writes it, and reading revocation state from the store rather than only from
/// this process's cache.
///
/// Both directions are asserted because they fail differently: gateway→RPC
/// depends on the key prefix and value shape, RPC→gateway depends on the
/// cache-miss fallthrough to the store.
#[actix_web::test]
async fn revocation_is_visible_across_both_authenticated_surfaces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store: Arc<dyn icn_store::Store> =
        Arc::new(icn_store::SledStore::open(dir.path()).expect("open store"));

    let authority = authority(
        Arc::new(StoreRevocationAuthority::new(store.clone()).unwrap()),
        AuthorityProfile::Institutional,
        1,
    );
    let app = protected_app!(authority.clone());

    // -- direction 1: gateway revoke must be written where RPC looks for it --
    let token = authority
        .auth_manager()
        .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
        .expect("mint");
    let claims = authority.auth_manager().verify_token(&token).unwrap();
    let jti = claims.jti.clone().expect("minted credential carries a jti");
    authority.revoke(&claims).expect("revoke");

    let rpc_key = format!("auth:revoked:{jti}").into_bytes();
    let entry = store
        .get(&rpc_key)
        .expect("store read")
        .expect("gateway revocation must be written under the shared RPC key prefix");
    let decoded: serde_json::Value =
        serde_json::from_slice(&entry).expect("entry must be JSON the RPC loader can parse");
    assert_eq!(decoded["jti"], jti);
    assert!(
        decoded["original_expiry"].as_u64().is_some(),
        "RPC's loader drops entries without a usable original_expiry"
    );

    // -- direction 2: an RPC-style revoke must be honored by the gateway ------
    let other = authority
        .auth_manager()
        .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
        .expect("mint");
    let other_jti = authority
        .auth_manager()
        .verify_token(&other)
        .unwrap()
        .jti
        .expect("jti");

    assert_eq!(
        get_protected(&app, &other).await,
        200,
        "precondition: valid before the out-of-process revoke"
    );

    // Written directly to the store, as the RPC surface would — this process's
    // cache knows nothing about it.
    store
        .put(
            format!("auth:revoked:{other_jti}").as_bytes(),
            serde_json::to_vec(&serde_json::json!({
                "jti": other_jti,
                "subject": "",
                "revoked_at": 1,
                "original_expiry": u64::MAX,
                "reason": "revoked via RPC",
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("store write");

    assert_eq!(
        get_protected(&app, &other).await,
        401,
        "a credential revoked on the other surface must not remain valid here"
    );
}

// ---------------------------------------------------------------------------
// 13. A failing revocation store denies; it never authorizes
// ---------------------------------------------------------------------------

/// A revocation authority whose backing state cannot be read.
struct FailingRevocationAuthority;

impl RevocationAuthority for FailingRevocationAuthority {
    fn is_revoked(&self, _jti: &str) -> icn_gateway::error::Result<bool> {
        Err(icn_gateway::error::GatewayError::InternalError(
            "revocation store unavailable".to_string(),
        ))
    }
    fn revoke(&self, _jti: &str, _expires_at: u64) -> icn_gateway::error::Result<()> {
        Ok(())
    }
    fn durability(&self) -> icn_gateway::session_authority::RevocationDurability {
        icn_gateway::session_authority::RevocationDurability::Durable
    }
    fn backend(&self) -> &'static str {
        "failing"
    }
}

#[actix_web::test]
async fn revocation_store_failure_denies_rather_than_authorizes() {
    let authority = authority(
        Arc::new(FailingRevocationAuthority),
        AuthorityProfile::Institutional,
        1,
    );
    let app = protected_app!(authority.clone());
    let token = authority
        .auth_manager()
        .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
        .expect("mint");

    assert_eq!(
        get_protected(&app, &token).await,
        401,
        "an unreadable revocation store must never read as authorization"
    );
}

// ---------------------------------------------------------------------------
// Migration: credentials minted before revocable ids existed
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn pre_jti_credentials_are_refused_by_institutional_profiles() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store: Arc<dyn icn_store::Store> =
        Arc::new(icn_store::SledStore::open(dir.path()).expect("open store"));
    let authority = authority(
        Arc::new(StoreRevocationAuthority::new(store).unwrap()),
        AuthorityProfile::Institutional,
        1,
    );
    let app = protected_app!(authority);

    // A legacy credential: valid signature, valid expiry, no `jti`.
    let now = icn_time::current_timestamp_secs();
    let legacy = TokenClaims {
        sub: did().to_string(),
        iat: now,
        exp: now + 3600,
        coop_id: "test-coop".to_string(),
        scopes: scopes(&["coop:read"]),
        entity_id: None,
        entity_type: None,
        jti: None,
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &legacy,
        &jsonwebtoken::EncodingKey::from_secret(SECRET),
    )
    .expect("encode");

    assert_eq!(
        get_protected(&app, &token).await,
        401,
        "an institution must not accept a credential it cannot withdraw"
    );
}

#[actix_web::test]
async fn every_mint_path_now_produces_a_revocable_credential() {
    // `issue_token` and `issue_entity_token` are the two public mint entry
    // points; every gateway call site (sessions, invites, enrollment,
    // icnctl --local-mint) bottoms out in them.
    let mgr = AuthManager::new(SECRET.to_vec());

    let plain = mgr
        .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
        .unwrap();
    let plain_claims = mgr.verify_token(&plain).unwrap();
    assert!(plain_claims.jti.is_some(), "issue_token must set a jti");

    let entity = mgr
        .issue_entity_token(&did(), "test-coop", None, scopes(&["coop:read"]))
        .unwrap();
    let entity_claims = mgr.verify_token(&entity).unwrap();
    assert!(
        entity_claims.jti.is_some(),
        "issue_entity_token must set a jti"
    );

    assert_ne!(
        plain_claims.jti, entity_claims.jti,
        "credential ids must be unique so they are individually revocable"
    );
}

// ---------------------------------------------------------------------------
// 14. Concurrent verification and revocation behave safely
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn concurrent_verification_and_revocation_are_safe() {
    let authority = authority(
        Arc::new(InMemoryRevocationAuthority::new()),
        AuthorityProfile::PortableEvaluator,
        1,
    );

    let tokens: Vec<String> = (0..32)
        .map(|_| {
            authority
                .auth_manager()
                .issue_token(&did(), "test-coop", scopes(&["coop:read"]))
                .expect("mint")
        })
        .collect();

    // Revoke half while verifying all, from many threads.
    let mut handles = Vec::new();
    for (i, token) in tokens.iter().cloned().enumerate() {
        let authority = authority.clone();
        handles.push(std::thread::spawn(move || {
            if i % 2 == 0 {
                let claims = authority.auth_manager().verify_token(&token).unwrap();
                authority.revoke(&claims).expect("revoke");
            }
            // Verification must never panic or poison the lock regardless of
            // interleaving; the outcome is checked deterministically below.
            let _ = authority.verify(&token);
        }));
    }
    for h in handles {
        h.join().expect("no thread panicked or poisoned a lock");
    }

    for (i, token) in tokens.iter().enumerate() {
        let verified = authority.verify(token);
        if i % 2 == 0 {
            assert!(
                verified.is_err(),
                "revoked credential {i} must stay revoked"
            );
        } else {
            assert!(verified.is_ok(), "untouched credential {i} must stay valid");
        }
    }
}
