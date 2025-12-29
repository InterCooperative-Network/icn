//! Dynamic Credit Limit Algorithms
//!
//! This module provides dynamic adjustment capabilities for credit limits,
//! responding to trust changes, contribution history, and member activity.
//!
//! # Features
//!
//! - **Decay**: Limits decrease for inactive members after a threshold period
//! - **Recovery**: Limits recover instantly or gradually when members become active
//! - **Events**: Emits events when limits change for real-time notifications
//!
//! # Concurrency
//!
//! The manager uses Sled for storage which provides internal synchronization.
//! However, for correctness under concurrent access to the same (DID, currency)
//! pair, callers should serialize mutations through the ledger's transaction
//! validation layer. Specifically:
//!
//! - `get_effective_limit()` is read-only and safe to call concurrently, but
//!   provides eventual consistency - it may show stale decay if another thread
//!   just called `record_activity()`
//! - `record_activity()` and `apply_decay()` mutate state and should be
//!   serialized per (DID, currency) pair to avoid lost updates
//! - In practice, this is naturally achieved when limit checks are part of
//!   atomic ledger transaction validation
//!
//! **Recommended pattern**: Call `record_activity()` as part of transaction
//! validation to ensure the limit is current before checking the balance
//!
//! # Example
//!
//! ```rust,ignore
//! use icn_ledger::dynamic_limits::{DynamicCreditLimitManager, DynamicLimitConfig};
//! use icn_ledger::CreditPolicy;
//!
//! let config = DynamicLimitConfig::default();
//! let base_policy = CreditPolicy::conservative("hours".to_string());
//! let manager = DynamicCreditLimitManager::new(store, base_policy, config);
//!
//! // Get effective limit (includes decay if inactive)
//! let trust_score = 0.6;
//! let cleared_volume = 5000;
//! let limit = manager.get_effective_limit(&did, "hours", trust_score, cleared_volume)?;
//!
//! // Record activity to recover limits
//! if let Some(event) = manager.record_activity(&did, "hours", trust_score, cleared_volume)? {
//!     // Limit changed, emit event
//! }
//! ```

use crate::credit_policy::CreditPolicy;
use crate::error::{LedgerError, Result};
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration for dynamic credit limit behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicLimitConfig {
    /// Days of inactivity before decay starts (default: 30)
    pub inactivity_threshold_days: u64,

    /// Percentage reduction per day of inactivity after threshold (default: 0.01 = 1%)
    pub decay_rate_per_day: f64,

    /// Minimum limit floor - never decay below this (default: 1000 centihours = 10 hours)
    pub min_limit_floor: i64,

    /// Percentage recovery per successful transaction (default: 0.05 = 5%)
    /// Only used if instant_recovery_on_activity is false
    pub recovery_rate_per_transaction: f64,

    /// If true, any activity immediately restores full calculated limit
    /// If false, recovery is gradual based on recovery_rate_per_transaction
    pub instant_recovery_on_activity: bool,

    /// Enable network-aware limit adjustments based on overall ledger health.
    ///
    /// When enabled in future versions, this flag will allow the credit limit
    /// algorithms to scale individual member limits up or down in response to
    /// network-wide metrics such as utilization, default rates, and available
    /// liquidity.
    ///
    /// NOTE: Currently reserved for future use and is effectively a no-op.
    #[allow(dead_code)]
    pub enable_network_adjustment: bool,

    /// Currency this config applies to
    pub currency: String,
}

impl Default for DynamicLimitConfig {
    fn default() -> Self {
        Self {
            inactivity_threshold_days: 30,
            decay_rate_per_day: 0.01,
            min_limit_floor: 1000, // 10 hours in centihours
            recovery_rate_per_transaction: 0.05,
            instant_recovery_on_activity: true,
            enable_network_adjustment: false,
            currency: "hours".to_string(),
        }
    }
}

