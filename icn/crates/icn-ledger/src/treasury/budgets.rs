//! Treasury budget management operations
//!
//! This module provides functionality for managing budgets within treasuries,
//! including budget creation, spending tracking, and status management.

use crate::types::ContentHash;
use anyhow::{bail, Result};
use icn_identity::Did;
use icn_obs::metrics::treasury as treasury_metrics;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use super::{uuid_simple, BUDGET_PREFIX, TREASURY_IDX_BUDGETS_PREFIX};

/// Budget allocation within a treasury
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryBudget {
    /// Unique budget ID
    pub id: String,

    /// Treasury this budget belongs to
    pub treasury_did: Did,

    /// Purpose of this budget (e.g., "operations", "grants", "emergency")
    pub purpose: String,

    /// Allocated amount from treasury
    pub allocated_amount: i64,

    /// Amount spent from this budget
    pub spent_amount: i64,

    /// Currency
    pub currency: String,

    /// Budget period (start timestamp)
    pub period_start: u64,

    /// Budget period (end timestamp, None = indefinite)
    pub period_end: Option<u64>,

    /// Current status
    pub status: BudgetStatus,

    /// Governance proposal that created this budget (if any)
    pub proposal_id: Option<String>,

    /// When created
    pub created_at: u64,

    /// Who created it
    pub created_by: Did,

    /// Notification thresholds (e.g., [50, 80, 100] = notify at 50%, 80%, 100%)
    pub notification_thresholds: Vec<u8>,

    /// Thresholds already notified
    pub notified_thresholds: Vec<u8>,
}

impl TreasuryBudget {
    /// Create a new budget allocation
    pub fn new(
        treasury_did: Did,
        purpose: String,
        allocated_amount: i64,
        currency: String,
        period_end: Option<u64>,
        created_by: Did,
        proposal_id: Option<String>,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();
        Self {
            id: format!("budget-{}-{}", now, uuid_simple()),
            treasury_did,
            purpose,
            allocated_amount,
            spent_amount: 0,
            currency,
            period_start: now,
            period_end,
            status: BudgetStatus::Active,
            proposal_id,
            created_at: now,
            created_by,
            notification_thresholds: vec![50, 80, 100],
            notified_thresholds: Vec::new(),
        }
    }

    /// Calculate remaining budget
    pub fn remaining(&self) -> i64 {
        self.allocated_amount - self.spent_amount
    }

    /// Calculate percentage used
    pub fn percentage_used(&self) -> f64 {
        if self.allocated_amount == 0 {
            return 0.0;
        }
        (self.spent_amount as f64 / self.allocated_amount as f64) * 100.0
    }

    /// Check if budget is exceeded
    pub fn is_exceeded(&self) -> bool {
        self.spent_amount > self.allocated_amount
    }

    /// Check if budget period has expired
    pub fn is_expired(&self, now: u64) -> bool {
        self.period_end.is_some_and(|end| now >= end)
    }

    /// Check if spending is allowed (active and not expired)
    pub fn can_spend(&self) -> bool {
        self.status == BudgetStatus::Active && !self.is_expired(icn_time::current_timestamp_secs())
    }

    /// Record spending and return any newly triggered thresholds
    ///
    /// Uses saturating addition to prevent integer overflow.
    pub fn record_spending(&mut self, amount: i64) -> Vec<u8> {
        self.spent_amount = self.spent_amount.saturating_add(amount);

        // Check for exceeded status
        if self.is_exceeded() {
            self.status = BudgetStatus::Exceeded;
        }

        // Check for threshold notifications
        // Round and clamp to valid u8 range (0-100)
        let percentage = self.percentage_used().round().clamp(0.0, 100.0) as u8;
        let mut triggered = Vec::new();

        for threshold in &self.notification_thresholds {
            if percentage >= *threshold && !self.notified_thresholds.contains(threshold) {
                self.notified_thresholds.push(*threshold);
                triggered.push(*threshold);
            }
        }

        triggered
    }
}

