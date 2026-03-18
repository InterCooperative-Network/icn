//! Budget enforcement types for the execution bridge.
//!
//! These types provide budget tracking at the kernel level, enabling
//! the executor to enforce spending limits before treasury operations
//! reach the ledger.
//!
//! # Saga Pattern
//!
//! Budget spending uses a two-phase saga to prevent the "state flip
//! before side-effect" bug:
//!
//! ```text
//! begin_spend(decision_hash, amount)
//!   → PendingSpend recorded, spent_total NOT yet incremented
//!   → BudgetRecord persisted in "Spending" state
//!   → Ledger mutation happens
//! confirm_spend()
//!   → spent_total incremented by pending amount
//!   → PendingSpend cleared
//!   → BudgetRecord persisted in final state
//! ```
//!
//! If the process crashes between begin_spend and confirm_spend,
//! the budget stays in the Spending state with the PendingSpend
//! recorded. On retry, `begin_spend` detects the pending spend
//! and returns `RetryNeeded` to skip re-persisting.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A kernel-level budget record for enforcement.
///
/// This is deliberately minimal — it tracks only what the kernel needs
/// to enforce: total limit, total spent, and pending saga state.
/// Richer budget metadata (categories, approval chains, velocity limits)
/// lives in the app layer (icn-ledger).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetRecord {
    /// Unique budget identifier (matches governance CreateBudget effect)
    pub budget_id: String,
    /// Scope: which cooperative/entity this budget belongs to
    pub scope_id: String,
    /// Treasury DID that funds this budget
    pub treasury_did: String,
    /// Currency denomination
    pub currency: String,
    /// Total spending limit for this budget
    pub total_limit: i64,
    /// Total amount spent so far (confirmed only)
    pub spent_total: i64,
    /// Current status
    pub status: BudgetStatus,
    /// In-flight spend (saga intermediate state)
    pub pending_spend: Option<PendingSpend>,
    /// When the budget was created (epoch seconds)
    pub created_at: u64,
    /// Decision hash that created this budget
    pub decision_hash: String,
}

/// Budget lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStatus {
    /// Budget is active and accepting spends
    Active,
    /// Budget has been exhausted (spent_total >= total_limit)
    Exceeded,
    /// Budget has been frozen by governance action
    Frozen,
}

/// In-flight spend during saga execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingSpend {
    /// Decision hash that initiated this spend
    pub decision_hash: String,
    /// Amount being spent
    pub amount: i64,
}

/// Outcome of `begin_spend` — tells the caller what to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginSpendOutcome {
    /// Fresh spend initiated. Caller should persist the record, then
    /// execute the ledger mutation, then call `confirm_spend`.
    Proceed,
    /// A prior attempt with the same decision_hash crashed mid-saga.
    /// The PendingSpend is already recorded — skip persist, retry
    /// the ledger mutation, then call `confirm_spend`.
    RetryNeeded,
}

/// Errors from `begin_spend`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetSpendError {
    /// This exact spend was already confirmed (idempotent success).
    AlreadySpentSameDecision,
    /// Budget limit would be exceeded.
    OverBudget {
        budget_id: String,
        remaining: i64,
        requested: i64,
    },
    /// Budget is frozen.
    BudgetFrozen,
    /// Budget is already exceeded.
    BudgetExceeded,
    /// A different spend is already in flight.
    ConflictingPendingSpend { existing_decision_hash: String },
}

impl std::fmt::Display for BudgetSpendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadySpentSameDecision => {
                write!(f, "Spend already confirmed for this decision")
            }
            Self::OverBudget {
                budget_id,
                remaining,
                requested,
            } => write!(
                f,
                "Budget {} over limit: remaining={}, requested={}",
                budget_id, remaining, requested
            ),
            Self::BudgetFrozen => write!(f, "Budget is frozen"),
            Self::BudgetExceeded => write!(f, "Budget is already exceeded"),
            Self::ConflictingPendingSpend {
                existing_decision_hash,
            } => write!(
                f,
                "Another spend is in flight (decision_hash={})",
                existing_decision_hash
            ),
        }
    }
}

