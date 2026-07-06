//! Governance-local bounded label vocabulary for broad-fallback scope-admission
//! observability.
//!
//! `icn-http-kit` holds the domain-agnostic observation vocabulary
//! (`ScopeAdmissionObservation`, `ObservationSurface`, `ObservationOutcome`,
//! `ScopeAdmission`). This module adds the governance-specific dimensions the
//! control map (`docs/design/governance-broad-fallback-observability.md` §4)
//! intentionally deferred from that crate: the mutation [`RouteFamily`] and the
//! [`RequiredClass`] scope. Keeping them here preserves the meaning firewall —
//! governance vocabulary stays in the governance app, not the shared HTTP kit.
//!
//! **Bounded / privacy-safe.** Every type here is a closed enum or a record of
//! closed enums. No identifier, token, DID, subject, domain, entity, resource
//! id, route parameter, payload, or free-form label is ever carried. The
//! `as_label` helpers return fixed static strings (the control-map §4 wire
//! values), never caller-supplied or high-cardinality values.
//!
//! **Observe-only, unwired.** Nothing here is wired into request handling and no
//! sink, telemetry, log, metric, event, or receipt is emitted. It is bounded
//! vocabulary a later slice can populate and (separately) emit.

use icn_http_kit::{
    ObservationOutcome, ObservationSurface, ScopeAdmission, ScopeAdmissionObservation,
};

/// The governance mutation family a scope-admission observation belongs to
/// (control map §4 `route_family`). Closed set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteFamily {
    /// Domain/charter mutations (create_domain, add/remove member, activate,
    /// adopt policy, declare institutional domain).
    CharterDomain,
    /// Proposal lifecycle, voting, and delegation.
    ProposalDelegation,
    /// Federation proposal helpers gated through the shared helper.
    FederationProposalHelper,
    /// Direct steward role assignment.
    StewardDirect,
    /// Discussion comments and reactions.
    CommentsReactions,
    /// Meetings, attendance, agenda, and action items.
    MeetingsActionItems,
    /// Activities, programs, structures, and milestones.
    ActivitiesProgramsStructuresMilestones,
    /// Process-recording (session/gate/deliberation/decision facts).
    ProcessRecording,
    /// Gateway governance alias / compatibility surfaces.
    GatewayGovernanceAlias,
    /// JSON-RPC governance method mappings.
    JsonRpcGovernance,
}

impl RouteFamily {
    /// Every family, in control-map order. Bounded and total.
    pub const ALL: [RouteFamily; 10] = [
        RouteFamily::CharterDomain,
        RouteFamily::ProposalDelegation,
        RouteFamily::FederationProposalHelper,
        RouteFamily::StewardDirect,
        RouteFamily::CommentsReactions,
        RouteFamily::MeetingsActionItems,
        RouteFamily::ActivitiesProgramsStructuresMilestones,
        RouteFamily::ProcessRecording,
        RouteFamily::GatewayGovernanceAlias,
        RouteFamily::JsonRpcGovernance,
    ];

    /// The stable, bounded label for this family (control-map §4 wire value).
    ///
    /// A fixed static string — never a caller-supplied or high-cardinality value.
    pub fn as_label(self) -> &'static str {
        match self {
            RouteFamily::CharterDomain => "charter_domain",
            RouteFamily::ProposalDelegation => "proposal_delegation",
            RouteFamily::FederationProposalHelper => "federation_proposal_helper",
            RouteFamily::StewardDirect => "steward_direct",
            RouteFamily::CommentsReactions => "comments_reactions",
            RouteFamily::MeetingsActionItems => "meetings_action_items",
            RouteFamily::ActivitiesProgramsStructuresMilestones => {
                "activities_programs_structures_milestones"
            }
            RouteFamily::ProcessRecording => "process_recording",
            RouteFamily::GatewayGovernanceAlias => "gateway_governance_alias",
            RouteFamily::JsonRpcGovernance => "json_rpc_governance",
        }
    }

    /// The transport surface this family is observed on (control map §4
    /// `surface_kind`), mapped into the domain-agnostic [`ObservationSurface`].
    pub fn surface(self) -> ObservationSurface {
        match self {
            RouteFamily::GatewayGovernanceAlias => ObservationSurface::GatewayAlias,
            RouteFamily::JsonRpcGovernance => ObservationSurface::JsonRpc,
            // All remaining families are application HTTP handler gates.
            RouteFamily::CharterDomain
            | RouteFamily::ProposalDelegation
            | RouteFamily::FederationProposalHelper
            | RouteFamily::StewardDirect
            | RouteFamily::CommentsReactions
            | RouteFamily::MeetingsActionItems
            | RouteFamily::ActivitiesProgramsStructuresMilestones
            | RouteFamily::ProcessRecording => ObservationSurface::Http,
        }
    }
}

