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
//! # What this does
//!
//! [`NetworkRateLimitOracle`] serves the network domain by selecting the
//! operator-configured limit for the peer's trust class, then clamping it to an
//! absolute ceiling.
//!
//! ## Honouring every configured tier (#2496)
//!
//! Operators configure four tiers — `rate_limiting.{isolated,known,partner,federated}`.
//! Routing `net` straight at the trust oracle ignored all of them: that oracle
//! emits its own hard-coded 5/20/100/`unlimited()` ladder, so a configured
//! `isolated` burst of 2 still produced 5. Passing only the `federated` values as
//! a ceiling fixed the top of the range and left the rest untouched.
//!
//! The selection happens **here**, at the composition root, because this is the
//! only layer that holds both halves: the neutral [`TrustClass`] classification
//! (via [`TrustService`], an `icn-kernel-api` trait) and the operator's
//! configuration. The flow is
//!
//! ```text
//! domain trust score -> neutral TrustClass -> operator-configured tier -> ceiling
//! ```
//!
//! It never inspects the inner oracle's numbers to work out which tier they came
//! from. Reconstructing "5 msg/s means Isolated" from a generic `ConstraintSet`
//! would be exactly the meaning-firewall violation the kernel/app split exists to
//! prevent; the class comes from the trust service, not from arithmetic.
//!
//! ## The absolute ceiling
//!
//! Tier selection does not replace the ceiling — both apply. The ceiling is
//! defence in depth for #2491: since the tier is chosen from an unauthenticated
//! DID, a misconfigured or overly generous tier must not become an unbounded
//! pre-authentication budget. #2491 itself (binding the tier to a *verified*
//! identity) is a protocol change tracked separately and deliberately not solved
//! here.
//!
//! ## Cost
//!
//! Selecting the tier costs one `TrustService::trust_score` call per *allowed*
//! message, on top of the one the inner oracle already makes. That is the price
//! of not inferring the class from the constraint values; denials short-circuit
//! before it.

use std::sync::Arc;

use icn_kernel_api::authz::{
    ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest, RateLimit,
};
use icn_kernel_api::services::{TrustClass, TrustService};

/// The operator-configured network rate limit for each trust class.
///
/// Mirrors `rate_limiting.{isolated,known,partner,federated}` from the daemon
/// config. Held as a struct rather than a map so adding a [`TrustClass`] variant
/// is a compile error here rather than a silently missing tier.
#[derive(Debug, Clone)]
pub struct NetworkRateLimitTiers {
    /// Limit for peers classified [`TrustClass::Isolated`].
    pub isolated: RateLimit,
    /// Limit for peers classified [`TrustClass::Known`].
    pub known: RateLimit,
    /// Limit for peers classified [`TrustClass::Partner`].
    pub partner: RateLimit,
    /// Limit for peers classified [`TrustClass::Federated`].
    pub federated: RateLimit,
}

impl NetworkRateLimitTiers {
    /// The absolute ceiling implied by this configuration.
    ///
    /// The maximum over all four tiers — deliberately *not* the federated tier.
    ///
    /// Using `federated` as a global ceiling silently clamped any tier an operator
    /// configured above it, which is a legitimate shape: a burstier local
    /// `partner` tier alongside a tighter WAN `federated` tier. Config validation
    /// imposes no monotonic ordering, so such a configuration is accepted and was
    /// then quietly overridden.
    ///
    /// Taking the maximum keeps the ceiling's actual purpose — bounding anything
    /// that is *not* an operator-configured tier, so no trust-derived value can
    /// exceed what the operator allowed — while guaranteeing it can never clamp a
    /// configured tier.
    pub fn ceiling(&self) -> RateLimit {
        let tiers = [&self.isolated, &self.known, &self.partner, &self.federated];
        RateLimit {
            messages_per_second: tiers
                .iter()
                .map(|t| t.messages_per_second)
                .max()
                .unwrap_or(0),
            burst_size: tiers.iter().map(|t| t.burst_size).max().unwrap_or(0),
        }
    }

