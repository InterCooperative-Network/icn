//! Blob location registry for distributed data tracking
//!
//! This module provides a registry that tracks which peers have which blobs.
//! It's used for locality-aware task placement (Phase 16C) to minimize data
//! transfer by placing tasks near their input data.
//!
//! ## Architecture
//!
//! - **BlobLocationRegistry**: Central registry mapping blob hashes to peer locations
//! - **TTL-based expiration**: Locations expire after 24 hours to handle churn
//! - **Size limits**: Configurable limits prevent memory exhaustion attacks
//! - **LRU eviction**: Oldest entries evicted when capacity is reached
//! - **Per-peer quotas**: Prevent any single peer from dominating the registry
//!
//! ## Usage
//!
//! ```rust,ignore
//! use icn_net::{BlobLocationRegistry, BlobRegistryConfig};
//! use icn_identity::Did;
//!
//! let config = BlobRegistryConfig::default();
//! let mut registry = BlobLocationRegistry::with_config(config);
//!
//! // Peer announces blob availability
//! let blob_hash = [0u8; 32];
//! let peer_did: Did = /* valid DID */;
//! registry.announce_blob(blob_hash, peer_did.clone(), 1024)?;
//!
//! // Query peers with blob
//! let peers = registry.get_peers_with_blob(&blob_hash);
//! assert_eq!(peers.len(), 1);
//! ```

use icn_gossip::types::ContentHash;
use icn_identity::Did;
use icn_obs::metrics::network::{
    blob_evicted_inc, blob_rejected_inc, blob_registry_entries_set, blob_registry_size_set,
    BlobRejectionReason,
};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Errors that can occur when interacting with the blob registry
#[derive(Debug, Error)]
pub enum BlobRegistryError {
    /// Blob size exceeds the maximum allowed size
    #[error("blob size {size} exceeds maximum allowed size {max}")]
    BlobTooLarge { size: u64, max: u64 },

    /// Peer has exceeded their quota
    #[error("peer {peer} quota exceeded: using {used}, limit {limit}, requested {requested}")]
    PeerQuotaExceeded {
        peer: String,
        used: u64,
        limit: u64,
        requested: u64,
    },
}

/// Configuration for the blob location registry
#[derive(Debug, Clone)]
pub struct BlobRegistryConfig {
    /// Maximum size of a single blob in bytes (default: 10MB)
    pub max_blob_size: u64,

    /// Maximum total size of all blobs in the registry in bytes (default: 100MB)
    pub max_registry_size: u64,

    /// Maximum total blob size per peer in bytes (default: 10MB)
    pub max_per_peer_size: u64,
}

impl Default for BlobRegistryConfig {
    fn default() -> Self {
        Self {
            max_blob_size: 10 * 1024 * 1024,      // 10MB
            max_registry_size: 100 * 1024 * 1024, // 100MB
            max_per_peer_size: 10 * 1024 * 1024,  // 10MB
        }
    }
}

/// Location information for a blob on a specific peer
#[derive(Debug, Clone)]
pub struct BlobLocation {
    /// Peer that has the blob
    pub peer_did: Did,

    /// Blob size in bytes
    pub size_bytes: u64,

    /// When this location was announced
    announced_at: Instant,
}

impl BlobLocation {
    /// Create a new blob location
    pub fn new(peer_did: Did, size_bytes: u64) -> Self {
        Self {
            peer_did,
            size_bytes,
            announced_at: Instant::now(),
        }
    }

    /// Check if this location has expired (24 hour TTL)
    pub fn is_expired(&self) -> bool {
        self.announced_at.elapsed() > Duration::from_secs(86400) // 24 hours
    }

    /// Get age of this location in seconds
    pub fn age_secs(&self) -> u64 {
        self.announced_at.elapsed().as_secs()
    }
}

/// Registry tracking blob locations across the network
#[derive(Debug)]
pub struct BlobLocationRegistry {
    /// Configuration for size limits
    config: BlobRegistryConfig,

    /// Map from blob hash to list of peers that have it
    locations: HashMap<ContentHash, Vec<BlobLocation>>,

    /// LRU order: (blob_hash, peer_did) pairs, oldest at front
    lru_order: VecDeque<(ContentHash, Did)>,