/// The class scope a governance mutation surface requires (control map §4
/// `required_class`), using the landed scope-class vocabulary. Closed set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequiredClass {
    /// `governance:charter:write`.
    Charter,
    /// `governance:proposal:write`.
    Proposal,
    /// `governance:steward:write`.
    Steward,
    /// `governance:federation:write`.
    Federation,
    /// `governance:comment:write`.
    Comment,
    /// `governance:meeting:write`.
    Meeting,
    /// `governance:activity:write`.
    Activity,
    /// `governance:process:write`.
    Process,
}

impl RequiredClass {
    /// Every class, in control-map order. Bounded and total.
    pub const ALL: [RequiredClass; 8] = [
        RequiredClass::Charter,
        RequiredClass::Proposal,
        RequiredClass::Steward,
        RequiredClass::Federation,
        RequiredClass::Comment,
        RequiredClass::Meeting,
        RequiredClass::Activity,
        RequiredClass::Process,
    ];

    /// The stable, bounded label for this class (control-map §4 wire value).
    ///
    /// A fixed static string — never a caller-supplied or high-cardinality value.
    pub fn as_label(self) -> &'static str {
        match self {
            RequiredClass::Charter => "charter",
            RequiredClass::Proposal => "proposal",
            RequiredClass::Steward => "steward",
            RequiredClass::Federation => "federation",
            RequiredClass::Comment => "comment",
            RequiredClass::Meeting => "meeting",
            RequiredClass::Activity => "activity",
            RequiredClass::Process => "process",
        }
    }
}

/// A bounded, governance-local scope-admission observation.
///
/// Combines the governance dimensions ([`RouteFamily`], [`RequiredClass`]) with
/// the domain-agnostic match/observation outcomes. Every field is a closed enum;
/// the record carries no identifier or payload of any kind. Nothing here is
/// wired into request handling or emitted — a later slice supplies these values
/// and (separately) emits them under the control-map privacy budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GovernanceScopeAdmission {
    /// Which governance mutation family the admission belongs to.
    pub route_family: RouteFamily,
    /// The class scope the surface requires.
    pub required_class: RequiredClass,
    /// How the class-first gate was satisfied (or why it would be rejected).
    pub match_outcome: ScopeAdmission,
    /// Whether the side-band observation itself was recorded.
    pub observation_outcome: ObservationOutcome,
}

impl GovernanceScopeAdmission {
    /// Construct a bounded governance-local observation. Every argument is a
    /// closed enum, so an observation can never carry an arbitrary or private
    /// value.
    pub fn new(
        route_family: RouteFamily,
        required_class: RequiredClass,
        match_outcome: ScopeAdmission,
        observation_outcome: ObservationOutcome,
    ) -> Self {
        Self {
            route_family,
            required_class,
            match_outcome,
            observation_outcome,
        }
    }

