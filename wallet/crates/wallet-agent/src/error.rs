use thiserror::Error;
use wallet_core::error::WalletError as CoreError;
use wallet_types::error::SharedError;

/// Agent error types
#[derive(Error, Debug)]
pub enum AgentError {
    /// Invalid parameters
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),
    
    /// Invalid state
    #[error("Invalid state: {0}")]
    InvalidState(String),
    
    /// Core wallet error (from wallet-core)
    #[error("Core error: {0}")]
    CoreError(#[from] CoreError),
    
    /// Shared error (from wallet-types)
    #[error("Shared error: {0}")]
    SharedError(#[from] SharedError),
    
    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),
    
    /// Store error
    #[error("Store error: {0}")]
    StoreError(String),
    
    /// Processing error
    #[error("Processing error: {0}")]
    ProcessingError(String),
    
    /// Queue error
    #[error("Queue error: {0}")]
    QueueError(String),
    
    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// DAG error
    #[error("DAG error: {0}")]
    DagError(String),
    
    /// Action error
    #[error("Action error: {0}")]
    ActionError(String),
    
    /// Not found
    #[error("Not found: {0}")]
    NotFound(String),
    
    /// Not implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),
    
    /// User intervention required
    #[error("User intervention required: {0}")]
    UserInterventionRequired(String),
    
    /// Connection error
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    /// Permission error
    #[error("Permission error: {0}")]
    PermissionError(String),
    
    /// Resource not found
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
    
    /// Server error
    #[error("Server error: {0}")]
    ServerError(String),
    
    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    
    /// Governance error
    #[error("Governance error: {0}")]
    GovernanceError(String),
    
    /// Sync error
    #[error("Sync error: {0}")]
    SyncError(String),
    
    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),
    
    /// Thread conflict
    #[error("Thread conflict: {0}")]
    ThreadConflict(String),
    
    /// Authorization error
    #[error("Authorization error: {0}")]
    AuthorizationError(String),
    
    /// Auth error
    #[error("Auth error: {0}")]
    AuthError(String),
    
    /// Wallet core error
    #[error("Wallet core error: {0}")]
    WalletCoreError(String),
    
    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    /// Task error
    #[error("Task error: {0}")]
    TaskError(String),
    
    /// Retry exhausted
    #[error("Retry exhausted: {0}")]
    RetryExhausted(String),
    
    /// Retry error
    #[error("Retry error: {0}")]
    RetryError(String),
    
    /// Other error
    #[error("Other error: {0}")]
    Other(String),
}

impl From<tokio::task::JoinError> for AgentError {
    fn from(err: tokio::task::JoinError) -> Self {
        AgentError::TaskError(err.to_string())
    }
}

impl From<serde_json::Error> for AgentError {
    fn from(err: serde_json::Error) -> Self {
        AgentError::SerializationError(err.to_string())
    }
}

impl From<reqwest::Error> for AgentError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_connect() {
            AgentError::ConnectionError(err.to_string())
        } else if err.is_timeout() {
            AgentError::NetworkError(format!("Request timed out: {}", err))
        } else if err.is_decode() {
            AgentError::SerializationError(format!("Failed to decode response: {}", err))
        } else {
            AgentError::Other(err.to_string())
        }
    }
}

// Add conversion from backoff::Error<AgentError> to AgentError
impl From<backoff::Error<AgentError>> for AgentError {
    fn from(err: backoff::Error<AgentError>) -> Self {
        match err {
            backoff::Error::Permanent(e) => e,
            backoff::Error::Transient { err, retry_after: _ } => {
                AgentError::RetryError(format!("Transient error during retry: {}", err))
            }
        }
    }
}

pub type AgentResult<T> = Result<T, AgentError>; 