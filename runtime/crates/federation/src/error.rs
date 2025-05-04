use thiserror::Error;
use cid::Cid;

/// Errors that can occur in the federation system
#[derive(Error, Debug)]
pub enum FederationError {
    /// Error during verification of a cryptographic signature or proof
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    /// Error during resolution of a DID
    #[error("DID resolution error: {0}")]
    DidResolutionError(String),
    
    /// Error due to invalid TrustBundle
    #[error("Invalid TrustBundle: {0}")]
    InvalidTrustBundle(String),
    
    /// Error due to unauthorized operation
    #[error("Unauthorized operation: {0}")]
    Unauthorized(String),
    
    /// Error during federation bootstrap
    #[error("Federation bootstrap error: {0}")]
    BootstrapError(String),
    
    /// Error involving Guardians
    #[error("Guardian error: {0}")]
    GuardianError(String),
    
    /// Error related to a missing CID or invalid DAG reference
    #[error("CID error: {0}")]
    CidError(String),
    
    /// Error related to serialization/deserialization
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),
    
    /// Error with incorrect epoch
    #[error("Epoch error: {0}")]
    EpochError(String),
    
    /// Error with cryptographic operations
    #[error("Crypto error: {0}")]
    CryptoError(String),
    
    /// Error when a resource is not found
    #[error("Not found: {0}")]
    NotFound(String),
    
    /// Error during validation of data or operations
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    /// Any other error
    #[error("Other error: {0}")]
    Other(String),
}

/// Result type for federation operations
pub type FederationResult<T> = Result<T, FederationError>; 