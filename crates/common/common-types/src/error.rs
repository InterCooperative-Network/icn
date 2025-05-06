//! Common error types for the ICN project
//!
//! This module provides a unified error handling approach for all ICN components.

use std::fmt::{self, Display};
use thiserror::Error;

/// Common error type for ICN components
#[derive(Debug, Error)]
pub enum Error {
    /// An error occurred during serialization or deserialization
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// An I/O error occurred
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A validation error occurred
    #[error("Validation error: {0}")]
    Validation(String),

    /// A network error occurred
    #[error("Network error: {0}")]
    Network(String),

    /// A database or storage error occurred
    #[error("Storage error: {0}")]
    Storage(String),

    /// An error occurred during cryptographic operations
    #[error("Crypto error: {0}")]
    Crypto(String),

    /// An error occurred with identity operations
    #[error("Identity error: {0}")]
    Identity(String),
    
    /// An unexpected or internal error occurred
    #[error("Internal error: {0}")]
    Internal(String),

    /// A custom error with arbitrary context
    #[error("{0}")]
    Custom(String),
}

/// Result type alias using the common Error type
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Create a new validation error with the given message
    pub fn validation<S: Into<String>>(msg: S) -> Self {
        Error::Validation(msg.into())
    }

    /// Create a new network error with the given message
    pub fn network<S: Into<String>>(msg: S) -> Self {
        Error::Network(msg.into())
    }

    /// Create a new storage error with the given message
    pub fn storage<S: Into<String>>(msg: S) -> Self {
        Error::Storage(msg.into())
    }

    /// Create a new crypto error with the given message
    pub fn crypto<S: Into<String>>(msg: S) -> Self {
        Error::Crypto(msg.into())
    }

    /// Create a new identity error with the given message
    pub fn identity<S: Into<String>>(msg: S) -> Self {
        Error::Identity(msg.into())
    }

    /// Create a new internal error with the given message
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Error::Internal(msg.into())
    }

    /// Create a new custom error with the given message
    pub fn custom<S: Into<String>>(msg: S) -> Self {
        Error::Custom(msg.into())
    }
} 