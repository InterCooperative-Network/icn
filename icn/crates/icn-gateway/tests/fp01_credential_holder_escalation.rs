//! Regression proof for F-P0-1 — credential-holder self-elevation to
//! arbitrary-cooperative membership and `ledger:write`.
//!
//! # The chain these tests close
//!
//! A holder of **one** valid gateway credential — any scope, any cooperative —
//! could:
//!
//! 1. `POST /v1/trust/attest` with `{to: <self>, score: 1.0}`. The route was
//!    authenticated but carried no capability check, and the trust-edge mutation
//!    had no self-edge guard, so the caller wrote an edge asserting that it
//!    vouched for itself.
//! 2. That self-edge is an *incoming* edge to the caller, so the level-2 vouch
//!    gate (`effective_trust >= 0.4`, averaged over incoming edges) was
//!    satisfied permanently and for free.
//! 3. `POST /v1/sdis/enrollment/verify/level2` accepted any valid credential as
//!    a steward, with no capability and no binding to the cooperative the
//!    enrollment named.
//! 4. `POST /v1/sdis/enrollment/complete` was mounted on the bare `/sdis` scope
//!    outside the auth wrapper and minted a credential carrying
//!    `ledger:read`+`ledger:write` scoped to the caller-supplied `coop_id` —
//!    even when every institutional write behind it had failed.
//! 5. `require_coop_access` then compared that `coop_id` by string equality and
//!    let the credential through.
//!
//! # Why these tests build the real scope
//!
//! The route tree is assembled inside a closure in `GatewayServer::run`, and the
//! campaign found that no gateway test reaches it (#2421) — a parallel test
//! router can agree with itself while diverging from what serves traffic, which
//! is how a mounted-outside-auth route survived review. These cases therefore
//! call [`icn_gateway::server::sdis_scope`], the same constructor the running
//! gateway calls, with the same mount flags, rather than rebuilding an
//! approximation of `/sdis`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{test, web, App};
use ed25519_dalek::{Signer, SigningKey};
use icn_gateway::api::sdis::simple_enrollment::{EnrollmentSession, EnrollmentStore};
use icn_gateway::auth::AuthManager;
use icn_gateway::commons_mgr::CommonsManager;
use icn_gateway::server::{sdis_scope, SdisMountFlags};
use icn_gateway::session_authority::{
    AuthorityProfile, InMemoryRevocationAuthority, SessionAuthority, TokenLifetimePolicy,
};
use icn_gateway::steward_mgr::StewardManager;
use icn_gateway::trust_mgr::{TrustManager, TrustMutationError};
use icn_identity::{Did, IdentityBundle};

const SECRET: &[u8] = b"fp01-containment-regression-secret-32b";

/// The trust-attestation route as the assembled router actually serves it.
///
/// Note the doubled segment. The handler macro is `#[post("/trust/attest")]`
/// *and* `GatewayServer::run` mounts it inside `web::scope("/trust")`, so the
/// live path is `/v1/trust/trust/attest` — not the `/v1/trust/attest` that the
/// docs and the architecture campaign both name. The same doubling applies to
/// every route in that scope. That is a separate (cosmetic) defect; this
/// containment deliberately does not renumber the API, and pins the real path
/// here so the capability check is proven against the route that actually
/// serves traffic rather than against a 404.
const ATTEST_URI: &str = "/v1/trust/trust/attest";

/// The cooperative an honest credential in these tests belongs to.
const HOME_COOP: &str = "home-coop";
/// A cooperative the caller has no relationship with — the escalation target.
const VICTIM_COOP: &str = "victim-coop";

