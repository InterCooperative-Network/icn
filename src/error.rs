use thiserror::Error;

pub type FederationResult<T> = Result<T, FederationError>;

#[derive(Error, Debug)]
pub enum FederationError {
    #[error("Cryptographic operation failed: {0}")]
    CryptoError(String),
    
    #[error("Invalid signature: {0}")]
    SignatureError(String),
    
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    #[error("Federation bootstrap error: {0}")]
    BootstrapError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("CID operation error: {0}")]
    CidError(String),
    
    #[error("DAG operation error: {0}")]
    DagError(String),
    
    #[error("Item not found: {0}")]
    NotFound(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Quorum not met: {0}")]
    QuorumError(String),
    
    #[error("Federation policy violation: {0}")]
    PolicyViolation(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<std::io::Error> for FederationError {
    fn from(err: std::io::Error) -> Self {
        FederationError::StorageError(err.to_string())
    }
}

impl From<serde_json::Error> for FederationError {
    fn from(err: serde_json::Error) -> Self {
        FederationError::SerializationError(err.to_string())
    }
} 