//! Network initialization and message handler creation
//!
//! This module extracts the incoming message handler from the supervisor,
//! providing a cleaner separation of concerns for network message routing.

use icn_identity::Did;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Type alias for the gossip handle used in message routing
pub type GossipHandle = Arc<RwLock<icn_gossip::GossipActor>>;

/// Type alias for the network handle holder (set after NetworkActor spawns)
pub type NetworkHandleHolder = Arc<RwLock<Option<icn_net::NetworkHandle>>>;

/// Dependencies required for creating the incoming message handler
#[derive(Clone)]
pub struct MessageHandlerDeps {
    /// Handle to the gossip actor for message routing
    pub gossip_handle: GossipHandle,
    /// Holder for network handle (filled after spawn)
    pub network_handle_holder: NetworkHandleHolder,
    /// This node's DID for outgoing messages
    pub own_did: Did,
    /// Whether federation features are enabled
    pub federation_enabled: bool,
}

/// Create the incoming message handler that routes network messages to gossip
///
/// This handler processes incoming `NetworkMessage`s and routes them appropriately:
/// - Gossip messages → GossipActor
/// - Subscribe/Unsubscribe → GossipActor topic management
/// - Signed messages → Verify and route based on payload type
/// - PeerExchange → Federation peer discovery
pub fn create_incoming_handler(deps: MessageHandlerDeps) -> icn_net::IncomingMessageHandler {
    let gossip_handle = deps.gossip_handle;
    let network_handle_holder = deps.network_handle_holder;
    let own_did = deps.own_did;
    let federation_enabled = deps.federation_enabled;

    Arc::new(move |net_msg| {
        let sender_did = net_msg.from.clone();

        match net_msg.payload {
            icn_net::MessagePayload::Gossip(gossip_msg) => {
                // Spawn async task to avoid blocking the callback thread
                let gossip = gossip_handle.clone();
                let sender = sender_did.clone();
                tokio::spawn(async move {
                    let mut gossip = gossip.write().await;
                    if let Err(e) = gossip.handle_message(&sender, gossip_msg).await {
                        warn!("Failed to handle gossip message: {}", e);
                    }
                });
            }

            icn_net::MessagePayload::Subscribe { topics } => {
                info!(
                    "Received Subscribe from {} for topics: {:?}",
                    sender_did, topics
                );
                icn_obs::metrics::gossip::subscribes_received_inc();

                let gossip = gossip_handle.clone();
                let net_holder = network_handle_holder.clone();
                let own = own_did.clone();

                tokio::spawn(async move {
                    let mut gossip = gossip.write().await;
                    let mut acked_topics = Vec::new();

                    for topic in &topics {
                        match gossip.subscribe(topic, sender_did.clone()).await {
                            Ok(_) => {
                                info!("Subscribed {} to topic: {}", sender_did, topic);
                                acked_topics.push(topic.clone());
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to subscribe {} to topic {}: {}",
                                    sender_did, topic, e
                                );
                            }
                        }
                    }

                    // Send SubscribeAck back if we have any successful subscriptions
                    if !acked_topics.is_empty() {
                        icn_obs::metrics::gossip::subscribe_acks_sent_inc();
                        if let Some(net_handle) = net_holder.read().await.as_ref() {
                            let ack_msg = icn_net::NetworkMessage::subscribe_ack(
                                own,
                                sender_did.clone(),
                                acked_topics,
                            );
                            if let Err(e) = net_handle.send_message(sender_did, ack_msg).await {
                                warn!("Failed to send SubscribeAck: {}", e);
                            }
                        }
                    }
                });
            }

            icn_net::MessagePayload::Unsubscribe { topics } => {
                info!(
                    "Received Unsubscribe from {} for topics: {:?}",
                    sender_did, topics
                );
                icn_obs::metrics::gossip::unsubscribes_received_inc();

                let gossip = gossip_handle.clone();
                tokio::spawn(async move {
                    let mut gossip = gossip.write().await;
                    for topic in &topics {
                        match gossip.unsubscribe(topic, &sender_did) {
                            Ok(_) => {
                                info!("Unsubscribed {} from topic: {}", sender_did, topic);
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to unsubscribe {} from topic {}: {}",
                                    sender_did, topic, e
                                );
                            }
                        }
                    }
                });
            }

            icn_net::MessagePayload::SubscribeAck { topics } => {
                info!(
                    "Received SubscribeAck from {} for topics: {:?}",
                    sender_did, topics
                );
                // Track successful subscription acknowledgment
                // In a full implementation, this could update a local subscription registry
            }

            icn_net::MessagePayload::Ping { .. } => {
                // Ping/Pong handled by network actor
            }

            icn_net::MessagePayload::Pong { .. } => {
                // Ping/Pong handled by network actor
            }

            icn_net::MessagePayload::Handshake { .. } => {
                // Handshake handled internally by network actor
            }

            icn_net::MessagePayload::HandshakeAck => {
                // Handshake ack handled internally by network actor
            }

            icn_net::MessagePayload::Hello { .. } => {
                // Hello message with DID-TLS binding handled internally by network actor
            }

            icn_net::MessagePayload::Signed(ref envelope) => {
                // Signed messages have been verified by NetworkActor
                debug!(
                    "Received verified signed message from {} (seq={}, type={:?})",
                    envelope.from, envelope.sequence, envelope.payload_type
                );

                // Route based on payload type
                match envelope.payload_type {
                    icn_net::PayloadType::Gossip => {
                        // Decode gossip message from signed envelope
                        let gossip_msg: icn_gossip::GossipMessage = match envelope.decode_payload()
                        {
                            Ok(msg) => msg,
                            Err(e) => {
                                warn!(
                                    "Failed to decode gossip payload from {}: {}",
                                    envelope.from, e
                                );
                                return;
                            }
                        };

                        let gossip = gossip_handle.clone();
                        let sender = envelope.from.clone();
                        tokio::spawn(async move {
                            let mut gossip = gossip.write().await;
                            if let Err(e) = gossip.handle_message(&sender, gossip_msg).await {
                                warn!("Failed to handle gossip message from {}: {}", sender, e);
                            }
                        });
                    }

                    _ => {
                        debug!(
                            "Received signed message with unhandled payload type: {:?}",
                            envelope.payload_type
                        );
                    }
                }
            }

            icn_net::MessagePayload::PeerExchange(ref peer_msg) => {
                handle_peer_exchange(
                    peer_msg,
                    &sender_did,
                    federation_enabled,
                    network_handle_holder.clone(),
                );
            }

            icn_net::MessagePayload::Onion(_) => {
                // Onion messages are handled directly by NetworkActor
                debug!("Received Onion message in supervisor handler - this should not happen");
            }
        }
    })
}

