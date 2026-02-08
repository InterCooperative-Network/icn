//! ICN Governance - Substrate for community decision-making
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//!
//! This crate provides governance primitives for decentralized decision-making
//! in cooperative networks. It is designed as a substrate: democratic by default,
//! configurable by communities, and extensible via contracts.
//!
//! # Core Concepts
//!
//! - **GovernanceDomain**: A decision space (organization, coop, etc.)
//! - **Proposal**: A decision to be made with title, description, and payload
//! - **Vote**: A member's choice on a proposal (for/against/abstain)
//! - **GovernanceProfile**: Rules for evaluating votes and determining outcomes
//! - **MembershipConfig**: Who is eligible to vote
//!
//! # Example
//!
//! ```rust
//! use icn_governance::*;
//!
//! // Create a cooperative governance domain
//! let domain = GovernanceDomain::new(
//!     "tech-coop".to_string(),
//!     GovernanceConfig::cooperative_default(),
//! );
//! ```

#[allow(missing_docs)]
pub mod amendment;
#[allow(missing_docs)]
pub mod appeal;
#[allow(missing_docs)]
pub mod charter;
#[allow(missing_docs)]
pub mod charter_store;
pub mod config;
#[allow(missing_docs)]
pub mod delegation;
#[allow(missing_docs)]
pub mod discussion;
pub mod domain;
#[allow(missing_docs)]
pub mod error;
pub mod handle;
pub mod membership;
#[allow(missing_docs)]
pub mod message;
pub mod profile;
pub mod proof;
#[allow(missing_docs)]
pub mod proposal;
#[allow(missing_docs)]
pub mod protocol;
#[allow(missing_docs)]
pub mod protocol_defaults;
#[allow(missing_docs)]
pub mod protocol_store;
#[allow(missing_docs)]
pub mod protocol_validation;
#[allow(missing_docs)]
pub mod resolver;
#[allow(missing_docs)]
pub mod sdis;
#[allow(missing_docs)]
pub mod steward;
#[allow(missing_docs)]
pub mod steward_store;
pub mod store;
pub mod tally;
pub mod vote;

// Action items for meeting/task tracking
pub mod action_item;

// Orphan cleanup (Phase 21)
pub mod orphan_cleanup;

// Proposal cleanup and archival
pub mod proposal_cleanup;

pub use amendment::{
    Amendment, AmendmentChange, AmendmentId, AmendmentScope, AmendmentStatus, AmendmentType,
    ChangeTarget, ChangeType, Ratification, RatificationRequirements, RatificationResult,
    RatifierType,
};
pub use appeal::{
    Appeal, AppealDeadlines, AppealEvidence, AppealGrounds, AppealId, AppealOutcome, AppealRemedy,
    AppealResponse, AppealScope, AppealStatus, AppealType, EvidenceType, ResponseType,
};
pub use charter::{
    AmendmentRef, ArbitratorSelection, Charter, CharterId, CharterStatus, ContributionRouting,
    DisputePolicy, EconomicPolicy, FeePeriod, FounderSignature, MembershipFee, MembershipPolicy,
    OrgType,
};
pub use charter_store::{CharterStore, CharterStoreBackend, InMemoryCharterStore};
pub use config::{
    default_max_execution_delay, EmergencyThresholds, GovernanceConfig, GovernanceParams,
    ProposalThresholds,
};
pub use delegation::{
    scopes_overlap, Delegation, DelegationError, DelegationId, DelegationManager, DelegationScope,
    ProposalDomainLookup, DEFAULT_MAX_DELEGATION_DEPTH,
};
pub use discussion::{
    Comment, CommentId, Discussion, DiscussionError, DiscussionStore, InMemoryDiscussionStore,
};
pub use domain::{GovernanceDomain, GovernanceDomainId};
pub use error::{GovernanceError, Result};
pub use handle::GovernanceOps;
pub use membership::{MembershipConfig, MembershipSource};
pub use message::{GovernanceMessage, ProposalOutcome, TallySnapshot};
pub use profile::{DecisionOutcome, GovernanceProfile, GovernanceProfileId, GovernanceRule};
pub use proof::{GovernanceProof, ProofOutcome};
pub use proposal::{
    DataSharingLevel, DisputeResolutionMethod, DisputeResolutionOutcome, FederationProposal,
    FederationTerms, ForcedOutcome, MembershipAction, Proposal, ProposalId, ProposalPayload,
    ProposalScope, ProposalState, ResourceAccessAction, TreasuryApprovalType,
    TreasuryProposalOperation, Version,
};
pub use resolver::{MembershipResolver, StaticMembershipResolver};
pub use sdis::{
    AttestationType, InstitutionalAuthority, JurisdictionTier, RevocationTarget, SdisProposal,
    SdisVotingRequirements, StewardPenalty, StewardStats, ThresholdOp, ThresholdType,
};
pub use steward::{
    AttestationType as StewardAttestationType, ContactMethod, DisputeOutcome, StewardAttestation,
    StewardContact, StewardId, StewardRecord, StewardStatus,
};
pub use steward_store::{InMemoryStewardStore, StewardStore, StewardStoreBackend};
pub use store::{GovernanceStore, InMemoryGovernanceStore, PaginatedResult};
pub use tally::{
    compute_detailed_tally_with_delegations, compute_tally_with_delegations, DelegatedTallyResult,
    VoteTally,
};
pub use vote::{Vote, VoteChoice};

// Protocol governance types (Phase 20)
pub use protocol::{
    ParameterChange, ParameterConstraints, ParameterScope, ParameterValidationError,
    ParameterValue, PendingChangeId, PendingChangeStatus, PendingParameterChange,
    ProtocolChangeProposal, ProtocolParameter,
};
pub use protocol_defaults::{default_parameters, get_default_parameter, ParameterCategory};
pub use protocol_store::{
    InMemoryParameterStore, ParameterStoreError, ProtocolParameterStore, SledParameterStore,
};
pub use protocol_validation::{
    validate_parameter_update, validate_parameter_value, validate_scope_override, validate_version,
};

// Orphan cleanup types (Phase 21)
pub use orphan_cleanup::{
    cleanup_orphan_parameters, cleanup_orphan_parameters_sync, count_orphan_parameters,
    OrphanCleanupConfig, OrphanCleanupResult, OrphanDetail,
};

// Proposal cleanup and archival types
pub use proposal_cleanup::{CleanupStats, ProposalArchive, ProposalCleanupTask, ProposalRetention};

// Action item types
pub use action_item::{
    ActionItem, ActionItemFilter, ActionItemId, ActionItemNote, ActionItemPriority,
    ActionItemStatus, ActionItemStore, ActionItemStoreBackend, InMemoryActionItemStore,
};

/// Unix timestamp in seconds
pub type Timestamp = u64;
