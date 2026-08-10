#![allow(clippy::unwrap_used, clippy::expect_used)]
//! QR gateway authority must be operator-owned, not request-derived (#2569).
//!
//! The invariant under test:
//!
//! > A URL embedded in QR material that causes another device to transmit an ICN bearer
//! > credential MUST have operator-authoritative scheme/host/port. Its authority MUST NOT be
//! > derived from request-controlled `Host`, `Forwarded`, `X-Forwarded-Host`,
//! > `X-Forwarded-Proto`, or request target authority — not even when the immediate peer is a
//! > trusted proxy.
//!
//! Why this is a credential-destination property and not a cosmetic one: the QR payload's
//! `gateway_url` is consumed by a *second device*. The reference scanner
//! (`examples/mobile-app/src/screens/QRScannerScreen.tsx`) POSTs to
//! `${qrData.gateway_url}/v1/sessions/{id}/approve` with `Authorization: Bearer <token>`.
//! Whoever controls that origin receives a member's bearer credential.
//!
//! #2567/#2568 established that a trusted immediate peer may assert the *client IP*. That does
//! not make every value such a peer relays an operator assertion: the appliance nginx is
//! `default_server` and relays `proxy_set_header Host $host`, so a caller-chosen `Host` reaches
//! the gateway over loopback wearing the proxy's trust. Authenticating the proxy does not
//! authenticate the provenance of what the proxy relays.
//!
//! These tests enter through the real QR-producing routes rather than the derivation helper,
//! because the property at stake is what lands in QR material, not what a formatter returns.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use actix_http::Request;
use actix_web::http::StatusCode;
use actix_web::{test, web, App};
use icn_gateway::api;
use icn_gateway::api::sdis::simple_enrollment::EnrollmentStore;
use icn_gateway::rate_limit::IpRateLimiter;
use icn_gateway::session::SessionManager;

/// Loopback is a trusted proxy under the default policy — this is exactly the appliance's
/// on-host nginx position, and the trust #2568 relies on.
const TRUSTED_PEER: &str = "127.0.0.1:54321";

/// A routable, non-loopback socket peer: NOT a trusted proxy under the default policy.
const UNTRUSTED_PEER: &str = "198.51.100.42:54321";

/// The origin an attacker wants a scanning phone to send its bearer token to.
const HOSTILE_HOST: &str = "attacker.example.net";

/// A canonical operator-owned origin, in the shape `deploy/k8s/configmap.yaml` and the
/// appliance's `ICN_APPLIANCE_LAN_ORIGIN` both produce.
const OPERATOR_ORIGIN: &str = "https://gateway.example.coop";

/// Serializes mutation of the process-global env the QR authority reads.
static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Pins `GATEWAY_BASE_URL` (and clears `TRUSTED_PROXY_IPS` to the documented default) for the
/// test's lifetime, restoring prior values on drop under a shared lock.
struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    prior_base: Option<String>,
    prior_proxies: Option<String>,
}

