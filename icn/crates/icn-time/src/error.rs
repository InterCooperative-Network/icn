//! Error types for clock synchronization

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TimeError {
    #[error("Timestamp out of acceptable range: {0}s skew (max: {1}s)")]
    TimestampOutOfRange(i64, u64),

    #[error("Clock sync failed: {0}")]
    SyncFailed(String),

    #[error("No time servers responded successfully")]
    NoServersAvailable,

    #[error("Insufficient responses for median calculation (got {0}, need {1})")]
    InsufficientResponses(usize, usize),

    #[error("Network error: {0}")]
    Network(#[from] std::io::Error),

    #[error("Rough Time protocol error: {0}")]
    Protocol(String),

    #[error("Clock not synchronized yet")]
    NotSynchronized,

    #[error("System clock error: time before UNIX epoch")]
    SystemClockError,

    #[error("Invalid timestamp: {0}ms exceeds safe range for offset calculation")]
    InvalidTimestamp(u64),
}

pub type Result<T> = std::result::Result<T, TimeError>;
