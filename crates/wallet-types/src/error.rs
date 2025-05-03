use thiserror::Error;

/// Shared error types used by multiple crates
#[derive(Error, Debug)]
pub enum SharedError {
    /// Not found error
    #[error("Not found: {0}")]
    NotFound(String),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),
    
    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    /// DAG error
    #[error("DAG error: {0}")]
    DagError(String),
    
    /// CID error
    #[error("CID error: {0}")]
    CidError(String),
    
    /// Invalid parameters
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),
    
    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),
    
    /// Other error
    #[error("Other error: {0}")]
    Other(String),
}

/// Shared result type
pub type SharedResult<T> = Result<T, SharedError>; 