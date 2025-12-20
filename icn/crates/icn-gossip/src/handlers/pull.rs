//! Pull protocol handlers: Digest, PullRequest, PullResponse, RequestMissing
//!
//! These handlers implement the pull-based anti-entropy protocol where nodes
//! exchange digests (vector clocks + bloom filters) to detect missing entries
//! and then request/respond with batches of entries.

use crate::bloom::BloomFilter;
use crate::gossip::GossipActor;
use crate::types::{BloomFilterData, ContentHash, GossipEntry, GossipMessage, TrustResourceLimits};
use crate::vector_clock::VectorClock;
use anyhow::Result;
use icn_identity::Did;
use tracing::debug;

impl GossipActor {
    /// Handle a Digest message - anti-entropy synchronization
    ///
    /// Compares our state with the remote peer's digest to detect if we're
    /// behind. If so, sends a PullRequest to fetch missing entries.
    ///
    /// Flow:
    /// 1. Reconstruct remote bloom filter from data
    /// 2. Compare vector clocks to detect if we're behind
    /// 3. Check trust class and get resource limits
    /// 4. Check backpressure: can we send to this peer?
    /// 5. Update peer sync state (vector, bloom, nonce, deficit)
    /// 6. Send PullRequest with selected entry hashes
    pub(crate) fn handle_digest(
        &mut self,
        sender: &Did,
        topic: String,
        vector: VectorClock,
        bloom: BloomFilterData,
        hint_count: u32,
        nonce: u64,
    ) -> Result<()> {
        icn_obs::metrics::gossip::digests_received_inc();
        debug!(
            "Received Digest: topic={}, hint_count={}, nonce={}",
            topic, hint_count, nonce
        );

        // Check if we have this topic
        if !self.topics.contains_key(&topic) {
            debug!("Received Digest for unknown topic: {}", topic);
            return Ok(());
        }

        // Reconstruct remote bloom filter
        let _remote_bloom = BloomFilter::from_data(&bloom);

        // PULL LOGIC: Detect if we're missing entries by comparing vector clocks
        let peer_did = sender.clone();

        // Check if we're behind this peer's sequence
        let mut are_we_behind = false;
        for (did, remote_seq) in &vector.clock {
            let our_seq = self.clock.get(did);
            if *remote_seq > our_seq {
                debug!(
                    "We're behind on {}: remote_seq={} > our_seq={}",
                    did, remote_seq, our_seq
                );
                are_we_behind = true;
                break;
            }
        }

        // Also check entry count hint
        let our_entry_count = self.entries.get(&topic).map(|e| e.len()).unwrap_or(0);
        if hint_count > our_entry_count as u32 {
            debug!(
                "Remote has {} entries, we have {} - we're behind",
                hint_count, our_entry_count
            );
            are_we_behind = true;
        }

        if !are_we_behind {
            debug!("We're not behind remote peer - no pull needed");
            return Ok(());
        }

        // We're behind! Send PullRequest with empty want_ids to request ALL entries
        debug!("Detected we're behind - sending PullRequest for all entries");

        // Get trust class and limits
        let trust_class =
            (self.trust_lookup)(&peer_did).unwrap_or(icn_trust::TrustClass::Isolated);
        let limits = TrustResourceLimits::for_trust_class(trust_class);
        let max_bytes = limits.max_pull_bytes;

        // Empty want_ids means "send all entries"
        let want_ids = Vec::new();
        let estimated_bytes = hint_count * 512; // Rough estimate

        // Update peer sync state (borrow mutable)
        let (request_nonce, deficit_after) = {
            let peer_state = self.peer_sync.get_or_create(peer_did.clone());

            // Update peer's known state
            peer_state.update_vector(vector.clone());
            peer_state.update_bloom(bloom.clone());

            // Check if we can send (backpressure + limits + backoff)
            if !peer_state.can_send(limits.max_outstanding_reqs, 10000) {
                debug!(
                    "Cannot send to peer {} - backpressured or at limit",
                    peer_did
                );
                let deficit = peer_state.deficit_bytes;
                icn_obs::metrics::gossip::peer_deficit_bytes_set(peer_did.as_str(), deficit);
                return Ok(());
            }

            // Generate nonce for this request
            let request_nonce = peer_state.next_nonce();

            // Record outgoing request
            peer_state.record_request();
            peer_state.debit_bytes(estimated_bytes);

            (request_nonce, peer_state.deficit_bytes)
        };

        debug!(
            "Sending PullRequest to {}: {} entries, ~{} bytes, nonce={}",
            peer_did,
            want_ids.len(),
            estimated_bytes,
            request_nonce
        );

        // Send PullRequest
        self.send_message(
            Some(peer_did.clone()),
            GossipMessage::PullRequest {
                topic: topic.clone(),
                want_ids,
                max_bytes,
                nonce: request_nonce,
            },
        );

        // Track metrics
        icn_obs::metrics::gossip::pull_requests_sent_inc();
        icn_obs::metrics::gossip::peer_deficit_bytes_set(peer_did.as_str(), deficit_after);

        Ok(())
    }