/// Budget status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetStatus {
    /// Budget is active and can be spent from
    Active,
    /// Budget is paused (no spending allowed)
    Paused,
    /// Budget has been exceeded
    Exceeded,
    /// Budget period has expired
    Expired,
    /// Budget was cancelled
    Cancelled,
}

/// Budget operations for TreasuryManager
impl super::TreasuryManager {
    /// Create a budget allocation
    pub fn create_budget(
        &mut self,
        treasury_did: Did,
        purpose: String,
        amount: i64,
        currency: String,
        period_end: Option<u64>,
        created_by: Did,
        proposal_id: Option<String>,
    ) -> Result<TreasuryBudget> {
        // Validate inputs
        if !self.treasuries.contains_key(&treasury_did) {
            bail!("Treasury not found: {treasury_did}");
        }

        if amount <= 0 {
            bail!("Budget amount must be positive, got: {amount}");
        }

        if purpose.trim().is_empty() {
            bail!("Budget purpose cannot be empty");
        }

        if currency.trim().is_empty() {
            bail!("Budget currency cannot be empty");
        }

        // Validate period_end is in the future if provided
        if let Some(end) = period_end {
            let now = icn_time::current_timestamp_secs();
            if end <= now {
                bail!("Budget period end must be in the future");
            }
        }

        let budget = TreasuryBudget::new(
            treasury_did.clone(),
            purpose.clone(),
            amount,
            currency,
            period_end,
            created_by,
            proposal_id,
        );

        info!(
            budget_id = %budget.id,
            treasury_did = %treasury_did,
            purpose = %purpose,
            amount = amount,
            "Creating treasury budget"
        );

        let budget_id = budget.id.clone();
        self.budgets.insert(budget_id.clone(), budget.clone());
        self.treasury_budgets
            .entry(treasury_did.clone())
            .or_default()
            .push(budget_id.clone());

        // Persist
        if let Some(ref store) = self.store {
            self.persist_budget(&budget, store)?;
            self.persist_budget_index(&treasury_did, &budget_id, store)?;
        }

        // Emit metrics
        treasury_metrics::budget_created_inc();
        treasury_metrics::budget_remaining_set(&budget_id, amount);

        Ok(budget)
    }

    /// Get budget by ID
    pub fn get_budget(&self, budget_id: &str) -> Option<&TreasuryBudget> {
        self.budgets.get(budget_id)
    }

    /// Get mutable budget by ID
    pub fn get_budget_mut(&mut self, budget_id: &str) -> Option<&mut TreasuryBudget> {
        self.budgets.get_mut(budget_id)
    }

    /// List budgets for a treasury
    pub fn list_budgets(&self, treasury_did: &Did) -> Vec<&TreasuryBudget> {
        self.treasury_budgets
            .get(treasury_did)
            .map(|ids| ids.iter().filter_map(|id| self.budgets.get(id)).collect())
            .unwrap_or_default()
    }

    /// Record spending against a budget
    pub fn record_spending(
        &mut self,
        budget_id: &str,
        amount: i64,
        ledger_entry_hash: ContentHash,
    ) -> Result<Vec<u8>> {
        // Validate amount is positive
        if amount <= 0 {
            bail!("Spending amount must be positive, got: {amount}");
        }

        // First validate without mutable borrow
        {
            let budget = self
                .budgets
                .get(budget_id)
                .ok_or_else(|| anyhow::anyhow!("Budget not found: {budget_id}"))?;

            if !budget.can_spend() {
                bail!("Budget {budget_id} is not available for spending");
            }

            if amount > budget.remaining() {
                bail!(
                    "Amount {} exceeds remaining budget {}",
                    amount,
                    budget.remaining()
                );
            }
        }

        info!(
            budget_id = %budget_id,
            amount = amount,
            entry_hash = %ledger_entry_hash,
            "Recording budget spending"
        );

        // Now mutate
        let budget = self
            .budgets
            .get_mut(budget_id)
            .ok_or_else(|| anyhow::anyhow!("Budget not found: {budget_id}"))?;
        let triggered_thresholds = budget.record_spending(amount);
        let budget_clone = budget.clone();

        // Persist updated budget
        if let Some(ref store) = self.store {
            self.persist_budget(&budget_clone, store)?;
        }

        Ok(triggered_thresholds)
    }

