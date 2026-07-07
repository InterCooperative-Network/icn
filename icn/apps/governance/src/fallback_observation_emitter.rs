//! Governance-local emission groundwork for broad-fallback scope-admission
//! observability.
//!
//! Prior slices landed the domain-agnostic observation vocabulary in
//! `icn-http-kit` (#2347) and the governance-local labels/wrapper (#2349,
//! [`crate::fallback_observation`]). This module adds the *emission seam*: a
//! bounded, closed-static-label aggregate key ([`FallbackObservationLabels`])
//! that a future metric/audit sink may key on, plus an emitter trait
//! ([`FallbackObservationEmitter`]) with a safe no-op default
//! ([`NoopFallbackObservationEmitter`]).
//!
//! **Bounded / privacy-safe by construction.** Every label is a fixed static
//! string (control-map §4 wire value); the only way to build a
//! [`FallbackObservationLabels`] is from a closed-enum [`GovernanceScopeAdmission`],
//! so no identifier, token, DID, subject, actor, domain/entity/resource id,
//! route parameter, payload, IP, user agent, free-form label, or error string
//! can ever be carried.
//!
//! **Observe-only, unwired.** [`FallbackObservationEmitter::emit`] returns
//! nothing, so an emitter can never affect a request, response, or handler
//! outcome, and an emission failure is swallowed rather than propagated. Nothing
//! here is wired into request handling and no real metric, log, event, or
//! receipt is emitted — the only shipped emitter is the no-op. It is groundwork
//! a later, separately-reviewed slice can back with a real (bounded)
//! metrics/audit sink and wire to a single handler family.

use crate::fallback_observation::GovernanceScopeAdmission;
use icn_http_kit::{ObservationOutcome, ObservationSurface, ScopeAdmission};

/// The bounded, closed-label aggregate key for one governance fallback
/// observation — the exact static labels a metric/audit sink may key on.
///
/// Every field is a fixed static string from the control-map §4 vocabulary, so
/// the key's cardinality is the product of small closed sets — never a function
/// of traffic volume, caller population, or resources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FallbackObservationLabels {
    /// §4 `surface_kind` — `governance_http` / `gateway_alias` / `json_rpc`.
    pub surface: &'static str,
    /// §4 `route_family`.
    pub route_family: &'static str,
    /// §4 `required_class`.
    pub required_class: &'static str,
    /// §4 `match_outcome`.
    pub match_outcome: &'static str,
    /// §4 `observation_outcome`.
    pub observation_outcome: &'static str,
}

impl From<GovernanceScopeAdmission> for FallbackObservationLabels {
    /// Project a bounded governance observation into its closed static-label
    /// aggregate key. Surface is derived from the route family (single source of
    /// truth, via [`GovernanceScopeAdmission`]'s own projection).
    fn from(observation: GovernanceScopeAdmission) -> Self {
        Self {
            surface: surface_label(observation.route_family.surface()),
            route_family: observation.route_family.as_label(),
            required_class: observation.required_class.as_label(),
            match_outcome: match_outcome_label(observation.match_outcome),
            observation_outcome: observation_outcome_label(observation.observation_outcome),
        }
    }
}

/// Control-map §4 `surface_kind` label for a transport surface.
///
/// Mapped governance-side so `icn-http-kit` stays domain-agnostic (the
/// `governance_http` label is governance-flavored).
fn surface_label(surface: ObservationSurface) -> &'static str {
    match surface {
        ObservationSurface::Http => "governance_http",
        ObservationSurface::GatewayAlias => "gateway_alias",
        ObservationSurface::JsonRpc => "json_rpc",
    }
}

