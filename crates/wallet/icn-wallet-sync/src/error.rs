use thiserror::Error;
use std::io;
use icn_wallet_types::SharedError;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("API error: {0}")]
    Api(String),

    #[error("DAG error: {0}")]
    Dag(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Federation error: {0}")]
    Federation(String),

    #[error("Node submission error: {0}")]
    NodeSubmission(String),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Backoff error: operation failed after retries")]
    BackoffError,

    #[error("Internal error: {0}")]
    Internal(String),
    
    #[error("Wallet error: {0}")]
    WalletError(#[from] SharedError),
    
    #[error("Type conversion error: {0}")]
    TypeConversion(String),
    
    #[error("Timestamp conversion error: {0}")]
    TimestampError(String),
}

// Implement a conversion from backoff::Error<SyncError> to SyncError
impl From<backoff::Error<SyncError>> for SyncError {
    fn from(err: backoff::Error<SyncError>) -> Self {
        match err {
            backoff::Error::Permanent(e) => e,
            backoff::Error::Transient { err, .. } => err,
        }
    }
}

// Add conversion to SharedError
impl From<SyncError> for SharedError {
    fn from(err: SyncError) -> Self {
        match err {
            SyncError::Network(msg) => SharedError::ConnectionError(msg),
            SyncError::Io(e) => SharedError::IoError(e),
            SyncError::Serialization(e) => SharedError::SerializationError(e.to_string()),
            SyncError::Api(msg) => SharedError::ConnectionError(msg),
            SyncError::Dag(msg) => SharedError::GenericError(format!("DAG error: {}", msg)),
            SyncError::Validation(msg) => SharedError::ValidationError(msg),
            SyncError::Authentication(msg) => SharedError::AuthenticationError(msg),
            SyncError::Federation(msg) => SharedError::GenericError(format!("Federation error: {}", msg)),
            SyncError::NodeSubmission(msg) => SharedError::GenericError(format!("Node submission error: {}", msg)),
            SyncError::NodeNotFound(id) => SharedError::ResourceNotFound(format!("Node not found: {}", id)),
            SyncError::Request(e) => SharedError::ConnectionError(e.to_string()),
            SyncError::BackoffError => SharedError::TimeoutError("Operation failed after retries".to_string()),
            SyncError::Internal(msg) => SharedError::GenericError(format!("Internal error: {}", msg)),
            SyncError::WalletError(e) => e,
            SyncError::TypeConversion(msg) => SharedError::SerializationError(format!("Type conversion error: {}", msg)),
            SyncError::TimestampError(msg) => SharedError::GenericError(format!("Timestamp error: {}", msg)),
        }
    }
}

// Add specific conversions for reqwest::Error to provide better error context
pub fn map_reqwest_error(err: reqwest::Error) -> SyncError {
    if err.is_timeout() {
        SyncError::Network("Request timed out".to_string())
    } else if err.is_connect() {
        SyncError::Network("Connection error".to_string())
    } else if let Some(status) = err.status() {
        match status.as_u16() {
            401 | 403 => SyncError::Authentication(format!("Authentication failed: {}", err)),
            404 => SyncError::NodeNotFound(format!("Resource not found: {}", err)),
            408 => SyncError::Network(format!("Request timeout: {}", err)),
            429 => SyncError::Network(format!("Rate limited: {}", err)),
            500..=599 => SyncError::Api(format!("Server error: {}", err)),
            _ => SyncError::Request(err),
        }
    } else {
        SyncError::Request(err)
    }
} 