    /// Handle a PullRequest message - request for entries
    ///
    /// Collects requested entries (or all entries if want_ids is empty)
    /// up to max_bytes and sends them in a PullResponse.
    pub(crate) fn handle_pull_request(
        &mut self,
        _sender: &Did,
        topic: String,
        want_ids: Vec<ContentHash>,
        max_bytes: u32,
        nonce: u64,
    ) -> Result<()> {
        icn_obs::metrics::gossip::pull_requests_received_inc();
        debug!(
            "Received PullRequest: topic={}, want_ids={}, max_bytes={}, nonce={}",
            topic,
            want_ids.len(),
            max_bytes,
            nonce
        );

        // Get entries for the topic
        let topic_entries = match self.entries.get(&topic) {
            Some(entries) => entries,
            None => {
                debug!("Topic not found: {}", topic);
                return Ok(());
            }
        };

        // Collect requested entries
        let mut response_entries = Vec::new();
        let mut total_bytes = 0u32;
        let mut truncated = false;

        // If want_ids is empty, interpret as "send all entries" (up to max_bytes)
        let hashes_to_send: Vec<ContentHash> = if want_ids.is_empty() {
            debug!("Empty want_ids - sending all entries for topic (up to max_bytes)");
            topic_entries.keys().copied().collect()
        } else {
            want_ids.clone()
        };

        for hash in hashes_to_send {
            if let Some(entry) = topic_entries.get(&hash) {
                // Estimate entry size (rough approximation)
                let entry_bytes = entry.data.len() as u32 + 256; // Data + overhead

                if total_bytes + entry_bytes > max_bytes {
                    truncated = true;
                    icn_obs::metrics::gossip::pull_truncated_inc();
                    break;
                }

                response_entries.push(entry.clone());
                total_bytes += entry_bytes;
            }
        }

        debug!(
            "Sending PullResponse: {} entries, {} bytes, truncated={}",
            response_entries.len(),
            total_bytes,
            truncated
        );

        // Send pull response
        self.send_message(
            None,
            GossipMessage::PullResponse {
                topic: topic.clone(),
                entries: response_entries,
                truncated,
                nonce,
            },
        );

        icn_obs::metrics::gossip::pull_responses_sent_inc();
        icn_obs::metrics::gossip::bytes_pushed_add(total_bytes as u64);

        Ok(())
    }

    /// Handle a PullResponse message - batch of entries
    ///
    /// Stores all received entries and updates peer sync state.
    pub(crate) fn handle_pull_response(
        &mut self,
        _sender: &Did,
        _topic: String,
        entries: Vec<GossipEntry>,
        truncated: bool,
        nonce: u64,
    ) -> Result<()> {
        icn_obs::metrics::gossip::pull_responses_received_inc();
        debug!(
            "Received PullResponse: entries={}, truncated={}, nonce={}",
            entries.len(),
            truncated,
            nonce
        );

        // Calculate total bytes received and extract peer DID from first entry
        let mut total_bytes = 0u32;
        let mut peer_did: Option<Did> = None;

        for entry in &entries {
            let entry_bytes = entry.data.len() as u32 + 256;
            total_bytes += entry_bytes;

            // Extract peer DID from entry author (first entry)
            if peer_did.is_none() {
                peer_did = Some(entry.author.clone());
            }
        }

        // Store all entries
        for entry in entries {
            self.store_entry(entry)?;
        }

        // Update peer sync state if we can identify the peer
        if let Some(did) = peer_did {
            if let Some(peer_state) = self.peer_sync.get_mut(&did) {
                peer_state.record_response(total_bytes);

                debug!(
                    "Updated peer {} sync state: deficit={}, outstanding={}",
                    did, peer_state.deficit_bytes, peer_state.outstanding_requests
                );

                // Track peer deficit metric
                icn_obs::metrics::gossip::peer_deficit_bytes_set(
                    did.as_str(),
                    peer_state.deficit_bytes,
                );
            }
        }

        icn_obs::metrics::gossip::bytes_pulled_add(total_bytes as u64);
        icn_obs::metrics::gossip::entries_received_inc();
        self.update_gauge_metrics();

        debug!(
            "PullResponse processed: {} bytes received, truncated={}",
            total_bytes, truncated
        );
        Ok(())
    }

    /// Handle a RequestMissing message - request for specific hashes
    ///
    /// Sends Response messages for each requested hash that we have.
    pub(crate) fn handle_request_missing(
        &mut self,
        _sender: &Did,
        hashes: Vec<ContentHash>,
    ) -> Result<()> {
        debug!("Received RequestMissing: {} hashes", hashes.len());

        // Send Response messages for each requested hash that we have
        let mut sent_count = 0;
        let mut not_found_count = 0;

        for hash in hashes {
            // Find entry across all topics
            let mut found = false;
            for entries in self.entries.values() {
                if let Some(entry) = entries.get(&hash) {
                    debug!(
                        "Sending requested entry: hash={:?}, topic={}",
                        hash, entry.topic
                    );
                    self.send_message(
                        None,
                        GossipMessage::Response {
                            entry: entry.clone(),
                        },
                    );
                    sent_count += 1;
                    found = true;
                    break;
                }
            }

            if !found {
                debug!("Requested entry not found: hash={:?}", hash);
                not_found_count += 1;
            }
        }

        debug!(
            "RequestMissing complete: sent={}, not_found={}",
            sent_count, not_found_count
        );
        Ok(())
    }
}
