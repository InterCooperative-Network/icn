//! The origin this gateway advertises to other devices (#2569).
//!
//! # Why this is not derived from the request
//!
//! QR login and SDIS enrollment embed a `gateway_url` in QR material that a **second device**
//! consumes. The reference scanner posts to `{gateway_url}/v1/sessions/{id}/approve` carrying
//! `Authorization: Bearer <token>` (`examples/mobile-app/src/screens/QRScannerScreen.tsx`), so
//! whoever controls that origin receives a member's bearer credential. The advertised origin is
//! therefore a **credential destination**, and its authority must be an operator assertion.
//!
//! Request metadata cannot carry that authority, for two independent reasons:
//!
//! - **A direct caller chooses its own `Host`.** The requester need not be the scanner: it can
//!   obtain QR material with an attacker-chosen authority and present that QR to someone else.
//!   "The requester sees its own QR" does not make request-derived `Host` self-targeting.
//! - **A trusted proxy authenticates the sender, not the provenance of what it relays.** #2567
//!   established that a trusted immediate peer may assert the *client IP*; it does not follow
//!   that every value such a peer forwards is an operator assertion. The appliance's nginx is
//!   `default_server` and historically relayed `proxy_set_header Host $host`, so a caller-chosen
//!   `Host` arrived over loopback wearing the proxy's trust.
//!
//! Actix's [`actix_web::HttpRequest::connection_info`] is not an escape hatch either: it parses
//! RFC 7239 `Forwarded` and then `X-Forwarded-Proto`/`-Host` with **no** trusted-peer gate, so
//! reading it would reintroduce exactly the header control this module exists to remove. Nothing
//! here takes an `HttpRequest`, which makes that regression a compile error rather than a review
//! question.
//!
//! # Configuration
//!
//! `GATEWAY_BASE_URL` is the single source of authority: one canonical externally reachable
//! origin per gateway process.
//!
//! - k8s supplies it from `deploy/k8s/configmap.yaml` (`gateway_base_url`).
//! - The LAN appliance supplies it from `ICN_APPLIANCE_LAN_ORIGIN` via `@LAN_ORIGIN@`, the same
//!   already-validated origin its CORS allowlist uses.
//!
//! There is deliberately **no inference fallback**. A bind address is not an advertised origin:
//! the appliance gateway is bound to `127.0.0.1`, and `0.0.0.0` names no reachable host at all,
//! so neither could be handed to a scanning phone. With no configured origin there is no safe
//! value to advertise, so QR issuance fails closed (503) rather than guessing one.
//!
//! Validation happens at use time rather than startup: a gateway that never issues QR material
//! should not fail to boot over configuration it does not need.

use url::Url;

use crate::error::{GatewayError, Result};

/// Operator-set absolute base URL for this gateway's externally reachable origin.
pub(crate) const GATEWAY_BASE_URL_ENV: &str = "GATEWAY_BASE_URL";

/// The operator-authoritative `scheme://host[:port]` to embed in device-facing QR material.
///
/// Returns [`GatewayError::ServiceUnavailable`] when no usable operator origin is configured.
/// The rejection reason is logged for the operator and deliberately **not** returned to the
/// caller: an unauthenticated client learning why the gateway's own configuration was refused
/// gains nothing it should have.
pub(crate) fn advertised_origin() -> Result<String> {
    let configured = std::env::var(GATEWAY_BASE_URL_ENV).unwrap_or_default();
    let configured = configured.trim();

    if configured.is_empty() {
        return Err(unusable("unset or empty"));
    }

    let url =
        Url::parse(configured).map_err(|e| unusable(&format!("not an absolute URL ({e})")))?;

    match url.scheme() {
        "http" | "https" => {}
        other => return Err(unusable(&format!("unsupported scheme `{other}`"))),
    }

    // Credentials in an advertised origin would be handed to every scanning device.
    if !url.username().is_empty() || url.password().is_some() {
        return Err(unusable("must not contain userinfo"));
    }

    // The scanner appends its own `/v1/...` path. Anything already occupying the path, query, or
    // fragment silently reroutes or truncates the approval request it builds.
    if !matches!(url.path(), "" | "/") {
        return Err(unusable("must not contain a path"));
    }
    if url.query().is_some() {
        return Err(unusable("must not contain a query string"));
    }
    if url.fragment().is_some() {
        return Err(unusable("must not contain a fragment"));
    }

    let origin = url.origin();
    if !origin.is_tuple() {
        return Err(unusable("does not denote a host-bearing origin"));
    }

    // Serialize through `Origin` rather than reassembling from parts: it keeps IPv6 literals
    // bracketed (`host_str()` strips the brackets, which would yield an unparseable URL at the
    // scanner), drops a redundant default port, and emits no trailing slash — so the scanner's
    // `{origin}/v1/...` concatenation cannot produce `//v1/...`.
    Ok(origin.ascii_serialization())
}