    /// The configured limit for a trust class.
    pub fn for_class(&self, class: TrustClass) -> &RateLimit {
        match class {
            TrustClass::Isolated => &self.isolated,
            TrustClass::Known => &self.known,
            TrustClass::Partner => &self.partner,
            TrustClass::Federated => &self.federated,
        }
    }
}

/// Read one configured tier as a kernel [`RateLimit`].
///
/// The config type is the daemon's; the kernel only ever sees the two numbers.
pub fn rate_limit_from_config(config: &crate::config::TrustClassRateLimitConfig) -> RateLimit {
    RateLimit {
        messages_per_second: config.max_messages_per_second,
        burst_size: config.burst_capacity,
    }
}

impl NetworkRateLimitTiers {
    /// Build the tier table from the daemon's `[rate_limiting]` section.
    ///
    /// This is the whole of the operator's half of the mapping. The trust half
    /// arrives separately, as a [`TrustClass`]; the two meet in
    /// [`NetworkRateLimitOracle::configured_limit_for`] and nowhere else.
    pub fn from_config(config: &crate::config::RateLimitingConfig) -> Self {
        Self {
            isolated: rate_limit_from_config(&config.isolated),
            known: rate_limit_from_config(&config.known),
            partner: rate_limit_from_config(&config.partner),
            federated: rate_limit_from_config(&config.federated),
        }
    }
}

/// Serves the network domain: selects the operator-configured tier for a peer's
/// trust class, then clamps it to an absolute ceiling.
pub struct NetworkRateLimitOracle {
    inner: Arc<dyn PolicyOracle>,
    trust: Arc<dyn TrustService>,
    domain: Domain,
    tiers: NetworkRateLimitTiers,
    ceiling: RateLimit,
}

// Hand-written: `Arc<dyn PolicyOracle>` is not `Debug`, and the trait does not
// require it.
impl std::fmt::Debug for NetworkRateLimitOracle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkRateLimitOracle")
            .field("domain", &self.domain)
            .field("tiers", &self.tiers)
            .field("ceiling", &self.ceiling)
            .finish_non_exhaustive()
    }
}

impl NetworkRateLimitOracle {
    /// Serve `domain` from `inner`, replacing its rate limit with the configured
    /// tier for the peer's trust class and clamping that to `ceiling`.
    pub fn new(
        inner: Arc<dyn PolicyOracle>,
        trust: Arc<dyn TrustService>,
        domain: Domain,
        tiers: NetworkRateLimitTiers,
        ceiling: RateLimit,
    ) -> Self {
        Self {
            inner,
            trust,
            domain,
            tiers,
            ceiling,
        }
    }

    /// The configured limit for `actor`, clamped to the absolute ceiling.
    ///
    /// The class comes from the trust service and is mapped through the neutral
    /// [`TrustClass`]; it is never derived from the inner oracle's numbers.
    fn configured_limit_for(&self, actor: &icn_kernel_api::types::Did) -> RateLimit {
        let class = TrustClass::from_score(self.trust.trust_score(actor));
        self.cap(self.tiers.for_class(class))
    }

    fn cap(&self, rate_limit: &RateLimit) -> RateLimit {
        RateLimit {
            messages_per_second: rate_limit
                .messages_per_second
                .min(self.ceiling.messages_per_second),
            burst_size: rate_limit.burst_size.min(self.ceiling.burst_size),
        }
    }
}

impl PolicyOracle for NetworkRateLimitOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        match self.inner.evaluate(request) {
            PolicyDecision::Allow { constraints, .. } => {
                // A decision with no rate-limit constraint is left alone: the
                // caller falls back to its configured default, which is already
                // operator-controlled. Synthesising one here would override that
                // fallback for oracles that deliberately abstain.
                let resolved = match &constraints.rate_limit {
                    Some(_) => {
                        // The inner oracle's own number is *replaced*, not
                        // clamped. Clamping alone was #2496: it bounded the top
                        // of the range and let the hard-coded ladder through
                        // everywhere below it.
                        let mut next = ConstraintSet::clone(&constraints);
                        next.rate_limit = Some(self.configured_limit_for(&request.core.actor));
                        next
                    }
                    None => constraints,
                };
                PolicyDecision::allow_with(resolved)
            }
            // Denials pass through untouched — neither tier selection nor
            // capping may turn a deny into an allow.
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

