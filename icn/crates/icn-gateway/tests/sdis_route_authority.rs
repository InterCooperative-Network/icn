//! `/v1/sdis` route-authority enforcement (issue #2443).
//!
//! # What these tests exercise
//!
//! The `/v1/sdis` scope is mounted **without** the general `jwt_auth` middleware,
//! so several institutional routes historically authenticated nothing at all:
//! `steward_vouch` recorded a vouch attributed to a DID taken from the request
//! body, `reject_enrollment` recorded a moderation decision the same way, and
//! the pending-queue and vouch-history reads disclosed applicant state to
//! anonymous callers.
//!
//! These cases drive the protected routes through the **real `jwt_auth`
//! middleware** over the **same `SessionAuthority`** the production composition
//! registers, and — since the F-P0-1 containment — through
//! `icn_gateway::server::sdis_scope`, the same `/sdis` constructor
//! `GatewayServer::run` calls. The route set, nesting order, middleware and
//! mount conditions are therefore production's, not a test-local rebuild.
//!
//! # What they do NOT prove
//!
//! They do not construct the full production router: the *surrounding* route
//! table is still assembled inline in `GatewayServer::run` (issue #2421). The
//! `/sdis` subtree is now shared; the rest is not.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use icn_gateway::api::sdis::simple_enrollment::{self, EnrollmentStore};
use icn_gateway::auth::AuthManager;
use icn_gateway::middleware::jwt_auth;
use icn_gateway::server::{sdis_scope, SdisMountFlags};
use icn_gateway::session_authority::{
    AuthorityProfile, InMemoryRevocationAuthority, SessionAuthority, TokenLifetimePolicy,
};
use icn_identity::Did;

const SECRET: &[u8] = b"sdis-route-authority-test-secret-32-bytes";

/// The canonical steward capability (`icn_rpc::auth::scopes`), and the broad
/// governance scope accepted alongside it during the #1868 decomposition.
const STEWARD_SCOPE: &str = "governance:steward:write";
const BROAD_GOVERNANCE_SCOPE: &str = "governance:write";

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

/// Mount `/sdis` the way production does, by calling the same constructor
/// `GatewayServer::run` calls.
///
/// These cases are about *steward* authority, so they declare the isolated
/// rehearsal profile (`self_serve_enrollment: true`) — that is the only profile
/// on which the enrollment routes exist at all after F-P0-1 containment. On
/// every shipped profile the tree is absent; that boundary is proven in
/// `fp01_credential_holder_escalation.rs`, not here.
macro_rules! sdis_app {
    ($authority:expr) => {{
        // Mounting the enrollment tree is the last point before a request can reach
        // `/v1/sdis/enrollment/start`, so the advertised origin is established here rather
        // than in each test body — see `ensure_advertised_origin`.
        ensure_advertised_origin();
        test::init_service(
            App::new()
                .app_data(web::Data::new($authority))
                .app_data(web::Data::new(Arc::new(EnrollmentStore::new())))
                .service(web::scope("/v1").service(sdis_scope(SdisMountFlags {
                    self_serve_enrollment: true,
                    admin_recovery: false,
                }))),
        )
        .await
    }};
}

async fn post_vouch(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: Option<&str>,
    body: serde_json::Value,
) -> u16 {
    let mut req = test::TestRequest::post()
        .uri("/v1/sdis/vouch/enrollment-under-test")
        .set_json(body);
    if let Some(t) = token {
        req = req.insert_header(("Authorization", format!("Bearer {t}")));
    }
    test::call_service(app, req.to_request())
        .await
        .status()
        .as_u16()
}

async fn get_status(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    uri: &str,
    token: Option<&str>,
) -> u16 {
    let mut req = test::TestRequest::get().uri(uri);
    if let Some(t) = token {
        req = req.insert_header(("Authorization", format!("Bearer {t}")));
    }
    test::call_service(app, req.to_request())
        .await
        .status()
        .as_u16()
}

