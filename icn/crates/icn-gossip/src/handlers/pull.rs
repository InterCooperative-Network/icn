//! Pull protocol handlers: Digest, PullRequest, PullResponse, RequestMissing
//!
//! These handlers implement the pull-based anti-entropy protocol where nodes
//! exchange digests (vector clocks + bloom filters) to detect missing entries
//! and then request/respond with batches of entries.

use crate::bloom::BloomFilter;
use crate::gossip::GossipActor;
use crate::types::{
    BloomFilterData, ContentHash, GossipEntry, GossipMessage, ResourceLimits, SyncCursor,
};
use crate::vector_clock::VectorClock;
use anyhow::Result;
use icn_identity::Did;
use icn_kernel_api::authz::{ActionKind, ConstraintValue, Domain, PolicyOracle, PolicyRequest};
use tracing::{debug, warn};

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
        for (did, remote_seq) in vector.iter() {
            let our_seq = self.clock.get(did);
            if remote_seq > our_seq {
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
        // Get trust score and limits
        let trust_score = if let Some(oracle) = &self.oracle {
            let req = PolicyRequest::new(
                peer_did.to_string(),
                ActionKind::Subscribe, // treat pull as subscribe for now? Or generic access
                Domain::trust(),
            );
            match oracle.evaluate(&req) {
                icn_kernel_api::authz::PolicyDecision::Allow { constraints } => constraints
                    .custom
                    .get("trust_score")
                    .and_then(|v| match v {
                        ConstraintValue::Float(f) => Some(f.into_inner()),
                        _ => None,
                    })
                    .unwrap_or(0.0),
                _ => 0.0,
            }
        } else {
            0.0
        };
        let limits = ResourceLimits::for_trust_score(trust_score);
        let max_bytes = limits.max_pull_bytes;

        // Empty want_ids means "send all entries"
        let want_ids = Vec::new();
        let estimated_bytes = hint_count * 512; // Rough estimate

        // Update peer sync state (borrow mutable)
        let (request_nonce, _deficit) = {
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
                // Note: Per-peer deficit tracking removed to avoid high-cardinality metrics.
                // Use aggregated metrics (pull_requests_sent/received) for overall health.
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
                cursor: None, // Initial request, no cursor
            },
        );

        // Track metrics
        icn_obs::metrics::gossip::pull_requests_sent_inc();

        Ok(())
    }

    /// Handle a PullRequest message - request for entries with pagination support
    ///
    /// Collects requested entries (or all entries if want_ids is empty)
    /// up to max_bytes and sends them in a PullResponse.
    /// If a cursor is provided, resumes from that position.
    pub(crate) fn handle_pull_request(
        &mut self,
        _sender: &Did,
        topic: String,
        want_ids: Vec<ContentHash>,
        max_bytes: u32,
        nonce: u64,
        cursor: Option<SyncCursor>,
    ) -> Result<()> {
        icn_obs::metrics::gossip::pull_requests_received_inc();
        debug!(
            "Received PullRequest: topic={}, want_ids={}, max_bytes={}, nonce={}, cursor={:?}",
            topic,
            want_ids.len(),
            max_bytes,
            nonce,
            cursor.is_some()
        );

        // Validate cursor if provided
        if let Some(ref c) = cursor {
            if c.is_expired() {
                warn!(
                    "Received expired cursor for topic {}, ignoring cursor",
                    topic
                );
            } else if c.topic != topic {
                warn!(
                    "Cursor topic mismatch: {} vs {}, ignoring cursor",
                    c.topic, topic
                );
            }
        }

        // Get entries for the topic
        let topic_entries = match self.entries.get(&topic) {
            Some(entries) => entries,
            None => {
                debug!("Topic not found: {}", topic);
                return Ok(());
            }
        };

        // Sort entries deterministically for pagination (by timestamp, then hash)
        let mut sorted_entries: Vec<_> = topic_entries.values().cloned().collect();
        sorted_entries.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.hash.cmp(&b.hash))
        });

        // Find starting position from cursor
        let start_index = if let Some(ref c) = cursor {
            if c.is_valid_for_topic(&topic) {
                // Find entry after cursor position
                sorted_entries
                    .iter()
                    .position(|e| e.hash == c.last_hash)
                    .map(|i| i + 1)
                    .unwrap_or(0) // If hash not found, start from beginning
            } else {
                0
            }
        } else {
            0
        };

        // Collect entries with pagination
        let mut response_entries = Vec::new();
        let mut total_bytes = 0u32;
        let mut last_entry_index = start_index;

        // If want_ids is empty, interpret as "send all entries" (up to max_bytes)
        let want_all = want_ids.is_empty();
        if want_all {
            debug!("Empty want_ids - sending all entries for topic (up to max_bytes)");
        }

        for (idx, entry) in sorted_entries.iter().enumerate().skip(start_index) {
            // Filter by want_ids if provided (unless empty = all)
            if !want_all && !want_ids.contains(&entry.hash) {
                continue;
            }

            // Estimate entry size (rough approximation)
            let entry_bytes = entry.data.len() as u32 + 256; // Data + overhead

            if total_bytes + entry_bytes > max_bytes {
                icn_obs::metrics::gossip::pull_truncated_inc();
                break;
            }

            response_entries.push(entry.clone());
            total_bytes += entry_bytes;
            last_entry_index = idx;
        }

        // Determine if there are more entries and create cursor
        let has_more = last_entry_index + 1 < sorted_entries.len() && !response_entries.is_empty();
        let next_cursor = if has_more {
            sorted_entries
                .get(last_entry_index)
                .map(|e| SyncCursor::new(last_entry_index as u64, e.hash, topic.clone()))
        } else {
            None
        };

        debug!(
            "Sending PullResponse: {} entries, {} bytes, truncated={}, has_cursor={}",
            response_entries.len(),
            total_bytes,
            has_more,
            next_cursor.is_some()
        );

        // Send pull response with cursor
        self.send_message(
            None,
            GossipMessage::PullResponse {
                topic: topic.clone(),
                entries: response_entries,
                truncated: has_more,
                nonce,
                next_cursor,
            },
        );

        icn_obs::metrics::gossip::pull_responses_sent_inc();
        icn_obs::metrics::gossip::bytes_pushed_add(total_bytes as u64);

        Ok(())
    }

    /// Handle a PullResponse message - batch of entries with auto-continuation
    ///
    /// Stores all received entries, updates peer sync state, and automatically
    /// continues pagination if a cursor is provided.
    pub(crate) async fn handle_pull_response(
        &mut self,
        sender: &Did,
        topic: String,
        entries: Vec<GossipEntry>,
        truncated: bool,
        nonce: u64,
        next_cursor: Option<SyncCursor>,
    ) -> Result<()> {
        icn_obs::metrics::gossip::pull_responses_received_inc();
        debug!(
            "Received PullResponse: entries={}, truncated={}, nonce={}, has_cursor={}",
            entries.len(),
            truncated,
            nonce,
            next_cursor.is_some()
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
            self.store_entry(entry).await?;
        }

        // Update peer sync state if we can identify the peer
        let peer_for_continuation = peer_did.clone().unwrap_or_else(|| sender.clone());
        if let Some(peer_state) = self.peer_sync.get_mut(&peer_for_continuation) {
            peer_state.record_response(total_bytes);

            debug!(
                "Updated peer {} sync state: deficit={}, outstanding={}",
                peer_for_continuation, peer_state.deficit_bytes, peer_state.outstanding_requests
            );

            // Note: Per-peer deficit tracking removed to avoid high-cardinality metrics.
        }

        icn_obs::metrics::gossip::bytes_pulled_add(total_bytes as u64);
        icn_obs::metrics::gossip::entries_received_inc();
        self.update_gauge_metrics();

        // Auto-continue if truncated and cursor provided (Issue #123)
        if truncated {
            if let Some(cursor) = next_cursor {
                icn_obs::metrics::gossip::pull_continuation_received_inc();

                // Get trust-based limits for continuation
                // Get trust-based limits for continuation
                let trust_score = if let Some(oracle) = &self.oracle {
                    let req = PolicyRequest::new(
                        peer_for_continuation.to_string(),
                        ActionKind::Subscribe,
                        Domain::trust(),
                    );
                    match oracle.evaluate(&req) {
                        icn_kernel_api::authz::PolicyDecision::Allow { constraints } => constraints
                            .custom
                            .get("trust_score")
                            .and_then(|v| match v {
                                ConstraintValue::Float(f) => Some(f.into_inner()),
                                _ => None,
                            })
                            .unwrap_or(0.0),
                        _ => 0.0,
                    }
                } else {
                    0.0
                };
                let limits = ResourceLimits::for_trust_score(trust_score);

                // Check backpressure before continuing
                let can_continue = self
                    .peer_sync
                    .get(&peer_for_continuation)
                    .map(|ps| ps.can_send(limits.max_outstanding_reqs, 10000))
                    .unwrap_or(false);

                if can_continue {
                    // Generate nonce for continuation request
                    let new_nonce = self
                        .peer_sync
                        .get_mut(&peer_for_continuation)
                        .map(|ps| {
                            ps.record_request();
                            ps.next_nonce()
                        })
                        .unwrap_or(0);

                    debug!(
                        "Auto-continuing paginated sync with cursor, new_nonce={}",
                        new_nonce
                    );

                    self.send_message(
                        Some(peer_for_continuation.clone()),
                        GossipMessage::PullRequest {
                            topic,
                            want_ids: Vec::new(), // Continue all entries
                            max_bytes: limits.max_pull_bytes,
                            nonce: new_nonce,
                            cursor: Some(cursor),
                        },
                    );

                    icn_obs::metrics::gossip::pull_continuation_sent_inc();
                    icn_obs::metrics::gossip::pull_requests_sent_inc();
                } else {
                    debug!(
                        "Cannot auto-continue - peer {} backpressured",
                        peer_for_continuation
                    );
                }
            }
        }

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
