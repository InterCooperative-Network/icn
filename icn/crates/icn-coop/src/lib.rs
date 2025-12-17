pub mod types;
pub mod lifecycle;
pub mod membership;
pub mod store;
pub mod actor;
pub mod handle;

pub use types::{Cooperative, CoopType, MemberRole, MemberStatus, CoopStatus, Member};
pub use lifecycle::{LifecycleManager, LifecycleEvent};
pub use membership::{MembershipManager, MembershipChange};
pub use store::CoopStore;
pub use actor::{CoopActor, CoopMessage};
pub use handle::CoopHandle;

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
