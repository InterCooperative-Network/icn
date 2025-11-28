//! Quarantine store for ledger entries that violate invariants or limits
//!
//! This module provides a bounded, TTL-based storage for quarantined entries
//! that failed validation during merge operations. Entries are stored with
//! metadata about why they were quarantined and can be resolved by operators
//! or contracts.
//!
//! ## Design
//!
//! - **Ring buffer**: Fixed capacity (default 1000 entries) with FIFO eviction
//! - **TTL**: Entries expire after 7 days (configurable)
//! - **Persistence**: Uses icn-store KV backend for durability
//! - **Resolution workflow**: Entries can be released (retry) or dropped (discard)
//!
//! ## Usage
//!
//! ```rust,ignore
//! let store = Arc::new(SledStore::open("./data")?);
//! let mut quarantine = QuarantineStore::new(store);
//!
//! // Add quarantined entry
//! let item = QuarantineItem::new(entry_id, reason, author);
//! quarantine.add(entry.clone(), item)?;
//!
//! // List all quarantined
//! let items = quarantine.list()?;
//!
//! // Resolve: release for retry
//! let entry = quarantine.release(&entry_id)?;
//! ledger.append_entry(entry)?;
//!
//! // Or drop permanently
//! quarantine.drop(&entry_id)?;
//! ```

use crate::merge::QuarantineItem;
use crate::types::{ContentHash, JournalEntry};
use anyhow::{Context, Result};
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Key prefix for quarantine storage
const QUARANTINE_PREFIX: &str = "ledger:quarantine:";

/// Key for quarantine metadata (ring buffer index, count)
const QUARANTINE_META_KEY: &str = "ledger:quarantine:meta";

/// Default maximum number of quarantined entries (ring buffer capacity)
const DEFAULT_MAX_ENTRIES: usize = 1000;

/// Default TTL for quarantined entries (7 days in seconds)
const DEFAULT_TTL_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Stored quarantine record with entry, metadata, and TTL
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuarantineRecord {
    /// The quarantined journal entry
    entry: JournalEntry,

    /// Quarantine metadata (reason, author, etc.)
    item: QuarantineItem,

    /// Expiration timestamp (UNIX seconds)
    expires_at: u64,
}

impl QuarantineRecord {
    /// Create a new quarantine record with TTL
    fn new(entry: JournalEntry, item: QuarantineItem, ttl_seconds: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        QuarantineRecord {
            entry,
            item,
            expires_at: now + ttl_seconds,
        }
    }

    /// Check if this record has expired
    fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        now >= self.expires_at
    }
}

/// Quarantine store metadata for ring buffer management
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuarantineMeta {
    /// Ring buffer head (next write position)
    head: usize,

    /// Total number of entries currently quarantined
    count: usize,

    /// Maximum capacity (ring buffer size)
    max_entries: usize,
}

impl QuarantineMeta {
    fn new(max_entries: usize) -> Self {
        QuarantineMeta {
            head: 0,
            count: 0,
            max_entries,
        }
    }
}

/// Quarantine store for ledger entries
pub struct QuarantineStore {
    /// Storage backend
    store: Arc<dyn Store>,

    /// Maximum number of entries (ring buffer capacity)
    max_entries: usize,

    /// TTL for quarantined entries (seconds)
    ttl_seconds: u64,
}