fn authority() -> Arc<SessionAuthority> {
    let lifetime = TokenLifetimePolicy::from_hours(1).unwrap();
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

fn scopes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn mint(authority: &SessionAuthority, did: &Did, coop: &str, scope_list: &[&str]) -> String {
    authority
        .auth_manager()
        .issue_token(did, coop, scopes(scope_list))
        .expect("mint")
}

/// Assemble `/v1/sdis` exactly as `GatewayServer::run` does, with the given
/// mount flags and the app data the enrollment handlers resolve.
macro_rules! sdis_app {
    ($authority:expr, $trust:expr, $flags:expr) => {{
        // Mounting the enrollment tree is the last point before a request can reach
        // `/v1/sdis/enrollment/start`, so the advertised origin is established here rather
        // than in each test body — see `ensure_advertised_origin`.
        ensure_advertised_origin();
        test::init_service(
            App::new()
                .app_data(web::Data::new($authority.clone()))
                .app_data(web::Data::new($trust.clone()))
                .app_data(web::Data::new(Arc::new(EnrollmentStore::new())))
                .app_data(web::Data::new(Arc::new(CommonsManager::new())))
                .app_data(web::Data::new(None::<Arc<StewardManager>>))
                .service(web::scope("/v1").service(sdis_scope($flags))),
        )
        .await
    }};
}

// ---------------------------------------------------------------------------
// 1. The trust-attestation route: capability check, and the dead extractor.
// ---------------------------------------------------------------------------

/// `POST /v1/trust/trust/attest` is UNREACHABLE on current `main`, and that
/// disproves link 2 of the reported chain.
///
/// `create_trust_attestation` takes `from_did: web::ReqData<Did>`. `ReqData<T>`
/// resolves `T` out of the request extensions — and **nothing in the gateway
/// ever inserts a bare `Did`**. `jwt_auth` inserts `icn_http_kit::auth::BasicClaims`
/// and the gateway's `TokenClaims` (`middleware.rs`); the trust rate limiter only
/// *reads* `TokenClaims`. So the extractor fails and the request 500s before the
/// handler body — including before the `require_scope` this containment added.
///
/// Consequences, recorded here because they are easy to lose:
///
/// * The self-edge write that the architecture campaign described as link 2 of
///   F-P0-1 **cannot happen over this route**. The campaign traced the handler
///   body without checking whether its extractor could resolve.
/// * The same defect kills `get_trust_score`, `revoke_trust_attestation`
///   (`api/trust.rs`) and seven handlers in `api/notifications.rs`.
/// * The chain is still reachable by another route — see
///   `enrollment_vouch_edge_clears_the_threshold_without_any_attestation` below.
///
/// This case pins the current behaviour deliberately. Repairing the extractor is
/// a surface-*expanding* change (it turns a dead write path into a live one) and
/// is intentionally not done in a containment tranche. Whoever repairs it will
/// see this test fail, and must keep the `require_scope` that is already in the
/// handler.
#[actix_web::test]
async fn trust_attest_route_is_unreachable_because_its_extractor_is_never_populated() {
    use actix_web_httpauth::middleware::HttpAuthentication;

    let authority = authority();
    let trust = Arc::new(TrustManager::new());
    let caller = IdentityBundle::generate().unwrap().did().clone();
    let target = IdentityBundle::generate().unwrap().did().clone();

    // A credential carrying the capability the route requires. Even this fails.
    let token = mint(&authority, &caller, HOME_COOP, &["trust:write"]);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(authority.clone()))
            .app_data(web::Data::new(trust.clone()))
            .app_data(web::Data::new(Arc::new(
                icn_gateway::events::EventBroadcaster::new(),
            )))
            .service(
                web::scope("/v1").service(
                    web::scope("/trust")
                        .service(icn_gateway::api::trust::create_trust_attestation)
                        .wrap(HttpAuthentication::bearer(
                            icn_gateway::middleware::jwt_auth,
                        )),
                ),
            ),
    )
    .await;

    let status = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(ATTEST_URI)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(serde_json::json!({ "to": target.to_string(), "score": 0.6 }))
            .to_request(),
    )
    .await
    .status()
    .as_u16();

    assert_eq!(
        status, 500,
        "the route is expected to be dead: ReqData<Did> is never populated. \
         If this now returns 200/403, the extractor was repaired — keep the \
         require_scope(TRUST_WRITE) check in create_trust_attestation and update \
         this test to assert 403-without-capability / 200-with."
    );

    // The decisive part: no edge was written, so no self-elevation is possible here.
    assert!(
        trust.get_incoming_edges_async(&target).await.is_empty(),
        "a 500'd attestation must not have written an edge"
    );
}