impl DynamicLimitConfig {
    /// Create a new config for a specific currency
    pub fn for_currency(currency: impl Into<String>) -> Self {
        Self {
            currency: currency.into(),
            ..Default::default()
        }
    }

    /// Create a conservative config with slower decay
    pub fn conservative(currency: impl Into<String>) -> Self {
        Self {
            inactivity_threshold_days: 60,
            decay_rate_per_day: 0.005, // 0.5% per day
            min_limit_floor: 1000,
            recovery_rate_per_transaction: 0.10,
            instant_recovery_on_activity: true,
            enable_network_adjustment: false,
            currency: currency.into(),
        }
    }

    /// Create an aggressive config with faster decay
    pub fn aggressive(currency: impl Into<String>) -> Self {
        Self {
            inactivity_threshold_days: 14,
            decay_rate_per_day: 0.02, // 2% per day
            min_limit_floor: 500,
            recovery_rate_per_transaction: 0.03,
            instant_recovery_on_activity: false,
            enable_network_adjustment: false,
            currency: currency.into(),
        }
    }
}

/// Tracks the current state of an account's dynamic limit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountLimitState {
    /// Account identifier
    pub did: String,

    /// Currency this state applies to
    pub currency: String,

    /// Current effective limit (after decay)
    pub current_limit: i64,

    /// Calculated limit without decay (based on trust + contributions)
    pub calculated_limit: i64,

    /// Unix timestamp of last transaction activity
    pub last_activity_timestamp: u64,

    /// Cumulative decay amount applied
    pub decay_applied: i64,

    /// When this state was last updated
    pub updated_at: u64,
}

impl AccountLimitState {
    /// Create a new state for an account
    pub fn new(did: &Did, currency: &str, calculated_limit: i64, now: u64) -> Self {
        Self {
            did: did.to_string(),
            currency: currency.to_string(),
            current_limit: calculated_limit,
            calculated_limit,
            last_activity_timestamp: now,
            decay_applied: 0,
            updated_at: now,
        }
    }

    /// Storage key for this state
    pub fn storage_key(did: &str, currency: &str) -> String {
        format!("dynamic_limit:state:{did}:{currency}")
    }

    /// Days since last activity
    pub fn days_inactive(&self, now: u64) -> u64 {
        if now <= self.last_activity_timestamp {
            return 0;
        }
        (now - self.last_activity_timestamp) / 86400 // seconds per day
    }
}

/// Reason for a credit limit change
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LimitChangeReason {
    /// Trust score changed
    TrustScoreChanged { old_score: f64, new_score: f64 },

    /// Contribution (cleared volume) increased (reserved for future use)
    #[allow(dead_code)]
    ContributionIncreased { cleared_volume: i64 },

    /// Limit decayed due to inactivity
    InactivityDecay { days_inactive: u64 },

    /// Limit recovered due to activity
    ActivityRecovery,

    /// Initial limit calculation
    InitialCalculation,

    /// Manual adjustment by administrator
    ManualAdjustment { adjusted_by: String },
}

/// Event emitted when a credit limit changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitChangeEvent {
    /// Account whose limit changed
    pub did: String,

    /// Currency affected
    pub currency: String,

    /// Previous limit
    pub old_limit: i64,

    /// New limit
    pub new_limit: i64,

    /// Why the limit changed
    pub reason: LimitChangeReason,

    /// When the change occurred
    pub timestamp: u64,
}

impl LimitChangeEvent {
    /// Create a new limit change event
    pub fn new(
        did: &Did,
        currency: &str,
        old_limit: i64,
        new_limit: i64,
        reason: LimitChangeReason,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            did: did.to_string(),
            currency: currency.to_string(),
            old_limit,
            new_limit,
            reason,
            timestamp: now,
        }
    }
}

/// Manager for dynamic credit limit calculations
pub struct DynamicCreditLimitManager {
    store: Arc<dyn Store>,
    base_policy: CreditPolicy,
    config: DynamicLimitConfig,
}

