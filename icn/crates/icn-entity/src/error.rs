//! Error types for the entity module

use thiserror::Error;

/// Errors that can occur in entity operations
///
/// These errors provide context about what operation failed and why,
/// with suggestions for resolution where applicable.
#[derive(Error, Debug)]
pub enum EntityError {
    /// Invalid entity ID format
    ///
    /// Entity IDs must follow a specific format (e.g., UUID or DID).
    #[error("Invalid entity ID format: '{0}'. Expected a valid UUID or DID.")]
    InvalidFormat(String),

    /// Entity not found
    ///
    /// The requested entity does not exist in the registry.
    #[error("Entity not found: '{0}'. Verify the ID is correct and the entity was created.")]
    NotFound(String),

    /// Entity already exists
    ///
    /// Cannot create an entity with an ID that is already in use.
    #[error("Entity already exists: '{0}'. Use update operations to modify existing entities.")]
    AlreadyExists(String),

    /// Invalid entity type
    ///
    /// The entity type is not recognized or cannot be used in this context.
    #[error(
        "Invalid entity type: '{0}'. Valid types: Cooperative, Federation, Community, Member."
    )]
    InvalidType(String),

    /// Invalid state transition
    ///
    /// Entity lifecycle transitions must follow valid paths (e.g., Pending -> Active -> Suspended).
    #[error("Invalid state transition from {from:?} to {to:?}. Check allowed state machine transitions.")]
    InvalidStateTransition {
        from: crate::EntityStatus,
        to: crate::EntityStatus,
    },

    /// Membership error
    ///
    /// Error related to entity membership operations (join, leave, member limits).
    #[error("Membership error: {0}")]
    MembershipError(String),

    /// Concurrent modification detected
    ///
    /// The entity was modified by another operation since it was read.
    /// This error occurs when optimistic locking detects a version mismatch.
    #[error("Concurrent modification detected for entity: '{0}'. The entity was updated by another operation. Please retry the operation with the latest entity version.")]
    ConcurrentModification(String),

    /// Registry error
    ///
    /// Error from the underlying storage layer.
    #[error("Registry error: {0}")]
    RegistryError(String),

    /// Serialization error
    ///
    /// Failed to serialize or deserialize entity data.
    #[error("Serialization error: {0}. Check entity data format.")]
    SerializationError(#[from] serde_json::Error),

    /// Internal error
    ///
    /// An unexpected internal error occurred.
    #[error("Internal error: {0}. Please report this issue.")]
    Internal(#[from] anyhow::Error),
}

/// Result type for entity operations
pub type Result<T> = std::result::Result<T, EntityError>;
