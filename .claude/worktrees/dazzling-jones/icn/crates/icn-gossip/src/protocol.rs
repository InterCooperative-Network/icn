//! Message protocol handling
//!
//! This module handles incoming gossip protocol messages with trust-gated validation.
//! It provides the central message dispatcher that routes messages to appropriate handlers.
//!
//! # Message Flow
//!
//! 1. Messages arrive via `handle_message()`
//! 2. Sender trust is validated against minimum threshold
//! 3. Messages are dispatched to type-specific handlers via shared dispatch logic
//!
//! # Trust Gating
//!
//! All messages are validated against the sender's trust score before processing.
//! Messages from low-trust senders are rejected with appropriate metrics tracking.
//!
//! # Dispatch Logic
//!
//! The dispatch logic is centralized in `handlers::dispatch` module to ensure
//! single source of truth for message routing. This design:
//! - Eliminates code duplication between single-message and batch handlers
//! - Avoids async recursion issues in batch processing
//! - Makes it easier to add new message types

use crate::gossip::GossipActor;
use crate::handlers::DispatchResult;
use crate::types::GossipMessage;
use anyhow::{bail, Result};
use icn_identity::Did;
use icn_kernel_api::authz::{ActionKind, Domain, PolicyRequest};
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
        if let Some(oracle) = &self.oracle {
            let req = PolicyRequest::new(
                sender.to_string(),
                ActionKind::Read, // Messages are treated as basic read/interact access
                Domain::trust(),
            );
            match oracle.evaluate(&req) {
                icn_kernel_api::authz::PolicyDecision::Allow { .. } => {
                    debug!(peer_did = %sender, "Processing message from authorized sender");
                }
                icn_kernel_api::authz::PolicyDecision::Deny { reason, .. } => {
                    warn!(
                        peer_did = %sender,
                        reason = %reason,
                        message_type = message.variant_name(),
                        "Rejecting message from peer denied by policy"
                    );
                    icn_obs::metrics::gossip::messages_rejected_low_trust_inc();
                    bail!("Access denied by policy: {reason}");
                }
            }
        }

        // Phase 18 Week 3: Record contact for partition detection
        if let Some(ref detector) = self.partition_detector {
            if let Ok(mut d) = detector.try_write() {
                d.record_contact(sender);
            }
        }

        // Dispatch to handler methods using shared dispatch logic.
        //
        // The dispatch returns an enum indicating whether the handler executed synchronously
        // or requires async completion:
        // - Sync: Handler already executed; propagate result directly
        // - AsyncResponse/AsyncPullResponse: Handler needs `.await`; extract params and call
        //
        // This two-phase approach (dispatch + await) avoids async recursion issues in batch
        // handlers while maintaining a single source of truth for message routing.
        match self.dispatch_message(sender, message) {
            DispatchResult::Sync(result) => result,
            DispatchResult::AsyncResponse(sender, entry) => {
                self.handle_response(&sender, entry).await
            }
            DispatchResult::AsyncPullResponse {
                sender,
                topic,
                entries,
                truncated,
                nonce,
                next_cursor,
            } => {
                self.handle_pull_response(&sender, topic, entries, truncated, nonce, next_cursor)
                    .await
            }
        }
    }
}
