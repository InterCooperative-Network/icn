//! Anti-entropy and bloom filter synchronization
//!
//! This module provides bloom filter-based synchronization primitives for
//! detecting and resolving entry differences between gossip peers.
//!
//! # Anti-Entropy Protocol
//!
//! 1. Peers periodically exchange bloom filters via `emit_digest()`
//! 2. Receiving peer checks for missing entries using `find_missing()`
//! 3. Missing entries are requested and transferred
//!
//! # Key Functions
//!
//! - [`GossipActor::get_bloom_filter`] - Get local bloom filter for a topic
//! - [`GossipActor::find_missing`] - Find entries we have that remote doesn't
//! - [`GossipActor::emit_digest`] - Broadcast our state to peers
//! - [`GossipActor::emit_all_digests`] - Broadcast all topic states

use crate::bloom::BloomFilter;
use crate::gossip::GossipActor;
use crate::types::{BloomFilterData, ContentHash, GossipMessage};
use anyhow::{Context as _, Result};
use icn_kernel_api::proofs::BloomProjection;
use tracing::debug;

// ============================================================================
// Cross-link to icn-kernel-api anti-entropy probe (issue #1834)
//
// `AntiEntropyProbe` / `StateDigest` live in `icn-kernel-api` so they remain
// usable by code that does not depend on the gossip layer. The Bloom-filter
// projection in kernel-api is wire-equivalent to this crate's existing
// `BloomFilterData` on the `{bits, num_hashes, size}` shape, plus a
// `hint_count` field that `BloomFilterData` does not carry. These helpers
// make the round-trip explicit so the wire shapes cannot silently drift
// apart.
//
// Note: Rust's orphan rule prevents an `impl From<&BloomFilterData> for
// BloomProjection` here (foreign trait, foreign type), so the kernel→gossip
// conversion is also exposed as a free function for symmetry.
// ============================================================================

/// Convert an existing gossip `BloomFilterData` into a kernel-api
/// `BloomProjection` for inclusion in a `StateDigest::Bloom`.
///
/// `BloomFilterData` does not carry a cardinality estimate; the caller
/// supplies `hint_count`. Typical callers use the same value they already
/// pass into `GossipMessage::Digest { hint_count, .. }`.
pub fn to_bloom_projection(data: &BloomFilterData, hint_count: u32) -> BloomProjection {
    BloomProjection {
        bits: data.bits.clone(),
        num_hashes: data.num_hashes,
        size: data.size,
        hint_count,
    }
}

/// Convert a kernel-api `BloomProjection` back to the gossip layer's
/// `BloomFilterData`. The `hint_count` is intentionally dropped because
/// `BloomFilterData` is a primitive that does not carry it.
pub fn to_bloom_filter_data(proj: &BloomProjection) -> BloomFilterData {
    BloomFilterData {
        bits: proj.bits.clone(),
        num_hashes: proj.num_hashes,
        size: proj.size,
    }
}

impl GossipActor {
    /// Get bloom filter for a topic
    pub fn get_bloom_filter(&self, topic: &str) -> Option<BloomFilter> {
        self.bloom_filters.get(topic).cloned()
    }

    /// Find missing entries based on remote bloom filter
    pub fn find_missing(&self, topic: &str, remote_filter: &BloomFilter) -> Vec<ContentHash> {
        let mut missing = Vec::new();

        if let Some(entries) = self.entries.get(topic) {
            for hash in entries.keys() {
                if !remote_filter.contains(hash) {
                    missing.push(*hash);
                }
            }
        }

        missing
    }

    /// Perform anti-entropy check against a remote peer's bloom filter
    ///
    /// Returns (our bloom filter data, entries we have that remote doesn't)
    pub fn anti_entropy_check(
        &self,
        topic: &str,
        remote_filter_data: &crate::types::BloomFilterData,
    ) -> Result<(crate::types::BloomFilterData, Vec<ContentHash>)> {
        // Get our bloom filter
        let local_filter = self.get_bloom_filter(topic).context("Topic not found")?;

        // Reconstruct remote bloom filter
        let remote_filter = BloomFilter::from_data(remote_filter_data);

        // Find entries we have that remote doesn't
        let missing_on_remote = self.find_missing(topic, &remote_filter);

        // Serialize our bloom filter for sending
        let local_filter_data = local_filter.to_data();

        Ok((local_filter_data, missing_on_remote))
    }