/// The reachable replacement for the disproved link 2.
///
/// `complete_enrollment` writes a steward→member trust edge at **0.5**, labelled
/// `enrollment-vouch`. The level-2 gate admits anyone whose average *incoming*
/// trust is >= 0.4. So a member who completed enrollment can immediately vouch
/// for further enrollments — no `trust/attest` call, and no self-edge, required.
///
/// That is why the cooperative binding on `verify_level2` is the load-bearing
/// control in this containment rather than the capability on `trust/attest`:
/// the trust threshold is satisfiable through an entirely legitimate path.
#[tokio::test]
async fn enrollment_vouch_edge_clears_the_threshold_without_any_attestation() {
    let trust = TrustManager::new();
    let steward = IdentityBundle::generate().unwrap().did().clone();
    let member = IdentityBundle::generate().unwrap().did().clone();

    // Exactly what complete_enrollment writes.
    trust
        .add_edge_async(
            icn_trust::TrustEdge::new(
                steward,
                member.clone(),
                icn_trust::TrustScore::unchecked(0.5),
            )
            .with_label("enrollment-vouch"),
        )
        .await
        .expect("the enrollment vouch edge is a normal, non-self edge");

    let incoming = trust.get_incoming_edges_async(&member).await;
    let avg = incoming.iter().map(|e| e.score.value()).sum::<f64>() / incoming.len() as f64;

    assert!(
        avg >= 0.4,
        "an enrolled member already clears the vouching threshold ({avg}) — the chain \
         does not depend on the dead trust/attest route"
    );
}

// ---------------------------------------------------------------------------
// 2. Self-attestation is rejected at the domain layer, not just over HTTP.
// ---------------------------------------------------------------------------

/// The guard must live below the transport so another caller cannot recreate the
/// bypass. This calls the manager directly, with no HTTP involved.
#[tokio::test]
async fn self_edge_rejected_at_the_mutation_layer() {
    let trust = TrustManager::new();
    let did = IdentityBundle::generate().unwrap().did().clone();

    let err = trust
        .add_edge_with_score(did.clone(), did.clone(), 1.0, None)
        .await
        .expect_err("self-attestation must be refused by the mutation layer itself");

    assert!(
        matches!(err, TrustMutationError::SelfAttestation { .. }),
        "the refusal must be a stable typed error, got {err:?}"
    );

    assert!(
        trust.get_incoming_edges_async(&did).await.is_empty(),
        "a rejected self-edge must not be silently stored"
    );
}

/// A self-edge is what made the level-2 gate unfireable. With the edge refused,
/// the caller's incoming-trust average stays at zero — below the 0.4 threshold.
#[tokio::test]
async fn refused_self_edge_leaves_the_vouch_threshold_unmet() {
    let trust = TrustManager::new();
    let did = IdentityBundle::generate().unwrap().did().clone();

    let _ = trust
        .add_edge_with_score(did.clone(), did.clone(), 1.0, None)
        .await;

    let incoming = trust.get_incoming_edges_async(&did).await;
    let avg = if incoming.is_empty() {
        0.0
    } else {
        incoming.iter().map(|e| e.score.value()).sum::<f64>() / incoming.len() as f64
    };
    assert!(
        avg < 0.4,
        "self-attestation must not lift the caller to the 0.4 vouching threshold (got {avg})"
    );
}

// ---------------------------------------------------------------------------
// 3 & 5. The enrollment route tree is absent on every non-rehearsal profile.
// ---------------------------------------------------------------------------

