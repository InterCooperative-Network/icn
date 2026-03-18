//! Treasury approval workflow operations
//!
//! This module provides functionality for managing spending rules and velocity limits
//! that control treasury withdrawals through governance approval requirements.

use anyhow::{bail, Result};
use icn_identity::Did;
use icn_obs::metrics::treasury as treasury_metrics;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use super::{uuid_simple, SPENDING_RULE_PREFIX, TREASURY_PREFIX};

/// Spending rule for treasury operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingRule {
    /// Unique rule ID
    pub id: String,

    /// Treasury this rule applies to
    pub treasury_did: Did,

    /// Rule name/description
    pub name: String,

    /// Amount threshold (spending above this requires governance)
    pub threshold_amount: i64,

    /// Currency this rule applies to
    pub currency: String,

    /// Required governance approval type
    pub approval_type: ApprovalType,

    /// Whether the rule is active
    pub is_active: bool,

    /// When created
    pub created_at: u64,

    /// Governance proposal that created/modified this rule
    pub proposal_id: Option<String>,
}

impl SpendingRule {
    /// Create a new spending rule
    pub fn new(
        treasury_did: Did,
        name: String,
        threshold_amount: i64,
        currency: String,
        approval_type: ApprovalType,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();
        Self {
            id: format!("rule-{}-{}", now, uuid_simple()),
            treasury_did,
            name,
            threshold_amount,
            currency,
            approval_type,
            is_active: true,
            created_at: now,
            proposal_id: None,
        }
    }

    /// Create with proposal reference
    pub fn with_proposal(mut self, proposal_id: String) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }
}

/// Type of approval required for treasury spending
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalType {
    /// No approval needed (within threshold)
    None,
    /// Simple majority vote (>50%)
    SimpleMajority,
    /// Super-majority (>66%)
    SuperMajority,
    /// Board/council approval only
    BoardOnly,
    /// Emergency threshold (75%+)
    Emergency,
}

/// Velocity limit for treasury withdrawals
///
/// Prevents treasury drain attacks by limiting total withdrawals
/// within a rolling time window. Unlike spending rules (which require
/// governance approval for large individual withdrawals), velocity limits
/// block rapid small withdrawals that could cumulatively drain a treasury.
///
/// # Example
/// ```text
/// // Limit: max 5000 hours per hour
/// VelocityLimit {
///     window_seconds: 3600,   // 1 hour window
///     max_amount: 5000,       // max 5000 per window
///     ..
/// }
///
/// // Over 1 hour, user makes 10 withdrawals of 400 each
/// // After 5000 cumulative, further withdrawals are blocked
/// // until older withdrawals expire from the rolling window
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelocityLimit {
    /// Unique limit ID
    pub id: String,

    /// Treasury this limit applies to
    pub treasury_did: Did,

    /// Currency this limit applies to
    pub currency: String,

    /// Rolling window size in seconds (e.g., 3600 for hourly, 86400 for daily)
    pub window_seconds: u64,

    /// Maximum total amount allowed within the window
    pub max_amount: i64,

    /// Whether the limit is active
    pub is_active: bool,

    /// When created
    pub created_at: u64,

    /// Governance proposal that created/modified this limit
    pub proposal_id: Option<String>,
}

impl VelocityLimit {
    /// Create a new velocity limit
    ///
    /// # Arguments
    /// * `treasury_did` - Treasury this applies to
    /// * `currency` - Currency to track
    /// * `window_seconds` - Rolling window size (e.g., 3600 for hourly)
    /// * `max_amount` - Maximum amount allowed in window
    pub fn new(treasury_did: Did, currency: String, window_seconds: u64, max_amount: i64) -> Self {
        let now = icn_time::current_timestamp_secs();
        Self {
            id: format!("vlimit-{}-{}", now, uuid_simple()),
            treasury_did,
            currency,
            window_seconds,
            max_amount,
            is_active: true,
            created_at: now,
            proposal_id: None,
        }
    }

    /// Create with proposal reference
    pub fn with_proposal(mut self, proposal_id: String) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }
}

/// Tracks withdrawal history for velocity limit checking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VelocityWindow {
    /// Withdrawals within the tracking period: (timestamp, amount)
    pub withdrawals: Vec<(u64, i64)>,

    /// When this window was last updated
    pub last_updated: u64,
}

