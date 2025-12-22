//! Partition healing handlers: PartitionHealRequest, PartitionHealResponse
//!
//! These handlers implement Phase 18 Week 3 partition healing protocol for
//! synchronizing vector clocks and recovering from network partitions.

use crate::gossip::GossipActor;
use crate::types::GossipMessage;
use crate::vector_clock::VectorClock;
use anyhow::Result;
use icn_identity::Did;
use tracing::info;

impl GossipActor {
    /// Handle a PartitionHealRequest message
    ///
    /// Compares vector clocks to identify diverged topics and entries
    /// that the requesting peer might be missing, then responds with
    /// our vector clock and the list of diverged topics.
    pub(crate) fn handle_partition_heal_request(
        &mut self,
        _sender: &Did,
        requesting_peer: Did,
        vector_clock: VectorClock,
        last_contact_ms: u64,
    ) -> Result<()> {
        info!(
            peer_did = %requesting_peer,
            last_contact_ms = last_contact_ms,
            message_type = "PartitionHealRequest",
            "Received partition heal request"
        );
        icn_obs::metrics::gossip::partition_detected_inc();

        // Get list of topics that may have diverged
        // (topics where our vector clock has entries they don't)
        let mut diverged_topics = Vec::new();
        let mut entries_behind = 0u64;

        for (topic_name, entries) in &self.entries {
            // Count entries they might be missing
            for entry in entries.values() {
                // Check if our entry clock is ahead of their clock for any peer
                let our_version = entry.clock.get(&entry.author);
                let their_version = vector_clock.get(&entry.author);
                if our_version > their_version {
                    if !diverged_topics.contains(topic_name) {
                        diverged_topics.push(topic_name.clone());
                    }
                    entries_behind += 1;
                }
            }
        }

        info!(
            peer = %requesting_peer,
            diverged_topics = ?diverged_topics,
            entries_behind = entries_behind,
            "Preparing partition heal response"
        );

        // Send our vector clock back
        self.send_message(
            Some(requesting_peer.clone()),
            GossipMessage::PartitionHealResponse {
                responding_peer: self.own_did.clone(),
                vector_clock: self.clock.clone(),
                diverged_topics,
                entries_behind,
            },
        );

        icn_obs::metrics::gossip::partition_vector_clock_merges_inc();
        Ok(())
    }

    /// Handle a PartitionHealResponse message
    ///
    /// Merges the remote vector clock with ours, updates partition detector
    /// and healer state, and requests missing entries from diverged topics.
    pub(crate) fn handle_partition_heal_response(
        &mut self,
        _sender: &Did,
        responding_peer: Did,
        vector_clock: VectorClock,
        diverged_topics: Vec<String>,
        entries_behind: u64,
    ) -> Result<()> {
        info!(
            peer_did = %responding_peer,
            diverged_topics_count = diverged_topics.len(),
            entries_behind = entries_behind,
            message_type = "PartitionHealResponse",
            "Received partition heal response"
        );

        // Merge their vector clock with ours (sync operation)
        self.clock.merge(&vector_clock);
        icn_obs::metrics::gossip::partition_vector_clock_merges_inc();

        info!(
            peer = %responding_peer,
            "Vector clocks merged during partition healing"
        );
        icn_obs::metrics::gossip::partition_healed_inc();

        // Update partition detector to clear partitioned status
        if let Some(ref detector) = self.partition_detector {
            if let Ok(mut d) = detector.try_write() {
                d.record_contact(&responding_peer);
            }
        }

        // Mark healing as complete with this peer
        if let Some(ref healer) = self.partition_healer {
            if let Ok(mut h) = healer.try_write() {
                h.mark_healing_complete(&responding_peer);
            }
        }

        // Request entries from diverged topics
        if entries_behind > 0 && !diverged_topics.is_empty() {
            info!(
                peer = %responding_peer,
                diverged_topics = ?diverged_topics,
                entries_behind = entries_behind,
                "Requesting missing entries from diverged topics"
            );

            // For each diverged topic, send a PullRequest
            for topic in diverged_topics {
                if self.topics.contains_key(&topic) {
                    // Request all entries (empty want_ids means "send everything")
                    let nonce = self
                        .peer_sync
                        .get_or_create(responding_peer.clone())
                        .next_nonce();
                    self.send_message(
                        Some(responding_peer.clone()),
                        GossipMessage::PullRequest {
                            topic,
                            want_ids: vec![],
                            max_bytes: 1_000_000, // 1MB max
                            nonce,
                            cursor: None, // Initial request, no cursor
                        },
                    );
                }
            }
        }

        Ok(())
    }
}