    /// Stand-in for the trust service: reports a fixed score for any actor.
    struct FixedScoreTrust(f64);

    impl icn_kernel_api::services::TrustService for FixedScoreTrust {
        fn oracle(&self) -> Arc<dyn PolicyOracle> {
            Arc::new(FixedOracle(None))
        }

        fn trust_score(&self, _actor: &icn_kernel_api::types::Did) -> f64 {
            self.0
        }

        fn record_event(
            &self,
            _actor: &icn_kernel_api::types::Did,
            _event: icn_kernel_api::services::TrustEvent,
        ) {
        }
    }

    /// The four configured tiers, deliberately distinct from the trust oracle's
    /// hard-coded 5/20/100/unlimited so a passing test cannot be an accident.
    ///
    /// Every rate here is a whole multiple of the token bucket's refill
    /// granularity (>= 1 / `refill_interval`, i.e. >= 10/s at the default 100 ms).
    /// Sub-granularity rates are deliberately avoided: `RateLimiter` computes
    /// `(rate * interval).max(1.0)` (`icn-net/src/rate_limit.rs:394`), so a
    /// configured 1/s would be *quantised up* to 10/s by the consumer. Asserting
    /// on such a value here would imply this layer can deliver a rate the limiter
    /// cannot honour. See the floor test in
    /// `tests/network_domain_oracle_registration.rs` and the tracking issue.
    fn configured_tiers() -> NetworkRateLimitTiers {
        NetworkRateLimitTiers {
            isolated: RateLimit {
                messages_per_second: 10,
                burst_size: 2,
            },
            known: RateLimit {
                messages_per_second: 30,
                burst_size: 9,
            },
            partner: RateLimit {
                messages_per_second: 70,
                burst_size: 13,
            },
            federated: RateLimit {
                messages_per_second: 170,
                burst_size: 19,
            },
        }
    }

    fn tiered_oracle(score: f64) -> NetworkRateLimitOracle {
        NetworkRateLimitOracle::new(
            // The inner oracle offers `unlimited()` for every actor, exactly as
            // the real trust oracle does at score >= 0.7. If the tier table is
            // ignored, that value is what comes out.
            Arc::new(FixedOracle(Some(RateLimit::unlimited()))),
            Arc::new(FixedScoreTrust(score)),
            Domain::new("net"),
            configured_tiers(),
            RateLimit {
                messages_per_second: 200,
                burst_size: 50,
            },
        )
    }

    /// Every configured tier is honoured, not just the federated one.
    ///
    /// This is #2496. The previous wiring passed only `rate_limiting.federated`
    /// as a ceiling and let the trust oracle's hard-coded 5/20/100/unlimited
    /// through underneath it, so lowering `isolated` or `partner` did nothing.
    #[test]
    fn each_configured_tier_is_honoured() {
        // (trust score, expected rate, expected burst) — one per class boundary.
        let cases = [
            (0.0, 10, 2),   // Isolated
            (0.05, 10, 2),  // Isolated, just below the Known threshold
            (0.1, 30, 9),   // Known, exactly at the threshold
            (0.39, 30, 9),  // Known
            (0.4, 70, 13),  // Partner, exactly at the threshold
            (0.69, 70, 13), // Partner
            (0.7, 170, 19), // Federated, exactly at the threshold
            (1.0, 170, 19), // Federated
        ];

        for (score, expected_rate, expected_burst) in cases {
            let limit = rate_limit_of(&tiered_oracle(score).evaluate(&request()))
                .expect("rate limit present");

            assert_eq!(
                (limit.messages_per_second, limit.burst_size),
                (expected_rate, expected_burst),
                "score {score} must select its operator-configured tier"
            );
        }
    }

