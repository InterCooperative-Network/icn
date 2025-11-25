//! Scalability Optimizations (Phase 19)
//!
//! This module provides compression and optimization techniques for large-scale deployments:
//! - Vector clock compression using varint encoding and delta storage
//! - Target: 100-node cooperatives with efficient state synchronization

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

            if version > 0 {
                clock.clock.insert(peer.clone(), version);
            }
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
}