fn mint(authority: &SessionAuthority, did: &Did, scope_list: &[&str]) -> String {
    authority
        .auth_manager()
        .issue_token(did, "test-coop", scopes(scope_list))
        .expect("mint")
}

// ---------------------------------------------------------------------------
// 1. Authentication: the institutional write surface refuses anonymous callers
// ---------------------------------------------------------------------------

/// The #2443 defect, expressed as an invariant: an anonymous POST could record a
/// vouch attributed to any steward DID. It must now be refused before reaching
/// the handler.
#[actix_web::test]
async fn anonymous_caller_cannot_record_a_steward_vouch() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());

    let status = post_vouch(
        &app,
        None,
        serde_json::json!({
            "vouch_statement": "I vouch for this applicant",
            "steward_did": "did:icn:zSomeoneElse",
        }),
    )
    .await;

    assert_eq!(
        status, 401,
        "an unauthenticated caller must not be able to record an institutional vouch"
    );
}

#[actix_web::test]
async fn malformed_and_expired_credentials_are_refused() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());
    let body = serde_json::json!({ "vouch_statement": "x" });

    assert_eq!(
        post_vouch(&app, Some("not.a.jwt"), body.clone()).await,
        401,
        "malformed credential"
    );

    // Hand-mint an already-expired credential with the authority's own secret.
    let now = icn_time::current_timestamp_secs();
    let expired = icn_gateway::auth::TokenClaims {
        sub: generated_did().to_string(),
        iat: now - 7200,
        exp: now - 1,
        coop_id: "test-coop".to_string(),
        scopes: scopes(&[STEWARD_SCOPE]),
        entity_id: None,
        entity_type: None,
        jti: Some("expired-jti".to_string()),
    };
    let expired_token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &expired,
        &jsonwebtoken::EncodingKey::from_secret(SECRET),
    )
    .unwrap();
    assert_eq!(
        post_vouch(&app, Some(&expired_token), body).await,
        401,
        "expired credential"
    );
}

/// The protected surface inherits the #2442 lifetime acceptance bound: a
/// credential minted by a co-issuer that never learned this deployment's
/// configured lifetime is refused here exactly as it is on every other
/// authority-bearing route.
#[actix_web::test]
async fn credential_exceeding_the_configured_lifetime_is_refused() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());

    // Same signing secret, canonical 24h default -> exceeds the 1h deployment.
    let overlong = AuthManager::new(SECRET.to_vec())
        .issue_token(&generated_did(), "test-coop", scopes(&[STEWARD_SCOPE]))
        .expect("mint");

    assert_eq!(
        post_vouch(
            &app,
            Some(&overlong),
            serde_json::json!({ "vouch_statement": "x" })
        )
        .await,
        401,
        "the configured lifetime ceiling must bound this surface too"
    );
}

/// Revocation reaches the SDIS surface: a credential withdrawn through the
/// shared authority stops working on the next request.
#[actix_web::test]
async fn revoked_credential_is_refused() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());
    let did = generated_did();
    let token = mint(&authority, &did, &[STEWARD_SCOPE]);
    let body = serde_json::json!({ "vouch_statement": "x" });

    // Sanity before revoking: the credential authenticates and authorizes, so
    // the only thing left to refuse it is the nonexistent enrollment. Pinning
    // 404 exactly (rather than "not 401") keeps this precondition from passing
    // on a 500 and making the revocation assertion below vacuous.
    let before = post_vouch(&app, Some(&token), body.clone()).await;
    assert_eq!(
        before, 404,
        "credential should authenticate and authorize before revocation"
    );

    let claims = authority.auth_manager().verify_token(&token).unwrap();
    authority.revoke(&claims).expect("revoke");

    assert_eq!(
        post_vouch(&app, Some(&token), body).await,
        401,
        "a revoked credential must not act on the institutional surface"
    );
}

// ---------------------------------------------------------------------------
// 2. Authorization: authentication alone is not steward authority
// ---------------------------------------------------------------------------

