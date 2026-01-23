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
use crate::types::{ContentHash, GossipMessage};
use anyhow::{Context as _, Result};
use tracing::debug;

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