    /// Emit a Digest message for a topic to all peers
    ///
    /// This broadcasts our current state (vector clock + bloom filter) to help peers
    /// discover what entries we have. Peers can then send PullRequests for entries
    /// they're missing.
    pub fn emit_digest(&mut self, topic: &str) -> Result<()> {
        // Check if topic exists
        if !self.topics.contains_key(topic) {
            debug!("Skipping digest for non-existent topic: {}", topic);
            return Ok(());
        }

        // Get entry count for adaptive bloom sizing
        let entry_count = self.entries.get(topic).map(|e| e.len()).unwrap_or(0);

        if entry_count == 0 {
            debug!("Skipping digest for empty topic: {}", topic);
            return Ok(());
        }

        // Build adaptive bloom filter
        let bloom = BloomFilter::new_adaptive(entry_count);
        let mut bloom_with_entries = bloom;

        // Add all entry hashes to bloom filter
        if let Some(entries) = self.entries.get(topic) {
            for hash in entries.keys() {
                bloom_with_entries.insert(hash);
            }
        }

        // Convert to BloomFilterData for transmission
        let bloom_data = bloom_with_entries.to_data();

        // Generate nonce for this digest (using own DID as peer state key)
        let nonce = self
            .peer_sync
            .get_or_create(self.own_did.clone())
            .next_nonce();

        // Create Digest message
        let digest = GossipMessage::Digest {
            topic: topic.to_string(),
            vector: self.clock.clone(),
            bloom: bloom_data,
            hint_count: entry_count as u32,
            nonce,
        };

        // Get topic scope and calculate adaptive fanout (#484)
        // SAFETY: topic existence was checked above via contains_key()
        #[allow(clippy::unwrap_used)]
        let topic_obj = self.topics.get(topic).unwrap();
        let scope = topic_obj.scope;

        // M2 #484: Calculate adaptive fanout based on network size
        let network_size = self.peer_sync.peer_count();
        let fanout = self
            .adaptive_fanout_config
            .calculate_fanout(network_size, &scope);

        // Send with scope-aware peer selection
        debug!(
            "Emitting digest for topic {} ({:?} scope, fanout={}, peers={}): {} entries, nonce={}",
            topic, scope, fanout, network_size, entry_count, nonce
        );
        self.send_message_scoped(scope, fanout, digest);

        // Track metrics
        icn_obs::metrics::gossip::digests_sent_inc();
        icn_obs::metrics::gossip::adaptive_fanout_record(&scope.to_string(), fanout, network_size);

        Ok(())
    }

    /// Emit digests for all topics
    ///
    /// This is typically called periodically by a background task to broadcast
    /// our current state to all peers.
    pub fn emit_all_digests(&mut self) -> Result<()> {
        let topics: Vec<String> = self.topics.keys().cloned().collect();

        for topic in topics {
            self.emit_digest(&topic)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod kernel_api_cross_link_tests {
    use super::*;
    use crate::bloom::BloomFilter;

    fn populated_bloom_data() -> BloomFilterData {
        let mut filter = BloomFilter::new_adaptive(4);
        for byte in 1..=4u8 {
            let mut h = [0u8; 32];
            h[0] = byte;
            filter.insert(&h);
        }
        filter.to_data()
    }

    #[test]
    fn bloom_projection_roundtrip_preserves_bits() {
        let original = populated_bloom_data();
        let projection = to_bloom_projection(&original, 4);
        assert_eq!(projection.bits, original.bits);
        assert_eq!(projection.num_hashes, original.num_hashes);
        assert_eq!(projection.size, original.size);
        assert_eq!(projection.hint_count, 4);

        let restored = to_bloom_filter_data(&projection);
        assert_eq!(restored.bits, original.bits);
        assert_eq!(restored.num_hashes, original.num_hashes);
        assert_eq!(restored.size, original.size);
    }

    #[test]
    fn bloom_projection_roundtrip_preserves_membership() {
        // Round-tripping through the kernel-api projection must not change
        // which content hashes the filter recognizes — this is the
        // load-bearing property of the cross-link.
        let original = populated_bloom_data();
        let projection = to_bloom_projection(&original, 4);
        let restored = to_bloom_filter_data(&projection);

        let original_filter = BloomFilter::from_data(&original);
        let restored_filter = BloomFilter::from_data(&restored);

        for byte in 1..=4u8 {
            let mut h = [0u8; 32];
            h[0] = byte;
            assert!(original_filter.contains(&h));
            assert!(restored_filter.contains(&h));
        }
    }

    #[test]
    fn bloom_projection_hint_count_is_caller_supplied() {
        // BloomFilterData does not carry hint_count, so the conversion is
        // not bidirectional on that field — the gossip→kernel-api direction
        // accepts an explicit hint from the caller and the reverse direction
        // drops it.
        let data = populated_bloom_data();
        let p_zero = to_bloom_projection(&data, 0);
        let p_ten = to_bloom_projection(&data, 10);
        assert_eq!(p_zero.bits, p_ten.bits);
        assert_ne!(p_zero.hint_count, p_ten.hint_count);
        // Reverse drops the hint.
        let d_zero = to_bloom_filter_data(&p_zero);
        let d_ten = to_bloom_filter_data(&p_ten);
        assert_eq!(d_zero.bits, d_ten.bits);
    }
}