    /// Project to the domain-agnostic [`ScopeAdmissionObservation`], deriving the
    /// surface from the [`RouteFamily`] so `(family, surface)` has a single
    /// source of truth and cannot disagree.
    pub fn observation(&self) -> ScopeAdmissionObservation {
        ScopeAdmissionObservation::new(
            self.route_family.surface(),
            self.match_outcome,
            self.observation_outcome,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn all_route_families_have_distinct_labels_and_a_surface() {
        assert_eq!(RouteFamily::ALL.len(), 10);
        let mut labels = BTreeSet::new();
        for family in RouteFamily::ALL {
            assert!(
                labels.insert(family.as_label()),
                "duplicate label {}",
                family.as_label()
            );
            // Every family maps to a bounded surface.
            let _surface: ObservationSurface = family.surface();
        }
        assert_eq!(labels.len(), 10);
        // The two non-app families map to their non-Http surfaces; all others HTTP.
        assert_eq!(
            RouteFamily::GatewayGovernanceAlias.surface(),
            ObservationSurface::GatewayAlias
        );
        assert_eq!(
            RouteFamily::JsonRpcGovernance.surface(),
            ObservationSurface::JsonRpc
        );
        for family in RouteFamily::ALL {
            if !matches!(
                family,
                RouteFamily::GatewayGovernanceAlias | RouteFamily::JsonRpcGovernance
            ) {
                assert_eq!(family.surface(), ObservationSurface::Http);
            }
        }
    }

    #[test]
    fn all_required_classes_have_distinct_labels() {
        assert_eq!(RequiredClass::ALL.len(), 8);
        let mut labels = BTreeSet::new();
        for class in RequiredClass::ALL {
            assert!(labels.insert(class.as_label()));
        }
        assert_eq!(labels.len(), 8);
    }

    #[test]
    fn labels_match_control_map_wire_values() {
        assert_eq!(RouteFamily::CharterDomain.as_label(), "charter_domain");
        assert_eq!(
            RouteFamily::ActivitiesProgramsStructuresMilestones.as_label(),
            "activities_programs_structures_milestones"
        );
        assert_eq!(
            RouteFamily::JsonRpcGovernance.as_label(),
            "json_rpc_governance"
        );
        assert_eq!(RequiredClass::Charter.as_label(), "charter");
        assert_eq!(RequiredClass::Process.as_label(), "process");
    }

    #[test]
    fn wrapper_constructs_from_fixed_labels_and_projects_consistently() {
        let g = GovernanceScopeAdmission::new(
            RouteFamily::ProposalDelegation,
            RequiredClass::Proposal,
            ScopeAdmission::Fallback,
            ObservationOutcome::Observed,
        );
        let obs = g.observation();
        assert_eq!(obs.surface, RouteFamily::ProposalDelegation.surface());
        assert_eq!(obs.surface, ObservationSurface::Http);
        assert_eq!(obs.match_outcome, ScopeAdmission::Fallback);
        assert_eq!(obs.observation_outcome, ObservationOutcome::Observed);
    }

    #[test]
    fn alias_and_rpc_families_project_to_their_surfaces() {
        assert_eq!(
            GovernanceScopeAdmission::new(
                RouteFamily::GatewayGovernanceAlias,
                RequiredClass::Proposal,
                ScopeAdmission::ClassPreferred,
                ObservationOutcome::Observed,
            )
            .observation()
            .surface,
            ObservationSurface::GatewayAlias
        );
        assert_eq!(
            GovernanceScopeAdmission::new(
                RouteFamily::JsonRpcGovernance,
                RequiredClass::Charter,
                ScopeAdmission::Class,
                ObservationOutcome::ObservationFailed,
            )
            .observation()
            .surface,
            ObservationSurface::JsonRpc
        );
    }

    #[test]
    fn record_is_only_closed_enums_across_the_full_cross_product() {
        // Structural: constructing every (family, class, match, outcome)
        // combination requires no arbitrary or private input.
        for route_family in RouteFamily::ALL {
            for required_class in RequiredClass::ALL {
                let g = GovernanceScopeAdmission::new(
                    route_family,
                    required_class,
                    ScopeAdmission::Class,
                    ObservationOutcome::Observed,
                );
                assert_eq!(g.route_family, route_family);
                assert_eq!(g.required_class, required_class);
            }
        }
    }
}
