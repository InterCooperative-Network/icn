//! ICN Governance - Substrate for community decision-making
#![warn(missing_docs)]
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

pub mod amendment;
pub mod appeal;
pub mod charter;
pub mod error;
pub mod charter_store;
pub mod config;
pub mod domain;
pub mod handle;
pub mod membership;
pub mod message;
pub mod profile;
pub mod proposal;
pub mod resolver;
pub mod sdis;
pub mod steward;
pub mod steward_store;
pub mod store;
pub mod tally;
pub mod vote;

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
pub use config::{EmergencyThresholds, GovernanceConfig, GovernanceParams};
pub use error::{GovernanceError, Result};
pub use domain::{GovernanceDomain, GovernanceDomainId};
pub use handle::GovernanceOps;
pub use membership::{MembershipConfig, MembershipSource};
pub use message::{GovernanceMessage, ProposalOutcome, TallySnapshot};
pub use profile::{DecisionOutcome, GovernanceProfile, GovernanceProfileId, GovernanceRule};
pub use proposal::{
    DisputeResolutionOutcome, ForcedOutcome, MembershipAction, Proposal, ProposalId,
    ProposalPayload, ProposalState,
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
pub use store::{GovernanceStore, InMemoryGovernanceStore};
pub use tally::VoteTally;
pub use vote::{Vote, VoteChoice};

/// Unix timestamp in seconds
pub type Timestamp = u64;
