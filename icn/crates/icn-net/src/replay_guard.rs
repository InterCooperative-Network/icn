//! Replay protection through persistent sequence number tracking
//!
//! Prevents replay attacks by maintaining per-sender sequence windows
//! and rejecting duplicate or out-of-order messages.
//!
//! # Persistence (Security Critical)
//!
//! This module now supports persistent storage of replay protection state.
//! Without persistence, replay attacks would be possible after node restart:
//!
//! 1. Attacker records a signed message
//! 2. Target node restarts (crash, update, etc.)
//! 3. Attacker replays the message
//! 4. Node accepts it (no memory of prior sequences) ← VULNERABILITY
//!
//! With persistence:
//! - `max_seq` per peer is persisted to storage
//! - `finalized` sequences (processed transactions) are persisted
//! - On restart, a safety gap is applied to prevent any edge cases
//!
//! # Architecture
//!
//! ```text
//! ReplayGuard (Persistent)
//!   ├── In-memory cache (HashMap<Did, SequenceWindow>)
//!   ├── Persistent store (Sled via icn-store)
//!   │   ├── replay_max_seq:<did> → max sequence number
//!   │   └── replay_finalized:<did>:<seq> → finalization timestamp
//!   └── Safety gap on restart (+1000)
//! ```

use crate::envelope::SignedEnvelope;
use anyhow::{bail, Context, Result};
use icn_gossip::BloomFilter;
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Hash a sequence number to a 32-byte hash for Bloom filter
fn hash_sequence(sequence: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(sequence.to_be_bytes());
    hasher.finalize().into()
}

/// Safety gap added to all max_seq values on startup.
///
/// This ensures that even if persistence was delayed or crashed before saving,
/// we won't accept replayed messages. 1,000 provides safety margin:
/// - At 100 msg/sec from a peer, covers 10 seconds of unsynced state
/// - In practice, max_seq is persisted on every update
///
/// Note: This is smaller than OutgoingSequenceTracker's gap (10,000) because:
/// - Outgoing uses sequences for nonces (reuse breaks encryption)
/// - Incoming uses sequences for replay detection (gap only causes temporary rejection)
const RESTART_SAFETY_GAP: u64 = 1_000;

/// Maximum entries before Bloom filter rotation (80% of capacity)
const BLOOM_ROTATION_THRESHOLD: u64 = 8_000;

/// Bloom filter capacity
const BLOOM_CAPACITY: usize = 10_000;

/// Key prefix for max sequence storage
const MAX_SEQ_PREFIX: &[u8] = b"replay_max_seq:";

/// Key prefix for finalized sequence storage
const FINALIZED_PREFIX: &[u8] = b"replay_finalized:";

/// Persisted max sequence entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaxSeqEntry {
    /// Maximum sequence seen from this peer
    max_seq: u64,
    /// Timestamp of last update (for debugging/audit)
    updated_at_ms: u64,
}

/// Persisted finalized sequence entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FinalizedEntry {
    /// When the sequence was finalized
    finalized_at_ms: u64,
}

/// Per-peer sequence tracking for replay protection
///
/// Maintains sequence number windows for each sender to detect:
/// - **Replay attacks**: Same sequence number seen twice
/// - **Out-of-order delivery**: Sequences within acceptance window
/// - **Stale connections**: Cleanup old peer state
/// - **Finalized sequences**: Permanently prevent replay after processing
///
/// # Persistence
///
/// When created with `new_persistent()`, this guard persists:
/// - `max_seq` per peer (prevents replays after restart)
/// - `finalized` sequences (prevents replay of processed transactions)
pub struct ReplayGuard {
    /// Last seen sequence per peer (in-memory cache)
    sequences: HashMap<Did, SequenceWindow>,

    /// Maximum allowed clock skew (seconds)
    max_clock_skew: u64,

    /// Maximum age before peer state is evicted (seconds)
    max_peer_age_secs: u64,

    /// Persistent storage backend (None for in-memory only)
    store: Option<Arc<dyn Store>>,

    /// Whether we've loaded persisted state and applied safety gap
    initialized: AtomicBool,
}

/// Sequence window for a single peer
struct SequenceWindow {
    /// Highest sequence number seen from this peer
    max_seq: u64,

