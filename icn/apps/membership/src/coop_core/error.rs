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
    Serialization(String),

    #[error("Governance error: {0}")]
    Governance(String),

    #[error("Ledger error: {0}")]
    Ledger(String),

    #[error("Treasury nonce mismatch for coop {coop_id}: expected {expected}, stored {stored}")]
    NonceMismatch {
        coop_id: String,
        expected: u64,
        stored: u64,
    },
}

pub type Result<T> = std::result::Result<T, CoopError>;

impl From<icn_encoding::Error> for CoopError {
    fn from(e: icn_encoding::Error) -> Self {
        CoopError::Serialization(e.to_string())
    }
}
