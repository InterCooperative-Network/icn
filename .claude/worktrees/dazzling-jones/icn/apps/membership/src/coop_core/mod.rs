//! Coop core module - cooperative management and lifecycle
//!
//! This module contains the cooperative types, actor, lifecycle, membership,
//! and storage that were originally in the `icn-coop` crate.
#![allow(missing_docs)]

pub mod actor;
pub mod error;
pub mod handle;
pub mod lifecycle;
pub mod membership;
pub mod store;
pub mod types;

pub use actor::{CoopActor, CoopMessage, GossipHandle, TreasuryManagerHandle, COOP_TOPIC};
pub use error::{CoopError, Result};
pub use handle::CoopHandle;
pub use lifecycle::{
    derive_treasury_anchor, derive_treasury_did, LifecycleEvent, LifecycleManager,
};
pub use membership::{MembershipChange, MembershipManager};
pub use store::CoopStore;
pub use types::{
    AssetDistributionPlan, BalanceAction, CapitalReturnMethod, CoopStatus, CoopType, Cooperative,
    CooperativeId, DebtAction, DissolutionRequest, FormationRequest, Member, MemberRole,
    MemberStatus, MembershipApplication, MembershipTier, StoredFounderSignature,
};

// Re-export governance charter types for convenience
pub use icn_governance::charter::{CharterId, FounderSignature};