    /// Total size of all blobs in the registry
    total_size: u64,

    /// Size used by each peer
    per_peer_size: HashMap<Did, u64>,

    /// Statistics: number of blobs rejected for being too large
    rejected_too_large: u64,

    /// Statistics: number of blobs rejected for peer quota exceeded
    rejected_quota_exceeded: u64,

    /// Statistics: number of entries evicted due to capacity
    evicted_count: u64,
}

impl BlobLocationRegistry {
    /// Create a new empty registry with default configuration
    pub fn new() -> Self {
        Self::with_config(BlobRegistryConfig::default())
    }

    /// Create a new registry with the specified configuration
    pub fn with_config(config: BlobRegistryConfig) -> Self {
        Self {
            config,
            locations: HashMap::new(),
            lru_order: VecDeque::new(),
            total_size: 0,
            per_peer_size: HashMap::new(),
            rejected_too_large: 0,
            rejected_quota_exceeded: 0,
            evicted_count: 0,
        }
    }

    /// Announce that a peer has a blob
    ///
    /// This updates the registry with the peer's location information.
    /// If the peer already announced this blob, updates the timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The blob size exceeds `max_blob_size`
    /// - The peer would exceed their quota (`max_per_peer_size`)
    pub fn announce_blob(
        &mut self,
        blob_hash: ContentHash,
        peer_did: Did,
        size_bytes: u64,
    ) -> Result<(), BlobRegistryError> {
        // Check per-blob size limit
        if size_bytes > self.config.max_blob_size {
            self.rejected_too_large += 1;
            blob_rejected_inc(BlobRejectionReason::TooLarge);
            return Err(BlobRegistryError::BlobTooLarge {
                size: size_bytes,
                max: self.config.max_blob_size,
            });
        }

        // Calculate the net size change for this peer
        let existing_size = self.get_existing_size(&blob_hash, &peer_did);
        let size_delta = size_bytes.saturating_sub(existing_size);

        // Check per-peer quota (only for new size, not updates)
        if size_delta > 0 {
            let current_peer_size = self.per_peer_size.get(&peer_did).copied().unwrap_or(0);
            let new_peer_size = current_peer_size + size_delta;

            if new_peer_size > self.config.max_per_peer_size {
                self.rejected_quota_exceeded += 1;
                blob_rejected_inc(BlobRejectionReason::PeerQuotaExceeded);
                return Err(BlobRegistryError::PeerQuotaExceeded {
                    peer: peer_did.to_string(),
                    used: current_peer_size,
                    limit: self.config.max_per_peer_size,
                    requested: size_bytes,
                });
            }
        }

        // Evict old entries if we would exceed total capacity
        while self.total_size + size_delta > self.config.max_registry_size {
            if !self.evict_oldest() {
                // Nothing left to evict, but we still need space
                // This shouldn't happen if max_blob_size <= max_registry_size
                break;
            }
        }

        // Remove existing entry if updating (to recalculate sizes)
        if existing_size > 0 {
            self.remove_entry(&blob_hash, &peer_did);
        }

        // Add the new entry
        let location = BlobLocation::new(peer_did.clone(), size_bytes);
        let locations = self.locations.entry(blob_hash).or_default();
        locations.push(location);

        // Update LRU order
        self.lru_order.push_back((blob_hash, peer_did.clone()));

        // Update size tracking
        self.total_size += size_bytes;
        *self.per_peer_size.entry(peer_did).or_insert(0) += size_bytes;

        // Update gauge metrics
        blob_registry_size_set(self.total_size);
        blob_registry_entries_set(self.location_count());

        Ok(())
    }

    /// Get the existing size of a blob announcement for a specific peer
    fn get_existing_size(&self, blob_hash: &ContentHash, peer_did: &Did) -> u64 {
        self.locations
            .get(blob_hash)
            .and_then(|locs| locs.iter().find(|loc| &loc.peer_did == peer_did))
            .map(|loc| loc.size_bytes)
            .unwrap_or(0)
    }