/// Control-map §4 `match_outcome` label.
fn match_outcome_label(outcome: ScopeAdmission) -> &'static str {
    match outcome {
        ScopeAdmission::Class => "class",
        ScopeAdmission::Fallback => "fallback",
        ScopeAdmission::ClassPreferred => "class_preferred",
        ScopeAdmission::RejectedSibling => "rejected_sibling",
        ScopeAdmission::RejectedUnrelated => "rejected_unrelated",
        ScopeAdmission::RejectedMissing => "rejected_missing",
    }
}

/// Control-map §4 `observation_outcome` label.
fn observation_outcome_label(outcome: ObservationOutcome) -> &'static str {
    match outcome {
        ObservationOutcome::Observed => "observed",
        ObservationOutcome::ObservationFailed => "observation_failed",
    }
}

/// A governance-local sink for bounded fallback-observation aggregates.
///
/// **Observe-only by contract:** [`emit`](Self::emit) returns nothing, so an
/// implementation cannot influence the request, response, or handler outcome,
/// and an emission failure must be swallowed (never surfaced as a request
/// error). It accepts only the closed-enum [`GovernanceScopeAdmission`] — never
/// a caller-supplied label string.
pub trait FallbackObservationEmitter: Send + Sync + 'static {
    /// Emit one bounded observation aggregate. Must not affect the request.
    fn emit(&self, observation: GovernanceScopeAdmission);
}

/// The default emitter: emits nothing.
///
/// The safe, always-available default (mirroring `NoopEventEmitter`). It cannot
/// fail, allocate, block, or affect any authorization or handler outcome. Until
/// a later slice ships a real bounded metric/audit emitter, this is the only
/// emitter.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopFallbackObservationEmitter;

