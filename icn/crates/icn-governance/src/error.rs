//! Error types for ICN Governance.

use thiserror::Error;

/// Result type for governance operations
pub type Result<T> = std::result::Result<T, GovernanceError>;

/// Errors that can occur in governance operations
#[derive(Debug, Error)]
pub enum GovernanceError {
    /// Domain not found
    #[error("domain not found: {0}")]
    DomainNotFound(String),

    /// Proposal not found
    #[error("proposal not found: {0}")]
    ProposalNotFound(String),

    /// Charter not found
    #[error("charter not found: {0}")]
    CharterNotFound(String),

    /// Steward not found
    #[error("steward not found: {0}")]
    StewardNotFound(String),

    /// Not a member
    #[error("not a member of domain: {0}")]
    NotAMember(String),

    /// Already voted
    #[error("already voted on proposal: {0}")]
    AlreadyVoted(String),

    /// Proposal not open for voting
    #[error("proposal not open for voting: {0}")]
    ProposalNotOpen(String),

    /// Voting period ended
    #[error("voting period ended for proposal: {0}")]
    VotingPeriodEnded(String),

    /// Insufficient quorum
    #[error("insufficient quorum: got {actual}, required {required}")]
    InsufficientQuorum { actual: u64, required: u64 },

    /// Amendment error
    #[error("amendment error: {0}")]
    Amendment(String),

    /// Appeal error
    #[error("appeal error: {0}")]
    Appeal(String),

    /// Invalid state transition
    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),

    /// Authorization failed
    #[error("authorization failed: {0}")]
    AuthorizationFailed(String),

    /// Configuration error
    #[error("configuration error: {0}")]
    Configuration(String),

    /// Storage error
    #[error("storage error: {0}")]
    Storage(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<serde_json::Error> for GovernanceError {
    fn from(e: serde_json::Error) -> Self {
        GovernanceError::Serialization(e.to_string())
    }
}