impl std::error::Error for BudgetSpendError {}

impl BudgetRecord {
    /// Create a new budget record from a CreateBudget effect.
    pub fn new(
        budget_id: String,
        scope_id: String,
        treasury_did: String,
        currency: String,
        total_limit: i64,
        decision_hash: String,
        created_at: u64,
    ) -> Self {
        Self {
            budget_id,
            scope_id,
            treasury_did,
            currency,
            total_limit,
            spent_total: 0,
            status: BudgetStatus::Active,
            pending_spend: None,
            created_at,
            decision_hash,
        }
    }

    /// Remaining budget capacity.
    pub fn remaining(&self) -> i64 {
        self.total_limit - self.spent_total
    }

    /// Phase 1 of the spend saga: validate and record the pending spend.
    ///
    /// Returns `Proceed` for a fresh spend, `RetryNeeded` if a prior attempt
    /// with the same decision_hash is already pending (crash recovery).
    pub fn begin_spend(
        &mut self,
        decision_hash: &str,
        amount: i64,
    ) -> std::result::Result<BeginSpendOutcome, BudgetSpendError> {
        // Check status
        match self.status {
            BudgetStatus::Frozen => return Err(BudgetSpendError::BudgetFrozen),
            BudgetStatus::Exceeded => return Err(BudgetSpendError::BudgetExceeded),
            BudgetStatus::Active => {}
        }

        // Check for existing pending spend (crash recovery)
        if let Some(ref pending) = self.pending_spend {
            if pending.decision_hash == decision_hash {
                // Same decision — this is a retry after crash
                return Ok(BeginSpendOutcome::RetryNeeded);
            }
            // Different decision — conflict
            return Err(BudgetSpendError::ConflictingPendingSpend {
                existing_decision_hash: pending.decision_hash.clone(),
            });
        }

        // Check budget capacity (including pending amount)
        let effective_remaining = self.remaining();
        if amount > effective_remaining {
            return Err(BudgetSpendError::OverBudget {
                budget_id: self.budget_id.clone(),
                remaining: effective_remaining,
                requested: amount,
            });
        }

        // Record the pending spend
        self.pending_spend = Some(PendingSpend {
            decision_hash: decision_hash.to_string(),
            amount,
        });

        Ok(BeginSpendOutcome::Proceed)
    }

    /// Phase 2 of the spend saga: confirm the spend after successful ledger mutation.
    ///
    /// Increments `spent_total` by the pending amount and clears the pending spend.
    /// Updates status to `Exceeded` if the budget is now fully consumed.
    pub fn confirm_spend(&mut self) {
        if let Some(pending) = self.pending_spend.take() {
            self.spent_total += pending.amount;
            if self.spent_total >= self.total_limit {
                self.status = BudgetStatus::Exceeded;
            }
        }
    }
}

/// Persistent store for budget records.
///
/// Implementations must provide durable storage (e.g., sled) so that
/// budget state survives process restarts.
pub trait BudgetStore: Send + Sync {
    /// Get a budget record by ID.
    fn get(&self, budget_id: &str) -> Result<Option<BudgetRecord>>;

    /// Persist a budget record (insert or update).
    fn put(&self, record: &BudgetRecord) -> Result<()>;

    /// List all budgets for a given scope.
    fn list_by_scope(&self, scope_id: &str) -> Result<Vec<BudgetRecord>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_budget(limit: i64) -> BudgetRecord {
        BudgetRecord::new(
            "budget-1".into(),
            "coop-1".into(),
            "did:icn:treasury".into(),
            "HOURS".into(),
            limit,
            "decision-hash-create".into(),
            1000,
        )
    }

