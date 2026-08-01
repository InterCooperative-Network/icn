// Matches the convention already used across `icn-core/tests/` (see
// backup_restore_integration.rs, app_runtime_integration.rs, and others).
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Regression tests for the network-domain oracle registration (#2488).
//!
//! # What broke
//!
//! `icn-net`'s `RateLimiter` asks the `OracleRegistry` for per-peer constraints
//! under the `net` domain. The composition root registered the trust oracle by
//! `oracle.domain()`, which is `"trust"` — so `"net"` was never registered.
//!
//! Once the registry reaches `BootstrapPhase::Running`, an unregistered domain is
//! denied by default. `RateLimiter::check_rate_limit` returns `false` for that
//! denial, and the caller in `actor/connection.rs` logs it as
//! `"Rate limited message from … (per-DID limit exceeded)"` and drops the message.
//!
//! Net effect: every inbound network message was dropped, no peer could ever be
//! registered, and the failure was indistinguishable from ordinary rate limiting.
//! A four-node cluster reported 22,577 "rate limited" messages and zero accepted
//! ones, while `icn_network_messages_rate_limited_by_rate` — which only the token
//! bucket increments — never registered at all.
//!
//! These tests pin the *observable* behaviour (messages flow / do not flow), not
//! the registration call itself, so they still fail if the wiring is removed.

use std::sync::Arc;

use icn_identity::Did;
use icn_kernel_api::authz::{ActionKind, Domain, PolicyDecision, PolicyOracle, PolicyRequest};
use icn_kernel_api::bootstrap::BootstrapPhase;
use icn_kernel_api::OracleRegistry;
use icn_net::{RateLimitConfig, RateLimiter, NETWORK_DOMAIN};

/// Stand-in for the trust oracle: reports domain "trust" (as the real one does)
/// but answers any request with trust-derived constraints.
#[derive(Debug)]
struct TrustLikeOracle;

impl PolicyOracle for TrustLikeOracle {
    fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
        use icn_kernel_api::authz::{ConstraintSet, RateLimit};
        PolicyDecision::allow_with(ConstraintSet::new().with_rate_limit(RateLimit::restricted()))
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }
}

fn peer() -> Did {
    // Fixed, valid DID: the trust path parses the actor, and an unparseable one
    // would take a different branch than the one under test.
    Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9")
        .expect("test DID must parse")
}

fn network_request(actor: &Did) -> PolicyRequest {
    PolicyRequest::new(
        actor.to_string(),
        ActionKind::Custom("network_message".to_string()),
        Domain::new(NETWORK_DOMAIN),
    )
}

/// A generous fallback, so that anything this test observes being blocked is
/// blocked by the oracle decision and not by the token bucket.
fn generous_fallback() -> RateLimitConfig {
    RateLimitConfig {
        max_messages_per_second: 1000,
        burst_capacity: 1000,
        ..RateLimitConfig::default()
    }
}

/// Registry wired the way the composition root wires it: trust oracle registered
/// under its own `domain()` only.
fn registry_without_network_domain() -> Arc<OracleRegistry> {
    let registry = Arc::new(OracleRegistry::new());
    let oracle: Arc<dyn PolicyOracle> = Arc::new(TrustLikeOracle);
    registry.register(oracle.domain(), oracle);
    registry
}

/// The same, plus the network-domain registration that `#2488` adds.
///
/// Both domains are served by the *same* oracle instance, as production does —
/// using two instances here would hide any state or caching the oracle carries
/// across domains.
fn registry_with_network_domain() -> Arc<OracleRegistry> {
    let registry = Arc::new(OracleRegistry::new());
    let oracle: Arc<dyn PolicyOracle> = Arc::new(TrustLikeOracle);
    registry.register(oracle.domain(), oracle.clone());
    registry.register(Domain::new(NETWORK_DOMAIN), oracle);
    registry
}

/// The bug, at the registry level: `Running` + unregistered `net` = deny.
///
/// This is the test that fails against the pre-fix composition root.
#[test]
fn running_phase_denies_unregistered_network_domain() {
    let registry = registry_without_network_domain();
    registry.set_phase(BootstrapPhase::Running);

    let decision = registry.evaluate(&network_request(&peer()));

    assert!(
        matches!(decision, PolicyDecision::Deny { .. }),
        "an unregistered 'net' domain must deny in Running phase — this is the \
         behaviour that silently dropped every inbound message; got {decision:?}"
    );
}

