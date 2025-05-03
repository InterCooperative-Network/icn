use thiserror::Error;
use wallet_core::error::WalletError as CoreError;
use wallet_sync::SyncError;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Queue error: {0}")]
    QueueError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Governance error: {0}")]
    GovernanceError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Credential error: {0}")]
    CredentialError(String),
    
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    #[error("Permission error: {0}")]
    PermissionError(String),
    
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    
    #[error("Server error: {0}")]
    ServerError(String),
    
    #[error("Core wallet error: {0}")]
    CoreError(#[from] CoreError),
    
    #[error("Feature not implemented: {0}")]
    NotImplemented(String),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
    
    #[error("Store error: {0}")]
    StoreError(String),
    
    #[error("Sync error: {0}")]
    SyncError(String),
    
    #[error("DAG error: {0}")]
    DagError(String),
}

pub type AgentResult<T> = Result<T, AgentError>; 