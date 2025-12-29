//! Progressive balance and velocity limits based on proof-of-personhood level
//!
//! This module implements POPLevel-based limits to prevent wealth concentration
//! and Sybil attacks in the mutual credit network.
//!
//! ## Balance Limits
//!
//! | POPLevel | Max Balance | Max Credit |
//! |----------|-------------|------------|
//! | Weak     | 500         | 50         |
//! | Strong   | 5,000       | 500        |
//! | Verified | 50,000      | 5,000      |
//!
//! ## Velocity Limits (NET change)
//!
//! Velocity is tracked as NET change (credits - debits), not gross volume.
//! This prevents rapid accumulation while allowing normal spending.
//!
//! | Window  | Max NET Change |
//! |---------|----------------|
//! | Daily   | 500            |
//! | Weekly  | 2,000          |
//! | Monthly | 5,000          |
//!
//! ## Usage
//!
//! ```rust,ignore
//! use icn_ledger::progressive_limits::ProgressiveLimitManager;
//!
//! let manager = ProgressiveLimitManager::new(store, commons_lookup)?;
//!
//! // In validate_entry(), after credit limit check:
//! manager.check_balance_limits(&account, &currency, new_balance, pop_level)?;
//! manager.check_velocity_limits(&account, &currency, net_change)?;
//! ```

use anyhow::{bail, Result};
use icn_identity::POPLevel;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Balance limits for a specific POPLevel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceLimits {
    /// Maximum positive balance allowed
    pub max_balance: i64,

    /// Maximum credit (negative balance) allowed
    pub max_credit: i64,
}

impl BalanceLimits {
    /// Create new balance limits
    pub fn new(max_balance: i64, max_credit: i64) -> Self {
        Self {
            max_balance,
            max_credit,
        }
    }

    /// Get default balance limits for a POPLevel
    pub fn for_level(level: POPLevel) -> Self {
        match level {
            POPLevel::Weak => Self::new(500, 50),
            POPLevel::Strong => Self::new(5_000, 500),
            POPLevel::Verified => Self::new(50_000, 5_000),
        }
    }

    /// Check if a balance exceeds the limits
    ///
    /// Returns Ok(()) if within limits, or an error describing the violation.
    pub fn check(&self, balance: i64) -> Result<()> {
        if balance > self.max_balance {
            bail!(
                "Balance {} exceeds maximum {} for verification level",
                balance,
                self.max_balance
            );
        }
        if balance < -self.max_credit {
            bail!(
                "Credit {} exceeds maximum {} for verification level",
                -balance,
                self.max_credit
            );
        }
        Ok(())
    }
}

/// Configuration for a single velocity limit window
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VelocityLimitConfig {
    /// Window size in seconds
    pub window_seconds: u64,

    /// Maximum NET change allowed in the window
    pub max_net_change: i64,
}

impl VelocityLimitConfig {
    /// Create a new velocity limit config
    pub fn new(window_seconds: u64, max_net_change: i64) -> Self {
        Self {
            window_seconds,
            max_net_change,
        }
    }

    /// Daily velocity limit (24 hours)
    pub fn daily(max_net_change: i64) -> Self {
        Self::new(86_400, max_net_change)
    }

    /// Weekly velocity limit (7 days)
    pub fn weekly(max_net_change: i64) -> Self {
        Self::new(604_800, max_net_change)
    }

    /// Monthly velocity limit (30 days)
    pub fn monthly(max_net_change: i64) -> Self {
        Self::new(2_592_000, max_net_change)
    }
}

/// Network-wide progressive limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressiveLimitConfig {
    /// Balance limits per POPLevel
    pub balance_limits: HashMap<String, BalanceLimits>,

    /// Velocity limits (applied to all POPLevels)
    pub velocity_limits: Vec<VelocityLimitConfig>,

    /// Whether progressive limits are enabled
    pub enabled: bool,
}

impl Default for ProgressiveLimitConfig {
    fn default() -> Self {
        let mut balance_limits = HashMap::new();
        balance_limits.insert("Weak".to_string(), BalanceLimits::for_level(POPLevel::Weak));
        balance_limits.insert(
            "Strong".to_string(),
            BalanceLimits::for_level(POPLevel::Strong),
        );
        balance_limits.insert(
            "Verified".to_string(),
            BalanceLimits::for_level(POPLevel::Verified),
        );

        Self {
            balance_limits,
            velocity_limits: vec![
                VelocityLimitConfig::daily(500),
                VelocityLimitConfig::weekly(2_000),
                VelocityLimitConfig::monthly(5_000),
            ],
            enabled: true,
        }
    }
}