/// Handle peer exchange messages for federation discovery
fn handle_peer_exchange(
    peer_msg: &icn_net::PeerExchangeMessage,
    sender_did: &Did,
    federation_enabled: bool,
    network_handle_holder: NetworkHandleHolder,
) {
    use icn_net::PeerExchangeMessage;

    match peer_msg {
        PeerExchangeMessage::Request { .. } => {
            // Requests are handled by NetworkActor directly
            debug!("Peer exchange request forwarded to handler (unexpected)");
        }
        PeerExchangeMessage::Response { peers, total_known } => {
            info!(
                "Received {} federation peers from {} (total: {})",
                peers.len(),
                sender_did,
                total_known
            );

            if federation_enabled {
                // Auto-dial discovered peers — prefer the first public endpoint, fall back to
                // the first endpoint of any kind if no public endpoint is available.
                let peers_to_dial: Vec<_> = peers
                    .iter()
                    .filter_map(|p| {
                        use icn_net::candidate::EndpointKind;
                        let addr = p
                            .endpoints
                            .iter()
                            .find(|e| e.kind == EndpointKind::Public)
                            .or_else(|| p.endpoints.first())
                            .map(|e| e.addr);
                        addr.map(|a| (p.did.clone(), a))
                    })
                    .collect();

                tokio::spawn(async move {
                    // Clone handle out of RwLock before awaiting dials — holding a
                    // Tokio RwLock guard across .await starves writers unnecessarily.
                    let net_handle = network_handle_holder.read().await.clone();
                    if let Some(net_handle) = net_handle {
                        for (did_str, addr) in peers_to_dial {
                            let peer_did = match icn_identity::Did::from_str(&did_str) {
                                Ok(d) => d,
                                Err(e) => {
                                    debug!("Skipping invalid DID {}: {}", did_str, e);
                                    continue;
                                }
                            };

                            info!("Auto-dialing discovered peer {} at {}", peer_did, addr);
                            match net_handle.dial(addr, peer_did.clone()).await {
                                Ok(_) => {
                                    icn_obs::metrics::peer_exchange::peers_dialed_inc();
                                }
                                Err(e) => {
                                    debug!("Failed to dial {}: {}", peer_did, e);
                                    icn_obs::metrics::peer_exchange::dial_failures_inc();
                                }
                            }
                        }
                    }
                });
            } else {
                for peer in peers {
                    debug!(
                        "Discovered peer (federation disabled): {} at {:?}",
                        peer.did, peer.endpoints
                    );
                }
            }
        }
        PeerExchangeMessage::Announce { peer } => {
            info!(
                "Peer announced by {}: {} at {:?}",
                sender_did, peer.did, peer.endpoints
            );

            if federation_enabled {
                use icn_net::candidate::EndpointKind;
                // Prefer Public endpoint for announced peers — Announce is broadcast to ALL
                // connected peers, not only LAN neighbors, so a Local-only address would be
                // unreachable for WAN recipients. Fall back to the first endpoint of any kind
                // if no Public endpoint is available.
                let addr_opt = peer
                    .endpoints
                    .iter()
                    .find(|e| e.kind == EndpointKind::Public)
                    .or_else(|| peer.endpoints.first())
                    .map(|e| e.addr);
                if let Some(addr) = addr_opt {
                    let peer_did_str = peer.did.clone();
                    tokio::spawn(async move {
                        // Clone handle out before awaiting dial — avoid holding RwLock across await.
                        let net_handle = network_handle_holder.read().await.clone();
                        if let Some(net_handle) = net_handle {
                            if let Ok(peer_did) = icn_identity::Did::from_str(&peer_did_str) {
                                info!("Auto-dialing announced peer {} at {}", peer_did, addr);
                                match net_handle.dial(addr, peer_did.clone()).await {
                                    Ok(_) => {
                                        icn_obs::metrics::peer_exchange::peers_dialed_inc();
                                    }
                                    Err(e) => {
                                        debug!("Failed to dial announced peer {}: {}", peer_did, e);
                                        icn_obs::metrics::peer_exchange::dial_failures_inc();
                                    }
                                }
                            }
                        }
                    });
                }
            }
        }
        PeerExchangeMessage::Unannounce { did } => {
            info!("Peer unannounced by {}: {}", sender_did, did);
            // Peer disconnected - no action needed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_handler_deps_clone() {
        // Verify MessageHandlerDeps is Clone (required for closure capture)
        // No oracle needed - test only verifies Clone trait
        let deps = MessageHandlerDeps {
            gossip_handle: Arc::new(RwLock::new(icn_gossip::GossipActor::new(
                icn_identity::KeyPair::generate().unwrap().did().clone(),
                None,
            ))),
            network_handle_holder: Arc::new(RwLock::new(None)),
            own_did: icn_identity::KeyPair::generate().unwrap().did().clone(),
            federation_enabled: false,
        };

        let _cloned = deps.clone();
    }
}
