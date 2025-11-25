//! Scalability Optimizations (Phase 19)
//!
//! This module provides compression and optimization techniques for large-scale deployments:
//! - Vector clock compression using varint encoding and delta storage
//! - Topic sharding for large topics (>10K entries)
//! - Target: 100-node cooperatives with efficient state synchronization

use crate::bloom::BloomFilter;
use crate::types::{ContentHash, GossipEntry};
use crate::vector_clock::VectorClock;
use anyhow::{anyhow, Result};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Varint-encoded integer (variable-length encoding)
///
/// Uses 1-10 bytes to encode u64 values, with small values using fewer bytes:
/// - 0-127: 1 byte
/// - 128-16383: 2 bytes
/// - 16384-2097151: 3 bytes
/// - etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarInt(pub u64);

impl VarInt {
    /// Create a new VarInt from a u64
    pub fn new(value: u64) -> Self {
        VarInt(value)
    }

    /// Encode to bytes using varint encoding
    pub fn encode(&self) -> Vec<u8> {
        let mut value = self.0;
        let mut bytes = Vec::new();

        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;

            if value != 0 {
                byte |= 0x80; // Set continuation bit
            }

            bytes.push(byte);

            if value == 0 {
                break;
            }
        }

        bytes
    }

    /// Decode from bytes using varint encoding
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize)> {
        let mut value = 0u64;
        let mut shift = 0;
        let mut bytes_read = 0;

        for &byte in bytes {
            bytes_read += 1;

            if shift >= 64 {
                return Err(anyhow!("Varint overflow: too many continuation bytes"));
            }

            value |= ((byte & 0x7F) as u64) << shift;
            shift += 7;

            if byte & 0x80 == 0 {
                // No continuation bit - done
                return Ok((VarInt(value), bytes_read));
            }

            if bytes_read >= 10 {
                return Err(anyhow!("Varint overflow: max 10 bytes for u64"));
            }
        }

        Err(anyhow!("Varint incomplete: unexpected end of input"))
    }

    /// Get the u64 value
    pub fn value(&self) -> u64 {
        self.0
    }
}

/// Compressed vector clock using varint encoding and delta storage
///
/// Reduces memory usage by:
/// 1. Storing only deltas from a baseline version (most peers are at similar versions)
/// 2. Using varint encoding for sequence numbers (small numbers use fewer bytes)
/// 3. Omitting zero deltas (peers at baseline version)
///
/// Typical compression: ~32 bytes/peer → ~8 bytes/peer for common cases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedVectorClock {
    /// Baseline version (most peers are near this value)
    baseline_version: u64,

    /// Deltas from baseline (only non-zero deltas stored)
    /// Positive delta: peer is ahead of baseline
    /// Negative delta: peer is behind baseline (stored as i64)
    deltas: HashMap<Did, i64>,
}

impl CompressedVectorClock {
    /// Create a new compressed vector clock with a baseline
    pub fn new(baseline_version: u64) -> Self {
        CompressedVectorClock {
            baseline_version,
            deltas: HashMap::new(),
        }
    }

    /// Compress a vector clock using median as baseline
    pub fn from_vector_clock(clock: &VectorClock) -> Self {
        if clock.clock.is_empty() {
            return CompressedVectorClock::new(0);
        }

        // Calculate baseline as median of all values
        let mut values: Vec<u64> = clock.clock.values().copied().collect();
        values.sort_unstable();
        let baseline_version = values[values.len() / 2];

        // Calculate deltas from baseline
        let mut deltas = HashMap::new();
        for (did, &version) in &clock.clock {
            let delta = version as i64 - baseline_version as i64;
            if delta != 0 {
                deltas.insert(did.clone(), delta);
            }
        }

        CompressedVectorClock {
            baseline_version,
            deltas,
        }
    }

    /// Decompress back to a full vector clock
    pub fn to_vector_clock(&self, all_peers: &[Did]) -> VectorClock {
        let mut clock = VectorClock::new();

        for peer in all_peers {
            let version = if let Some(&delta) = self.deltas.get(peer) {
                (self.baseline_version as i64 + delta).max(0) as u64
            } else {
                self.baseline_version
            };

            // Insert all peers, even with version 0 (fixes roundtrip consistency)
            clock.clock.insert(peer.clone(), version);
        }

        clock
    }

