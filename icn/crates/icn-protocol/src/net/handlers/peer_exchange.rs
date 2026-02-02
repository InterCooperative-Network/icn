//! Peer exchange protocol handler - cross-network peer discovery
//!
//! Handles PeerExchangeMessage variants:
//! - Request: Respond with known peers
//! - Response: Process discovered peers
//! - Announce: New peer introduction
//! - Unannounce: Peer departure notification

use super::ConnectionContext;
use crate::protocol::{KnownPeer, NetworkMessage, PeerExchangeMessage};
use icn_identity::Did;
use tracing::{info, warn};

impl ConnectionContext {
    /// Handle a PeerExchange message
    pub async fn handle_peer_exchange(
        &self,
        connection: &quinn::Connection,
        message: NetworkMessage,
        from: &Did,
        peer_msg: &PeerExchangeMessage,
    ) {
        match peer_msg {
            PeerExchangeMessage::Request {
                max_peers,
                network_filter,
            } => {
                self.handle_peer_exchange_request(connection, from, *max_peers, network_filter)
                    .await;
            }
            PeerExchangeMessage::Response { peers, total_known } => {
                self.handle_peer_exchange_response(message, from, peers, *total_known);
            }
            PeerExchangeMessage::Announce { peer } => {
                self.handle_peer_announce(message, peer);
            }
            PeerExchangeMessage::Unannounce { did } => {
                self.handle_peer_unannounce(message, did);
            }
        }
    }

    /// Handle peer exchange request - respond with known peers
    async fn handle_peer_exchange_request(
        &self,
        connection: &quinn::Connection,
        from: &Did,
        max_peers: usize,
        network_filter: &Option<String>,
    ) {
        info!(
            "Received peer exchange request from {} (max={}, filter={:?})",
            from, max_peers, network_filter
        );
        icn_obs::metrics::peer_exchange::requests_received_inc();

        // Gather known peers from session manager
        let sm = self.session_manager.read().await;
        let connections = sm.connections().await;
        drop(sm);

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut known_peers: Vec<KnownPeer> = Vec::new();

        for (did_str, conn) in connections {
            // Skip the requesting peer
            if did_str == from.as_str() {
                continue;
            }

            known_peers.push(KnownPeer {
                did: did_str.to_string(),
                addresses: vec![conn.remote_address().to_string()],
                version: "0.1.0".to_string(),
                network_name: network_filter.clone(),
                observed_trust: None,
                last_seen: now_secs,
                is_local: false,
            });

            if known_peers.len() >= max_peers {
                break;
            }
        }

        let total_known = known_peers.len();

        // Send response
        let response = NetworkMessage::peer_exchange_response(
            self.own_did.clone(),
            from.clone(),
            known_peers,
            total_known,
        );

        let connection_clone = connection.clone();
        let from_did = from.clone();
        tokio::spawn(async move {
            match connection_clone.open_bi().await {
                Ok((mut send_stream, _recv)) => {
                    if let Err(e) =
                        crate::protocol::write_message(&mut send_stream, &response).await
                    {
                        warn!("Failed to send peer exchange response: {}", e);
                    } else if let Err(e) = send_stream.finish() {
                        warn!("Failed to finish peer exchange response stream: {}", e);
                    } else {
                        info!("Sent {} peers to {}", total_known, from_did);
                        icn_obs::metrics::peer_exchange::responses_sent_inc();
                    }
                }
                Err(e) => {
                    warn!("Failed to open stream for peer exchange response: {}", e);
                }
            }
        });
    }

    /// Handle peer exchange response - process discovered peers
    fn handle_peer_exchange_response(
        &self,
        message: NetworkMessage,
        from: &Did,
        peers: &[KnownPeer],
        total_known: usize,
    ) {
        info!(
            "Received {} peers from {} (total known: {})",
            peers.len(),
            from,
            total_known
        );
        icn_obs::metrics::peer_exchange::responses_received_inc();
        icn_obs::metrics::peer_exchange::peers_discovered_add(peers.len() as u64);

        // Forward to handler for processing
        self.forward_to_handler(message);
    }

    /// Handle peer announce - new peer introduction
    fn handle_peer_announce(&self, message: NetworkMessage, peer: &KnownPeer) {
        info!("Peer announced: {} at {:?}", peer.did, peer.addresses);
        icn_obs::metrics::peer_exchange::announces_received_inc();
        self.forward_to_handler(message);
    }

    /// Handle peer unannounce - peer departure
    fn handle_peer_unannounce(&self, message: NetworkMessage, did: &str) {
        info!("Peer unannounced: {}", did);
        icn_obs::metrics::peer_exchange::unannounces_received_inc();
        self.forward_to_handler(message);
    }
}