    /// Remove a specific entry from the registry
    fn remove_entry(&mut self, blob_hash: &ContentHash, peer_did: &Did) {
        if let Some(locations) = self.locations.get_mut(blob_hash) {
            if let Some(idx) = locations.iter().position(|loc| &loc.peer_did == peer_did) {
                let removed = locations.remove(idx);

                // Update size tracking
                self.total_size = self.total_size.saturating_sub(removed.size_bytes);
                if let Some(peer_size) = self.per_peer_size.get_mut(peer_did) {
                    *peer_size = peer_size.saturating_sub(removed.size_bytes);
                    if *peer_size == 0 {
                        self.per_peer_size.remove(peer_did);
                    }
                }
            }

            // Remove blob entry if no locations remain
            if locations.is_empty() {
                self.locations.remove(blob_hash);
            }
        }

        // Remove from LRU order (may have duplicates, remove first match)
        if let Some(idx) = self
            .lru_order
            .iter()
            .position(|(h, p)| h == blob_hash && p == peer_did)
        {
            self.lru_order.remove(idx);
        }
    }

    /// Evict the oldest entry from the registry
    ///
    /// Returns true if an entry was evicted, false if the registry is empty.
    fn evict_oldest(&mut self) -> bool {
        // Find the oldest non-expired entry
        while let Some((blob_hash, peer_did)) = self.lru_order.pop_front() {
            // Check if this entry still exists (may have been removed by cleanup)
            if self
                .locations
                .get(&blob_hash)
                .map(|locs| locs.iter().any(|loc| loc.peer_did == peer_did))
                .unwrap_or(false)
            {
                self.remove_entry(&blob_hash, &peer_did);
                self.evicted_count += 1;
                blob_evicted_inc();
                return true;
            }
        }
        false
    }

    /// Get all peers that have a specific blob
    ///
    /// Returns only non-expired locations, sorted by age (newest first).
    pub fn get_peers_with_blob(&self, blob_hash: &ContentHash) -> Vec<BlobLocation> {
        self.locations
            .get(blob_hash)
            .map(|locs| {
                let mut valid: Vec<_> = locs
                    .iter()
                    .filter(|loc| !loc.is_expired())
                    .cloned()
                    .collect();

                // Sort by age (newest first)
                valid.sort_by_key(|loc| loc.age_secs());
                valid
            })
            .unwrap_or_default()
    }

    /// Query multiple blobs at once
    ///
    /// Returns a map of blob hash → peer list for each requested blob.
    pub fn query_blobs(
        &self,
        blob_hashes: &[ContentHash],
    ) -> HashMap<ContentHash, Vec<BlobLocation>> {
        blob_hashes
            .iter()
            .map(|hash| (*hash, self.get_peers_with_blob(hash)))
            .filter(|(_, peers)| !peers.is_empty())
            .collect()
    }

    /// Find peers that have all requested blobs (data locality optimization)
    ///
    /// Returns peers sorted by number of matching blobs (descending).
    pub fn find_peers_with_all(&self, blob_hashes: &[ContentHash]) -> Vec<(Did, usize)> {
        if blob_hashes.is_empty() {
            return Vec::new();
        }

        // Count how many blobs each peer has
        let mut peer_counts: HashMap<Did, usize> = HashMap::new();

        for hash in blob_hashes {
            let peers = self.get_peers_with_blob(hash);
            for loc in peers {
                *peer_counts.entry(loc.peer_did).or_insert(0) += 1;
            }
        }

        // Convert to sorted vec (most blobs first)
        let mut result: Vec<_> = peer_counts.into_iter().collect();
        result.sort_by(|a, b| b.1.cmp(&a.1));
        result
    }

    /// Remove expired locations
    ///
    /// This should be called periodically to clean up stale entries.
    pub fn cleanup_expired(&mut self) {
        // Collect entries to remove
        let mut to_remove: Vec<(ContentHash, Did)> = Vec::new();

        for (hash, locations) in &self.locations {
            for loc in locations {
                if loc.is_expired() {
                    to_remove.push((*hash, loc.peer_did.clone()));
                }
            }
        }

        // Remove expired entries
        for (hash, peer) in to_remove {
            self.remove_entry(&hash, &peer);
        }

        // Update gauge metrics after cleanup
        blob_registry_size_set(self.total_size);
        blob_registry_entries_set(self.location_count());
    }

