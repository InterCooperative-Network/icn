//! Message protocol handling
//!
//! This module handles incoming gossip protocol messages with trust-gated validation.
//! It provides the central message dispatcher that routes messages to appropriate handlers.
//!
//! # Message Flow
//!
//! 1. Messages arrive via `handle_message()`
//! 2. Sender trust is validated against minimum threshold
//! 3. Messages are dispatched to type-specific handlers (in `handlers/` module)
//!
//! # Trust Gating
//!
//! All messages are validated against the sender's trust score before processing.
//! Messages from low-trust senders are rejected with appropriate metrics tracking.

use crate::gossip::GossipActor;
use crate::types::GossipMessage;
use anyhow::Result;
use icn_identity::Did;
use tracing::{debug, instrument, warn};

impl GossipActor {
    /// Handle incoming gossip message from network
    #[instrument(skip(self, message), fields(peer_did = %sender, message_type = message.variant_name()))]
    pub async fn handle_message(&mut self, sender: &Did, message: GossipMessage) -> Result<()> {
        // Issue #181: Track message processing latency
        let start = std::time::Instant::now();
        let result = self.handle_message_inner(sender, message).await;
        let elapsed = start.elapsed().as_secs_f64();
        icn_obs::metrics::gossip::message_latency_record(elapsed);
        result
    }

    /// Inner implementation of handle_message (for latency tracking)
    async fn handle_message_inner(&mut self, sender: &Did, message: GossipMessage) -> Result<()> {
        // H7 fix: Trust-gated message handling
        // Check sender's trust score before processing messages
        const MIN_TRUST_FOR_MESSAGE: f64 = 0.1; // Known trust class minimum

        if let Some(ref trust_graph) = self.trust_graph {
            if let Ok(tg) = trust_graph.try_read() {
                match tg.compute_trust_score(sender) {
                    Ok(score) if score < MIN_TRUST_FOR_MESSAGE => {
                        warn!(
                            peer_did = %sender,
                            trust_score = score,
                            min_required = MIN_TRUST_FOR_MESSAGE,
                            message_type = message.variant_name(),
                            "Rejecting message from low-trust sender"
                        );
                        icn_obs::metrics::gossip::messages_rejected_low_trust_inc();
                        anyhow::bail!(
                            "Message sender {sender} has insufficient trust ({score:.3} < {MIN_TRUST_FOR_MESSAGE:.3})"
                        );
                    }
                    Ok(_) => {
                        // Trust validated successfully
                    }
                    Err(e) => {
                        // Unknown sender - reject by default
                        debug!(
                            peer_did = %sender,
                            error = %e,
                            "Cannot compute trust score for message sender"
                        );
                        icn_obs::metrics::gossip::messages_rejected_low_trust_inc();
                        anyhow::bail!("Cannot verify trust for message sender {sender}: {e}");
                    }
                }
            }
            // If we can't acquire lock, skip trust check (avoid blocking)
        }

        // Phase 18 Week 3: Record contact for partition detection
        if let Some(ref detector) = self.partition_detector {
            if let Ok(mut d) = detector.try_write() {
                d.record_contact(sender);
            }
        }

        // Dispatch to handler methods (extracted to handlers/ module)
        match message {
            GossipMessage::Announce {
                hash,
                author,
                clock: _,
                topic,
            } => self.handle_announce(sender, hash, author, topic),

            GossipMessage::Request { hash } => self.handle_request(sender, hash),

            GossipMessage::Response { entry } => self.handle_response(sender, entry).await,

            GossipMessage::RequestBloomFilter { topic } => {
                self.handle_request_bloom_filter(sender, topic)
            }

            GossipMessage::SendBloomFilter { topic, filter } => {
                self.handle_send_bloom_filter(sender, topic, filter)
            }

            GossipMessage::RequestMissing { hashes } => self.handle_request_missing(sender, hashes),

            GossipMessage::Digest {
                topic,
                vector,
                bloom,
                hint_count,
                nonce,
            } => self.handle_digest(sender, topic, vector, bloom, hint_count, nonce),

            GossipMessage::PullRequest {
                topic,
                want_ids,
                max_bytes,
                nonce,
                cursor,
            } => self.handle_pull_request(sender, topic, want_ids, max_bytes, nonce, cursor),

            GossipMessage::PullResponse {
                topic,
                entries,
                truncated,
                nonce,
                next_cursor,
            } => {
                self.handle_pull_response(sender, topic, entries, truncated, nonce, next_cursor)
                    .await
            }

            GossipMessage::BlobAnnounce {
                blob_hash,
                peer_did,
                size_bytes,
            } => self.handle_blob_announce(sender, blob_hash, peer_did, size_bytes),

            GossipMessage::ReplicaRequest {
                content_hash,
                requesting_peer,
            } => self.handle_replica_request(sender, content_hash, requesting_peer),

            GossipMessage::ReplicaOffer {
                content_hash,
                offering_peer,
                health,
            } => self.handle_replica_offer(sender, content_hash, offering_peer, health),

            GossipMessage::ReplicaStatus {
                content_hash,
                replicas,
            } => self.handle_replica_status(sender, content_hash, replicas),

            GossipMessage::PartitionHealRequest {
                requesting_peer,
                vector_clock,
                last_contact_ms,
            } => self.handle_partition_heal_request(
                sender,
                requesting_peer,
                vector_clock,
                last_contact_ms,
            ),

            GossipMessage::PartitionHealResponse {
                responding_peer,
                vector_clock,
                diverged_topics,
                entries_behind,
            } => self.handle_partition_heal_response(
                sender,
                responding_peer,
                vector_clock,
                diverged_topics,
                entries_behind,
            ),

            GossipMessage::StorageChallengeMsg { challenge } => {
                self.handle_storage_challenge(sender, challenge)
            }

            GossipMessage::StorageProofMsg { proof } => self.handle_storage_proof(sender, proof),

            GossipMessage::StorageContentNotFoundMsg { response } => {
                self.handle_storage_content_not_found(sender, response)
            }
        }
    }
}