    /// Set a peer's version (stores delta from baseline)
    pub fn set(&mut self, peer: Did, version: u64) {
        let delta = version as i64 - self.baseline_version as i64;
        if delta != 0 {
            self.deltas.insert(peer, delta);
        } else {
            self.deltas.remove(&peer);
        }
    }

    /// Get a peer's version
    pub fn get(&self, peer: &Did) -> u64 {
        if let Some(&delta) = self.deltas.get(peer) {
            (self.baseline_version as i64 + delta).max(0) as u64
        } else {
            self.baseline_version
        }
    }

    /// Increment a peer's version
    pub fn increment(&mut self, peer: &Did) {
        let current = self.get(peer);
        self.set(peer.clone(), current + 1);
    }

    /// Get the number of stored deltas (measure of sparsity)
    pub fn delta_count(&self) -> usize {
        self.deltas.len()
    }

    /// Estimate compressed size in bytes
    pub fn estimate_size(&self) -> usize {
        let mut size = 8; // baseline_version (u64)

        for &delta in self.deltas.values() {
            // Estimate varint size for delta (typically 1-2 bytes)
            let abs_delta = delta.unsigned_abs();
            let varint_size = if abs_delta < 128 {
                1
            } else if abs_delta < 16384 {
                2
            } else if abs_delta < 2097152 {
                3
            } else {
                4
            };
            size += 50 + varint_size; // DID (~50 bytes) + varint delta
        }

        size
    }

    /// Calculate compression ratio compared to uncompressed vector clock
    pub fn compression_ratio(&self, peer_count: usize) -> f64 {
        let uncompressed_size = peer_count * (50 + 8); // DID + u64 per peer
        let compressed_size = self.estimate_size();
        uncompressed_size as f64 / compressed_size as f64
    }
}

impl Default for CompressedVectorClock {
    fn default() -> Self {
        Self::new(0)
    }
}

// ============================================================================
// Topic Sharding (Phase 19 Week 2)
// ============================================================================

/// Number of shards for topic partitioning (must be power of 2)
const SHARD_COUNT: usize = 256;

/// Minimum entries before sharding is beneficial
const SHARD_THRESHOLD: usize = 1000;

/// Topic shard containing a subset of entries
///
/// Large topics (>10K entries) are partitioned into 256 shards based on
/// content hash for efficient anti-entropy. Each shard has its own Bloom
/// filter, reducing sync overhead from O(n) to O(n/256).
#[derive(Clone)]
pub struct TopicShard {
    /// Shard ID (0-255)
    pub shard_id: usize,

    /// Entries in this shard (hash -> entry)
    pub entries: HashMap<ContentHash, GossipEntry>,

    /// Bloom filter for this shard (for anti-entropy)
    pub bloom: BloomFilter,
}

impl TopicShard {
    /// Create a new empty topic shard
    pub fn new(shard_id: usize) -> Self {
        Self {
            shard_id,
            entries: HashMap::new(),
            bloom: BloomFilter::new(10000, 0.01), // 10K entries, 1% FP rate
        }
    }

    /// Add an entry to this shard
    pub fn insert(&mut self, hash: ContentHash, entry: GossipEntry) {
        self.bloom.insert(&hash);
        self.entries.insert(hash, entry);
    }

    /// Check if this shard contains a hash
    pub fn contains(&self, hash: &ContentHash) -> bool {
        self.bloom.contains(hash)
    }

    /// Get an entry from this shard
    pub fn get(&self, hash: &ContentHash) -> Option<&GossipEntry> {
        self.entries.get(hash)
    }

    /// Remove an entry from this shard
    pub fn remove(&mut self, hash: &ContentHash) -> Option<GossipEntry> {
        // Note: Bloom filter cannot remove (false positives acceptable)
        self.entries.remove(hash)
    }

    /// Get the number of entries in this shard
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if this shard is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries in this shard
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bloom = BloomFilter::new(10000, 0.01); // Reset bloom filter
    }
}

/// Sharded topic storage for large topics
///
/// Distributes entries across 256 shards based on content hash.
/// Each shard has its own Bloom filter for efficient anti-entropy.
///
/// Benefits:
/// - Anti-entropy overhead: O(n) → O(n/256)
/// - Memory locality: Hot shards in cache
/// - Parallel processing: Independent shards
#[derive(Clone)]
pub struct ShardedTopic {
    /// Topic name
    pub name: String,