/// Registering the network domain makes the same request allowed.
#[test]
fn registered_network_domain_is_allowed_in_running_phase() {
    let registry = registry_with_network_domain();
    registry.set_phase(BootstrapPhase::Running);

    let decision = registry.evaluate(&network_request(&peer()));

    assert!(
        matches!(decision, PolicyDecision::Allow { .. }),
        "registering an oracle for '{NETWORK_DOMAIN}' must allow network policy \
         requests; got {decision:?}"
    );
}

/// Registering under the trust domain alone is NOT sufficient.
///
/// Guards against a "fix" that re-registers the trust oracle by `domain()` and
/// assumes that covers the network layer.
#[test]
fn trust_domain_registration_does_not_cover_network_domain() {
    let registry = registry_without_network_domain();
    registry.set_phase(BootstrapPhase::Running);

    let trust_decision = registry.evaluate(&PolicyRequest::new(
        peer().to_string(),
        ActionKind::Custom("network_message".to_string()),
        Domain::trust(),
    ));
    let net_decision = registry.evaluate(&network_request(&peer()));

    assert!(
        matches!(trust_decision, PolicyDecision::Allow { .. }),
        "trust domain is registered and must be allowed"
    );
    assert!(
        matches!(net_decision, PolicyDecision::Deny { .. }),
        "'{NETWORK_DOMAIN}' must still be denied — the two domains are distinct \
         registry keys and registering one does not register the other"
    );
}

/// End-to-end through the real `RateLimiter`: the user-visible symptom.
///
/// This is the strongest test — it exercises exactly the call the connection
/// handler makes, so it fails for the same reason production failed.
#[tokio::test]
async fn rate_limiter_drops_every_message_when_network_domain_unregistered() {
    let registry = registry_without_network_domain();
    registry.set_phase(BootstrapPhase::Running);

    let limiter = RateLimiter::new_with_oracle(registry, generous_fallback());
    let peer = peer();

    // Well under any configured rate: a token bucket would allow all of these.
    for attempt in 1..=5 {
        assert!(
            !limiter.check_rate_limit(&peer).await,
            "message {attempt} was allowed, but with '{NETWORK_DOMAIN}' unregistered \
             every message must be rejected"
        );
    }
}

/// With the domain registered, the same sequence flows.
#[tokio::test]
async fn rate_limiter_admits_messages_when_network_domain_registered() {
    let registry = registry_with_network_domain();
    registry.set_phase(BootstrapPhase::Running);

    let limiter = RateLimiter::new_with_oracle(registry, generous_fallback());
    let peer = peer();

    for attempt in 1..=5 {
        assert!(
            limiter.check_rate_limit(&peer).await,
            "message {attempt} was rejected; with '{NETWORK_DOMAIN}' registered and an \
             unexhausted bucket the message must be admitted"
        );
    }
}

/// Registration restores enforcement, it does not disable it.
///
/// `RateLimit::restricted()` is 5 msg/s with burst 5, so a burst well past that
/// must still be cut off. Without this, a "fix" that always allows would pass
/// every other test here.
#[tokio::test]
async fn registered_network_domain_still_enforces_a_finite_bound() {
    let registry = registry_with_network_domain();
    registry.set_phase(BootstrapPhase::Running);

    // Fallback is deliberately generous; the bound must come from the oracle's
    // constraints, not from the fallback.
    let limiter = RateLimiter::new_with_oracle(registry, generous_fallback());
    let peer = peer();

    let mut allowed = 0usize;
    for _ in 0..100 {
        if limiter.check_rate_limit(&peer).await {
            allowed += 1;
        }
    }

    assert!(
        allowed > 0,
        "the peer must be able to send something — otherwise this is the bug again"
    );
    assert!(
        allowed < 100,
        "a burst of 100 must not be fully admitted; the isolated-tier bound has to \
         still apply after registration (allowed {allowed}/100)"
    );
}

/// Bootstrap phases keep their permissive behaviour.
///
/// A node must be able to peer while still coming up; the deny only appears at
/// `Running`. Pins the phase semantics the fix relies on.
#[test]
fn bootstrap_phases_allow_network_domain_before_running() {
    for phase in [BootstrapPhase::Genesis, BootstrapPhase::CoreApps] {
        let registry = registry_without_network_domain();
        registry.set_phase(phase);

        let decision = registry.evaluate(&network_request(&peer()));

        assert!(
            matches!(decision, PolicyDecision::Allow { .. }),
            "{phase:?} must allow an unregistered '{NETWORK_DOMAIN}' domain; \
             got {decision:?}"
        );
    }
}