/// With self-serve enrollment not declared — the default, and what every shipped
/// deployment profile produces — the whole enrollment tree is absent. A caller
/// cannot enroll into an arbitrary cooperative because there is no route to.
#[actix_web::test]
async fn enrollment_tree_absent_without_explicit_declaration() {
    let authority = authority();
    let trust = Arc::new(TrustManager::new());
    let app = sdis_app!(authority, trust, SdisMountFlags::default());

    for (method, uri) in [
        ("POST", "/v1/sdis/enrollment/start"),
        ("POST", "/v1/sdis/enrollment/verify/level1"),
        ("POST", "/v1/sdis/enrollment/verify/level2"),
        ("POST", "/v1/sdis/enrollment/complete"),
    ] {
        let req = match method {
            "POST" => test::TestRequest::post()
                .uri(uri)
                .set_json(serde_json::json!({})),
            _ => test::TestRequest::get().uri(uri),
        };
        let status = test::call_service(&app, req.to_request())
            .await
            .status()
            .as_u16();
        // 404 (no such route) or 401 (the request fell through to the nested
        // authenticated `/sdis` scope, which matches any remaining path and
        // rejects an anonymous caller before routing). Either proves the same
        // thing: no enrollment handler ran. What must never appear is 2xx.
        assert!(
            status == 404 || status == 401,
            "{uri} must not be served when self-serve enrollment is undeclared (got {status})"
        );
    }
}

/// The containment must not take the steward surface with it: those routes are
/// mounted unconditionally and stay behind `jwt_auth`, answering 401 to an
/// anonymous caller rather than 404.
#[actix_web::test]
async fn steward_surface_survives_the_containment() {
    let authority = authority();
    let trust = Arc::new(TrustManager::new());
    let app = sdis_app!(authority, trust, SdisMountFlags::default());

    let req = test::TestRequest::post()
        .uri("/v1/sdis/vouch/some-enrollment")
        .set_json(serde_json::json!({ "vouch_statement": "x" }))
        .to_request();

    let status = test::call_service(&app, req).await.status().as_u16();
    assert_eq!(
        status, 401,
        "the protected steward surface must remain mounted and authenticated, not removed"
    );
}

/// The declared isolated-rehearsal profile still serves the surface — the gate
/// is a mount decision, not a removal of the feature.
#[actix_web::test]
async fn enrollment_tree_present_under_explicit_declaration() {
    let authority = authority();
    let trust = Arc::new(TrustManager::new());
    let app = sdis_app!(
        authority,
        trust,
        SdisMountFlags {
            self_serve_enrollment: true,
            admin_recovery: false,
        }
    );

    let req = test::TestRequest::post()
        .uri("/v1/sdis/enrollment/start")
        .set_json(serde_json::json!({
            "identity_name": "Ada",
            "coop_id": HOME_COOP,
        }))
        .to_request();

    let status = test::call_service(&app, req).await.status().as_u16();
    assert_eq!(
        status, 200,
        "the declared rehearsal profile must still serve enrollment initiation"
    );
}

// ---------------------------------------------------------------------------
// 3 (cont). Even where mounted, the vouch is bound to the cooperative.
// ---------------------------------------------------------------------------

