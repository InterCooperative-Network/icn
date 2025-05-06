use thiserror::Error;
use anyhow;

/// Errors that can occur during identity operations
#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("Invalid DID: {0}")]
    InvalidDid(String),
    
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    
    #[error("Invalid credential: {0}")]
    InvalidCredential(String),

    #[error("Invalid proof type")]
    InvalidProofType,
    
    #[error("Scope violation: {0}")]
    ScopeViolation(String),

    #[error("ZK verification failed: {0}")]
    ZkVerificationFailed(String),

    #[error("Keypair generation failed: {0}")]
    KeypairGenerationFailed(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Verification error: {0}")]
    VerificationError(String),

    #[error("Unknown error: {0}")]
    Unknown(String),

    #[error("Key storage error: {0}")]
    KeyStorageError(String),

    #[error("Metadata storage error: {0}")]
    MetadataStorageError(String),

    #[error("DID resolution error: {0}")]
    DidResolutionError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Internal error: {0}")]
    InternalError(#[from] anyhow::Error), // Allow conversion from anyhow
}

/// Result type for identity operations
pub type IdentityResult<T> = Result<T, IdentityError>; 