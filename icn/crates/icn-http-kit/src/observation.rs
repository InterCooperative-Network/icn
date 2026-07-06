//! Bounded, privacy-safe observation vocabulary for governance broad-fallback
//! scope admission.
//!
//! This is the "bounded in-memory / test observation sink" step (§9 slice 3) of
//! the governance broad-fallback observability control map
//! (`docs/design/governance-broad-fallback-observability.md`). It defines a
//! bounded record of *how* a class-first accepted-also scope gate was satisfied,
//! plus a side-band sink to receive it, so a later slice can measure broad-
//! fallback use. **Nothing here is wired into request handling** and no
//! production telemetry (logs, metrics, events, receipts) is emitted.
//!
//! **Observe-only.** [`ScopeAdmissionObserver::observe`] returns nothing, so an
//! implementation cannot influence an authorization decision, a response, or any
//! handler outcome.
//!
//! **Privacy budget.** A [`ScopeAdmissionObservation`] carries only closed enums
//! (control map §4/§5): no token contents, DID, subject, actor, domain, entity,
//! resource identifier, payload, request body, IP, user agent, free-form label,
//! or error string may ever be recorded here.
//!
//! **Domain-agnostic.** This crate holds "no domain knowledge", so the
//! governance-specific dimensions the control map also names — `route_family`
//! and `required_class` — are intentionally **not** part of this record. They
//! are governance vocabulary and belong to a later governance-local wrapper
//! slice that supplies them when it constructs an observation, just as
//! [`crate::auth::classify_scope_admission`] takes the sibling scope set as a
//! parameter rather than hardcoding it.

use crate::auth::ScopeAdmission;

/// The transport surface an admission was observed on.
///
/// Closed set (control map §4 `surface_kind`), named generically so this crate
/// stays domain-agnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationSurface {
    /// An application HTTP handler gate.
    Http,
    /// A gateway alias / compatibility surface.
    GatewayAlias,
    /// A JSON-RPC method mapping.
    JsonRpc,
}

/// Whether the side-band observation itself succeeded (control map §4
/// `observation_outcome`).
///
/// This is about the observation, never the request: an
/// [`ObservationFailed`](Self::ObservationFailed) still means the request was
/// authorized exactly as it would have been without any observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationOutcome {
    /// The admission was recorded.
    Observed,
    /// The observation could not be recorded; the request is unaffected.
    ObservationFailed,
}

/// A bounded, privacy-safe record of one scope-admission observation.
///
/// Every field is a closed enum, so the record's cardinality is the product of
/// small fixed sets — never a function of traffic volume, caller population, or
/// resources. It carries no identifier or payload of any kind (see the module
/// privacy budget). `match_outcome` reuses [`ScopeAdmission`] as the single
/// source of match-outcome vocabulary; this record introduces no alternative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeAdmissionObservation {
    /// Which surface produced the observation.
    pub surface: ObservationSurface,
    /// How the class-first gate was satisfied (or why it would be rejected).
    pub match_outcome: ScopeAdmission,
    /// Whether the observation itself was recorded.
    pub observation_outcome: ObservationOutcome,
}

impl ScopeAdmissionObservation {
    /// Construct a bounded observation. Every argument is a closed enum, so an
    /// observation can never carry an arbitrary or private value.
    pub fn new(
        surface: ObservationSurface,
        match_outcome: ScopeAdmission,
        observation_outcome: ObservationOutcome,
    ) -> Self {
        Self {
            surface,
            match_outcome,
            observation_outcome,
        }
    }
}

/// A side-band sink for scope-admission observations.
///
/// **Observe-only by contract:** [`observe`](Self::observe) returns nothing, so
/// an implementation cannot influence the authorization decision, the response,
/// or any handler outcome. Implementations must also not panic or block on the
/// request path; an observation that cannot be recorded is dropped (or reflected
/// as [`ObservationOutcome::ObservationFailed`]), never surfaced as a request
/// error.
pub trait ScopeAdmissionObserver: Send + Sync + 'static {
    /// Record one bounded observation. Must not affect the request.
    fn observe(&self, observation: ScopeAdmissionObservation);
}

/// The default observer: records nothing.
///
/// The safe, always-available default (mirroring `NoopEventEmitter` in the
/// governance app). It cannot fail, allocate, block, or affect any authorization
/// or handler outcome. Until a later slice wires a real sink, this is the only
/// production observer.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopScopeAdmissionObserver;