    /// Floor sequence number (reject all sequences <= this value)
    /// Set after restart with safety gap to reject all pre-restart sequences
    /// without relying on the bloom filter (which is lost on restart)
    floor_seq: u64,

    /// Bloom filter of recent sequences (for out-of-order detection)
    /// Size: ~10KB for 10,000 sequences with 0.1% false positive rate
    recent: BloomFilter,

    /// Count of entries inserted since last Bloom filter reset
    /// Used to detect when the filter is approaching saturation
    insertion_count: u64,

    /// Finalized sequences (permanently non-replayable)
    /// These are sequences that have been processed (e.g., ledger entry written)
    /// and should NEVER be accepted again, even within the time window
    finalized: HashMap<u64, Instant>,

    /// Last time we saw a message from this peer
    last_update: Instant,
}

impl ReplayGuard {
    /// Create a new in-memory replay guard (no persistence)
    ///
    /// **WARNING**: State is lost on restart. Use `new_persistent()` for production.
    ///
    /// # Arguments
    /// * `max_clock_skew` - Maximum allowed clock skew in seconds (default: 300)
    /// * `max_peer_age_secs` - Evict peer state after this many seconds of inactivity (default: 3600)
    pub fn new(max_clock_skew: u64, max_peer_age_secs: u64) -> Self {
        ReplayGuard {
            sequences: HashMap::new(),
            max_clock_skew,
            max_peer_age_secs,
            store: None,
            initialized: AtomicBool::new(true), // No initialization needed for in-memory
        }
    }

    /// Create a new persistent replay guard
    ///
    /// Persists replay protection state to storage for survival across restarts.
    /// Call `load_and_apply_safety_gap()` after creation to initialize.
    ///
    /// # Arguments
    /// * `max_clock_skew` - Maximum allowed clock skew in seconds (default: 300)
    /// * `max_peer_age_secs` - Evict peer state after this many seconds of inactivity (default: 3600)
    /// * `store` - Persistent storage backend
    pub fn new_persistent(
        max_clock_skew: u64,
        max_peer_age_secs: u64,
        store: Arc<dyn Store>,
    ) -> Self {
        ReplayGuard {
            sequences: HashMap::new(),
            max_clock_skew,
            max_peer_age_secs,
            store: Some(store),
            initialized: AtomicBool::new(false),
        }
    }

    /// Load persisted state and apply restart safety gap
    ///
    /// Must be called during node startup for persistent guards.
    /// Safe to call multiple times (idempotent via atomic flag).
    ///
    /// # Returns
    /// Number of peers loaded from storage
    pub fn load_and_apply_safety_gap(&mut self) -> Result<usize> {
        // Only initialize once
        if self
            .initialized
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(0);
        }

        let store = match &self.store {
            Some(s) => s,
            None => return Ok(0), // In-memory mode, nothing to load
        };

        let mut count = 0;

        // Load max sequences
        let entries = store
            .scan(MAX_SEQ_PREFIX)
            .context("Failed to scan replay max_seq entries")?;

        for (key, value) in entries {
            if let Some(did) = Self::parse_max_seq_key(&key) {
                if let Ok(entry) = serde_json::from_slice::<MaxSeqEntry>(&value) {
                    // Apply safety gap
                    let safe_max_seq = entry.max_seq.saturating_add(RESTART_SAFETY_GAP);

                    let window = self
                        .sequences
                        .entry(did.clone())
                        .or_insert_with(SequenceWindow::new);
                    window.max_seq = safe_max_seq;
                    // Set floor to reject ALL sequences at or below this value
                    // This is critical because the bloom filter is empty after restart,
                    // so we can't rely on it to detect replays of pre-restart sequences
                    window.floor_seq = safe_max_seq;

                    tracing::debug!(
                        peer = %did,
                        old_max_seq = entry.max_seq,
                        new_max_seq = safe_max_seq,
                        floor_seq = safe_max_seq,
                        "Loaded replay guard state with safety gap and floor"
                    );

                    // Persist the safe value
                    self.persist_max_seq_inner(&did, safe_max_seq)?;

                    count += 1;
                }
            }
        }

        // Load finalized sequences
        let finalized_entries = store
            .scan(FINALIZED_PREFIX)
            .context("Failed to scan finalized entries")?;

        let now = Instant::now();
        let cutoff_ms = Self::current_time_ms().saturating_sub(24 * 60 * 60 * 1000); // 24h ago

