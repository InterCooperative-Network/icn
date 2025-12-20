//! JSON-RPC error codes
//!
//! This module defines standard JSON-RPC error codes and ICN-specific
//! application error codes. Using constants instead of magic numbers
//! improves code readability and consistency.
//!
//! # Error Code Ranges
//!
//! - `-32700` to `-32600`: Standard JSON-RPC errors (parse, invalid request, etc.)
//! - `-32099` to `-32000`: Server errors (reserved for implementation)
//! - `-31999` to `-31000`: ICN application errors
//!
//! # Usage
//!
//! ```rust
//! use icn_rpc::error_codes::{RpcErrorCode, RESOURCE_NOT_AVAILABLE};
//! use icn_rpc::types::RpcResponse;
//!
//! // Using the enum
//! let response = RpcErrorCode::InvalidParams.to_response(1, "Missing 'account_id' field");
//!
//! // Using constants for common cases
//! let response = RpcResponse::error(1, RESOURCE_NOT_AVAILABLE, "Ledger not available".into());
//! ```

use crate::types::RpcResponse;

// =============================================================================
// Standard JSON-RPC Error Codes (defined in JSON-RPC 2.0 spec)
// =============================================================================

/// Parse error: Invalid JSON was received by the server
pub const PARSE_ERROR: i32 = -32700;

/// Invalid Request: The JSON sent is not a valid Request object
pub const INVALID_REQUEST: i32 = -32600;

/// Method not found: The method does not exist / is not available
pub const METHOD_NOT_FOUND: i32 = -32601;

/// Invalid params: Invalid method parameter(s)
pub const INVALID_PARAMS: i32 = -32602;

/// Internal error: Internal JSON-RPC error
pub const INTERNAL_ERROR: i32 = -32603;

// =============================================================================
// Server Error Codes (-32099 to -32000, reserved for implementation)
// =============================================================================

/// Resource not available (ledger, trust graph, etc.)
pub const RESOURCE_NOT_AVAILABLE: i32 = -32000;

/// Authentication required
pub const AUTHENTICATION_REQUIRED: i32 = -32001;

/// Permission denied (insufficient trust, missing capability)
pub const PERMISSION_DENIED: i32 = -32002;

/// Rate limited
pub const RATE_LIMITED: i32 = -32003;

/// Resource not found (specific entry, proposal, etc.)
pub const NOT_FOUND: i32 = -32004;

/// Operation conflict (duplicate, already exists, etc.)
pub const CONFLICT: i32 = -32005;

/// Validation failed (business logic validation)
pub const VALIDATION_FAILED: i32 = -32006;

/// Operation timeout
pub const TIMEOUT: i32 = -32007;

/// Service unavailable (temporary)
pub const SERVICE_UNAVAILABLE: i32 = -32008;

// =============================================================================
// ICN Application Error Codes (-31999 to -31000)
// =============================================================================

/// Ledger operation failed
pub const LEDGER_ERROR: i32 = -31000;

/// Trust graph operation failed
pub const TRUST_ERROR: i32 = -31001;

/// Governance operation failed
pub const GOVERNANCE_ERROR: i32 = -31002;

/// Contract execution failed
pub const CONTRACT_ERROR: i32 = -31003;

/// Compute task failed
pub const COMPUTE_ERROR: i32 = -31004;

/// Network operation failed
pub const NETWORK_ERROR: i32 = -31005;

/// Identity operation failed
pub const IDENTITY_ERROR: i32 = -31006;

/// Federation operation failed
pub const FEDERATION_ERROR: i32 = -31007;

/// Recovery operation failed
pub const RECOVERY_ERROR: i32 = -31008;

/// Dispute operation failed
pub const DISPUTE_ERROR: i32 = -31009;

// =============================================================================
// RpcErrorCode Enum for type-safe error handling
// =============================================================================

/// Strongly-typed RPC error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcErrorCode {
    // Standard JSON-RPC errors
    /// Parse error: Invalid JSON
    ParseError,
    /// Invalid Request object
    InvalidRequest,
    /// Method does not exist
    MethodNotFound,
    /// Invalid method parameters
    InvalidParams,
    /// Internal JSON-RPC error
    InternalError,

    // Server errors
    /// Resource not available
    ResourceNotAvailable,
    /// Authentication required
    AuthenticationRequired,
    /// Permission denied
    PermissionDenied,
    /// Rate limited
    RateLimited,
    /// Resource not found
    NotFound,
    /// Operation conflict
    Conflict,
    /// Validation failed
    ValidationFailed,
    /// Operation timeout
    Timeout,
    /// Service unavailable
    ServiceUnavailable,

    // Application errors
    /// Ledger operation failed
    LedgerError,
    /// Trust graph operation failed
    TrustError,
    /// Governance operation failed
    GovernanceError,
    /// Contract execution failed
    ContractError,
    /// Compute task failed
    ComputeError,
    /// Network operation failed
    NetworkError,
    /// Identity operation failed
    IdentityError,
    /// Federation operation failed
    FederationError,
    /// Recovery operation failed
    RecoveryError,
    /// Dispute operation failed
    DisputeError,
}