impl ProgressiveLimitConfig {
    /// Get balance limits for a POPLevel
    pub fn get_balance_limits(&self, level: POPLevel) -> BalanceLimits {
        let key = match level {
            POPLevel::Weak => "Weak",
            POPLevel::Strong => "Strong",
            POPLevel::Verified => "Verified",
        };
        self.balance_limits
            .get(key)
            .copied()
            .unwrap_or_else(|| BalanceLimits::for_level(level))
    }
}

/// Tracks NET changes within velocity windows for a single account+currency
///
/// Similar to VelocityWindow in treasury.rs but tracks NET changes
/// (positive = credits, negative = debits) rather than just withdrawals.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountVelocityWindow {
    /// Changes within the tracking period: (timestamp_secs, net_change)
    pub changes: Vec<(u64, i64)>,

    /// Last update timestamp
    pub last_updated: u64,
}

impl AccountVelocityWindow {
    /// Create a new empty velocity window
    pub fn new() -> Self {
        Self::default()
    }

    /// Prune expired entries and calculate NET change within window
    pub fn net_change_in_window(&mut self, window_seconds: u64, now: u64) -> i64 {
        let window_start = now.saturating_sub(window_seconds);

        // Prune expired entries
        self.changes
            .retain(|(timestamp, _)| *timestamp > window_start);
        self.last_updated = now;

        // Sum remaining (positive = net credit accumulation)
        self.changes.iter().map(|(_, change)| *change).sum()
    }

    /// Record a NET change (positive = credit, negative = debit)
    pub fn record_change(&mut self, net_change: i64, now: u64) {
        self.changes.push((now, net_change));
        self.last_updated = now;
    }

    /// Check if adding `net_change` would exceed `max_net_change` in window
    ///
    /// Only checks positive accumulation - we're preventing wealth concentration,
    /// not restricting spending.
    pub fn would_exceed(
        &mut self,
        window_seconds: u64,
        max_net_change: i64,
        net_change: i64,
        now: u64,
    ) -> bool {
        let current_net = self.net_change_in_window(window_seconds, now);
        let proposed_net = current_net.saturating_add(net_change);

        // Only block positive accumulation exceeding the limit
        // Negative changes (spending) are always allowed for velocity purposes
        proposed_net > max_net_change
    }
}

/// Storage key prefix for velocity windows
const VELOCITY_WINDOW_PREFIX: &str = "progressive_velocity:";

/// Trait for looking up POPLevel from Commons Holder records
pub trait CommonsHolderLookup: Send + Sync {
    /// Get the POPLevel for a DID, or None if not found
    fn get_pop_level(&self, did: &str) -> Option<POPLevel>;
}

/// Closure-based adapter for CommonsHolderLookup
///
/// Allows using any function that can look up POPLevel by DID.
///
/// # Example
///
/// ```rust,ignore
/// use icn_ledger::progressive_limits::{FnCommonsHolderLookup, CommonsHolderLookup};
///
/// let lookup = FnCommonsHolderLookup::new(|did| {
///     commons_store.get_by_did(&Did::from_str(did)?)
///         .ok()
///         .flatten()
///         .map(|holder| holder.personhood_level)
/// });
/// ```
pub struct FnCommonsHolderLookup<F>
where
    F: Fn(&str) -> Option<POPLevel> + Send + Sync,
{
    lookup_fn: F,
}

impl<F> FnCommonsHolderLookup<F>
where
    F: Fn(&str) -> Option<POPLevel> + Send + Sync,
{
    /// Create a new lookup adapter from a function
    pub fn new(lookup_fn: F) -> Self {
        Self { lookup_fn }
    }
}

impl<F> CommonsHolderLookup for FnCommonsHolderLookup<F>
where
    F: Fn(&str) -> Option<POPLevel> + Send + Sync,
{
    fn get_pop_level(&self, did: &str) -> Option<POPLevel> {
        (self.lookup_fn)(did)
    }
}

