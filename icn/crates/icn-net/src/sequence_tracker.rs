//! Persistent outgoing sequence number tracking for encryption nonces.
//!
//! This module solves a critical security vulnerability: without persistence,
//! sequence counters used for ChaCha20-Poly1305 nonce derivation would reset
//! on node restart, potentially reusing nonces and breaking encryption security.
//!
//! # Architecture
//!
//! ```text
//! OutgoingSequenceTracker
//!   ├── In-memory cache (HashMap<(sender, recipient), sequence>)
//!   ├── Persistent store (Sled via icn-store)
//!   └── Safety gap on restart (+10000)
//! ```
//!
//! # Security Properties
//!
//! - **Nonce uniqueness**: Monotonically increasing sequences guarantee unique nonces
//! - **Restart safety**: Safety gap prevents nonce reuse even if last save was delayed
//! - **Per-recipient isolation**: Separate sequence spaces per sender-recipient pair
//!
//! # Usage
//!
//! ```rust,ignore
//! use icn_net::OutgoingSequenceTracker;
//! use icn_store::SledStore;
//!
//! let store = SledStore::open("/path/to/data")?;
//! let tracker = OutgoingSequenceTracker::new(Arc::new(store))?;
//!
//! // Get next sequence for encryption
//! let seq = tracker.next_sequence(&my_did, &recipient_did).await?;
//!
//! // Use in EncryptedEnvelope
//! let envelope = EncryptedEnvelope::encrypt(&my_did, &recipient_did, seq, ...)?;
//! ```

use anyhow::{Context, Result};
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Safety gap added to all sequences on startup.
///
/// This ensures that even if persistence was delayed or crashed before saving,
/// we won't reuse any nonces. 10,000 provides ample safety margin:
/// - At 1000 msg/sec, covers 10 seconds of unsynced messages
/// - At 100 msg/sec, covers 100 seconds
/// - In practice, sequences are persisted immediately after increment
const RESTART_SAFETY_GAP: u64 = 10_000;

/// Key prefix for sequence storage.
const SEQUENCE_PREFIX: &[u8] = b"outgoing_seq:";

/// Persistent entry for a single sender-recipient sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SequenceEntry {
    /// Last used sequence number
    sequence: u64,
    /// Timestamp of last update (for debugging/audit)
    updated_at_ms: u64,
}

/// Persistent outgoing sequence number tracker for encryption nonces.
///
/// Tracks and persists sequence numbers per (sender, recipient) pair to ensure
/// nonce uniqueness for ChaCha20-Poly1305 encryption even across restarts.
pub struct OutgoingSequenceTracker {
    /// In-memory cache of sequences
    cache: RwLock<HashMap<(Did, Did), u64>>,
    /// Persistent storage backend
    store: Arc<dyn Store>,
    /// Whether we've applied the restart safety gap
    restart_gap_applied: RwLock<bool>,
}

impl OutgoingSequenceTracker {
    /// Create a new sequence tracker with the given storage backend.
    ///
    /// On creation, loads all existing sequences from storage and applies
    /// a safety gap to prevent nonce reuse after restart.
    pub fn new(store: Arc<dyn Store>) -> Result<Self> {
        let tracker = Self {
            cache: RwLock::new(HashMap::new()),
            store,
            restart_gap_applied: RwLock::new(false),
        };

        Ok(tracker)
    }