impl DynamicCreditLimitManager {
    /// Create a new dynamic limit manager
    pub fn new(
        store: Arc<dyn Store>,
        base_policy: CreditPolicy,
        config: DynamicLimitConfig,
    ) -> Self {
        Self {
            store,
            base_policy,
            config,
        }
    }

    /// Get the base policy
    pub fn base_policy(&self) -> &CreditPolicy {
        &self.base_policy
    }

    /// Get the config
    pub fn config(&self) -> &DynamicLimitConfig {
        &self.config
    }

    /// Get current Unix timestamp
    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Calculate the base limit from trust and contributions
    fn calculate_base_limit(&self, trust_score: f64, cleared_volume: i64) -> i64 {
        let trust_score = trust_score.clamp(0.0, 1.0);

        let trust_bonus = (self.base_policy.baseline as f64
            * trust_score
            * self.base_policy.trust_multiplier) as i64;

        let history_bonus = (cleared_volume as f64 * self.base_policy.history_bonus_rate) as i64;

        self.base_policy.baseline + trust_bonus + history_bonus
    }

    /// Calculate decay amount based on inactivity
    fn calculate_decay(&self, calculated_limit: i64, days_inactive: u64) -> i64 {
        if days_inactive <= self.config.inactivity_threshold_days {
            return 0;
        }

        let decay_days = days_inactive - self.config.inactivity_threshold_days;

        // Clamp decay_days to prevent i32 overflow (1000 days = ~2.7 years is reasonable max)
        let decay_days_clamped = decay_days.min(1000) as i32;

        // Compound decay: limit * (1 - rate)^days, but we calculate the reduction
        let decay_factor = (1.0 - self.config.decay_rate_per_day).powi(decay_days_clamped);
        let decayed_limit = (calculated_limit as f64 * decay_factor) as i64;

        // Decay is the difference
        let decay = calculated_limit - decayed_limit;

        // But don't decay below the floor
        let max_decay = calculated_limit - self.config.min_limit_floor;
        decay.min(max_decay.max(0))
    }

