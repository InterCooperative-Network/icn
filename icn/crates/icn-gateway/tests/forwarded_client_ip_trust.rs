#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Forwarded client identity must be gated on the immediate transport peer (#2567).
//!
//! The invariant under test:
//!
//! > Forwarded client identity is an untrusted request claim unless the immediate transport
//! > peer is explicitly trusted to assert forwarding metadata. If the immediate peer is not
//! > trusted, the socket peer IP is the client IP.
//!
//! These tests enter through the real route (`POST /auth/challenge`) rather than calling the
//! extraction helper directly, because the property at stake is which value reaches
//! `IpRateLimiter`, not what a comma parser returns.
//!
//! Rate-limit identity is observed behaviourally: drain one bucket, then check whether a
//! caller-supplied header can mint a second one from the same socket.

use std::net::SocketAddr;
use std::sync::Arc;

use actix_http::Request;
use actix_web::http::StatusCode;
use actix_web::{test, web, App};
use icn_gateway::api;
use icn_gateway::auth::AuthManager;
use icn_gateway::rate_limit::IpRateLimiter;
use icn_gateway::session_authority::SessionAuthority;
use icn_identity::IdentityBundle;

/// A routable, non-loopback socket peer: NOT a trusted proxy under the default policy.
const UNTRUSTED_PEER: &str = "198.51.100.42:54321";

/// Loopback is trusted to assert forwarding metadata when `TRUSTED_PROXY_IPS` is unset,
/// per `crate::client_ip`. That trust covers rate-limit identity only — it does not extend to
/// the origin advertised in QR material, which is operator-configured (#2569).
const TRUSTED_PEER: &str = "127.0.0.1:54321";

/// `IpRateLimiter::new_for_auth` has capacity 20 and refills 2 tokens/second. An in-process
/// drain completes in milliseconds, so this bound is generous but finite.
const MAX_DRAIN: usize = 200;

fn challenge_request(peer: &str, did: &str, forwarded_for: Option<&str>) -> Request {
    let peer_addr: SocketAddr = peer.parse().expect("test setup: valid socket address");
    let mut req = test::TestRequest::post()
        .uri("/auth/challenge")
        .peer_addr(peer_addr)
        .set_json(serde_json::json!({ "did": did }));

    if let Some(value) = forwarded_for {
        req = req.insert_header(("x-forwarded-for", value));
    }

    req.to_request()
}

fn test_app_data() -> (Arc<SessionAuthority>, Arc<IpRateLimiter>) {
    let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
    (
        Arc::new(SessionAuthority::evaluator(auth)),
        Arc::new(IpRateLimiter::new_for_auth()),
    )
}

/// Build the app, returning it alongside a freshly generated DID string.
macro_rules! challenge_app {
    () => {{
        let (authority, ip_limiter) = test_app_data();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(authority))
                .app_data(web::Data::new(ip_limiter))
                .service(api::auth::challenge),
        )
        .await;
        let bundle = IdentityBundle::generate().expect("test setup: identity generation");
        (app, bundle.did().to_string())
    }};
}

/// Send requests until the limiter rejects one. Returns how many it took.
///
/// This is the control half of every test below: if the bucket never drains, a later
/// "still limited" assertion would pass for the wrong reason.
macro_rules! drain_bucket {
    ($app:expr, $peer:expr, $did:expr, $xff:expr) => {{
        let mut attempts = 0usize;
        let mut drained = false;
        for _ in 0..MAX_DRAIN {
            attempts += 1;
            let resp = test::call_service(&$app, challenge_request($peer, &$did, $xff)).await;
            if resp.status() == StatusCode::TOO_MANY_REQUESTS {
                drained = true;
                break;
            }
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "control: unexpected status while draining the bucket"
            );
        }
        assert!(
            drained,
            "control failed: the limiter never rejected a request from {} after {} attempts, \
             so a later 'still limited' assertion would be vacuous",
            $peer, MAX_DRAIN
        );
        attempts
    }};
}

/// An untrusted socket peer cannot relocate its own rate-limit identity with a header.
#[actix_web::test]
async fn untrusted_peer_cannot_mint_buckets_with_forwarded_for() {
    let (app, did) = challenge_app!();

    drain_bucket!(app, UNTRUSTED_PEER, did, None);

    // Same socket, a novel forwarded claim per request.
    for n in 1..=5u8 {
        let spoofed = format!("203.0.113.{n}");
        let resp = test::call_service(
            &app,
            challenge_request(UNTRUSTED_PEER, &did, Some(&spoofed)),
        )
        .await;

        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "peer {UNTRUSTED_PEER} is not a trusted proxy, so `X-Forwarded-For: {spoofed}` \
             must not mint a fresh limiter bucket"
        );
    }
}

/// The chain shape produced by the repository's own nginx config must not move the key either.
///
/// `deploy/config/nginx.conf` sets `X-Forwarded-For $proxy_add_x_forwarded_for`, which
/// *appends* the peer address to whatever the client sent. A client that sends
/// `X-Forwarded-For: 203.0.113.7` causes the upstream to receive
/// `203.0.113.7, <real-peer>` — so the leftmost element is caller-controlled end to end.
#[actix_web::test]
async fn untrusted_peer_cannot_mint_buckets_with_appended_chain() {
    let (app, did) = challenge_app!();

    drain_bucket!(app, UNTRUSTED_PEER, did, None);

    for n in 1..=5u8 {
        let chain = format!("203.0.113.{n}, 198.51.100.42");
        let resp =
            test::call_service(&app, challenge_request(UNTRUSTED_PEER, &did, Some(&chain))).await;

        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "peer {UNTRUSTED_PEER} is not a trusted proxy, so the nginx-shaped chain \
             `{chain}` must not mint a fresh limiter bucket"
        );
    }
}