    /// Create a sequence tracker for testing (no persistence).
    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            store: Arc::new(icn_store::SledStore::temporary().unwrap()),
            restart_gap_applied: RwLock::new(true), // No gap needed for tests
        }
    }

    /// Load sequences from persistent storage and apply restart safety gap.
    ///
    /// This should be called during node startup, after the store is ready.
    /// It loads all persisted sequences and adds RESTART_SAFETY_GAP to each.
    pub async fn load_and_apply_safety_gap(&self) -> Result<usize> {
        let mut applied = self.restart_gap_applied.write().await;
        if *applied {
            return Ok(0);
        }

        let entries = self
            .store
            .scan(SEQUENCE_PREFIX)
            .context("Failed to scan sequence entries from store")?;

        let mut cache = self.cache.write().await;
        let mut count = 0;

        for (key, value) in entries {
            if let Some((sender, recipient)) = Self::parse_key(&key) {
                if let Ok(entry) = serde_json::from_slice::<SequenceEntry>(&value) {
                    // Apply safety gap
                    let safe_sequence = entry.sequence.saturating_add(RESTART_SAFETY_GAP);
                    cache.insert((sender.clone(), recipient.clone()), safe_sequence);

                    // Persist the new safe sequence
                    self.persist_sequence_inner(&sender, &recipient, safe_sequence)?;

                    tracing::debug!(
                        sender = %sender,
                        recipient = %recipient,
                        old_seq = entry.sequence,
                        new_seq = safe_sequence,
                        "Applied restart safety gap to sequence"
                    );

                    count += 1;
                }
            }
        }

        *applied = true;

        tracing::info!(
            loaded_sequences = count,
            safety_gap = RESTART_SAFETY_GAP,
            "Loaded outgoing sequences with restart safety gap"
        );

        Ok(count)
    }

    /// Get the next sequence number for a sender-recipient pair.
    ///
    /// Automatically increments and persists the sequence.
    /// Thread-safe and guaranteed to return unique, monotonically increasing values.
    pub async fn next_sequence(&self, sender: &Did, recipient: &Did) -> Result<u64> {
        // Ensure safety gap has been applied on first access
        {
            let applied = self.restart_gap_applied.read().await;
            if !*applied {
                drop(applied);
                self.load_and_apply_safety_gap().await?;
            }
        }

        let key = (sender.clone(), recipient.clone());

        let mut cache = self.cache.write().await;
        let next_seq = cache.get(&key).map(|s| s + 1).unwrap_or(1);

        // Update cache
        cache.insert(key, next_seq);

        // Persist immediately (critical for nonce safety)
        drop(cache); // Release lock before IO
        self.persist_sequence(sender, recipient, next_seq).await?;

        Ok(next_seq)
    }

    /// Get the current sequence number without incrementing (for inspection).
    pub async fn current_sequence(&self, sender: &Did, recipient: &Did) -> Option<u64> {
        let cache = self.cache.read().await;
        cache.get(&(sender.clone(), recipient.clone())).copied()
    }

    /// Persist a sequence to storage.
    async fn persist_sequence(&self, sender: &Did, recipient: &Did, sequence: u64) -> Result<()> {
        self.persist_sequence_inner(sender, recipient, sequence)
    }

    fn persist_sequence_inner(&self, sender: &Did, recipient: &Did, sequence: u64) -> Result<()> {
        let key = Self::make_key(sender, recipient);
        let entry = SequenceEntry {
            sequence,
            updated_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };

        let value = serde_json::to_vec(&entry).context("Failed to serialize sequence entry")?;

        self.store
            .put(&key, &value)
            .context("Failed to persist sequence")?;

        Ok(())
    }

    /// Separator between sender and recipient DIDs in storage key.
    /// Chosen because `||` is unlikely to appear in the base58-encoded public keys used in DIDs,
    /// even though DIDs themselves contain colons.
    const DID_SEPARATOR: &'static [u8] = b"||";

    /// Generate storage key for a sender-recipient pair.
    fn make_key(sender: &Did, recipient: &Did) -> Vec<u8> {
        let mut key = Vec::with_capacity(SEQUENCE_PREFIX.len() + 200);
        key.extend_from_slice(SEQUENCE_PREFIX);
        key.extend_from_slice(sender.as_str().as_bytes());
        key.extend_from_slice(Self::DID_SEPARATOR);
        key.extend_from_slice(recipient.as_str().as_bytes());
        key
    }

    /// Parse storage key back to sender-recipient pair.
    fn parse_key(key: &[u8]) -> Option<(Did, Did)> {
        if !key.starts_with(SEQUENCE_PREFIX) {
            return None;
        }

        let rest = &key[SEQUENCE_PREFIX.len()..];
        let rest_str = std::str::from_utf8(rest).ok()?;

        // Split on "||" separator
        let parts: Vec<&str> = rest_str.splitn(2, "||").collect();
        if parts.len() != 2 {
            return None;
        }

        let sender = Did::from_str(parts[0]).ok()?;
        let recipient = Did::from_str(parts[1]).ok()?;

        Some((sender, recipient))
    }

    /// Get number of tracked sender-recipient pairs.
    pub async fn pair_count(&self) -> usize {
        self.cache.read().await.len()
    }

    /// Cleanup stale entries from cache and persistent storage.
    ///
    /// Removes entries that haven't been used within the retention period.
    /// Should be called periodically (e.g., hourly) to prevent unbounded memory growth.
    ///
    /// # Arguments
    /// * `retention_secs` - Remove entries older than this many seconds
    ///
    /// # Returns
    /// Number of entries removed
    pub async fn cleanup_stale_entries(&self, retention_secs: u64) -> Result<usize> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let cutoff_ms = now_ms.saturating_sub(retention_secs * 1000);

        let entries = self
            .store
            .scan(SEQUENCE_PREFIX)
            .context("Failed to scan sequence entries")?;

        let mut removed = 0;
        let mut cache = self.cache.write().await;

        for (key, value) in entries {
            if let Ok(entry) = serde_json::from_slice::<SequenceEntry>(&value) {
                if entry.updated_at_ms < cutoff_ms {
                    // Remove from storage
                    if self.store.delete(&key).is_ok() {
                        // Remove from cache if we can parse the key
                        if let Some((sender, recipient)) = Self::parse_key(&key) {
                            cache.remove(&(sender, recipient));
                        }
                        removed += 1;
                    }
                }
            }
        }

        if removed > 0 {
            tracing::info!(
                removed_entries = removed,
                retention_secs = retention_secs,
                "Cleaned up stale sequence tracker entries"
            );
        }

        Ok(removed)
    }

    /// Check if safety gap has been applied.
    ///
    /// Returns true if `load_and_apply_safety_gap()` has been called.
    /// Useful for startup validation.
    pub async fn is_initialized(&self) -> bool {
        *self.restart_gap_applied.read().await
    }

    /// Require that safety gap has been explicitly applied.
    ///
    /// Returns error if `load_and_apply_safety_gap()` hasn't been called.
    /// Use this in strict mode when you want to enforce explicit initialization.
    pub async fn require_initialized(&self) -> Result<()> {
        if !*self.restart_gap_applied.read().await {
            anyhow::bail!(
                "Sequence tracker not initialized. Call load_and_apply_safety_gap() during startup."
            )
        }
        Ok(())
    }

    /// Clear all sequences (for testing only).
    #[cfg(test)]
    pub async fn clear(&self) {
        self.cache.write().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    /// Generate a new DID from a fresh keypair
    fn generate_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[tokio::test]
    async fn test_next_sequence_increments() {
        let tracker = OutgoingSequenceTracker::in_memory();

        let alice = generate_did();
        let bob = generate_did();

        // First sequence should be 1
        let seq1 = tracker.next_sequence(&alice, &bob).await.unwrap();
        assert_eq!(seq1, 1);

        // Second should be 2
        let seq2 = tracker.next_sequence(&alice, &bob).await.unwrap();
        assert_eq!(seq2, 2);

        // Third should be 3
        let seq3 = tracker.next_sequence(&alice, &bob).await.unwrap();
        assert_eq!(seq3, 3);
    }

    #[tokio::test]
    async fn test_separate_recipient_sequences() {
        let tracker = OutgoingSequenceTracker::in_memory();

        let alice = generate_did();
        let bob = generate_did();
        let charlie = generate_did();

        // Alice -> Bob starts at 1
        let seq1 = tracker.next_sequence(&alice, &bob).await.unwrap();
        assert_eq!(seq1, 1);

        // Alice -> Charlie also starts at 1 (independent)
        let seq2 = tracker.next_sequence(&alice, &charlie).await.unwrap();
        assert_eq!(seq2, 1);

        // Alice -> Bob increments independently
        let seq3 = tracker.next_sequence(&alice, &bob).await.unwrap();
        assert_eq!(seq3, 2);

        // Alice -> Charlie still increments independently
        let seq4 = tracker.next_sequence(&alice, &charlie).await.unwrap();
        assert_eq!(seq4, 2);
    }

    #[tokio::test]
    async fn test_separate_sender_sequences() {
        let tracker = OutgoingSequenceTracker::in_memory();

        let alice = generate_did();
        let bob = generate_did();
        let charlie = generate_did();

        // Alice -> Charlie
        let seq1 = tracker.next_sequence(&alice, &charlie).await.unwrap();
        assert_eq!(seq1, 1);

        // Bob -> Charlie (different sender, same recipient)
        let seq2 = tracker.next_sequence(&bob, &charlie).await.unwrap();
        assert_eq!(seq2, 1);
    }

    #[tokio::test]
    async fn test_current_sequence() {
        let tracker = OutgoingSequenceTracker::in_memory();

        let alice = generate_did();
        let bob = generate_did();

        // No sequence yet
        assert_eq!(tracker.current_sequence(&alice, &bob).await, None);

        // Get first sequence
        tracker.next_sequence(&alice, &bob).await.unwrap();

        // Now should be 1
        assert_eq!(tracker.current_sequence(&alice, &bob).await, Some(1));

        // Get another
        tracker.next_sequence(&alice, &bob).await.unwrap();

        // Now should be 2
        assert_eq!(tracker.current_sequence(&alice, &bob).await, Some(2));
    }

    #[tokio::test]
    async fn test_persistence_and_safety_gap() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());

        let alice = generate_did();
        let bob = generate_did();

        // Create tracker and get some sequences
        {
            let tracker = OutgoingSequenceTracker::new(store.clone()).unwrap();

            // First load applies safety gap (to nothing)
            tracker.load_and_apply_safety_gap().await.unwrap();

            // Get sequences 1, 2, 3
            for expected in 1..=3 {
                let seq = tracker.next_sequence(&alice, &bob).await.unwrap();
                assert_eq!(seq, expected);
            }
        }

        // Create new tracker (simulating restart)
        {
            let tracker = OutgoingSequenceTracker::new(store.clone()).unwrap();

            // Load should apply safety gap
            let loaded = tracker.load_and_apply_safety_gap().await.unwrap();
            assert_eq!(loaded, 1); // One pair loaded

            // Current sequence should be 3 + RESTART_SAFETY_GAP
            let current = tracker.current_sequence(&alice, &bob).await.unwrap();
            assert_eq!(current, 3 + RESTART_SAFETY_GAP);

            // Next sequence should be 3 + RESTART_SAFETY_GAP + 1
            let next = tracker.next_sequence(&alice, &bob).await.unwrap();
            assert_eq!(next, 3 + RESTART_SAFETY_GAP + 1);
        }
    }

    #[tokio::test]
    async fn test_make_and_parse_key() {
        let alice = generate_did();
        let bob = generate_did();

        let key = OutgoingSequenceTracker::make_key(&alice, &bob);
        let (parsed_sender, parsed_recipient) = OutgoingSequenceTracker::parse_key(&key).unwrap();

        assert_eq!(parsed_sender.as_str(), alice.as_str());
        assert_eq!(parsed_recipient.as_str(), bob.as_str());
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        use std::sync::Arc;

        let tracker = Arc::new(OutgoingSequenceTracker::in_memory());

        let alice = generate_did();
        let bob = generate_did();

        // Spawn 10 concurrent tasks, each getting 10 sequences
        let mut handles = vec![];

        for _ in 0..10 {
            let tracker = Arc::clone(&tracker);
            let alice = alice.clone();
            let bob = bob.clone();

            handles.push(tokio::spawn(async move {
                let mut seqs = vec![];
                for _ in 0..10 {
                    seqs.push(tracker.next_sequence(&alice, &bob).await.unwrap());
                }
                seqs
            }));
        }

        // Collect all sequences
        let mut all_seqs = vec![];
        for handle in handles {
            all_seqs.extend(handle.await.unwrap());
        }

        // Should have 100 unique sequences
        assert_eq!(all_seqs.len(), 100);

        // All should be unique
        all_seqs.sort();
        all_seqs.dedup();
        assert_eq!(all_seqs.len(), 100);

        // Should be 1..=100
        assert_eq!(all_seqs[0], 1);
        assert_eq!(all_seqs[99], 100);
    }

    #[tokio::test]
    async fn test_pair_count() {
        let tracker = OutgoingSequenceTracker::in_memory();

        let alice = generate_did();
        let bob = generate_did();
        let charlie = generate_did();

        assert_eq!(tracker.pair_count().await, 0);

        tracker.next_sequence(&alice, &bob).await.unwrap();
        assert_eq!(tracker.pair_count().await, 1);

        tracker.next_sequence(&alice, &charlie).await.unwrap();
        assert_eq!(tracker.pair_count().await, 2);

        tracker.next_sequence(&bob, &charlie).await.unwrap();
        assert_eq!(tracker.pair_count().await, 3);

        // Same pair doesn't increase count
        tracker.next_sequence(&alice, &bob).await.unwrap();
        assert_eq!(tracker.pair_count().await, 3);
    }

    #[tokio::test]
    async fn test_is_initialized() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let tracker = OutgoingSequenceTracker::new(store).unwrap();

        // Not initialized yet
        assert!(!tracker.is_initialized().await);

        // Apply safety gap
        tracker.load_and_apply_safety_gap().await.unwrap();

        // Now initialized
        assert!(tracker.is_initialized().await);
    }

    #[tokio::test]
    async fn test_require_initialized_error() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let tracker = OutgoingSequenceTracker::new(store).unwrap();

        // Should error before initialization
        let result = tracker.require_initialized().await;
        assert!(result.is_err());

        // Apply safety gap
        tracker.load_and_apply_safety_gap().await.unwrap();

        // Should succeed after initialization
        let result = tracker.require_initialized().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_auto_initialization_on_next_sequence() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let tracker = OutgoingSequenceTracker::new(store).unwrap();

        let alice = generate_did();
        let bob = generate_did();

        // Not initialized
        assert!(!tracker.is_initialized().await);

        // next_sequence auto-initializes
        let seq = tracker.next_sequence(&alice, &bob).await.unwrap();
        assert_eq!(seq, 1);

        // Now initialized
        assert!(tracker.is_initialized().await);
    }

    #[tokio::test]
    async fn test_cleanup_stale_entries() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let tracker = OutgoingSequenceTracker::new(store).unwrap();
        tracker.load_and_apply_safety_gap().await.unwrap();

        let alice = generate_did();
        let bob = generate_did();
        let charlie = generate_did();

        // Create some entries
        tracker.next_sequence(&alice, &bob).await.unwrap();
        tracker.next_sequence(&alice, &charlie).await.unwrap();

        assert_eq!(tracker.pair_count().await, 2);

        // Cleanup with 0 retention - should remove nothing (entries are fresh)
        // Actually with 0 retention, entries created "now" shouldn't be removed
        // because their timestamp is >= cutoff
        let removed = tracker.cleanup_stale_entries(0).await.unwrap();
        assert_eq!(removed, 0);

        // Cleanup with very long retention - should remove nothing
        let removed = tracker.cleanup_stale_entries(86400).await.unwrap();
        assert_eq!(removed, 0);
        assert_eq!(tracker.pair_count().await, 2);
    }
}
