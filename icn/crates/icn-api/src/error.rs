//! API error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Compute not available")]
    ComputeNotAvailable,

    #[error("Governance not available")]
    GovernanceNotAvailable,

    #[error("Ledger not available")]
    LedgerNotAvailable,

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Not authenticated")]
    NotAuthenticated,

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Compute error: {0}")]
    ComputeError(String),

    #[error("Governance error: {0}")]
    GovernanceError(String),

    #[error("Ledger error: {0}")]
    LedgerError(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl ApiError {
    /// Convert to RPC error code
    pub fn to_rpc_code(&self) -> i32 {
        match self {
            ApiError::ComputeNotAvailable => -32000,
            ApiError::GovernanceNotAvailable => -32000,
            ApiError::LedgerNotAvailable => -32000,
            ApiError::InvalidParameter(_) => -32602,
            ApiError::ValidationError(_) => -32602,
            ApiError::NotAuthenticated => -32001,
            ApiError::TaskNotFound(_) => -32000,
            ApiError::ComputeError(_) => -32000,
            ApiError::GovernanceError(_) => -32000,
            ApiError::LedgerError(_) => -32000,
            ApiError::Internal(_) => -32603,
        }
    }
}
