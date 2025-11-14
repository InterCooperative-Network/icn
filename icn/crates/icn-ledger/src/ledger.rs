//! Ledger implementation with storage

use crate::balance::compute_all_balances;
use crate::merge::{MergeDecision, QuarantineItem};
use crate::quarantine::QuarantineStore;
use crate::sync::{serialize_sync_message, LedgerSyncMessage};
use crate::types::{AccountBalances, ContentHash, JournalEntry, QuarantineReason};
use anyhow::{Context, Result};
use icn_gossip::GossipHandle;
use icn_identity::Did;
use icn_store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Key prefix for journal entries in storage
const JOURNAL_PREFIX: &str = "ledger:journal:";

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
}

impl Ledger {
    /// Create a new ledger with the given storage backend
    pub fn new(store: Arc<dyn Store>) -> Result<Self> {
        let quarantine = QuarantineStore::new(store.clone());

        let mut ledger = Ledger {
            store,
            cached_balances: HashMap::new(),
            gossip: None,
            quarantine,
            last_merge: None,
        };

        // Load cached balances from storage
        ledger.load_cached_balances()?;

        Ok(ledger)
    }

    /// Set the gossip handle for distributed synchronization
    pub fn set_gossip(&mut self, gossip: GossipHandle) {
        self.gossip = Some(gossip);
    }

    /// Append a journal entry to the ledger
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

        debug!("Appended journal entry: {}", hash);

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

        info!("Ledger entry appended: {}", hash);

        // Publish to gossip if available
        if let Some(gossip) = &self.gossip {
            self.publish_to_gossip(gossip, &entry)?;
        }

        Ok(hash)
    }

    /// Publish a journal entry to gossip for distributed synchronization
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

            debug!("Published entry {} to topic {}", hash, topic);
        }

        Ok(())
    }

    /// Handle an incoming sync message from gossip
    pub fn handle_sync_message(&mut self, msg: LedgerSyncMessage) -> Result<()> {
        match msg {
            LedgerSyncMessage::NewEntry { hash, mut entry } => {
                // Check if we already have this entry
                if self.get_entry(&hash)?.is_some() {
                    debug!("Already have entry {}, skipping", hash);
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
                        info!("Received and stored entry {} via gossip", h);
                        Ok(())
                    }
                    Err(e) => {
                        warn!("Failed to store received entry: {}", e);
                        Ok(()) // Don't propagate error, just log it
                    }
                }
            }

            LedgerSyncMessage::RequestEntry { hash } => {
                // Handle request for specific entry
                debug!("Received request for entry {}", hash);

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
        info!("Recomputing all balances from journal");

        let entries = self.get_all_entries()?;
        let balances = compute_all_balances(&entries);

        self.cached_balances = balances;
        self.save_cached_balances()?;

        info!("Balance recomputation complete");
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
            warn!("Balance mismatch detected!");
            return Err(anyhow::anyhow!(
                "Cached balances do not match computed balances"
            ));
        }

        info!("Ledger integrity verified: {} entries", entries.len());
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
            let key = format!(
                "{}{}",
                BALANCE_PREFIX,
                serde_json::to_string(account_id)?
            );
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
    fn validate_entry(&self, entry: &JournalEntry) -> Result<()> {
        // Check that entry has at least one account delta
        if entry.accounts.is_empty() {
            anyhow::bail!("Entry has no account deltas");
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
                anyhow::bail!("Currency {} does not balance (sum = {})", currency, sum);
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
}