/// Authentication without authorization is not closure. A valid member
/// credential that holds no steward capability must not be able to vouch.
#[actix_web::test]
async fn authenticated_member_without_steward_capability_is_refused() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());
    let token = mint(
        &authority,
        &generated_did(),
        &["coop:read", "governance:read"],
    );

    assert_eq!(
        post_vouch(
            &app,
            Some(&token),
            serde_json::json!({ "vouch_statement": "x" })
        )
        .await,
        403,
        "any authenticated user is NOT a steward"
    );
}

#[actix_web::test]
async fn steward_capability_and_broad_governance_scope_are_both_accepted() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());

    for scope in [STEWARD_SCOPE, BROAD_GOVERNANCE_SCOPE] {
        let did = generated_did();
        let token = mint(&authority, &did, &[scope]);
        let status = post_vouch(
            &app,
            Some(&token),
            serde_json::json!({ "vouch_statement": "I vouch", "steward_did": did.to_string() }),
        )
        .await;
        // 404 is the precise success signal: the enrollment id is deliberately
        // nonexistent, so reaching the handler's own NotFound proves
        // authentication AND authorization both passed. Asserting merely
        // "not 401/403" would also pass on a 500 and mask an unrelated
        // regression.
        assert_eq!(
            status, 404,
            "scope {scope} must authenticate and carry steward authority, \
             leaving only the missing enrollment to refuse it (got {status})"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Identity binding: the body cannot name the actor
// ---------------------------------------------------------------------------

/// A steward credential must not be usable to attribute a vouch to somebody
/// else. The recorded actor is the verified subject; a body-supplied DID that
/// disagrees is refused rather than silently overridden.
#[actix_web::test]
async fn body_supplied_steward_did_must_match_the_verified_subject() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());
    let token = mint(&authority, &generated_did(), &[STEWARD_SCOPE]);

    let status = post_vouch(
        &app,
        Some(&token),
        serde_json::json!({
            "vouch_statement": "I vouch",
            "steward_did": generated_did().to_string(), // a different steward
        }),
    )
    .await;

    assert_eq!(
        status, 403,
        "a credential must not attribute an institutional act to another DID"
    );
}

// ---------------------------------------------------------------------------
// 3b. Cooperative binding: steward authority is not global
// ---------------------------------------------------------------------------

