use thiserror::Error;
use wallet_core::error::WalletError as CoreError;
use wallet_agent::error::AgentError;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    #[error("DAG error: {0}")]
    DagError(String),
    
    #[error("CID error: {0}")]
    CidError(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Core wallet error: {0}")]
    CoreError(#[from] CoreError),
    
    #[error("Agent error: {0}")]
    AgentError(#[from] AgentError),
    
    #[error("HTTP client error: {0}")]
    HttpError(String),
}

pub type SyncResult<T> = Result<T, SyncError>; 