/*!
# ICN Models

This crate contains shared data models used by multiple components of the ICN Runtime.
It defines interfaces and data structures that are used across multiple crates,
helping to prevent circular dependencies.
*/

#![deny(missing_docs)]
#![deny(unused_imports)]

// Re-export cid for convenience
pub use cid::Cid;

// Module declarations
pub mod dag;
pub mod storage;

#[cfg(test)]
mod tests;

// Re-export common types for ease of use
pub use dag::{DagNode, DagNodeBuilder, DagNodeMetadata, DagType, DagCodec, dag_storage_codec};
pub use storage::{StorageBackend, StorageError, StorageResult, BasicStorageManager, DagStorageManager};

/// Result type for operations that can fail
pub type Result<T> = anyhow::Result<T>;

/// Common error types for model operations
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// Error when serializing data
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// Error when deserializing data
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
    
    /// Error when validating data
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    /// Error when a required field is missing
    #[error("Missing field: {0}")]
    MissingField(String),
}

// Placeholder for future shared functionality