impl FallbackObservationEmitter for NoopFallbackObservationEmitter {
    #[inline]
    fn emit(&self, _observation: GovernanceScopeAdmission) {
        // Intentionally does nothing: no metric, no log, no event, no receipt.
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::fallback_observation::{RequiredClass, RouteFamily};
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// A test-only emitter that records the projected bounded labels.
    #[derive(Default)]
    struct RecordingEmitter {
        seen: Mutex<Vec<FallbackObservationLabels>>,
    }
    impl FallbackObservationEmitter for RecordingEmitter {
        fn emit(&self, observation: GovernanceScopeAdmission) {
            self.seen
                .lock()
                .expect("poisoned")
                .push(FallbackObservationLabels::from(observation));
        }
    }

    /// A test-only emitter whose internal sink always "fails". The observe-only
    /// contract requires it to swallow the failure and return unit — never
    /// propagate — so a failing emitter cannot affect the caller.
    struct SwallowingEmitter {
        swallowed: AtomicUsize,
    }
    impl FallbackObservationEmitter for SwallowingEmitter {
        fn emit(&self, _observation: GovernanceScopeAdmission) {
            let sink: Result<(), &'static str> = Err("sink unavailable");
            if sink.is_err() {
                self.swallowed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn sample(
        route_family: RouteFamily,
        required_class: RequiredClass,
    ) -> GovernanceScopeAdmission {
        GovernanceScopeAdmission::new(
            route_family,
            required_class,
            ScopeAdmission::Fallback,
            ObservationOutcome::Observed,
        )
    }

    #[test]
    fn labels_are_bounded_static_control_map_values() {
        let labels = FallbackObservationLabels::from(sample(
            RouteFamily::ProposalDelegation,
            RequiredClass::Proposal,
        ));
        assert_eq!(labels.surface, "governance_http");
        assert_eq!(labels.route_family, "proposal_delegation");
        assert_eq!(labels.required_class, "proposal");
        assert_eq!(labels.match_outcome, "fallback");
        assert_eq!(labels.observation_outcome, "observed");
    }

    #[test]
    fn surface_label_is_total_and_bounded_across_all_families() {
        assert_eq!(
            FallbackObservationLabels::from(sample(
                RouteFamily::GatewayGovernanceAlias,
                RequiredClass::Proposal
            ))
            .surface,
            "gateway_alias"
        );
        assert_eq!(
            FallbackObservationLabels::from(sample(
                RouteFamily::JsonRpcGovernance,
                RequiredClass::Charter
            ))
            .surface,
            "json_rpc"
        );
        let mut surfaces = BTreeSet::new();
        for route_family in RouteFamily::ALL {
            surfaces.insert(
                FallbackObservationLabels::from(sample(route_family, RequiredClass::Charter))
                    .surface,
            );
        }
        assert_eq!(
            surfaces,
            BTreeSet::from(["governance_http", "gateway_alias", "json_rpc"])
        );
    }

    #[test]
    fn match_and_observation_outcome_labels_are_total_and_bounded() {
        let matches = [
            ScopeAdmission::Class,
            ScopeAdmission::Fallback,
            ScopeAdmission::ClassPreferred,
            ScopeAdmission::RejectedSibling,
            ScopeAdmission::RejectedUnrelated,
            ScopeAdmission::RejectedMissing,
        ];
        let mut match_labels = BTreeSet::new();
        for outcome in matches {
            match_labels.insert(match_outcome_label(outcome));
        }
        assert_eq!(
            match_labels,
            BTreeSet::from([
                "class",
                "fallback",
                "class_preferred",
                "rejected_sibling",
                "rejected_unrelated",
                "rejected_missing",
            ])
        );
        let mut obs_labels = BTreeSet::new();
        for outcome in [
            ObservationOutcome::Observed,
            ObservationOutcome::ObservationFailed,
        ] {
            obs_labels.insert(observation_outcome_label(outcome));
        }
        assert_eq!(
            obs_labels,
            BTreeSet::from(["observed", "observation_failed"])
        );
    }

    #[test]
    fn full_cross_product_projects_to_bounded_labels_only() {
        // Constructing the whole (family, class) product yields only static
        // labels — no arbitrary or private value is ever needed to build a key.
        let mut keys = BTreeSet::new();
        for route_family in RouteFamily::ALL {
            for required_class in RequiredClass::ALL {
                let l = FallbackObservationLabels::from(sample(route_family, required_class));
                keys.insert((
                    l.surface,
                    l.route_family,
                    l.required_class,
                    l.match_outcome,
                    l.observation_outcome,
                ));
            }
        }
        assert_eq!(
            keys.len(),
            RouteFamily::ALL.len() * RequiredClass::ALL.len()
        );
    }

    #[test]
    fn noop_emitter_records_nothing_and_returns_unit() {
        let emitter = NoopFallbackObservationEmitter;
        let unit: () = emitter.emit(sample(RouteFamily::CharterDomain, RequiredClass::Charter));
        assert_eq!(unit, ());
    }

    #[test]
    fn recording_emitter_captures_bounded_labels() {
        let emitter = RecordingEmitter::default();
        emitter.emit(sample(
            RouteFamily::MeetingsActionItems,
            RequiredClass::Meeting,
        ));
        let seen = emitter.seen.lock().expect("poisoned").clone();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].route_family, "meetings_action_items");
        assert_eq!(seen[0].required_class, "meeting");
        assert_eq!(seen[0].surface, "governance_http");
    }

    #[test]
    fn failing_emitter_swallows_failure_and_cannot_affect_the_caller() {
        let emitter = SwallowingEmitter {
            swallowed: AtomicUsize::new(0),
        };
        // A "request" proceeds normally regardless of the emitter's internal
        // sink failure: emit returns unit and propagates nothing.
        let unit: () = emitter.emit(sample(
            RouteFamily::ProcessRecording,
            RequiredClass::Process,
        ));
        assert_eq!(unit, ());
        // The failure was contained inside the emitter.
        assert_eq!(emitter.swallowed.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn emitter_usable_behind_a_trait_object() {
        let emitter: &dyn FallbackObservationEmitter = &NoopFallbackObservationEmitter;
        emitter.emit(sample(RouteFamily::StewardDirect, RequiredClass::Steward));
    }
}
