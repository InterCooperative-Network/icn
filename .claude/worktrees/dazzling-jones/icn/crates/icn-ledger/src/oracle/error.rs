//! Oracle-specific error types

use super::types::CurrencyPair;
use thiserror::Error;

/// Errors that can occur in the exchange rate oracle
#[derive(Debug, Error)]
pub enum OracleError {
    /// Rate not found for the specified currency pair
    #[error("rate not found for pair {0}")]
    RateNotFound(CurrencyPair),

    /// No sources available to provide a rate
    #[error("no sources available for pair {0}")]
    NoSourcesAvailable(CurrencyPair),

    /// A specific source is unavailable
    #[error("source '{0}' unavailable: {1}")]
    SourceUnavailable(String, String),

    /// Error from a rate source
    #[error("source '{0}' error: {1}")]
    SourceError(String, String),

    /// Rate is stale (beyond staleness threshold)
    #[error("rate is stale (last update: {0} seconds ago)")]
    RateStale(u64),

    /// Invalid rate value (e.g., negative, NaN)
    #[error("invalid rate value: {0}")]
    InvalidRate(String),

    /// Not enough sources for consensus
    #[error("consensus failed: insufficient sources ({got} < {required})")]
    InsufficientSources { got: usize, required: usize },

    /// Storage error
    #[error("storage error: {0}")]
    Storage(String),

    /// Serialization/deserialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Rate already exists (for manual rate setting)
    #[error("rate already exists for pair {0}")]
    RateExists(CurrencyPair),

    /// Invalid currency pair
    #[error("invalid currency pair: {0}")]
    InvalidPair(String),

    /// Configuration error
    #[error("configuration error: {0}")]
    Config(String),
}

impl From<serde_json::Error> for OracleError {
    fn from(e: serde_json::Error) -> Self {
        OracleError::Serialization(e.to_string())
    }
}

impl From<OracleError> for crate::LedgerError {
    fn from(e: OracleError) -> Self {
        crate::LedgerError::Internal(e.to_string())
    }
}

/// Result type for oracle operations
pub type OracleResult<T> = std::result::Result<T, OracleError>;
