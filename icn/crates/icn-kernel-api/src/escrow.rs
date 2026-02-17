//! Escrow types for the kernel execution layer.
//!
//! An escrow is a locked allocation that can only be released by a confirmed
//! governance decision within the escrow's governing scope. The kernel enforces
//! the lock; the app layer decides when to release.
//!
//! # Domain-Level Idempotency
//!
//! `EscrowRecord.release_decision_hash` provides the second idempotency lock:
//! - Decision-level: `ExecutionStore` prevents replaying the same decision
//! - Domain-level: `EscrowStore` prevents releasing the same escrow twice
//!
//! Together they handle: replayed events, crash recovery, and conflicting releases.

use serde::{Deserialize, Serialize};

/// Status of an escrow allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscrowStatus {
    /// Funds are locked, awaiting governance decision.
    Locked,
    /// Funds have been released to the beneficiary.
    Released,
    /// Escrow was cancelled, funds returned to funder.
    Cancelled,
}

/// A locked allocation that can be released by governance decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowRecord {
    /// Unique escrow identifier.
    pub escrow_id: String,

    /// Governing scope (cooperative DID or federation ID).
    pub scope_id: String,

    /// Who funded the escrow (treasury DID or member DID).
    pub funder_did: String,

    /// Who receives funds on release.
    pub beneficiary_did: String,

    /// Locked amount.
    pub amount: i64,

    /// Currency / asset type.
    pub currency: String,

    /// Current status.
    pub status: EscrowStatus,

    /// Unix timestamp (seconds) when escrow was created.
    pub created_at: u64,

    /// Decision hash that released this escrow (set on release).
    pub release_decision_hash: Option<String>,

    /// Unix timestamp (seconds) when escrow was released (if any).
    pub released_at: Option<u64>,
}

impl EscrowRecord {
    /// Create a new locked escrow.
    pub fn new_locked(
        escrow_id: impl Into<String>,
        scope_id: impl Into<String>,
        funder_did: impl Into<String>,
        beneficiary_did: impl Into<String>,
        amount: i64,
        currency: impl Into<String>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            escrow_id: escrow_id.into(),
            scope_id: scope_id.into(),
            funder_did: funder_did.into(),
            beneficiary_did: beneficiary_did.into(),
            amount,
            currency: currency.into(),
            status: EscrowStatus::Locked,
            created_at: now,
            release_decision_hash: None,
            released_at: None,
        }
    }

    /// Whether this escrow can be released.
    pub fn is_locked(&self) -> bool {
        self.status == EscrowStatus::Locked
    }

    /// Mark as released with the authorizing decision hash.
    ///
    /// Returns `Err` if already released (with the conflicting decision hash).
    pub fn release(&mut self, decision_hash: &str) -> Result<(), EscrowReleaseError> {
        match self.status {
            EscrowStatus::Locked => {
                self.status = EscrowStatus::Released;
                self.release_decision_hash = Some(decision_hash.to_string());
                self.released_at = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                );
                Ok(())
            }
            EscrowStatus::Released => {
                let existing_hash = self.release_decision_hash.as_deref().unwrap_or("unknown");
                if existing_hash == decision_hash {
                    // Same decision — idempotent, already released
                    Err(EscrowReleaseError::AlreadyReleasedSameDecision)
                } else {
                    // Different decision — conflict
                    Err(EscrowReleaseError::AlreadyReleasedByOther {
                        existing_decision_hash: existing_hash.to_string(),
                    })
                }
            }
            EscrowStatus::Cancelled => Err(EscrowReleaseError::Cancelled),
        }
    }
}

/// Error returned when an escrow release fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscrowReleaseError {
    /// Escrow was already released by the same decision (idempotent — not a real error).
    AlreadyReleasedSameDecision,
    /// Escrow was already released by a different decision (conflict).
    AlreadyReleasedByOther { existing_decision_hash: String },
    /// Escrow was cancelled.
    Cancelled,
}

impl std::fmt::Display for EscrowReleaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyReleasedSameDecision => {
                write!(f, "escrow already released by this decision (idempotent)")
            }
            Self::AlreadyReleasedByOther {
                existing_decision_hash,
            } => {
                write!(
                    f,
                    "escrow already released by decision {}",
                    existing_decision_hash
                )
            }
            Self::Cancelled => write!(f, "escrow was cancelled"),
        }
    }
}

/// Trait for persistent escrow storage.
///
/// Implementations must be durable (survive restarts).
/// Keyed by `escrow_id`.
pub trait EscrowStore: Send + Sync {
    /// Get an escrow record by ID.
    fn get(&self, escrow_id: &str) -> anyhow::Result<Option<EscrowRecord>>;

    /// Insert or update an escrow record.
    fn put(&self, record: &EscrowRecord) -> anyhow::Result<()>;

    /// List escrows by scope.
    fn list_by_scope(&self, scope_id: &str) -> anyhow::Result<Vec<EscrowRecord>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_locked() {
        let escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );
        assert!(escrow.is_locked());
        assert_eq!(escrow.status, EscrowStatus::Locked);
        assert!(escrow.release_decision_hash.is_none());
    }

    #[test]
    fn test_release_success() {
        let mut escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );
        assert!(escrow.release("hash-1").is_ok());
        assert_eq!(escrow.status, EscrowStatus::Released);
        assert_eq!(escrow.release_decision_hash.as_deref(), Some("hash-1"));
        assert!(escrow.released_at.is_some());
    }

    #[test]
    fn test_release_idempotent_same_decision() {
        let mut escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );
        escrow.release("hash-1").unwrap();

        let err = escrow.release("hash-1").unwrap_err();
        assert_eq!(err, EscrowReleaseError::AlreadyReleasedSameDecision);
    }

    #[test]
    fn test_release_conflict_different_decision() {
        let mut escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );
        escrow.release("hash-1").unwrap();

        let err = escrow.release("hash-2").unwrap_err();
        assert_eq!(
            err,
            EscrowReleaseError::AlreadyReleasedByOther {
                existing_decision_hash: "hash-1".to_string()
            }
        );
    }

    #[test]
    fn test_release_cancelled() {
        let mut escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );
        escrow.status = EscrowStatus::Cancelled;

        let err = escrow.release("hash-1").unwrap_err();
        assert_eq!(err, EscrowReleaseError::Cancelled);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut escrow =
            EscrowRecord::new_locked("esc-1", "scope", "funder", "beneficiary", 1000, "USD");
        escrow.release("hash-x").unwrap();

        let json = serde_json::to_string(&escrow).unwrap();
        let parsed: EscrowRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, EscrowStatus::Released);
        assert_eq!(parsed.release_decision_hash.as_deref(), Some("hash-x"));
    }
}
