//! Credit policy system for dynamic credit limits and economic safety
//!
//! Implements:
//! - Dynamic credit limits based on trust + transaction history
//! - New member protective throttling
//! - Cleared volume tracking for credit rewards

use crate::ledger::Ledger;
use crate::types::CreditLimit;
use anyhow::Result;
use icn_identity::Did;
use icn_trust::TrustGraph;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Policy for calculating dynamic credit limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditPolicy {
    /// Base credit limit for all members (in smallest currency unit)
    /// Example: 100 hours = 100 * 100 = 10,000 (if decimals=2)
    pub baseline: i64,

    /// Multiplier for trust score (0.0 - 1.0)
    /// Example: 0.5 means 50% bonus for fully trusted members
    pub trust_multiplier: f64,

    /// Percentage of cleared volume to add as bonus
    /// Example: 0.1 = 10% of total cleared transactions
    pub history_bonus_rate: f64,

    /// Currency this policy applies to
    pub currency: String,
}

impl CreditPolicy {
    /// Create a new credit policy
    pub fn new(
        baseline: i64,
        trust_multiplier: f64,
        history_bonus_rate: f64,
        currency: String,
    ) -> Self {
        CreditPolicy {
            baseline,
            trust_multiplier,
            history_bonus_rate,
            currency,
        }
    }

    /// Create a conservative policy for new communities
    pub fn conservative(currency: String) -> Self {
        Self::new(
            10_000, // 100 hours (assuming 2 decimals)
            0.3,    // 30% trust bonus (max +30 hours)
            0.05,   // 5% of cleared volume
            currency,
        )
    }

    /// Create a permissive policy for established communities
    pub fn permissive(currency: String) -> Self {
        Self::new(
            50_000, // 500 hours (assuming 2 decimals)
            0.5,    // 50% trust bonus (max +250 hours)
            0.15,   // 15% of cleared volume
            currency,
        )
    }

    /// Calculate credit limit for a member
    ///
    /// Formula: baseline + (baseline * trust_score * trust_multiplier) + (cleared_volume * history_bonus_rate)
    ///
    /// Example with conservative policy:
    /// - baseline = 100 hours
    /// - trust_score = 0.8 (trusted member)
    /// - cleared_volume = 1000 hours (historical contributions)
    ///
    /// Limit = 100 + (100 * 0.8 * 0.3) + (1000 * 0.05)
    ///       = 100 + 24 + 50
    ///       = 174 hours
    pub fn calculate_limit(
        &self,
        member: &Did,
        ledger: &Ledger,
        trust_graph: &TrustGraph,
    ) -> Result<i64> {
        // Get trust score (0.0 = untrusted, 1.0 = fully trusted)
        let trust_score = trust_graph
            .compute_trust_score(member)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);

        // Get cleared volume (sum of all cleared credits)
        let cleared_volume = ledger.total_cleared_by(member, &self.currency)?;

        // Calculate limit
        let trust_bonus = (self.baseline as f64 * trust_score * self.trust_multiplier) as i64;
        let history_bonus = (cleared_volume as f64 * self.history_bonus_rate) as i64;

        let limit = self.baseline + trust_bonus + history_bonus;

        Ok(limit)
    }

    /// Check if a proposed balance change would exceed credit limit
    pub fn would_exceed_limit(
        &self,
        _member: &Did,
        current_balance: i64,
        delta: i64,
        calculated_limit: i64,
    ) -> bool {
        let new_balance = current_balance + delta;

        // Credit limit is for negative balances (how much you can owe)
        // If new balance is more negative than -limit, it exceeds
        new_balance < -calculated_limit
    }
}

/// Policy for throttling new members
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMemberPolicy {
    /// Initial credit limit for brand new members
    /// Should be very conservative (e.g., 10-20 hours)
    pub initial_limit: i64,

    /// Time period over which credit ramps to full limit
    pub ramp_period: Duration,

    /// Minimum cleared contributions before ramping starts
    /// Example: Must clear 50 hours of contributions
    pub contribution_threshold: i64,

    /// Currency this policy applies to
    pub currency: String,
}

