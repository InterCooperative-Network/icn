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

    /// A persisted `federation/attestations` row could not be attributed to a
    /// principal (N2-A, #2703). Carries the error class and position, never the
    /// value: the `Did` deserializer echoes the spelling it rejected.
    #[error(
        "Unreadable persisted federation/attestations row ({key_len}-byte key, {value_len}-byte value): {reason}"
    )]
    AttestationStoreUnreadable {
        key_len: usize,
        value_len: usize,
        reason: String,
    },

    /// A persisted `federation/attestations` key is not the key its own value
    /// implies, so the row cannot be attributed to a principal.
    #[error(
        "Persisted federation/attestations key ({key_len} bytes) disagrees with its value: principal {principal_fingerprint}…, source {source_coop_id}"
    )]
    AttestationStoreKeyValueMismatch {
        principal_fingerprint: String,
        source_coop_id: String,
        key_len: usize,
    },

    /// Two persisted `federation/attestations` rows name one principal from one
    /// source cooperative under different spellings. No federation-domain rule
    /// authorizes choosing or combining them, so the operation that would have
    /// interpreted them fails closed. `principal_fingerprint` is eight hex
    /// characters of the identifier — the N2-A scanner's rule — and
    /// `colliding_pairs` counts every such pair the operation saw.
    #[error(
        "Ambiguous federation/attestations rows: {row_count} rows for principal {principal_fingerprint}… from source {source_coop_id} ({colliding_pairs} colliding pair(s)); no federation merge rule is authorized"
    )]
    AttestationStorePrincipalCollision {
        principal_fingerprint: String,
        source_coop_id: String,
        row_count: usize,
        colliding_pairs: usize,
    },

    /// A write would have persisted a second spelling of a
    /// `(principal, source_coop_id)` pair that is already stored — the
    /// collision above, one write early.
    #[error(
        "Refusing federation/attestations write: principal {principal_fingerprint}… from source {source_coop_id} is already persisted under another spelling"
    )]
    AttestationStoreAliasWriteRefused {
        principal_fingerprint: String,
        source_coop_id: String,
    },

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

    // Agreement errors
    #[error("Agreement not found: {0}")]
    AgreementNotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Configuration error: {0}")]
    Config(String),

    // Internal errors
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not initialized: {0}")]
    NotInitialized(String),

    // ----- Agreement store: canonical rows and the party-index projection (N2-A, #2627) -----
    /// A persisted `federation/agreements/` row could not be read as an
    /// `Agreement`. Carries the error class and position, never the value: the
    /// `Did` deserializer echoes the spelling it rejected. Every operation that
    /// needs the row fails with this rather than treating it as absent.
    #[error(
        "Unreadable persisted federation/agreements row ({key_len}-byte key, {value_len}-byte value): {reason}"
    )]
    AgreementStoreUnreadable {
        key_len: usize,
        value_len: usize,
        reason: String,
    },

    /// `idx_agreement_party/` holds rows the agreement store could never have
    /// written: a key that does not parse as
    /// `idx_agreement_party/<did>/<agreement id>`, a spelling that names no
    /// principal, or a value naming a different agreement than the key. Such a
    /// row cannot be attributed to any canonical fact, so operations that
    /// interpret the projection refuse rather than read around it. `rows`
    /// counts every such row; `first_reason` describes one without its bytes.
    /// The remedy is `AgreementStore::rebuild_party_index`, which recomputes
    /// the projection from the canonical rows.
    #[error(
        "Malformed idx_agreement_party/ projection: {rows} row(s) cannot be attributed ({first_reason}); rebuild the party index from the canonical agreement rows"
    )]
    AgreementPartyIndexMalformed { rows: usize, first_reason: String },

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

impl From<String> for FederationError {
    fn from(err: String) -> Self {
        FederationError::Internal(err)
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
