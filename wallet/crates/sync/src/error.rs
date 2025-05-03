use thiserror::Error;
use std::io;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

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
            _ => SyncError::Request(err),
        }
    } else {
        SyncError::Request(err)
    }
} 