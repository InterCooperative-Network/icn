//! Application-level replay protection for blob transfer messages.
//!
//! Tracks per-peer nonces to prevent duplicate processing of:
//! - `BlobRequest`: uses `request_id` as nonce
//! - `BlobTransferChunk`: uses `blake3(request_id || chunk_index)` as composite nonce
//!
//! This is complementary to envelope-level replay protection (sequence-based
//! `ReplayGuard` in `icn-net`). Envelope replay guards prevent resending the
//! exact same signed message, while this guard prevents *semantic* duplicates:
//! the same logical request or chunk repackaged in a new envelope.
//!
//! # Design
//!
//! - Per-peer nonce windows prevent cross-peer replay
//! - Time-based expiry prevents unbounded memory growth
//! - Cleanup runs lazily on each check (no background task needed)

use crate::types::ContentHash;
use icn_identity::Did;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Default TTL for blob nonces (5 minutes)
pub const DEFAULT_NONCE_TTL_SECS: u64 = 300;

/// Maximum nonce entries per peer before forced eviction
pub const MAX_NONCES_PER_PEER: usize = 2048;

/// Derive a composite nonce from request_id and chunk_index.
///
/// Uses blake3 to combine the two values into a single 32-byte nonce,
/// ensuring collision resistance and deterministic derivation.
pub fn composite_chunk_nonce(request_id: &ContentHash, chunk_index: u32) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(request_id);
    hasher.update(&chunk_index.to_be_bytes());
    *hasher.finalize().as_bytes()
}

/// Per-peer nonce tracking window
struct PeerNonceWindow {
    /// Seen nonces mapped to their insertion time
    nonces: HashMap<ContentHash, Instant>,
}

impl PeerNonceWindow {
    fn new() -> Self {
        Self {
            nonces: HashMap::new(),
        }
    }

    /// Remove expired entries and return number removed.
    fn evict_expired(&mut self, ttl: Duration) -> usize {
        let cutoff = Instant::now() - ttl;
        let before = self.nonces.len();
        self.nonces.retain(|_, inserted_at| *inserted_at > cutoff);
        before - self.nonces.len()
    }

    /// Force-evict oldest entries until under the limit.
    fn evict_oldest(&mut self, max: usize) {
        if self.nonces.len() <= max {
            return;
        }
        let to_remove = self.nonces.len() - max;
        // Collect the oldest entries
        let mut entries: Vec<(ContentHash, Instant)> =
            self.nonces.iter().map(|(k, v)| (*k, *v)).collect();
        entries.sort_by_key(|(_, t)| *t);
        for (nonce, _) in entries.into_iter().take(to_remove) {
            self.nonces.remove(&nonce);
        }
    }
}

/// Application-level nonce guard for blob transfer replay protection.
///
/// Prevents duplicate processing of blob requests and transfer chunks
/// by tracking per-peer nonces with time-based expiry.
pub struct BlobNonceGuard {
    /// Per-peer nonce windows
    peers: HashMap<Did, PeerNonceWindow>,
    /// Time-to-live for nonce entries
    ttl: Duration,
    /// Maximum entries per peer
    max_per_peer: usize,
}

impl BlobNonceGuard {
    /// Create a new blob nonce guard with the given TTL and per-peer limit.
    pub fn new(ttl: Duration, max_per_peer: usize) -> Self {
        Self {
            peers: HashMap::new(),
            ttl,
            max_per_peer,
        }
    }

    /// Create a guard with default settings (5 min TTL, 2048 max per peer).
    pub fn default_config() -> Self {
        Self::new(
            Duration::from_secs(DEFAULT_NONCE_TTL_SECS),
            MAX_NONCES_PER_PEER,
        )
    }

    /// Check and record a nonce for a given sender.
    ///
    /// Returns `Ok(())` if the nonce is fresh (not seen before within TTL).
    /// Returns `Err` if the nonce was already recorded (replay).
    ///
    /// Lazily evicts expired entries on each call.
    pub fn check_and_record(
        &mut self,
        sender: &Did,
        nonce: ContentHash,
    ) -> Result<(), BlobReplayError> {
        let window = self
            .peers
            .entry(sender.clone())
            .or_insert_with(PeerNonceWindow::new);

        // Lazy cleanup: evict expired entries
        window.evict_expired(self.ttl);

        // Check for duplicate
        if window.nonces.contains_key(&nonce) {
            return Err(BlobReplayError {
                sender: sender.clone(),
                nonce,
            });
        }

        // Enforce per-peer limit
        if window.nonces.len() >= self.max_per_peer {
            window.evict_oldest(self.max_per_peer - 1);
        }

        window.nonces.insert(nonce, Instant::now());
        Ok(())
    }

    /// Remove all state for inactive peers (no nonces remaining after expiry).
    ///
    /// Should be called periodically (e.g., in the GossipActor cleanup tick).
    /// Used by PR2c (transfer state machine) for housekeeping.
    #[allow(dead_code)]
    pub fn cleanup_empty_peers(&mut self) {
        self.peers.retain(|_, window| {
            window.evict_expired(self.ttl);
            !window.nonces.is_empty()
        });
    }

