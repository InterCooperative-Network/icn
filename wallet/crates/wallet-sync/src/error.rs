use thiserror::Error;
use wallet_core::error::WalletError as CoreError;
use wallet_types::error::SharedError;

/// Sync errors
#[derive(Error, Debug)]
pub enum SyncError {
    /// Connection error
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    /// Protocol error
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    
    /// Verification error
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    /// Resource not found
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
    
    /// DAG error
    #[error("DAG error: {0}")]
    DagError(String),
    
    /// IO error
    #[error("IO error: {0}")]
    IoError(String),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// Core wallet error
    #[error("Core wallet error: {0}")]
    CoreError(#[from] CoreError),
    
    /// Shared error (from wallet-types)
    #[error("Shared error: {0}")]
    SharedError(#[from] SharedError),
    
    /// HTTP client error
    #[error("HTTP client error: {0}")]
    HttpError(String),
    
    /// Not found
    #[error("Not found: {0}")]
    NotFound(String),
    
    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    /// Network submission error
    #[error("Network submission error: {0}")]
    SubmissionError(String),
    
    /// Network is offline
    #[error("Network is offline: {0}")]
    Offline(String),
    
    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),
    
    /// Not authorized
    #[error("Not authorized: {0}")]
    NotAuthorized(String),
    
    /// Internal error
    #[error("Internal error: {0}")]
    InternalError(String),
    
    /// CID error
    #[error("CID error: {0}")]
    CidError(String),
}

/// Sync result type
pub type SyncResult<T> = Result<T, SyncError>;

impl From<reqwest::Error> for SyncError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            SyncError::ConnectionError(format!("Connection timeout: {}", error))
        } else if error.is_connect() {
            SyncError::ConnectionError(format!("Connection error: {}", error))
        } else {
            SyncError::ConnectionError(format!("HTTP error: {}", error))
        }
    }
} 