use thiserror::Error;

#[derive(Error, Debug)]
pub enum CooperativeError {
    #[error("Cooperative not found: {0}")]
    NotFound(String),

    #[error("Insufficient founding members: need {required}, have {actual}")]
    InsufficientFounders { required: usize, actual: usize },

    #[error("Member already exists: {0}")]
    MemberAlreadyExists(String),

    #[error("Member not found: {0}")]
    MemberNotFound(String),

    #[error("Insufficient votes for action: need {required}, have {actual}")]
    InsufficientVotes { required: u64, actual: u64 },

    #[error("Invalid cooperative status transition from {from:?} to {to:?}")]
    InvalidStatusTransition {
        from: crate::types::CooperativeStatus,
        to: crate::types::CooperativeStatus,
    },

    #[error("Action not permitted: {0}")]
    NotPermitted(String),

    #[error("Invalid cooperative type: {0}")]
    InvalidType(String),

    #[error("Governance error: {0}")]
    Governance(String),

    #[error("Ledger error: {0}")]
    Ledger(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

pub type Result<T> = std::result::Result<T, CooperativeError>;
