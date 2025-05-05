/*!
# Federation Error Handling

This module provides comprehensive error types and handling for the federation layer.
*/

use thiserror::Error;
use std::fmt;

/// Result type for federation operations
pub type FederationResult<T> = Result<T, FederationError>;

/// Error types that can occur during federation operations
#[derive(Error, Debug)]
pub enum FederationError {
    /// Network error occurred
    #[error("Network error: {0}")]
    NetworkError(String),
    
    /// Peer error occurred
    #[error("Peer error: {0}")]
    PeerError(String),
    
    /// Storage error occurred
    #[error("Storage error: {0}")]
    StorageError(String),
    
    /// Serialization error occurred
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// Trust bundle error occurred
    #[error("Trust bundle error: {kind} - {details}")]
    TrustBundleError {
        kind: TrustBundleErrorKind,
        details: String,
    },
    
    /// Authentication error occurred
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    /// Authorization error occurred
    #[error("Authorization error: {0}")]
    AuthorizationError(String),
    
    /// Timeout error occurred
    #[error("Timeout error: {0}")]
    TimeoutError(String),
    
    /// Internal error occurred
    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Specific kinds of trust bundle errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustBundleErrorKind {
    /// Bundle not found
    NotFound,
    /// Bundle verification failed
    VerificationFailed,
    /// Bundle is invalid
    Invalid,
    /// Bundle is expired
    Expired,
    /// Bundle has invalid epoch
    InvalidEpoch,
    /// Bundle is already present
    AlreadyPresent,
}

impl fmt::Display for TrustBundleErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "NotFound"),
            Self::VerificationFailed => write!(f, "VerificationFailed"),
            Self::Invalid => write!(f, "Invalid"),
            Self::Expired => write!(f, "Expired"),
            Self::InvalidEpoch => write!(f, "InvalidEpoch"),
            Self::AlreadyPresent => write!(f, "AlreadyPresent"),
        }
    }
}

/// Extensions for FederationResult to improve error handling
pub trait FederationResultExt<T> {
    /// Add context to an error
    fn with_context<C, F>(self, context: F) -> FederationResult<T>
    where
        C: fmt::Display,
        F: FnOnce() -> C;
    
    /// Wrap network errors
    fn network_context<C, F>(self, context: F) -> FederationResult<T>
    where
        C: fmt::Display,
        F: FnOnce() -> C;
    
    /// Wrap storage errors
    fn storage_context<C, F>(self, context: F) -> FederationResult<T>
    where
        C: fmt::Display,
        F: FnOnce() -> C;
}

impl<T, E> FederationResultExt<T> for Result<T, E>
where
    E: std::error::Error + 'static,
{
    fn with_context<C, F>(self, context: F) -> FederationResult<T>
    where
        C: fmt::Display,
        F: FnOnce() -> C,
    {
        self.map_err(|e| {
            let ctx = context();
            FederationError::InternalError(format!("{}: {}", ctx, e))
        })
    }
    
    fn network_context<C, F>(self, context: F) -> FederationResult<T>
    where
        C: fmt::Display,
        F: FnOnce() -> C,
    {
        self.map_err(|e| {
            let ctx = context();
            FederationError::NetworkError(format!("{}: {}", ctx, e))
        })
    }
    
    fn storage_context<C, F>(self, context: F) -> FederationResult<T>
    where
        C: fmt::Display,
        F: FnOnce() -> C,
    {
        self.map_err(|e| {
            let ctx = context();
            FederationError::StorageError(format!("{}: {}", ctx, e))
        })
    }
} 