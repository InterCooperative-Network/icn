//! Ledger implementation with storage

use crate::balance::{compute_all_balances, compute_account_balance, compute_balance};
use crate::types::{AccountBalances, ContentHash, JournalEntry};
use anyhow::{Context, Result};
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
}

impl Ledger {
    /// Create a new ledger with the given storage backend
    pub fn new(store: Arc<dyn Store>) -> Result<Self> {
        let mut ledger = Ledger {
            store,
            cached_balances: HashMap::new(),
        };

        // Load cached balances from storage
        ledger.load_cached_balances()?;

        Ok(ledger)
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

        Ok(hash)
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
}
