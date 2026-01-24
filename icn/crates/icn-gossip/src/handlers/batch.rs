//! Batch message handlers
//!
//! Handlers for processing batched gossip messages.
//!
//! # Implementation Note
//!
//! The batch handler duplicates the message dispatch logic from `protocol.rs`
//! (`handle_message_inner`). This is intentional to avoid async recursion issues
//! that would arise from calling `handle_message` recursively for each message
//! in a batch. While this creates maintenance overhead (new message types must
//! be added in both places), it provides cleaner error handling and prevents
//! nested batch recursion.

use crate::gossip::GossipActor;
use crate::types::GossipMessage;
use anyhow::Result;
use icn_identity::Did;
use tracing::{debug, warn};

/// Maximum messages allowed in a single batch to prevent DoS attacks.
/// A malicious peer could otherwise send unbounded batches.
const MAX_BATCH_MESSAGES: usize = 100;

/// Minimum trust score required to process messages (same as protocol.rs)
const MIN_TRUST_FOR_MESSAGE: f64 = 0.1;

impl GossipActor {
    /// Handle a batch of gossip messages
    ///
    /// # Security
    ///
    /// This handler performs the same trust validation as `handle_message_inner`:
    /// - Rejects messages from senders with trust score below MIN_TRUST_FOR_MESSAGE
    /// - Rejects batches with more than MAX_BATCH_MESSAGES to prevent DoS
    pub(crate) async fn handle_batch(
        &mut self,
        sender: &Did,
        batch_id: u64,
        messages: Vec<GossipMessage>,
        _compressed: bool,
    ) -> Result<()> {
        // CRITICAL: Validate batch size to prevent DoS attacks
        if messages.len() > MAX_BATCH_MESSAGES {
            warn!(
                peer_did = %sender,
                batch_id,
                message_count = messages.len(),
                max_allowed = MAX_BATCH_MESSAGES,
                "Rejecting oversized batch"
            );
            icn_obs::metrics::gossip::batches_rejected_oversized_inc();
            anyhow::bail!(
                "Batch too large: {} messages exceeds limit of {}",
                messages.len(),
                MAX_BATCH_MESSAGES
            );
        }

        // CRITICAL: Trust-gated message handling (same check as protocol.rs)
        // Batches must not bypass the trust check that handle_message_inner performs
        if let Some(ref trust_graph) = self.trust_graph {
            if let Ok(tg) = trust_graph.try_read() {
                match tg.compute_trust_score(sender) {
                    Ok(score) if score < MIN_TRUST_FOR_MESSAGE => {
                        warn!(
                            peer_did = %sender,
                            trust_score = score,
                            min_required = MIN_TRUST_FOR_MESSAGE,
                            batch_id,
                            "Rejecting batch from low-trust sender"
                        );
                        icn_obs::metrics::gossip::messages_rejected_low_trust_inc();
                        anyhow::bail!(
                            "Batch sender {sender} has insufficient trust ({score:.3} < {MIN_TRUST_FOR_MESSAGE:.3})"
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
                            batch_id,
                            "Cannot compute trust score for batch sender"
                        );
                        icn_obs::metrics::gossip::messages_rejected_low_trust_inc();
                        anyhow::bail!("Cannot verify trust for batch sender {sender}: {e}");
                    }
                }
            } else {
                // If we can't acquire lock, skip trust check (avoid blocking)
                icn_obs::metrics::gossip::trust_check_lock_skipped_inc();
            }
        }

        debug!(
            peer_did = %sender,
            batch_id,
            message_count = messages.len(),
            "Received message batch"
        );

        // Record metrics
        icn_obs::metrics::gossip::batches_received_inc();
        icn_obs::metrics::gossip::batch_size_record(messages.len());

        // Process each message in the batch
        // Note: We dispatch directly to handlers to avoid async recursion issues
        let mut success_count = 0;
        let mut error_count = 0;

        for message in messages {
            // Prevent nested batches
            if matches!(message, GossipMessage::Batch { .. }) {
                warn!(
                    peer_did = %sender,
                    batch_id,
                    "Ignoring nested batch message"
                );
                error_count += 1;
                continue;
            }

            // Dispatch directly to handlers (copied from protocol.rs handle_message_inner)
            let result = match message {
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

                GossipMessage::RequestMissing { hashes } => {
                    self.handle_request_missing(sender, hashes)
                }

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

                GossipMessage::StorageProofMsg { proof } => {
                    self.handle_storage_proof(sender, proof)
                }

                GossipMessage::StorageContentNotFoundMsg { response } => {
                    self.handle_storage_content_not_found(sender, response)
                }

                // This should be unreachable due to the check above
                GossipMessage::Batch { .. } => {
                    unreachable!("Nested batch prevented by earlier check")
                }
            };

            match result {
                Ok(_) => success_count += 1,
                Err(e) => {
                    warn!(
                        peer_did = %sender,
                        batch_id,
                        error = %e,
                        "Failed to process message in batch"
                    );
                    error_count += 1;
                }
            }
        }

        debug!(
            peer_did = %sender,
            batch_id,
            success_count,
            error_count,
            "Finished processing batch"
        );

        Ok(())
    }
}