/// Drive the real chain on the rehearsal profile: start an enrollment naming a
/// cooperative the caller has nothing to do with, then try to vouch for it with
/// a credential issued for a different cooperative.
///
/// This is the step that stops a caller-supplied `coop_id` from becoming
/// enforceable authority: the trust threshold says nothing about *which*
/// institution the voucher belongs to.
#[actix_web::test]
async fn vouch_denied_for_a_foreign_cooperative() {
    let authority = authority();
    let trust = Arc::new(TrustManager::new());
    let app = sdis_app!(
        authority,
        trust,
        SdisMountFlags {
            self_serve_enrollment: true,
            admin_recovery: false,
        }
    );

    // Start an enrollment into the victim cooperative (unauthenticated, as designed).
    let start = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/start")
            .set_json(serde_json::json!({
                "identity_name": "Mallory",
                "coop_id": VICTIM_COOP,
            }))
            .to_request(),
    )
    .await;
    assert_eq!(start.status().as_u16(), 200);
    let start: serde_json::Value = test::read_body_json(start).await;
    let enrollment_id = start["enrollment_id"].as_str().unwrap().to_string();

    // Level 1: device proof over the enrollment id.
    let signing = SigningKey::from_bytes(&[7u8; 32]);
    let ephemeral = Did::from_public_key(&signing.verifying_key());
    let sig = signing.sign(enrollment_id.as_bytes());
    let l1 = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/verify/level1")
            .set_json(serde_json::json!({
                "enrollment_id": enrollment_id,
                "device_proof": {
                    "ephemeral_did": ephemeral.to_string(),
                    "signature": hex::encode(sig.to_bytes()),
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(l1.status().as_u16(), 200, "level 1 should succeed");

    // Level 2 with a credential for HOME_COOP, and enough trust to clear the
    // threshold by honest means (an edge from someone else).
    let attacker = IdentityBundle::generate().unwrap().did().clone();
    let voucher = IdentityBundle::generate().unwrap().did().clone();
    trust
        .add_edge_with_score(voucher, attacker.clone(), 1.0, None)
        .await
        .expect("honest edge");
    let token = mint(
        &authority,
        &attacker,
        HOME_COOP,
        &["governance:steward:write"],
    );

    let l2 = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/verify/level2")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(serde_json::json!({
                "enrollment_id": enrollment_id,
                "vouch_statement": "I vouch",
            }))
            .to_request(),
    )
    .await;

    assert_eq!(
        l2.status().as_u16(),
        403,
        "a credential for '{HOME_COOP}' must not advance an enrollment into '{VICTIM_COOP}', \
         even with sufficient trust"
    );
}

/// Completion cannot be reached while the enrollment is stuck at level 1, so no
/// credential is minted for the foreign cooperative.
#[actix_web::test]
async fn completion_denied_while_vouch_is_unbound() {
    let authority = authority();
    let trust = Arc::new(TrustManager::new());
    let app = sdis_app!(
        authority,
        trust,
        SdisMountFlags {
            self_serve_enrollment: true,
            admin_recovery: false,
        }
    );

    let start = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/start")
            .set_json(serde_json::json!({
                "identity_name": "Mallory",
                "coop_id": VICTIM_COOP,
            }))
            .to_request(),
    )
    .await;
    let start: serde_json::Value = test::read_body_json(start).await;
    let enrollment_id = start["enrollment_id"].as_str().unwrap().to_string();

    let signing = SigningKey::from_bytes(&[9u8; 32]);
    let ephemeral = Did::from_public_key(&signing.verifying_key());
    let sig = signing.sign(format!("complete:{enrollment_id}").as_bytes());

    let done = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/complete")
            .set_json(serde_json::json!({
                "enrollment_id": enrollment_id,
                "ephemeral_did": ephemeral.to_string(),
                "ephemeral_signature": hex::encode(sig.to_bytes()),
                "device_info": { "device_type": "t", "os": "o", "app_version": "v" },
            }))
            .to_request(),
    )
    .await;

    let status = done.status().as_u16();
    assert_ne!(
        status, 200,
        "completion must not mint a credential for an enrollment that never cleared a bound vouch"
    );

    let body: serde_json::Value = test::read_body_json(done).await;
    assert!(
        body.get("auth_token").is_none(),
        "no credential may be returned: {body}"
    );
}

// ---------------------------------------------------------------------------
// 4 & 7. Fail-closed mint, and the legitimate flow still works.
// ---------------------------------------------------------------------------