    /// Update budget status
    pub fn update_budget_status(&mut self, budget_id: &str, status: BudgetStatus) -> Result<()> {
        let budget = self
            .budgets
            .get_mut(budget_id)
            .ok_or_else(|| anyhow::anyhow!("Budget not found: {budget_id}"))?;

        info!(
            budget_id = %budget_id,
            old_status = ?budget.status,
            new_status = ?status,
            "Updating budget status"
        );

        budget.status = status;
        let budget_clone = budget.clone();

        if let Some(ref store) = self.store {
            self.persist_budget(&budget_clone, store)?;
        }

        Ok(())
    }

    /// Save budget changes to persistent storage
    ///
    /// Call this after making direct mutations to budget fields (e.g., allocated_amount).
    /// This is a public wrapper around the internal persist_budget method.
    pub fn save_budget(&self, budget_id: &str) -> Result<()> {
        let budget = self
            .budgets
            .get(budget_id)
            .ok_or_else(|| anyhow::anyhow!("Budget not found: {budget_id}"))?;

        if let Some(ref store) = self.store {
            self.persist_budget(budget, store)?;
        }

        Ok(())
    }

    /// Persist a budget snapshot directly to storage without updating in-memory state.
    ///
    /// This is used for two-phase commit patterns where we want to persist changes
    /// before committing them to in-memory state. The budget's ID is used as the key.
    ///
    /// # Two-Phase Commit Pattern
    ///
    /// 1. Clone budgets from in-memory state
    /// 2. Modify the clones
    /// 3. Call `save_budget_snapshot` to persist clones (if this fails, no state corruption)
    /// 4. Call `apply_budget_snapshot` to update in-memory state
    ///
    /// This ensures that if persistence fails, in-memory state remains consistent.
    pub fn save_budget_snapshot(&self, budget: &TreasuryBudget) -> Result<()> {
        if let Some(ref store) = self.store {
            self.persist_budget(budget, store)?;
        }
        Ok(())
    }

    /// Apply a budget snapshot to in-memory state.
    ///
    /// Used after the persist phase of two-phase commit to sync in-memory state
    /// with storage. Returns error if the budget ID doesn't exist in-memory.
    ///
    /// Takes a reference and clones internally to avoid consuming the caller's budget,
    /// allowing the caller to continue using the budget if needed (e.g., for logging).
    ///
    /// # Safety
    ///
    /// Only call this after successfully calling `save_budget_snapshot` for the same budget.
    /// This ensures storage and memory remain consistent.
    pub fn apply_budget_snapshot(&mut self, budget: &TreasuryBudget) -> Result<()> {
        if !self.budgets.contains_key(&budget.id) {
            bail!("Budget not found in-memory: {}", budget.id);
        }
        self.budgets.insert(budget.id.clone(), budget.clone());
        Ok(())
    }

    /// Persist budget to storage (internal helper)
    pub(super) fn persist_budget(
        &self,
        budget: &TreasuryBudget,
        store: &Arc<dyn Store>,
    ) -> Result<()> {
        let key = format!("{}{}", BUDGET_PREFIX, budget.id);
        let value = serde_json::to_vec(budget)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    /// Persist budget index (internal helper)
    pub(super) fn persist_budget_index(
        &self,
        treasury_did: &Did,
        budget_id: &str,
        store: &Arc<dyn Store>,
    ) -> Result<()> {
        let key = format!("{TREASURY_IDX_BUDGETS_PREFIX}{treasury_did}:{budget_id}");
        store.put(key.as_bytes(), budget_id.as_bytes())?;
        Ok(())
    }
}
