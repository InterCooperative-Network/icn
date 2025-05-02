use thiserror::Error;
use wallet_core::error::WalletError as CoreError;

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
    
    #[error("Core wallet error: {0}")]
    CoreError(#[from] CoreError),
}

pub type AgentResult<T> = Result<T, AgentError>; 