impl EnvGuard {
    fn acquire(base_url: Option<&str>) -> Self {
        let lock = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prior_base = std::env::var("GATEWAY_BASE_URL").ok();
        let prior_proxies = std::env::var("TRUSTED_PROXY_IPS").ok();
        set_or_clear("GATEWAY_BASE_URL", base_url);
        // Unset => loopback-only default, which is what the appliance actually runs.
        set_or_clear("TRUSTED_PROXY_IPS", None);
        Self {
            _lock: lock,
            prior_base,
            prior_proxies,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        set_or_clear("GATEWAY_BASE_URL", self.prior_base.as_deref());
        set_or_clear("TRUSTED_PROXY_IPS", self.prior_proxies.as_deref());
    }
}

fn set_or_clear(key: &str, value: Option<&str>) {
    match value {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

/// Every request-controlled lever that has ever fed the advertised origin.
#[derive(Clone, Copy)]
struct HostileHeaders {
    host: Option<&'static str>,
    forwarded_host: Option<&'static str>,
    forwarded_proto: Option<&'static str>,
    forwarded: Option<&'static str>,
}

impl HostileHeaders {
    const fn none() -> Self {
        Self {
            host: None,
            forwarded_host: None,
            forwarded_proto: None,
            forwarded: None,
        }
    }
}

fn session_request(peer: &str, headers: HostileHeaders) -> Request {
    let peer_addr: SocketAddr = peer.parse().expect("test setup: valid socket address");
    let mut req = test::TestRequest::post()
        .uri("/v1/sessions")
        .peer_addr(peer_addr)
        .set_json(serde_json::json!({ "coop_id": "test-coop" }));

    if let Some(v) = headers.host {
        req = req.insert_header(("host", v));
    }
    if let Some(v) = headers.forwarded_host {
        req = req.insert_header(("x-forwarded-host", v));
    }
    if let Some(v) = headers.forwarded_proto {
        req = req.insert_header(("x-forwarded-proto", v));
    }
    if let Some(v) = headers.forwarded {
        req = req.insert_header(("forwarded", v));
    }
    req.to_request()
}

fn enrollment_request(peer: &str, headers: HostileHeaders) -> Request {
    let peer_addr: SocketAddr = peer.parse().expect("test setup: valid socket address");
    let mut req = test::TestRequest::post()
        .uri("/v1/sdis/enrollment/start")
        .peer_addr(peer_addr)
        .set_json(serde_json::json!({
            "identity_name": "Test User",
            "coop_id": "test-coop"
        }));

    if let Some(v) = headers.host {
        req = req.insert_header(("host", v));
    }
    if let Some(v) = headers.forwarded_host {
        req = req.insert_header(("x-forwarded-host", v));
    }
    if let Some(v) = headers.forwarded_proto {
        req = req.insert_header(("x-forwarded-proto", v));
    }
    if let Some(v) = headers.forwarded {
        req = req.insert_header(("forwarded", v));
    }
    req.to_request()
}

macro_rules! session_app {
    () => {{
        test::init_service(
            App::new()
                .app_data(web::Data::new(Arc::new(SessionManager::new())))
                .app_data(web::Data::new(Arc::new(IpRateLimiter::new_for_auth())))
                .service(web::scope("/v1/sessions").service(api::sessions::create_session)),
        )
        .await
    }};
}

macro_rules! enrollment_app {
    () => {{
        test::init_service(
            App::new()
                .app_data(web::Data::new(Arc::new(EnrollmentStore::new())))
                .service(
                    web::scope("/v1/sdis").service(api::sdis::simple_enrollment::start_enrollment),
                ),
        )
        .await
    }};
}

/// The full matrix of request-controlled authority claims, each paired with the peer position
/// that makes it maximally dangerous.
fn hostile_matrix() -> Vec<(&'static str, &'static str, HostileHeaders)> {
    vec![
        (
            "trusted proxy relays a caller-chosen Host (appliance nginx `Host $host`)",
            TRUSTED_PEER,
            HostileHeaders {
                host: Some(HOSTILE_HOST),
                ..HostileHeaders::none()
            },
        ),
        (
            "trusted proxy relays a caller-chosen X-Forwarded-Host",
            TRUSTED_PEER,
            HostileHeaders {
                host: Some("gateway.example.coop"),
                forwarded_host: Some(HOSTILE_HOST),
                ..HostileHeaders::none()
            },
        ),
        (
            "trusted proxy relays a caller-chosen X-Forwarded-Proto",
            TRUSTED_PEER,
            HostileHeaders {
                host: Some("gateway.example.coop"),
                forwarded_proto: Some("http"),
                ..HostileHeaders::none()
            },
        ),
        (
            "direct caller chooses Host (no proxy anywhere)",
            UNTRUSTED_PEER,
            HostileHeaders {
                host: Some(HOSTILE_HOST),
                ..HostileHeaders::none()
            },
        ),
        (
            "direct caller chooses RFC 7239 Forwarded",
            UNTRUSTED_PEER,
            HostileHeaders {
                host: Some("gateway.example.coop"),
                forwarded: Some("proto=http;host=attacker.example.net"),
                ..HostileHeaders::none()
            },
        ),
        (
            "direct caller chooses X-Forwarded-Host and X-Forwarded-Proto",
            UNTRUSTED_PEER,
            HostileHeaders {
                host: Some("gateway.example.coop"),
                forwarded_host: Some(HOSTILE_HOST),
                forwarded_proto: Some("http"),
                ..HostileHeaders::none()
            },
        ),
    ]
}

// ============================================================================
// Operator origin configured: it wins over every request-controlled claim.
// ============================================================================

/// Covers requirements 1, 2, 3, 4 and 5: no request-controlled lever — through a trusted proxy
/// or direct — can move the origin a scanning device will send its bearer token to.
#[actix_web::test]
async fn configured_origin_defeats_every_request_controlled_authority_claim() {
    let _env = EnvGuard::acquire(Some(OPERATOR_ORIGIN));
    let app = session_app!();

    for (label, peer, headers) in hostile_matrix() {
        let resp = test::call_service(&app, session_request(peer, headers)).await;
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "{label}: a configured operator origin must still serve QR sessions"
        );

        let body: serde_json::Value = test::read_body_json(resp).await;
        let advertised = body["qr_data"]["gateway_url"]
            .as_str()
            .expect("qr_data.gateway_url must be a string");

        assert_eq!(
            advertised, OPERATOR_ORIGIN,
            "{label}: QR authority must be the operator origin verbatim"
        );
        assert!(
            !advertised.contains(HOSTILE_HOST),
            "{label}: a scanning device would send its bearer token to {HOSTILE_HOST}"
        );
    }
}

/// Requirement 9: the shipped k8s posture (`gateway_base_url` in configmap.yaml) is unchanged —
/// an explicit host:port origin round-trips exactly, port included.
#[actix_web::test]
async fn configured_origin_with_explicit_port_is_preserved() {
    let _env = EnvGuard::acquire(Some("http://192.0.2.10:30080"));
    let app = session_app!();

    let resp = test::call_service(
        &app,
        session_request(
            TRUSTED_PEER,
            HostileHeaders {
                host: Some(HOSTILE_HOST),
                ..HostileHeaders::none()
            },
        ),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["qr_data"]["gateway_url"], "http://192.0.2.10:30080");
}

/// Requirement 8: the supported appliance path still advertises a reachable external origin.
/// The appliance gateway is bound to 127.0.0.1 and fronted by nginx, so the advertised origin
/// can only come from configuration — never from the request or the bind.
#[actix_web::test]
async fn appliance_lan_origin_is_advertised_verbatim() {
    let _env = EnvGuard::acquire(Some("https://rehearsal.lan"));
    let app = session_app!();

    let resp = test::call_service(
        &app,
        session_request(
            TRUSTED_PEER,
            HostileHeaders {
                host: Some(HOSTILE_HOST),
                forwarded_proto: Some("http"),
                ..HostileHeaders::none()
            },
        ),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: serde_json::Value = test::read_body_json(resp).await;
    let advertised = body["qr_data"]["gateway_url"].as_str().unwrap();
    assert_eq!(advertised, "https://rehearsal.lan");
    assert!(
        advertised.starts_with("https://"),
        "a TLS appliance must not advertise a downgraded scheme to a scanning device"
    );
}

/// A configured origin's trailing slash must not produce `//v1/...` at the scanner, which some
/// routers treat as a different path and some proxies collapse to a different origin.
#[actix_web::test]
async fn configured_origin_trailing_slash_is_normalized() {
    let _env = EnvGuard::acquire(Some("https://gateway.example.coop/"));
    let app = session_app!();

    let resp =
        test::call_service(&app, session_request(TRUSTED_PEER, HostileHeaders::none())).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: serde_json::Value = test::read_body_json(resp).await;
    let advertised = body["qr_data"]["gateway_url"].as_str().unwrap();
    assert_eq!(advertised, OPERATOR_ORIGIN);
    assert!(
        !advertised.ends_with('/'),
        "scanner concatenates `{{gateway_url}}/v1/...`; a trailing slash yields `//v1/...`"
    );
}

// ============================================================================
// Missing configuration: fail closed.
// ============================================================================

/// Requirement 6: with no operator-authoritative origin there is no safe value to advertise,
/// so QR creation fails rather than inferring one from the request.
#[actix_web::test]
async fn missing_configured_origin_fails_closed_for_every_peer_position() {
    let _env = EnvGuard::acquire(None);
    let app = session_app!();

    for (label, peer, headers) in hostile_matrix() {
        let resp = test::call_service(&app, session_request(peer, headers)).await;
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{label}: unconfigured QR authority must fail closed, not infer an origin"
        );

        let body = test::read_body(resp).await;
        let text = String::from_utf8_lossy(&body);
        assert!(
            !text.contains(HOSTILE_HOST),
            "{label}: the hostile authority must not reach the client even in an error body"
        );
    }
}

/// Fail-closed must not depend on the request being hostile: a perfectly benign request with no
/// forwarding headers at all is equally unable to establish an advertised origin.
#[actix_web::test]
async fn missing_configured_origin_fails_closed_for_benign_requests() {
    let _env = EnvGuard::acquire(None);
    let app = session_app!();

    let resp =
        test::call_service(&app, session_request(TRUSTED_PEER, HostileHeaders::none())).await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// An empty value is a present-but-unusable setting; treating it as "configured" would advertise
/// an empty authority, so it must fail closed exactly like an absent one.
#[actix_web::test]
async fn empty_configured_origin_fails_closed() {
    let _env = EnvGuard::acquire(Some("   "));
    let app = session_app!();

    let resp =
        test::call_service(&app, session_request(TRUSTED_PEER, HostileHeaders::none())).await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ============================================================================
// Malformed configuration: explicit, fail-closed behaviour.
// ============================================================================

/// Requirement 7. Each of these is a way a configured value could still hand a scanning device
/// somewhere the operator did not mean, so none may be silently accepted.
#[actix_web::test]
async fn malformed_configured_origin_fails_closed() {
    let cases: &[(&str, &str)] = &[
        (
            "gateway.example.coop",
            "no scheme: scanner would build a relative request",
        ),
        ("ftp://gateway.example.coop", "unsupported scheme"),
        ("javascript:alert(1)", "non-hierarchical scheme"),
        ("https://", "no host"),
        (
            "https://user:pass@gateway.example.coop",
            "userinfo: credentials in an advertised origin",
        ),
        (
            "https://gateway.example.coop/api",
            "path: silently reroutes /v1/sessions/{id}/approve",
        ),
        (
            "https://gateway.example.coop?x=1",
            "query: appended before the scanner's own path",
        ),
        (
            "https://gateway.example.coop#frag",
            "fragment: truncates the scanner's constructed URL",
        ),
        ("http://[::1", "malformed IPv6 literal"),
    ];

    for (value, why) in cases {
        let _env = EnvGuard::acquire(Some(value));
        let app = session_app!();

        let resp =
            test::call_service(&app, session_request(TRUSTED_PEER, HostileHeaders::none())).await;
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "GATEWAY_BASE_URL={value:?} must fail closed ({why})"
        );
    }
}

/// A bracketed IPv6 literal is a legitimate operator origin and must survive intact — dropping
/// the brackets would produce an unparseable URL at the scanner.
#[actix_web::test]
async fn configured_ipv6_origin_is_preserved() {
    let _env = EnvGuard::acquire(Some("http://[2001:db8::1]:30080"));
    let app = session_app!();

    let resp =
        test::call_service(&app, session_request(TRUSTED_PEER, HostileHeaders::none())).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["qr_data"]["gateway_url"], "http://[2001:db8::1]:30080");
}

// ============================================================================
// Enrollment path parity.
// ============================================================================

/// Requirement 10: the enrollment QR is a second device-facing payload built from the same
/// authority, so it must not be able to reintroduce request-derived origins.
#[actix_web::test]
async fn enrollment_qr_authority_is_operator_owned() {
    let _env = EnvGuard::acquire(Some(OPERATOR_ORIGIN));
    let app = enrollment_app!();

    for (label, peer, headers) in hostile_matrix() {
        let resp = test::call_service(&app, enrollment_request(peer, headers)).await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "{label}: configured origin must still serve enrollment QR"
        );

        let body: serde_json::Value = test::read_body_json(resp).await;
        let qr = body["qr_code"].as_str().expect("qr_code must be a string");
        assert!(
            qr.contains(&format!("\"gateway_url\":\"{OPERATOR_ORIGIN}\"")),
            "{label}: enrollment QR authority must be the operator origin, got {qr}"
        );
        assert!(
            !qr.contains(HOSTILE_HOST),
            "{label}: enrollment QR would point a device at {HOSTILE_HOST}"
        );
    }
}

/// Enrollment fails closed on missing configuration for the same reason sessions do.
#[actix_web::test]
async fn enrollment_qr_fails_closed_without_configured_origin() {
    let _env = EnvGuard::acquire(None);
    let app = enrollment_app!();

    let resp = test::call_service(
        &app,
        enrollment_request(TRUSTED_PEER, HostileHeaders::none()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}
