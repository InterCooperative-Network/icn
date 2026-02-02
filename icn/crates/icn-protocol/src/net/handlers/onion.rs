//! Onion routing handler - privacy-preserving message relay
//!
//! Handles OnionMessage processing:
//! - Peeling layers when acting as a relay
//! - Extracting payloads when we're the destination
//! - Forwarding to next hop

use super::ConnectionContext;
use crate::protocol::NetworkMessage;
use icn_privacy::{OnionMessage, OnionRouter};
use tracing::{debug, info, warn};

impl ConnectionContext {
    /// Handle an onion-routed message
    ///
    /// Either:
    /// - Forward to next hop (if we're a relay)
    /// - Extract and process payload (if we're the destination)
    pub async fn handle_onion(&self, onion_msg: &OnionMessage) {
        debug!(
            "Received onion-routed message (hop_index={})",
            onion_msg.hop_index
        );
        icn_obs::metrics::privacy::onion_messages_received_inc();

        // Create temporary OnionRouter for this node
        let x25519_secret = self.identity_bundle.x25519_secret();
        let router = OnionRouter::new(self.own_did.clone(), x25519_secret);

        // Peel one layer off the onion
        match router.peel_layer(onion_msg.clone()) {
            Ok(Some((next_hop, peeled_onion))) => {
                // We are a relay - forward to next hop
                self.forward_onion(&next_hop, peeled_onion).await;
            }
            Ok(None) => {
                // We are the final destination - extract payload
                self.extract_onion_payload(&router, onion_msg).await;
            }
            Err(e) => {
                warn!("Failed to peel onion layer: {}", e);
                icn_obs::metrics::privacy::onion_routing_errors_inc();
            }
        }
    }

    /// Forward an onion message to the next hop
    async fn forward_onion(&self, next_hop: &icn_identity::Did, peeled_onion: OnionMessage) {
        info!(
            "Onion relay: forwarding to {} (hop_index={})",
            next_hop, peeled_onion.hop_index
        );
        icn_obs::metrics::privacy::onion_hops_forwarded_inc();

        // Get connection to next hop
        let sm = self.session_manager.read().await;
        let conn_opt = sm
            .connections()
            .await
            .iter()
            .find(|(did, _)| *did == next_hop.as_str())
            .map(|(_, conn)| conn.clone());
        drop(sm);

        if let Some(conn) = conn_opt {
            let forward_msg = NetworkMessage::onion(next_hop.clone(), peeled_onion);

            match conn.open_bi().await {
                Ok((mut send, _recv)) => {
                    if let Err(e) = crate::protocol::write_message(&mut send, &forward_msg).await {
                        warn!("Failed to forward onion to {}: {}", next_hop, e);
                    } else if let Err(e) = send.finish() {
                        warn!("Failed to finish onion stream to {}: {}", next_hop, e);
                    } else {
                        debug!("Forwarded onion to next hop: {}", next_hop);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to open stream for onion forward to {}: {}",
                        next_hop, e
                    );
                }
            }
        } else {
            warn!("No connection to next hop {} for onion relay", next_hop);
        }
    }

    /// Extract and process an onion payload (we're the destination)
    async fn extract_onion_payload(&self, router: &OnionRouter, onion_msg: &OnionMessage) {
        match router.extract_payload(onion_msg) {
            Ok(payload) => {
                info!(
                    "Onion message arrived at destination ({} bytes)",
                    payload.len()
                );
                icn_obs::metrics::privacy::onion_messages_delivered_inc();

                // Attempt to deserialize as NetworkMessage and forward
                match icn_encoding::decode::<NetworkMessage>(&payload) {
                    Ok(inner_msg) => {
                        debug!(
                            "Extracted inner message: {:?}",
                            inner_msg.payload.variant_name()
                        );
                        self.forward_to_handler(inner_msg);
                    }
                    Err(e) => {
                        warn!("Failed to deserialize onion payload: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to extract onion payload: {}", e);
            }
        }
    }
}
