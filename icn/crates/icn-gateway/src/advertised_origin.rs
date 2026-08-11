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
//! Where each shipped profile that can issue QR material gets it:
//!
//! - k8s supplies it from `deploy/k8s/configmap.yaml` (`gateway_base_url`).
//! - The LAN appliance supplies it from `ICN_APPLIANCE_LAN_ORIGIN` via `@LAN_ORIGIN@`, the same
//!   already-validated origin its CORS allowlist uses.
//! - `deploy/devnet` — ADR-0086's canonical Compose entry point — takes
//!   `ICN_DEVNET_NODE_{A,B,C}_ORIGIN`, defaulting to empty rather than to a fabricated URL:
//!   each node is published on a different host port and the host is unknown, so no static
//!   default could name an origin a scanning phone can reach. Empty is treated as unset.
//! - Native installs get a documented, commented example in `deploy/icnd.env.example`.
//!
//! The other Compose trees (`deploy/docker-compose.yml`, `deploy/compose/`) are compatibility
//! material under ADR-0086 and are deliberately not wired here; so are `deploy/kubernetes/` and
//! `deploy/helm/icn/`, which cannot bring a gateway to Ready at all (they probe
//! `/health/liveness`, a route that does not exist, and set env names nothing reads).
//!
//! Profiles that deliberately supply nothing, where failing closed is the correct outcome:
//! the appliance base unit (bound to `127.0.0.1`) and the QEMU demo profile (`0.0.0.0` behind
//! user-mode hostfwd is host-only, so no second device can route to it at all).
//!
//! There is deliberately **no inference fallback**. A bind address is not an advertised origin:
//! the appliance gateway is bound to `127.0.0.1`, and `0.0.0.0` names no reachable host at all,
//! so neither could be handed to a scanning phone. With no configured origin there is no safe
//! value to advertise, so QR issuance fails closed (503) rather than guessing one.
//!
//! That sentence is enforced, not merely asserted: an operator who pastes a wildcard bind into
//! the variable is refused. `0.0.0.0`, `[::]` and their IPv4-mapped form parse as perfectly
//! ordinary tuple origins, so validation rejects unspecified hosts explicitly — a phone that
//! resolved one would address itself rather than this gateway.
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
    let configured = configured_origin().unwrap_or_default();
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

    // Port 0 parses (WHATWG allows it, and `url`'s `parse_port` only rejects >u16::MAX), and
    // would be advertised verbatim as `https://host:0`. No device can connect there — port 0
    // names "any free port" to a listener and is not a destination to a client. Rejecting it
    // is the same rule as the path/query/fragment cases above: refuse a value the scanner
    // cannot use, here at the gateway with a logged reason, rather than shipping a QR code
    // that fails silently in someone's hand.
    if url.port() == Some(0) {
        return Err(unusable("port 0 is not a reachable destination"));
    }

    // The wildcard *bind* addresses, pasted into an *advertised* origin. `0.0.0.0` and `[::]`
    // parse as ordinary tuple origins, so nothing above rejects them — but they name no host.
    // A phone that resolved one would target itself, not this gateway. This is the same rule as
    // port 0, and the module contract above already says a bind address is not an origin; this
    // is where that sentence becomes enforceable rather than advisory.
    if let Some(url::Host::Ipv4(v4)) = url.host() {
        if v4.is_unspecified() {
            return Err(unusable(
                "unspecified address is not a reachable destination",
            ));
        }
    }
    if let Some(url::Host::Ipv6(v6)) = url.host() {
        // `to_ipv4_mapped`, not `to_ipv4`: the latter also unwraps IPv4-*compatible* addresses,
        // which is the deprecated form this codebase deliberately avoids (#2564).
        let mapped_unspecified = v6.to_ipv4_mapped().is_some_and(|v4| v4.is_unspecified());
        if v6.is_unspecified() || mapped_unspecified {
            return Err(unusable(
                "unspecified address is not a reachable destination",
            ));
        }
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

/// Where the raw configured origin comes from.
///
/// In a normal build this is the process environment — the operator's deployment authority.
/// Under `cfg(test)` it is a thread-local slot owned by [`test_env`] instead; see that module
/// for why the lib test process must not resolve this through a shared global.
#[cfg(not(test))]
fn configured_origin() -> Option<String> {
    std::env::var(GATEWAY_BASE_URL_ENV).ok()
}

#[cfg(test)]
fn configured_origin() -> Option<String> {
    test_env::configured_origin()
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

/// Per-test origin configuration for the lib test binary.
///
/// Every `#[cfg(test)]` module in this crate compiles into the *same* lib test binary, and
/// libtest runs those tests concurrently in one process. A process-global therefore has no
/// owner: two modules that each mutate `GATEWAY_BASE_URL` under their own mutex still race on
/// the one variable, and a per-module lock serializes nothing against the other module's. That
/// is not merely flaky — it lets a fail-closed assertion observe a *different* test's
/// configured origin and pass for the wrong reason.
///
/// A previous revision tried to hold that line by asserting, via a source scan, that this
/// module was the crate's only mutator. That is the wrong enforcement boundary: source
/// spelling is not a structural property, and review found the scan blind to a rustfmt-wrapped
/// `set_var(\n  "GATEWAY_BASE_URL",\n  ..)` — a form the formatter itself produces.
///
/// So the shared mutable state is removed instead of policed. Under `cfg(test)` the origin
/// resolves from a thread-local, and libtest gives each test its own thread. Tests are
/// order-independent by construction and need no lock, and a stray `std::env::set_var` cannot
/// steer [`advertised_origin`] — there is no spelling left to detect, because this resolver
/// reads no shared cell. `tests::process_env_mutation_is_inert_against_the_code_under_test`
/// proves exactly that, using the counterexamples the scan missed.
///
/// **Scope of that claim, precisely.** It covers this module's resolver, not every environment
/// reader in the crate. `GATEWAY_BASE_URL` now has exactly one consumer — this module — because
/// [`crate::api::invites`] was moved onto its own `ICN_INVITE_BASE_URL` (the same variable was
/// previously carrying both a device-facing API origin and a human-facing UI origin). That
/// module still reads *its* variable from the real process environment, so a test mutating
/// `ICN_INVITE_BASE_URL` can perturb it. What is guaranteed here is narrower and exact: nothing
/// a test does to the process environment can change the advertised origin.
///
/// The production path is unchanged and still reads the process environment; integration
/// tests in `tests/` link the lib without `cfg(test)`, so they exercise that real read
/// end-to-end through the HTTP call sites.
#[cfg(test)]
pub(crate) mod test_env {
    use std::cell::RefCell;

    thread_local! {
        /// `None` models an unconfigured gateway. Private to this module: the only way to
        /// change it is [`EnvGuard`], which restores on drop.
        static CONFIGURED_ORIGIN: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    /// Read by `super::configured_origin()` under `cfg(test)`.
    pub(super) fn configured_origin() -> Option<String> {
        CONFIGURED_ORIGIN.with(|slot| slot.borrow().clone())
    }

    /// Pins the advertised origin for this test's thread for the guard's lifetime, restoring
    /// the prior value on drop. Hold it for as long as the value must stay pinned.
    ///
    /// Restoring matters even though each test normally owns its thread: `--test-threads=1`
    /// runs tests on a shared thread, and restore-on-drop keeps that mode order-independent
    /// too. `EnvGuard::acquire(None)` is the fail-closed precondition, and it is a real
    /// assertion of absence rather than the hope that nothing else configured one.
    pub(crate) struct EnvGuard {
        prior: Option<String>,
    }

    impl EnvGuard {
        pub(crate) fn acquire(value: Option<&str>) -> Self {
            let prior = CONFIGURED_ORIGIN.with(|slot| slot.replace(value.map(str::to_owned)));
            Self { prior }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            let prior = self.prior.take();
            CONFIGURED_ORIGIN.with(|slot| *slot.borrow_mut() = prior);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_env::EnvGuard;
    use super::*;

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
            // Parses fine; nothing can connect to it.
            (Some("https://gateway.example.coop:0"), "port zero"),
            (Some("http://192.0.2.10:0"), "port zero on a literal IPv4"),
            // A wildcard *bind* pasted into an *advertised* origin. Parses as a tuple origin,
            // but names no host: a phone resolving it would target itself.
            (Some("http://0.0.0.0:8080"), "unspecified IPv4 bind"),
            (Some("http://0.0.0.0"), "unspecified IPv4, default port"),
            (Some("http://[::]:8080"), "unspecified IPv6 bind"),
            (Some("https://[::]"), "unspecified IPv6, default port"),
            (
                Some("http://[::ffff:0.0.0.0]:8080"),
                "IPv4-mapped unspecified",
            ),
        ];

        for (configured, why) in cases {
            let err = resolve(configured).expect_err(&format!("{why} must be rejected"));
            assert!(
                matches!(err, GatewayError::ServiceUnavailable(_)),
                "{why}: expected fail-closed ServiceUnavailable, got {err:?}"
            );
        }
    }

    /// The property the deleted source scanner was reaching for, asserted behaviourally.
    ///
    /// That scanner tried to prove "no other module mutates `GATEWAY_BASE_URL`" by matching
    /// `set_var(`/`remove_var(` and the variable name **on the same source line**. Review
    /// supplied two counterexamples; measuring them showed one real hole:
    ///
    /// - `set_var(GATEWAY_BASE_URL_ENV, ..)` was in fact caught — the constant's identifier
    ///   contains the variable name as a substring — so that half of the report was wrong;
    /// - the rustfmt-wrapped multi-line call below was **not** caught, and rustfmt produces
    ///   exactly that shape whenever the arguments cross the width limit. A guard the
    ///   formatter can silently disarm is not a guard.
    ///
    /// The fix is not a third spelling. Under `cfg(test)` the origin no longer resolves
    /// through any process-global, so both spellings — and any spelling not yet imagined —
    /// are inert against the code under test. This test runs them to show that, and would
    /// fail loudly if `configured_origin()` were ever repointed at the environment.
    ///
    /// Scope: `advertised_origin` only. `api::invites` still reads the variable straight from
    /// the environment, so these mutations remain visible *there* — see the `test_env` module
    /// doc. This asserts what the resolver ignores, not that the crate has no env readers.
    #[test]
    fn process_env_mutation_is_inert_against_the_code_under_test() {
        let _guard = EnvGuard::acquire(Some("https://gateway.example.coop"));

        // Counterexample 1: single-line literal — the form the scanner did catch.
        std::env::set_var(GATEWAY_BASE_URL_ENV, "https://attacker.example");
        assert_eq!(
            advertised_origin().expect("guard value must survive"),
            "https://gateway.example.coop",
            "a single-line set_var reached the code under test"
        );

        // Counterexample 2: the rustfmt-wrapped form the scanner could not see.
        #[rustfmt::skip]
        std::env::set_var(
            GATEWAY_BASE_URL_ENV,
            "https://attacker.example/hijacked",
        );
        assert_eq!(
            advertised_origin().expect("guard value must survive"),
            "https://gateway.example.coop",
            "a multi-line set_var reached the code under test"
        );

        // And removal cannot forge a fail-closed result either: a fail-closed assertion must
        // observe *its own* absence, never another test's `remove_var`.
        std::env::remove_var(GATEWAY_BASE_URL_ENV);
        assert_eq!(
            advertised_origin().expect("guard value must survive"),
            "https://gateway.example.coop",
            "a remove_var reached the code under test"
        );
    }

    /// Order-independence, asserted rather than assumed.
    ///
    /// The failure this rules out is directional and silent: a test asserting the fail-closed
    /// 503 can only be *fooled* by inheriting a configured origin, so absence must be
    /// established by the test itself. Taking the guard after a sibling has pinned a value
    /// must yield absence, and dropping it must restore what the sibling had — which is what
    /// keeps `--test-threads=1`, where tests share a thread, honest too.
    #[test]
    fn absence_is_established_not_inherited() {
        let outer = EnvGuard::acquire(Some("https://sibling.example.coop"));
        assert_eq!(
            advertised_origin().expect("sibling value applies"),
            "https://sibling.example.coop"
        );

        {
            let _inner = EnvGuard::acquire(None);
            assert!(
                matches!(
                    advertised_origin(),
                    Err(GatewayError::ServiceUnavailable(_))
                ),
                "acquiring None must mean absence, not the sibling's origin"
            );
        }

        assert_eq!(
            advertised_origin().expect("prior value must be restored on drop"),
            "https://sibling.example.coop",
            "dropping a nested guard must restore, or a shared thread leaks between tests"
        );
        drop(outer);
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