    /// Load account state from storage
    fn load_state(&self, did: &Did, currency: &str) -> Result<Option<AccountLimitState>> {
        let key = AccountLimitState::storage_key(&did.to_string(), currency);

        match self.store.get(key.as_bytes()) {
            Ok(Some(data)) => {
                let state: AccountLimitState = serde_json::from_slice(&data)
                    .map_err(|e| LedgerError::Storage(format!("deserialization error: {e}")))?;
                Ok(Some(state))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(LedgerError::Storage(e.to_string())),
        }
    }

    /// Save account state to storage
    fn save_state(&self, state: &AccountLimitState) -> Result<()> {
        let key = AccountLimitState::storage_key(&state.did, &state.currency);
        let data = serde_json::to_vec(state)
            .map_err(|e| LedgerError::Storage(format!("serialization error: {e}")))?;

        self.store
            .put(key.as_bytes(), &data)
            .map_err(|e| LedgerError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Get the effective credit limit for an account
    ///
    /// This calculates the base limit from trust/contributions, then applies
    /// any decay due to inactivity.
    pub fn get_effective_limit(
        &self,
        did: &Did,
        currency: &str,
        trust_score: f64,
        cleared_volume: i64,
    ) -> Result<i64> {
        let now = Self::now();
        let calculated_limit = self.calculate_base_limit(trust_score, cleared_volume);

        // Load existing state - this is a read-only operation
        // Note: calculated_limit in state may be stale; we use fresh calculation
        let state = match self.load_state(did, currency)? {
            Some(state) => state,
            None => {
                // New account, no decay yet
                return Ok(calculated_limit);
            }
        };

        // Calculate decay
        let days_inactive = state.days_inactive(now);
        let decay = self.calculate_decay(calculated_limit, days_inactive);

        Ok((calculated_limit - decay).max(self.config.min_limit_floor))
    }

    /// Record activity for an account
    ///
    /// This updates the last activity timestamp and potentially recovers
    /// the limit if it was decayed.
    ///
    /// Returns a LimitChangeEvent if the limit changed.
    pub fn record_activity(
        &self,
        did: &Did,
        currency: &str,
        trust_score: f64,
        cleared_volume: i64,
    ) -> Result<Option<LimitChangeEvent>> {
        let now = Self::now();
        let calculated_limit = self.calculate_base_limit(trust_score, cleared_volume);

        let mut state = match self.load_state(did, currency)? {
            Some(s) => s,
            None => {
                // First activity - create initial state
                let state = AccountLimitState::new(did, currency, calculated_limit, now);
                self.save_state(&state)?;

                return Ok(Some(LimitChangeEvent::new(
                    did,
                    currency,
                    0,
                    calculated_limit,
                    LimitChangeReason::InitialCalculation,
                )));
            }
        };

        let old_limit = state.current_limit;

        // Determine new limit based on recovery mode
        let new_limit = if self.config.instant_recovery_on_activity {
            // Instant recovery to full calculated limit
            calculated_limit
        } else {
            // Gradual recovery based on gap to calculated limit
            // This ensures consistent recovery speed regardless of limit size
            if old_limit >= calculated_limit {
                calculated_limit
            } else {
                let gap = calculated_limit - old_limit;
                // Use .max(1) to ensure at least 1 unit recovery per transaction
                // Prevents stall when gap * rate truncates to 0
                let recovery_amount =
                    ((gap as f64 * self.config.recovery_rate_per_transaction) as i64).max(1);
                (old_limit + recovery_amount).min(calculated_limit)
            }
        };

        // Update state
        state.calculated_limit = calculated_limit;
        state.current_limit = new_limit;
        state.last_activity_timestamp = now;
        state.decay_applied = calculated_limit - new_limit;
        state.updated_at = now;

        self.save_state(&state)?;

        // Emit event if limit changed
        if new_limit != old_limit {
            icn_obs::metrics::ledger::dynamic_limit_recovery_inc();
            Ok(Some(LimitChangeEvent::new(
                did,
                currency,
                old_limit,
                new_limit,
                LimitChangeReason::ActivityRecovery,
            )))
        } else {
            Ok(None)
        }
    }

    /// Apply decay to an inactive account
    ///
    /// This is typically called periodically (e.g., daily) to update limits
    /// for accounts that haven't had recent activity.
    ///
    /// Returns a LimitChangeEvent if decay was applied.
    pub fn apply_decay(&self, did: &Did, currency: &str) -> Result<Option<LimitChangeEvent>> {
        let now = Self::now();

        let mut state = match self.load_state(did, currency)? {
            Some(s) => s,
            None => return Ok(None), // No state = no decay to apply
        };

        let days_inactive = state.days_inactive(now);

        // Check if we're past the threshold
        if days_inactive <= self.config.inactivity_threshold_days {
            return Ok(None);
        }

        let old_limit = state.current_limit;
        let decay = self.calculate_decay(state.calculated_limit, days_inactive);
        let new_limit = (state.calculated_limit - decay).max(self.config.min_limit_floor);

        // Only update if limit actually changed
        if new_limit == old_limit {
            return Ok(None);
        }

        state.current_limit = new_limit;
        state.decay_applied = decay;
        state.updated_at = now;

        self.save_state(&state)?;

        // Increment metric
        icn_obs::metrics::ledger::dynamic_limit_decay_applied_inc();

        Ok(Some(LimitChangeEvent::new(
            did,
            currency,
            old_limit,
            new_limit,
            LimitChangeReason::InactivityDecay { days_inactive },
        )))
    }

    /// Recalculate limit due to trust score change
    ///
    /// Called when a trust score is updated to recalculate the limit.
    pub fn recalculate_for_trust_change(
        &self,
        did: &Did,
        currency: &str,
        old_trust: f64,
        new_trust: f64,
        cleared_volume: i64,
    ) -> Result<Option<LimitChangeEvent>> {
        let now = Self::now();
        let new_calculated = self.calculate_base_limit(new_trust, cleared_volume);

        let mut state = match self.load_state(did, currency)? {
            Some(s) => s,
            None => {
                // Create new state
                let state = AccountLimitState::new(did, currency, new_calculated, now);
                self.save_state(&state)?;
                return Ok(Some(LimitChangeEvent::new(
                    did,
                    currency,
                    0,
                    new_calculated,
                    LimitChangeReason::TrustScoreChanged {
                        old_score: old_trust,
                        new_score: new_trust,
                    },
                )));
            }
        };

        let old_limit = state.current_limit;

        // Recalculate decay based on actual inactivity time, not preserved ratio
        // This ensures decay is always time-based regardless of limit changes
        let days_inactive = state.days_inactive(now);
        let new_decay = self.calculate_decay(new_calculated, days_inactive);
        let new_limit = (new_calculated - new_decay).max(self.config.min_limit_floor);

        state.calculated_limit = new_calculated;
        state.current_limit = new_limit;
        state.decay_applied = new_decay;
        state.updated_at = now;

        self.save_state(&state)?;

        // Increment metric
        icn_obs::metrics::ledger::dynamic_limit_recalculations_inc();

        if new_limit != old_limit {
            Ok(Some(LimitChangeEvent::new(
                did,
                currency,
                old_limit,
                new_limit,
                LimitChangeReason::TrustScoreChanged {
                    old_score: old_trust,
                    new_score: new_trust,
                },
            )))
        } else {
            Ok(None)
        }
    }

    /// Get the current state for an account (for debugging/display)
    pub fn get_state(&self, did: &Did, currency: &str) -> Result<Option<AccountLimitState>> {
        self.load_state(did, currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use tempfile::TempDir;

    fn create_test_manager() -> (DynamicCreditLimitManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

        let base_policy = CreditPolicy::conservative("hours".to_string());
        let config = DynamicLimitConfig::default();

        let manager = DynamicCreditLimitManager::new(store, base_policy, config);
        (manager, temp_dir)
    }

    /// Helper to create a timestamp N days in the past
    fn days_ago(days: u64) -> u64 {
        DynamicCreditLimitManager::now().saturating_sub(days * 86400)
    }

    #[test]
    fn test_config_defaults() {
        let config = DynamicLimitConfig::default();
        assert_eq!(config.inactivity_threshold_days, 30);
        assert_eq!(config.decay_rate_per_day, 0.01);
        assert_eq!(config.min_limit_floor, 1000);
        assert!(config.instant_recovery_on_activity);
    }

    #[test]
    fn test_config_conservative() {
        let config = DynamicLimitConfig::conservative("hours");
        assert_eq!(config.inactivity_threshold_days, 60);
        assert_eq!(config.decay_rate_per_day, 0.005);
    }

    #[test]
    fn test_config_aggressive() {
        let config = DynamicLimitConfig::aggressive("hours");
        assert_eq!(config.inactivity_threshold_days, 14);
        assert_eq!(config.decay_rate_per_day, 0.02);
        assert!(!config.instant_recovery_on_activity);
    }

    #[test]
    fn test_calculate_base_limit() {
        let (manager, _temp) = create_test_manager();

        // No trust, no history = baseline only
        let limit = manager.calculate_base_limit(0.0, 0);
        assert_eq!(limit, manager.base_policy.baseline);

        // Full trust = baseline + trust bonus
        let limit = manager.calculate_base_limit(1.0, 0);
        let expected = manager.base_policy.baseline
            + (manager.base_policy.baseline as f64 * manager.base_policy.trust_multiplier) as i64;
        assert_eq!(limit, expected);

        // History bonus
        let cleared = 10000;
        let limit = manager.calculate_base_limit(0.0, cleared);
        let expected = manager.base_policy.baseline
            + (cleared as f64 * manager.base_policy.history_bonus_rate) as i64;
        assert_eq!(limit, expected);
    }

    #[test]
    fn test_calculate_decay_before_threshold() {
        let (manager, _temp) = create_test_manager();

        // Before threshold = no decay
        let decay = manager.calculate_decay(10000, 20);
        assert_eq!(decay, 0);

        // At threshold = no decay
        let decay = manager.calculate_decay(10000, 30);
        assert_eq!(decay, 0);
    }

    #[test]
    fn test_calculate_decay_after_threshold() {
        let (manager, _temp) = create_test_manager();

        // 1 day past threshold with 1% decay
        let decay = manager.calculate_decay(10000, 31);
        // 10000 * (1 - 0.99) = 100
        assert_eq!(decay, 100);

        // 10 days past threshold
        let decay = manager.calculate_decay(10000, 40);
        // 10000 * (1 - 0.99^10) = ~956
        assert!(decay > 900 && decay < 1000);
    }

    #[test]
    fn test_decay_respects_floor() {
        let (manager, _temp) = create_test_manager();

        // Very long inactivity - should not decay below floor
        let decay = manager.calculate_decay(10000, 500);
        let final_limit = 10000 - decay;
        assert!(final_limit >= manager.config.min_limit_floor);
    }

    #[test]
    fn test_get_effective_limit_new_account() {
        let (manager, _temp) = create_test_manager();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // New account gets full calculated limit (no state = no decay)
        let limit = manager
            .get_effective_limit(&did, "hours", 0.5, 5000)
            .unwrap();
        let expected = manager.calculate_base_limit(0.5, 5000);
        assert_eq!(limit, expected);
    }

    #[test]
    fn test_record_activity_creates_state() {
        let (manager, _temp) = create_test_manager();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // First activity creates state
        let event = manager.record_activity(&did, "hours", 0.5, 0).unwrap();
        assert!(event.is_some());

        let event = event.unwrap();
        assert_eq!(event.reason, LimitChangeReason::InitialCalculation);
        assert_eq!(event.old_limit, 0);
        assert!(event.new_limit > 0);

        // State should now exist
        let state = manager.get_state(&did, "hours").unwrap();
        assert!(state.is_some());
    }

    #[test]
    fn test_instant_recovery() {
        let (manager, _temp) = create_test_manager();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create initial state with activity 60 days ago
        let calculated = manager.calculate_base_limit(0.5, 0);
        let mut state = AccountLimitState::new(&did, "hours", calculated, days_ago(60));

        // Simulate decay state
        state.current_limit = calculated / 2; // Decayed to half
        state.decay_applied = calculated / 2;
        manager.save_state(&state).unwrap();

        // Record activity - should instantly recover
        let event = manager.record_activity(&did, "hours", 0.5, 0).unwrap();
        assert!(event.is_some());

        let event = event.unwrap();
        assert_eq!(event.reason, LimitChangeReason::ActivityRecovery);
        assert_eq!(event.new_limit, calculated); // Full recovery

        // Verify state updated
        let state = manager.get_state(&did, "hours").unwrap().unwrap();
        assert_eq!(state.current_limit, calculated);
        assert_eq!(state.decay_applied, 0);
    }

    #[test]
    fn test_gradual_recovery() {
        // Create manager with gradual recovery config
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let base_policy = CreditPolicy::conservative("hours".to_string());
        let mut config = DynamicLimitConfig::default();
        config.instant_recovery_on_activity = false;
        config.recovery_rate_per_transaction = 0.25; // 25% of gap per tx

        let manager = DynamicCreditLimitManager::new(store, base_policy, config);
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let calculated = manager.calculate_base_limit(0.5, 0);
        let decayed_to = calculated / 2; // Decayed to 50%

        // Create state with decayed limit (60 days ago)
        let mut state = AccountLimitState::new(&did, "hours", calculated, days_ago(60));
        state.current_limit = decayed_to;
        state.decay_applied = calculated - decayed_to;
        manager.save_state(&state).unwrap();

        // First activity - should recover 25% of the gap (with .max(1) floor)
        let event = manager.record_activity(&did, "hours", 0.5, 0).unwrap();
        assert!(event.is_some());

        let event = event.unwrap();
        let gap = calculated - decayed_to;
        let expected_recovery = ((gap as f64 * 0.25) as i64).max(1);
        let expected_limit = decayed_to + expected_recovery;

        assert_eq!(event.new_limit, expected_limit);
        assert!(event.new_limit < calculated); // Not fully recovered yet

        // Second activity - should recover 25% of remaining gap (with .max(1) floor)
        let state = manager.get_state(&did, "hours").unwrap().unwrap();
        let new_gap = calculated - state.current_limit;
        let expected_recovery2 = ((new_gap as f64 * 0.25) as i64).max(1);
        let expected_limit2 = state.current_limit + expected_recovery2;

        let event2 = manager.record_activity(&did, "hours", 0.5, 0).unwrap();
        assert!(event2.is_some());
        assert_eq!(event2.unwrap().new_limit, expected_limit2);
    }

    #[test]
    fn test_apply_decay() {
        let (manager, _temp) = create_test_manager();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create state with old activity
        let calculated = 10000;
        let mut state = AccountLimitState::new(&did, "hours", calculated, 0);
        // Set activity to 60 days ago (30 past threshold)
        state.last_activity_timestamp = DynamicCreditLimitManager::now() - (60 * 86400);
        manager.save_state(&state).unwrap();

        // Apply decay
        let event = manager.apply_decay(&did, "hours").unwrap();
        assert!(event.is_some());

        let event = event.unwrap();
        match event.reason {
            LimitChangeReason::InactivityDecay { days_inactive } => {
                assert!(days_inactive >= 30);
            }
            _ => panic!("Expected InactivityDecay reason"),
        }
        assert!(event.new_limit < event.old_limit);
    }

    #[test]
    fn test_recalculate_for_trust_change() {
        let (manager, _temp) = create_test_manager();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create initial state with low trust
        manager.record_activity(&did, "hours", 0.2, 0).unwrap();
        let old_state = manager.get_state(&did, "hours").unwrap().unwrap();

        // Trust improved significantly
        let event = manager
            .recalculate_for_trust_change(&did, "hours", 0.2, 0.8, 0)
            .unwrap();

        assert!(event.is_some());
        let event = event.unwrap();

        match event.reason {
            LimitChangeReason::TrustScoreChanged {
                old_score,
                new_score,
            } => {
                assert!((old_score - 0.2).abs() < 0.01);
                assert!((new_score - 0.8).abs() < 0.01);
            }
            _ => panic!("Expected TrustScoreChanged reason"),
        }

        assert!(event.new_limit > old_state.current_limit);
    }

    #[test]
    fn test_state_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create manager and record activity
        {
            let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
            let base_policy = CreditPolicy::conservative("hours".to_string());
            let config = DynamicLimitConfig::default();
            let manager = DynamicCreditLimitManager::new(store, base_policy, config);

            manager.record_activity(&did, "hours", 0.5, 1000).unwrap();
        }

        // Create new manager with same store
        {
            let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
            let base_policy = CreditPolicy::conservative("hours".to_string());
            let config = DynamicLimitConfig::default();
            let manager = DynamicCreditLimitManager::new(store, base_policy, config);

            // State should persist
            let state = manager.get_state(&did, "hours").unwrap();
            assert!(state.is_some());
            let state = state.unwrap();
            assert!(state.current_limit > 0);
        }
    }
}