/// From a *trusted* peer, the caller-prepended element must not displace the real client.
///
/// This is the nginx case with the proxy actually in front: the socket peer is the proxy, the
/// chain is `<caller claim>, <address nginx saw>`. Only the latter is proxy-asserted, so it —
/// not the prepended claim — is the client identity.
#[actix_web::test]
async fn trusted_peer_ignores_caller_prepended_chain_element() {
    let (app, did) = challenge_app!();

    // Establish the bucket belonging to the address the proxy actually observed.
    drain_bucket!(app, TRUSTED_PEER, did, Some("198.51.100.42"));

    for n in 1..=5u8 {
        let chain = format!("203.0.113.{n}, 198.51.100.42");
        let resp =
            test::call_service(&app, challenge_request(TRUSTED_PEER, &did, Some(&chain))).await;

        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the proxy asserted 198.51.100.42; the caller-prepended `203.0.113.{n}` in \
             `{chain}` must not mint a fresh limiter bucket"
        );
    }
}

/// Audit attribution must not be mintable either.
///
/// `api/auth.rs` derives one `client_ip` and feeds it to both `IpRateLimiter` and
/// `AuditLogger`. This captures the `auth_attempt` audit event and asserts the recorded
/// address is the one the transport establishes, not the one the caller asked for.
mod audit_attribution {
    use super::*;
    use std::sync::Mutex;
    use tracing::field::{Field, Visit};
    use tracing_subscriber::layer::{Context, SubscriberExt};
    use tracing_subscriber::Layer;

    #[derive(Default)]
    struct Fields {
        event_type: Option<String>,
        ip: Option<String>,
    }

    impl Visit for Fields {
        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "event_type" => self.event_type = Some(value.to_string()),
                "ip" => self.ip = Some(value.to_string()),
                _ => {}
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            let rendered = format!("{value:?}");
            self.record_str(field, rendered.trim_matches('"'));
        }
    }

    #[derive(Clone, Default)]
    struct CapturedAuthIps(Arc<Mutex<Vec<String>>>);

    impl<S: tracing::Subscriber> Layer<S> for CapturedAuthIps {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut fields = Fields::default();
            event.record(&mut fields);
            if fields.event_type.as_deref() == Some("auth_attempt") {
                if let Some(ip) = fields.ip {
                    self.0.lock().expect("capture mutex").push(ip);
                }
            }
        }
    }

    /// Drive one failing `/auth/verify` and return the audited client IP.
    async fn audited_ip_for(peer: &str, forwarded_for: Option<&str>) -> String {
        let captured = CapturedAuthIps::default();
        let subscriber = tracing_subscriber::registry().with(captured.clone());
        // Thread-local: `#[actix_web::test]` drives the handler on this thread, and other
        // tests in this binary are unaffected.
        let _guard = tracing::subscriber::set_default(subscriber);

        let (authority, ip_limiter) = test_app_data();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(authority))
                .app_data(web::Data::new(ip_limiter))
                .service(api::auth::verify),
        )
        .await;

        let bundle = IdentityBundle::generate().expect("test setup: identity generation");
        let peer_addr: SocketAddr = peer.parse().expect("test setup: valid socket address");
        let mut req = test::TestRequest::post()
            .uri("/auth/verify")
            .peer_addr(peer_addr)
            .set_json(serde_json::json!({
                "did": bundle.did().to_string(),
                // Well-formed but wrong: reaches the failure path that audits the attempt.
                "signature": hex::encode([0u8; 64]),
                "coop_id": "test-coop",
                "scopes": [],
            }));
        if let Some(value) = forwarded_for {
            req = req.insert_header(("x-forwarded-for", value));
        }

        let resp = test::call_service(&app, req.to_request()).await;
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "control: the request must reach the audited failure path"
        );

        let ips = captured.0.lock().expect("capture mutex").clone();
        assert_eq!(
            ips.len(),
            1,
            "control: expected exactly one audited auth_attempt, got {ips:?}"
        );
        ips.into_iter().next().expect("one captured ip")
    }

    #[actix_web::test]
    async fn untrusted_peer_is_audited_by_its_socket_address() {
        let audited = audited_ip_for(UNTRUSTED_PEER, Some("203.0.113.7")).await;
        assert_eq!(
            audited, "198.51.100.42",
            "peer {UNTRUSTED_PEER} is not a trusted proxy, so the audit record must name the \
             socket address, not the caller-supplied 203.0.113.7"
        );
    }

    #[actix_web::test]
    async fn trusted_peer_audits_the_proxy_asserted_client() {
        let audited = audited_ip_for(TRUSTED_PEER, Some("203.0.113.7, 198.51.100.42")).await;
        assert_eq!(
            audited, "198.51.100.42",
            "the proxy asserted 198.51.100.42; the caller-prepended 203.0.113.7 must not \
             become the audit identity"
        );
    }
}

/// Guard against over-correction: trusted forwarding must still distinguish real clients.
///
/// If the fix simply keyed everything on the socket peer, every client behind the proxy would
/// share one bucket and this test would fail.
#[actix_web::test]
async fn trusted_peer_still_separates_distinct_forwarded_clients() {
    let (app, did) = challenge_app!();

    drain_bucket!(app, TRUSTED_PEER, did, Some("203.0.113.7"));

    let resp = test::call_service(
        &app,
        challenge_request(TRUSTED_PEER, &did, Some("203.0.113.8")),
    )
    .await;

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "203.0.113.8 is a different client asserted by a trusted proxy and must have its own \
         bucket; collapsing all proxied clients onto the proxy address would be a regression"
    );
}
