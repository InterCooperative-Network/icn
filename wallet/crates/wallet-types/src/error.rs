use thiserror::Error;
use std::io;

/// Common error type used across wallet components
#[derive(Error, Debug)]
pub enum WalletError {
    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// DAG error
    #[error("DAG error: {0}")]
    DagError(String),
    
    /// Identity error
    #[error("Identity error: {0}")]
    IdentityError(String),
    
    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    /// Resource not found
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
    
    /// Connection error
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    /// Timeout error
    #[error("Timeout error: {0}")]
    TimeoutError(String),
    
    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),
    
    /// Generic error
    #[error("{0}")]
    GenericError(String),
}

impl From<serde_json::Error> for WalletError {
    fn from(err: serde_json::Error) -> Self {
        WalletError::SerializationError(err.to_string())
    }
}

/// Result type alias using WalletError
pub type WalletResult<T> = Result<T, WalletError>; 