impl NewMemberPolicy {
    /// Create a new member policy
    pub fn new(
        initial_limit: i64,
        ramp_period: Duration,
        contribution_threshold: i64,
        currency: String,
    ) -> Self {
        NewMemberPolicy {
            initial_limit,
            ramp_period,
            contribution_threshold,
            currency,
        }
    }

    /// Create a conservative policy for protecting communities
    pub fn conservative(currency: String) -> Self {
        Self::new(
            1_000,                           // 10 hours initial limit
            Duration::from_secs(90 * 86400), // 90 days
            5_000,                           // Must clear 50 hours first
            currency,
        )
    }

    /// Calculate effective credit limit for a member based on tenure and contributions
    ///
    /// Logic:
    /// 1. If cleared < contribution_threshold: use initial_limit
    /// 2. Otherwise: ramp linearly from initial_limit to full_limit over ramp_period
    ///
    /// Example:
    /// - member joined 30 days ago
    /// - has cleared 60 hours (exceeds 50 hour threshold)
    /// - ramp period is 90 days
    /// - initial_limit = 10 hours, full_limit = 100 hours
    ///
    /// Limit = 10 + ((100 - 10) * (30 / 90))
    ///       = 10 + (90 * 0.33)
    ///       = 10 + 30
    ///       = 40 hours
    pub fn calculate_effective_limit(
        &self,
        _member: &Did,
        member_since: u64, // Unix timestamp when member joined
        current_time: u64, // Current Unix timestamp
        cleared_volume: i64,
        full_limit: i64, // The limit they'll eventually reach
    ) -> i64 {
        // If hasn't met contribution threshold, use initial limit
        if cleared_volume < self.contribution_threshold {
            return self.initial_limit;
        }

        // Calculate tenure (how long they've been a member)
        let tenure_secs = current_time.saturating_sub(member_since);
        let ramp_period_secs = self.ramp_period.as_secs();

        // If tenure exceeds ramp period, they get full limit
        if tenure_secs >= ramp_period_secs {
            return full_limit;
        }

        // Linear ramp from initial_limit to full_limit
        let ramp_progress = tenure_secs as f64 / ramp_period_secs as f64;
        let limit_range = full_limit - self.initial_limit;
        let ramped_bonus = (limit_range as f64 * ramp_progress) as i64;

        self.initial_limit + ramped_bonus
    }
}

/// Combined credit policy manager
pub struct CreditPolicyManager {
    /// Base credit policy
    pub credit_policy: CreditPolicy,

    /// New member throttling policy
    pub new_member_policy: NewMemberPolicy,
}

impl CreditPolicyManager {
    /// Create a new policy manager
    pub fn new(credit_policy: CreditPolicy, new_member_policy: NewMemberPolicy) -> Self {
        CreditPolicyManager {
            credit_policy,
            new_member_policy,
        }
    }

    /// Create conservative policies for new communities
    pub fn conservative(currency: String) -> Self {
        Self::new(
            CreditPolicy::conservative(currency.clone()),
            NewMemberPolicy::conservative(currency),
        )
    }

    /// Calculate final credit limit for a member
    ///
    /// This combines:
    /// 1. Base limit from CreditPolicy (trust + history)
    /// 2. New member throttling from NewMemberPolicy
    ///
    /// The effective limit is the minimum of the two.
    pub fn calculate_credit_limit(
        &self,
        member: &Did,
        member_since: u64,
        current_time: u64,
        ledger: &Ledger,
        trust_graph: &TrustGraph,
    ) -> Result<CreditLimit> {
        // Calculate base limit from credit policy
        let base_limit = self
            .credit_policy
            .calculate_limit(member, ledger, trust_graph)?;

        // Get cleared volume for new member check
        let cleared_volume = ledger.total_cleared_by(member, &self.credit_policy.currency)?;

        // Calculate effective limit considering new member throttling
        let throttled_limit = self.new_member_policy.calculate_effective_limit(
            member,
            member_since,
            current_time,
            cleared_volume,
            base_limit,
        );

        // Effective limit is the minimum of the two
        let effective_limit = throttled_limit.min(base_limit);

        Ok(CreditLimit {
            participant: member.clone(),
            currency: self.credit_policy.currency.clone(),
            max_negative_balance: effective_limit,
            set_by: member.clone(), // Self-determined by policy
        })
    }

