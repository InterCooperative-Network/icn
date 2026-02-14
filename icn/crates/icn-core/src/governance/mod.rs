//! Governance actor for decentralized decision-making
//!
//! The GovernanceActor implementation has been extracted to the
//! `icn-governance-actor` crate (apps/governance). This module
//! re-exports its public types for backwards compatibility.

pub use icn_governance_actor::actor;
pub use icn_governance_actor::{
    ForcedOutcome, GovernanceActor, GovernanceCommand, GovernanceConfigLite, GovernanceHandle,
    ProposalId,
};
