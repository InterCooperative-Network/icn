//! Federation error types

use thiserror::Error;

/// Result type for federation operations
pub type Result<T> = std::result::Result<T, FederationError>;

/// Errors that can occur in federation operations
#[derive(Error, Debug)]
pub enum FederationError {
    // Registry errors
    #[error("Cooperative not found: {0}")]
    CooperativeNotFound(String),

    #[error("Cooperative already registered: {0}")]
    CooperativeAlreadyExists(String),

    #[error("Invalid cooperative ID: {0}")]
    InvalidCooperativeId(String),

    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    #[error("Insufficient vouches: required {required}, got {actual}")]
    InsufficientVouches { required: u8, actual: u8 },

    #[error("Cannot vouch: {0}")]
    VouchNotAllowed(String),

    // Signature errors
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Missing signature")]
    MissingSignature,

    // Attestation errors
    #[error("Attestation not found for DID: {0}")]
    AttestationNotFound(String),

    #[error("Attestation expired")]
    AttestationExpired,

    #[error("Invalid attestation: {0}")]
    InvalidAttestation(String),

    #[error("Attestation rate limit exceeded")]
    AttestationRateLimitExceeded,

    // Clearing errors
    #[error("Clearing agreement not found: {0}")]
    ClearingAgreementNotFound(String),

    #[error("Clearing agreement already exists between {0} and {1}")]
    ClearingAgreementExists(String, String),

    #[error("Transfer not found: {0}")]
    TransferNotFound(String),

    #[error("Imbalance limit exceeded: max {max}, current {current}")]
    ImbalanceLimitExceeded { max: i64, current: i64 },

    #[error("Invalid transfer: {0}")]
    InvalidTransfer(String),

    #[error("Exchange rate not found for {0} -> {1}")]
    ExchangeRateNotFound(String, String),

    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),

    // Channel errors
    #[error("Federation channel not found: {0}")]
    ChannelNotFound(String),

    #[error("Channel connection failed: {0}")]
    ChannelConnectionFailed(String),

    #[error("Channel timeout: {0}")]
    ChannelTimeout(String),

    #[error("Maximum channels exceeded: {0}")]
    MaxChannelsExceeded(usize),

    // DID resolution errors
    #[error("DID resolution failed: {0}")]
    DidResolutionFailed(String),

    #[error("Invalid DID format: {0}")]
    InvalidDidFormat(String),

    #[error("DID not found: {0}")]
    DidNotFound(String),

    // Gossip errors
    #[error("Gossip publish failed: {0}")]
    GossipPublishFailed(String),

    #[error("Invalid gossip message: {0}")]
    InvalidGossipMessage(String),

    // Storage errors
    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    // Identity errors
    #[error("Identity error: {0}")]
    IdentityError(String),

    // Internal errors
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not initialized: {0}")]
    NotInitialized(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),
}

impl From<serde_json::Error> for FederationError {
    fn from(err: serde_json::Error) -> Self {
        FederationError::SerializationError(err.to_string())
    }
}

impl From<ed25519_dalek::SignatureError> for FederationError {
    fn from(_err: ed25519_dalek::SignatureError) -> Self {
        FederationError::SignatureVerificationFailed
    }
}

impl From<anyhow::Error> for FederationError {
    fn from(err: anyhow::Error) -> Self {
        FederationError::Internal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = FederationError::CooperativeNotFound("food-coop".to_string());
        assert_eq!(err.to_string(), "Cooperative not found: food-coop");

        let err = FederationError::InsufficientVouches {
            required: 3,
            actual: 1,
        };
        assert_eq!(err.to_string(), "Insufficient vouches: required 3, got 1");
    }
}