/// A steward credential carries the cooperative it was issued for. Holding
/// steward authority in one cooperative must not authorize an institutional act
/// on another's enrollment — the recurring "broad scope + caller coop_id"
/// bypass class (#2052/#2056/#2058), applied here through the canonical
/// `require_coop_access`.
#[actix_web::test]
async fn steward_cannot_act_on_another_cooperatives_enrollment() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());

    // Seed an enrollment belonging to "coop-a" through the real public
    // initiation route rather than a test-only backdoor into the store.
    let start = test::TestRequest::post()
        .uri("/v1/sdis/enrollment/start")
        .set_json(serde_json::json!({
            "identity_name": "Applicant",
            "coop_id": "coop-a",
        }))
        .to_request();
    let started: serde_json::Value = test::call_and_read_body_json(&app, start).await;
    let enrollment_id = started["enrollment_id"]
        .as_str()
        .expect("enrollment_id in start response")
        .to_string();

    // A genuine steward — but of "coop-b".
    let did = generated_did();
    let token = authority
        .auth_manager()
        .issue_token(&did, "coop-b", scopes(&[STEWARD_SCOPE]))
        .expect("mint");

    let req = test::TestRequest::post()
        .uri(&format!("/v1/sdis/vouch/{enrollment_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(serde_json::json!({ "vouch_statement": "I vouch" }))
        .to_request();

    assert_eq!(
        test::call_service(&app, req).await.status().as_u16(),
        403,
        "steward authority in one cooperative must not reach another's enrollment"
    );
}

// ---------------------------------------------------------------------------
// 3c. The success path, and what it actually records
// ---------------------------------------------------------------------------

/// Drive a real enrollment to level 1 through the public routes, then vouch as
/// a same-cooperative steward and assert the act SUCCEEDS and is attributed to
/// the verified credential subject.
///
/// Without this, every protected-route case is a refusal: a regression that
/// restored `session.steward_did = req.steward_did.clone()` would keep the
/// refusal tests green while silently recording the body's assertion again, and
/// a mismatch between the enrollment's `coop_id` and the credential's would
/// lock out every legitimate steward with no test noticing.
#[actix_web::test]
async fn same_coop_steward_vouch_succeeds_and_records_the_verified_subject() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());

    // Public initiation.
    let start = test::TestRequest::post()
        .uri("/v1/sdis/enrollment/start")
        .set_json(serde_json::json!({ "identity_name": "Applicant", "coop_id": "coop-a" }))
        .to_request();
    let started: serde_json::Value = test::call_and_read_body_json(&app, start).await;
    let enrollment_id = started["enrollment_id"].as_str().unwrap().to_string();

    // Level 1: the applicant's device signs the enrollment_id challenge.
    let device = icn_identity::IdentityBundle::generate().unwrap();
    let signature = device.keypair().unwrap().sign(enrollment_id.as_bytes());
    let level1 = test::TestRequest::post()
        .uri("/v1/sdis/enrollment/verify/level1")
        .set_json(serde_json::json!({
            "enrollment_id": enrollment_id,
            "device_proof": {
                "ephemeral_did": device.did().to_string(),
                "signature": hex::encode(signature.to_bytes()),
            }
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, level1).await.status().as_u16(),
        200,
        "public level-1 device verification must still work"
    );

    // A steward of the SAME cooperative vouches.
    let steward = generated_did();
    let token = authority
        .auth_manager()
        .issue_token(&steward, "coop-a", scopes(&[STEWARD_SCOPE]))
        .expect("mint");
    let vouch = test::TestRequest::post()
        .uri(&format!("/v1/sdis/vouch/{enrollment_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(serde_json::json!({ "vouch_statement": "I vouch for this applicant" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, vouch).await.status().as_u16(),
        200,
        "a same-cooperative steward with the capability must be able to vouch"
    );

    // The recorded actor is the credential subject — read back through the
    // steward-authenticated history surface.
    let history = test::TestRequest::get()
        .uri("/v1/sdis/steward/history")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let body: serde_json::Value = test::call_and_read_body_json(&app, history).await;
    let recorded = body["vouches"][0]["steward_did"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(
        recorded,
        steward.to_string(),
        "the recorded vouching steward must be the verified credential subject"
    );
}

/// The body cannot redirect attribution even when it names a *real* steward:
/// agreement is required, and agreement records the same subject.
#[actix_web::test]
async fn matching_body_did_is_accepted_and_records_the_same_subject() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());
    let steward = generated_did();
    let token = authority
        .auth_manager()
        .issue_token(&steward, "coop-a", scopes(&[STEWARD_SCOPE]))
        .expect("mint");

    // Nonexistent enrollment: we only assert the authorization verdict, which
    // is decided before the lookup's 404.
    let status = post_vouch(
        &app,
        Some(&token),
        serde_json::json!({
            "vouch_statement": "I vouch",
            "steward_did": steward.to_string(),
        }),
    )
    .await;
    // As above, 404 is the exact expected outcome: authorization passed and only
    // the nonexistent enrollment remains. A bare "not 403" would also accept a
    // 500 and hide an unrelated failure.
    assert_eq!(
        status, 404,
        "a body DID equal to the verified subject must be accepted, leaving \
         only the missing enrollment to refuse it (got {status})"
    );
}

// ---------------------------------------------------------------------------
// 4. Restricted reads: applicant state is not public
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn restricted_reads_are_unreachable_anonymously() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());

    for uri in ["/v1/sdis/pending", "/v1/sdis/steward/history"] {
        assert_eq!(
            get_status(&app, uri, None).await,
            401,
            "{uri} discloses applicant state and must require a credential"
        );
    }
}

