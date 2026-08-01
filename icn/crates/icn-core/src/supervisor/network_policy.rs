//! Operator-configured ceiling on network rate-limit constraints.
//!
//! # Why this exists
//!
//! `icn-net`'s rate limiter is keyed on `NetworkMessage.from`, which is
//! **self-asserted and not yet authenticated** when the check runs — the Hello
//! binding proof and `SignedEnvelope` verification both happen after dispatch
//! (`icn-net/src/actor/connection.rs`). DIDs are public, so a sender can name any
//! DID it likes and receive whatever tier that DID's trust score maps to.
//!
//! On its own that is a pre-existing weakness bounded by whatever the tier
//! allows. It stops being bounded when the tier is `RateLimit::unlimited()`,
//! which is `u32::MAX` for both rate and burst: a sender that claims a
//! well-trusted DID gets no effective limit at all, and can force repeated
//! deserialization, binding checks, and signature verification for free.
//!
//! [`CappedRateLimitOracle`] closes that by clamping whatever the inner oracle
//! returns to the operator's configured maximum. It is a numeric ceiling and
//! nothing more: it does not read trust scores, infer a tier, or reconstruct any
//! domain meaning from the `ConstraintSet` — doing so inside a kernel crate would
//! violate the meaning firewall. It only enforces "no peer, however it is
//! classified, exceeds what the operator configured."
//!
//! The deeper fix — applying trust-derived tiers only after the connection is
//! bound to a verified DID — is a protocol change tracked separately.

use std::sync::Arc;

use icn_kernel_api::authz::{
    ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest, RateLimit,
};

/// Wraps a `PolicyOracle` and caps any rate-limit constraint it returns.
pub struct CappedRateLimitOracle {
    inner: Arc<dyn PolicyOracle>,
    domain: Domain,
    max_messages_per_second: u32,
    max_burst: u32,
}

// Hand-written: `Arc<dyn PolicyOracle>` is not `Debug`, and the trait does not
// require it.
impl std::fmt::Debug for CappedRateLimitOracle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CappedRateLimitOracle")
            .field("domain", &self.domain)
            .field("max_messages_per_second", &self.max_messages_per_second)
            .field("max_burst", &self.max_burst)
            .finish_non_exhaustive()
    }
}

impl CappedRateLimitOracle {
    /// Serve `domain` from `inner`, clamping rate limits to the given ceiling.
    pub fn new(
        inner: Arc<dyn PolicyOracle>,
        domain: Domain,
        max_messages_per_second: u32,
        max_burst: u32,
    ) -> Self {
        Self {
            inner,
            domain,
            max_messages_per_second,
            max_burst,
        }
    }

    fn cap(&self, rate_limit: &RateLimit) -> RateLimit {
        RateLimit {
            messages_per_second: rate_limit
                .messages_per_second
                .min(self.max_messages_per_second),
            burst_size: rate_limit.burst_size.min(self.max_burst),
        }
    }
}

