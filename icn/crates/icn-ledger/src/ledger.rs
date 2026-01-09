//! Ledger implementation with storage

use crate::balance::compute_all_balances;
use crate::credit_policy::CreditPolicyManager;
use crate::dynamic_limits::DynamicCreditLimitManager;
use crate::error::LedgerError;
use crate::events::{BalanceChanged, SharedEventEmitter, Transfer};
use crate::fork_resolution::{
    Fork, ForkDetector, ForkResolution, ForkResolutionStrategy, ForkResolver,
};
use crate::freeze::{FreezeManager, FrozenMember};
use crate::membership::MembershipStore;
use crate::merge::{MergeDecision, QuarantineItem};
use crate::progressive_limits::ProgressiveLimitManager;
use crate::quarantine::QuarantineStore;
use crate::sync::{serialize_sync_message, LedgerSyncMessage};
use crate::types::{AccountBalances, ContentHash, JournalEntry, QuarantineReason};
use anyhow::{Context, Result};
use icn_gossip::GossipHandle;
use icn_identity::Did;
use icn_store::Store;
use icn_trust::TrustGraph;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, error, info, instrument, warn};

/// Type alias for validation hook callback
pub type ValidationHook = Box<dyn Fn(&JournalEntry) -> Result<()> + Send + Sync>;

/// Type alias for pagination cursor (timestamp, hash for tie-breaking)
pub type PaginationCursor = (u64, String);

/// Key prefix for journal entries in storage
const JOURNAL_PREFIX: &str = "ledger:journal:";

/// Key prefix for timestamp-ordered index (for efficient pagination)
/// Format: ledger:journal_ts:{timestamp:020}:{hash}
/// The timestamp is zero-padded to 20 digits for proper lexicographic ordering
const JOURNAL_TS_PREFIX: &str = "ledger:journal_ts:";

/// Statistics about forks in the ledger
#[derive(Debug, Clone)]
pub struct ForkStats {
    /// Total number of detected forks
    pub total_forks: usize,
    /// Parent hashes that have forks
    pub parents_with_forks: Vec<ContentHash>,
}

/// Key prefix for cached balances in storage
const BALANCE_PREFIX: &str = "ledger:balance:";

// Suspicious rate threshold moved to OracleConfig for per-pair configuration (Issue #474)
// Use oracle.config().get_suspicious_rate_threshold(&pair) instead

/// Key prefix for cleared volume index (total credits received per account/currency)
const CLEARED_VOLUME_PREFIX: &str = "ledger:cleared_volume:";

/// Key prefix for archived entries (from rollback operations)
const ARCHIVE_PREFIX: &str = "ledger:archive:";

/// Record of an archived entry (from rollback operations)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveRecord {
    /// The archived journal entry
    pub entry: JournalEntry,
    /// Unix timestamp when the entry was archived
    pub archived_at: u64,
    /// Reason for archival (from governance rollback proposal)
    pub reason: String,
}

/// Minimum trust score required for entry acceptance (Known+ trust level)
/// Default: 0.1 (requires at least Known trust class)
const DEFAULT_MIN_TRUST_FOR_ENTRY: f64 = 0.1;

/// Key for storing journal version in storage
const JOURNAL_VERSION_KEY: &str = "ledger:journal_version";

/// Ledger manager for double-entry mutual credit accounting
pub struct Ledger {
    /// Storage backend
    store: Arc<dyn Store>,

    /// Cached balances (in-memory for fast queries)
    cached_balances: HashMap<Did, AccountBalances>,

    /// Cleared volume index: tracks total credits received per (account, currency)
    /// Used for O(1) credit limit calculations based on transaction history
    cleared_volume_index: HashMap<(Did, String), i64>,

    /// Optional gossip handle for distributed synchronization
    gossip: Option<GossipHandle>,

    /// Quarantine store for entries that violate invariants
    quarantine: QuarantineStore,

    /// Last merge decision (for reporting)
    last_merge: Option<MergeDecision>,

    /// Fork detector for detecting conflicting entries (Phase 18 Week 5)
    fork_detector: ForkDetector,

    /// Fork resolver for resolving detected forks (Phase 18 Week 5)
    fork_resolver: ForkResolver,

    /// Freeze manager for emergency member freezes (Issue #25)
    freeze_manager: FreezeManager,

    /// Byzantine fault detector (Phase 18)
    misbehavior_detector: Option<Arc<tokio::sync::RwLock<icn_security::MisbehaviorDetector>>>,

    /// Trust graph for entry validation (wrapped in RwLock for live updates)
    trust_graph: Option<Arc<tokio::sync::RwLock<TrustGraph>>>,

    /// Minimum trust score for entry acceptance
    min_trust_for_entry: f64,

    /// Journal version counter for snapshot isolation (M7 fix)
    /// Incremented on each entry append to detect concurrent modifications
    journal_version: u64,

    /// Optional event emitter for real-time notifications
    /// When set, ledger operations emit events for WebSocket/notification systems
    event_emitter: Option<SharedEventEmitter>,

    /// Domain ID for this ledger (used in event payloads)
    domain_id: Option<String>,

    /// Optional validation hook for charter/policy enforcement
    /// Called before accepting entries. Returns Ok(()) if entry is valid,
    /// Err with reason if entry should be rejected.
    validation_hook: Option<ValidationHook>,

    /// Credit policy manager for enforcing credit limits
    /// When set, entries that would exceed credit limits are rejected.
    credit_policy_manager: Option<CreditPolicyManager>,

    /// Membership store for tracking when members joined
    /// Used to apply new member credit limit ramping
    membership_store: Option<Arc<dyn MembershipStore>>,

    /// Progressive limit manager for POPLevel-based balance and velocity limits
    /// When set, entries that exceed limits based on verification level are rejected.
    progressive_limit_manager: Option<Arc<ProgressiveLimitManager>>,

    /// Dynamic credit limit manager for adaptive limits based on activity/trust
    /// When set, provides decay on inactivity and recovery on activity.
    /// Takes precedence over static credit_policy_manager for limit calculations.
    dynamic_limit_manager: Option<Arc<DynamicCreditLimitManager>>,

    /// Exchange rate oracle for multi-currency support
    /// When set, enables currency conversion operations.
    oracle_manager: Option<Arc<crate::oracle::OracleManager>>,

    /// FX configuration for cross-currency transfers
    /// When set, enables cross-currency payment operations.
    fx_config: Option<crate::fx::FxConfig>,
}

impl Ledger {
    /// Create a new ledger with the given storage backend
    pub fn new(store: Arc<dyn Store>) -> Result<Self> {
        let quarantine = QuarantineStore::new(store.clone());
        let freeze_manager = FreezeManager::with_store(store.clone())?;

        // Load journal version from storage (or default to 0)
        let journal_version = Self::load_journal_version_from_store(&store)?;

        let mut ledger = Ledger {
            store,
            cached_balances: HashMap::new(),
            cleared_volume_index: HashMap::new(),
            gossip: None,
            quarantine,
            last_merge: None,
            fork_detector: ForkDetector::new(),
            fork_resolver: ForkResolver::new(ForkResolutionStrategy::default()), // Hybrid strategy
            misbehavior_detector: None, // Set via set_misbehavior_detector()
            freeze_manager,
            trust_graph: None,
            min_trust_for_entry: DEFAULT_MIN_TRUST_FOR_ENTRY,
            journal_version,
            event_emitter: None,             // Set via set_event_emitter()
            domain_id: None,                 // Set via set_domain_id()
            validation_hook: None,           // Set via set_validation_hook()
            credit_policy_manager: None,     // Set via set_credit_policy_manager()
            membership_store: None,          // Set via set_membership_store()
            progressive_limit_manager: None, // Set via set_progressive_limit_manager()
            dynamic_limit_manager: None,     // Set via set_dynamic_limit_manager()
            oracle_manager: None,            // Set via set_oracle_manager()
            fx_config: None,                 // Set via set_fx_config()
        };

        // Load cached balances from storage
        ledger.load_cached_balances()?;

        // Load cleared volume index from storage
        ledger.load_cleared_volume_index()?;

        // Ensure timestamp index exists (migrates existing ledgers)
        ledger.ensure_timestamp_index()?;

        // Index existing entries for fork detection
        ledger.rebuild_fork_index()?;

        Ok(ledger)
    }

    /// Load journal version from storage
    fn load_journal_version_from_store(store: &Arc<dyn Store>) -> Result<u64> {
        match store.get(JOURNAL_VERSION_KEY.as_bytes())? {
            Some(bytes) => {
                let version: u64 = serde_json::from_slice(&bytes)?;
                debug!(version, "Loaded journal version from storage");
                Ok(version)
            }
            None => {
                debug!("No journal version in storage, starting at 0");
                Ok(0)
            }
        }
    }

