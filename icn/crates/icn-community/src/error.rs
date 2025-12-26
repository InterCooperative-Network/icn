use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommunityError {
    #[error("Community not found: {0}")]
    NotFound(String),

    #[error("Member/cooperative not found: {0}")]
    MemberNotFound(String),

    #[error("Insufficient resources: need {required}, have {available}")]
    InsufficientResources { required: u64, available: u64 },

    #[error("Action not permitted: {0}")]
    NotPermitted(String),

    #[error("Invalid status transition from {from:?} to {to:?}")]
    InvalidStatusTransition {
        from: crate::types::CommunityStatus,
        to: crate::types::CommunityStatus,
    },

    #[error("Governance error: {0}")]
    Governance(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, CommunityError>;
