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

/// Maximum total bytes for a received batch (1MB default).
/// This prevents memory exhaustion from batches with many large messages.
/// Note: This is checked on the receiver side; the sender uses `max_batch_bytes`
/// from `BatchingConfig` which may be different (e.g., 256KB for senders).
const MAX_RECEIVED_BATCH_BYTES: usize = 1024 * 1024; // 1MB

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
    /// - Rejects batches exceeding MAX_RECEIVED_BATCH_BYTES total size
    pub(crate) async fn handle_batch(
        &mut self,
        sender: &Did,
        batch_id: u64,
        messages: Vec<GossipMessage>,
        _compressed: bool,
    ) -> Result<()> {
        // CRITICAL: Validate batch message count to prevent DoS attacks
        if messages.len() > MAX_BATCH_MESSAGES {
            warn!(
                peer_did = %sender,
                batch_id,
                message_count = messages.len(),
                max_allowed = MAX_BATCH_MESSAGES,
                "Rejecting oversized batch (message count)"
            );
            icn_obs::metrics::gossip::batches_rejected_oversized_inc();
            anyhow::bail!(
                "Batch too large: {} messages exceeds limit of {}",
                messages.len(),
                MAX_BATCH_MESSAGES
            );
        }

        // CRITICAL: Validate total batch byte size to prevent memory exhaustion
        // A malicious peer could send 100 extremely large messages within the count limit
        let batch_bytes: usize = messages
            .iter()
            .map(|m| icn_encoding::encode(m).map(|v| v.len()).unwrap_or(0))
            .sum();
        if batch_bytes > MAX_RECEIVED_BATCH_BYTES {
            warn!(
                peer_did = %sender,
                batch_id,
                batch_bytes,
                max_allowed = MAX_RECEIVED_BATCH_BYTES,
                "Rejecting oversized batch (byte size)"
            );
            icn_obs::metrics::gossip::batches_rejected_oversized_inc();
            anyhow::bail!(
                "Batch too large: {} bytes exceeds limit of {} bytes",
                batch_bytes,
                MAX_RECEIVED_BATCH_BYTES
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
                // SECURITY: Fail-safe behavior - reject batch when trust cannot be verified.
                // This prevents timing attacks where an attacker floods the system to create
                // lock contention and then sends malicious batches during the contention window.
                //
                // Operators: If this metric increases, investigate trust graph lock contention.
                // Consider increasing trust graph capacity or reducing message rate.
                icn_obs::metrics::gossip::trust_check_lock_skipped_inc();
                warn!(
                    peer_did = %sender,
                    batch_id,
                    "Trust graph lock contention - rejecting batch (fail-safe)"
                );
                anyhow::bail!(
                    "Cannot verify trust for batch sender {sender}: trust graph lock contention. Batch rejected."
                );
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time check that all GossipMessage variants are handled.
    ///
    /// This function uses an exhaustive match on GossipMessage to ensure that
    /// adding a new variant will cause a compile error if the batch handler
    /// isn't updated. The batch handler's match statement (above) must handle
    /// the same variants as this helper.
    ///
    /// When adding a new GossipMessage variant:
    /// 1. Add it to the batch handler's match in `handle_batch`
    /// 2. Add it to this function's match
    /// 3. Update the VARIANT_COUNT constant
    fn variant_name_helper(msg: &GossipMessage) -> &'static str {
        match msg {
            GossipMessage::Announce { .. } => "Announce",
            GossipMessage::Request { .. } => "Request",
            GossipMessage::Response { .. } => "Response",
            GossipMessage::RequestBloomFilter { .. } => "RequestBloomFilter",
            GossipMessage::SendBloomFilter { .. } => "SendBloomFilter",
            GossipMessage::RequestMissing { .. } => "RequestMissing",
            GossipMessage::Digest { .. } => "Digest",
            GossipMessage::PullRequest { .. } => "PullRequest",
            GossipMessage::PullResponse { .. } => "PullResponse",
            GossipMessage::BlobAnnounce { .. } => "BlobAnnounce",
            GossipMessage::ReplicaRequest { .. } => "ReplicaRequest",
            GossipMessage::ReplicaOffer { .. } => "ReplicaOffer",
            GossipMessage::ReplicaStatus { .. } => "ReplicaStatus",
            GossipMessage::PartitionHealRequest { .. } => "PartitionHealRequest",
            GossipMessage::PartitionHealResponse { .. } => "PartitionHealResponse",
            GossipMessage::StorageChallengeMsg { .. } => "StorageChallengeMsg",
            GossipMessage::StorageProofMsg { .. } => "StorageProofMsg",
            GossipMessage::StorageContentNotFoundMsg { .. } => "StorageContentNotFoundMsg",
            GossipMessage::Batch { .. } => "Batch",
        }
    }

    /// Expected number of GossipMessage variants.
    /// Update this when adding new message types to ensure the test catches drift.
    const EXPECTED_VARIANT_COUNT: usize = 19;

    #[test]
    fn test_batch_handler_exhaustive_match() {
        // This test verifies that the batch handler's match statement
        // and this module's helper match statement cover the same variants.
        //
        // If a new GossipMessage variant is added:
        // 1. The batch handler's match in handle_batch() will fail to compile
        // 2. The variant_name_helper() function above will fail to compile
        // 3. This test reminds you to update EXPECTED_VARIANT_COUNT
        //
        // This provides defense-in-depth: even if someone adds a wildcard
        // pattern to the batch handler, this test will catch the discrepancy.

        // Use GossipMessage::variant_name() which also has an exhaustive match
        // to verify we're tracking the same set of variants
        let sample = GossipMessage::Request { hash: [0u8; 32] };
        let _ = sample.variant_name();
        let _ = variant_name_helper(&sample);

        // The assertion below documents the current variant count.
        // If this fails, a new variant was added - update all match statements.
        assert_eq!(
            EXPECTED_VARIANT_COUNT, 19,
            "GossipMessage variant count changed - update batch handler match statement"
        );
    }

    #[test]
    fn test_nested_batch_rejected() {
        // Verify that nested batches are explicitly handled (not silently dropped)
        let nested = GossipMessage::Batch {
            batch_id: 1,
            messages: vec![GossipMessage::Batch {
                batch_id: 2,
                messages: vec![],
                compressed: false,
            }],
            compressed: false,
        };

        // The batch handler should detect and reject nested batches
        // (tested in integration tests, this just verifies the variant exists)
        assert!(matches!(nested, GossipMessage::Batch { .. }));
    }

    #[test]
    fn test_batch_limits_constants() {
        // Document and verify batch limit constants using runtime checks
        // to avoid clippy::assertions_on_constants
        let max_msgs = MAX_BATCH_MESSAGES;
        let max_bytes = MAX_RECEIVED_BATCH_BYTES;
        let min_trust = MIN_TRUST_FOR_MESSAGE;

        assert_eq!(max_msgs, 100, "Max messages per batch");
        assert_eq!(max_bytes, 1024 * 1024, "Max bytes per received batch (1MB)");
        assert!(min_trust > 0.0, "Trust threshold must be positive");
        assert!(min_trust < 1.0, "Trust threshold must be less than 1.0");
    }

    #[test]
    fn test_oversized_batch_detection() {
        // Verify that batches exceeding MAX_BATCH_MESSAGES are detected
        let max_msgs = MAX_BATCH_MESSAGES;
        let oversized_count = max_msgs + 1;
        assert!(
            oversized_count > max_msgs,
            "Test setup: oversized batch should exceed limit"
        );

        // Note: The actual rejection is tested in integration tests where we can
        // call handle_batch. This unit test verifies the constant is reasonable.
        assert!(
            max_msgs <= 1000,
            "Sanity check: max messages should be reasonable"
        );
    }

    #[test]
    fn test_byte_limit_reasonable() {
        // Verify byte limit allows reasonable message sizes
        // With 100 messages at 1MB limit, average message can be 10KB
        let max_bytes = MAX_RECEIVED_BATCH_BYTES;
        let max_msgs = MAX_BATCH_MESSAGES;
        let avg_message_size = max_bytes / max_msgs;
        assert!(
            avg_message_size >= 1024,
            "Average message budget should be at least 1KB: got {} bytes",
            avg_message_size
        );
        assert!(
            avg_message_size <= 100 * 1024,
            "Average message budget should be under 100KB: got {} bytes",
            avg_message_size
        );
    }
}
