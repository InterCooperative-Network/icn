//! Error types for the compute layer.

use thiserror::Error;

/// Result type for compute operations
pub type Result<T> = std::result::Result<T, ComputeError>;

/// Errors that can occur in the compute layer
#[derive(Debug, Error)]
pub enum ComputeError {
    /// Task not found
    #[error("task not found: {0}")]
    TaskNotFound(String),

    /// Insufficient trust to perform operation
    #[error("insufficient trust: required {required}, got {actual}")]
    InsufficientTrust { required: f64, actual: f64 },

    /// Task already claimed by another executor
    #[error("task already claimed by {0}")]
    TaskAlreadyClaimed(String),

    /// Task execution failed
    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    /// Out of fuel during execution
    #[error("out of fuel: used {used}, limit {limit}")]
    OutOfFuel { used: u64, limit: u64 },

    /// Task deadline exceeded
    #[error("deadline exceeded")]
    DeadlineExceeded,

    /// Invalid task code
    #[error("invalid code: {0}")]
    InvalidCode(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Missing required capability
    #[error("missing capability: {0:?}")]
    MissingCapability(String),

    /// Signature verification failed
    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    /// Policy violation (Phase 16E)
    #[error("policy violation: {0}")]
    PolicyViolation(String),

    /// Invalid input
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),

    /// Invalid result
    #[error("invalid result: {0}")]
    InvalidResult(String),

    /// Dispute closed
    #[error("dispute closed")]
    DisputeClosed,

    /// Invalid dispute state
    #[error("invalid dispute state")]
    InvalidDisputeState,

    /// Resource not found
    #[error("not found")]
    NotFound,

    /// Lock error
    #[error("lock error")]
    LockError,

    /// Insufficient commons credits for the requested operation (Epic 6 #948).
    /// Accounting infrastructure only — enforcement at scheduling time is deferred.
    #[error("insufficient commons credits: balance={balance}, required={required}")]
    InsufficientCommonsCredits { balance: i64, required: i64 },

    /// WASM module fetch from remote peer failed (Issue #1074).
    /// Distinct from execution failure -- the module was never obtained.
    #[error("module fetch failed for {hash}: {reason}")]
    ModuleFetchFailed { hash: String, reason: String },

    /// WASM module hash mismatch after remote fetch (Issue #1074).
    /// The fetched bytes do not match the expected content hash.
    #[error("module hash mismatch: expected {expected}, got {actual}")]
    ModuleHashMismatch { expected: String, actual: String },
}

impl From<icn_encoding::Error> for ComputeError {
    fn from(e: icn_encoding::Error) -> Self {
        ComputeError::Serialization(e.to_string())
    }
}
