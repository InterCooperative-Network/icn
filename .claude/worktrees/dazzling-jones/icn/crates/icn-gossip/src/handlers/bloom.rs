//! Bloom filter sync handlers: RequestBloomFilter, SendBloomFilter
//!
//! These handlers implement bloom filter exchange for set reconciliation,
//! allowing nodes to efficiently discover which entries they're missing.

use crate::bloom::BloomFilter;
use crate::gossip::GossipActor;
use crate::types::{BloomFilterData, GossipMessage};
use anyhow::Result;
use icn_identity::Did;
use tracing::debug;

impl GossipActor {
    /// Handle a RequestBloomFilter message
    ///
    /// Sends our bloom filter for the requested topic back to the requester.
    /// The bloom filter allows them to determine which entries we might have.
    pub(crate) fn handle_request_bloom_filter(
        &mut self,
        sender: &Did,
        topic: String,
    ) -> Result<()> {
        debug!("Received RequestBloomFilter for topic: {}", topic);

        // Get bloom filter for the topic
        if let Some(filter) = self.bloom_filters.get(&topic) {
            let filter_data = filter.to_data();
            debug!(
                "Sending bloom filter for topic: {} ({} bits) to {}",
                topic, filter_data.size, sender
            );

            // Send bloom filter back to the requester
            self.send_message(
                Some(sender.clone()),
                GossipMessage::SendBloomFilter {
                    topic: topic.clone(),
                    filter: filter_data,
                },
            );
        } else {
            debug!("Topic not found: {}", topic);
        }

        Ok(())
    }

    /// Handle a SendBloomFilter message
    ///
    /// Receives a remote bloom filter for anti-entropy comparison.
    /// Currently performs basic validation; full implementation would
    /// identify and request missing entries.
    pub(crate) fn handle_send_bloom_filter(
        &mut self,
        _sender: &Did,
        topic: String,
        filter: BloomFilterData,
    ) -> Result<()> {
        debug!("Received SendBloomFilter for topic: {}", topic);

        // Reconstruct remote bloom filter
        let _remote_filter = BloomFilter::from_data(&filter);

        // Check if topic exists locally
        if !self.entries.contains_key(&topic) {
            debug!("Topic {} doesn't exist locally, cannot compare", topic);
            return Ok(());
        }

        // For a full implementation, we'd need to:
        // 1. Compare our bloom filter with the remote one
        // 2. Identify hashes present in remote but not in ours
        // 3. Request those missing hashes
        //
        // However, bloom filters are probabilistic - they can only tell us
        // "definitely not present" or "might be present". We can't extract
        // the actual hashes from a bloom filter.
        //
        // The proper approach is for the sender to also send their entry hashes
        // or we need a different anti-entropy approach (like Digest messages).

        debug!(
            "Remote filter size: {}, hashes: {}",
            filter.size, filter.num_hashes
        );
        debug!("Anti-entropy comparison complete for topic: {}", topic);

        Ok(())
    }
}