    #[test]
    fn test_begin_spend_proceed() {
        let mut budget = make_budget(1000);
        let result = budget.begin_spend("decision-1", 500);
        assert_eq!(result, Ok(BeginSpendOutcome::Proceed));
        assert!(budget.pending_spend.is_some());
        assert_eq!(budget.spent_total, 0); // NOT yet incremented
    }

    #[test]
    fn test_confirm_spend() {
        let mut budget = make_budget(1000);
        budget.begin_spend("decision-1", 500).unwrap();
        budget.confirm_spend();
        assert_eq!(budget.spent_total, 500);
        assert!(budget.pending_spend.is_none());
        assert_eq!(budget.status, BudgetStatus::Active);
    }

    #[test]
    fn test_overspend_rejected() {
        let mut budget = make_budget(1000);
        let result = budget.begin_spend("decision-1", 1500);
        assert_eq!(
            result,
            Err(BudgetSpendError::OverBudget {
                budget_id: "budget-1".into(),
                remaining: 1000,
                requested: 1500,
            })
        );
    }

    #[test]
    fn test_exact_limit_spend() {
        let mut budget = make_budget(1000);
        budget.begin_spend("decision-1", 1000).unwrap();
        budget.confirm_spend();
        assert_eq!(budget.spent_total, 1000);
        assert_eq!(budget.status, BudgetStatus::Exceeded);
    }

    #[test]
    fn test_retry_same_decision() {
        let mut budget = make_budget(1000);
        budget.begin_spend("decision-1", 500).unwrap();
        // Simulate crash recovery — same decision_hash
        let result = budget.begin_spend("decision-1", 500);
        assert_eq!(result, Ok(BeginSpendOutcome::RetryNeeded));
    }

    #[test]
    fn test_conflicting_pending_spend() {
        let mut budget = make_budget(1000);
        budget.begin_spend("decision-1", 500).unwrap();
        // Different decision while one is pending
        let result = budget.begin_spend("decision-2", 300);
        assert_eq!(
            result,
            Err(BudgetSpendError::ConflictingPendingSpend {
                existing_decision_hash: "decision-1".into(),
            })
        );
    }

    #[test]
    fn test_frozen_budget_rejects_spend() {
        let mut budget = make_budget(1000);
        budget.status = BudgetStatus::Frozen;
        let result = budget.begin_spend("decision-1", 100);
        assert_eq!(result, Err(BudgetSpendError::BudgetFrozen));
    }

    #[test]
    fn test_exceeded_budget_rejects_spend() {
        let mut budget = make_budget(1000);
        budget.status = BudgetStatus::Exceeded;
        let result = budget.begin_spend("decision-1", 100);
        assert_eq!(result, Err(BudgetSpendError::BudgetExceeded));
    }

    #[test]
    fn test_multiple_spends_track_remaining() {
        let mut budget = make_budget(1000);

        // First spend: 300
        budget.begin_spend("d1", 300).unwrap();
        budget.confirm_spend();
        assert_eq!(budget.remaining(), 700);

        // Second spend: 400
        budget.begin_spend("d2", 400).unwrap();
        budget.confirm_spend();
        assert_eq!(budget.remaining(), 300);

        // Third spend: 301 — should fail
        let result = budget.begin_spend("d3", 301);
        assert_eq!(
            result,
            Err(BudgetSpendError::OverBudget {
                budget_id: "budget-1".into(),
                remaining: 300,
                requested: 301,
            })
        );

        // Third spend: 300 — should succeed (exact remaining)
        budget.begin_spend("d3", 300).unwrap();
        budget.confirm_spend();
        assert_eq!(budget.remaining(), 0);
        assert_eq!(budget.status, BudgetStatus::Exceeded);
    }

    #[test]
    fn test_serde_roundtrip() {
        let budget = make_budget(5000);
        let json = serde_json::to_string(&budget).unwrap();
        let parsed: BudgetRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(budget, parsed);
    }
}