impl PolicyOracle for CappedRateLimitOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        match self.inner.evaluate(request) {
            PolicyDecision::Allow { constraints, .. } => {
                // A decision with no rate-limit constraint is left alone: the
                // caller falls back to its configured default, which is already
                // operator-controlled.
                let capped = match &constraints.rate_limit {
                    Some(rate_limit) => {
                        let mut next = ConstraintSet::clone(&constraints);
                        next.rate_limit = Some(self.cap(rate_limit));
                        next
                    }
                    None => constraints,
                };
                PolicyDecision::allow_with(capped)
            }
            // Denials pass through untouched — capping must never turn a deny
            // into an allow.
            deny => deny,
        }
    }

    fn domain(&self) -> Domain {
        self.domain.clone()
    }

    fn cache_ttl(&self) -> std::time::Duration {
        self.inner.cache_ttl()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::authz::ActionKind;

    #[derive(Debug)]
    struct FixedOracle(Option<RateLimit>);

    impl PolicyOracle for FixedOracle {
        fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
            match &self.0 {
                Some(rate_limit) => PolicyDecision::allow_with(
                    ConstraintSet::new().with_rate_limit(rate_limit.clone()),
                ),
                None => PolicyDecision::allow_with(ConstraintSet::new()),
            }
        }

        fn domain(&self) -> Domain {
            Domain::trust()
        }
    }

    #[derive(Debug)]
    struct DenyOracle;

    impl PolicyOracle for DenyOracle {
        fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
            PolicyDecision::deny("nope".to_string())
        }

        fn domain(&self) -> Domain {
            Domain::trust()
        }
    }

    fn request() -> PolicyRequest {
        PolicyRequest::new(
            "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9".to_string(),
            ActionKind::Custom("network_message".to_string()),
            Domain::new("net"),
        )
    }

    fn rate_limit_of(decision: &PolicyDecision) -> Option<RateLimit> {
        match decision {
            PolicyDecision::Allow { constraints, .. } => constraints.rate_limit.clone(),
            PolicyDecision::Deny { .. } => None,
        }
    }

    /// The security case: an unlimited tier must not survive the cap.
    ///
    /// `RateLimit::unlimited()` is `u32::MAX`, and the DID it was derived from is
    /// unauthenticated at check time, so without this an attacker naming a
    /// well-trusted DID gets no bound at all.
    #[test]
    fn unlimited_is_capped_to_the_configured_ceiling() {
        let oracle = CappedRateLimitOracle::new(
            Arc::new(FixedOracle(Some(RateLimit::unlimited()))),
            Domain::new("net"),
            200,
            50,
        );

        let limit = rate_limit_of(&oracle.evaluate(&request())).expect("rate limit present");

        assert_eq!(
            (limit.messages_per_second, limit.burst_size),
            (200, 50),
            "an unlimited tier must be clamped to the operator's configured maximum"
        );
    }

    /// Limits already below the ceiling are untouched — the cap is a ceiling, not
    /// an override that would silently *raise* a restrictive tier.
    #[test]
    fn limits_below_the_ceiling_are_left_alone() {
        let oracle = CappedRateLimitOracle::new(
            Arc::new(FixedOracle(Some(RateLimit::restricted()))),
            Domain::new("net"),
            200,
            50,
        );

        let limit = rate_limit_of(&oracle.evaluate(&request())).expect("rate limit present");

        assert_eq!(
            (limit.messages_per_second, limit.burst_size),
            (5, 5),
            "restricted() is already under the ceiling and must pass through unchanged"
        );
    }

    /// Each field is capped independently.
    #[test]
    fn each_field_is_capped_independently() {
        let oracle = CappedRateLimitOracle::new(
            Arc::new(FixedOracle(Some(RateLimit {
                messages_per_second: 10,
                burst_size: u32::MAX,
            }))),
            Domain::new("net"),
            200,
            50,
        );

        let limit = rate_limit_of(&oracle.evaluate(&request())).expect("rate limit present");

        assert_eq!(
            (limit.messages_per_second, limit.burst_size),
            (10, 50),
            "a low rate with an unbounded burst must keep the rate and cap the burst"
        );
    }

    /// Capping must never convert a denial into an allow.
    #[test]
    fn denials_pass_through_untouched() {
        let oracle = CappedRateLimitOracle::new(Arc::new(DenyOracle), Domain::new("net"), 200, 50);

        assert!(
            matches!(oracle.evaluate(&request()), PolicyDecision::Deny { .. }),
            "a deny from the inner oracle must remain a deny"
        );
    }

    /// No rate-limit constraint means the caller's configured fallback applies;
    /// the wrapper must not invent one.
    #[test]
    fn absent_rate_limit_is_not_fabricated() {
        let oracle =
            CappedRateLimitOracle::new(Arc::new(FixedOracle(None)), Domain::new("net"), 200, 50);

        assert!(
            rate_limit_of(&oracle.evaluate(&request())).is_none(),
            "the wrapper must not synthesise a rate limit where the inner oracle gave none"
        );
    }

    /// The wrapper reports the domain it was registered for, not the inner
    /// oracle's — that mismatch is what #2488 was about.
    #[test]
    fn reports_the_domain_it_serves_not_the_inner_domain() {
        let oracle =
            CappedRateLimitOracle::new(Arc::new(FixedOracle(None)), Domain::new("net"), 200, 50);

        assert_eq!(oracle.domain(), Domain::new("net"));
        assert_ne!(oracle.domain(), Domain::trust());
    }
}