    /// All shards (256 shards)
    pub shards: Vec<TopicShard>,

    /// Total number of entries across all shards
    pub total_entries: usize,

    /// Whether sharding is enabled (only enable for large topics)
    pub sharding_enabled: bool,
}

impl ShardedTopic {
    /// Create a new sharded topic
    pub fn new(name: String) -> Self {
        let shards = (0..SHARD_COUNT)
            .map(|i| TopicShard::new(i))
            .collect();

        Self {
            name,
            shards,
            total_entries: 0,
            sharding_enabled: false,
        }
    }

    /// Determine which shard a content hash belongs to
    ///
    /// Uses first byte of hash modulo 256 for even distribution
    pub fn shard_for_hash(hash: &ContentHash) -> usize {
        hash[0] as usize % SHARD_COUNT
    }

    /// Insert an entry into the appropriate shard
    pub fn insert(&mut self, hash: ContentHash, entry: GossipEntry) {
        let shard_id = Self::shard_for_hash(&hash);
        self.shards[shard_id].insert(hash, entry);
        self.total_entries += 1;

        // Enable sharding if we exceed threshold
        if !self.sharding_enabled && self.total_entries >= SHARD_THRESHOLD {
            self.sharding_enabled = true;
            tracing::info!(
                "Enabled sharding for topic {} ({} entries)",
                self.name,
                self.total_entries
            );
            icn_obs::metrics::scalability::topic_sharding_enabled_inc();
        }

        icn_obs::metrics::scalability::sharded_topic_size_set(&self.name, self.total_entries);
    }

    /// Check if a hash exists in any shard
    pub fn contains(&self, hash: &ContentHash) -> bool {
        let shard_id = Self::shard_for_hash(hash);
        self.shards[shard_id].contains(hash)
    }

    /// Get an entry from the appropriate shard
    pub fn get(&self, hash: &ContentHash) -> Option<&GossipEntry> {
        let shard_id = Self::shard_for_hash(hash);
        self.shards[shard_id].get(hash)
    }

    /// Remove an entry from the appropriate shard
    pub fn remove(&mut self, hash: &ContentHash) -> Option<GossipEntry> {
        let shard_id = Self::shard_for_hash(hash);
        if let Some(entry) = self.shards[shard_id].remove(hash) {
            self.total_entries = self.total_entries.saturating_sub(1);
            icn_obs::metrics::scalability::sharded_topic_size_set(&self.name, self.total_entries);
            Some(entry)
        } else {
            None
        }
    }

    /// Get all entries across all shards (expensive - use sparingly)
    pub fn all_entries(&self) -> Vec<&GossipEntry> {
        self.shards
            .iter()
            .flat_map(|shard| shard.entries.values())
            .collect()
    }

    /// Get entries from a specific shard (for anti-entropy)
    pub fn shard_entries(&self, shard_id: usize) -> Option<Vec<&GossipEntry>> {
        if shard_id >= SHARD_COUNT {
            return None;
        }
        Some(self.shards[shard_id].entries.values().collect())
    }

    /// Get Bloom filter for a specific shard (for anti-entropy)
    pub fn shard_bloom(&self, shard_id: usize) -> Option<&BloomFilter> {
        if shard_id >= SHARD_COUNT {
            return None;
        }
        Some(&self.shards[shard_id].bloom)
    }

    /// Get total number of entries
    pub fn len(&self) -> usize {
        self.total_entries
    }

    /// Check if topic is empty
    pub fn is_empty(&self) -> bool {
        self.total_entries == 0
    }

    /// Get shard statistics (for monitoring)
    pub fn shard_stats(&self) -> ShardStats {
        let shard_sizes: Vec<usize> = self.shards.iter().map(|s| s.len()).collect();
        let min_size = *shard_sizes.iter().min().unwrap_or(&0);
        let max_size = *shard_sizes.iter().max().unwrap_or(&0);
        let avg_size = if !shard_sizes.is_empty() {
            shard_sizes.iter().sum::<usize>() as f64 / shard_sizes.len() as f64
        } else {
            0.0
        };

        // Count non-empty shards
        let active_shards = shard_sizes.iter().filter(|&&size| size > 0).count();

        ShardStats {
            total_entries: self.total_entries,
            total_shards: SHARD_COUNT,
            active_shards,
            min_shard_size: min_size,
            max_shard_size: max_size,
            avg_shard_size: avg_size,
            sharding_enabled: self.sharding_enabled,
        }
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        for shard in &mut self.shards {
            shard.clear();
        }
        self.total_entries = 0;
        self.sharding_enabled = false;
        icn_obs::metrics::scalability::sharded_topic_size_set(&self.name, 0);
    }
}