    /// Get total number of tracked blobs
    pub fn blob_count(&self) -> usize {
        self.locations.len()
    }

    /// Get total number of location entries
    pub fn location_count(&self) -> usize {
        self.locations.values().map(|v| v.len()).sum()
    }

    /// Get total size of all blobs in the registry
    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    /// Get the size used by a specific peer
    pub fn peer_size(&self, peer_did: &Did) -> u64 {
        self.per_peer_size.get(peer_did).copied().unwrap_or(0)
    }

    /// Get statistics about rejected blobs
    pub fn rejected_stats(&self) -> (u64, u64) {
        (self.rejected_too_large, self.rejected_quota_exceeded)
    }

    /// Get count of evicted entries
    pub fn evicted_count(&self) -> u64 {
        self.evicted_count
    }

    /// Remove all locations for a specific peer (e.g., when peer disconnects)
    pub fn remove_peer(&mut self, peer_did: &Did) {
        // Collect blob hashes where this peer has entries
        let blob_hashes: Vec<_> = self
            .locations
            .iter()
            .filter(|(_, locs)| locs.iter().any(|loc| &loc.peer_did == peer_did))
            .map(|(hash, _)| *hash)
            .collect();

        // Remove entries for this peer
        for hash in blob_hashes {
            self.remove_entry(&hash, peer_did);
        }

        // Update gauge metrics after removal
        blob_registry_size_set(self.total_size);
        blob_registry_entries_set(self.location_count());
    }
}