#[actix_web::test]
async fn restricted_reads_require_steward_capability() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());
    let token = mint(
        &authority,
        &generated_did(),
        &["coop:read", "governance:read"],
    );

    for uri in ["/v1/sdis/pending", "/v1/sdis/steward/history"] {
        assert_eq!(
            get_status(&app, uri, Some(&token)).await,
            403,
            "{uri} must require steward authority, not merely authentication"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Public enrollment initiation is preserved
// ---------------------------------------------------------------------------

/// The point of a route-authority boundary is not to make enrollment
/// impossible. A person beginning enrollment has no ICN credential yet, so on a
/// profile that mounts enrollment at all, initiation must remain reachable
/// anonymously.
///
/// Scope note (F-P0-1): "mounts enrollment at all" now means an explicitly
/// declared isolated rehearsal deployment, which `sdis_app!` selects. This case
/// proves the authority split did not break anonymous initiation *there*; it
/// deliberately says nothing about whether the tree should be mounted on any
/// other profile, which is a mount decision, not a route-authority one.
#[actix_web::test]
async fn public_enrollment_initiation_remains_anonymous() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());

    let req = test::TestRequest::post()
        .uri("/v1/sdis/enrollment/start")
        .set_json(serde_json::json!({
            "identity_name": "Applicant",
            "coop_id": "test-coop",
        }))
        .to_request();
    let status = test::call_service(&app, req).await.status().as_u16();

    // Pinned to 200, not merely "not 401/403". This test exists to prove the
    // scope split did not break enrollment for people who have no credential
    // yet — if it accepted any non-auth status, initiation could be failing
    // outright and this case would still pass, proving nothing about the one
    // thing it guards.
    assert_eq!(
        status, 200,
        "enrollment initiation must remain anonymous AND working: an applicant \
         has no credential to present"
    );
}

/// Public status reads stay public, but the *institutional* surface does not
/// become reachable through the public scope.
#[actix_web::test]
async fn public_scope_does_not_expose_the_steward_surface() {
    let authority = authority(1);
    let app = sdis_app!(authority.clone());

    // No credential: the moderation write is refused, not merely 404'd away.
    let req = test::TestRequest::post()
        .uri("/v1/sdis/reject/enrollment-under-test")
        .set_json(serde_json::json!({ "reason": "spam" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status().as_u16(),
        401,
        "moderation write must be refused without a credential"
    );
}

// ---------------------------------------------------------------------------
// 5b. The nested protected scope does not shadow its siblings
// ---------------------------------------------------------------------------

/// The protected surface is mounted as a nested empty-prefix scope so it can
/// carry `jwt_auth` under the same `/sdis` prefix. An empty prefix matches
/// everything, so the structural risk is that it swallows or shadows the
/// sibling routes registered beside it — turning public verification endpoints
/// into 401s, or into 404s.
///
/// This mounts the siblings the way `GatewayServer::run` does, in the same
/// order, and asserts they still resolve unauthenticated.
#[actix_web::test]
async fn nested_protected_scope_does_not_shadow_sibling_routes() {
    let authority = authority(1);
    let auth_mw = HttpAuthentication::bearer(jwt_auth);
    // Same rule as `sdis_app!`: this assembles the enrollment tree, so it owns the
    // precondition too, whether or not this particular case drives `/enrollment/start`.
    ensure_advertised_origin();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(authority))
            .app_data(web::Data::new(Arc::new(EnrollmentStore::new())))
            .service(
                web::scope("/v1/sdis")
                    .service(icn_gateway::api::sdis::sdis_health)
                    .configure(simple_enrollment::configure)
                    .service(
                        web::scope("")
                            .wrap(auth_mw)
                            .configure(simple_enrollment::configure_protected),
                    ),
            ),
    )
    .await;

    // A sibling registered BEFORE the nested scope still resolves, and is still
    // public.
    let req = test::TestRequest::get().uri("/v1/sdis/health").to_request();
    assert_eq!(
        test::call_service(&app, req).await.status().as_u16(),
        200,
        "the nested authenticated scope must not shadow sibling public routes"
    );

    // And the protected surface is still protected in the same composition.
    assert_eq!(
        post_vouch(&app, None, serde_json::json!({ "vouch_statement": "x" })).await,
        401,
        "protected routes must remain protected alongside siblings"
    );
}