impl VelocityWindow {
    /// Calculate total withdrawn within the window, pruning expired entries
    pub fn total_in_window(&mut self, window_seconds: u64, now: u64) -> i64 {
        let window_start = now.saturating_sub(window_seconds);

        // Prune expired entries
        self.withdrawals
            .retain(|(timestamp, _)| *timestamp > window_start);
        self.last_updated = now;

        // Sum remaining
        self.withdrawals.iter().map(|(_, amount)| *amount).sum()
    }

    /// Record a new withdrawal
    pub fn record_withdrawal(&mut self, amount: i64) {
        let now = icn_time::current_timestamp_secs();
        self.withdrawals.push((now, amount));
        self.last_updated = now;
    }

    /// Check if adding `amount` would exceed `max_amount` in window
    pub fn would_exceed(&mut self, window_seconds: u64, max_amount: i64, amount: i64) -> bool {
        let now = icn_time::current_timestamp_secs();
        let current_total = self.total_in_window(window_seconds, now);
        current_total.saturating_add(amount) > max_amount
    }
}

/// Helper function to determine approval priority level
/// Get priority value for approval types (higher = more strict)
pub(super) fn approval_type_priority(approval: ApprovalType) -> u8 {
    match approval {
        ApprovalType::None => 0,
        ApprovalType::SimpleMajority => 1,
        ApprovalType::SuperMajority => 2,
        ApprovalType::BoardOnly => 3,
        ApprovalType::Emergency => 4,
    }
}

/// Approval operations for TreasuryManager
impl super::TreasuryManager {
    /// Add a spending rule
    pub fn add_spending_rule(&mut self, rule: SpendingRule) -> Result<()> {
        if !self.treasuries.contains_key(&rule.treasury_did) {
            bail!("Treasury not found: {}", rule.treasury_did);
        }

        info!(
            rule_id = %rule.id,
            treasury_did = %rule.treasury_did,
            threshold = rule.threshold_amount,
            approval_type = ?rule.approval_type,
            "Adding spending rule"
        );

        let rule_id = rule.id.clone();
        let treasury_did = rule.treasury_did.clone();

        self.spending_rules.insert(rule_id.clone(), rule.clone());
        self.treasury_rules
            .entry(treasury_did.clone())
            .or_default()
            .push(rule_id);

        if let Some(ref store) = self.store {
            self.persist_spending_rule(&rule, store)?;
        }

        Ok(())
    }

    /// Check if an amount requires governance approval
    /// Returns the highest approval type required, or None if no approval needed
    pub fn requires_approval(
        &self,
        treasury_did: &Did,
        amount: i64,
        currency: &str,
    ) -> Option<ApprovalType> {
        let rule_ids = self.treasury_rules.get(treasury_did)?;

        let mut highest_approval: Option<ApprovalType> = None;

        for rule_id in rule_ids {
            if let Some(rule) = self.spending_rules.get(rule_id) {
                if rule.is_active && rule.currency == currency && amount >= rule.threshold_amount {
                    // Return the highest approval type
                    let current = highest_approval.unwrap_or(ApprovalType::None);
                    if approval_type_priority(rule.approval_type) > approval_type_priority(current)
                    {
                        highest_approval = Some(rule.approval_type);
                    }
                }
            }
        }

        // Only return if it requires some approval
        let result = match highest_approval {
            Some(ApprovalType::None) => None,
            other => other,
        };

        // Emit metric if approval is required
        if let Some(ref approval_type) = result {
            let type_str = match approval_type {
                ApprovalType::None => "none",
                ApprovalType::SimpleMajority => "simple_majority",
                ApprovalType::SuperMajority => "super_majority",
                ApprovalType::BoardOnly => "board_only",
                ApprovalType::Emergency => "emergency",
            };
            treasury_metrics::approval_required_inc(type_str);
        }

        result
    }