/// Statistics about shard distribution
#[derive(Debug, Clone, PartialEq)]
pub struct ShardStats {
    pub total_entries: usize,
    pub total_shards: usize,
    pub active_shards: usize,
    pub min_shard_size: usize,
    pub max_shard_size: usize,
    pub avg_shard_size: f64,
    pub sharding_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_varint_encode_decode() {
        let test_cases = vec![
            0u64,
            1,
            127,
            128,
            255,
            256,
            16383,
            16384,
            65535,
            1_000_000,
            u64::MAX,
        ];

        for value in test_cases {
            let varint = VarInt::new(value);
            let encoded = varint.encode();
            let (decoded, bytes_read) = VarInt::decode(&encoded).unwrap();

            assert_eq!(decoded.value(), value, "Failed for value {}", value);
            assert_eq!(bytes_read, encoded.len());

            // Verify size efficiency
            if value < 128 {
                assert_eq!(encoded.len(), 1);
            } else if value < 16384 {
                assert_eq!(encoded.len(), 2);
            }
        }
    }

    #[test]
    fn test_varint_overflow() {
        // Test too many continuation bytes
        let bad_bytes = vec![0xFF; 11]; // 11 bytes with all continuation bits set
        let result = VarInt::decode(&bad_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_varint_incomplete() {
        // Test incomplete varint (ends with continuation bit set)
        let bad_bytes = vec![0xFF]; // Continuation bit set but no more bytes
        let result = VarInt::decode(&bad_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_compressed_vector_clock_basic() {
        let mut compressed = CompressedVectorClock::new(100);
        let peer1 = make_test_did();
        let peer2 = make_test_did();

        // Set versions
        compressed.set(peer1.clone(), 105);
        compressed.set(peer2.clone(), 95);

        // Verify retrieval
        assert_eq!(compressed.get(&peer1), 105);
        assert_eq!(compressed.get(&peer2), 95);

        // Verify deltas are stored
        assert_eq!(compressed.delta_count(), 2);
    }

    #[test]
    fn test_compressed_vector_clock_baseline() {
        let mut compressed = CompressedVectorClock::new(100);
        let peer = make_test_did();

        // Set to baseline - should not store delta
        compressed.set(peer.clone(), 100);
        assert_eq!(compressed.delta_count(), 0);

        // Retrieve should return baseline
        assert_eq!(compressed.get(&peer), 100);
    }

    #[test]
    fn test_compressed_vector_clock_increment() {
        let mut compressed = CompressedVectorClock::new(50);
        let peer = make_test_did();

        compressed.increment(&peer);
        assert_eq!(compressed.get(&peer), 51);

        compressed.increment(&peer);
        assert_eq!(compressed.get(&peer), 52);
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let peer1 = make_test_did();
        let peer2 = make_test_did();
        let peer3 = make_test_did();

        // Create original vector clock
        let mut original = VectorClock::new();
        original.increment(&peer1);
        original.increment(&peer1);
        original.increment(&peer2);
        original.increment(&peer3);
        original.increment(&peer3);
        original.increment(&peer3);

        // Compress
        let compressed = CompressedVectorClock::from_vector_clock(&original);

        // Decompress with all peers
        let all_peers = vec![peer1.clone(), peer2.clone(), peer3.clone()];
        let decompressed = compressed.to_vector_clock(&all_peers);

        // Verify all values match
        assert_eq!(decompressed.get(&peer1), 2);
        assert_eq!(decompressed.get(&peer2), 1);
        assert_eq!(decompressed.get(&peer3), 3);
    }

    #[test]
    fn test_roundtrip_with_version_zero() {
        // Regression test for bug where peers with version 0 were filtered out
        // during decompression when baseline_version = 0
        let peer1 = make_test_did();
        let peer2 = make_test_did();
        let peer3 = make_test_did();

        // Create clock where some peers have version 0
        let mut original = VectorClock::new();
        // peer1 stays at 0 (never incremented)
        original.clock.insert(peer1.clone(), 0);
        // peer2 at version 0 (explicitly set)
        original.clock.insert(peer2.clone(), 0);
        // peer3 at version 0
        original.clock.insert(peer3.clone(), 0);

        // Compress (baseline should be 0, median of [0, 0, 0])
        let compressed = CompressedVectorClock::from_vector_clock(&original);
        assert_eq!(compressed.baseline_version, 0);

        // Decompress
        let all_peers = vec![peer1.clone(), peer2.clone(), peer3.clone()];
        let decompressed = compressed.to_vector_clock(&all_peers);

        // All peers should be present with version 0
        assert_eq!(decompressed.get(&peer1), 0, "peer1 should have version 0");
        assert_eq!(decompressed.get(&peer2), 0, "peer2 should have version 0");
        assert_eq!(decompressed.get(&peer3), 0, "peer3 should have version 0");
        assert_eq!(decompressed.clock.len(), 3, "all 3 peers should be present");
    }

    #[test]
    fn test_roundtrip_with_mixed_zero_and_nonzero() {
        // Test roundtrip when baseline is 0 but some peers have non-zero versions
        let peer1 = make_test_did();
        let peer2 = make_test_did();
        let peer3 = make_test_did();

        let mut original = VectorClock::new();
        original.clock.insert(peer1.clone(), 0); // At baseline
        original.clock.insert(peer2.clone(), 0); // At baseline
        original.clock.insert(peer3.clone(), 5); // Above baseline

        // Compress (baseline should be 0, median of [0, 0, 5])
        let compressed = CompressedVectorClock::from_vector_clock(&original);
        assert_eq!(compressed.baseline_version, 0);

        // Decompress
        let all_peers = vec![peer1.clone(), peer2.clone(), peer3.clone()];
        let decompressed = compressed.to_vector_clock(&all_peers);

        // All peers should be present with correct versions
        assert_eq!(decompressed.get(&peer1), 0, "peer1 should have version 0");
        assert_eq!(decompressed.get(&peer2), 0, "peer2 should have version 0");
        assert_eq!(decompressed.get(&peer3), 5, "peer3 should have version 5");
        assert_eq!(decompressed.clock.len(), 3, "all 3 peers should be present");
    }

    #[test]
    fn test_compression_with_similar_values() {
        let peers: Vec<Did> = (0..50).map(|_| make_test_did()).collect();

        // Create vector clock with values tightly clustered around 100
        // Most peers at exactly 100, a few at 99 or 101
        let mut clock = VectorClock::new();
        for (i, peer) in peers.iter().enumerate() {
            let target = if i < 40 {
                100 // Most peers at exactly 100
            } else if i < 45 {
                99 // A few at 99
            } else {
                101 // A few at 101
            };
            for _ in 0..target {
                clock.increment(peer);
            }
        }

        // Compress
        let compressed = CompressedVectorClock::from_vector_clock(&clock);

        // With tightly clustered values, most deltas should be zero (baseline = median = 100)
        // Only 10 peers should have non-zero deltas (5 at 99, 5 at 101)
        assert!(
            compressed.delta_count() <= 15,
            "Too many deltas: {}",
            compressed.delta_count()
        );

        // Estimate compression ratio
        let ratio = compressed.compression_ratio(50);
        println!("Compression ratio: {:.2}x", ratio);

        // With 50 peers and only ~10 deltas, should achieve good compression (>5x)
        assert!(ratio > 5.0, "Compression ratio {:.2} is too low", ratio);
    }

    #[test]
    fn test_compression_with_sparse_updates() {
        let peers: Vec<Did> = (0..100).map(|_| make_test_did()).collect();

        // Create vector clock with sparse updates (most peers at baseline)
        let mut clock = VectorClock::new();
        for peer in peers.iter().take(10) {
            clock.increment(peer); // Only 10 peers have updates
        }

        // Compress
        let compressed = CompressedVectorClock::from_vector_clock(&clock);

        // Should store very few deltas
        assert!(compressed.delta_count() <= 10);

        // Should achieve excellent compression
        let ratio = compressed.compression_ratio(100);
        println!("Sparse compression ratio: {:.2}x", ratio);
        assert!(ratio > 10.0, "Sparse compression ratio {:.2} is too low", ratio);
    }

    #[test]
    fn test_estimate_size() {
        let peers: Vec<Did> = (0..50).map(|_| make_test_did()).collect();

        // Create vector clock
        let mut clock = VectorClock::new();
        for peer in peers.iter() {
            clock.increment(peer);
        }

        // Compress
        let compressed = CompressedVectorClock::from_vector_clock(&clock);

        // Estimate size
        let estimated_size = compressed.estimate_size();
        println!("Estimated compressed size: {} bytes", estimated_size);
        println!("Uncompressed size: {} bytes", 50 * (50 + 8));

        // Compressed size should be significantly smaller
        let uncompressed_size = 50 * (50 + 8);
        assert!(
            estimated_size < uncompressed_size / 3,
            "Compression not effective enough"
        );
    }

    #[test]
    fn test_negative_delta_handling() {
        let mut compressed = CompressedVectorClock::new(100);
        let peer = make_test_did();

        // Set version below baseline
        compressed.set(peer.clone(), 50);
        assert_eq!(compressed.get(&peer), 50);

        // Verify delta is stored as negative
        assert_eq!(compressed.delta_count(), 1);
    }

    #[test]
    fn test_zero_version_handling() {
        let mut compressed = CompressedVectorClock::new(10);
        let peer = make_test_did();

        // Set version to 0
        compressed.set(peer.clone(), 0);

        // Decompress
        let decompressed = compressed.to_vector_clock(&[peer.clone()]);

        // Should handle zero version correctly
        assert_eq!(decompressed.get(&peer), 0);
    }

    // ========================================================================
    // Topic Sharding Tests
    // ========================================================================

    fn make_test_entry(hash: ContentHash) -> GossipEntry {
        GossipEntry {
            hash,
            author: make_test_did(),
            clock: VectorClock::new(),
            topic: "test".to_string(),
            data: b"test data".to_vec(),
            compressed: false,
            timestamp: 0,
            replica_offered: None,
        }
    }

    fn make_test_hash(value: u8) -> ContentHash {
        let mut hash = [0u8; 32];
        hash[0] = value; // First byte determines shard
        hash
    }

    #[test]
    fn test_shard_for_hash() {
        // Test hash distribution
        let hash0 = make_test_hash(0);
        let hash1 = make_test_hash(1);
        let hash255 = make_test_hash(255);

        assert_eq!(ShardedTopic::shard_for_hash(&hash0), 0);
        assert_eq!(ShardedTopic::shard_for_hash(&hash1), 1);
        assert_eq!(ShardedTopic::shard_for_hash(&hash255), 255);
    }

    #[test]
    fn test_sharded_topic_basic() {
        let mut topic = ShardedTopic::new("test".to_string());
        let hash = make_test_hash(42);
        let entry = make_test_entry(hash);

        // Insert entry
        topic.insert(hash, entry);

        assert_eq!(topic.len(), 1);
        assert!(topic.contains(&hash));
        assert!(topic.get(&hash).is_some());
    }

    #[test]
    fn test_sharded_topic_distribution() {
        let mut topic = ShardedTopic::new("test".to_string());

        // Insert 256 entries (one per shard)
        for i in 0..256 {
            let hash = make_test_hash(i as u8);
            let entry = make_test_entry(hash);
            topic.insert(hash, entry);
        }

        assert_eq!(topic.len(), 256);

        // Each shard should have exactly 1 entry
        let stats = topic.shard_stats();
        assert_eq!(stats.active_shards, 256);
        assert_eq!(stats.min_shard_size, 1);
        assert_eq!(stats.max_shard_size, 1);
        assert_eq!(stats.avg_shard_size, 1.0);
    }

    #[test]
    fn test_sharded_topic_remove() {
        let mut topic = ShardedTopic::new("test".to_string());
        let hash = make_test_hash(100);
        let entry = make_test_entry(hash);

        topic.insert(hash, entry);
        assert_eq!(topic.len(), 1);

        let removed = topic.remove(&hash);
        assert!(removed.is_some());
        assert_eq!(topic.len(), 0);

        // Note: Bloom filter will still contain the hash (cannot remove from Bloom filter)
        // This is expected behavior - false positives are acceptable
        assert!(topic.get(&hash).is_none()); // Entry is actually removed
    }

    #[test]
    fn test_sharded_topic_threshold() {
        let mut topic = ShardedTopic::new("test".to_string());

        // Should not enable sharding below threshold
        for i in 0..999 {
            let hash = make_test_hash((i % 256) as u8);
            let entry = make_test_entry(hash);
            topic.insert(hash, entry);
        }
        assert!(!topic.sharding_enabled);

        // Should enable sharding at threshold
        let hash = make_test_hash(123);
        let entry = make_test_entry(hash);
        topic.insert(hash, entry);
        assert!(topic.sharding_enabled);
        assert_eq!(topic.len(), 1000);
    }

    #[test]
    fn test_sharded_topic_shard_entries() {
        let mut topic = ShardedTopic::new("test".to_string());

        // Insert 10 entries into shard 0
        for i in 0..10 {
            let mut hash = make_test_hash(0);
            hash[1] = i; // Vary second byte to keep in same shard
            let entry = make_test_entry(hash);
            topic.insert(hash, entry);
        }

        // Get shard 0 entries
        let shard0_entries = topic.shard_entries(0).unwrap();
        assert_eq!(shard0_entries.len(), 10);

        // Other shards should be empty
        let shard1_entries = topic.shard_entries(1).unwrap();
        assert_eq!(shard1_entries.len(), 0);
    }

    #[test]
    fn test_sharded_topic_bloom_filter() {
        let mut topic = ShardedTopic::new("test".to_string());
        let hash = make_test_hash(50);
        let entry = make_test_entry(hash);

        topic.insert(hash, entry);

        // Bloom filter should contain hash
        let shard_id = ShardedTopic::shard_for_hash(&hash);
        let bloom = topic.shard_bloom(shard_id).unwrap();
        assert!(bloom.contains(&hash));

        // Other shards should not contain it
        let other_shard = (shard_id + 1) % 256;
        let other_bloom = topic.shard_bloom(other_shard).unwrap();
        assert!(!other_bloom.contains(&hash));
    }

    #[test]
    fn test_sharded_topic_clear() {
        let mut topic = ShardedTopic::new("test".to_string());

        // Add 100 entries
        for i in 0..100 {
            let hash = make_test_hash((i % 256) as u8);
            let entry = make_test_entry(hash);
            topic.insert(hash, entry);
        }

        assert_eq!(topic.len(), 100);

        // Clear topic
        topic.clear();

        assert_eq!(topic.len(), 0);
        assert!(topic.is_empty());
        assert!(!topic.sharding_enabled);
    }

    #[test]
    fn test_sharded_topic_all_entries() {
        let mut topic = ShardedTopic::new("test".to_string());

        // Insert 50 entries
        for i in 0..50 {
            let hash = make_test_hash((i % 256) as u8);
            let entry = make_test_entry(hash);
            topic.insert(hash, entry);
        }

        let all = topic.all_entries();
        assert_eq!(all.len(), 50);
    }

    #[test]
    fn test_shard_stats() {
        let mut topic = ShardedTopic::new("test".to_string());

        // Insert 512 entries (2 per shard for uniform distribution)
        for i in 0..512 {
            let mut hash = make_test_hash((i % 256) as u8);
            hash[1] = (i / 256) as u8; // Vary second byte to avoid hash collisions
            let entry = make_test_entry(hash);
            topic.insert(hash, entry);
        }

        let stats = topic.shard_stats();
        assert_eq!(stats.total_entries, 512);
        assert_eq!(stats.total_shards, 256);
        assert_eq!(stats.active_shards, 256); // All shards have entries
        assert_eq!(stats.min_shard_size, 2);
        assert_eq!(stats.max_shard_size, 2);
        assert_eq!(stats.avg_shard_size, 2.0); // Exactly 2 entries per shard
    }

    #[test]
    fn test_shard_stats_empty() {
        let topic = ShardedTopic::new("test".to_string());
        let stats = topic.shard_stats();

        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.active_shards, 0);
        assert_eq!(stats.min_shard_size, 0);
        assert_eq!(stats.max_shard_size, 0);
        assert_eq!(stats.avg_shard_size, 0.0);
    }
}