impl RpcErrorCode {
    /// Get the numeric error code
    pub fn code(&self) -> i32 {
        match self {
            // Standard JSON-RPC errors
            RpcErrorCode::ParseError => PARSE_ERROR,
            RpcErrorCode::InvalidRequest => INVALID_REQUEST,
            RpcErrorCode::MethodNotFound => METHOD_NOT_FOUND,
            RpcErrorCode::InvalidParams => INVALID_PARAMS,
            RpcErrorCode::InternalError => INTERNAL_ERROR,

            // Server errors
            RpcErrorCode::ResourceNotAvailable => RESOURCE_NOT_AVAILABLE,
            RpcErrorCode::AuthenticationRequired => AUTHENTICATION_REQUIRED,
            RpcErrorCode::PermissionDenied => PERMISSION_DENIED,
            RpcErrorCode::RateLimited => RATE_LIMITED,
            RpcErrorCode::NotFound => NOT_FOUND,
            RpcErrorCode::Conflict => CONFLICT,
            RpcErrorCode::ValidationFailed => VALIDATION_FAILED,
            RpcErrorCode::Timeout => TIMEOUT,
            RpcErrorCode::ServiceUnavailable => SERVICE_UNAVAILABLE,

            // Application errors
            RpcErrorCode::LedgerError => LEDGER_ERROR,
            RpcErrorCode::TrustError => TRUST_ERROR,
            RpcErrorCode::GovernanceError => GOVERNANCE_ERROR,
            RpcErrorCode::ContractError => CONTRACT_ERROR,
            RpcErrorCode::ComputeError => COMPUTE_ERROR,
            RpcErrorCode::NetworkError => NETWORK_ERROR,
            RpcErrorCode::IdentityError => IDENTITY_ERROR,
            RpcErrorCode::FederationError => FEDERATION_ERROR,
            RpcErrorCode::RecoveryError => RECOVERY_ERROR,
            RpcErrorCode::DisputeError => DISPUTE_ERROR,
        }
    }

    /// Get a default message for this error code
    pub fn default_message(&self) -> &'static str {
        match self {
            // Standard JSON-RPC errors
            RpcErrorCode::ParseError => "Parse error",
            RpcErrorCode::InvalidRequest => "Invalid request",
            RpcErrorCode::MethodNotFound => "Method not found",
            RpcErrorCode::InvalidParams => "Invalid params",
            RpcErrorCode::InternalError => "Internal error",

            // Server errors
            RpcErrorCode::ResourceNotAvailable => "Resource not available",
            RpcErrorCode::AuthenticationRequired => "Authentication required",
            RpcErrorCode::PermissionDenied => "Permission denied",
            RpcErrorCode::RateLimited => "Rate limited",
            RpcErrorCode::NotFound => "Not found",
            RpcErrorCode::Conflict => "Conflict",
            RpcErrorCode::ValidationFailed => "Validation failed",
            RpcErrorCode::Timeout => "Operation timeout",
            RpcErrorCode::ServiceUnavailable => "Service unavailable",

            // Application errors
            RpcErrorCode::LedgerError => "Ledger error",
            RpcErrorCode::TrustError => "Trust graph error",
            RpcErrorCode::GovernanceError => "Governance error",
            RpcErrorCode::ContractError => "Contract error",
            RpcErrorCode::ComputeError => "Compute error",
            RpcErrorCode::NetworkError => "Network error",
            RpcErrorCode::IdentityError => "Identity error",
            RpcErrorCode::FederationError => "Federation error",
            RpcErrorCode::RecoveryError => "Recovery error",
            RpcErrorCode::DisputeError => "Dispute error",
        }
    }

    /// Create an RPC error response with a custom message
    pub fn to_response(&self, id: u64, message: &str) -> RpcResponse {
        RpcResponse::error(id, self.code(), message.to_string())
    }

    /// Create an RPC error response with the default message
    pub fn to_default_response(&self, id: u64) -> RpcResponse {
        RpcResponse::error(id, self.code(), self.default_message().to_string())
    }

    /// Create an RPC error response with a formatted message
    pub fn to_response_fmt(&self, id: u64, message: impl std::fmt::Display) -> RpcResponse {
        RpcResponse::error(id, self.code(), message.to_string())
    }
}

impl From<RpcErrorCode> for i32 {
    fn from(code: RpcErrorCode) -> i32 {
        code.code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_values() {
        assert_eq!(RpcErrorCode::ParseError.code(), -32700);
        assert_eq!(RpcErrorCode::InvalidParams.code(), -32602);
        assert_eq!(RpcErrorCode::ResourceNotAvailable.code(), -32000);
        assert_eq!(RpcErrorCode::LedgerError.code(), -31000);
    }

    #[test]
    fn test_error_response_creation() {
        let response = RpcErrorCode::InvalidParams.to_response(1, "Missing 'account_id'");
        assert_eq!(response.id, 1);
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32602);
        assert_eq!(error.message, "Missing 'account_id'");
    }

    #[test]
    fn test_default_message() {
        assert_eq!(RpcErrorCode::NotFound.default_message(), "Not found");
        assert_eq!(
            RpcErrorCode::ResourceNotAvailable.default_message(),
            "Resource not available"
        );
    }

    #[test]
    fn test_into_i32() {
        let code: i32 = RpcErrorCode::InternalError.into();
        assert_eq!(code, -32603);
    }
}