    /// The exact regression from the #2496 review: a configured isolated burst of
    /// 2 must produce an effective burst of 2, not the trust oracle's hard-coded 5.
    #[test]
    fn configured_isolated_burst_of_two_is_not_widened_to_five() {
        let limit =
            rate_limit_of(&tiered_oracle(0.0).evaluate(&request())).expect("rate limit present");

        assert_eq!(
            limit.burst_size, 2,
            "configured isolated burst = 2 must produce effective burst = 2; a 5 \
             here means the trust oracle's `RateLimit::restricted()` leaked through"
        );
        assert_ne!(
            limit.burst_size, 5,
            "5 is `RateLimit::restricted()`'s burst - the value this bug shipped"
        );
    }

    /// A tier configured above the federated tier is not clamped.
    ///
    /// Regression for the review finding that used `rate_limiting.federated` as a
    /// global ceiling. A burstier local `partner` tier alongside a tighter WAN
    /// `federated` tier is a legitimate configuration, config validation imposes
    /// no monotonic ordering, and the ceiling silently overrode it.
    #[test]
    fn a_tier_configured_above_the_federated_tier_is_not_clamped() {
        let tiers = NetworkRateLimitTiers {
            // Partner is deliberately burstier and faster than federated.
            partner: RateLimit {
                messages_per_second: 500,
                burst_size: 90,
            },
            federated: RateLimit {
                messages_per_second: 200,
                burst_size: 50,
            },
            ..configured_tiers()
        };
        let ceiling = tiers.ceiling();

        let oracle = NetworkRateLimitOracle::new(
            Arc::new(FixedOracle(Some(RateLimit::unlimited()))),
            Arc::new(FixedScoreTrust(0.5)), // Partner
            Domain::new("net"),
            tiers,
            ceiling,
        );

        let limit = rate_limit_of(&oracle.evaluate(&request())).expect("rate limit present");

        assert_eq!(
            (limit.messages_per_second, limit.burst_size),
            (500, 90),
            "a partner tier configured above the federated tier must be honoured, \
             not clamped down to the federated values"
        );
    }

    /// The derived ceiling is the maximum across all tiers.
    #[test]
    fn the_ceiling_is_the_maximum_across_all_tiers() {
        let tiers = NetworkRateLimitTiers {
            isolated: RateLimit {
                messages_per_second: 10,
                burst_size: 99,
            },
            known: RateLimit {
                messages_per_second: 30,
                burst_size: 9,
            },
            partner: RateLimit {
                messages_per_second: 700,
                burst_size: 13,
            },
            federated: RateLimit {
                messages_per_second: 200,
                burst_size: 50,
            },
        };

        let ceiling = tiers.ceiling();

        assert_eq!(
            (ceiling.messages_per_second, ceiling.burst_size),
            (700, 99),
            "each field takes the maximum independently, so no configured tier can \
             be clamped in either dimension"
        );
    }

    /// The absolute ceiling still binds, as defence in depth for #2491.
    ///
    /// The rate limiter keys on an unauthenticated `NetworkMessage.from`, so a
    /// misconfigured tier must not become an unbounded pre-auth budget.
    #[test]
    fn a_tier_above_the_absolute_ceiling_is_still_clamped() {
        let oracle = NetworkRateLimitOracle::new(
            Arc::new(FixedOracle(Some(RateLimit::unlimited()))),
            Arc::new(FixedScoreTrust(1.0)),
            Domain::new("net"),
            NetworkRateLimitTiers {
                federated: RateLimit::unlimited(),
                ..configured_tiers()
            },
            RateLimit {
                messages_per_second: 200,
                burst_size: 50,
            },
        );

        let limit = rate_limit_of(&oracle.evaluate(&request())).expect("rate limit present");

        assert_eq!(
            (limit.messages_per_second, limit.burst_size),
            (200, 50),
            "no configured tier may exceed the operator's absolute ceiling"
        );
    }

