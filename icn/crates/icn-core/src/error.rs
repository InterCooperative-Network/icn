//! Error types for ICN Core.

use thiserror::Error;

/// Result type for core operations
pub type Result<T> = std::result::Result<T, CoreError>;

/// Errors that can occur in the core runtime
#[derive(Debug, Error)]
pub enum CoreError {
    /// Channel send failed
    #[error("channel send failed: {0}")]
    ChannelSend(String),

    /// Channel receive failed
    #[error("channel receive failed: {0}")]
    ChannelReceive(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("deserialization error: {0}")]
    Deserialization(String),

    /// Proposal not found
    #[error("proposal not found: {0}")]
    ProposalNotFound(String),

    /// Domain not found
    #[error("domain not found: {0}")]
    DomainNotFound(String),

    /// Federation registry error
    #[error("federation error: {0}")]
    Federation(String),

    /// Configuration error
    #[error("configuration error: {0}")]
    Configuration(String),

    /// Lock poisoned
    #[error("lock poisoned: {0}")]
    LockPoisoned(String),

    /// Actor not running
    #[error("actor not running: {0}")]
    ActorNotRunning(String),

    /// Invalid state
    #[error("invalid state: {0}")]
    InvalidState(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        CoreError::Serialization(e.to_string())
    }
}

impl From<bincode::Error> for CoreError {
    fn from(e: bincode::Error) -> Self {
        CoreError::Serialization(e.to_string())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for CoreError {
    fn from(e: tokio::sync::mpsc::error::SendError<T>) -> Self {
        CoreError::ChannelSend(e.to_string())
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for CoreError {
    fn from(e: tokio::sync::oneshot::error::RecvError) -> Self {
        CoreError::ChannelReceive(e.to_string())
    }
}
