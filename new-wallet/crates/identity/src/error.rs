use thiserror::Error;
use std::io;

/// Error type for identity operations
#[derive(Error, Debug)]
pub enum IdentityError {
    /// Cryptographic operations failed
    #[error("Cryptographic error: {0}")]
    CryptoError(String),

    /// Key management error
    #[error("Key management error: {0}")]
    KeyError(String),

    /// Serialization or deserialization failed
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// I/O operation failed
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// Invalid DID format or structure
    #[error("Invalid DID: {0}")]
    InvalidDid(String),

    /// Invalid identity scope
    #[error("Invalid identity scope: {0}")]
    InvalidScope(String),

    /// Identity not found
    #[error("Identity not found: {0}")]
    NotFound(String),

    /// Verification error for signatures or proofs
    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    /// Credential error
    #[error("Credential error: {0}")]
    CredentialError(String),

    /// Permission error
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Other unexpected errors
    #[error("Unexpected error: {0}")]
    Other(String),
}

/// Implement conversion from ed25519-dalek errors
impl From<ed25519_dalek::SignatureError> for IdentityError {
    fn from(err: ed25519_dalek::SignatureError) -> Self {
        IdentityError::CryptoError(format!("Signature error: {}", err))
    }
}

/// Implement conversion from serde_json errors
impl From<serde_json::Error> for IdentityError {
    fn from(err: serde_json::Error) -> Self {
        IdentityError::SerializationError(format!("JSON error: {}", err))
    }
}

/// Result type for identity operations
pub type IdentityResult<T> = std::result::Result<T, IdentityError>; 