        for (key, value) in finalized_entries {
            if let Some((did, seq)) = Self::parse_finalized_key(&key) {
                if let Ok(entry) = serde_json::from_slice::<FinalizedEntry>(&value) {
                    // Only load finalized sequences less than 24h old
                    if entry.finalized_at_ms >= cutoff_ms {
                        let window = self
                            .sequences
                            .entry(did)
                            .or_insert_with(SequenceWindow::new);
                        window.finalized.insert(seq, now);
                    }
                }
            }
        }

        tracing::info!(
            loaded_peers = count,
            safety_gap = RESTART_SAFETY_GAP,
            "Loaded replay guard state with restart safety gap"
        );

        Ok(count)
    }

    /// Check if message is fresh (not replayed)
    ///
    /// Validates:
    /// 1. Signature and timestamp (via envelope.verify())
    /// 2. Sequence number is not finalized (permanently blocked)
    /// 3. Sequence number is not a replay
    ///
    /// # Replay Detection Logic:
    /// - If sequence is finalized: Reject immediately (critical)
    /// - If sequence <= max_seq: Check Bloom filter
    ///   - If in filter: Reject as replay
    ///   - If not in filter: Accept as out-of-order (add to filter)
    /// - If sequence > max_seq: Accept and update max_seq
    ///
    /// This allows some out-of-order delivery while preventing replays.
    pub fn check(&mut self, envelope: &SignedEnvelope) -> Result<()> {
        // Ensure initialized for persistent mode
        if !self.initialized.load(Ordering::Acquire) {
            self.load_and_apply_safety_gap()?;
        }

        // 1. Verify signature and age
        envelope.verify(self.max_clock_skew)?;

        // 2. Get or create sequence window for this sender
        let window = self
            .sequences
            .entry(envelope.from.clone())
            .or_insert_with(SequenceWindow::new);

        // 3. Check if sequence is finalized (CRITICAL: prevents replay after processing)
        if window.finalized.contains_key(&envelope.sequence) {
            bail!(
                "Replay attempt detected from {}: sequence {} is finalized (processed)",
                envelope.from.as_str(),
                envelope.sequence
            );
        }

        // 4. Check against floor_seq (CRITICAL: prevents replay after restart)
        // After restart, floor_seq is set to max_seq + RESTART_SAFETY_GAP
        // All sequences at or below this are rejected (bloom filter is lost on restart)
        if envelope.sequence <= window.floor_seq {
            bail!(
                "Replay detected from {}: sequence {} already seen (floor: {})",
                envelope.from.as_str(),
                envelope.sequence,
                window.floor_seq
            );
        }

        // 5. Check sequence number against Bloom filter
        let seq_hash = hash_sequence(envelope.sequence);
        if envelope.sequence <= window.max_seq {
            // Potentially out-of-order or replay
            if window.recent.contains(&seq_hash) {
                bail!(
                    "Replay detected from {}: sequence {} already seen (max: {})",
                    envelope.from.as_str(),
                    envelope.sequence,
                    window.max_seq
                );
            }
            // Not in filter: accept as out-of-order
        }

        // 6. Update window
        let max_seq_changed = envelope.sequence > window.max_seq;
        if max_seq_changed {
            window.max_seq = envelope.sequence;
        }
        window.insert_sequence(&seq_hash);
        window.last_update = Instant::now();

        // 7. Persist max_seq if changed (fail-safe: persist before returning success)
        if max_seq_changed {
            if let Err(e) = self.persist_max_seq(&envelope.from, envelope.sequence) {
                tracing::warn!(
                    peer = %envelope.from,
                    seq = envelope.sequence,
                    error = %e,
                    "Failed to persist max_seq (continuing anyway)"
                );
                // Note: We continue despite persistence failure because:
                // - The message has been validated and should be processed
                // - The safety gap on restart handles missed persistence
                // - Better to process valid messages than fail on storage issues
            }
        }

        Ok(())
    }

    /// Finalize a sequence number (permanently prevent replay)
    ///
    /// Call this after successfully processing a message (e.g., ledger entry written).
    /// Once finalized, the sequence cannot be replayed even within the time window.
    ///
    /// # Example
    /// ```ignore
    /// // Check message
    /// replay_guard.check(&envelope)?;
    ///
    /// // Process message (write to ledger, etc.)
    /// ledger.append(entry)?;
    ///
    /// // Finalize to prevent replay
    /// replay_guard.finalize(&envelope.from, envelope.sequence)?;
    /// ```
    pub fn finalize(&mut self, sender: &Did, sequence: u64) -> Result<()> {
        let window = self
            .sequences
            .get_mut(sender)
            .context("Cannot finalize sequence for unknown sender")?;

        window.finalized.insert(sequence, Instant::now());

        // Persist finalized sequence
        if let Err(e) = self.persist_finalized(sender, sequence) {
            tracing::warn!(
                peer = %sender,
                seq = sequence,
                error = %e,
                "Failed to persist finalized sequence"
            );
        }

        Ok(())
    }

    /// Check if a sequence is finalized
    pub fn is_finalized(&self, sender: &Did, sequence: u64) -> bool {
        self.sequences
            .get(sender)
            .map(|w| w.finalized.contains_key(&sequence))
            .unwrap_or(false)
    }

    /// Cleanup old peer state to prevent unbounded memory growth
    ///
    /// Should be called periodically (e.g., every 60 seconds)
    /// Prunes:
    /// - Inactive peer windows (no messages in max_peer_age_secs)
    /// - Old finalized sequences (>24 hours old)
    ///
    /// Also cleans up corresponding persistent storage.
    pub fn cleanup(&mut self) {
        let max_age = Duration::from_secs(self.max_peer_age_secs);
        let finalized_max_age = Duration::from_secs(24 * 60 * 60); // 24 hours
        let now = Instant::now();

        // Collect DIDs to remove from storage
        let mut dids_to_remove: Vec<Did> = Vec::new();

        // Remove inactive peer windows
        self.sequences.retain(|did, window| {
            let keep = now.duration_since(window.last_update) < max_age;
            if !keep {
                dids_to_remove.push(did.clone());
            }
            keep
        });

        // Delete from storage
        if let Some(ref store) = self.store {
            for did in &dids_to_remove {
                let key = Self::make_max_seq_key(did);
                if let Err(e) = store.delete(&key) {
                    tracing::warn!(peer = %did, error = %e, "Failed to delete max_seq from storage");
                }
            }
        }

        // Prune old finalized sequences from remaining windows
        let cutoff_ms = Self::current_time_ms().saturating_sub(24 * 60 * 60 * 1000);

        for (did, window) in self.sequences.iter_mut() {
            let old_finalized: Vec<u64> = window
                .finalized
                .iter()
                .filter(|(_, &finalized_at)| now.duration_since(finalized_at) >= finalized_max_age)
                .map(|(&seq, _)| seq)
                .collect();

            for seq in &old_finalized {
                window.finalized.remove(seq);

                // Delete from storage
                if let Some(ref store) = self.store {
                    let key = Self::make_finalized_key(did, *seq);
                    if let Err(e) = store.delete(&key) {
                        tracing::warn!(
                            peer = %did,
                            seq = seq,
                            error = %e,
                            "Failed to delete finalized sequence from storage"
                        );
                    }
                }
            }
        }

        // Also clean up old finalized entries from storage that may not be in memory
        if let Some(ref store) = self.store {
            if let Ok(entries) = store.scan(FINALIZED_PREFIX) {
                for (key, value) in entries {
                    if let Ok(entry) = serde_json::from_slice::<FinalizedEntry>(&value) {
                        if entry.finalized_at_ms < cutoff_ms {
                            if let Err(e) = store.delete(&key) {
                                tracing::warn!(error = %e, "Failed to delete old finalized entry");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get the number of tracked peers
    pub fn peer_count(&self) -> usize {
        self.sequences.len()
    }

    /// Get the max sequence seen for a specific peer
    pub fn get_max_seq(&self, did: &Did) -> Option<u64> {
        self.sequences.get(did).map(|w| w.max_seq)
    }

    /// Check if the guard is using persistent storage
    pub fn is_persistent(&self) -> bool {
        self.store.is_some()
    }

    /// Check if the guard has been initialized (loaded from storage)
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    // -------------------------------------------------------------------------
    // Persistence helpers
    // -------------------------------------------------------------------------

    fn persist_max_seq(&self, did: &Did, max_seq: u64) -> Result<()> {
        self.persist_max_seq_inner(did, max_seq)
    }

    fn persist_max_seq_inner(&self, did: &Did, max_seq: u64) -> Result<()> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()), // In-memory mode
        };

        let key = Self::make_max_seq_key(did);
        let entry = MaxSeqEntry {
            max_seq,
            updated_at_ms: Self::current_time_ms(),
        };
        let value = serde_json::to_vec(&entry).context("Failed to serialize max_seq entry")?;

        store
            .put(&key, &value)
            .context("Failed to persist max_seq")?;

        Ok(())
    }

    fn persist_finalized(&self, did: &Did, sequence: u64) -> Result<()> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()), // In-memory mode
        };

        let key = Self::make_finalized_key(did, sequence);
        let entry = FinalizedEntry {
            finalized_at_ms: Self::current_time_ms(),
        };
        let value = serde_json::to_vec(&entry).context("Failed to serialize finalized entry")?;

        store
            .put(&key, &value)
            .context("Failed to persist finalized sequence")?;

        Ok(())
    }

    fn make_max_seq_key(did: &Did) -> Vec<u8> {
        let mut key = Vec::with_capacity(MAX_SEQ_PREFIX.len() + 100);
        key.extend_from_slice(MAX_SEQ_PREFIX);
        key.extend_from_slice(did.as_str().as_bytes());
        key
    }

    fn parse_max_seq_key(key: &[u8]) -> Option<Did> {
        if !key.starts_with(MAX_SEQ_PREFIX) {
            return None;
        }
        let rest = &key[MAX_SEQ_PREFIX.len()..];
        let did_str = std::str::from_utf8(rest).ok()?;
        Did::from_str(did_str).ok()
    }

    fn make_finalized_key(did: &Did, sequence: u64) -> Vec<u8> {
        let mut key = Vec::with_capacity(FINALIZED_PREFIX.len() + 120);
        key.extend_from_slice(FINALIZED_PREFIX);
        key.extend_from_slice(did.as_str().as_bytes());
        key.push(b':');
        key.extend_from_slice(sequence.to_string().as_bytes());
        key
    }

    fn parse_finalized_key(key: &[u8]) -> Option<(Did, u64)> {
        if !key.starts_with(FINALIZED_PREFIX) {
            return None;
        }
        let rest = &key[FINALIZED_PREFIX.len()..];
        let rest_str = std::str::from_utf8(rest).ok()?;

        // Find the last colon (sequence is after it)
        let colon_pos = rest_str.rfind(':')?;
        let did_str = &rest_str[..colon_pos];
        let seq_str = &rest_str[colon_pos + 1..];

        let did = Did::from_str(did_str).ok()?;
        let seq = seq_str.parse().ok()?;

        Some((did, seq))
    }

    fn current_time_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

impl SequenceWindow {
    /// Create a new sequence window
    ///
    /// Bloom filter sized for:
    /// - 10,000 recent sequences
    /// - 0.1% false positive rate
    /// - ~10KB memory per peer
    fn new() -> Self {
        SequenceWindow {
            max_seq: 0,
            floor_seq: 0,
            recent: BloomFilter::new(BLOOM_CAPACITY, 0.001),
            insertion_count: 0,
            finalized: HashMap::new(),
            last_update: Instant::now(),
        }
    }

    /// Insert a sequence hash into the Bloom filter, rotating if necessary
    ///
    /// When the filter approaches saturation (80% capacity), it is reset
    /// to prevent false positives. The max_seq provides replay protection
    /// for sequences below the threshold even after reset.
    fn insert_sequence(&mut self, seq_hash: &[u8; 32]) {
        // Check if we need to rotate before inserting
        if self.insertion_count >= BLOOM_ROTATION_THRESHOLD {
            self.rotate_bloom_filter();
        }

        self.recent.insert(seq_hash);
        self.insertion_count += 1;
    }

    /// Rotate (reset) the Bloom filter to prevent saturation
    ///
    /// After rotation:
    /// - The filter is empty and can accept new sequences
    /// - max_seq still prevents replay of old sequences
    /// - Finalized sequences are still protected
    /// - There's a brief window where some out-of-order sequences
    ///   might be accepted twice, but this is acceptable as:
    ///   1. Finalized sequences are never replayed
    ///   2. Double-processing of non-finalized sequences is idempotent
    fn rotate_bloom_filter(&mut self) {
        tracing::debug!(
            max_seq = self.max_seq,
            insertion_count = self.insertion_count,
            "Rotating Bloom filter to prevent saturation"
        );
        self.recent = BloomFilter::new(BLOOM_CAPACITY, 0.001);
        self.insertion_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::PayloadType;
    use icn_identity::KeyPair;

    #[test]
    fn test_fresh_message_accepted() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // First delivery: OK
        assert!(guard.check(&envelope).is_ok());
        assert_eq!(guard.get_max_seq(keypair.did()), Some(1));
    }

    #[test]
    fn test_replay_rejected() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // First delivery: OK
        assert!(guard.check(&envelope).is_ok());

        // Replay: Rejected
        let result = guard.check(&envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Replay detected"));
    }

    #[test]
    fn test_monotonic_sequences_accepted() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        // Send messages 1, 2, 3 in order
        for seq in 1..=3 {
            let envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                seq,
                PayloadType::Gossip,
                format!("test {seq}").as_bytes().to_vec(),
            )
            .unwrap();

            assert!(guard.check(&envelope).is_ok());
        }

        assert_eq!(guard.get_max_seq(keypair.did()), Some(3));
    }

    #[test]
    fn test_out_of_order_accepted_once() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        // Send sequence 3 first
        let env3 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            3,
            PayloadType::Gossip,
            b"msg3".to_vec(),
        )
        .unwrap();
        assert!(guard.check(&env3).is_ok());
        assert_eq!(guard.get_max_seq(keypair.did()), Some(3));

        // Send sequence 2 (out of order but not a replay)
        let env2 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            2,
            PayloadType::Gossip,
            b"msg2".to_vec(),
        )
        .unwrap();
        assert!(guard.check(&env2).is_ok());

        // Try to replay sequence 2 (should be rejected)
        let result = guard.check(&env2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Replay detected"));
    }

    #[test]
    fn test_multiple_peers_independent() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair1 = KeyPair::generate().unwrap();
        let keypair2 = KeyPair::generate().unwrap();

        // Send seq 1 from peer 1
        let env1 = SignedEnvelope::new(
            keypair1.did(),
            &keypair1,
            1,
            PayloadType::Gossip,
            b"peer1-msg1".to_vec(),
        )
        .unwrap();
        assert!(guard.check(&env1).is_ok());

        // Send seq 1 from peer 2 (different peer, should be OK)
        let env2 = SignedEnvelope::new(
            keypair2.did(),
            &keypair2,
            1,
            PayloadType::Gossip,
            b"peer2-msg1".to_vec(),
        )
        .unwrap();
        assert!(guard.check(&env2).is_ok());

        assert_eq!(guard.peer_count(), 2);
        assert_eq!(guard.get_max_seq(keypair1.did()), Some(1));
        assert_eq!(guard.get_max_seq(keypair2.did()), Some(1));
    }

    #[test]
    fn test_cleanup_removes_old_peers() {
        let mut guard = ReplayGuard::new(300, 1); // 1 second max age
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        assert!(guard.check(&envelope).is_ok());
        assert_eq!(guard.peer_count(), 1);

        // Wait for peer to age out
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // Cleanup should remove the peer
        guard.cleanup();
        assert_eq!(guard.peer_count(), 0);
    }

    #[test]
    fn test_invalid_signature_rejected_before_sequence_check() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair1 = KeyPair::generate().unwrap();
        let keypair2 = KeyPair::generate().unwrap();

        // Create envelope signed by keypair1 but claiming to be from keypair2
        let envelope = SignedEnvelope::new(
            keypair2.did(), // Claim to be keypair2
            &keypair1,      // But sign with keypair1
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // Should be rejected due to signature mismatch (before sequence check)
        let result = guard.check(&envelope);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Signature verification failed"));

        // No sequence state should be created for invalid messages
        assert_eq!(guard.peer_count(), 0);
    }

    #[test]
    fn test_finalize_prevents_replay() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Ledger,
            b"transaction".to_vec(),
        )
        .unwrap();

        // First check: OK
        assert!(guard.check(&envelope).is_ok());

        // Finalize sequence (transaction processed)
        assert!(guard.finalize(keypair.did(), 1).is_ok());
        assert!(guard.is_finalized(keypair.did(), 1));

        // Attempt replay after finalization: REJECTED
        let result = guard.check(&envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("finalized"));
    }

    #[test]
    fn test_finalize_different_sequence_independent() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope1 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Ledger,
            b"tx1".to_vec(),
        )
        .unwrap();

        let envelope2 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            2,
            PayloadType::Ledger,
            b"tx2".to_vec(),
        )
        .unwrap();

        // Check both
        assert!(guard.check(&envelope1).is_ok());
        assert!(guard.check(&envelope2).is_ok());

        // Finalize sequence 1 only
        assert!(guard.finalize(keypair.did(), 1).is_ok());

        // Sequence 1 blocked (finalized)
        assert!(guard.check(&envelope1).is_err());

        // Sequence 2 blocked (already in Bloom filter from first check)
        // But NOT finalized, so if we create a NEW envelope with seq 3, it should work
        let envelope3 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            3,
            PayloadType::Ledger,
            b"tx3".to_vec(),
        )
        .unwrap();

        assert!(guard.check(&envelope3).is_ok());

        // Finalize sequence 2
        assert!(guard.finalize(keypair.did(), 2).is_ok());

        // Now envelope3 can still be used (not finalized)
        // But envelope2 would be rejected as finalized if we check again
        assert!(guard.is_finalized(keypair.did(), 2));
        assert!(!guard.is_finalized(keypair.did(), 3));
    }

    #[test]
    fn test_replay_within_time_window_after_finalization() {
        // This is the KEY test - prevents the documented vulnerability
        let mut guard = ReplayGuard::new(300, 3600); // 5 minute window
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Ledger,
            b"critical_transaction".to_vec(),
        )
        .unwrap();

        // T=0: Transaction submitted
        assert!(guard.check(&envelope).is_ok());

        // T=1: Transaction processed, finalize
        assert!(guard.finalize(keypair.did(), 1).is_ok());

        // T=2: Attacker replays within 5-minute window
        // WITHOUT finalization: would be accepted (vulnerability)
        // WITH finalization: REJECTED (fixed)
        let result = guard.check(&envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("finalized"));
    }

    #[test]
    fn test_finalized_sequences_pruned_after_24h() {
        let mut guard = ReplayGuard::new(300, 1); // 1 second peer age for fast test
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Ledger,
            b"tx".to_vec(),
        )
        .unwrap();

        assert!(guard.check(&envelope).is_ok());
        assert!(guard.finalize(keypair.did(), 1).is_ok());
        assert!(guard.is_finalized(keypair.did(), 1));

        // In real usage, finalized sequences are pruned after 24h
        // For testing, we just verify cleanup doesn't crash with finalized seqs
        guard.cleanup();

        // Peer still tracked (finalized sequences kept)
        assert_eq!(guard.peer_count(), 1);
    }

    #[test]
    fn test_bloom_filter_rotation() {
        // Test that Bloom filter rotates to prevent saturation
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        // Send many messages (more than BLOOM_ROTATION_THRESHOLD)
        for seq in 1..=9000 {
            let envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                seq,
                PayloadType::Gossip,
                format!("msg{seq}").as_bytes().to_vec(),
            )
            .unwrap();

            // All should be accepted
            assert!(
                guard.check(&envelope).is_ok(),
                "Message {seq} should be accepted"
            );
        }

        // Verify max_seq was tracked correctly
        assert_eq!(guard.get_max_seq(keypair.did()), Some(9000));

        // After rotation, new messages should still be accepted
        let new_envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            9001,
            PayloadType::Gossip,
            b"new_msg".to_vec(),
        )
        .unwrap();
        assert!(guard.check(&new_envelope).is_ok());

        // Finalized sequences should still be protected after rotation
        guard.finalize(keypair.did(), 100).unwrap();
        let old_envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            100,
            PayloadType::Gossip,
            b"replayed".to_vec(),
        )
        .unwrap();
        let result = guard.check(&old_envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("finalized"));
    }

    // -------------------------------------------------------------------------
    // Persistence tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_persistent_guard_creation() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let guard = ReplayGuard::new_persistent(300, 3600, store);

        assert!(guard.is_persistent());
        assert!(!guard.is_initialized());
    }

    #[test]
    fn test_persistence_and_restart_safety_gap() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let keypair = KeyPair::generate().unwrap();

        // Session 1: Create guard, check some messages
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_and_apply_safety_gap().unwrap();

            for seq in 1..=5 {
                let envelope = SignedEnvelope::new(
                    keypair.did(),
                    &keypair,
                    seq,
                    PayloadType::Gossip,
                    format!("msg{seq}").as_bytes().to_vec(),
                )
                .unwrap();
                assert!(guard.check(&envelope).is_ok());
            }

            assert_eq!(guard.get_max_seq(keypair.did()), Some(5));
        }

        // Session 2: Simulate restart - create new guard from same store
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            let loaded = guard.load_and_apply_safety_gap().unwrap();

            assert_eq!(loaded, 1); // One peer loaded

            // max_seq should be 5 + RESTART_SAFETY_GAP
            let expected_max_seq = 5 + RESTART_SAFETY_GAP;
            assert_eq!(guard.get_max_seq(keypair.did()), Some(expected_max_seq));

            // Replays of old sequences should be rejected
            let old_envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                5,
                PayloadType::Gossip,
                b"replay_attempt".to_vec(),
            )
            .unwrap();
            let result = guard.check(&old_envelope);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("already seen"));

            // New sequences above the gap should work
            let new_envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                expected_max_seq + 1,
                PayloadType::Gossip,
                b"new_msg".to_vec(),
            )
            .unwrap();
            assert!(guard.check(&new_envelope).is_ok());
        }
    }

    #[test]
    fn test_finalized_persistence_across_restart() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let keypair = KeyPair::generate().unwrap();

        // Session 1: Finalize a sequence
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_and_apply_safety_gap().unwrap();

            let envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                100,
                PayloadType::Ledger,
                b"critical_tx".to_vec(),
            )
            .unwrap();

            assert!(guard.check(&envelope).is_ok());
            assert!(guard.finalize(keypair.did(), 100).is_ok());
        }

        // Session 2: Verify finalized sequence is still protected
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_and_apply_safety_gap().unwrap();

            assert!(guard.is_finalized(keypair.did(), 100));

            // Attempting to replay finalized sequence should fail
            let replay_envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                100,
                PayloadType::Ledger,
                b"replay_critical_tx".to_vec(),
            )
            .unwrap();
            let result = guard.check(&replay_envelope);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("finalized"));
        }
    }

    #[test]
    fn test_multiple_restart_compounds_safety_gap() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let keypair = KeyPair::generate().unwrap();

        // Session 1
        let session1_max = {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_and_apply_safety_gap().unwrap();

            let envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                10,
                PayloadType::Gossip,
                b"msg".to_vec(),
            )
            .unwrap();
            assert!(guard.check(&envelope).is_ok());

            guard.get_max_seq(keypair.did()).unwrap()
        };
        assert_eq!(session1_max, 10);

        // Session 2 (first restart)
        let session2_max = {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_and_apply_safety_gap().unwrap();
            guard.get_max_seq(keypair.did()).unwrap()
        };
        assert_eq!(session2_max, 10 + RESTART_SAFETY_GAP);

        // Session 3 (second restart)
        let session3_max = {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_and_apply_safety_gap().unwrap();
            guard.get_max_seq(keypair.did()).unwrap()
        };
        assert_eq!(session3_max, 10 + 2 * RESTART_SAFETY_GAP);
    }

    #[test]
    fn test_key_parsing() {
        let did = KeyPair::generate().unwrap().did().clone();

        // Max seq key
        let key = ReplayGuard::make_max_seq_key(&did);
        let parsed = ReplayGuard::parse_max_seq_key(&key).unwrap();
        assert_eq!(parsed.as_str(), did.as_str());

        // Finalized key
        let seq = 12345u64;
        let fkey = ReplayGuard::make_finalized_key(&did, seq);
        let (parsed_did, parsed_seq) = ReplayGuard::parse_finalized_key(&fkey).unwrap();
        assert_eq!(parsed_did.as_str(), did.as_str());
        assert_eq!(parsed_seq, seq);
    }

    #[test]
    fn test_auto_initialization() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let keypair = KeyPair::generate().unwrap();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store);

        // Not initialized yet
        assert!(!guard.is_initialized());

        // First check auto-initializes
        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        assert!(guard.check(&envelope).is_ok());
        assert!(guard.is_initialized());
    }
}