impl Default for BlobLocationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn create_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn create_test_config() -> BlobRegistryConfig {
        BlobRegistryConfig {
            max_blob_size: 1024 * 1024,      // 1MB
            max_registry_size: 10 * 1024 * 1024, // 10MB
            max_per_peer_size: 2 * 1024 * 1024,  // 2MB
        }
    }

    #[test]
    fn test_announce_and_query_blob() {
        let mut registry = BlobLocationRegistry::new();
        let blob_hash = [1u8; 32];
        let peer1 = create_test_did();

        // Announce blob
        registry.announce_blob(blob_hash, peer1.clone(), 1024).unwrap();

        // Query should return the peer
        let peers = registry.get_peers_with_blob(&blob_hash);
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].peer_did, peer1);
        assert_eq!(peers[0].size_bytes, 1024);
    }

    #[test]
    fn test_multiple_peers_same_blob() {
        let mut registry = BlobLocationRegistry::new();
        let blob_hash = [2u8; 32];
        let peer1 = create_test_did();
        let peer2 = create_test_did();

        // Multiple peers announce same blob
        registry.announce_blob(blob_hash, peer1.clone(), 1024).unwrap();
        registry.announce_blob(blob_hash, peer2.clone(), 2048).unwrap();

        let peers = registry.get_peers_with_blob(&blob_hash);
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn test_update_existing_announcement() {
        let mut registry = BlobLocationRegistry::new();
        let blob_hash = [3u8; 32];
        let peer = create_test_did();

        // First announcement
        registry.announce_blob(blob_hash, peer.clone(), 1024).unwrap();
        let peers1 = registry.get_peers_with_blob(&blob_hash);
        assert_eq!(peers1.len(), 1);
        assert_eq!(peers1[0].size_bytes, 1024);

        // Update announcement (different size)
        registry.announce_blob(blob_hash, peer.clone(), 2048).unwrap();
        let peers2 = registry.get_peers_with_blob(&blob_hash);
        assert_eq!(peers2.len(), 1); // Still one entry
        assert_eq!(peers2[0].size_bytes, 2048); // Size updated
    }

    #[test]
    fn test_query_multiple_blobs() {
        let mut registry = BlobLocationRegistry::new();
        let blob1 = [1u8; 32];
        let blob2 = [2u8; 32];
        let blob3 = [3u8; 32];
        let peer = create_test_did();

        registry.announce_blob(blob1, peer.clone(), 1024).unwrap();
        registry.announce_blob(blob2, peer.clone(), 2048).unwrap();
        // blob3 not announced

        let result = registry.query_blobs(&[blob1, blob2, blob3]);
        assert_eq!(result.len(), 2); // Only blob1 and blob2
        assert!(result.contains_key(&blob1));
        assert!(result.contains_key(&blob2));
        assert!(!result.contains_key(&blob3));
    }

    #[test]
    fn test_find_peers_with_all() {
        let mut registry = BlobLocationRegistry::new();
        let blob1 = [1u8; 32];
        let blob2 = [2u8; 32];
        let blob3 = [3u8; 32];

        let peer_a = create_test_did();
        let peer_b = create_test_did();
        let peer_c = create_test_did();

        // Peer A has blob1 and blob2
        registry.announce_blob(blob1, peer_a.clone(), 1024).unwrap();
        registry.announce_blob(blob2, peer_a.clone(), 1024).unwrap();

        // Peer B has blob1 and blob3
        registry.announce_blob(blob1, peer_b.clone(), 1024).unwrap();
        registry.announce_blob(blob3, peer_b.clone(), 1024).unwrap();

        // Peer C has only blob1
        registry.announce_blob(blob1, peer_c.clone(), 1024).unwrap();

        // Query for blob1 and blob2
        let result = registry.find_peers_with_all(&[blob1, blob2]);
        assert_eq!(result.len(), 3); // peer_a (2), peer_b (1), peer_c (1)
        assert_eq!(result[0].1, 2); // First peer has 2 blobs
        assert_eq!(result[0].0, peer_a); // peer_a should be first (most matches)
    }

    #[test]
    fn test_remove_peer() {
        let mut registry = BlobLocationRegistry::new();
        let blob1 = [1u8; 32];
        let blob2 = [2u8; 32];
        let peer_a = create_test_did();
        let peer_b = create_test_did();

        registry.announce_blob(blob1, peer_a.clone(), 1024).unwrap();
        registry.announce_blob(blob1, peer_b.clone(), 1024).unwrap();
        registry.announce_blob(blob2, peer_a.clone(), 2048).unwrap();

        assert_eq!(registry.blob_count(), 2);
        assert_eq!(registry.location_count(), 3);

        // Remove peer_a
        registry.remove_peer(&peer_a);

        assert_eq!(registry.blob_count(), 1); // blob2 removed (only had peer_a)
        assert_eq!(registry.location_count(), 1); // Only peer_b for blob1 remains
    }

    #[test]
    fn test_cleanup_expired() {
        let mut registry = BlobLocationRegistry::new();
        let blob_hash = [1u8; 32];
        let peer = create_test_did();

        registry.announce_blob(blob_hash, peer, 1024).unwrap();
        assert_eq!(registry.blob_count(), 1);

        // Fresh announcements won't be cleaned up
        registry.cleanup_expired();
        assert_eq!(registry.blob_count(), 1);
    }

    #[test]
    fn test_empty_query() {
        let registry = BlobLocationRegistry::new();
        let blob_hash = [1u8; 32];

        let peers = registry.get_peers_with_blob(&blob_hash);
        assert_eq!(peers.len(), 0);
    }

    // === New tests for size limits ===

    #[test]
    fn test_reject_blob_too_large() {
        let config = create_test_config();
        let mut registry = BlobLocationRegistry::with_config(config);
        let blob_hash = [1u8; 32];
        let peer = create_test_did();

        // Try to announce a blob larger than max_blob_size (1MB)
        let result = registry.announce_blob(blob_hash, peer, 2 * 1024 * 1024);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BlobRegistryError::BlobTooLarge { .. }
        ));
        assert_eq!(registry.rejected_stats().0, 1);
    }

    #[test]
    fn test_reject_peer_quota_exceeded() {
        let config = create_test_config();
        let mut registry = BlobLocationRegistry::with_config(config);
        let peer = create_test_did();

        // Announce blobs up to the per-peer quota (2MB)
        registry.announce_blob([1u8; 32], peer.clone(), 1024 * 1024).unwrap();
        registry.announce_blob([2u8; 32], peer.clone(), 1024 * 1024).unwrap();

        // This should exceed the quota
        let result = registry.announce_blob([3u8; 32], peer, 1024 * 1024);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BlobRegistryError::PeerQuotaExceeded { .. }
        ));
        assert_eq!(registry.rejected_stats().1, 1);
    }

    #[test]
    fn test_lru_eviction_on_capacity() {
        let config = BlobRegistryConfig {
            max_blob_size: 1024,
            max_registry_size: 2048, // Only room for 2 blobs of 1024 bytes
            max_per_peer_size: 10 * 1024 * 1024,
        };
        let mut registry = BlobLocationRegistry::with_config(config);

        let peer1 = create_test_did();
        let peer2 = create_test_did();
        let peer3 = create_test_did();

        // Add three blobs, each from a different peer (to avoid per-peer quota issues)
        registry.announce_blob([1u8; 32], peer1.clone(), 1024).unwrap();
        registry.announce_blob([2u8; 32], peer2.clone(), 1024).unwrap();

        // Adding a third should evict the oldest
        registry.announce_blob([3u8; 32], peer3.clone(), 1024).unwrap();

        // Should have evicted the first blob
        assert_eq!(registry.evicted_count(), 1);
        assert_eq!(registry.location_count(), 2);

        // First blob should be gone
        let peers1 = registry.get_peers_with_blob(&[1u8; 32]);
        assert_eq!(peers1.len(), 0);

        // Second and third should still be present
        let peers2 = registry.get_peers_with_blob(&[2u8; 32]);
        let peers3 = registry.get_peers_with_blob(&[3u8; 32]);
        assert_eq!(peers2.len(), 1);
        assert_eq!(peers3.len(), 1);
    }

    #[test]
    fn test_total_size_tracking() {
        let mut registry = BlobLocationRegistry::new();
        let peer = create_test_did();

        registry.announce_blob([1u8; 32], peer.clone(), 1024).unwrap();
        assert_eq!(registry.total_size(), 1024);

        registry.announce_blob([2u8; 32], peer.clone(), 2048).unwrap();
        assert_eq!(registry.total_size(), 3072);

        // Update first blob with smaller size
        registry.announce_blob([1u8; 32], peer.clone(), 512).unwrap();
        assert_eq!(registry.total_size(), 2560); // 512 + 2048
    }

    #[test]
    fn test_per_peer_size_tracking() {
        let mut registry = BlobLocationRegistry::new();
        let peer1 = create_test_did();
        let peer2 = create_test_did();

        registry.announce_blob([1u8; 32], peer1.clone(), 1024).unwrap();
        registry.announce_blob([2u8; 32], peer1.clone(), 2048).unwrap();
        registry.announce_blob([3u8; 32], peer2.clone(), 512).unwrap();

        assert_eq!(registry.peer_size(&peer1), 3072);
        assert_eq!(registry.peer_size(&peer2), 512);

        // Remove peer1
        registry.remove_peer(&peer1);
        assert_eq!(registry.peer_size(&peer1), 0);
        assert_eq!(registry.peer_size(&peer2), 512);
    }

    #[test]
    fn test_update_within_quota() {
        // Config: max_blob_size=1MB, max_per_peer_size=2MB
        let config = create_test_config();
        let mut registry = BlobLocationRegistry::with_config(config);
        let peer = create_test_did();

        // Use most of quota with two blobs (1MB + 0.5MB = 1.5MB of 2MB quota)
        registry.announce_blob([1u8; 32], peer.clone(), 1024 * 1024).unwrap();
        registry.announce_blob([2u8; 32], peer.clone(), 512 * 1024).unwrap();

        // Update first blob with same size should work (no net increase)
        let result = registry.announce_blob([1u8; 32], peer.clone(), 1024 * 1024);
        assert!(result.is_ok(), "Updating blob with same size should succeed");

        // Update first blob with smaller size should work
        let result = registry.announce_blob([1u8; 32], peer.clone(), 512 * 1024);
        assert!(result.is_ok(), "Updating blob with smaller size should succeed");
    }

    #[test]
    fn test_with_config() {
        let config = BlobRegistryConfig {
            max_blob_size: 5000,
            max_registry_size: 50000,
            max_per_peer_size: 15000,
        };

        let mut registry = BlobLocationRegistry::with_config(config);
        let peer = create_test_did();

        // Should accept blob up to 5000 bytes
        registry.announce_blob([1u8; 32], peer.clone(), 5000).unwrap();

        // Should reject blob over 5000 bytes
        let result = registry.announce_blob([2u8; 32], peer, 5001);
        assert!(result.is_err());
    }
}