    /// Save journal version to storage
    fn save_journal_version(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&self.journal_version)?;
        self.store.put(JOURNAL_VERSION_KEY.as_bytes(), &bytes)?;
        Ok(())
    }

    /// Get the current journal version (for snapshot isolation)
    pub fn journal_version(&self) -> u64 {
        self.journal_version
    }

    /// Set the trust graph for trust-weighted fork resolution and entry validation
    pub fn set_trust_graph(&mut self, trust_graph: Arc<tokio::sync::RwLock<TrustGraph>>) {
        self.trust_graph = Some(trust_graph.clone());
        self.fork_resolver.set_trust_graph(trust_graph);
    }

    /// Set the minimum trust score required for entry acceptance
    /// Default is 0.1 (Known trust class)
    pub fn set_min_trust_for_entry(&mut self, min_trust: f64) {
        self.min_trust_for_entry = min_trust;
    }

    /// Set the fork resolution strategy
    pub fn set_fork_resolution_strategy(&mut self, strategy: ForkResolutionStrategy) {
        self.fork_resolver = ForkResolver::new(strategy);
    }

    /// Set the gossip handle for distributed synchronization
    pub fn set_gossip(&mut self, gossip: GossipHandle) {
        self.gossip = Some(gossip);
    }

    /// Set the misbehavior detector for Byzantine fault detection (Phase 18)
    pub fn set_misbehavior_detector(
        &mut self,
        detector: Arc<tokio::sync::RwLock<icn_security::MisbehaviorDetector>>,
    ) {
        self.misbehavior_detector = Some(detector);
    }

    /// Set the event emitter for real-time notifications
    ///
    /// When set, the ledger will emit events for:
    /// - Transaction creation
    /// - Balance changes
    /// - Member freeze/unfreeze
    /// - Fork detection/resolution
    /// - Rollback operations
    pub fn set_event_emitter(&mut self, emitter: SharedEventEmitter) {
        self.event_emitter = Some(emitter);
    }

    /// Get a reference to the event emitter (if set)
    pub fn event_emitter(&self) -> Option<&SharedEventEmitter> {
        self.event_emitter.as_ref()
    }

    /// Set the domain ID for this ledger
    ///
    /// The domain ID is included in event payloads to identify
    /// which cooperative/domain the events belong to.
    pub fn set_domain_id(&mut self, domain_id: String) {
        self.domain_id = Some(domain_id);
    }

    /// Get the domain ID (if set)
    pub fn domain_id(&self) -> Option<&str> {
        self.domain_id.as_deref()
    }

    /// Set validation hook for charter/policy enforcement
    ///
    /// The hook is called before accepting entries into the ledger.
    /// If the hook returns Err, the entry will be rejected.
    pub fn set_validation_hook<F>(&mut self, hook: F)
    where
        F: Fn(&JournalEntry) -> Result<()> + Send + Sync + 'static,
    {
        self.validation_hook = Some(Box::new(hook));
    }

    /// Set the credit policy manager for credit limit enforcement
    ///
    /// When set, entries are validated against credit limits before acceptance.
    /// Entries that would cause accounts to exceed their credit limits are rejected.
    pub fn set_credit_policy_manager(&mut self, manager: CreditPolicyManager) {
        self.credit_policy_manager = Some(manager);
    }

    /// Get the credit policy manager (if set)
    pub fn credit_policy_manager(&self) -> Option<&CreditPolicyManager> {
        self.credit_policy_manager.as_ref()
    }

    /// Set the membership store for tracking when members joined
    ///
    /// When set, credit limits use actual join dates for new member ramping.
    /// New members start with conservative limits that increase over time.
    pub fn set_membership_store(&mut self, store: Arc<dyn MembershipStore>) {
        self.membership_store = Some(store);
    }

    /// Get the membership store (if set)
    pub fn membership_store(&self) -> Option<&Arc<dyn MembershipStore>> {
        self.membership_store.as_ref()
    }

    /// Set the progressive limit manager for POPLevel-based balance and velocity limits
    ///
    /// When set, entries are validated against:
    /// - Balance limits based on the account's proof-of-personhood level
    /// - Velocity limits to prevent rapid wealth accumulation
    pub fn set_progressive_limit_manager(&mut self, manager: Arc<ProgressiveLimitManager>) {
        self.progressive_limit_manager = Some(manager);
    }

    /// Get the progressive limit manager (if set)
    pub fn progressive_limit_manager(&self) -> Option<&Arc<ProgressiveLimitManager>> {
        self.progressive_limit_manager.as_ref()
    }

    /// Set the dynamic credit limit manager for adaptive decay/recovery
    ///
    /// When set, this manager provides:
    /// - Decay: Limits decrease after inactivity threshold (default 30 days)
    /// - Recovery: Limits restore on activity (instant or gradual)
    /// - Trust-weighted calculation integrated with base CreditPolicy
    ///
    /// This takes precedence over static credit_policy_manager for limit calculations.
    /// The manager's state is automatically updated when entries are appended.
    ///
    /// # Thread-Safety
    /// The manager's load-modify-save operations are serialized through Ledger's
    /// `&mut self` requirement. Concurrent transactions are naturally serialized
    /// per (DID, currency) pair by the ledger's transaction validation layer.
    ///
    /// # Warning
    /// If no trust graph is set, all trust scores will default to 0.0, which
    /// results in baseline-only credit limits (no trust multiplier bonus).
    pub fn set_dynamic_limit_manager(&mut self, manager: Arc<DynamicCreditLimitManager>) {
        if self.trust_graph.is_none() {
            warn!("Dynamic credit limits set without trust graph; all trust scores will be 0.0");
        }
        self.dynamic_limit_manager = Some(manager);
    }

    /// Get the dynamic credit limit manager (if set)
    pub fn dynamic_limit_manager(&self) -> Option<&Arc<DynamicCreditLimitManager>> {
        self.dynamic_limit_manager.as_ref()
    }

    /// Set the exchange rate oracle for multi-currency support
    ///
    /// When set, enables:
    /// - `convert_amount()` for currency conversion
    /// - Cross-currency payment support (future)
    ///
    /// # Example
    /// ```rust,no_run
    /// # use icn_ledger::{Ledger, OracleManager, ManualRateSource, CurrencyPair};
    /// # use icn_store::SledStore;
    /// # use std::sync::Arc;
    /// # fn example() -> anyhow::Result<()> {
    /// let store = Arc::new(SledStore::temporary()?);
    /// let mut ledger = Ledger::new(store.clone())?;
    ///
    /// // Create oracle with manual rate source
    /// let oracle = OracleManager::new(store.clone());
    /// let manual_source = ManualRateSource::new(store.clone());
    /// manual_source.set_rate(&CurrencyPair::new("hours", "USD"), 25.0, "did:icn:admin", None)?;
    ///
    /// ledger.set_oracle_manager(Arc::new(oracle));
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_oracle_manager(&mut self, oracle: Arc<crate::oracle::OracleManager>) {
        self.oracle_manager = Some(oracle);
    }

    /// Get the oracle manager (if set)
    pub fn oracle_manager(&self) -> Option<&Arc<crate::oracle::OracleManager>> {
        self.oracle_manager.as_ref()
    }

    /// Convert an amount between currencies using the oracle
    ///
    /// Returns `None` if no oracle is configured.
    /// Returns an error if the conversion fails (e.g., rate not available).
    ///
    /// # Arguments
    /// * `amount` - The amount to convert (in smallest unit)
    /// * `from` - Source currency code
    /// * `to` - Target currency code
    ///
    /// # Example
    /// ```rust,no_run
    /// # use icn_ledger::Ledger;
    /// # async fn example(ledger: &Ledger) -> anyhow::Result<()> {
    /// // Convert 10 hours to USD (requires oracle to be configured)
    /// if let Some(usd_amount) = ledger.convert_amount(10, "hours", "USD").await? {
    ///     println!("10 hours = {} USD", usd_amount);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn convert_amount(&self, amount: i64, from: &str, to: &str) -> Result<Option<i64>> {
        let oracle = match &self.oracle_manager {
            Some(o) => o,
            None => return Ok(None),
        };

        let converted = oracle.convert_amount(amount, from, to).await?;
        Ok(Some(converted))
    }

    /// Set the FX configuration for cross-currency transfers
    ///
    /// This must be set before calling cross-currency transfer methods.
    /// The clearing account DID in the config must be a valid system account.
    pub fn set_fx_config(&mut self, config: crate::fx::FxConfig) {
        self.fx_config = Some(config);
    }

    /// Get the FX configuration (if set)
    pub fn fx_config(&self) -> Option<&crate::fx::FxConfig> {
        self.fx_config.as_ref()
    }

    /// Get prerequisites for cross-currency transfers
    ///
    /// Returns cloned copies of FxConfig and OracleManager for use outside
    /// the ledger lock. This enables fetching exchange rates without holding
    /// a lock on the ledger.
    ///
    /// # Returns
    /// Tuple of (FxConfig, Arc<OracleManager>)
    ///
    /// # Errors
    /// - `FxError::NotConfigured` if FX config not set
    /// - `FxError::OracleNotConfigured` if oracle not set
    pub fn fx_prerequisites(
        &self,
    ) -> std::result::Result<
        (crate::fx::FxConfig, Arc<crate::oracle::OracleManager>),
        crate::fx::FxError,
    > {
        use crate::fx::FxError;

        let fx_config = self.fx_config.clone().ok_or(FxError::NotConfigured)?;

        let oracle = self
            .oracle_manager
            .clone()
            .ok_or(FxError::OracleNotConfigured)?;

        Ok((fx_config, oracle))
    }

    /// Prepare a cross-currency transfer without committing
    ///
    /// This fetches the current exchange rate and builds a journal entry
    /// that can be inspected before committing. The prepared transfer
    /// expires after the rate's TTL.
    ///
    /// # Arguments
    /// * `from` - Sender DID
    /// * `to` - Recipient DID
    /// * `source_amount` - Amount to debit from sender (in source currency)
    /// * `from_currency` - Source currency code
    /// * `to_currency` - Target currency code
    /// * `max_target_amount` - Optional slippage protection (reject if converted > max)
    ///
    /// # Errors
    /// - `FxError::NotConfigured` if FX config not set
    /// - `FxError::OracleNotConfigured` if oracle not set
    /// - `FxError::SameCurrency` if from/to currencies match
    /// - `FxError::RateNotAvailable` if oracle has no rate
    /// - `FxError::StaleRate` if rate is stale and not allowed
    /// - `FxError::SlippageExceeded` if converted amount exceeds max
    pub async fn prepare_cross_currency_transfer(
        &self,
        from: &Did,
        to: &Did,
        source_amount: i64,
        from_currency: &str,
        to_currency: &str,
        max_target_amount: Option<i64>,
    ) -> std::result::Result<crate::fx::PreparedFxTransfer, crate::fx::FxError> {
        use crate::entry::JournalEntryBuilder;
        use crate::fx::{FxConversionDetails, FxError, PreparedFxTransfer};
        use crate::oracle::CurrencyPair;

        // Validate configuration
        let fx_config = self.fx_config.as_ref().ok_or(FxError::NotConfigured)?;

        let oracle = self
            .oracle_manager
            .as_ref()
            .ok_or(FxError::OracleNotConfigured)?;

        // Validate currencies differ
        if from_currency == to_currency {
            return Err(FxError::SameCurrency(from_currency.to_string()));
        }

        // Validate amount
        if source_amount <= 0 {
            return Err(FxError::InvalidAmount(format!(
                "Source amount must be positive, got: {source_amount}"
            )));
        }

        // Get exchange rate from oracle
        let pair = CurrencyPair::new(from_currency, to_currency);
        let rate = oracle
            .get_rate(&pair)
            .await
            .map_err(|e| FxError::RateNotAvailable {
                from: from_currency.to_string(),
                to: to_currency.to_string(),
                reason: e.to_string(),
            })?;

        // Validate rate staleness using centralized helper
        fx_config.validate_rate_staleness(&rate, from_currency, to_currency)?;

        // Convert amount using oracle rate
        let gross_target_amount = rate.convert(source_amount).ok_or_else(|| {
            // Log a warning if overflow occurs with a suspiciously high rate
            let threshold = oracle.config().get_suspicious_rate_threshold(&pair);
            if rate.rate > threshold {
                tracing::warn!(
                    rate = rate.rate,
                    threshold = threshold,
                    source_amount = source_amount,
                    from_currency = from_currency,
                    to_currency = to_currency,
                    "Conversion overflow with high exchange rate - possible oracle misconfiguration"
                );
            }
            FxError::ConversionOverflow {
                source_amount,
                from_currency: from_currency.to_string(),
                rate: rate.rate,
            }
        })?;

        // Check slippage if max specified
        if let Some(max) = max_target_amount {
            if gross_target_amount > max {
                return Err(FxError::SlippageExceeded {
                    expected: max,
                    actual: gross_target_amount,
                    max_slippage_bps: fx_config.max_slippage_basis_points,
                });
            }
        }

        // Calculate fee and net amount
        let fee_amount = fx_config.calculate_fee(gross_target_amount);
        let net_target_amount = gross_target_amount - fee_amount;

        // Build 4-leg (or 5-leg with fee) journal entry
        // In ICN mutual credit: CREDIT = sending (balance decreases), DEBIT = receiving (balance increases)
        // 1. CREDIT sender (source currency) - sender's balance decreases
        // 2. DEBIT clearing (source currency) - clearing receives source
        // 3. CREDIT clearing (target currency) - clearing sends target
        // 4. DEBIT recipient (target currency) - recipient's balance increases
        // 5. DEBIT treasury (target currency) - treasury receives fee, only if fee > 0
        let mut builder = JournalEntryBuilder::new(from.clone())
            .credit(from.clone(), from_currency.to_string(), source_amount)
            .debit(
                fx_config.clearing_did.clone(),
                from_currency.to_string(),
                source_amount,
            )
            .credit(
                fx_config.clearing_did.clone(),
                to_currency.to_string(),
                gross_target_amount,
            );

        // Add treasury fee leg if applicable
        // Simplified logic: fee is only collected when both fee_amount > 0 AND treasury is configured
        // Note: calculate_fee() already returns 0 when treasury_did.is_none(), but we make this
        // explicit here for clarity and to prevent any drift between the two conditions.
        let (recipient_amount, has_treasury_fee) =
            if fee_amount > 0 && fx_config.treasury_did.is_some() {
                (net_target_amount, true)
            } else {
                (gross_target_amount, false)
            };

        if has_treasury_fee {
            // Use if-let pattern to satisfy clippy::unwrap_used lint
            // has_treasury_fee is true only when treasury_did.is_some()
            if let Some(treasury) = &fx_config.treasury_did {
                builder = builder
                    .debit(treasury.clone(), to_currency.to_string(), fee_amount)
                    .debit(to.clone(), to_currency.to_string(), recipient_amount);
            }
        } else {
            builder = builder.debit(to.clone(), to_currency.to_string(), recipient_amount);
        }

        let entry = builder
            .build()
            .map_err(|e| FxError::InvalidAmount(format!("Failed to build journal entry: {e}")))?;

        // Calculate expiration (use rate TTL)
        let valid_until = rate.aggregated_at.saturating_add(rate.ttl_secs);

        // Build conversion details for audit
        // Note: recipient_amount is calculated above and matches the journal entry logic
        let details = FxConversionDetails::new(
            from.to_string(),
            to.to_string(),
            from_currency.to_string(),
            to_currency.to_string(),
            source_amount,
            rate.rate,
            rate.aggregated_at,
            rate.sources(),
            rate.is_stale,
            gross_target_amount,
            fee_amount,
            recipient_amount, // Uses same logic as journal entry builder
            fx_config.fee_basis_points,
        );

        Ok(PreparedFxTransfer {
            entry,
            details,
            valid_until,
        })
    }

    /// Prepare a cross-currency transfer with a pre-fetched exchange rate
    ///
    /// This is a synchronous version of `prepare_cross_currency_transfer` that
    /// takes an already-fetched exchange rate. Use this when you need to avoid
    /// holding a lock during the async oracle call.
    ///
    /// # Workflow
    /// 1. Call `fx_prerequisites()` to get FxConfig and OracleManager (short lock)
    /// 2. Fetch rate from oracle outside the lock: `oracle.get_rate(&pair).await`
    /// 3. Call this method with the fetched rate (read lock, but sync - no await)
    /// 4. Call `execute_cross_currency_transfer()` to commit (write lock)
    ///
    /// # Arguments
    /// * `from` - Sender DID
    /// * `to` - Recipient DID
    /// * `source_amount` - Amount to debit from sender (in source currency)
    /// * `from_currency` - Source currency code
    /// * `to_currency` - Target currency code
    /// * `max_target_amount` - Optional slippage protection
    /// * `rate` - Pre-fetched exchange rate from the oracle
    ///
    /// # Errors
    /// - `FxError::NotConfigured` if FX config not set
    /// - `FxError::SameCurrency` if from/to currencies match
    /// - `FxError::StaleRate` / `FxError::StaleRateAge` if rate is stale
    /// - `FxError::SlippageExceeded` if converted amount exceeds max
    /// - `FxError::RateNotAvailable` if rate is for wrong currency pair
    pub fn prepare_cross_currency_transfer_with_rate(
        &self,
        from: &Did,
        to: &Did,
        source_amount: i64,
        from_currency: &str,
        to_currency: &str,
        max_target_amount: Option<i64>,
        rate: &crate::oracle::ExchangeRate,
    ) -> std::result::Result<crate::fx::PreparedFxTransfer, crate::fx::FxError> {
        use crate::entry::JournalEntryBuilder;
        use crate::fx::{FxConversionDetails, FxError, PreparedFxTransfer};

        // Validate configuration
        let fx_config = self.fx_config.as_ref().ok_or(FxError::NotConfigured)?;

        // Validate currencies differ
        if from_currency == to_currency {
            return Err(FxError::SameCurrency(from_currency.to_string()));
        }

        // Validate amount
        if source_amount <= 0 {
            return Err(FxError::InvalidAmount(format!(
                "Source amount must be positive, got: {source_amount}"
            )));
        }

        // Validate rate is for the correct currency pair
        // Note: This comparison is intentionally case-sensitive. Currency codes
        // should be normalized (e.g., uppercase) at the API boundary. The oracle
        // and ledger use consistent codes from CurrencyPair::new() which stores
        // strings as-is without normalization.
        if rate.pair.from != from_currency || rate.pair.to != to_currency {
            return Err(FxError::RateNotAvailable {
                from: from_currency.to_string(),
                to: to_currency.to_string(),
                reason: format!(
                    "Rate is for {}/{} but expected {}/{}",
                    rate.pair.from, rate.pair.to, from_currency, to_currency
                ),
            });
        }

        // Validate rate staleness using centralized helper
        fx_config.validate_rate_staleness(rate, from_currency, to_currency)?;

        // Convert amount using oracle rate
        let gross_target_amount = rate.convert(source_amount).ok_or_else(|| {
            // Log a warning if overflow occurs with a suspiciously high rate
            // Use per-pair threshold from oracle config, or default if no oracle configured
            let threshold = self
                .oracle_manager
                .as_ref()
                .map(|oracle| oracle.config().get_suspicious_rate_threshold(&rate.pair))
                .unwrap_or(crate::oracle::DEFAULT_SUSPICIOUS_RATE_THRESHOLD);
            if rate.rate > threshold {
                tracing::warn!(
                    rate = rate.rate,
                    threshold = threshold,
                    source_amount = source_amount,
                    from_currency = from_currency,
                    to_currency = to_currency,
                    "Conversion overflow with high exchange rate - possible oracle misconfiguration"
                );
            }
            FxError::ConversionOverflow {
                source_amount,
                from_currency: from_currency.to_string(),
                rate: rate.rate,
            }
        })?;

        // Check slippage if max specified
        if let Some(max) = max_target_amount {
            if gross_target_amount > max {
                return Err(FxError::SlippageExceeded {
                    expected: max,
                    actual: gross_target_amount,
                    max_slippage_bps: fx_config.max_slippage_basis_points,
                });
            }
        }

        // Calculate fee and net amount
        let fee_amount = fx_config.calculate_fee(gross_target_amount);
        let net_target_amount = gross_target_amount - fee_amount;

        // Build 4-leg (or 5-leg with fee) journal entry
        let mut builder = JournalEntryBuilder::new(from.clone())
            .credit(from.clone(), from_currency.to_string(), source_amount)
            .debit(
                fx_config.clearing_did.clone(),
                from_currency.to_string(),
                source_amount,
            )
            .credit(
                fx_config.clearing_did.clone(),
                to_currency.to_string(),
                gross_target_amount,
            );

        // Add treasury fee leg if applicable
        let (recipient_amount, has_treasury_fee) =
            if fee_amount > 0 && fx_config.treasury_did.is_some() {
                (net_target_amount, true)
            } else {
                (gross_target_amount, false)
            };

        if has_treasury_fee {
            if let Some(treasury) = &fx_config.treasury_did {
                builder = builder
                    .debit(treasury.clone(), to_currency.to_string(), fee_amount)
                    .debit(to.clone(), to_currency.to_string(), recipient_amount);
            }
        } else {
            builder = builder.debit(to.clone(), to_currency.to_string(), recipient_amount);
        }

        let entry = builder
            .build()
            .map_err(|e| FxError::InvalidAmount(format!("Failed to build journal entry: {e}")))?;

        // Calculate expiration (use rate TTL)
        let valid_until = rate.aggregated_at.saturating_add(rate.ttl_secs);

        // Build conversion details for audit
        let details = FxConversionDetails::new(
            from.to_string(),
            to.to_string(),
            from_currency.to_string(),
            to_currency.to_string(),
            source_amount,
            rate.rate,
            rate.aggregated_at,
            rate.sources(),
            rate.is_stale,
            gross_target_amount,
            fee_amount,
            recipient_amount,
            fx_config.fee_basis_points,
        );

        Ok(PreparedFxTransfer {
            entry,
            details,
            valid_until,
        })
    }

    /// Execute a prepared cross-currency transfer
    ///
    /// Commits the journal entry from a prepared transfer to the ledger.
    /// The prepared transfer must not be expired.
    ///
    /// # Arguments
    /// * `prepared` - The prepared transfer from `prepare_cross_currency_transfer`
    ///
    /// # Returns
    /// The content hash of the committed entry and conversion details.
    pub async fn execute_cross_currency_transfer(
        &mut self,
        prepared: crate::fx::PreparedFxTransfer,
    ) -> std::result::Result<(ContentHash, crate::fx::FxConversionDetails), crate::fx::FxError>
    {
        use crate::fx::FxError;

        // Check if prepared transfer is still valid (includes safety margin)
        prepared.validate_expiry()?;

        let now = crate::current_timestamp_secs();

        // Append the entry to the ledger
        let hash = self
            .append_entry(prepared.entry)
            .await
            .map_err(|e| FxError::InvalidAmount(format!("Failed to append entry: {e}")))?;

        // Emit cross-currency transfer event
        if let Some(ref emitter) = self.event_emitter {
            // Parse DIDs from the details (they were just converted to strings from DIDs)
            if let (Ok(from_did), Ok(to_did)) = (
                Did::from_str(&prepared.details.from_did),
                Did::from_str(&prepared.details.to_did),
            ) {
                emitter.emit_cross_currency_transfer(
                    &hash,
                    &from_did,
                    &to_did,
                    prepared.details.source_amount,
                    &prepared.details.from_currency,
                    prepared.details.gross_target_amount,
                    prepared.details.fee_amount,
                    prepared.details.net_target_amount,
                    &prepared.details.to_currency,
                    prepared.details.rate,
                    prepared.details.rate_timestamp,
                    prepared.details.rate_sources.clone(),
                    now,
                    self.domain_id.clone(),
                );
            }
        }

        Ok((hash, prepared.details))
    }

    /// Execute a cross-currency transfer in one step
    ///
    /// Convenience method that prepares and executes a transfer atomically.
    /// Use `prepare_cross_currency_transfer` + `execute_cross_currency_transfer`
    /// if you need to inspect the transfer before committing.
    ///
    /// # Arguments
    /// * `from` - Sender DID
    /// * `to` - Recipient DID
    /// * `source_amount` - Amount to debit from sender (in source currency)
    /// * `from_currency` - Source currency code
    /// * `to_currency` - Target currency code
    /// * `max_target_amount` - Optional slippage protection
    ///
    /// # Returns
    /// The content hash and conversion details.
    pub async fn transfer_with_conversion(
        &mut self,
        from: &Did,
        to: &Did,
        source_amount: i64,
        from_currency: &str,
        to_currency: &str,
        max_target_amount: Option<i64>,
    ) -> std::result::Result<(ContentHash, crate::fx::FxConversionDetails), crate::fx::FxError>
    {
        let prepared = self
            .prepare_cross_currency_transfer(
                from,
                to,
                source_amount,
                from_currency,
                to_currency,
                max_target_amount,
            )
            .await?;

        self.execute_cross_currency_transfer(prepared).await
    }

    /// Append a journal entry to the ledger
    #[instrument(skip(self, entry), fields(entry_hash = entry.id.as_ref().map(|h| h.to_hex()).unwrap_or_else(|| "none".to_string()), account_count = entry.accounts.len()))]
    pub async fn append_entry(&mut self, entry: JournalEntry) -> Result<ContentHash> {
        self.append_entry_internal(entry, true).await
    }

    /// Append a journal entry without publishing to gossip
    /// Used when receiving entries from gossip to avoid re-broadcasting
    pub async fn append_entry_from_sync(&mut self, entry: JournalEntry) -> Result<ContentHash> {
        self.append_entry_internal(entry, false).await
    }

    /// Internal append method with control over gossip publishing
    ///
    /// # Arguments
    /// * `entry` - The journal entry to append
    /// * `broadcast` - Whether to publish to gossip (false when receiving from gossip)
    async fn append_entry_internal(
        &mut self,
        entry: JournalEntry,
        broadcast: bool,
    ) -> Result<ContentHash> {
        // Validate the entry has a hash
        let hash = entry
            .id
            .as_ref()
            .context("Entry must have a computed hash before appending")?
            .clone();

        // Trust-based entry validation (H5 fix)
        // Skip trust check if no trust graph configured (allows local-only operation)
        if let Some(ref trust_graph) = self.trust_graph {
            let author_did = &entry.author;
            let min_trust = self.min_trust_for_entry;

            // Acquire read lock on trust graph using async read
            let trust_result: Result<f64, anyhow::Error> = {
                let graph = trust_graph.read().await;
                graph
                    .compute_trust_score(author_did)
                    .map_err(|e| anyhow::anyhow!(e))
            };

            match trust_result {
                Ok(trust_score) => {
                    if trust_score < min_trust {
                        warn!(
                            author = %author_did,
                            trust_score = trust_score,
                            min_required = min_trust,
                            entry_hash = %hash,
                            "Rejecting entry from low-trust author"
                        );
                        icn_obs::metrics::ledger::entries_rejected_low_trust_inc();
                        anyhow::bail!(
                            "Entry author {author_did} has insufficient trust score ({trust_score:.3} < {min_trust:.3})"
                        );
                    }
                    debug!(
                        author = %author_did,
                        trust_score = trust_score,
                        "Entry author trust validated"
                    );
                }
                Err(e) => {
                    // If we can't compute trust, treat as unknown/isolated peer
                    warn!(
                        author = %author_did,
                        error = %e,
                        entry_hash = %hash,
                        "Cannot compute trust score for entry author, treating as isolated"
                    );
                    // For unknown peers, check against minimum threshold
                    if min_trust > 0.0 {
                        icn_obs::metrics::ledger::entries_rejected_low_trust_inc();
                        anyhow::bail!("Cannot verify trust for entry author {author_did}: {e}");
                    }
                }
            }
        }

        // Charter/policy validation hook (Gap #2 fix)
        // Allow external validation logic (e.g., charter rules) to validate entries
        if let Some(ref hook) = self.validation_hook {
            if let Err(e) = hook(&entry) {
                warn!(
                    entry_hash = %hash,
                    author = %entry.author,
                    error = %e,
                    "Entry failed validation hook, quarantining"
                );

                // Quarantine the entry for governance review
                let quarantine_item = QuarantineItem {
                    entry_id: hash.clone(),
                    reason: QuarantineReason::CharterViolation,
                    author: entry.author.clone(),
                    observed_at: icn_time::current_timestamp_secs(),
                    metadata: Some(e.to_string()),
                };

                self.quarantine.add(entry.clone(), quarantine_item)?;

                anyhow::bail!("Entry failed validation: {e}");
            }

            debug!(
                entry_hash = %hash,
                "Entry passed validation hook"
            );
        }

        // Validate entry invariants (double-entry, frozen members, credit limits)
        self.validate_entry(&entry)?;

        // Issue #499: Validate parent entries exist (hybrid causal ordering)
        // For local operations (broadcast=true): reject if parents are missing
        // For sync operations (broadcast=false): accept but track as orphan
        if !entry.parents.is_empty() {
            let missing_parents: Vec<_> = entry
                .parents
                .iter()
                .filter(|parent_hash| {
                    self.get_entry(parent_hash)
                        .map(|opt| opt.is_none())
                        .unwrap_or(true)
                })
                .collect();

            if !missing_parents.is_empty() {
                let missing_count = missing_parents.len();
                icn_obs::metrics::ledger::missing_parents_add(missing_count as u64);

                if broadcast {
                    // Local operation: reject entry with missing parents
                    icn_obs::metrics::ledger::entries_rejected_missing_parents_inc();
                    warn!(
                        entry_hash = %hash,
                        missing_count = missing_count,
                        missing_parents = ?missing_parents.iter().map(|h| h.to_hex()).collect::<Vec<_>>(),
                        "Rejecting local entry with missing parent entries"
                    );
                    anyhow::bail!(
                        "Entry {hash} references {missing_count} missing parent entries; ensure parents are committed first"
                    );
                } else {
                    // Sync operation: accept but track as orphan
                    icn_obs::metrics::ledger::orphan_entries_inc();
                    debug!(
                        entry_hash = %hash,
                        missing_count = missing_count,
                        "Accepting orphan entry from sync (missing {} parents)",
                        missing_count
                    );
                }
            }
        }

        // Serialize and store
        let key = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
        let value = serde_json::to_vec(&entry)?;
        self.store.put(key.as_bytes(), &value)?;

        // Add to timestamp index for efficient pagination
        // Key format: ledger:journal_ts:{timestamp:020}:{hash}
        // Zero-padding ensures lexicographic order = chronological order
        let ts_key = format!(
            "{}{:020}:{}",
            JOURNAL_TS_PREFIX,
            entry.timestamp,
            hash.to_hex()
        );
        self.store.put(ts_key.as_bytes(), hash.as_bytes())?;

        debug!(
            entry_hash = %hash,
            account_count = entry.accounts.len(),
            "Appended journal entry to store"
        );

        // Update cached balances and cleared volume index
        // Also collect balance changes for event emission
        let mut balance_changes: Vec<BalanceChanged> = Vec::new();
        let mut transfers: Vec<Transfer> = Vec::new();

        for delta in &entry.accounts {
            // Capture old balance before update
            let old_balance = self
                .cached_balances
                .get(&delta.account_id)
                .map(|b| b.get(&delta.currency))
                .unwrap_or(0);

            let account_balances = self
                .cached_balances
                .entry(delta.account_id.clone())
                .or_insert_with(|| AccountBalances::new(delta.account_id.clone()));

            account_balances.apply_delta(delta)?;

            // Capture new balance after update
            let new_balance = account_balances.get(&delta.currency);

            // Record balance change for event emission
            balance_changes.push(BalanceChanged {
                account: delta.account_id.to_string(),
                currency: delta.currency.clone(),
                old_balance,
                new_balance,
                change: new_balance - old_balance,
                entry_hash: hash.to_hex(),
                timestamp: entry.timestamp,
                domain_id: self.domain_id.clone(),
            });

            // Update cleared volume index (track total credits received)
            // Note: This index is used for credit limit calculation (history bonus).
            // Contribution tracking in MembershipStore is NOT used for credit limits.
            if let Some(credit) = delta.credit {
                let key = (delta.account_id.clone(), delta.currency.clone());
                *self.cleared_volume_index.entry(key).or_insert(0) += credit;
            }
        }

        // Register members who appear in this entry (for new member ramping).
        // Done once per unique DID after the delta loop for efficiency.
        if let Some(ref membership_store) = self.membership_store {
            // Collect unique DIDs to avoid repeated storage calls
            let unique_dids: std::collections::HashSet<_> =
                entry.accounts.iter().map(|d| &d.account_id).collect();

            for did in unique_dids {
                match membership_store.register_if_new(did, entry.timestamp) {
                    Ok(registered_at) if registered_at == entry.timestamp => {
                        // This was a new member registration
                        icn_obs::metrics::ledger::new_members_registered_inc();
                    }
                    Ok(_) => {
                        // Existing member, no action needed
                    }
                    Err(e) => {
                        warn!(
                            account = %did,
                            timestamp = entry.timestamp,
                            error = %e,
                            "Failed to register new member"
                        );
                    }
                }
            }
        }

        // Update dynamic limit manager state (record activity for all accounts)
        // This updates last_activity_timestamp and potentially recovers decayed limits.
        if let Some(ref dynamic_manager) = self.dynamic_limit_manager {
            // Collect unique (DID, currency) pairs from this entry
            let unique_pairs: std::collections::HashSet<_> = entry
                .accounts
                .iter()
                .map(|d| (&d.account_id, d.currency.as_str()))
                .collect();

            for (did, currency) in unique_pairs {
                // Get trust score for the account using try_read() to avoid blocking
                // Activity recording is non-critical, so we use 0.0 if lock unavailable
                let trust_score: f64 = if let Some(ref trust_graph) = self.trust_graph {
                    match trust_graph.try_read() {
                        Ok(graph) => graph.compute_trust_score(did).unwrap_or(0.0),
                        Err(_) => {
                            debug!(
                                account = %did,
                                "Trust graph locked during activity recording; using 0.0"
                            );
                            0.0
                        }
                    }
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);

                // Get cleared volume (already updated above for this entry)
                let cleared_volume = self
                    .cleared_volume_index
                    .get(&(did.clone(), currency.to_string()))
                    .copied()
                    .unwrap_or(0);

                match dynamic_manager.record_activity(did, currency, trust_score, cleared_volume) {
                    Ok(Some(event)) => {
                        debug!(
                            account = %did,
                            currency = currency,
                            old_limit = event.old_limit,
                            new_limit = event.new_limit,
                            reason = ?event.reason,
                            "Dynamic limit updated on activity"
                        );
                    }
                    Ok(None) => {
                        // No limit change (normal case)
                    }
                    Err(e) => {
                        // Log but don't fail the transaction - limit tracking is non-critical
                        warn!(
                            account = %did,
                            currency = currency,
                            error = ?e,
                            "Failed to record activity for dynamic limits"
                        );
                    }
                }
            }
        }

        // Build transfers list from deltas (pair debits with credits)
        // Group by currency and match debits to credits
        let mut debits_by_currency: HashMap<String, Vec<(&icn_identity::Did, i64)>> =
            HashMap::new();
        let mut credits_by_currency: HashMap<String, Vec<(&icn_identity::Did, i64)>> =
            HashMap::new();

        for delta in &entry.accounts {
            if let Some(debit) = delta.debit {
                debits_by_currency
                    .entry(delta.currency.clone())
                    .or_default()
                    .push((&delta.account_id, debit));
            }
            if let Some(credit) = delta.credit {
                credits_by_currency
                    .entry(delta.currency.clone())
                    .or_default()
                    .push((&delta.account_id, credit));
            }
        }

        // Match debits to credits by currency
        for (currency, debits) in &debits_by_currency {
            if let Some(credits) = credits_by_currency.get(currency) {
                for (from_did, amount) in debits {
                    for (to_did, credit_amount) in credits {
                        if amount == credit_amount {
                            transfers.push(Transfer {
                                from: from_did.to_string(),
                                to: to_did.to_string(),
                                amount: *amount,
                                currency: currency.clone(),
                            });
                            break; // Match found
                        }
                    }
                }
            }
        }

        // Persist updated balances and cleared volume index
        self.save_cached_balances()?;
        self.save_cleared_volume_index()?;

        // Record velocity changes for progressive limits (Issue #336)
        // This MUST be done after the entry is committed to storage
        if let Some(ref prog_manager) = self.progressive_limit_manager {
            for delta in &entry.accounts {
                // Use saturating version since velocity tracking is best-effort
                // and we already validated the entry above
                let net_change = delta.net_change_saturating();
                if let Err(e) = prog_manager.record_change(
                    delta.account_id.as_str(),
                    &delta.currency,
                    net_change,
                ) {
                    // Log but don't fail - velocity tracking is best-effort
                    // The entry is already committed, so we can't roll back
                    warn!(
                        account = %delta.account_id,
                        currency = %delta.currency,
                        error = %e,
                        "Failed to record velocity change"
                    );
                }
            }
        }

        // Emit events if emitter is configured
        if let Some(ref emitter) = self.event_emitter {
            // Emit TransactionCreated event
            emitter.emit_transaction_created(
                hash.clone(),
                &entry.author,
                transfers,
                entry.timestamp,
                self.domain_id.clone(),
            );

            // Emit batch balance change event
            if !balance_changes.is_empty() {
                emitter.emit_batch_balance_changed(&hash, balance_changes, entry.timestamp);
            }
        }

        // Increment and persist journal version for snapshot isolation (M7 fix)
        self.journal_version += 1;
        self.save_journal_version()?;

        info!(
            entry_hash = %hash,
            account_count = entry.accounts.len(),
            timestamp = entry.timestamp,
            journal_version = self.journal_version,
            "Ledger entry appended"
        );

        // Phase 18 Week 5: Index entry for fork detection
        self.fork_detector.index_entry(&entry);

        // Check if this creates a fork (multiple children of same parent)
        for parent in &entry.parents {
            if self.fork_detector.has_fork(parent) {
                icn_obs::metrics::ledger_forks::detected_inc();
                warn!(
                    parent = %parent.to_hex(),
                    new_entry = %hash.to_hex(),
                    "Potential fork detected - multiple entries share parent"
                );

                // Emit fork detected event
                if let Some(ref emitter) = self.event_emitter {
                    // Get all children of this parent for the event
                    let forks = self.fork_detector.detect_forks();
                    if let Some((_, children)) = forks.iter().find(|(p, _)| p == parent) {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        emitter.emit_fork_detected(parent, children.clone(), now);
                    }
                }
            }
        }

        // Publish to gossip if available and broadcast is enabled
        // (broadcast is false when receiving from gossip to avoid re-broadcasting)
        if broadcast {
            if let Some(gossip) = &self.gossip {
                self.publish_to_gossip(gossip, &entry).await?;
            }
        }

        Ok(hash)
    }

    /// Publish a journal entry to gossip for distributed synchronization
    #[instrument(skip(self, gossip, entry), fields(entry_hash = entry.id.as_ref().map(|h| h.to_hex()).unwrap_or_else(|| "none".to_string())))]
    async fn publish_to_gossip(&self, gossip: &GossipHandle, entry: &JournalEntry) -> Result<()> {
        use crate::sync::ledger_topic;

        let hash = entry.id.as_ref().context("Entry must have hash")?.clone();

        // Get all currencies from this entry
        let mut currencies: std::collections::HashSet<String> = std::collections::HashSet::new();
        for delta in &entry.accounts {
            currencies.insert(delta.currency.clone());
        }

        // Publish to each currency's topic
        for currency in currencies {
            let topic = ledger_topic(&currency);
            let msg = LedgerSyncMessage::NewEntry {
                hash: hash.clone(),
                entry: entry.clone(),
            };

            let data = serialize_sync_message(&msg)?;

            // Publish via gossip
            let mut gossip_actor = gossip.write().await;
            gossip_actor.publish(&topic, data).await?;

            debug!(
                entry_hash = %hash,
                topic = %topic,
                currency = %currency,
                message_type = "NewEntry",
                "Published ledger entry to gossip"
            );
        }

        Ok(())
    }

    /// Handle an incoming sync message from gossip
    #[instrument(skip(self, msg), fields(message_type = match &msg {
        LedgerSyncMessage::NewEntry { .. } => "NewEntry",
        LedgerSyncMessage::RequestEntry { .. } => "RequestEntry",
        LedgerSyncMessage::EntryResponse { .. } => "EntryResponse",
        LedgerSyncMessage::RollbackNotification { .. } => "RollbackNotification",
    }))]
    pub async fn handle_sync_message(&mut self, msg: LedgerSyncMessage) -> Result<()> {
        // Issue #181: Track sync lag (time since entry was created)
        let observe_sync_lag = |entry_timestamp: u64| {
            let now_ms = icn_time::current_timestamp_millis();
            let lag_secs = (now_ms.saturating_sub(entry_timestamp) as f64) / 1000.0;
            icn_obs::metrics::ledger::sync_lag_seconds_set(lag_secs);
        };

        match msg {
            LedgerSyncMessage::NewEntry { hash, mut entry } => {
                observe_sync_lag(entry.timestamp);

                // Check if we already have this entry
                if self.get_entry(&hash)?.is_some() {
                    debug!(
                        entry_hash = %hash,
                        "Already have entry, skipping duplicate"
                    );
                    return Ok(());
                }

                // Ensure entry has the correct ID
                entry.id = Some(hash.clone());

                // Use append_entry_from_sync to avoid re-broadcasting entries we received
                let result = self.append_entry_from_sync(entry).await;

                match result {
                    Ok(h) => {
                        info!(
                            entry_hash = %h,
                            source = "gossip",
                            "Received and stored ledger entry"
                        );
                        Ok(())
                    }
                    Err(e) => {
                        warn!(
                            entry_hash = %hash,
                            error = %e,
                            "Failed to store received entry"
                        );
                        Ok(()) // Don't propagate error, just log it
                    }
                }
            }

            LedgerSyncMessage::RequestEntry { hash } => {
                // Handle request for specific entry
                debug!(
                    entry_hash = %hash,
                    message_type = "RequestEntry",
                    "Received entry request"
                );

                if let Some(gossip) = &self.gossip {
                    let entry_opt = self.get_entry(&hash)?;

                    // Determine topic from entry if available
                    if let Some(ref e) = entry_opt {
                        if let Some(delta) = e.accounts.first() {
                            use crate::sync::ledger_topic;
                            let topic = ledger_topic(&delta.currency);

                            let response = LedgerSyncMessage::EntryResponse {
                                hash: hash.clone(),
                                entry: entry_opt,
                            };
                            let data = serialize_sync_message(&response)?;

                            let mut gossip_actor = gossip.write().await;
                            gossip_actor.publish(&topic, data).await?;

                            debug!("Sent entry {} response", hash);
                        }
                    } else {
                        debug!("Entry {} not found locally", hash);
                    }
                }

                Ok(())
            }

            LedgerSyncMessage::EntryResponse { hash, entry } => {
                // Handle response with entry
                if let Some(e) = entry {
                    observe_sync_lag(e.timestamp);
                    debug!("Received entry {} response", hash);
                    // Use append_entry_from_sync to avoid re-broadcasting entries we received
                    let result = self.append_entry_from_sync(e).await;

                    if let Err(e) = result {
                        warn!("Failed to store entry from response: {}", e);
                    }
                } else {
                    debug!("Entry {} not found on remote", hash);
                }

                Ok(())
            }

            LedgerSyncMessage::RollbackNotification {
                target_hash,
                archived_entries,
                reason,
                executed_at,
            } => {
                // Handle rollback notification from network
                // This is triggered when another node (the proposal executor) performs a rollback
                // We should verify and apply the same rollback locally
                warn!(
                    "📢 Received rollback notification: target={}, archived={} entries, reason={}",
                    target_hash,
                    archived_entries.len(),
                    reason
                );

                // Check if we have the target entry
                if self.get_entry(&target_hash)?.is_none() {
                    warn!(
                        "Rollback target {} not found locally - may need full resync",
                        target_hash
                    );
                    return Ok(());
                }

                // Execute the rollback locally (don't broadcast again)
                match self.rollback_to_entry(&target_hash, &reason, false).await {
                    Ok(local_archived) => {
                        // Verify our archived entries match
                        if local_archived.len() != archived_entries.len() {
                            warn!(
                                "Rollback mismatch: archived {} locally vs {} from notification",
                                local_archived.len(),
                                archived_entries.len()
                            );
                        }
                        info!(
                            "✅ Applied rollback from network: archived {} entries (notification at {})",
                            local_archived.len(),
                            executed_at
                        );
                    }
                    Err(e) => {
                        error!("Failed to apply rollback from network: {}", e);
                    }
                }

                Ok(())
            }
        }
    }

    /// Get a journal entry by its hash
    pub fn get_entry(&self, hash: &ContentHash) -> Result<Option<JournalEntry>> {
        let key = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
        let value = self.store.get(key.as_bytes())?;

        match value {
            Some(bytes) => {
                let entry: JournalEntry = serde_json::from_slice(&bytes)?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    /// Get all journal entries
    pub fn get_all_entries(&self) -> Result<Vec<JournalEntry>> {
        let prefix = JOURNAL_PREFIX.as_bytes();
        let pairs = self.store.scan(prefix)?;

        let mut entries = Vec::new();
        for (_key, value) in pairs {
            let entry: JournalEntry = serde_json::from_slice(&value)?;
            entries.push(entry);
        }

        // Sort by timestamp for deterministic ordering
        entries.sort_by_key(|e| e.timestamp);

        Ok(entries)
    }

    /// Count the total number of journal entries
    ///
    /// More efficient than `get_all_entries().len()` as it doesn't
    /// deserialize entries.
    pub fn count_entries(&self) -> Result<usize> {
        let prefix = JOURNAL_PREFIX.as_bytes();
        self.store.scan_count(prefix)
    }

    /// Get journal entries with pagination (newest first)
    ///
    /// Returns entries in reverse chronological order (most recent first),
    /// which is the typical use case for displaying transaction history.
    ///
    /// Uses a timestamp-based secondary index for O(1) count and O(limit)
    /// entry retrieval, making this efficient for large ledgers.
    ///
    /// # Arguments
    /// * `offset` - Number of entries to skip (0-based)
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Returns
    /// Tuple of (entries, total_count)
    pub fn get_entries_paginated(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<JournalEntry>, usize)> {
        // Use timestamp index for efficient pagination (entries sorted by timestamp)
        let ts_prefix = JOURNAL_TS_PREFIX.as_bytes();
        let total = self.store.scan_count(ts_prefix)?;

        // Early return if offset is beyond total
        if offset >= total {
            return Ok((Vec::new(), total));
        }

        // For descending order (newest first), we need to read from the end
        // Calculate the starting position from the ascending index
        let items_from_end = offset + limit;
        let skip_from_start = total.saturating_sub(items_from_end);
        let take_count = limit.min(total.saturating_sub(offset));

        // Scan the timestamp index with calculated offset
        let (ts_pairs, _) = self
            .store
            .scan_paginated(ts_prefix, skip_from_start, take_count)?;

        // Look up entries by hash (values in timestamp index are hashes)
        let mut entries = Vec::with_capacity(ts_pairs.len());
        for (_key, hash_bytes) in ts_pairs {
            let hash_hex = hex::encode(&hash_bytes);
            let entry_key = format!("{}{}", JOURNAL_PREFIX, &hash_hex);
            if let Some(entry_data) = self.store.get(entry_key.as_bytes())? {
                let mut entry: JournalEntry = serde_json::from_slice(&entry_data)?;
                // Restore the id from the hash (not serialized due to #[serde(skip)])
                if let Ok(hash_arr) = <[u8; 32]>::try_from(hash_bytes.as_slice()) {
                    entry.id = Some(ContentHash::from_bytes(hash_arr));
                }
                entries.push(entry);
            }
        }

        // Reverse to get descending order (newest first)
        entries.reverse();

        Ok((entries, total))
    }

    /// Get journal entries with pagination (oldest first)
    ///
    /// Returns entries in chronological order (oldest first).
    /// Useful for auditing and sequential processing.
    ///
    /// Uses a timestamp-based secondary index for O(1) count and O(limit)
    /// entry retrieval, making this efficient for large ledgers.
    ///
    /// # Arguments
    /// * `offset` - Number of entries to skip (0-based)
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Returns
    /// Tuple of (entries, total_count)
    pub fn get_entries_paginated_asc(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<JournalEntry>, usize)> {
        // Use timestamp index for efficient pagination (already sorted by timestamp ASC)
        let ts_prefix = JOURNAL_TS_PREFIX.as_bytes();

        // Scan with pagination - this efficiently skips and takes from the sorted index
        let (ts_pairs, total) = self.store.scan_paginated(ts_prefix, offset, limit)?;

        // Early return if offset is beyond total
        if offset >= total {
            return Ok((Vec::new(), total));
        }

        // Look up entries by hash (values in timestamp index are hashes)
        let mut entries = Vec::with_capacity(ts_pairs.len());
        for (_key, hash_bytes) in ts_pairs {
            let hash_hex = hex::encode(&hash_bytes);
            let entry_key = format!("{}{}", JOURNAL_PREFIX, &hash_hex);
            if let Some(entry_data) = self.store.get(entry_key.as_bytes())? {
                let mut entry: JournalEntry = serde_json::from_slice(&entry_data)?;
                // Restore the id from the hash (not serialized due to #[serde(skip)])
                if let Ok(hash_arr) = <[u8; 32]>::try_from(hash_bytes.as_slice()) {
                    entry.id = Some(ContentHash::from_bytes(hash_arr));
                }
                entries.push(entry);
            }
        }

        Ok((entries, total))
    }

    /// Get journal entries with filtered pagination (oldest first)
    ///
    /// This is a memory-efficient version of filtered queries. Instead of loading
    /// all entries and then filtering, it streams through entries one by one,
    /// applying the filter during iteration and stopping once `limit` matches found.
    ///
    /// Uses cursor-based pagination: pass the timestamp from the last entry of
    /// the previous page to continue from that point.
    ///
    /// # Arguments
    /// * `filter_did` - Optional DID to filter by (entry must involve this DID)
    /// * `cursor_ts` - Optional timestamp to start after (exclusive)
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Returns
    /// Tuple of (entries, next_cursor_timestamp)
    /// The next_cursor is Some if there may be more entries to fetch.
    ///
    /// # Performance Notes
    /// - Memory bounded to O(limit) instead of O(total_entries)
    /// - Uses N+1 query pattern: one index scan + one lookup per entry
    /// - Acceptable for local Sled storage; would need batch fetching for network storage
    /// - Filtered queries scan all index entries but only deserialize matching entries
    ///
    /// # Cursor Format
    /// The cursor is a tuple of (timestamp, optional_hash) for proper tie-breaking.
    /// When multiple entries share the same timestamp, the hash provides deterministic ordering.
    pub fn get_entries_filtered_paginated(
        &self,
        filter_did: Option<&Did>,
        cursor: Option<(u64, Option<String>)>,
        limit: usize,
    ) -> Result<(Vec<JournalEntry>, Option<PaginationCursor>)> {
        // Fast path: No filter AND no cursor - use efficient offset-based pagination
        if filter_did.is_none() && cursor.is_none() {
            let (entries, _total) = self.get_entries_paginated_asc(0, limit + 1)?;
            let has_more = entries.len() > limit;
            let entries: Vec<_> = entries.into_iter().take(limit).collect();
            let next_cursor = if has_more {
                entries.last().map(|e| {
                    let hash = e.id.as_ref().map(|h| h.to_hex()).unwrap_or_default();
                    (e.timestamp, hash)
                })
            } else {
                None
            };
            return Ok((entries, next_cursor));
        }

        // Extract cursor components
        let cursor_ts = cursor.as_ref().map(|(ts, _)| *ts);
        let cursor_hash = cursor.as_ref().and_then(|(_, h)| h.clone());

        // Streaming path: Either has filter OR has cursor (or both)
        // Stream through timestamp index, applying filter and cursor, stopping early
        let ts_prefix = JOURNAL_TS_PREFIX.as_bytes();
        let ts_pairs = self.store.scan(ts_prefix)?;

        let mut entries = Vec::with_capacity(limit);

        for (key, hash_bytes) in ts_pairs {
            // Extract timestamp and hash from key
            // Key format: "ledger:journal_ts:{timestamp:020}:{hash}"
            // Timestamp is stored as zero-padded decimal (not hex)
            let key_str = String::from_utf8_lossy(&key);
            let entry_hash = hex::encode(&hash_bytes);

            // Parse timestamp from key - log errors instead of silently skipping
            let entry_ts = match key_str.strip_prefix(JOURNAL_TS_PREFIX) {
                Some(ts_str) => match ts_str.split(':').next() {
                    Some(ts_decimal) => match ts_decimal.parse::<u64>() {
                        Ok(ts) => ts,
                        Err(e) => {
                            warn!(
                                key = %key_str,
                                error = %e,
                                "Malformed timestamp in journal index key - skipping entry"
                            );
                            continue;
                        }
                    },
                    None => {
                        warn!(
                            key = %key_str,
                            "Missing timestamp component in journal index key - skipping entry"
                        );
                        continue;
                    }
                },
                None => {
                    warn!(
                        key = %key_str,
                        expected_prefix = JOURNAL_TS_PREFIX,
                        "Unexpected key format in journal timestamp index - skipping entry"
                    );
                    continue;
                }
            };

            // Skip entries at or before the cursor (using hash for tie-breaking)
            // Entries are sorted by (timestamp ASC, hash ASC) in the index.
            //
            // The cursor points to the LAST entry returned in the previous page.
            // We use <= (not <) for the hash comparison because:
            // - If entry_hash == cursor_hash: this is the same entry, skip to avoid duplicates
            // - If entry_hash < cursor_hash: this entry was already returned, skip it
            // - If entry_hash > cursor_hash: this is a new entry, include it
            if let Some(cursor) = cursor_ts {
                if entry_ts < cursor {
                    continue;
                }
                if entry_ts == cursor {
                    // Same timestamp - use hash for tie-breaking
                    if let Some(ref cursor_h) = cursor_hash {
                        if entry_hash <= *cursor_h {
                            continue;
                        }
                    }
                }
            }

            // Look up the full entry
            let entry_key = format!("{}{}", JOURNAL_PREFIX, &entry_hash);
            if let Some(entry_data) = self.store.get(entry_key.as_bytes())? {
                let mut entry: JournalEntry = serde_json::from_slice(&entry_data)?;
                // Restore the id from the hash (not serialized due to #[serde(skip)])
                if let Ok(hash_arr) = <[u8; 32]>::try_from(hash_bytes.as_slice()) {
                    entry.id = Some(ContentHash::from_bytes(hash_arr));
                }

                // Apply DID filter if present
                let matches_filter = match filter_did {
                    Some(did) => entry.accounts.iter().any(|delta| &delta.account_id == did),
                    None => true, // No filter means all entries match
                };

                if matches_filter {
                    entries.push(entry);

                    // Stop once we have enough + 1 (to check if there are more)
                    if entries.len() > limit {
                        break;
                    }
                }
            }
        }

        // Check if there are more entries
        let has_more = entries.len() > limit;
        if has_more {
            entries.pop(); // Remove the extra entry
        }

        // Next cursor includes both timestamp and hash for proper tie-breaking
        let next_cursor = if has_more {
            entries.last().map(|e| {
                let hash = e.id.as_ref().map(|h| h.to_hex()).unwrap_or_default();
                (e.timestamp, hash)
            })
        } else {
            None
        };

        Ok((entries, next_cursor))
    }

    // === Phase 18 Week 5: Fork Detection and Resolution ===

    /// Rebuild the fork detection index from stored entries
    fn rebuild_fork_index(&mut self) -> Result<()> {
        let entries = self.get_all_entries()?;
        let entry_count = entries.len();

        for entry in entries {
            self.fork_detector.index_entry(&entry);
        }

        info!(entry_count = entry_count, "Rebuilt fork detection index");

        Ok(())
    }

    /// Ensure timestamp index exists for efficient pagination
    ///
    /// This migrates existing ledgers that were created before the timestamp
    /// index was introduced. Only runs if the index is empty but entries exist.
    fn ensure_timestamp_index(&self) -> Result<()> {
        let ts_prefix = JOURNAL_TS_PREFIX.as_bytes();
        let ts_count = self.store.scan_count(ts_prefix)?;

        // If timestamp index already has entries, no migration needed
        if ts_count > 0 {
            return Ok(());
        }

        // Check if we have journal entries that need indexing
        let journal_prefix = JOURNAL_PREFIX.as_bytes();
        let pairs = self.store.scan(journal_prefix)?;

        if pairs.is_empty() {
            return Ok(());
        }

        info!(
            entry_count = pairs.len(),
            "Migrating ledger: building timestamp index"
        );

        for (key, value) in pairs {
            // Extract hash from key (after prefix)
            let key_str = String::from_utf8_lossy(&key);
            let hash_hex = key_str.trim_start_matches(JOURNAL_PREFIX);

            // Deserialize entry to get timestamp
            let entry: JournalEntry = serde_json::from_slice(&value)?;

            // Write to timestamp index
            let ts_key = format!("{}{:020}:{}", JOURNAL_TS_PREFIX, entry.timestamp, hash_hex);
            if let Ok(hash_bytes) = hex::decode(hash_hex) {
                self.store.put(ts_key.as_bytes(), &hash_bytes)?;
            }
        }

        info!("Timestamp index migration complete");
        Ok(())
    }

    /// Detect any forks in the ledger
    pub fn detect_forks(&self) -> Vec<(ContentHash, Vec<ContentHash>)> {
        self.fork_detector.detect_forks()
    }

    /// Check if a specific parent has a fork
    pub fn has_fork(&self, parent: &ContentHash) -> bool {
        self.fork_detector.has_fork(parent)
    }

    /// Detect and resolve all forks in the ledger
    ///
    /// Returns a list of resolved forks with their resolutions.
    /// Entries that should be discarded are quarantined.
    #[instrument(skip(self))]
    pub fn detect_and_resolve_forks(&mut self) -> Result<Vec<(Fork, ForkResolution)>> {
        let forks = self.fork_detector.detect_forks();

        if forks.is_empty() {
            debug!("No forks detected in ledger");
            return Ok(vec![]);
        }

        info!(
            fork_count = forks.len(),
            "Detected forks in ledger, attempting resolution"
        );

        let mut resolutions = Vec::new();

        for (parent, children) in forks {
            // Get the actual entries for comparison
            let mut entries = Vec::new();
            for child_hash in &children {
                if let Some(entry) = self.get_entry(child_hash)? {
                    entries.push(entry);
                }
            }

            // Handle N-way forks using tournament-style resolution
            // Compare entries pairwise: winner of round 1 vs entry 3, winner vs entry 4, etc.
            if entries.len() >= 2 {
                let is_nway = entries.len() > 2;
                let entry_count = entries.len();

                // Track winning entry index and all losers
                let mut winner_idx = 0;
                let mut losers: Vec<usize> = Vec::new();
                let mut requires_manual = false;
                let mut manual_reason = String::new();

                // Tournament: compare current winner against each subsequent entry
                for challenger_idx in 1..entries.len() {
                    let fork = Fork {
                        common_parents: vec![parent.clone()],
                        entry1: entries[winner_idx].clone(),
                        entry2: entries[challenger_idx].clone(),
                        detected_at: SystemTime::now(),
                    };

                    match self.fork_resolver.resolve_fork(&fork) {
                        Ok(resolution) => {
                            match &resolution {
                                ForkResolution::KeepFirst => {
                                    // Current winner stays, challenger loses
                                    losers.push(challenger_idx);
                                    debug!(
                                        round = challenger_idx,
                                        winner = winner_idx,
                                        "Tournament round: keeping current winner"
                                    );
                                }
                                ForkResolution::KeepSecond => {
                                    // Challenger wins, previous winner loses
                                    losers.push(winner_idx);
                                    winner_idx = challenger_idx;
                                    debug!(
                                        round = challenger_idx,
                                        new_winner = winner_idx,
                                        "Tournament round: challenger wins"
                                    );
                                }
                                ForkResolution::RequiresManual { reason } => {
                                    requires_manual = true;
                                    manual_reason = reason.clone();
                                    warn!(
                                        parent = %parent.to_hex(),
                                        round = challenger_idx,
                                        reason = reason,
                                        "Fork requires manual resolution, stopping tournament"
                                    );
                                    break;
                                }
                            }

                            // Store the last resolution for reporting
                            if challenger_idx == entries.len() - 1 && !requires_manual {
                                resolutions.push((fork, resolution));
                            }
                        }
                        Err(e) => {
                            warn!(
                                parent = %parent.to_hex(),
                                round = challenger_idx,
                                error = %e,
                                "Failed to resolve fork round"
                            );
                        }
                    }
                }

                // Handle manual resolution requirement
                if requires_manual {
                    icn_obs::metrics::ledger_forks::manual_resolution_required_inc(&manual_reason);
                    continue;
                }

                // Quarantine all losing entries
                for loser_idx in &losers {
                    let loser_entry = &entries[*loser_idx];
                    if let Some(hash) = &loser_entry.id {
                        self.quarantine_forked_entry(
                            loser_entry,
                            if is_nway {
                                "Lost N-way fork resolution"
                            } else {
                                "Lost fork resolution"
                            },
                        )?;
                        debug!(
                            quarantined = %hash.to_hex(),
                            entry_index = loser_idx,
                            "Quarantined losing fork entry"
                        );
                    }
                }

                // Record metrics
                icn_obs::metrics::ledger_forks::resolved_inc("hybrid");
                if is_nway {
                    icn_obs::metrics::ledger_forks::nway_fork_resolved_inc(entry_count);
                    info!(
                        parent = %parent.to_hex(),
                        entry_count = entry_count,
                        losers_quarantined = losers.len(),
                        winner_idx = winner_idx,
                        "Resolved N-way fork via tournament"
                    );
                } else {
                    info!(
                        parent = %parent.to_hex(),
                        "Resolved 2-way fork"
                    );
                }
            }
        }

        Ok(resolutions)
    }

    /// Quarantine an entry that lost fork resolution
    fn quarantine_forked_entry(&mut self, entry: &JournalEntry, reason: &str) -> Result<()> {
        let hash = entry.id.as_ref().context("Entry missing hash")?;

        // Remove from main store
        let key = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
        self.store.delete(key.as_bytes())?;

        // Remove from timestamp index
        let ts_key = format!(
            "{}{:020}:{}",
            JOURNAL_TS_PREFIX,
            entry.timestamp,
            hash.to_hex()
        );
        self.store.delete(ts_key.as_bytes())?;

        // Add to quarantine
        let item = QuarantineItem::new(
            hash.clone(),
            QuarantineReason::ForkConflict(reason.to_string()),
            entry.author.clone(),
        );
        self.quarantine.add(entry.clone(), item)?;

        // Record Byzantine violation for conflicting ledger entries (Phase 18)
        if let Some(ref detector) = self.misbehavior_detector {
            // Find the conflicting entry hash from the first parent
            let conflicting_hash = entry
                .parents
                .first()
                .cloned()
                .unwrap_or_else(|| hash.clone());

            let violation = icn_security::Violation::ConflictingLedgerEntries {
                entry1: hash.as_bytes().try_into().unwrap_or([0u8; 32]),
                entry2: conflicting_hash.as_bytes().try_into().unwrap_or([0u8; 32]),
            };

            // Report violation using block_in_place (may be called from tokio runtime)
            let detector_clone = detector.clone();
            let author = entry.author.clone();
            tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    detector_clone
                        .write()
                        .await
                        .record_violation(&author, violation, vec![]);
                })
            });
        }

        // Recompute balances (expensive but necessary for correctness)
        self.recompute_balances()?;

        Ok(())
    }

    /// Get fork resolution statistics
    pub fn get_fork_stats(&self) -> ForkStats {
        let forks = self.fork_detector.detect_forks();
        ForkStats {
            total_forks: forks.len(),
            parents_with_forks: forks.iter().map(|(p, _)| p.clone()).collect(),
        }
    }

    /// Get account balance for a specific currency
    pub fn get_balance(&self, account_id: &Did, currency: &str) -> i64 {
        self.cached_balances
            .get(account_id)
            .map(|b| b.get(currency))
            .unwrap_or(0)
    }

    /// Get all balances for an account
    pub fn get_account_balances(&self, account_id: &Did) -> AccountBalances {
        self.cached_balances
            .get(account_id)
            .cloned()
            .unwrap_or_else(|| AccountBalances::new(account_id.clone()))
    }

    /// Get all balances across all accounts
    pub fn get_all_balances(&self) -> HashMap<Did, AccountBalances> {
        self.cached_balances.clone()
    }

    /// Get total cleared volume for an account in a currency (O(1) lookup)
    ///
    /// This returns the total credits received by the account in the specified currency,
    /// which represents their total historical contributions to the system.
    /// Used for calculating credit limit bonuses based on transaction history.
    ///
    /// Example: If Alice has received 500 hours of credits over time,
    /// this returns 500 (even if her current balance is lower due to debits).
    ///
    /// Performance: O(1) - uses the pre-computed cleared volume index
    pub fn total_cleared_by(&self, account_id: &Did, currency: &str) -> Result<i64> {
        let key = (account_id.clone(), currency.to_string());
        Ok(*self.cleared_volume_index.get(&key).unwrap_or(&0))
    }

    /// Recompute all balances and cleared volumes from journal entries (for verification)
    ///
    /// This method uses snapshot isolation to prevent race conditions (M7 fix):
    /// 1. Capture the journal version at snapshot time
    /// 2. Compute new balances from the snapshot
    /// 3. Validate the version hasn't changed before applying
    /// 4. If version changed, return error (caller should retry)
    pub fn recompute_balances(&mut self) -> Result<()> {
        // M7 Fix: Capture journal version at snapshot time for isolation
        let snapshot_version = self.journal_version;

        info!(
            cached_account_count = self.cached_balances.len(),
            cleared_volume_count = self.cleared_volume_index.len(),
            snapshot_version,
            "Recomputing all balances and cleared volumes from journal"
        );

        // Take snapshot of entries
        let entries = self.get_all_entries()?;
        let balances = compute_all_balances(&entries)?;

        // Also recompute cleared volume index
        let mut cleared_volumes: HashMap<(Did, String), i64> = HashMap::new();
        for entry in &entries {
            for delta in &entry.accounts {
                if let Some(credit) = delta.credit {
                    let key = (delta.account_id.clone(), delta.currency.clone());
                    *cleared_volumes.entry(key).or_insert(0) += credit;
                }
            }
        }

        // M7 Fix: Validate journal version hasn't changed during recomputation
        // This prevents the race condition where concurrent entry appends are lost
        if self.journal_version != snapshot_version {
            warn!(
                snapshot_version,
                current_version = self.journal_version,
                "Journal modified during balance recomputation - aborting to prevent data loss"
            );
            icn_obs::metrics::ledger::recompute_aborted_version_mismatch_inc();
            anyhow::bail!(
                "Journal modified during balance recomputation (version {} -> {}). \
                 Retry the operation to ensure data consistency.",
                snapshot_version,
                self.journal_version
            );
        }

        // Safe to apply - journal hasn't changed during our computation
        self.cached_balances = balances;
        self.cleared_volume_index = cleared_volumes;
        self.save_cached_balances()?;
        self.save_cleared_volume_index()?;

        info!(
            entry_count = entries.len(),
            account_count = self.cached_balances.len(),
            cleared_volume_count = self.cleared_volume_index.len(),
            snapshot_version,
            "Balance and cleared volume recomputation complete"
        );
        Ok(())
    }

    /// Recompute balances with automatic retry on version mismatch
    ///
    /// This is a convenience wrapper around `recompute_balances` that handles
    /// the race condition by retrying up to `max_retries` times.
    pub fn recompute_balances_with_retry(&mut self, max_retries: usize) -> Result<()> {
        for attempt in 0..=max_retries {
            match self.recompute_balances() {
                Ok(()) => return Ok(()),
                Err(e) if attempt < max_retries && e.to_string().contains("Journal modified") => {
                    warn!(
                        attempt = attempt + 1,
                        max_retries, "Balance recomputation retry due to concurrent modification"
                    );
                    // Small delay to reduce contention
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => return Err(e),
            }
        }
        anyhow::bail!(
            "Balance recomputation failed after {max_retries} retries due to concurrent modifications"
        );
    }

    /// Verify ledger integrity
    pub fn verify_integrity(&self) -> Result<()> {
        info!("Verifying ledger integrity");

        // Get all entries
        let entries = self.get_all_entries()?;

        // Recompute balances from scratch
        let computed_balances = compute_all_balances(&entries)?;

        // Compare with cached balances
        if computed_balances != self.cached_balances {
            warn!(
                cached_accounts = self.cached_balances.len(),
                computed_accounts = computed_balances.len(),
                "Balance mismatch detected!"
            );
            return Err(anyhow::anyhow!(
                "Cached balances do not match computed balances"
            ));
        }

        info!(
            entry_count = entries.len(),
            account_count = self.cached_balances.len(),
            "Ledger integrity verified"
        );
        Ok(())
    }

    /// Load cached balances from storage
    fn load_cached_balances(&mut self) -> Result<()> {
        let prefix = BALANCE_PREFIX.as_bytes();
        let pairs = self.store.scan(prefix)?;

        for (_key, value) in pairs {
            let balances: AccountBalances = serde_json::from_slice(&value)?;
            self.cached_balances
                .insert(balances.account_id.clone(), balances);
        }

        debug!("Loaded {} cached balances", self.cached_balances.len());
        Ok(())
    }

    /// Save cached balances to storage
    fn save_cached_balances(&self) -> Result<()> {
        for (account_id, balances) in &self.cached_balances {
            let key = format!("{}{}", BALANCE_PREFIX, serde_json::to_string(account_id)?);
            let value = serde_json::to_vec(balances)?;
            self.store.put(key.as_bytes(), &value)?;
        }

        Ok(())
    }

    /// Load cleared volume index from storage
    fn load_cleared_volume_index(&mut self) -> Result<()> {
        let prefix = CLEARED_VOLUME_PREFIX.as_bytes();
        let pairs = self.store.scan(prefix)?;

        for (key, value) in pairs {
            // Key format: "ledger:cleared_volume:{did}:{currency}"
            let key_str = String::from_utf8_lossy(&key);
            if let Some(rest) = key_str.strip_prefix(CLEARED_VOLUME_PREFIX) {
                // Parse the composite key - format is "did:currency"
                // Use rfind to find the last colon, since DIDs can contain colons
                if let Some(last_colon) = rest.rfind(':') {
                    let did_str = &rest[..last_colon];
                    let currency = &rest[last_colon + 1..];

                    if let Ok(did) = serde_json::from_str::<Did>(&format!("\"{did_str}\"")) {
                        if let Ok(volume) = serde_json::from_slice::<i64>(&value) {
                            self.cleared_volume_index
                                .insert((did, currency.to_string()), volume);
                        }
                    }
                }
            }
        }

        debug!(
            "Loaded {} cleared volume entries",
            self.cleared_volume_index.len()
        );
        Ok(())
    }

    /// Save cleared volume index to storage
    fn save_cleared_volume_index(&self) -> Result<()> {
        for ((account_id, currency), volume) in &self.cleared_volume_index {
            // Store with composite key: "{prefix}{did}:{currency}"
            let key = format!("{CLEARED_VOLUME_PREFIX}{account_id}:{currency}");
            let value = serde_json::to_vec(volume)?;
            self.store.put(key.as_bytes(), &value)?;
        }

        Ok(())
    }

    /// Get the last merge decision (for reporting)
    pub fn last_merge_decision(&self) -> Option<&MergeDecision> {
        self.last_merge.as_ref()
    }

    /// Get a reference to the quarantine store
    pub fn quarantine(&self) -> &QuarantineStore {
        &self.quarantine
    }

    /// Get a mutable reference to the quarantine store
    pub fn quarantine_mut(&mut self) -> &mut QuarantineStore {
        &mut self.quarantine
    }

    // === Member Freeze Methods (Issue #25) ===

    /// Freeze a member account - blocks all ledger transactions involving them
    ///
    /// This is an emergency action that should only be invoked after a governance
    /// proposal passes with super-majority approval.
    ///
    /// # Arguments
    /// * `did` - The member DID to freeze
    /// * `reason` - Reason for freezing (for audit trail)
    /// * `duration_seconds` - Optional duration (None = indefinite)
    pub fn freeze_member(&mut self, did: Did, reason: String, duration_seconds: Option<u64>) {
        info!(
            did = %did,
            reason = %reason,
            duration = ?duration_seconds,
            "Freezing member account"
        );
        self.freeze_manager
            .freeze(did.clone(), reason.clone(), duration_seconds);

        // Emit freeze event
        if let Some(ref emitter) = self.event_emitter {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            emitter.emit_member_frozen(&did, reason, None, None, duration_seconds, now);
        }
    }

    /// Freeze a member with full metadata (for governance integration)
    pub fn freeze_member_with_metadata(
        &mut self,
        did: Did,
        reason: String,
        duration_seconds: Option<u64>,
        proposal_id: Option<String>,
        frozen_by: Option<Did>,
    ) {
        info!(
            did = %did,
            reason = %reason,
            proposal = ?proposal_id,
            "Freezing member account via governance"
        );
        self.freeze_manager.freeze_with_metadata(
            did.clone(),
            reason.clone(),
            duration_seconds,
            proposal_id.clone(),
            frozen_by.clone(),
        );

        // Emit freeze event
        if let Some(ref emitter) = self.event_emitter {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            emitter.emit_member_frozen(
                &did,
                reason,
                frozen_by.as_ref(),
                proposal_id,
                duration_seconds,
                now,
            );
        }
    }

    /// Unfreeze a member account
    ///
    /// This is also an emergency action requiring super-majority approval,
    /// unless the freeze has expired.
    pub fn unfreeze_member(&mut self, did: &Did, reason: String) -> Option<FrozenMember> {
        info!(
            did = %did,
            reason = %reason,
            "Unfreezing member account"
        );
        let result = self.freeze_manager.unfreeze(did, reason.clone());

        // Emit unfreeze event if member was unfrozen
        if result.is_some() {
            if let Some(ref emitter) = self.event_emitter {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                emitter.emit_member_unfrozen(did, reason, None, None, now);
            }
        }

        result
    }

    /// Unfreeze a member with full metadata (for governance integration)
    pub fn unfreeze_member_with_metadata(
        &mut self,
        did: &Did,
        reason: String,
        proposal_id: Option<String>,
        unfrozen_by: Option<Did>,
    ) -> Option<FrozenMember> {
        info!(
            did = %did,
            reason = %reason,
            proposal = ?proposal_id,
            "Unfreezing member account via governance"
        );
        let result = self.freeze_manager.unfreeze_with_metadata(
            did,
            reason.clone(),
            proposal_id.clone(),
            unfrozen_by.clone(),
        );

        // Emit unfreeze event if member was unfrozen
        if result.is_some() {
            if let Some(ref emitter) = self.event_emitter {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                emitter.emit_member_unfrozen(did, reason, unfrozen_by.as_ref(), proposal_id, now);
            }
        }

        result
    }

    /// Check if a member is currently frozen
    pub fn is_member_frozen(&mut self, did: &Did) -> bool {
        self.freeze_manager.is_frozen(did)
    }

    /// Get the freeze record for a member if frozen
    pub fn get_freeze_record(&mut self, did: &Did) -> Option<&FrozenMember> {
        self.freeze_manager.get_frozen(did)
    }

    /// List all currently frozen members
    pub fn list_frozen_members(&mut self) -> Vec<&FrozenMember> {
        self.freeze_manager.list_frozen()
    }

    /// Get count of frozen members
    pub fn frozen_member_count(&mut self) -> usize {
        self.freeze_manager.frozen_count()
    }

    /// Clean up expired freezes
    pub fn cleanup_expired_freezes(&mut self) -> usize {
        self.freeze_manager.cleanup_expired()
    }

    /// Transfer balances from old_did to new_did during recovery
    ///
    /// Creates journal entries transferring all balances from the old DID
    /// to the new DID, maintaining the audit trail. This is called when
    /// a social recovery is finalized.
    ///
    /// Returns the number of currencies transferred.
    pub async fn transfer_balances_for_recovery(
        &mut self,
        old_did: &Did,
        new_did: &Did,
        recovery_id: &str,
    ) -> Result<usize> {
        info!(
            "Transferring ledger balances from {} to {} (recovery: {})",
            old_did, new_did, recovery_id
        );

        let balances = self.get_account_balances(old_did);
        let mut transferred_count = 0;

        // For each currency with a non-zero balance
        for (currency, balance) in balances.balances.iter() {
            if *balance == 0 {
                continue; // Skip zero balances
            }

            info!(
                "Transferring {} {} from {} to {}",
                balance, currency, old_did, new_did
            );

            // Create journal entry for this transfer
            // If balance is positive (credit), debit old_did and credit new_did
            // If balance is negative (debit), credit old_did and debit new_did
            use crate::entry::JournalEntryBuilder;

            let entry = if *balance > 0 {
                // old_did has positive balance (+100 means they have credit)
                // Transfer it to new_did: reduce old_did's balance, increase new_did's balance
                JournalEntryBuilder::new(new_did.clone())
                    .credit(old_did.clone(), currency.clone(), *balance) // Reduce old_did's balance
                    .debit(new_did.clone(), currency.clone(), *balance) // Increase new_did's balance
                    .build()?
            } else {
                // old_did has negative balance (-100 means they owe credit)
                // Transfer the debt to new_did: reduce old_did's debt, increase new_did's debt
                let debt_amount = balance.abs();
                JournalEntryBuilder::new(new_did.clone())
                    .debit(old_did.clone(), currency.clone(), debt_amount) // Reduce old_did's debt
                    .credit(new_did.clone(), currency.clone(), debt_amount) // Increase new_did's debt
                    .build()?
            };

            // Append the entry
            self.append_entry(entry).await?;
            transferred_count += 1;
        }

        info!(
            "Transferred {} currencies from {} to {}",
            transferred_count, old_did, new_did
        );

        Ok(transferred_count)
    }

    /// Merge a batch of entries and report the decision
    ///
    /// This processes incoming entries, validates them, and decides:
    /// - Accept: Add to canonical chain
    /// - Discard: Redundant or superseded
    /// - Quarantine: Violates invariants or limits
    ///
    /// Returns a MergeDecision capturing all outcomes for observability.
    pub async fn merge_batch(&mut self, entries: Vec<JournalEntry>) -> Result<MergeDecision> {
        // Get current tip (last entry hash)
        let all_entries = self.get_all_entries()?;
        let tip_hash = all_entries
            .last()
            .and_then(|e| e.id.clone())
            .unwrap_or_else(|| ContentHash::from_bytes([0u8; 32]));

        let mut decision = MergeDecision::new(tip_hash.clone());

        for entry in entries {
            let entry_id = match entry.id.as_ref() {
                Some(id) => id.clone(),
                None => {
                    warn!("Entry has no ID, skipping");
                    continue;
                }
            };

            // Check if we already have this entry
            if self.get_entry(&entry_id)?.is_some() {
                debug!("Already have entry {}, discarding", entry_id);
                decision.add_discarded(entry_id);
                continue;
            }

            // Validate entry (simplified - real implementation would check signatures, balances, etc.)
            if let Err(e) = self.validate_entry(&entry) {
                warn!("Entry {} failed validation: {}", entry_id, e);

                // Create quarantine items (must clone author before moving entry)
                let author = entry.author.clone();
                let item = QuarantineItem::new(
                    entry_id.clone(),
                    QuarantineReason::InvariantViolation(e.to_string()),
                    author.clone(),
                );

                // Add to quarantine store
                self.quarantine.add(entry, item.clone())?;

                // Add to decision
                decision.add_quarantined(item);
                continue;
            }

            // Accept the entry
            match self.append_entry(entry).await {
                Ok(hash) => {
                    debug!("Accepted entry {}", hash);
                    decision.increment_accepted();
                }
                Err(e) => {
                    warn!("Failed to append entry {}: {}", entry_id, e);
                    decision.add_discarded(entry_id);
                }
            }
        }

        // Update the tip after merging
        let all_entries = self.get_all_entries()?;
        if let Some(last_entry) = all_entries.last() {
            if let Some(last_id) = &last_entry.id {
                decision.canonical_chain_tip = last_id.clone();
            }
        }

        // Emit metrics
        use icn_obs::metrics::ledger;
        for _ in &decision.conflicts {
            ledger::merge_conflicts_inc();
        }
        for _ in &decision.quarantined {
            ledger::entries_quarantined_inc();
        }
        for _ in &decision.discarded {
            ledger::entries_discarded_inc();
        }

        // Update quarantine size metric
        let quarantine_count = self.quarantine.count()?;
        ledger::quarantine_size_set(quarantine_count as u64);

        // Store decision for reporting
        self.last_merge = Some(decision.clone());

        info!(
            "Merge complete: {} accepted, {} discarded, {} quarantined, {} conflicts",
            decision.accepted_count,
            decision.discarded.len(),
            decision.quarantined.len(),
            decision.conflicts.len()
        );

        Ok(decision)
    }

    // === Emergency Recovery: Ledger Rollback (C1) ===

    /// Roll back the ledger to a specific entry
    ///
    /// This is an emergency recovery operation that:
    /// 1. Verifies the target hash exists in the ledger
    /// 2. Identifies all entries that come after the target (by timestamp)
    /// 3. Archives those entries to a separate storage namespace
    /// 4. Removes them from the active ledger
    /// 5. Recomputes all balances from remaining entries
    /// 6. Optionally broadcasts a rollback notification via gossip
    ///
    /// # Arguments
    /// * `target_hash` - Hash of the entry to roll back to (this entry is kept)
    /// * `reason` - Reason for the rollback (from governance proposal)
    /// * `broadcast` - Whether to broadcast rollback notification via gossip
    ///
    /// # Returns
    /// Vector of archived entry hashes
    ///
    /// # Safety
    /// This is a destructive operation. Entries are moved to archive storage
    /// but not deleted, allowing potential recovery if needed.
    #[instrument(skip(self), fields(target_hash = %target_hash))]
    pub async fn rollback_to_entry(
        &mut self,
        target_hash: &ContentHash,
        reason: &str,
        broadcast: bool,
    ) -> Result<Vec<ContentHash>> {
        info!(
            "🚨 ROLLBACK: Beginning rollback to entry {} (reason: {})",
            target_hash, reason
        );

        // Step 1: Verify target entry exists
        let target_entry = self
            .get_entry(target_hash)?
            .ok_or_else(|| anyhow::anyhow!("Target entry {target_hash} not found"))?;

        let target_timestamp = target_entry.timestamp;
        info!(
            "Found target entry with timestamp {}, author: {}",
            target_timestamp, target_entry.author
        );

        // Step 2: Get all entries and identify those to archive
        let all_entries = self.get_all_entries()?;
        let entries_to_archive: Vec<JournalEntry> = all_entries
            .into_iter()
            .filter(|e| e.timestamp > target_timestamp)
            .collect();

        let archived_count = entries_to_archive.len();
        info!(
            "Identified {} entries to archive (after timestamp {})",
            archived_count, target_timestamp
        );

        if entries_to_archive.is_empty() {
            info!("No entries to archive - target is already at tip");
            return Ok(vec![]);
        }

        // Step 3: Archive entries to separate namespace
        // Track (hash, timestamp) pairs for deletion from both main store and timestamp index
        let mut archived_entries: Vec<(ContentHash, u64)> = Vec::with_capacity(archived_count);
        let archive_timestamp = icn_time::current_timestamp_secs();

        for entry in &entries_to_archive {
            if let Some(ref hash) = entry.id {
                // Store in archive namespace with metadata
                let archive_key =
                    format!("{}{}:{}", ARCHIVE_PREFIX, archive_timestamp, hash.to_hex());
                let archive_data = serde_json::to_vec(&ArchiveRecord {
                    entry: entry.clone(),
                    archived_at: archive_timestamp,
                    reason: reason.to_string(),
                })?;

                self.store.put(archive_key.as_bytes(), &archive_data)?;
                archived_entries.push((hash.clone(), entry.timestamp));

                debug!("Archived entry {} to {}", hash, archive_key);
            }
        }

        // Step 4: Remove entries from active ledger and timestamp index
        for (hash, entry_timestamp) in &archived_entries {
            // Remove from main store
            let journal_key = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
            self.store.delete(journal_key.as_bytes())?;

            // Remove from timestamp index
            let ts_key = format!(
                "{}{:020}:{}",
                JOURNAL_TS_PREFIX,
                entry_timestamp,
                hash.to_hex()
            );
            self.store.delete(ts_key.as_bytes())?;

            debug!("Removed entry {} from active ledger", hash);
        }

        // Extract just the hashes for return value and notification
        let archived_hashes: Vec<ContentHash> =
            archived_entries.iter().map(|(h, _)| h.clone()).collect();

        // Step 5: Rebuild fork index with remaining entries
        self.fork_detector = ForkDetector::new();
        self.rebuild_fork_index()?;

        // Step 6: Recompute balances from remaining entries
        self.recompute_balances()?;

        info!(
            "✓ Rollback complete: archived {} entries, new balance for {} accounts",
            archived_count,
            self.cached_balances.len()
        );

        // Step 7: Broadcast rollback notification via gossip
        if broadcast {
            if let Some(ref gossip) = self.gossip {
                let notification = LedgerSyncMessage::RollbackNotification {
                    target_hash: target_hash.clone(),
                    archived_entries: archived_hashes.clone(),
                    reason: reason.to_string(),
                    executed_at: archive_timestamp,
                };

                let data = serialize_sync_message(&notification)?;
                // Use "ledger:system" topic for system-wide notifications
                if let Err(e) = gossip.write().await.publish("ledger:system", data).await {
                    warn!("Failed to broadcast rollback notification: {}", e);
                } else {
                    info!("Broadcast rollback notification to network");
                }
            }
        }

        // Emit metrics
        icn_obs::metrics::ledger::rollback_performed_inc();

        // Emit rollback event
        if let Some(ref emitter) = self.event_emitter {
            emitter.emit_rollback_performed(target_hash, archived_count, reason, archive_timestamp);
        }

        Ok(archived_hashes)
    }

    /// Get archived entries for a specific rollback timestamp
    pub fn get_archived_entries(&self, archive_timestamp: u64) -> Result<Vec<JournalEntry>> {
        let prefix = format!("{ARCHIVE_PREFIX}{archive_timestamp}:");
        let pairs = self.store.scan(prefix.as_bytes())?;

        let mut entries = Vec::new();
        for (_key, value) in pairs {
            let record: ArchiveRecord = serde_json::from_slice(&value)?;
            entries.push(record.entry);
        }

        Ok(entries)
    }

    /// List all rollback timestamps (for recovery purposes)
    pub fn list_rollback_timestamps(&self) -> Result<Vec<u64>> {
        let prefix = ARCHIVE_PREFIX.as_bytes();
        let pairs = self.store.scan(prefix)?;

        let mut timestamps = std::collections::HashSet::new();
        for (key, _value) in pairs {
            // Key format: "ledger:archive:{timestamp}:{hash}"
            let key_str = String::from_utf8_lossy(&key);
            if let Some(rest) = key_str.strip_prefix(ARCHIVE_PREFIX) {
                if let Some(ts_str) = rest.split(':').next() {
                    if let Ok(ts) = ts_str.parse::<u64>() {
                        timestamps.insert(ts);
                    }
                }
            }
        }

        let mut sorted: Vec<_> = timestamps.into_iter().collect();
        sorted.sort();
        Ok(sorted)
    }

    /// Validate a journal entry before accepting it
    ///
    /// This validates:
    /// - Entry has at least one account delta
    /// - Author and affected accounts are not frozen (Issue #25)
    /// - Double-entry invariants (Σ debits == Σ credits per currency)
    /// - Credit limits are respected
    ///
    /// Note: Parent validation (Merkle-DAG links) is done separately in
    /// `append_entry_internal` with different behavior for local vs sync operations
    /// (Issue #499: hybrid causal ordering).
    fn validate_entry(&mut self, entry: &JournalEntry) -> Result<()> {
        // Check that entry has at least one account delta
        if entry.accounts.is_empty() {
            anyhow::bail!("Entry has no account deltas");
        }

        // Check for frozen members (Issue #25)
        // Both the author and all affected accounts must not be frozen
        if self.freeze_manager.is_frozen(&entry.author) {
            anyhow::bail!(
                "Entry author {} is frozen and cannot create transactions",
                entry.author
            );
        }

        for delta in &entry.accounts {
            if self.freeze_manager.is_frozen(&delta.account_id) {
                anyhow::bail!(
                    "Account {} is frozen and cannot participate in transactions",
                    delta.account_id
                );
            }
        }

        // Check double-entry invariant per currency
        let mut currency_sums: HashMap<String, i64> = HashMap::new();
        for delta in &entry.accounts {
            let sum = currency_sums.entry(delta.currency.clone()).or_insert(0);
            let change = delta.net_change()?;
            *sum = sum.checked_add(change).ok_or_else(|| {
                LedgerError::ArithmeticOverflow(format!(
                    "overflow in double-entry check: {sum} + {change} for currency {}",
                    delta.currency
                ))
            })?;
        }

        // All currencies must sum to zero (double-entry)
        for (currency, sum) in currency_sums {
            if sum != 0 {
                anyhow::bail!("Currency {currency} does not balance (sum = {sum})");
            }
        }

        // Enforce credit limits (Issue #164, #326)
        // Check that no account would exceed its credit limit after this entry.
        // Priority: dynamic_limit_manager > credit_policy_manager
        let has_credit_limits =
            self.dynamic_limit_manager.is_some() || self.credit_policy_manager.is_some();

        if has_credit_limits {
            // PERFORMANCE: Calculate current time once for all deltas in this entry.
            // This ensures consistent timestamps and reduces syscalls.
            // SECURITY: Use actual system time for ramping calculation.
            // This prevents timestamp manipulation attacks where a malicious actor
            // creates entries with old timestamps to bypass credit limit ramping.
            let validation_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            for delta in &entry.accounts {
                // Only check accounts that are going more negative (spending credit)
                let transaction_delta = delta.net_change()?;
                if transaction_delta >= 0 {
                    // Account is receiving, not spending credit
                    continue;
                }

                let account = &delta.account_id;
                let currency = &delta.currency;
                let current_balance = self.get_balance(account, currency);

                // Get trust score for the account (or 0.0 if unknown)
                // SAFETY: Use block_in_place because validate_entry is sync but may be
                // called from async context (e.g., append_entry_internal).
                let trust_score: f64 = if let Some(ref trust_graph) = self.trust_graph {
                    tokio::task::block_in_place(|| {
                        let graph = trust_graph.blocking_read();
                        graph.compute_trust_score(account).unwrap_or(0.0)
                    })
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);

                // Get cleared volume for history bonus
                let cleared_volume = self
                    .cleared_volume_index
                    .get(&(account.clone(), currency.clone()))
                    .copied()
                    .unwrap_or(0);

                // Calculate credit limit - use dynamic manager if available
                let calculated_limit = if let Some(ref dynamic_manager) = self.dynamic_limit_manager
                {
                    // Use dynamic limit manager (includes decay/recovery)
                    match dynamic_manager.get_effective_limit(
                        account,
                        currency,
                        trust_score,
                        cleared_volume,
                    ) {
                        Ok(limit) => limit,
                        Err(e) => {
                            // On error, log and fall back to static calculation
                            warn!(
                                account = %account,
                                currency = currency,
                                error = ?e,
                                "Dynamic limit calculation failed; falling back to static"
                            );
                            self.calculate_static_credit_limit(
                                account,
                                currency,
                                trust_score,
                                cleared_volume,
                                validation_time,
                                entry.timestamp,
                            )
                        }
                    }
                } else {
                    // Use static credit policy manager
                    self.calculate_static_credit_limit(
                        account,
                        currency,
                        trust_score,
                        cleared_volume,
                        validation_time,
                        entry.timestamp,
                    )
                };

                // Check if the new balance would exceed the limit
                let new_balance = current_balance + transaction_delta;
                if new_balance < -calculated_limit {
                    warn!(
                        account = %account,
                        currency = currency,
                        current_balance = current_balance,
                        transaction_delta = transaction_delta,
                        new_balance = new_balance,
                        credit_limit = calculated_limit,
                        "Entry would exceed credit limit"
                    );
                    icn_obs::metrics::ledger::entries_rejected_credit_limit_inc();
                    anyhow::bail!(
                        "Account {account} would exceed credit limit for {currency}: new balance {new_balance} < -{calculated_limit}"
                    );
                }
            }
        }

        // Enforce progressive balance and velocity limits (Issue #336)
        // Check that no account would exceed POPLevel-based limits after this entry
        if let Some(ref prog_manager) = self.progressive_limit_manager {
            for delta in &entry.accounts {
                let account = &delta.account_id;
                let currency = &delta.currency;
                let net_change = delta.net_change()?;

                // Get POPLevel for the account (defaults to Weak if unknown)
                let pop_level = prog_manager.get_pop_level(account.as_str());

                // Calculate new balance after this transaction
                let current_balance = self.get_balance(account, currency);
                let new_balance = current_balance.checked_add(net_change).ok_or_else(|| {
                    LedgerError::ArithmeticOverflow(format!(
                        "overflow calculating new balance: {current_balance} + {net_change} for account {account} currency {currency}"
                    ))
                })?;

                // Check balance limits based on POPLevel
                if let Err(e) = prog_manager.check_balance_limits(
                    account.as_str(),
                    currency,
                    new_balance,
                    pop_level,
                ) {
                    warn!(
                        account = %account,
                        currency = currency,
                        pop_level = ?pop_level,
                        new_balance = new_balance,
                        "Entry would exceed progressive balance limit"
                    );
                    icn_obs::metrics::ledger::progressive_balance_limit_exceeded_inc();
                    anyhow::bail!("{e}");
                }

                // Check velocity limits (only for positive changes)
                if let Err(e) =
                    prog_manager.check_velocity_limits(account.as_str(), currency, net_change)
                {
                    warn!(
                        account = %account,
                        currency = currency,
                        net_change = net_change,
                        "Entry would exceed progressive velocity limit"
                    );
                    icn_obs::metrics::ledger::progressive_velocity_limit_exceeded_inc();
                    anyhow::bail!("{e}");
                }
            }
        }

        Ok(())
    }

    /// Calculate static credit limit using credit_policy_manager
    ///
    /// This is the fallback method when dynamic_limit_manager is not configured
    /// or fails. It applies the base policy formula plus new member ramping.
    fn calculate_static_credit_limit(
        &self,
        account: &Did,
        _currency: &str,
        trust_score: f64,
        cleared_volume: i64,
        validation_time: u64,
        entry_timestamp: u64,
    ) -> i64 {
        let Some(ref policy_manager) = self.credit_policy_manager else {
            // No policy manager = unlimited credit (return i64::MAX)
            return i64::MAX;
        };

        let base_policy = &policy_manager.credit_policy;

        // Calculate: baseline + trust_bonus + history_bonus
        let trust_bonus =
            (base_policy.baseline as f64 * trust_score * base_policy.trust_multiplier) as i64;
        let history_bonus = (cleared_volume as f64 * base_policy.history_bonus_rate) as i64;
        let full_limit = base_policy.baseline + trust_bonus + history_bonus;

        // Apply new member ramping only if membership store is configured
        let Some(ref membership_store) = self.membership_store else {
            return full_limit;
        };

        // Track when members bypass ramping via cleared volume threshold
        if cleared_volume >= policy_manager.new_member_policy.cleared_volume_threshold {
            icn_obs::metrics::ledger::credit_limit_bypass_via_contribution_inc();
        }

        // Get member_since from store, with proper error handling
        match membership_store.get_member_since(account) {
            Ok(Some(member_since)) => {
                // Apply new member ramping
                policy_manager.new_member_policy.calculate_effective_limit(
                    account,
                    member_since,
                    validation_time,
                    cleared_volume,
                    full_limit,
                )
            }
            Ok(None) => {
                // New member - use entry timestamp as join date
                policy_manager.new_member_policy.calculate_effective_limit(
                    account,
                    entry_timestamp,
                    validation_time,
                    cleared_volume,
                    full_limit,
                )
            }
            Err(e) => {
                // Storage error - log, track metric, and use full limit
                warn!(
                    account = %account,
                    error = ?e,
                    "Failed to read member_since; using full credit limit"
                );
                icn_obs::metrics::ledger::membership_storage_errors_inc();
                full_limit
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::JournalEntryBuilder;
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use tempfile::TempDir;

    fn create_test_ledger() -> (Ledger, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let ledger = Ledger::new(store).unwrap();
        (ledger, temp_dir)
    }

    #[tokio::test]
    async fn test_append_and_retrieve_entry() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let hash = entry.id.clone().unwrap();
        ledger.append_entry(entry).await.unwrap();

        let retrieved = ledger.get_entry(&hash).unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.accounts.len(), 2);
    }

    #[tokio::test]
    async fn test_balance_computation() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry1 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let entry2 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .build()
            .unwrap();

        ledger.append_entry(entry1).await.unwrap();
        ledger.append_entry(entry2).await.unwrap();

        assert_eq!(ledger.get_balance(&alice, "hours"), 15);
        assert_eq!(ledger.get_balance(&bob, "hours"), -15);
    }

    #[tokio::test]
    async fn test_verify_integrity() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        ledger.append_entry(entry).await.unwrap();

        assert!(ledger.verify_integrity().is_ok());
    }

    #[tokio::test]
    async fn test_recompute_balances() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        ledger.append_entry(entry).await.unwrap();

        // Manually corrupt balances
        ledger.cached_balances.clear();

        // Recompute should restore correct balances
        ledger.recompute_balances().unwrap();

        assert_eq!(ledger.get_balance(&alice, "hours"), 10);
        assert_eq!(ledger.get_balance(&bob, "hours"), -10);
    }

    #[tokio::test]
    async fn test_merge_batch_accepts_valid_entries() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create two valid entries
        let entry1 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let entry2 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .build()
            .unwrap();

        let entries = vec![entry1, entry2];

        // Merge batch
        let decision = ledger.merge_batch(entries).await.unwrap();

        // Both should be accepted
        assert_eq!(decision.accepted_count, 2);
        assert_eq!(decision.discarded.len(), 0);
        assert_eq!(decision.quarantined.len(), 0);

        // Verify balances updated correctly
        assert_eq!(ledger.get_balance(&alice, "hours"), 15);
        assert_eq!(ledger.get_balance(&bob, "hours"), -15);
    }

    #[tokio::test]
    async fn test_merge_batch_discards_duplicates() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create entry
        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let entry_id = entry.id.clone().unwrap();

        // Append once directly
        ledger.append_entry(entry.clone()).await.unwrap();

        // Try to merge same entry again
        let entries = vec![entry];
        let decision = ledger.merge_batch(entries).await.unwrap();

        // Should be discarded as duplicate
        assert_eq!(decision.accepted_count, 0);
        assert_eq!(decision.discarded.len(), 1);
        assert_eq!(decision.discarded[0], entry_id);
    }

    #[tokio::test]
    async fn test_merge_batch_quarantines_invalid_entries() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create invalid entry (unbalanced - only credit, no debit)
        let mut entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        // Corrupt it by removing one account delta (makes it unbalanced)
        entry.accounts.pop();

        let entry_id = entry.id.clone().unwrap();

        // Merge the invalid entry
        let entries = vec![entry];
        let decision = ledger.merge_batch(entries).await.unwrap();

        // Should be quarantined
        assert_eq!(decision.accepted_count, 0);
        assert_eq!(decision.quarantined.len(), 1);
        assert_eq!(decision.quarantined[0].entry_id, entry_id);

        // Verify it's in quarantine store
        let quarantine_items = ledger.quarantine().list().unwrap();
        assert_eq!(quarantine_items.len(), 1);
    }

    #[tokio::test]
    async fn test_merge_decision_stored() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        // Merge
        ledger.merge_batch(vec![entry]).await.unwrap();

        // Last merge decision should be available
        let last_decision = ledger.last_merge_decision();
        assert!(last_decision.is_some());

        let decision = last_decision.unwrap();
        assert_eq!(decision.accepted_count, 1);
    }

    // === Emergency Flow Tests (Issue #25) ===

    #[tokio::test]
    async fn test_freeze_member_blocks_transactions_as_author() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Alice creates a successful entry before being frozen
        let entry1 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let decision1 = ledger.merge_batch(vec![entry1]).await.unwrap();
        assert_eq!(decision1.accepted_count, 1);

        // Freeze Alice
        ledger.freeze_member(alice.clone(), "Suspected fraud".to_string(), None);
        assert!(ledger.is_member_frozen(&alice));

        // Alice tries to create another entry - should be quarantined
        let entry2 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .build()
            .unwrap();

        let decision2 = ledger.merge_batch(vec![entry2]).await.unwrap();
        assert_eq!(decision2.accepted_count, 0);
        assert_eq!(decision2.quarantined.len(), 1);

        // Original balance should be unchanged
        assert_eq!(ledger.get_balance(&alice, "hours"), 10);
    }

    #[tokio::test]
    async fn test_freeze_member_blocks_transactions_as_participant() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let charlie = KeyPair::generate().unwrap().did().clone();

        // First transaction is fine
        let entry1 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        ledger.merge_batch(vec![entry1]).await.unwrap();

        // Freeze Bob (not the author, but a participant)
        ledger.freeze_member(bob.clone(), "Account compromised".to_string(), None);
        assert!(ledger.is_member_frozen(&bob));

        // Charlie (not frozen) tries to transact with Bob (frozen) - should fail
        let entry2 = JournalEntryBuilder::new(charlie.clone())
            .debit(charlie.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .build()
            .unwrap();

        let decision = ledger.merge_batch(vec![entry2]).await.unwrap();
        assert_eq!(decision.accepted_count, 0);
        assert_eq!(decision.quarantined.len(), 1);

        // Bob's balance should be unchanged
        assert_eq!(ledger.get_balance(&bob, "hours"), -10);
    }

    #[tokio::test]
    async fn test_unfreeze_member_allows_transactions() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Freeze Alice
        ledger.freeze_member(alice.clone(), "Investigation".to_string(), None);
        assert!(ledger.is_member_frozen(&alice));

        // Alice tries to transact - should fail
        let entry1 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let decision1 = ledger.merge_batch(vec![entry1]).await.unwrap();
        assert_eq!(decision1.quarantined.len(), 1);

        // Unfreeze Alice
        let removed = ledger.unfreeze_member(&alice, "Investigation complete".to_string());
        assert!(removed.is_some());
        assert!(!ledger.is_member_frozen(&alice));

        // Alice should be able to transact now
        let entry2 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let decision2 = ledger.merge_batch(vec![entry2]).await.unwrap();
        assert_eq!(decision2.accepted_count, 1);
        assert_eq!(ledger.get_balance(&alice, "hours"), 10);
    }

    #[test]
    fn test_freeze_with_metadata() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let admin = KeyPair::generate().unwrap().did().clone();

        // Freeze with full metadata
        ledger.freeze_member_with_metadata(
            alice.clone(),
            "Fraud detected".to_string(),
            Some(86400), // 24 hours
            Some("proposal-freeze-123".to_string()),
            Some(admin.clone()),
        );

        // Verify frozen
        assert!(ledger.is_member_frozen(&alice));

        // Get freeze record
        let record = ledger.get_freeze_record(&alice).unwrap();
        assert_eq!(record.reason, "Fraud detected");
        assert_eq!(record.proposal_id, Some("proposal-freeze-123".to_string()));
        assert_eq!(record.frozen_by, Some(admin));
    }

    #[test]
    fn test_list_frozen_members() {
        let (mut ledger, _temp) = create_test_ledger();

        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let charlie = KeyPair::generate().unwrap().did().clone();

        // Freeze two members
        ledger.freeze_member(alice.clone(), "Reason A".to_string(), None);
        ledger.freeze_member(bob.clone(), "Reason B".to_string(), Some(3600));

        // List should have 2
        assert_eq!(ledger.frozen_member_count(), 2);

        // Charlie is not frozen
        assert!(!ledger.is_member_frozen(&charlie));

        // Unfreeze one
        ledger.unfreeze_member(&alice, "Cleared".to_string());
        assert_eq!(ledger.frozen_member_count(), 1);
    }

    #[tokio::test]
    async fn test_freeze_preserves_balance() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create some transactions first
        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 50)
            .credit(bob.clone(), "hours".to_string(), 50)
            .build()
            .unwrap();

        ledger.merge_batch(vec![entry]).await.unwrap();
        assert_eq!(ledger.get_balance(&alice, "hours"), 50);
        assert_eq!(ledger.get_balance(&bob, "hours"), -50);

        // Freeze both
        ledger.freeze_member(alice.clone(), "Investigation".to_string(), None);
        ledger.freeze_member(bob.clone(), "Investigation".to_string(), None);

        // Balances should remain unchanged
        assert_eq!(ledger.get_balance(&alice, "hours"), 50);
        assert_eq!(ledger.get_balance(&bob, "hours"), -50);

        // Unfreeze and verify still correct
        ledger.unfreeze_member(&alice, "Clear".to_string());
        ledger.unfreeze_member(&bob, "Clear".to_string());
        assert_eq!(ledger.get_balance(&alice, "hours"), 50);
        assert_eq!(ledger.get_balance(&bob, "hours"), -50);
    }

    // ========================================================================
    // Progressive Limit Integration Tests (Issue #336)
    // ========================================================================

    use crate::progressive_limits::{
        DefaultWeakLookup, FnCommonsHolderLookup, ProgressiveLimitManager,
    };

    fn create_test_ledger_with_progressive_limits() -> (Ledger, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

        // Create progressive limit manager with default (Weak) lookup
        let lookup = Arc::new(DefaultWeakLookup);
        let prog_manager = Arc::new(ProgressiveLimitManager::new(
            store.clone() as Arc<dyn icn_store::Store>,
            lookup,
        ));

        let mut ledger = Ledger::new(store).unwrap();
        ledger.set_progressive_limit_manager(prog_manager);
        (ledger, temp_dir)
    }

    #[tokio::test]
    async fn test_progressive_balance_limit_blocks_excessive_accumulation() {
        let (mut ledger, _temp) = create_test_ledger_with_progressive_limits();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Weak POPLevel has max_balance of 500
        // Try to give alice 600 hours (should fail)
        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 600)
            .credit(bob.clone(), "hours".to_string(), 600)
            .build()
            .unwrap();

        let result = ledger.append_entry(entry).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Balance limit exceeded"));
    }

    #[tokio::test]
    async fn test_progressive_balance_limit_allows_within_limit() {
        let (mut ledger, _temp) = create_test_ledger_with_progressive_limits();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Weak POPLevel: max_balance=500, max_credit=50
        // Give alice 40 hours - bob goes to -40 (within credit limit of 50)
        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 40)
            .credit(bob.clone(), "hours".to_string(), 40)
            .build()
            .unwrap();

        let result = ledger.append_entry(entry).await;
        assert!(result.is_ok(), "Expected Ok but got: {:?}", result.err());
        assert_eq!(ledger.get_balance(&alice, "hours"), 40);
        assert_eq!(ledger.get_balance(&bob, "hours"), -40);
    }

    #[tokio::test]
    async fn test_progressive_velocity_limit_blocks_rapid_accumulation() {
        // Use Strong POPLevel for Alice to test velocity independently of balance
        // Strong: max_balance=5000, max_credit=500, daily_velocity=500
        // This lets us test velocity limit (500) without hitting balance limit (5000)

        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let alice_str = alice.to_string();

        // Create lookup that returns Strong for Alice, Weak for others
        let lookup = Arc::new(FnCommonsHolderLookup::new(move |did: &str| {
            if did == alice_str {
                Some(icn_identity::POPLevel::Strong)
            } else {
                Some(icn_identity::POPLevel::Weak)
            }
        }));

        let prog_manager = Arc::new(ProgressiveLimitManager::new(
            store.clone() as Arc<dyn icn_store::Store>,
            lookup,
        ));

        let mut ledger = Ledger::new(store).unwrap();
        ledger.set_progressive_limit_manager(prog_manager);

        let funders: Vec<_> = (0..15)
            .map(|_| KeyPair::generate().unwrap().did().clone())
            .collect();

        // Alice receives 50 from 10 different funders = 500 total
        // Funders are Weak (max_credit=50), so 50 per funder is at their limit
        // Velocity = 500 (at limit), balance = 500 (well under 5000 limit)
        for (i, funder) in funders.iter().take(10).enumerate() {
            let entry = JournalEntryBuilder::new(alice.clone())
                .debit(alice.clone(), "hours".to_string(), 50)
                .credit(funder.clone(), "hours".to_string(), 50)
                .build()
                .unwrap();

            let result = ledger.append_entry(entry).await;
            assert!(
                result.is_ok(),
                "Receive {} failed: {:?}",
                i + 1,
                result.err()
            );
        }
        assert_eq!(ledger.get_balance(&alice, "hours"), 500);

        // Try to receive 10 more - this should fail on VELOCITY
        // Balance would be 510 (< 5000, OK)
        // Velocity would be 510 (> 500, FAIL)
        let entry_over = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(funders[10].clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let result = ledger.append_entry(entry_over).await;
        assert!(result.is_err(), "Expected error but got Ok");
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("Velocity limit"),
            "Expected velocity limit error but got: {err_str}"
        );
    }

    #[tokio::test]
    async fn test_progressive_velocity_allows_spending() {
        let (mut ledger, _temp) = create_test_ledger_with_progressive_limits();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let charlie = KeyPair::generate().unwrap().did().clone();

        // Build up alice's balance to 40 (within limits)
        let entry1 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 40)
            .credit(bob.clone(), "hours".to_string(), 40)
            .build()
            .unwrap();

        let result = ledger.append_entry(entry1).await;
        assert!(result.is_ok(), "Entry1 failed: {:?}", result.err());
        assert_eq!(ledger.get_balance(&alice, "hours"), 40);

        // Alice spends 30 hours to charlie
        // This is a CREDIT to alice (spending), so alice's net_change is NEGATIVE
        // Velocity should allow spending even if accumulation velocity was maxed
        let entry2 = JournalEntryBuilder::new(alice.clone())
            .debit(charlie.clone(), "hours".to_string(), 30)
            .credit(alice.clone(), "hours".to_string(), 30)
            .build()
            .unwrap();

        // This should succeed because spending (credit to alice = negative net_change) is allowed
        let result = ledger.append_entry(entry2).await;
        assert!(
            result.is_ok(),
            "Entry2 (spending) failed: {:?}",
            result.err()
        );
        assert_eq!(ledger.get_balance(&alice, "hours"), 10); // 40 - 30 = 10
        assert_eq!(ledger.get_balance(&charlie, "hours"), 30);
    }

    #[tokio::test]
    async fn test_progressive_credit_limit() {
        let (mut ledger, _temp) = create_test_ledger_with_progressive_limits();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Weak POPLevel has max_credit of 50
        // Try to put alice -100 in debt (should fail)
        let entry = JournalEntryBuilder::new(bob.clone())
            .debit(bob.clone(), "hours".to_string(), 100)
            .credit(alice.clone(), "hours".to_string(), 100)
            .build()
            .unwrap();

        let result = ledger.append_entry(entry).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Balance limit exceeded"));
    }

    // =========================================================================
    // Pagination tests
    // =========================================================================

    /// Helper to create a ledger with N entries for pagination testing.
    /// Returns the ledger, temp dir, and all participant DIDs.
    async fn create_ledger_with_entries(count: usize) -> (Ledger, TempDir, Vec<Did>) {
        let (mut ledger, temp_dir) = create_test_ledger();
        let mut dids = Vec::new();

        // Create unique DIDs for each entry to avoid balance conflicts
        for i in 0..count {
            let sender = KeyPair::generate().unwrap().did().clone();
            let receiver = KeyPair::generate().unwrap().did().clone();
            dids.push(sender.clone());
            dids.push(receiver.clone());

            let entry = JournalEntryBuilder::new(sender.clone())
                .debit(sender.clone(), "hours".to_string(), (i + 1) as i64)
                .credit(receiver.clone(), "hours".to_string(), (i + 1) as i64)
                .build()
                .unwrap();

            ledger.append_entry(entry).await.unwrap();
            // Small delay to ensure distinct timestamps
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        (ledger, temp_dir, dids)
    }

    #[tokio::test]
    async fn test_pagination_basic() {
        let (ledger, _temp, _dids) = create_ledger_with_entries(10).await;

        // Get first page of 3
        let (entries, total) = ledger.get_entries_paginated(0, 3).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(total, 10);

        // Get second page
        let (entries2, _) = ledger.get_entries_paginated(3, 3).unwrap();
        assert_eq!(entries2.len(), 3);

        // Entries should be different (using timestamp as unique identifier)
        assert_ne!(entries[0].timestamp, entries2[0].timestamp);
    }

    #[tokio::test]
    async fn test_pagination_last_page() {
        let (ledger, _temp, _dids) = create_ledger_with_entries(10).await;

        // Get last partial page (offset 8, limit 5 -> should only get 2)
        let (entries, total) = ledger.get_entries_paginated(8, 5).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(total, 10);
    }

    #[test]
    fn test_pagination_empty_ledger() {
        let (ledger, _temp) = create_test_ledger();

        let (entries, total) = ledger.get_entries_paginated(0, 10).unwrap();
        assert!(entries.is_empty());
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn test_pagination_exact_boundary() {
        let (ledger, _temp, _dids) = create_ledger_with_entries(10).await;

        // Get exactly all entries
        let (entries, total) = ledger.get_entries_paginated(0, 10).unwrap();
        assert_eq!(entries.len(), 10);
        assert_eq!(total, 10);

        // Offset at boundary should return empty
        let (entries, _) = ledger.get_entries_paginated(10, 5).unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_filtered_pagination_no_filter() {
        let (ledger, _temp, _dids) = create_ledger_with_entries(10).await;

        // No filter, no cursor - fast path
        let (entries, next_cursor) = ledger
            .get_entries_filtered_paginated(None, None, 5)
            .unwrap();
        assert_eq!(entries.len(), 5);
        assert!(next_cursor.is_some());

        // Get second page using cursor (convert (ts, hash) to (ts, Some(hash)))
        let cursor_for_next = next_cursor.map(|(ts, hash)| (ts, Some(hash)));
        let (entries2, next_cursor2) = ledger
            .get_entries_filtered_paginated(None, cursor_for_next, 5)
            .unwrap();
        assert_eq!(entries2.len(), 5);
        assert!(next_cursor2.is_none()); // No more pages
    }

    #[tokio::test]
    async fn test_filtered_pagination_with_did_filter() {
        let (mut ledger, temp_dir) = create_test_ledger();

        // Create specific test scenario with known DID
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let charlie = KeyPair::generate().unwrap().did().clone();

        // Alice sends 5 entries to Bob
        for i in 0..5 {
            let entry = JournalEntryBuilder::new(alice.clone())
                .debit(alice.clone(), "hours".to_string(), (i + 1) as i64)
                .credit(bob.clone(), "hours".to_string(), (i + 1) as i64)
                .build()
                .unwrap();
            ledger.append_entry(entry).await.unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // Charlie sends 3 entries (should not match alice filter)
        for i in 0..3 {
            let entry = JournalEntryBuilder::new(charlie.clone())
                .debit(charlie.clone(), "hours".to_string(), (i + 1) as i64)
                .credit(bob.clone(), "hours".to_string(), (i + 1) as i64)
                .build()
                .unwrap();
            ledger.append_entry(entry).await.unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // Filter by alice - should get 5 entries
        let (entries, next_cursor) = ledger
            .get_entries_filtered_paginated(Some(&alice), None, 10)
            .unwrap();
        assert_eq!(entries.len(), 5, "Should find exactly 5 entries for alice");
        assert!(next_cursor.is_none());

        // Filter by alice with small page size
        let (page1, cursor1) = ledger
            .get_entries_filtered_paginated(Some(&alice), None, 3)
            .unwrap();
        assert_eq!(page1.len(), 3);
        assert!(cursor1.is_some());

        // Get second page (convert (ts, hash) to (ts, Some(hash)))
        let cursor1_for_next = cursor1.map(|(ts, hash)| (ts, Some(hash)));
        let (page2, cursor2) = ledger
            .get_entries_filtered_paginated(Some(&alice), cursor1_for_next, 3)
            .unwrap();
        assert_eq!(page2.len(), 2);
        assert!(cursor2.is_none());

        // Verify pagination doesn't return duplicates (using timestamp as unique identifier)
        let page1_ts: Vec<_> = page1.iter().map(|e| e.timestamp).collect();
        let page2_ts: Vec<_> = page2.iter().map(|e| e.timestamp).collect();
        for ts in &page2_ts {
            assert!(!page1_ts.contains(ts), "Duplicate entry across pages");
        }
        drop(temp_dir);
    }

    #[tokio::test]
    async fn test_filtered_pagination_cursor_continuity() {
        let (ledger, _temp, _dids) = create_ledger_with_entries(15).await;

        let mut all_entries = Vec::new();
        let mut cursor: Option<(u64, Option<String>)> = None;

        // Paginate through all entries with page size 4
        loop {
            let (entries, next_cursor) = ledger
                .get_entries_filtered_paginated(None, cursor, 4)
                .unwrap();

            if entries.is_empty() {
                break;
            }

            all_entries.extend(entries);
            // Convert (ts, hash) to (ts, Some(hash)) for next iteration
            cursor = next_cursor.map(|(ts, hash)| (ts, Some(hash)));

            if cursor.is_none() {
                break;
            }
        }

        // Should have collected all 15 entries
        assert_eq!(all_entries.len(), 15);

        // Verify no duplicates (using timestamp as unique identifier)
        let mut seen_ts = std::collections::HashSet::new();
        for entry in &all_entries {
            assert!(
                seen_ts.insert(entry.timestamp),
                "Duplicate entry in pagination"
            );
        }
    }

    #[tokio::test]
    async fn test_pagination_asc_order() {
        let (ledger, _temp, _dids) = create_ledger_with_entries(5).await;

        let (entries, _) = ledger.get_entries_paginated_asc(0, 10).unwrap();

        // Verify ascending order by timestamp
        for i in 1..entries.len() {
            assert!(
                entries[i].timestamp >= entries[i - 1].timestamp,
                "Entries should be in ascending timestamp order"
            );
        }
    }

    #[tokio::test]
    async fn test_pagination_desc_order() {
        let (ledger, _temp, _dids) = create_ledger_with_entries(5).await;

        let (entries, _) = ledger.get_entries_paginated(0, 10).unwrap();

        // Verify descending order by timestamp
        for i in 1..entries.len() {
            assert!(
                entries[i].timestamp <= entries[i - 1].timestamp,
                "Entries should be in descending timestamp order"
            );
        }
    }

    // Issue #499: Causal ordering tests

    #[tokio::test]
    async fn test_local_entry_with_missing_parent_rejected() {
        let (mut ledger, _temp) = create_test_ledger();

        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create an entry that references a non-existent parent
        let fake_parent = ContentHash::from_bytes([0xDE; 32]);

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .add_parent(fake_parent)
            .build()
            .unwrap();

        // Local append should fail due to missing parent
        let result = ledger.append_entry(entry).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("missing parent"),
            "Error should mention missing parent: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_sync_entry_with_missing_parent_accepted_as_orphan() {
        let (mut ledger, _temp) = create_test_ledger();

        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create an entry that references a non-existent parent
        let fake_parent = ContentHash::from_bytes([0xDE; 32]);

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .add_parent(fake_parent)
            .build()
            .unwrap();

        let hash = entry.id.clone().unwrap();

        // Sync append should succeed (orphan tracking)
        let result = ledger.append_entry_from_sync(entry).await;
        assert!(result.is_ok(), "Sync entry should be accepted: {result:?}");

        // Verify entry was stored
        let stored = ledger.get_entry(&hash).unwrap();
        assert!(stored.is_some(), "Entry should be stored as orphan");
    }

    #[tokio::test]
    async fn test_entry_with_valid_parent_accepted() {
        let (mut ledger, _temp) = create_test_ledger();

        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create first entry (no parent)
        let entry1 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();
        let hash1 = entry1.id.clone().unwrap();
        ledger.append_entry(entry1).await.unwrap();

        // Create second entry referencing the first as parent
        let entry2 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .add_parent(hash1.clone())
            .build()
            .unwrap();
        let hash2 = entry2.id.clone().unwrap();

        // Local append should succeed since parent exists
        let result = ledger.append_entry(entry2).await;
        assert!(result.is_ok(), "Entry with valid parent should be accepted");

        // Verify both entries exist
        assert!(ledger.get_entry(&hash1).unwrap().is_some());
        assert!(ledger.get_entry(&hash2).unwrap().is_some());
    }

    #[tokio::test]
    async fn test_entry_without_parents_accepted() {
        let (mut ledger, _temp) = create_test_ledger();

        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Entry with no parents (genesis-style) should be accepted
        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let result = ledger.append_entry(entry).await;
        assert!(
            result.is_ok(),
            "Entry without parents should be accepted: {result:?}"
        );
    }
}