    /// The inner oracle's own numbers never reach the caller.
    ///
    /// This is the difference between #2490's first cut and the fix. Clamping the
    /// inner value meant its hard-coded ladder still decided everything below the
    /// ceiling; replacing it means the operator's table decides. The inner oracle
    /// here says `unlimited()` for every actor and none of it survives.
    #[test]
    fn the_inner_oracles_own_numbers_never_reach_the_caller() {
        for score in [0.0, 0.2, 0.5, 0.9] {
            let limit = rate_limit_of(&tiered_oracle(score).evaluate(&request()))
                .expect("rate limit present");

            assert_ne!(
                limit.messages_per_second,
                u32::MAX,
                "the inner oracle's unlimited() must never survive"
            );
            assert!(
                [10, 30, 70, 170].contains(&limit.messages_per_second),
                "the effective limit must come from the configured tier table, \
                 got {} msg/s",
                limit.messages_per_second
            );
        }
    }

    /// The ceiling clamps each field independently.
    #[test]
    fn the_ceiling_clamps_each_field_independently() {
        let oracle = NetworkRateLimitOracle::new(
            Arc::new(FixedOracle(Some(RateLimit::unlimited()))),
            Arc::new(FixedScoreTrust(1.0)),
            Domain::new("net"),
            NetworkRateLimitTiers {
                federated: RateLimit {
                    messages_per_second: 10,
                    burst_size: u32::MAX,
                },
                ..configured_tiers()
            },
            RateLimit {
                messages_per_second: 200,
                burst_size: 50,
            },
        );

        let limit = rate_limit_of(&oracle.evaluate(&request())).expect("rate limit present");

        assert_eq!(
            (limit.messages_per_second, limit.burst_size),
            (10, 50),
            "a configured tier with a low rate and an unbounded burst must keep \
             the rate and clamp only the burst"
        );
    }

    /// Neither tier selection nor capping may convert a denial into an allow.
    #[test]
    fn denials_pass_through_untouched() {
        let oracle = NetworkRateLimitOracle::new(
            Arc::new(DenyOracle),
            Arc::new(FixedScoreTrust(1.0)),
            Domain::new("net"),
            configured_tiers(),
            RateLimit {
                messages_per_second: 200,
                burst_size: 50,
            },
        );

        assert!(
            matches!(oracle.evaluate(&request()), PolicyDecision::Deny { .. }),
            "a deny from the inner oracle must remain a deny"
        );
    }

    /// No rate-limit constraint means the caller's configured fallback applies;
    /// the wrapper must not invent one.
    ///
    /// An oracle that deliberately abstains is not the same as one that grants a
    /// tier, and `icn-net`'s fallback is itself operator-configured.
    #[test]
    fn absent_rate_limit_is_not_fabricated() {
        let oracle = NetworkRateLimitOracle::new(
            Arc::new(FixedOracle(None)),
            Arc::new(FixedScoreTrust(1.0)),
            Domain::new("net"),
            configured_tiers(),
            RateLimit {
                messages_per_second: 200,
                burst_size: 50,
            },
        );

        assert!(
            rate_limit_of(&oracle.evaluate(&request())).is_none(),
            "the wrapper must not synthesise a rate limit where the inner oracle gave none"
        );
    }

    /// The wrapper reports the domain it was registered for, not the inner
    /// oracle's — that mismatch is what #2488 was about.
    #[test]
    fn reports_the_domain_it_serves_not_the_inner_domain() {
        let oracle = NetworkRateLimitOracle::new(
            Arc::new(FixedOracle(None)),
            Arc::new(FixedScoreTrust(1.0)),
            Domain::new("net"),
            configured_tiers(),
            RateLimit {
                messages_per_second: 200,
                burst_size: 50,
            },
        );

        assert_eq!(oracle.domain(), Domain::new("net"));
        assert_ne!(oracle.domain(), Domain::trust());
    }
}