/// Log the operator-facing reason and return the client-facing refusal.
fn unusable(reason: &str) -> GatewayError {
    tracing::error!(
        env = GATEWAY_BASE_URL_ENV,
        reason,
        "Refusing to issue QR material: no operator-authoritative gateway origin. A QR \
         `gateway_url` is where a scanning device sends its bearer credential, so it cannot be \
         inferred from the request or the bind address."
    );
    GatewayError::ServiceUnavailable(
        "QR login is not configured on this gateway: no advertised gateway origin".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// Serializes mutation of the process-global env this module reads.
    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _lock: MutexGuard<'static, ()>,
        prior: Option<String>,
    }

    impl EnvGuard {
        fn acquire(value: Option<&str>) -> Self {
            let lock = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prior = std::env::var(GATEWAY_BASE_URL_ENV).ok();
            match value {
                Some(v) => std::env::set_var(GATEWAY_BASE_URL_ENV, v),
                None => std::env::remove_var(GATEWAY_BASE_URL_ENV),
            }
            Self { _lock: lock, prior }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.prior.as_deref() {
                Some(v) => std::env::set_var(GATEWAY_BASE_URL_ENV, v),
                None => std::env::remove_var(GATEWAY_BASE_URL_ENV),
            }
        }
    }

    fn resolve(value: Option<&str>) -> Result<String> {
        let _guard = EnvGuard::acquire(value);
        advertised_origin()
    }

    #[test]
    fn canonicalizes_accepted_origins() {
        // (configured, advertised)
        let cases = [
            (
                "https://gateway.example.coop",
                "https://gateway.example.coop",
            ),
            // trailing slash dropped: the scanner concatenates `{origin}/v1/...`
            (
                "https://gateway.example.coop/",
                "https://gateway.example.coop",
            ),
            // explicit non-default port preserved (the k8s NodePort posture)
            ("http://192.0.2.10:30080", "http://192.0.2.10:30080"),
            // redundant default port dropped
            (
                "https://gateway.example.coop:443",
                "https://gateway.example.coop",
            ),
            // IPv6 literal stays bracketed
            ("http://[2001:db8::1]:30080", "http://[2001:db8::1]:30080"),
            // host case is normalized, not attacker-relevant but must not error
            (
                "HTTPS://Gateway.Example.COOP",
                "https://gateway.example.coop",
            ),
            // surrounding whitespace from a config file is not a malformed value
            ("  https://rehearsal.lan  ", "https://rehearsal.lan"),
        ];

        for (configured, expected) in cases {
            assert_eq!(
                resolve(Some(configured)).expect("should be accepted"),
                expected,
                "GATEWAY_BASE_URL={configured:?}"
            );
        }
    }

    #[test]
    fn rejects_unusable_origins() {
        let cases = [
            (None, "absent"),
            (Some(""), "empty"),
            (Some("   "), "whitespace only"),
            (Some("gateway.example.coop"), "no scheme"),
            (Some("//gateway.example.coop"), "scheme-relative"),
            (Some("ftp://gateway.example.coop"), "unsupported scheme"),
            (Some("javascript:alert(1)"), "non-hierarchical scheme"),
            (Some("https://"), "no host"),
            (Some("http://[::1"), "malformed IPv6 literal"),
            (Some("https://user:pass@gateway.example.coop"), "userinfo"),
            (Some("https://user@gateway.example.coop"), "username only"),
            (Some("https://gateway.example.coop/api"), "path"),
            (Some("https://gateway.example.coop?x=1"), "query"),
            (Some("https://gateway.example.coop#frag"), "fragment"),
        ];

        for (configured, why) in cases {
            let err = resolve(configured).expect_err(&format!("{why} must be rejected"));
            assert!(
                matches!(err, GatewayError::ServiceUnavailable(_)),
                "{why}: expected fail-closed ServiceUnavailable, got {err:?}"
            );
        }
    }

    /// The refusal must not echo the configured value back to an unauthenticated caller.
    #[test]
    fn refusal_does_not_leak_the_configured_value() {
        let err = resolve(Some("https://internal-admin.example.coop/secret-path"))
            .expect_err("path must be rejected");
        let rendered = err.to_string();
        assert!(!rendered.contains("internal-admin"));
        assert!(!rendered.contains("secret-path"));
    }
}
