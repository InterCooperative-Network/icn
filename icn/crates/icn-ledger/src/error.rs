//! Error types for ICN Ledger.

use thiserror::Error;

/// Result type for ledger operations
pub type Result<T> = std::result::Result<T, LedgerError>;

/// Errors that can occur in the ledger
#[derive(Debug, Error)]
pub enum LedgerError {
    /// Entry not found
    #[error("entry not found: {0}")]
    EntryNotFound(String),

    /// The entry id already names a journal record with the same content.
    ///
    /// A refusal, not an idempotent success. The id covers no provenance and only a millisecond
    /// timestamp, so two separate events can produce byte-identical entries; applying the second
    /// would double its deltas, and answering `Ok` without applying it would drop it silently.
    /// Same record is not same operation: recognising a retry of one operation needs an
    /// operation identity chosen by the author, which this check does not provide. Nothing was
    /// written.
    #[error("journal entry {0} is already journaled; refusing to append it again")]
    EntryAlreadyJournaled(String),

    /// The entry id already names a journal record with different content (or one that does
    /// not parse). The journaled record is left as it is; nothing was written.
    #[error(
        "journal entry id {0} is already journaled with different content; refusing to overwrite it"
    )]
    EntryIdCollision(String),

    /// Two entries of one batch share an id. Nothing in the batch was written.
    #[error("journal entry id {0} appears more than once in this batch; refusing the batch")]
    EntryIdRepeatedInBatch(String),

    /// Invalid entry (double-entry invariant violated)
    #[error("invalid entry: {0}")]
    InvalidEntry(String),

    /// Credit limit exceeded
    #[error("credit limit exceeded for {account} in {currency}: balance would be {would_be}, limit is {limit}")]
    CreditLimitExceeded {
        account: String,
        currency: String,
        would_be: i64,
        limit: i64,
    },

    /// Account frozen
    #[error("account frozen: {0}")]
    AccountFrozen(String),

    /// Fork detected
    #[error("fork detected: {0}")]
    ForkDetected(String),

    /// Fork resolution failed
    #[error("fork resolution failed: {0}")]
    ForkResolutionFailed(String),

    /// Trust service required
    #[error("trust service required for this operation")]
    TrustServiceRequired,

    /// Duplicate entry
    #[error("duplicate entry: {0}")]
    DuplicateEntry(String),

    /// Invalid hash
    #[error("invalid hash: {0}")]
    InvalidHash(String),

    /// Signature verification failed
    #[error("signature verification failed: {0}")]
    SignatureInvalid(String),

    /// Entry quarantined
    #[error("entry quarantined: {reason}")]
    Quarantined { hash: String, reason: String },

    /// Dispute error
    #[error("dispute error: {0}")]
    Dispute(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Storage error
    #[error("storage error: {0}")]
    Storage(String),

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),

    /// Arithmetic overflow in balance calculation
    #[error("arithmetic overflow: {0}")]
    ArithmeticOverflow(String),
}

impl From<serde_json::Error> for LedgerError {
    fn from(e: serde_json::Error) -> Self {
        LedgerError::Serialization(e.to_string())
    }
}

impl From<icn_encoding::Error> for LedgerError {
    fn from(e: icn_encoding::Error) -> Self {
        LedgerError::Serialization(e.to_string())
    }
}