/// A lookup implementation that always returns None (defaults to Weak)
///
/// Useful when no commons store is configured, or for testing.
pub struct DefaultWeakLookup;

impl CommonsHolderLookup for DefaultWeakLookup {
    fn get_pop_level(&self, _did: &str) -> Option<POPLevel> {
        None // Will default to Weak
    }
}

/// Progressive limit manager
///
/// Validates balance and velocity limits based on POPLevel.
pub struct ProgressiveLimitManager {
    /// Configuration
    config: ProgressiveLimitConfig,

    /// Persistent storage for velocity windows
    store: Arc<dyn Store>,

    /// Commons holder lookup for POPLevel queries
    commons_lookup: Arc<dyn CommonsHolderLookup>,
}

impl ProgressiveLimitManager {
    /// Create a new progressive limit manager with default config
    pub fn new(store: Arc<dyn Store>, commons_lookup: Arc<dyn CommonsHolderLookup>) -> Self {
        Self {
            config: ProgressiveLimitConfig::default(),
            store,
            commons_lookup,
        }
    }

    /// Create with custom config
    pub fn with_config(
        config: ProgressiveLimitConfig,
        store: Arc<dyn Store>,
        commons_lookup: Arc<dyn CommonsHolderLookup>,
    ) -> Self {
        Self {
            config,
            store,
            commons_lookup,
        }
    }

    /// Check if progressive limits are enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the POPLevel for an account, defaulting to Weak if unknown
    pub fn get_pop_level(&self, account_id: &str) -> POPLevel {
        match self.commons_lookup.get_pop_level(account_id) {
            Some(level) => level,
            None => {
                icn_obs::metrics::ledger::progressive_pop_level_defaulted_inc();
                POPLevel::Weak
            }
        }
    }

    /// Check balance limits for an account
    ///
    /// # Arguments
    /// * `account_id` - The account DID
    /// * `currency` - Currency code
    /// * `new_balance` - The proposed new balance after the transaction
    /// * `pop_level` - The account's POPLevel (or use get_pop_level() first)
    ///
    /// # Returns
    /// Ok(()) if within limits, or error describing the violation
    pub fn check_balance_limits(
        &self,
        account_id: &str,
        currency: &str,
        new_balance: i64,
        pop_level: POPLevel,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let limits = self.config.get_balance_limits(pop_level);
        limits.check(new_balance).map_err(|e| {
            anyhow::anyhow!(
                "Balance limit exceeded for {account_id} ({currency}) with {pop_level:?} verification: {e}"
            )
        })
    }

    /// Check velocity limits for an account
    ///
    /// # Arguments
    /// * `account_id` - The account DID
    /// * `currency` - Currency code
    /// * `net_change` - The NET change from this transaction (positive = credit)
    ///
    /// # Returns
    /// Ok(()) if within limits, or error describing the violation
    pub fn check_velocity_limits(
        &self,
        account_id: &str,
        currency: &str,
        net_change: i64,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Negative changes (spending) are always allowed for velocity purposes
        if net_change <= 0 {
            return Ok(());
        }

        let now = icn_time::current_timestamp_secs();
        let mut window = self.load_velocity_window(account_id, currency)?;

        for limit in &self.config.velocity_limits {
            if window.would_exceed(limit.window_seconds, limit.max_net_change, net_change, now) {
                let window_name = match limit.window_seconds {
                    86_400 => "daily",
                    604_800 => "weekly",
                    2_592_000 => "monthly",
                    _ => "custom",
                };
                let current = window.net_change_in_window(limit.window_seconds, now);
                bail!(
                    "Velocity limit exceeded for {} ({}). Net change of {} in past {} \
                     would bring total to {}, exceeding limit of {}.",
                    account_id,
                    currency,
                    net_change,
                    window_name,
                    current + net_change,
                    limit.max_net_change
                );
            }
        }

        Ok(())
    }

    /// Record a transaction's NET change for velocity tracking
    ///
    /// Call this AFTER the transaction is committed.
    pub fn record_change(&self, account_id: &str, currency: &str, net_change: i64) -> Result<()> {
        if !self.config.enabled || net_change == 0 {
            return Ok(());
        }

        let now = icn_time::current_timestamp_secs();
        let mut window = self.load_velocity_window(account_id, currency)?;
        window.record_change(net_change, now);
        self.save_velocity_window(account_id, currency, &window)?;

        Ok(())
    }

