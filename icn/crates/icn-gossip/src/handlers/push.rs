//! Push protocol handlers: Announce, Request, Response
//!
//! These handlers implement the push-based gossip protocol where nodes
//! announce new entries and request/respond with full entry data.

use crate::gossip::GossipActor;
use crate::types::{ContentHash, GossipEntry, GossipMessage};
use anyhow::Result;
use icn_identity::Did;
use tracing::debug;

impl GossipActor {
    /// Handle an Announce message - notification of a new entry
    ///
    /// When we receive an announcement for an entry we don't have,
    /// we send a Request to the author to get the full entry.
    pub(crate) fn handle_announce(
        &mut self,
        _sender: &Did,
        hash: ContentHash,
        author: Did,
        topic: String,
    ) -> Result<()> {
        debug!(
            topic = %topic,
            entry_hash = %hex::encode(hash),
            author_did = %author,
            message_type = "Announce",
            "Received gossip Announce"
        );
        icn_obs::metrics::gossip::announces_received_inc();

        // Check if we already have this entry
        if let Some(entries) = self.entries.get(&topic) {
            if entries.contains_key(&hash) {
                debug!(
                    entry_hash = %hex::encode(hash),
                    topic = %topic,
                    "Already have entry, skipping"
                );
                return Ok(());
            }
        }

        // Request full entry if we don't have it
        debug!(
            entry_hash = %hex::encode(hash),
            from_did = %author,
            topic = %topic,
            message_type = "Request",
            "Requesting missing entry"
        );
        self.send_message(Some(author), GossipMessage::Request { hash });

        Ok(())
    }

    /// Handle a Request message - request for a specific entry by hash
    ///
    /// Searches all topics for the requested entry and sends a Response
    /// back to the requester if found.
    pub(crate) fn handle_request(&mut self, sender: &Did, hash: ContentHash) -> Result<()> {
        icn_obs::metrics::gossip::requests_received_inc();
        debug!(
            peer_did = %sender,
            entry_hash = %hex::encode(hash),
            message_type = "Request",
            "Received gossip Request"
        );

        // Find entry across all topics
        for entries in self.entries.values() {
            if let Some(entry) = entries.get(&hash) {
                debug!(
                    entry_hash = %hex::encode(hash),
                    topic = %entry.topic,
                    to_did = %sender,
                    message_type = "Response",
                    "Found entry, sending Response"
                );

                // Send Response back to the requester
                self.send_message(
                    Some(sender.clone()),
                    GossipMessage::Response {
                        entry: entry.clone(),
                    },
                );

                return Ok(());
            }
        }

        debug!(
            entry_hash = %hex::encode(hash),
            peer_did = %sender,
            "Entry not found for Request"
        );
        Ok(())
    }

    /// Handle a Response message - full entry data in response to a Request
    ///
    /// Stores the received entry, which triggers:
    /// 1. Subscriber notifications
    /// 2. Vector clock merge
    /// 3. max_entries limit enforcement
    /// 4. Duplicate detection
    pub(crate) fn handle_response(&mut self, sender: &Did, entry: GossipEntry) -> Result<()> {
        icn_obs::metrics::gossip::responses_received_inc();
        debug!(
            peer_did = %sender,
            topic = %entry.topic,
            entry_hash = %hex::encode(entry.hash),
            entry_size = entry.data.len(),
            message_type = "Response",
            "Received gossip Response"
        );

        // Store the entry using store_entry() to ensure proper handling
        self.store_entry(entry)?;

        // Track metrics
        icn_obs::metrics::gossip::entries_received_inc();
        self.update_gauge_metrics();

        Ok(())
    }
}