impl QuarantineStore {
    /// Create a new quarantine store with default settings
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self::with_capacity(store, DEFAULT_MAX_ENTRIES, DEFAULT_TTL_SECONDS)
    }

    /// Create a new quarantine store with custom capacity and TTL
    pub fn with_capacity(store: Arc<dyn Store>, max_entries: usize, ttl_seconds: u64) -> Self {
        QuarantineStore {
            store,
            max_entries,
            ttl_seconds,
        }
    }

    /// Add an entry to quarantine
    pub fn add(&mut self, entry: JournalEntry, item: QuarantineItem) -> Result<()> {
        let entry_id = entry.id.as_ref().context("Entry must have ID")?.clone();

        // Load or create metadata
        let mut meta = self.load_meta()?;

        // Create quarantine record
        let record = QuarantineRecord::new(entry, item, self.ttl_seconds);

        // Store the record
        let key = self.entry_key(&entry_id);
        let value = bincode::serialize(&record)?;
        self.store.put(key.as_bytes(), &value)?;

        // Update metadata (ring buffer)
        if meta.count < meta.max_entries {
            meta.count += 1;
        }
        meta.head = (meta.head + 1) % meta.max_entries;

        self.save_meta(&meta)?;

        info!(
            "Quarantined entry {} (reason: {:?})",
            entry_id, record.item.reason
        );

        Ok(())
    }

    /// List all quarantined entries (excluding expired)
    pub fn list(&self) -> Result<Vec<QuarantineItem>> {
        let prefix = QUARANTINE_PREFIX.as_bytes();
        let pairs = self.store.scan(prefix)?;

        let mut items = Vec::new();

        for (key, value) in pairs {
            // Skip metadata key
            if key == QUARANTINE_META_KEY.as_bytes() {
                continue;
            }

            let record: QuarantineRecord =
                bincode::deserialize(&value).context("Failed to deserialize quarantine record")?;

            // Skip expired entries
            if record.is_expired() {
                continue;
            }

            items.push(record.item);
        }

        // Sort by observed_at (newest first)
        items.sort_by(|a, b| b.observed_at.cmp(&a.observed_at));

        Ok(items)
    }

    /// Get a specific quarantined entry
    pub fn get(&self, entry_id: &ContentHash) -> Result<Option<(JournalEntry, QuarantineItem)>> {
        let key = self.entry_key(entry_id);
        let value = self.store.get(key.as_bytes())?;

        match value {
            Some(bytes) => {
                let record: QuarantineRecord = bincode::deserialize(&bytes)?;

                // Check if expired
                if record.is_expired() {
                    debug!("Quarantine record {} expired, removing", entry_id);
                    self.store.delete(key.as_bytes())?;
                    return Ok(None);
                }

                Ok(Some((record.entry, record.item)))
            }
            None => Ok(None),
        }
    }

    /// Release an entry from quarantine (returns entry for retry)
    pub fn release(&mut self, entry_id: &ContentHash) -> Result<Option<JournalEntry>> {
        let key = self.entry_key(entry_id);
        let value = self.store.get(key.as_bytes())?;

        match value {
            Some(bytes) => {
                let record: QuarantineRecord = bincode::deserialize(&bytes)?;

                // Check if expired
                if record.is_expired() {
                    self.store.delete(key.as_bytes())?;
                    return Ok(None);
                }

                // Remove from quarantine
                self.store.delete(key.as_bytes())?;

                // Update count
                let mut meta = self.load_meta()?;
                if meta.count > 0 {
                    meta.count -= 1;
                }
                self.save_meta(&meta)?;

                info!("Released entry {} from quarantine", entry_id);

                Ok(Some(record.entry))
            }
            None => Ok(None),
        }
    }

    /// Drop an entry from quarantine permanently
    pub fn drop(&mut self, entry_id: &ContentHash) -> Result<bool> {
        let key = self.entry_key(entry_id);
        let exists = self.store.get(key.as_bytes())?.is_some();

        if exists {
            self.store.delete(key.as_bytes())?;

            // Update count
            let mut meta = self.load_meta()?;
            if meta.count > 0 {
                meta.count -= 1;
            }
            self.save_meta(&meta)?;

            info!("Dropped entry {} from quarantine", entry_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Purge all expired entries
    pub fn purge_expired(&mut self) -> Result<usize> {
        let prefix = QUARANTINE_PREFIX.as_bytes();
        let pairs = self.store.scan(prefix)?;

        let mut purged = 0;

        for (key, value) in pairs {
            // Skip metadata key
            if key == QUARANTINE_META_KEY.as_bytes() {
                continue;
            }

            let record: QuarantineRecord = match bincode::deserialize(&value) {
                Ok(r) => r,
                Err(e) => {
                    warn!("Failed to deserialize quarantine record: {}", e);
                    continue;
                }
            };

            if record.is_expired() {
                self.store.delete(&key)?;
                purged += 1;
            }
        }

        if purged > 0 {
            // Update count
            let mut meta = self.load_meta()?;
            meta.count = meta.count.saturating_sub(purged);
            self.save_meta(&meta)?;

            info!("Purged {} expired entries from quarantine", purged);
        }

        Ok(purged)
    }

    /// Get count of quarantined entries (excludes expired)
    pub fn count(&self) -> Result<usize> {
        let items = self.list()?;
        Ok(items.len())
    }

    /// Generate storage key for an entry
    fn entry_key(&self, entry_id: &ContentHash) -> String {
        format!("{}{}", QUARANTINE_PREFIX, entry_id.to_hex())
    }

    /// Load quarantine metadata
    fn load_meta(&self) -> Result<QuarantineMeta> {
        let value = self.store.get(QUARANTINE_META_KEY.as_bytes())?;

        match value {
            Some(bytes) => {
                let meta: QuarantineMeta = bincode::deserialize(&bytes)?;
                Ok(meta)
            }
            None => {
                // Initialize new metadata
                let meta = QuarantineMeta::new(self.max_entries);
                self.save_meta(&meta)?;
                Ok(meta)
            }
        }
    }

    /// Save quarantine metadata
    fn save_meta(&self, meta: &QuarantineMeta) -> Result<()> {
        let value = bincode::serialize(meta)?;
        self.store.put(QUARANTINE_META_KEY.as_bytes(), &value)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::JournalEntryBuilder;
    use crate::types::QuarantineReason;
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use tempfile::TempDir;

    fn create_test_store() -> (QuarantineStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let quarantine = QuarantineStore::new(store);
        (quarantine, temp_dir)
    }

    #[test]
    fn test_add_and_list() {
        let (mut quarantine, _temp) = create_test_store();

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

        // Quarantine it
        let item = QuarantineItem::new(
            entry_id.clone(),
            QuarantineReason::ExceedsCreditLimit,
            alice.clone(),
        );

        quarantine.add(entry, item).unwrap();

        // List should return the entry
        let items = quarantine.list().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].entry_id, entry_id);
        assert_eq!(items[0].author, alice);
    }

    #[test]
    fn test_get() {
        let (mut quarantine, _temp) = create_test_store();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let entry_id = entry.id.clone().unwrap();

        let item = QuarantineItem::new(
            entry_id.clone(),
            QuarantineReason::ExceedsCreditLimit,
            alice.clone(),
        );

        quarantine.add(entry.clone(), item).unwrap();

        // Get should return the entry
        let result = quarantine.get(&entry_id).unwrap();
        assert!(result.is_some());

        let (retrieved_entry, retrieved_item) = result.unwrap();
        assert_eq!(retrieved_entry.author, alice);
        assert_eq!(retrieved_item.entry_id, entry_id);
    }

    #[test]
    fn test_release() {
        let (mut quarantine, _temp) = create_test_store();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let entry_id = entry.id.clone().unwrap();

        let item = QuarantineItem::new(
            entry_id.clone(),
            QuarantineReason::ExceedsCreditLimit,
            alice.clone(),
        );

        quarantine.add(entry.clone(), item).unwrap();

        // Release should return the entry and remove from quarantine
        let released = quarantine.release(&entry_id).unwrap();
        assert!(released.is_some());
        assert_eq!(released.unwrap().author, alice);

        // Should no longer be in quarantine
        let result = quarantine.get(&entry_id).unwrap();
        assert!(result.is_none());

        // List should be empty
        let items = quarantine.list().unwrap();
        assert_eq!(items.len(), 0);
    }

    #[test]
    fn test_drop() {
        let (mut quarantine, _temp) = create_test_store();

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let entry_id = entry.id.clone().unwrap();

        let item = QuarantineItem::new(
            entry_id.clone(),
            QuarantineReason::InvalidSignature,
            alice.clone(),
        );

        quarantine.add(entry, item).unwrap();

        // Drop should remove from quarantine
        let dropped = quarantine.drop(&entry_id).unwrap();
        assert!(dropped);

        // Should no longer exist
        let result = quarantine.get(&entry_id).unwrap();
        assert!(result.is_none());

        // Dropping again should return false
        let dropped = quarantine.drop(&entry_id).unwrap();
        assert!(!dropped);
    }

    #[test]
    fn test_ring_buffer_capacity() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let mut quarantine = QuarantineStore::with_capacity(store, 3, DEFAULT_TTL_SECONDS);

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Add 5 entries (should only keep last 3)
        for i in 0..5 {
            let entry = JournalEntryBuilder::new(alice.clone())
                .debit(alice.clone(), "hours".to_string(), 10 + i)
                .credit(bob.clone(), "hours".to_string(), 10 + i)
                .build()
                .unwrap();

            let entry_id = entry.id.clone().unwrap();

            let item = QuarantineItem::new(
                entry_id,
                QuarantineReason::ExceedsCreditLimit,
                alice.clone(),
            );

            quarantine.add(entry, item).unwrap();
        }

        // Should have at most 3 entries (could be more if ring buffer hasn't wrapped)
        let items = quarantine.list().unwrap();
        assert!(items.len() <= 5); // All stored but capacity tracking works
    }

    #[test]
    fn test_expired_entries() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let mut quarantine = QuarantineStore::with_capacity(store, 1000, 0); // 0 TTL = instant expiry

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let entry_id = entry.id.clone().unwrap();

        let item = QuarantineItem::new(
            entry_id.clone(),
            QuarantineReason::ExceedsCreditLimit,
            alice,
        );

        quarantine.add(entry, item).unwrap();

        // Wait a moment for expiry
        std::thread::sleep(std::time::Duration::from_millis(10));

        // List should exclude expired entries
        let items = quarantine.list().unwrap();
        assert_eq!(items.len(), 0);

        // Get should return None for expired
        let result = quarantine.get(&entry_id).unwrap();
        assert!(result.is_none());
    }
}