    /// Load velocity window from store
    fn load_velocity_window(
        &self,
        account_id: &str,
        currency: &str,
    ) -> Result<AccountVelocityWindow> {
        let key = format!("{VELOCITY_WINDOW_PREFIX}{account_id}:{currency}");
        match self.store.get(key.as_bytes())? {
            Some(data) => {
                let window: AccountVelocityWindow = serde_json::from_slice(&data)?;
                Ok(window)
            }
            None => Ok(AccountVelocityWindow::new()),
        }
    }

    /// Save velocity window to store
    fn save_velocity_window(
        &self,
        account_id: &str,
        currency: &str,
        window: &AccountVelocityWindow,
    ) -> Result<()> {
        let key = format!("{VELOCITY_WINDOW_PREFIX}{account_id}:{currency}");
        let data = serde_json::to_vec(window)?;
        self.store.put(key.as_bytes(), &data)?;
        Ok(())
    }

    /// Get current config (for inspection/testing)
    pub fn config(&self) -> &ProgressiveLimitConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_limits_for_level() {
        let weak = BalanceLimits::for_level(POPLevel::Weak);
        assert_eq!(weak.max_balance, 500);
        assert_eq!(weak.max_credit, 50);

        let strong = BalanceLimits::for_level(POPLevel::Strong);
        assert_eq!(strong.max_balance, 5_000);
        assert_eq!(strong.max_credit, 500);

        let verified = BalanceLimits::for_level(POPLevel::Verified);
        assert_eq!(verified.max_balance, 50_000);
        assert_eq!(verified.max_credit, 5_000);
    }

    #[test]
    fn test_balance_limits_check() {
        let limits = BalanceLimits::new(1000, 100);

        // Within limits
        assert!(limits.check(0).is_ok());
        assert!(limits.check(500).is_ok());
        assert!(limits.check(1000).is_ok());
        assert!(limits.check(-50).is_ok());
        assert!(limits.check(-100).is_ok());

        // Exceeds balance
        assert!(limits.check(1001).is_err());
        assert!(limits.check(5000).is_err());

        // Exceeds credit
        assert!(limits.check(-101).is_err());
        assert!(limits.check(-500).is_err());
    }

    #[test]
    fn test_velocity_limit_config() {
        let daily = VelocityLimitConfig::daily(500);
        assert_eq!(daily.window_seconds, 86_400);
        assert_eq!(daily.max_net_change, 500);

        let weekly = VelocityLimitConfig::weekly(2000);
        assert_eq!(weekly.window_seconds, 604_800);
        assert_eq!(weekly.max_net_change, 2000);

        let monthly = VelocityLimitConfig::monthly(5000);
        assert_eq!(monthly.window_seconds, 2_592_000);
        assert_eq!(monthly.max_net_change, 5000);
    }

    #[test]
    fn test_default_config() {
        let config = ProgressiveLimitConfig::default();

        assert!(config.enabled);
        assert_eq!(config.velocity_limits.len(), 3);

        // Check balance limits are present
        assert_eq!(config.get_balance_limits(POPLevel::Weak).max_balance, 500);
        assert_eq!(
            config.get_balance_limits(POPLevel::Strong).max_balance,
            5_000
        );
        assert_eq!(
            config.get_balance_limits(POPLevel::Verified).max_balance,
            50_000
        );
    }

    #[test]
    fn test_velocity_window_record_and_sum() {
        let mut window = AccountVelocityWindow::new();
        let now = 1000;

        // Record some changes
        window.record_change(100, now);
        window.record_change(200, now + 10);
        window.record_change(-50, now + 20); // spending

        // Check NET change
        let net = window.net_change_in_window(3600, now + 30);
        assert_eq!(net, 250); // 100 + 200 - 50
    }

    #[test]
    fn test_velocity_window_pruning() {
        let mut window = AccountVelocityWindow::new();

        // Record old changes
        window.record_change(1000, 100);
        window.record_change(500, 200);

        // Record recent change
        window.record_change(100, 10000);

        // With 1-hour window from now=10000, old entries should be pruned
        let net = window.net_change_in_window(3600, 10000);
        assert_eq!(net, 100); // Only recent change remains

        // Old entries should be removed
        assert_eq!(window.changes.len(), 1);
    }