/// Drive one full, legitimate enrollment on the declared rehearsal profile.
///
/// `seed` distinguishes the device keypair; `coop` is both the enrollment's
/// cooperative and the voucher's, so the vouch is properly bound. Returns the
/// completion response status and body.
async fn run_enrollment(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    authority: &SessionAuthority,
    trust: &TrustManager,
    coop: &str,
    seed: u8,
) -> (u16, serde_json::Value) {
    let start = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/start")
            .set_json(serde_json::json!({ "identity_name": "Ada", "coop_id": coop }))
            .to_request(),
    )
    .await;
    assert_eq!(start.status().as_u16(), 200, "enrollment start");
    let start: serde_json::Value = test::read_body_json(start).await;
    let enrollment_id = start["enrollment_id"].as_str().unwrap().to_string();

    let signing = SigningKey::from_bytes(&[seed; 32]);
    let ephemeral = Did::from_public_key(&signing.verifying_key());

    let l1 = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/verify/level1")
            .set_json(serde_json::json!({
                "enrollment_id": enrollment_id,
                "device_proof": {
                    "ephemeral_did": ephemeral.to_string(),
                    "signature": hex::encode(signing.sign(enrollment_id.as_bytes()).to_bytes()),
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(l1.status().as_u16(), 200, "level 1");

    // A steward who belongs to the same cooperative and has honest incoming trust.
    let steward = IdentityBundle::generate().unwrap().did().clone();
    let voucher = IdentityBundle::generate().unwrap().did().clone();
    trust
        .add_edge_with_score(voucher, steward.clone(), 1.0, None)
        .await
        .expect("honest trust edge");
    let token = mint(authority, &steward, coop, &["governance:steward:write"]);

    let l2 = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/verify/level2")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(serde_json::json!({
                "enrollment_id": enrollment_id,
                "vouch_statement": "I vouch for Ada",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(
        l2.status().as_u16(),
        200,
        "a same-cooperative steward with real trust must still be able to vouch"
    );

    let done = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/complete")
            .set_json(serde_json::json!({
                "enrollment_id": enrollment_id,
                "ephemeral_did": ephemeral.to_string(),
                "ephemeral_signature":
                    hex::encode(
                        signing.sign(format!("complete:{enrollment_id}").as_bytes()).to_bytes()
                    ),
                "device_info": { "device_type": "t", "os": "o", "app_version": "v" },
            }))
            .to_request(),
    )
    .await;

    let status = done.status().as_u16();
    let body: serde_json::Value = test::read_body_json(done).await;
    (status, body)
}

/// The legitimate flow still completes end to end and still mints a credential —
/// the containment denies unauthorized callers, it does not disable enrollment
/// on the profile that declares it.
#[actix_web::test]
async fn legitimate_enrollment_still_completes() {
    let authority = authority();
    let trust = Arc::new(TrustManager::new());
    let app = sdis_app!(
        authority,
        trust,
        SdisMountFlags {
            self_serve_enrollment: true,
            admin_recovery: false,
        }
    );

    let (status, body) = run_enrollment(&app, &authority, &trust, HOME_COOP, 21).await;

    assert_eq!(status, 200, "legitimate enrollment must complete: {body}");
    assert!(
        body["auth_token"].as_str().is_some(),
        "a legitimate completion mints a credential: {body}"
    );
    assert_eq!(
        body["membership_status"].as_str(),
        Some("provisional"),
        "membership must be durably recorded, not reported as absent: {body}"
    );
    assert!(
        body["anchor_id"].as_str().is_some() && body["holder_id"].as_str().is_some(),
        "anchor and holder must both exist: {body}"
    );
}

/// A completion whose required institutional write fails must not mint.
///
/// # How this reproduces the original defect deterministically
///
/// A level-2 session records the vouching steward's DID as a string. If that
/// string is not a parseable DID, `complete_enrollment` derives
/// `steward_did_opt = None`, and the anchor is then created on the `Genesis`
/// pathway rather than `WebOfTrust` — which attests `POPLevel::Weak` instead of
/// `Strong`. `join_jurisdiction` refuses a holder below `Strong`, so the
/// cooperative membership write **fails**.
///
/// Before the containment that failure was a `warn!` and execution continued to
/// the mint: the caller received a signed credential carrying
/// `ledger:read`+`ledger:write` for the cooperative, alongside
/// `membership_status: null` — authority asserted over institutional state that
/// was never written, which `require_coop_access` cannot distinguish from the
/// real thing because it only compares the `coop_id` string.
///
/// The session is injected through the store's public `insert` rather than
/// driven through `verify_level2`, because `verify_level2` can only ever record
/// a well-formed subject DID. The point under test is the completion handler's
/// behaviour when a required write fails, not how the session got that way.
#[actix_web::test]
async fn no_credential_is_minted_when_a_required_write_fails() {
    let authority = authority();
    let trust = Arc::new(TrustManager::new());
    let store = Arc::new(EnrollmentStore::new());

    // Same rule as `sdis_app!`: this assembles the enrollment tree, so it owns the
    // precondition too. This case only drives `/enrollment/complete`, but the mounted
    // tree includes `/enrollment/start` and the next edit here should not have to notice.
    ensure_advertised_origin();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(authority.clone()))
            .app_data(web::Data::new(trust.clone()))
            .app_data(web::Data::new(store.clone()))
            .app_data(web::Data::new(Arc::new(CommonsManager::new())))
            .app_data(web::Data::new(None::<Arc<StewardManager>>))
            .service(web::scope("/v1").service(sdis_scope(SdisMountFlags {
                self_serve_enrollment: true,
                admin_recovery: false,
            }))),
    )
    .await;

    let signing = SigningKey::from_bytes(&[57u8; 32]);
    let ephemeral = Did::from_public_key(&signing.verifying_key());
    let enrollment_id = "fp01-required-write-fails".to_string();
    let now = icn_time::current_timestamp_secs();

    store
        .insert(
            enrollment_id.clone(),
            EnrollmentSession {
                enrollment_id: enrollment_id.clone(),
                identity_name: "Ada".to_string(),
                coop_id: VICTIM_COOP.to_string(),
                verification_code: "VERIFY-0000".to_string(),
                level: 2,
                created_at: now,
                expires_at: now + 86_400,
                ephemeral_did: Some(ephemeral.to_string()),
                steward_vouch: Some("I vouch".to_string()),
                // Not a parseable DID ⇒ the anchor is created without a voucher
                // ⇒ Weak POP ⇒ the jurisdiction join fails.
                steward_did: Some("not-a-parseable-did".to_string()),
                vouched_at: Some(now),
                rejected: false,
                rejection_reason: None,
                rejected_at: None,
                rejected_by: None,
            },
        )
        .await;

    let done = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/sdis/enrollment/complete")
            .set_json(serde_json::json!({
                "enrollment_id": enrollment_id,
                "ephemeral_did": ephemeral.to_string(),
                "ephemeral_signature": hex::encode(
                    signing.sign(format!("complete:{enrollment_id}").as_bytes()).to_bytes()
                ),
                "device_info": { "device_type": "t", "os": "o", "app_version": "v" },
            }))
            .to_request(),
    )
    .await;

    let status = done.status().as_u16();
    let body: serde_json::Value = test::read_body_json(done).await;

    assert_ne!(
        status, 200,
        "a completion whose institutional writes failed must not succeed: {body}"
    );
    assert!(
        body.get("auth_token").is_none(),
        "no credential may be minted when a required institutional write failed: {body}"
    );
    assert!(
        body.get("membership_status")
            .and_then(|v| v.as_str())
            .is_none(),
        "no membership may be reported: {body}"
    );
}

/// Enrollment initiation issues a device-facing QR, whose `gateway_url` requires an
/// operator-authoritative origin (#2569). These tests are about containment and route
/// authority, not QR authority, so they only need a valid origin to exist.
///
/// **Called from app assembly, never from a test body.** An earlier revision left this to
/// each test to remember, and `legitimate_enrollment_still_completes` did not: it drives
/// `/v1/sdis/enrollment/start` through `run_enrollment` and passed only when some sibling
/// test had already set the variable. Run alone it returned 503 against an expected 200 — a
/// suite that was green while that case proved nothing. Assembling the app is the one step
/// no test can skip and still issue a request, so the precondition lives there.
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