    /// List spending rules for a treasury
    pub fn list_spending_rules(&self, treasury_did: &Did) -> Vec<&SpendingRule> {
        self.treasury_rules
            .get(treasury_did)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.spending_rules.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Update a spending rule
    pub fn update_spending_rule(
        &mut self,
        rule_id: &str,
        threshold_amount: Option<i64>,
        approval_type: Option<ApprovalType>,
        is_active: Option<bool>,
    ) -> Result<()> {
        let rule = self
            .spending_rules
            .get_mut(rule_id)
            .ok_or_else(|| anyhow::anyhow!("Spending rule not found: {rule_id}"))?;

        if let Some(threshold) = threshold_amount {
            rule.threshold_amount = threshold;
        }
        if let Some(approval) = approval_type {
            rule.approval_type = approval;
        }
        if let Some(active) = is_active {
            rule.is_active = active;
        }

        info!(
            rule_id = %rule_id,
            threshold = rule.threshold_amount,
            approval_type = ?rule.approval_type,
            is_active = rule.is_active,
            "Updated spending rule"
        );

        let rule_clone = rule.clone();

        if let Some(ref store) = self.store {
            self.persist_spending_rule(&rule_clone, store)?;
        }

        Ok(())
    }

    /// Add a velocity limit to a treasury
    ///
    /// Velocity limits prevent treasury drain attacks by limiting the total
    /// amount that can be withdrawn within a rolling time window.
    pub fn add_velocity_limit(&mut self, limit: VelocityLimit) -> Result<()> {
        if !self.treasuries.contains_key(&limit.treasury_did) {
            bail!("Treasury not found: {}", limit.treasury_did);
        }

        info!(
            limit_id = %limit.id,
            treasury_did = %limit.treasury_did,
            window_seconds = limit.window_seconds,
            max_amount = limit.max_amount,
            currency = %limit.currency,
            "Adding velocity limit"
        );

        let limit_id = limit.id.clone();
        let treasury_did = limit.treasury_did.clone();

        self.velocity_limits.insert(limit_id.clone(), limit.clone());
        self.treasury_velocity_limits
            .entry(treasury_did)
            .or_default()
            .push(limit_id);

        if let Some(ref store) = self.store {
            self.persist_velocity_limit(&limit, store)?;
        }

        Ok(())
    }

    /// List velocity limits for a treasury
    pub fn list_velocity_limits(&self, treasury_did: &Did) -> Vec<&VelocityLimit> {
        self.treasury_velocity_limits
            .get(treasury_did)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.velocity_limits.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if a withdrawal would violate velocity limits
    ///
    /// Returns Some(limit) if the withdrawal would exceed a velocity limit,
    /// or None if the withdrawal is allowed.
    pub fn check_velocity(
        &mut self,
        treasury_did: &Did,
        amount: i64,
        currency: &str,
    ) -> Option<&VelocityLimit> {
        let limit_ids = self.treasury_velocity_limits.get(treasury_did)?.clone();

        for limit_id in &limit_ids {
            if let Some(limit) = self.velocity_limits.get(limit_id) {
                if !limit.is_active || limit.currency != currency {
                    continue;
                }

                // Get or create velocity window for this treasury/currency pair
                let window_key = (treasury_did.clone(), currency.to_string());
                let window = self.velocity_windows.entry(window_key).or_default();

                if window.would_exceed(limit.window_seconds, limit.max_amount, amount) {
                    // Emit metric for velocity limit exceeded
                    treasury_metrics::velocity_limit_exceeded_inc(currency);
                    // Return the limit that would be violated
                    return self.velocity_limits.get(limit_id);
                }
            }
        }

        None
    }

    /// Record a withdrawal for velocity tracking
    ///
    /// Call this after a successful withdrawal to update the velocity window.
    pub fn record_withdrawal_for_velocity(
        &mut self,
        treasury_did: &Did,
        amount: i64,
        currency: &str,
    ) {
        let window_key = (treasury_did.clone(), currency.to_string());
        let window = self.velocity_windows.entry(window_key).or_default();
        window.record_withdrawal(amount);
    }

    /// Persist spending rule to storage (internal helper)
    pub(super) fn persist_spending_rule(
        &self,
        rule: &SpendingRule,
        store: &Arc<dyn Store>,
    ) -> Result<()> {
        let key = format!("{}{}", SPENDING_RULE_PREFIX, rule.id);
        let value = serde_json::to_vec(rule)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    /// Persist velocity limit to storage (internal helper)
    pub(super) fn persist_velocity_limit(
        &self,
        limit: &VelocityLimit,
        store: &Arc<dyn Store>,
    ) -> Result<()> {
        let key = format!("{}vlimit:{}", TREASURY_PREFIX, limit.id);
        let value = serde_json::to_vec(limit)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }
}
