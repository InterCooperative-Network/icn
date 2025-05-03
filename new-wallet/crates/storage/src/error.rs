use thiserror::Error;
use std::io;

/// Error type for storage operations
#[derive(Error, Debug)]
pub enum StorageError {
    /// I/O operations failed
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// Serialization or deserialization failed
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Key not found in storage
    #[error("Item not found: {0}")]
    NotFound(String),

    /// Storage path invalid or inaccessible
    #[error("Invalid storage path: {0}")]
    InvalidPath(String),

    /// Data corruption detected
    #[error("Data corruption: {0}")]
    DataCorruption(String),

    /// Concurrent access conflict
    #[error("Concurrent access conflict: {0}")]
    ConcurrencyError(String),

    /// Insufficient permissions for operation
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Storage capacity exceeded
    #[error("Storage capacity exceeded: {0}")]
    CapacityExceeded(String),

    /// Invalid data format
    #[error("Invalid data format: {0}")]
    InvalidFormat(String),

    /// Encryption or decryption failed
    #[error("Encryption error: {0}")]
    EncryptionError(String),

    /// Other unexpected errors
    #[error("Unexpected error: {0}")]
    Other(String),
}

/// Implement conversion from serde_json errors
impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        StorageError::SerializationError(format!("JSON error: {}", err))
    }
}

/// Result type for storage operations
pub type StorageResult<T> = std::result::Result<T, StorageError>; 