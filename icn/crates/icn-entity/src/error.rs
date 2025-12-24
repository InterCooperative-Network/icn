//! Error types for the entity module

use thiserror::Error;

/// Errors that can occur in entity operations
#[derive(Error, Debug)]
pub enum EntityError {
    /// Invalid entity ID format
    #[error("Invalid entity ID format: {0}")]
    InvalidFormat(String),

    /// Entity not found
    #[error("Entity not found: {0}")]
    NotFound(String),

    /// Entity already exists
    #[error("Entity already exists: {0}")]
    AlreadyExists(String),

    /// Invalid entity type
    #[error("Invalid entity type: {0}")]
    InvalidType(String),

    /// Invalid state transition
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: crate::EntityStatus,
        to: crate::EntityStatus,
    },

    /// Membership error
    #[error("Membership error: {0}")]
    MembershipError(String),

    /// Registry error
    #[error("Registry error: {0}")]
    RegistryError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

/// Result type for entity operations
pub type Result<T> = std::result::Result<T, EntityError>;
