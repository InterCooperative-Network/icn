//! Ledger implementation with storage

use crate::balance::compute_all_balances;
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
use tracing::{debug, info, instrument, warn};

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

/// Ledger manager for double-entry mutual credit accounting
pub struct Ledger {
    /// Storage backend
    store: Arc<dyn Store>,

    /// Cached balances (in-memory for fast queries)
    cached_balances: HashMap<Did, AccountBalances>,

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
}

impl Ledger {
    /// Create a new ledger with the given storage backend
    pub fn new(store: Arc<dyn Store>) -> Result<Self> {
        let quarantine = QuarantineStore::new(store.clone());
        let freeze_manager = FreezeManager::with_store(store.clone())?;

        let mut ledger = Ledger {
            store,
            cached_balances: HashMap::new(),
            gossip: None,
            quarantine,
            last_merge: None,
            fork_detector: ForkDetector::new(),
            fork_resolver: ForkResolver::new(ForkResolutionStrategy::default()), // Hybrid strategy
            misbehavior_detector: None, // Set via set_misbehavior_detector()
            freeze_manager,
        };

        // Load cached balances from storage
        ledger.load_cached_balances()?;

        // Index existing entries for fork detection
        ledger.rebuild_fork_index()?;

        Ok(ledger)
    }

    /// Set the trust graph for trust-weighted fork resolution (Phase 18 Week 5)
    pub fn set_trust_graph(&mut self, trust_graph: Arc<TrustGraph>) {
        self.fork_resolver.set_trust_graph(trust_graph);
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

    /// Append a journal entry to the ledger
    #[instrument(skip(self, entry), fields(entry_hash = entry.id.as_ref().map(|h| h.to_hex()).unwrap_or_else(|| "none".to_string()), account_count = entry.accounts.len()))]
    pub fn append_entry(&mut self, entry: JournalEntry) -> Result<ContentHash> {
        // Validate the entry has a hash
        let hash = entry
            .id
            .as_ref()
            .context("Entry must have a computed hash before appending")?
            .clone();

        // Serialize and store
        let key = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
        let value = serde_json::to_vec(&entry)?;
        self.store.put(key.as_bytes(), &value)?;

        debug!(
            entry_hash = %hash,
            account_count = entry.accounts.len(),
            "Appended journal entry to store"
        );

        // Update cached balances
        for delta in &entry.accounts {
            let account_balances = self
                .cached_balances
                .entry(delta.account_id.clone())
                .or_insert_with(|| AccountBalances::new(delta.account_id.clone()));

            account_balances.apply_delta(delta);
        }

        // Persist updated balances
        self.save_cached_balances()?;

        info!(
            entry_hash = %hash,
            account_count = entry.accounts.len(),
            timestamp = entry.timestamp,
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
            }
        }

        // Publish to gossip if available
        if let Some(gossip) = &self.gossip {
            self.publish_to_gossip(gossip, &entry)?;
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

                // Append the entry (this will also publish to gossip if gossip is set,
                // but we should avoid re-publishing entries we just received)
                // For now, temporarily remove gossip handle to avoid re-broadcast
                let gossip = self.gossip.take();
                let result = self.append_entry(entry);
                self.gossip = gossip;

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
                    // Temporarily remove gossip to avoid re-broadcast
                    let gossip = self.gossip.take();
                    let result = self.append_entry(e);
                    self.gossip = gossip;

                    if let Err(e) = result {
                        warn!("Failed to store entry from response: {}", e);
                    }
                } else {
                    debug!("Entry {} not found on remote", hash);
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

    /// Get total cleared volume for an account in a currency
    ///
    /// This sums all credit (positive contribution) deltas for the account,
    /// which represents their total historical contributions to the system.
    /// Used for calculating credit limit bonuses based on transaction history.
    ///
    /// Example: If Alice has received 500 hours of credits over time,
    /// this returns 500 (even if her current balance is lower due to debits).
    pub fn total_cleared_by(&self, account_id: &Did, currency: &str) -> Result<i64> {
        let entries = self.get_all_entries()?;

        let mut total_credits: i64 = 0;

        for entry in entries {
            for delta in &entry.accounts {
                if delta.account_id == *account_id && delta.currency == currency {
                    // Sum only credits (positive contributions)
                    if let Some(credit) = delta.credit {
                        total_credits += credit;
                    }
                }
            }
        }

        Ok(total_credits)
    }

    /// Recompute all balances from journal entries (for verification)
    pub fn recompute_balances(&mut self) -> Result<()> {
        info!(
            cached_account_count = self.cached_balances.len(),
            "Recomputing all balances from journal"
        );

        let entries = self.get_all_entries()?;
        let balances = compute_all_balances(&entries);

        self.cached_balances = balances;
        self.save_cached_balances()?;

        info!(
            entry_count = entries.len(),
            account_count = self.cached_balances.len(),
            "Balance recomputation complete"
        );
        Ok(())
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
        self.freeze_manager.freeze(did, reason, duration_seconds);
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
            did,
            reason,
            duration_seconds,
            proposal_id,
            frozen_by,
        );
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
        self.freeze_manager.unfreeze(did, reason)
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
        self.freeze_manager
            .unfreeze_with_metadata(did, reason, proposal_id, unfrozen_by)
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
