use thiserror::Error;

/// Errors that can occur in escrow operations
#[derive(Error, Debug)]
pub enum EscrowError {
    #[error("Contract error: {0}")]
    ContractError(String),
    
    #[error("Token error: {0}")]
    TokenError(String),
    
    #[error("State error: {0}")]
    StateError(String),
    
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    #[error("Distribution error: {0}")]
    DistributionError(String),
    
    #[error("Dispute error: {0}")]
    DisputeError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for escrow operations
pub type EscrowResult<T> = Result<T, EscrowError>; 