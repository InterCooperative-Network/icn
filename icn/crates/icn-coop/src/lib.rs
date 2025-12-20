//! ICN Coop - Cooperative management and lifecycle
#![warn(missing_docs)]

/// Cooperative actor for handling coop operations
pub mod actor;
/// Handle for interacting with the coop actor
pub mod handle;
/// Lifecycle events and state transitions
pub mod lifecycle;
/// Membership management
pub mod membership;
/// Persistent storage for cooperatives
pub mod store;
/// Cooperative types and data structures
pub mod types;

pub use actor::{CoopActor, CoopMessage, GossipHandle, COOP_TOPIC};
pub use handle::CoopHandle;
pub use lifecycle::{LifecycleEvent, LifecycleManager};
pub use membership::{MembershipChange, MembershipManager};
pub use store::CoopStore;
pub use types::{CoopStatus, CoopType, Cooperative, Member, MemberRole, MemberStatus};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoopError {
    #[error("Cooperative not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),

    #[error("Member not found: {0}")]
    MemberNotFound(String),

    #[error("Duplicate member: {0}")]
    DuplicateMember(String),

    #[error("Storage error: {0}")]
    Storage(#[from] sled::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("Governance error: {0}")]
    Governance(String),

    #[error("Ledger error: {0}")]
    Ledger(String),
}

pub type Result<T> = std::result::Result<T, CoopError>;
