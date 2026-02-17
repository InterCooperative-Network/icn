//! Escrow types for the kernel execution layer.
//!
//! An escrow is a locked allocation that can only be released by a confirmed
//! governance decision within the escrow's governing scope. The kernel enforces
//! the lock; the app layer decides when to release.
//!
//! # Saga State Machine
//!
//! ```text
//! Locked → Releasing(decision_hash) → Released(decision_hash)
//!                                   ↘ (crash) → Releasing → retry
//! ```
//!
//! The `Releasing` state prevents the "state flip before side-effect" bug:
//! - `begin_release()` transitions to `Releasing` (intent recorded)
//! - Ledger mutation happens
//! - `confirm_release()` transitions to `Released` (side-effect confirmed)
//!
//! On crash between `Releasing` and `Released`, replay detects `Releasing`
//! with the same `decision_hash` and re-attempts the ledger mutation.
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
    /// Release in progress — intent recorded, ledger mutation pending.
    /// On crash recovery, this state triggers a retry of the ledger mutation.
    Releasing,
    /// Funds have been released to the beneficiary (ledger mutation confirmed).
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

    /// Phase 1 of the saga: record intent to release.
    ///
    /// Transitions `Locked → Releasing` and records the `decision_hash`.
    /// This MUST be persisted BEFORE attempting the ledger mutation.
    ///
    /// If the escrow is already `Releasing` with the same decision_hash,
    /// returns `Ok(BeginReleaseOutcome::RetryNeeded)` — the caller should
    /// re-attempt the ledger mutation (crash recovery path).
    ///
    /// If the escrow is already `Released` with the same decision_hash,
    /// returns `Err(AlreadyReleasedSameDecision)` — fully idempotent.
    pub fn begin_release(
        &mut self,
        decision_hash: &str,
    ) -> Result<BeginReleaseOutcome, EscrowReleaseError> {
        match self.status {
            EscrowStatus::Locked => {
                self.status = EscrowStatus::Releasing;
                self.release_decision_hash = Some(decision_hash.to_string());
                Ok(BeginReleaseOutcome::Proceed)
            }
            EscrowStatus::Releasing => {
                let existing_hash = self.release_decision_hash.as_deref().unwrap_or("unknown");
                if existing_hash == decision_hash {
                    // Same decision in Releasing state — crash recovery.
                    // The ledger mutation may not have completed. Caller should retry.
                    Ok(BeginReleaseOutcome::RetryNeeded)
                } else {
                    // Different decision trying to release — conflict.
                    Err(EscrowReleaseError::AlreadyReleasedByOther {
                        existing_decision_hash: existing_hash.to_string(),
                    })
                }
            }
            EscrowStatus::Released => {
                let existing_hash = self.release_decision_hash.as_deref().unwrap_or("unknown");
                if existing_hash == decision_hash {
                    Err(EscrowReleaseError::AlreadyReleasedSameDecision)
                } else {
                    Err(EscrowReleaseError::AlreadyReleasedByOther {
                        existing_decision_hash: existing_hash.to_string(),
                    })
                }
            }
            EscrowStatus::Cancelled => Err(EscrowReleaseError::Cancelled),
        }
    }

    /// Phase 2 of the saga: confirm release after successful ledger mutation.
    ///
    /// Transitions `Releasing → Released` and sets `released_at`.
    /// Only valid when status is `Releasing`.
    pub fn confirm_release(&mut self) {
        debug_assert_eq!(
            self.status,
            EscrowStatus::Releasing,
            "confirm_release called on non-Releasing escrow"
        );
        self.status = EscrowStatus::Released;
        self.released_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
    }
}

/// Outcome of `begin_release()` — tells the caller what to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeginReleaseOutcome {
    /// Fresh release — proceed with ledger mutation.
    Proceed,
    /// Crash recovery — the escrow was already in `Releasing` state
    /// with the same decision_hash. Re-attempt the ledger mutation.
    RetryNeeded,
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
    fn test_saga_begin_and_confirm() {
        let mut escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );

        // Phase 1: begin_release → Releasing
        let outcome = escrow.begin_release("hash-1").unwrap();
        assert_eq!(outcome, BeginReleaseOutcome::Proceed);
        assert_eq!(escrow.status, EscrowStatus::Releasing);
        assert_eq!(escrow.release_decision_hash.as_deref(), Some("hash-1"));
        assert!(escrow.released_at.is_none()); // Not yet finalized

        // Phase 2: confirm_release → Released
        escrow.confirm_release();
        assert_eq!(escrow.status, EscrowStatus::Released);
        assert!(escrow.released_at.is_some());
    }

    #[test]
    fn test_saga_crash_recovery_same_decision() {
        let mut escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );

        // Phase 1: begin_release → Releasing
        escrow.begin_release("hash-1").unwrap();
        assert_eq!(escrow.status, EscrowStatus::Releasing);

        // Simulate crash (no confirm_release). On recovery, same decision retries:
        let outcome = escrow.begin_release("hash-1").unwrap();
        assert_eq!(
            outcome,
            BeginReleaseOutcome::RetryNeeded,
            "Same decision in Releasing should signal retry"
        );
    }

    #[test]
    fn test_saga_releasing_conflict_different_decision() {
        let mut escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );

        // Phase 1: begin_release with decision A
        escrow.begin_release("hash-A").unwrap();
        assert_eq!(escrow.status, EscrowStatus::Releasing);

        // Different decision B tries during Releasing → conflict
        let err = escrow.begin_release("hash-B").unwrap_err();
        assert_eq!(
            err,
            EscrowReleaseError::AlreadyReleasedByOther {
                existing_decision_hash: "hash-A".to_string()
            }
        );
    }

    #[test]
    fn test_released_idempotent_same_decision() {
        let mut escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );
        escrow.begin_release("hash-1").unwrap();
        escrow.confirm_release();

        // Same decision on fully Released escrow → idempotent
        let err = escrow.begin_release("hash-1").unwrap_err();
        assert_eq!(err, EscrowReleaseError::AlreadyReleasedSameDecision);
    }

    #[test]
    fn test_released_conflict_different_decision() {
        let mut escrow = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );
        escrow.begin_release("hash-1").unwrap();
        escrow.confirm_release();

        let err = escrow.begin_release("hash-2").unwrap_err();
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

        let err = escrow.begin_release("hash-1").unwrap_err();
        assert_eq!(err, EscrowReleaseError::Cancelled);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut escrow =
            EscrowRecord::new_locked("esc-1", "scope", "funder", "beneficiary", 1000, "USD");
        escrow.begin_release("hash-x").unwrap();
        escrow.confirm_release();

        let json = serde_json::to_string(&escrow).unwrap();
        let parsed: EscrowRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, EscrowStatus::Released);
        assert_eq!(parsed.release_decision_hash.as_deref(), Some("hash-x"));
    }

    #[test]
    fn test_serde_roundtrip_releasing() {
        let mut escrow =
            EscrowRecord::new_locked("esc-2", "scope", "funder", "beneficiary", 2000, "USD");
        escrow.begin_release("hash-y").unwrap();
        // Deliberately NOT calling confirm_release — testing Releasing state

        let json = serde_json::to_string(&escrow).unwrap();
        let parsed: EscrowRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, EscrowStatus::Releasing);
        assert_eq!(parsed.release_decision_hash.as_deref(), Some("hash-y"));
        assert!(parsed.released_at.is_none());
    }
}