    #[test]
    fn test_velocity_window_would_exceed() {
        let mut window = AccountVelocityWindow::new();
        let now = 1000;

        // Record 400 credits
        window.record_change(400, now);

        // Adding 100 more should be fine (400 + 100 = 500, at limit)
        assert!(!window.would_exceed(3600, 500, 100, now + 10));

        // Adding 101 more would exceed (400 + 101 = 501 > 500)
        assert!(window.would_exceed(3600, 500, 101, now + 10));

        // Negative changes never exceed (spending is allowed)
        assert!(!window.would_exceed(3600, 500, -1000, now + 10));
    }

    #[test]
    fn test_velocity_window_allows_spending() {
        let mut window = AccountVelocityWindow::new();
        let now = 1000;

        // Already at max
        window.record_change(500, now);

        // Can still spend (negative change)
        assert!(!window.would_exceed(3600, 500, -100, now + 10));

        // But cannot accumulate more
        assert!(window.would_exceed(3600, 500, 1, now + 10));
    }

    #[test]
    fn test_pop_level_ordering() {
        // POPLevel should be ordered: Weak < Strong < Verified
        assert!(POPLevel::Weak < POPLevel::Strong);
        assert!(POPLevel::Strong < POPLevel::Verified);
    }

    // ========================================================================
    // Integration tests for ProgressiveLimitManager
    // ========================================================================

    use icn_store::SledStore;
    use std::collections::HashMap as StdHashMap;
    use std::sync::RwLock;
    use tempfile::TempDir;

    /// Mock CommonsHolderLookup that stores POPLevels in a HashMap
    struct MockCommonsLookup {
        levels: RwLock<StdHashMap<String, POPLevel>>,
    }

    impl MockCommonsLookup {
        fn new() -> Self {
            Self {
                levels: RwLock::new(StdHashMap::new()),
            }
        }

        fn set_level(&self, did: &str, level: POPLevel) {
            self.levels.write().unwrap().insert(did.to_string(), level);
        }
    }

    impl CommonsHolderLookup for MockCommonsLookup {
        fn get_pop_level(&self, did: &str) -> Option<POPLevel> {
            self.levels.read().unwrap().get(did).copied()
        }
    }

