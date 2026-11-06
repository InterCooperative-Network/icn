//! Human actor model for a governance domain (Technical Alpha rung 4).
//!
//! A [`GovernanceDomain`](crate::GovernanceDomain) answers one question this
//! module owns: *who is a human actor in this domain* — a legacy device
//! **Principal** (a `Did` listed in the membership source), or a
//! context-scoped **Subject** (GEN-A / GEN-B)?
//!
//! # What this is
//!
//! A safety boundary. [`HumanActorModel::ContextSubjectV1`] is an explicit,
//! opt-in domain mode in which every legacy Principal-based membership,
//! electorate, vote, delegation and closure path **refuses** (fail-closed)
//! before any mutation. It is not a Subject voting system: no Subject-based
//! membership, electorate or tally exists yet, so a Subject-mode domain is
//! refusal-only until a later, separately authorised increment lands one.
//!
//! # What this is not
//!
//! - Not recognition-as-authority. A GEN-B `SubjectRecognitionV1` (including
//!   its non-production `BootstrapRehearsalV1` basis) is a verifiable *fact*;
//!   it is never a membership credential, an electorate entry, or a vote.
//! - Not a claim that every human relationship is an institution. The
//!   personal, bilateral, household and informal-group contexts are outside
//!   `GovernanceDomain` and are not touched here.
//! - Not a migration. There is deliberately no legacy → Subject conversion of
//!   membership; unsafe combinations are refused, never repaired.
//!
//! # Default and persistence
//!
//! The field defaults to [`HumanActorModel::LegacyPrincipalV0`] when absent, so
//! every domain serialized before this field existed loads unchanged and
//! behaves as before. A legacy domain re-serializes **without** the field, so
//! its bytes are identical to the pre-field encoding. Nothing is rewritten on
//! load. An unknown or `null` value is a deserialization error, never a
//! silent fallback to either model.
//!
//! # The mixed-state invariant
//!
//! A Subject-mode domain must carry no legacy membership source. The
//! membership model has no "none" variant, so the one inert shape is
//! `StaticList(vec![])`: it names nobody and resolves to no electorate.
//! Any non-empty `StaticList`, and any `TrustThreshold` (which is always
//! "active": it resolves an electorate from the trust graph), is refused.

use thiserror::Error;

use crate::membership::MembershipSource;

/// Which kind of human actor a governance domain recognises.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum HumanActorModel {
    /// Legacy: humans act through device Principals (`Did`s) listed in the
    /// domain's membership source. The default for every existing domain.
    #[default]
    LegacyPrincipalV0,
    /// Context-scoped human Subjects. Explicit opt-in only. Refusal-only on
    /// every legacy path until Subject membership and voting are implemented.
    ContextSubjectV1,
}

impl HumanActorModel {
    /// `true` for the legacy default. Used by `skip_serializing_if` so a legacy
    /// domain serializes exactly as it did before this field existed.
    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::LegacyPrincipalV0)
    }
}

/// A legacy Principal-rail operation that a Subject-mode domain refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyRailOp {
    /// Adding or removing a legacy member.
    MembershipMutation,
    /// Replacing the domain configuration.
    ConfigMutation,
    /// Recording a Principal-keyed vote.
    VoteCast,
    /// Resolving a Principal-based electorate.
    ElectorateResolution,
    /// Closing, vetoing or force-closing a proposal.
    ProposalClosure,
    /// Creating a Principal-to-Principal vote delegation.
    DelegationCreation,
}

impl LegacyRailOp {
    /// Stable operation name used in refusal messages.
    pub fn name(&self) -> &'static str {
        match self {
            Self::MembershipMutation => "membership_mutation",
            Self::ConfigMutation => "config_mutation",
            Self::VoteCast => "vote_cast",
            Self::ElectorateResolution => "electorate_resolution",
            Self::ProposalClosure => "proposal_closure",
            Self::DelegationCreation => "delegation_creation",
        }
    }
}

/// Why the actor-model firewall refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ActorModelError {
    /// The domain is `ContextSubjectV1` and the caller used a legacy
    /// Principal-rail operation.
    #[error(
        "domain '{domain}' uses the ContextSubjectV1 actor model: legacy Principal-rail \
         operation '{}' is refused (Subject membership and voting are not implemented; \
         recognition is not authorization)",
        .op.name()
    )]
    LegacyRailRefused {
        /// The domain that refused.
        domain: String,
        /// The refused operation.
        op: LegacyRailOp,
    },
    /// A `ContextSubjectV1` domain would carry a legacy membership source.
    #[error(
        "domain '{domain}': ContextSubjectV1 forbids a legacy membership source \
         (found {found}); no implicit conversion is performed"
    )]
    LegacyMembershipPresent {
        /// The domain that would be mixed.
        domain: String,
        /// What was found: `static_list(<n> members)` or `trust_threshold`.
        found: String,
    },
}

/// Check that `source` is the one inert shape a Subject-mode domain may hold.
pub(crate) fn check_no_legacy_membership(
    domain: &str,
    source: &MembershipSource,
) -> Result<(), ActorModelError> {
    match source {
        MembershipSource::StaticList(members) if members.is_empty() => Ok(()),
        MembershipSource::StaticList(members) => Err(ActorModelError::LegacyMembershipPresent {
            domain: domain.to_string(),
            found: format!("static_list({} members)", members.len()),
        }),
        MembershipSource::TrustThreshold(_) => Err(ActorModelError::LegacyMembershipPresent {
            domain: domain.to_string(),
            found: "trust_threshold".to_string(),
        }),
    }
}