    /// Check if a transaction would violate credit limit
    #[allow(clippy::too_many_arguments)] // Required for comprehensive limit checking
    pub fn check_transaction(
        &self,
        member: &Did,
        member_since: u64,
        current_time: u64,
        current_balance: i64,
        transaction_delta: i64,
        ledger: &Ledger,
        trust_graph: &TrustGraph,
    ) -> Result<bool> {
        let limit =
            self.calculate_credit_limit(member, member_since, current_time, ledger, trust_graph)?;

        let would_exceed = self.credit_policy.would_exceed_limit(
            member,
            current_balance,
            transaction_delta,
            limit.max_negative_balance,
        );

        Ok(!would_exceed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credit_policy_conservative_defaults() {
        let policy = CreditPolicy::conservative("hours".to_string());

        // Verify conservative policy settings
        assert_eq!(policy.baseline, 10_000); // 100 hours (with 2 decimals)
        assert_eq!(policy.trust_multiplier, 0.3); // 30% trust bonus
        assert_eq!(policy.history_bonus_rate, 0.05); // 5% of cleared volume
        assert_eq!(policy.currency, "hours");
    }

    #[test]
    fn test_would_exceed_limit() {
        let policy = CreditPolicy::conservative("hours".to_string());
        let member = icn_identity::KeyPair::generate().unwrap().did().clone();

        // Current balance: -5,000 (owes 50 hours)
        // Calculated limit: 10,000 (can owe up to 100 hours)
        // Delta: -6,000 (additional 60 hours debt)
        // New balance would be: -11,000 (owes 110 hours)
        // Should exceed limit of 10,000

        let current_balance = -5_000;
        let delta = -6_000;
        let limit = 10_000;

        assert!(policy.would_exceed_limit(&member, current_balance, delta, limit));

        // Test within limit
        let small_delta = -4_000; // Would result in -9,000, within -10,000 limit
        assert!(!policy.would_exceed_limit(&member, current_balance, small_delta, limit));
    }

    #[test]
    fn test_new_member_policy_conservative() {
        let policy = NewMemberPolicy::conservative("hours".to_string());

        assert_eq!(policy.initial_limit, 1_000); // 10 hours
        assert_eq!(policy.contribution_threshold, 5_000); // 50 hours
        assert_eq!(policy.ramp_period.as_secs(), 90 * 86400); // 90 days
    }

    #[test]
    fn test_new_member_ramping() {
        let policy = NewMemberPolicy::conservative("hours".to_string());
        let member = icn_identity::KeyPair::generate().unwrap().did().clone();

        let member_since = 1000;
        let full_limit = 10_000; // 100 hours

        // Test 1: Not enough cleared volume yet
        let cleared = 4_000; // Only 40 hours cleared (< 50 threshold)
        let current_time = member_since + 30 * 86400; // 30 days later
        let limit = policy.calculate_effective_limit(
            &member,
            member_since,
            current_time,
            cleared,
            full_limit,
        );
        assert_eq!(limit, policy.initial_limit); // Still at initial limit

        // Test 2: Cleared enough, 30 days in (1/3 of 90 day ramp)
        let cleared = 6_000; // 60 hours cleared (> 50 threshold)
        let current_time = member_since + 30 * 86400; // 30 days later
        let limit = policy.calculate_effective_limit(
            &member,
            member_since,
            current_time,
            cleared,
            full_limit,
        );

        // Expected: 1,000 + ((10,000 - 1,000) * (30/90))
        //         = 1,000 + (9,000 * 0.333...)
        //         = 1,000 + 3,000
        //         = 4,000
        assert!(limit > policy.initial_limit);
        assert!(limit < full_limit);
        assert!((3_000..=4_000).contains(&limit)); // Approximate due to rounding

        // Test 3: Full ramp period complete
        let cleared = 10_000;
        let current_time = member_since + 100 * 86400; // 100 days (> 90 day ramp)
        let limit = policy.calculate_effective_limit(
            &member,
            member_since,
            current_time,
            cleared,
            full_limit,
        );
        assert_eq!(limit, full_limit); // Should be at full limit now
    }
}
