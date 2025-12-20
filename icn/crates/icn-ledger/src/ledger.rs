//! Ledger implementation with storage

use crate::balance::compute_all_balances;
use crate::events::{BalanceChanged, SharedEventEmitter, Transfer};
use crate::fork_resolution::{
    Fork, ForkDetector, ForkResolution, ForkResolutionStrategy, ForkResolver,
};
use crate::freeze::{FreezeManager, FrozenMember};
use crate::merge::{MergeDecision, QuarantineItem};
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

/// Key prefix for journal entries in storage
const JOURNAL_PREFIX: &str = "ledger:journal:";

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

    /// Trust graph for entry validation
    trust_graph: Option<Arc<TrustGraph>>,

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
            event_emitter: None,   // Set via set_event_emitter()
            domain_id: None,       // Set via set_domain_id()
            validation_hook: None, // Set via set_validation_hook()
        };

        // Load cached balances from storage
        ledger.load_cached_balances()?;

        // Load cleared volume index from storage
        ledger.load_cleared_volume_index()?;

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
    pub fn set_trust_graph(&mut self, trust_graph: Arc<TrustGraph>) {
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

    /// Append a journal entry to the ledger
    #[instrument(skip(self, entry), fields(entry_hash = entry.id.as_ref().map(|h| h.to_hex()).unwrap_or_else(|| "none".to_string()), account_count = entry.accounts.len()))]
    pub fn append_entry(&mut self, entry: JournalEntry) -> Result<ContentHash> {
        self.append_entry_internal(entry, true)
    }

    /// Append a journal entry without publishing to gossip
    /// Used when receiving entries from gossip to avoid re-broadcasting
    pub fn append_entry_from_sync(&mut self, entry: JournalEntry) -> Result<ContentHash> {
        self.append_entry_internal(entry, false)
    }

    /// Internal append method with control over gossip publishing
    ///
    /// # Arguments
    /// * `entry` - The journal entry to append
    /// * `broadcast` - Whether to publish to gossip (false when receiving from gossip)
    fn append_entry_internal(
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
            match trust_graph.compute_trust_score(author_did) {
                Ok(trust_score) => {
                    if trust_score < self.min_trust_for_entry {
                        warn!(
                            author = %author_did,
                            trust_score = trust_score,
                            min_required = self.min_trust_for_entry,
                            entry_hash = %hash,
                            "Rejecting entry from low-trust author"
                        );
                        icn_obs::metrics::ledger::entries_rejected_low_trust_inc();
                        anyhow::bail!(
                            "Entry author {} has insufficient trust score ({:.3} < {:.3})",
                            author_did,
                            trust_score,
                            self.min_trust_for_entry
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
                    if self.min_trust_for_entry > 0.0 {
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
                    observed_at: SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
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

        // Serialize and store
        let key = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
        let value = serde_json::to_vec(&entry)?;
        self.store.put(key.as_bytes(), &value)?;

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

            account_balances.apply_delta(delta);

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
            if let Some(credit) = delta.credit {
                let key = (delta.account_id.clone(), delta.currency.clone());
                *self.cleared_volume_index.entry(key).or_insert(0) += credit;
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
                self.publish_to_gossip(gossip, &entry)?;
            }
        }

        Ok(hash)
    }

    /// Publish a journal entry to gossip for distributed synchronization
    #[instrument(skip(self, gossip, entry), fields(entry_hash = entry.id.as_ref().map(|h| h.to_hex()).unwrap_or_else(|| "none".to_string())))]
    fn publish_to_gossip(&self, gossip: &GossipHandle, entry: &JournalEntry) -> Result<()> {
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

            // Publish via gossip (blocking lock)
            let mut gossip_actor = gossip.blocking_write();
            gossip_actor.publish(&topic, data)?;

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
    pub fn handle_sync_message(&mut self, msg: LedgerSyncMessage) -> Result<()> {
        match msg {
            LedgerSyncMessage::NewEntry { hash, mut entry } => {
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
                let result = self.append_entry_from_sync(entry);

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

                            let mut gossip_actor = gossip.blocking_write();
                            gossip_actor.publish(&topic, data)?;

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
                    debug!("Received entry {} response", hash);
                    // Use append_entry_from_sync to avoid re-broadcasting entries we received
                    let result = self.append_entry_from_sync(e);

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
                match self.rollback_to_entry(&target_hash, &reason, false) {
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
    /// # Arguments
    /// * `offset` - Number of entries to skip (0-based)
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Returns
    /// Tuple of (entries, total_count)
    ///
    /// # Performance Note
    /// Currently loads and sorts all entries in memory before paginating.
    /// For very large ledgers (100K+ entries), consider implementing a
    /// secondary timestamp index for O(log n) access. See issue #111.
    pub fn get_entries_paginated(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<JournalEntry>, usize)> {
        let prefix = JOURNAL_PREFIX.as_bytes();
        let pairs = self.store.scan(prefix)?;
        let total = pairs.len();

        // Early return if offset is beyond total
        if offset >= total {
            return Ok((Vec::new(), total));
        }

        // Deserialize and sort entries
        let mut entries = Vec::with_capacity(pairs.len());
        for (_key, value) in pairs {
            let entry: JournalEntry = serde_json::from_slice(&value)?;
            entries.push(entry);
        }

        // Sort by timestamp descending (newest first)
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply pagination
        let paginated: Vec<JournalEntry> = entries
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();

        Ok((paginated, total))
    }

    /// Get journal entries with pagination (oldest first)
    ///
    /// Returns entries in chronological order (oldest first).
    /// Useful for auditing and sequential processing.
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
        let prefix = JOURNAL_PREFIX.as_bytes();
        let pairs = self.store.scan(prefix)?;
        let total = pairs.len();

        // Early return if offset is beyond total
        if offset >= total {
            return Ok((Vec::new(), total));
        }

        // Deserialize and sort entries
        let mut entries = Vec::with_capacity(pairs.len());
        for (_key, value) in pairs {
            let entry: JournalEntry = serde_json::from_slice(&value)?;
            entries.push(entry);
        }

        // Sort by timestamp ascending (oldest first)
        entries.sort_by_key(|e| e.timestamp);

        // Apply pagination
        let paginated: Vec<JournalEntry> = entries
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();

        Ok((paginated, total))
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

            // Report violation asynchronously (block_in_place for sync context)
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
        let balances = compute_all_balances(&entries);

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
        let computed_balances = compute_all_balances(&entries);

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
    pub fn transfer_balances_for_recovery(
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
            self.append_entry(entry)?;
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
    pub fn merge_batch(&mut self, entries: Vec<JournalEntry>) -> Result<MergeDecision> {
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
            match self.append_entry(entry) {
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
    pub fn rollback_to_entry(
        &mut self,
        target_hash: &ContentHash,
        reason: &str,
        broadcast: bool,
    ) -> Result<Vec<ContentHash>> {
        use std::time::{SystemTime, UNIX_EPOCH};

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
        let mut archived_hashes = Vec::with_capacity(archived_count);
        let archive_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

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
                archived_hashes.push(hash.clone());

                debug!("Archived entry {} to {}", hash, archive_key);
            }
        }

        // Step 4: Remove entries from active ledger
        for hash in &archived_hashes {
            let journal_key = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
            self.store.delete(journal_key.as_bytes())?;
            debug!("Removed entry {} from active ledger", hash);
        }

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
                if let Err(e) = gossip.blocking_write().publish("ledger:system", data) {
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
    /// This is a simplified validation. A real implementation would:
    /// - Verify signatures
    /// - Check double-entry invariants (Σ debits == Σ credits per currency)
    /// - Enforce credit limits
    /// - Validate parent links in Merkle-DAG
    /// - Check for frozen members (Issue #25)
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
            *sum += delta.net_change();
        }

        // All currencies must sum to zero (double-entry)
        for (currency, sum) in currency_sums {
            if sum != 0 {
                anyhow::bail!("Currency {currency} does not balance (sum = {sum})");
            }
        }

        Ok(())
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

    #[test]
    fn test_append_and_retrieve_entry() {
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
        ledger.append_entry(entry).unwrap();

        let retrieved = ledger.get_entry(&hash).unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.accounts.len(), 2);
    }

    #[test]
    fn test_balance_computation() {
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

        ledger.append_entry(entry1).unwrap();
        ledger.append_entry(entry2).unwrap();

        assert_eq!(ledger.get_balance(&alice, "hours"), 15);
        assert_eq!(ledger.get_balance(&bob, "hours"), -15);
    }

    #[test]
    fn test_verify_integrity() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        ledger.append_entry(entry).unwrap();

        assert!(ledger.verify_integrity().is_ok());
    }

    #[test]
    fn test_recompute_balances() {
        let (mut ledger, _temp) = create_test_ledger();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        ledger.append_entry(entry).unwrap();

        // Manually corrupt balances
        ledger.cached_balances.clear();

        // Recompute should restore correct balances
        ledger.recompute_balances().unwrap();

        assert_eq!(ledger.get_balance(&alice, "hours"), 10);
        assert_eq!(ledger.get_balance(&bob, "hours"), -10);
    }

    #[test]
    fn test_merge_batch_accepts_valid_entries() {
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
        let decision = ledger.merge_batch(entries).unwrap();

        // Both should be accepted
        assert_eq!(decision.accepted_count, 2);
        assert_eq!(decision.discarded.len(), 0);
        assert_eq!(decision.quarantined.len(), 0);

        // Verify balances updated correctly
        assert_eq!(ledger.get_balance(&alice, "hours"), 15);
        assert_eq!(ledger.get_balance(&bob, "hours"), -15);
    }

    #[test]
    fn test_merge_batch_discards_duplicates() {
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
        ledger.append_entry(entry.clone()).unwrap();

        // Try to merge same entry again
        let entries = vec![entry];
        let decision = ledger.merge_batch(entries).unwrap();

        // Should be discarded as duplicate
        assert_eq!(decision.accepted_count, 0);
        assert_eq!(decision.discarded.len(), 1);
        assert_eq!(decision.discarded[0], entry_id);
    }

    #[test]
    fn test_merge_batch_quarantines_invalid_entries() {
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
        let decision = ledger.merge_batch(entries).unwrap();

        // Should be quarantined
        assert_eq!(decision.accepted_count, 0);
        assert_eq!(decision.quarantined.len(), 1);
        assert_eq!(decision.quarantined[0].entry_id, entry_id);

        // Verify it's in quarantine store
        let quarantine_items = ledger.quarantine().list().unwrap();
        assert_eq!(quarantine_items.len(), 1);
    }

    #[test]
    fn test_merge_decision_stored() {
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
        ledger.merge_batch(vec![entry]).unwrap();

        // Last merge decision should be available
        let last_decision = ledger.last_merge_decision();
        assert!(last_decision.is_some());

        let decision = last_decision.unwrap();
        assert_eq!(decision.accepted_count, 1);
    }

    // === Emergency Flow Tests (Issue #25) ===

    #[test]
    fn test_freeze_member_blocks_transactions_as_author() {
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

        let decision1 = ledger.merge_batch(vec![entry1]).unwrap();
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

        let decision2 = ledger.merge_batch(vec![entry2]).unwrap();
        assert_eq!(decision2.accepted_count, 0);
        assert_eq!(decision2.quarantined.len(), 1);

        // Original balance should be unchanged
        assert_eq!(ledger.get_balance(&alice, "hours"), 10);
    }

    #[test]
    fn test_freeze_member_blocks_transactions_as_participant() {
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

        ledger.merge_batch(vec![entry1]).unwrap();

        // Freeze Bob (not the author, but a participant)
        ledger.freeze_member(bob.clone(), "Account compromised".to_string(), None);
        assert!(ledger.is_member_frozen(&bob));

        // Charlie (not frozen) tries to transact with Bob (frozen) - should fail
        let entry2 = JournalEntryBuilder::new(charlie.clone())
            .debit(charlie.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .build()
            .unwrap();

        let decision = ledger.merge_batch(vec![entry2]).unwrap();
        assert_eq!(decision.accepted_count, 0);
        assert_eq!(decision.quarantined.len(), 1);

        // Bob's balance should be unchanged
        assert_eq!(ledger.get_balance(&bob, "hours"), -10);
    }

    #[test]
    fn test_unfreeze_member_allows_transactions() {
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

        let decision1 = ledger.merge_batch(vec![entry1]).unwrap();
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

        let decision2 = ledger.merge_batch(vec![entry2]).unwrap();
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

    #[test]
    fn test_freeze_preserves_balance() {
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

        ledger.merge_batch(vec![entry]).unwrap();
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
}
