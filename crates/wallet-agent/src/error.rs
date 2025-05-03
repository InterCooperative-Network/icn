use thiserror::Error;
use wallet_core::error::WalletError as CoreError;
use wallet_sync::SyncError;

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
    CoreError(#[from] wallet_core::error::WalletError),
    
    /// Sync error (from wallet-sync)
    #[error("Sync error: {0}")]
    SyncError(String),
    
    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),
    
    /// Store error
    #[error("Store error: {0}")]
    StoreError(String),
    
    /// Processing error
    #[error("Processing error: {0}")]
    ProcessingError(String),
    
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
}

pub type AgentResult<T> = Result<T, AgentError>; 