    /// Number of tracked peers.
    #[allow(dead_code)]
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Number of tracked nonces for a specific peer.
    #[allow(dead_code)]
    pub fn nonce_count(&self, peer: &Did) -> usize {
        self.peers.get(peer).map(|w| w.nonces.len()).unwrap_or(0)
    }
}

/// Error returned when a blob message nonce is replayed.
#[derive(Debug)]
pub struct BlobReplayError {
    pub sender: Did,
    pub nonce: ContentHash,
}

impl std::fmt::Display for BlobReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Blob replay detected from {}: nonce {}",
            self.sender,
            hex::encode(self.nonce)
        )
    }
}

impl std::error::Error for BlobReplayError {}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn fresh_nonce_accepted() {
        let mut guard = BlobNonceGuard::default_config();
        let sender = make_did();
        let nonce = [1u8; 32];

        assert!(guard.check_and_record(&sender, nonce).is_ok());
        assert_eq!(guard.nonce_count(&sender), 1);
    }

    #[test]
    fn duplicate_nonce_rejected() {
        let mut guard = BlobNonceGuard::default_config();
        let sender = make_did();
        let nonce = [1u8; 32];

        assert!(guard.check_and_record(&sender, nonce).is_ok());
        let err = guard.check_and_record(&sender, nonce);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Blob replay detected"));
    }

    #[test]
    fn different_nonces_accepted() {
        let mut guard = BlobNonceGuard::default_config();
        let sender = make_did();

        assert!(guard.check_and_record(&sender, [1u8; 32]).is_ok());
        assert!(guard.check_and_record(&sender, [2u8; 32]).is_ok());
        assert_eq!(guard.nonce_count(&sender), 2);
    }

    #[test]
    fn different_peers_independent() {
        let mut guard = BlobNonceGuard::default_config();
        let peer_a = make_did();
        let peer_b = make_did();
        let nonce = [42u8; 32];

        // Same nonce from different peers → both accepted
        assert!(guard.check_and_record(&peer_a, nonce).is_ok());
        assert!(guard.check_and_record(&peer_b, nonce).is_ok());
        assert_eq!(guard.peer_count(), 2);
    }

    #[test]
    fn composite_chunk_nonce_same_request_different_chunks() {
        let mut guard = BlobNonceGuard::default_config();
        let sender = make_did();
        let request_id = [0xAA; 32];

        let nonce_0 = composite_chunk_nonce(&request_id, 0);
        let nonce_1 = composite_chunk_nonce(&request_id, 1);

        // Different chunk indices → different nonces → both accepted
        assert_ne!(nonce_0, nonce_1);
        assert!(guard.check_and_record(&sender, nonce_0).is_ok());
        assert!(guard.check_and_record(&sender, nonce_1).is_ok());
    }

    #[test]
    fn composite_chunk_nonce_same_chunk_rejected() {
        let mut guard = BlobNonceGuard::default_config();
        let sender = make_did();
        let request_id = [0xBB; 32];

        let nonce = composite_chunk_nonce(&request_id, 5);

        assert!(guard.check_and_record(&sender, nonce).is_ok());
        assert!(guard.check_and_record(&sender, nonce).is_err());
    }

    #[test]
    fn expired_nonces_evicted() {
        let mut guard = BlobNonceGuard::new(Duration::from_millis(50), MAX_NONCES_PER_PEER);
        let sender = make_did();
        let nonce = [1u8; 32];

        assert!(guard.check_and_record(&sender, nonce).is_ok());
        assert_eq!(guard.nonce_count(&sender), 1);

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(60));

        // Same nonce should now be accepted (expired)
        assert!(guard.check_and_record(&sender, nonce).is_ok());
    }

    #[test]
    fn per_peer_limit_enforced() {
        let mut guard = BlobNonceGuard::new(Duration::from_secs(300), 4);
        let sender = make_did();

        // Fill to capacity
        for i in 0..4u8 {
            let mut nonce = [0u8; 32];
            nonce[0] = i;
            assert!(guard.check_and_record(&sender, nonce).is_ok());
        }
        assert_eq!(guard.nonce_count(&sender), 4);

        // One more should succeed (evicts oldest)
        let mut nonce = [0u8; 32];
        nonce[0] = 99;
        assert!(guard.check_and_record(&sender, nonce).is_ok());
        // Should be at limit (evicted one, added one)
        assert_eq!(guard.nonce_count(&sender), 4);
    }

    #[test]
    fn cleanup_removes_empty_peers() {
        let mut guard = BlobNonceGuard::new(Duration::from_millis(50), MAX_NONCES_PER_PEER);
        let sender = make_did();

        assert!(guard.check_and_record(&sender, [1u8; 32]).is_ok());
        assert_eq!(guard.peer_count(), 1);

        std::thread::sleep(Duration::from_millis(60));

        guard.cleanup_empty_peers();
        assert_eq!(guard.peer_count(), 0);
    }

    #[test]
    fn composite_nonce_is_deterministic() {
        let request_id = [0xCC; 32];
        let chunk_index = 42u32;

        let nonce_a = composite_chunk_nonce(&request_id, chunk_index);
        let nonce_b = composite_chunk_nonce(&request_id, chunk_index);

        assert_eq!(nonce_a, nonce_b);
    }
}