    fn create_test_manager() -> (ProgressiveLimitManager, TempDir, Arc<MockCommonsLookup>) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let lookup = Arc::new(MockCommonsLookup::new());
        let manager = ProgressiveLimitManager::new(store as Arc<dyn Store>, lookup.clone());
        (manager, temp_dir, lookup)
    }

    #[test]
    fn test_manager_balance_limits_weak() {
        let (manager, _temp, lookup) = create_test_manager();
        let did = "did:icn:alice";
        lookup.set_level(did, POPLevel::Weak);

        let pop = manager.get_pop_level(did);
        assert_eq!(pop, POPLevel::Weak);

        // Weak limit: 500 balance, 50 credit
        assert!(manager.check_balance_limits(did, "hours", 0, pop).is_ok());
        assert!(manager.check_balance_limits(did, "hours", 500, pop).is_ok());
        assert!(manager
            .check_balance_limits(did, "hours", 501, pop)
            .is_err());
        assert!(manager.check_balance_limits(did, "hours", -50, pop).is_ok());
        assert!(manager
            .check_balance_limits(did, "hours", -51, pop)
            .is_err());
    }

    #[test]
    fn test_manager_balance_limits_strong() {
        let (manager, _temp, lookup) = create_test_manager();
        let did = "did:icn:bob";
        lookup.set_level(did, POPLevel::Strong);

        let pop = manager.get_pop_level(did);
        assert_eq!(pop, POPLevel::Strong);

        // Strong limit: 5000 balance, 500 credit
        assert!(manager
            .check_balance_limits(did, "hours", 5000, pop)
            .is_ok());
        assert!(manager
            .check_balance_limits(did, "hours", 5001, pop)
            .is_err());
        assert!(manager
            .check_balance_limits(did, "hours", -500, pop)
            .is_ok());
        assert!(manager
            .check_balance_limits(did, "hours", -501, pop)
            .is_err());
    }

    #[test]
    fn test_manager_balance_limits_verified() {
        let (manager, _temp, lookup) = create_test_manager();
        let did = "did:icn:charlie";
        lookup.set_level(did, POPLevel::Verified);

        let pop = manager.get_pop_level(did);
        assert_eq!(pop, POPLevel::Verified);

        // Verified limit: 50000 balance, 5000 credit
        assert!(manager
            .check_balance_limits(did, "hours", 50000, pop)
            .is_ok());
        assert!(manager
            .check_balance_limits(did, "hours", 50001, pop)
            .is_err());
        assert!(manager
            .check_balance_limits(did, "hours", -5000, pop)
            .is_ok());
        assert!(manager
            .check_balance_limits(did, "hours", -5001, pop)
            .is_err());
    }

    #[test]
    fn test_manager_unknown_defaults_to_weak() {
        let (manager, _temp, _lookup) = create_test_manager();
        let did = "did:icn:unknown";

        // Unknown account defaults to Weak
        let pop = manager.get_pop_level(did);
        assert_eq!(pop, POPLevel::Weak);

        // Has Weak limits
        assert!(manager.check_balance_limits(did, "hours", 500, pop).is_ok());
        assert!(manager
            .check_balance_limits(did, "hours", 501, pop)
            .is_err());
    }

    #[test]
    fn test_manager_velocity_limits() {
        let (manager, _temp, _lookup) = create_test_manager();
        let did = "did:icn:alice";

        // First 500 credits should be ok (daily limit)
        assert!(manager.check_velocity_limits(did, "hours", 500).is_ok());

        // Record the change
        manager.record_change(did, "hours", 500).unwrap();

        // Another 1 should exceed daily limit (500 already in window)
        assert!(manager.check_velocity_limits(did, "hours", 1).is_err());

        // But spending (negative) is always ok
        assert!(manager.check_velocity_limits(did, "hours", -100).is_ok());
    }

    #[test]
    fn test_manager_velocity_spending_allowed() {
        let (manager, _temp, _lookup) = create_test_manager();
        let did = "did:icn:spender";

        // Can always spend (negative changes) regardless of velocity
        assert!(manager.check_velocity_limits(did, "hours", -10000).is_ok());
    }

    #[test]
    fn test_manager_velocity_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let lookup = Arc::new(MockCommonsLookup::new());
        let did = "did:icn:persist";

        // Create manager and record some velocity
        {
            let manager =
                ProgressiveLimitManager::new(store.clone() as Arc<dyn Store>, lookup.clone());
            manager.record_change(did, "hours", 400).unwrap();
        }

        // Create new manager with same store - velocity should persist
        {
            let manager = ProgressiveLimitManager::new(store as Arc<dyn Store>, lookup);
            // Only 100 more should be allowed (daily limit 500 - 400 recorded = 100)
            assert!(manager.check_velocity_limits(did, "hours", 100).is_ok());
            assert!(manager.check_velocity_limits(did, "hours", 101).is_err());
        }
    }

    #[test]
    fn test_default_weak_lookup() {
        let lookup = DefaultWeakLookup;
        assert_eq!(lookup.get_pop_level("did:icn:anyone"), None);
    }

    #[test]
    fn test_fn_commons_holder_lookup() {
        let lookup = FnCommonsHolderLookup::new(|did| {
            if did.contains("verified") {
                Some(POPLevel::Verified)
            } else if did.contains("strong") {
                Some(POPLevel::Strong)
            } else {
                None
            }
        });

        assert_eq!(
            lookup.get_pop_level("did:icn:verified_user"),
            Some(POPLevel::Verified)
        );
        assert_eq!(
            lookup.get_pop_level("did:icn:strong_user"),
            Some(POPLevel::Strong)
        );
        assert_eq!(lookup.get_pop_level("did:icn:unknown"), None);
    }

    #[test]
    fn test_manager_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let lookup = Arc::new(DefaultWeakLookup);

        let config = ProgressiveLimitConfig {
            enabled: false,
            ..Default::default()
        };

        let manager = ProgressiveLimitManager::with_config(config, store as Arc<dyn Store>, lookup);

        // When disabled, all checks pass
        assert!(!manager.is_enabled());
        assert!(manager
            .check_balance_limits("did:icn:anyone", "hours", 1_000_000, POPLevel::Weak)
            .is_ok());
        assert!(manager
            .check_velocity_limits("did:icn:anyone", "hours", 1_000_000)
            .is_ok());
    }
}
