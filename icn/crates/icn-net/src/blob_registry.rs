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
//! - **Gossip integration**: Peers announce blob availability via gossip protocol
//! - **Query API**: Executors query registry to find data-local placement
//!
//! ## Usage
//!
//! ```rust,ignore
//! use icn_net::BlobLocationRegistry;
//! use icn_identity::Did;
//!
//! let mut registry = BlobLocationRegistry::new();
//!
//! // Peer announces blob availability
//! let blob_hash = [0u8; 32];
//! let peer_did: Did = /* valid DID */;
//! registry.announce_blob(blob_hash, peer_did.clone(), 1024);
//!
//! // Query peers with blob
//! let peers = registry.get_peers_with_blob(&blob_hash);
//! assert_eq!(peers.len(), 1);
//! ```

use icn_gossip::types::ContentHash;
use icn_identity::Did;
use std::collections::HashMap;
use std::time::{Duration, Instant};

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
    /// Map from blob hash to list of peers that have it
    locations: HashMap<ContentHash, Vec<BlobLocation>>,
}

impl BlobLocationRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            locations: HashMap::new(),
        }
    }

    /// Announce that a peer has a blob
    ///
    /// This updates the registry with the peer's location information.
    /// If the peer already announced this blob, updates the timestamp.
    pub fn announce_blob(&mut self, blob_hash: ContentHash, peer_did: Did, size_bytes: u64) {
        let location = BlobLocation::new(peer_did.clone(), size_bytes);

        let locations = self.locations.entry(blob_hash).or_default();

        // Check if peer already announced this blob
        if let Some(existing) = locations.iter_mut().find(|loc| loc.peer_did == peer_did) {
            // Update existing entry
            *existing = location;
        } else {
            // Add new entry
            locations.push(location);
        }
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
        // Remove expired locations from each blob's list
        for locations in self.locations.values_mut() {
            locations.retain(|loc| !loc.is_expired());
        }

        // Remove blobs with no valid locations
        self.locations.retain(|_, locs| !locs.is_empty());
    }

    /// Get total number of tracked blobs
    pub fn blob_count(&self) -> usize {
        self.locations.len()
    }

    /// Get total number of location entries (before cleanup)
    pub fn location_count(&self) -> usize {
        self.locations.values().map(|v| v.len()).sum()
    }

    /// Remove all locations for a specific peer (e.g., when peer disconnects)
    pub fn remove_peer(&mut self, peer_did: &Did) {
        for locations in self.locations.values_mut() {
            locations.retain(|loc| &loc.peer_did != peer_did);
        }

        // Remove blobs with no remaining locations
        self.locations.retain(|_, locs| !locs.is_empty());
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

    #[test]
    fn test_announce_and_query_blob() {
        let mut registry = BlobLocationRegistry::new();
        let blob_hash = [1u8; 32];
        let peer1 = create_test_did();

        // Announce blob
        registry.announce_blob(blob_hash, peer1.clone(), 1024);

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
        registry.announce_blob(blob_hash, peer1.clone(), 1024);
        registry.announce_blob(blob_hash, peer2.clone(), 2048);

        let peers = registry.get_peers_with_blob(&blob_hash);
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn test_update_existing_announcement() {
        let mut registry = BlobLocationRegistry::new();
        let blob_hash = [3u8; 32];
        let peer = create_test_did();

        // First announcement
        registry.announce_blob(blob_hash, peer.clone(), 1024);
        let peers1 = registry.get_peers_with_blob(&blob_hash);
        assert_eq!(peers1.len(), 1);
        assert_eq!(peers1[0].size_bytes, 1024);

        // Update announcement (different size)
        registry.announce_blob(blob_hash, peer.clone(), 2048);
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

        registry.announce_blob(blob1, peer.clone(), 1024);
        registry.announce_blob(blob2, peer.clone(), 2048);
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
        registry.announce_blob(blob1, peer_a.clone(), 1024);
        registry.announce_blob(blob2, peer_a.clone(), 1024);

        // Peer B has blob1 and blob3
        registry.announce_blob(blob1, peer_b.clone(), 1024);
        registry.announce_blob(blob3, peer_b.clone(), 1024);

        // Peer C has only blob1
        registry.announce_blob(blob1, peer_c.clone(), 1024);

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

        registry.announce_blob(blob1, peer_a.clone(), 1024);
        registry.announce_blob(blob1, peer_b.clone(), 1024);
        registry.announce_blob(blob2, peer_a.clone(), 2048);

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

        registry.announce_blob(blob_hash, peer, 1024);
        assert_eq!(registry.blob_count(), 1);

        // Manually mark as expired (can't wait 24 hours in test)
        // In real usage, expired entries would be filtered by get_peers_with_blob()
        // and removed by cleanup_expired()

        // This test demonstrates the API exists
        registry.cleanup_expired();
        // Fresh announcements won't be cleaned up
        assert_eq!(registry.blob_count(), 1);
    }

    #[test]
    fn test_empty_query() {
        let registry = BlobLocationRegistry::new();
        let blob_hash = [1u8; 32];

        let peers = registry.get_peers_with_blob(&blob_hash);
        assert_eq!(peers.len(), 0);
    }
}