impl ScopeAdmissionObserver for NoopScopeAdmissionObserver {
    #[inline]
    fn observe(&self, _observation: ScopeAdmissionObservation) {
        // Intentionally does nothing: no record, no I/O, no side effect.
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A test-only in-memory sink, proving the privacy-budget shape and capture.
    /// It exists only under `cfg(test)` — nothing production-visible records.
    #[derive(Default)]
    struct InMemoryObserver {
        records: std::sync::Mutex<Vec<ScopeAdmissionObservation>>,
    }

    impl ScopeAdmissionObserver for InMemoryObserver {
        fn observe(&self, observation: ScopeAdmissionObservation) {
            self.records.lock().expect("poisoned").push(observation);
        }
    }

    const SURFACES: [ObservationSurface; 3] = [
        ObservationSurface::Http,
        ObservationSurface::GatewayAlias,
        ObservationSurface::JsonRpc,
    ];
    const MATCHES: [ScopeAdmission; 6] = [
        ScopeAdmission::Class,
        ScopeAdmission::Fallback,
        ScopeAdmission::ClassPreferred,
        ScopeAdmission::RejectedSibling,
        ScopeAdmission::RejectedUnrelated,
        ScopeAdmission::RejectedMissing,
    ];
    const OBS_OUTCOMES: [ObservationOutcome; 2] = [
        ObservationOutcome::Observed,
        ObservationOutcome::ObservationFailed,
    ];

    #[test]
    fn observation_is_built_only_from_bounded_closed_enums() {
        // Every combination is constructible with no arbitrary or private input;
        // the record round-trips its closed-enum fields and nothing else.
        for surface in SURFACES {
            for match_outcome in MATCHES {
                for observation_outcome in OBS_OUTCOMES {
                    let obs =
                        ScopeAdmissionObservation::new(surface, match_outcome, observation_outcome);
                    assert_eq!(obs.surface, surface);
                    assert_eq!(obs.match_outcome, match_outcome);
                    assert_eq!(obs.observation_outcome, observation_outcome);
                }
            }
        }
    }

    #[test]
    fn scope_admission_is_the_only_match_outcome_vocabulary() {
        // The record's match_outcome field is exactly `ScopeAdmission`; there is
        // no alternative match-outcome type.
        let obs = ScopeAdmissionObservation::new(
            ObservationSurface::Http,
            ScopeAdmission::ClassPreferred,
            ObservationOutcome::Observed,
        );
        let outcome: ScopeAdmission = obs.match_outcome;
        assert!(outcome.is_accepted());
    }

    #[test]
    fn noop_observer_records_nothing_and_returns_unit() {
        // The no-op observer has no observable effect: `observe` returns `()`,
        // does not panic, and holds no state to leak.
        let observer = NoopScopeAdmissionObserver;
        let obs = ScopeAdmissionObservation::new(
            ObservationSurface::GatewayAlias,
            ScopeAdmission::Fallback,
            ObservationOutcome::Observed,
        );
        let unit: () = observer.observe(obs);
        observer.observe(obs);
        assert_eq!(unit, ());
    }

    #[test]
    fn test_sink_captures_exactly_what_was_observed() {
        let sink = InMemoryObserver::default();
        let a = ScopeAdmissionObservation::new(
            ObservationSurface::GatewayAlias,
            ScopeAdmission::Class,
            ObservationOutcome::Observed,
        );
        let b = ScopeAdmissionObservation::new(
            ObservationSurface::JsonRpc,
            ScopeAdmission::RejectedMissing,
            ObservationOutcome::ObservationFailed,
        );
        sink.observe(a);
        sink.observe(b);
        let recorded = sink.records.lock().expect("poisoned").clone();
        assert_eq!(recorded, vec![a, b]);
    }

    #[test]
    fn observer_is_usable_behind_a_trait_object() {
        // Confirms the sink is a dyn-compatible seam a later slice can inject,
        // defaulting to the no-op observer with no behavioral effect.
        let observer: &dyn ScopeAdmissionObserver = &NoopScopeAdmissionObserver;
        observer.observe(ScopeAdmissionObservation::new(
            ObservationSurface::JsonRpc,
            ScopeAdmission::RejectedUnrelated,
            ObservationOutcome::ObservationFailed,
        ));
    }
}