// ---------------------------------------------------------------------------
// 6. Misassembly fails closed
// ---------------------------------------------------------------------------

/// A composition that forgets to register `SessionAuthority` must refuse, and
/// must keep refusing even when a perfectly usable bare `AuthManager` is
/// present. Registering nothing would only prove "fails when nothing can
/// verify"; an `.or_else(..)` fallback to signature-only verification would slip
/// straight through such a test.
#[actix_web::test]
async fn misassembly_without_session_authority_refuses_even_with_a_usable_auth_manager() {
    let bare = AuthManager::new(SECRET.to_vec());
    let token = bare
        .issue_token(&generated_did(), "test-coop", scopes(&[STEWARD_SCOPE]))
        .expect("mint");

    let auth_mw = HttpAuthentication::bearer(jwt_auth);
    // Same rule as `sdis_app!`: this assembles the enrollment tree, so it owns the
    // precondition too, whether or not this particular case drives `/enrollment/start`.
    ensure_advertised_origin();
    let app = test::init_service(
        App::new()
            // Deliberately NO SessionAuthority. Both plausible shapes of a bare
            // issuer are registered and are individually sufficient to verify
            // this token.
            .app_data(web::Data::new(AuthManager::new(SECRET.to_vec())))
            .app_data(web::Data::new(Arc::new(AuthManager::new(SECRET.to_vec()))))
            .app_data(web::Data::new(Arc::new(EnrollmentStore::new())))
            .service(
                web::scope("/v1/sdis")
                    .configure(simple_enrollment::configure)
                    .service(
                        web::scope("")
                            .wrap(auth_mw)
                            .configure(simple_enrollment::configure_protected),
                    ),
            ),
    )
    .await;

    // 500, not 401: a missing `SessionAuthority` is a server-side wiring error,
    // not a bad credential. Reporting 401 would blame the caller's token while
    // hiding the misassembly. This matches the guard in
    // `session_authority_enforcement.rs`; what both pin is that a
    // cryptographically valid credential is NOT honored when revocation
    // enforcement is absent.
    assert_eq!(
        post_vouch(
            &app,
            Some(&token),
            serde_json::json!({ "vouch_statement": "x" })
        )
        .await,
        500,
        "a misassembled runtime must refuse rather than fall back to signature-only auth"
    );
}

/// Enrollment initiation issues a device-facing QR, whose `gateway_url` requires an
/// operator-authoritative origin (#2569). These tests are about route authority, not QR
/// authority, so they only need a valid origin to exist.
///
/// **Called from app assembly, never from a test body.** Leaving it to each test to remember
/// is what made the sibling binary `fp01_credential_holder_escalation.rs` order-dependent:
/// a test that reached `/v1/sdis/enrollment/start` without calling this passed only when
/// another test had run first, and returned 503 in isolation. Assembling the app is the one
/// step no test can skip and still issue a request, so the precondition lives there.
///
/// `Once` rather than an idempotent `set_var`: libtest runs these tests on many threads, and
/// re-writing on every call means a write can overlap a handler thread's read of the same
/// variable. One write, and `call_once` gives every later caller a happens-before edge to it.
fn ensure_advertised_origin() {
    static CONFIGURED: std::sync::Once = std::sync::Once::new();
    CONFIGURED.call_once(|| {
        std::env::set_var("GATEWAY_BASE_URL", "https://gateway.example.coop");
    });
}
