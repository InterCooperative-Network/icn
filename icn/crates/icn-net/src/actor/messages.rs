//! Message handling for the network actor

use anyhow::{Context, Result};
use icn_identity::Did;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, instrument, warn};

use crate::{
    protocol::{write_message, write_message_negotiated, NetworkMessage},
    NetworkMsg, NetworkStats, SessionManager,
};

use super::NetworkActor;

impl NetworkActor {
    /// Handle a single message
    pub(super) async fn handle_message(&mut self, msg: NetworkMsg) {
        match msg {
            NetworkMsg::GetPeers(tx) => {
                let peers = self.discovery.peers().await;
                let _ = tx.send(peers);
            }

            NetworkMsg::Dial {
                addr,
                did,
                response,
            } => {
                // Timeout for dial operation (30 seconds to allow for slow networks)
                const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

                let dial_future = async {
                    self.session_manager
                        .read()
                        .await
                        .dial(addr, did.as_str().to_string())
                        .await
                };

                // Allow bind_instead_of_map: when post-quantum feature is enabled,
                // the closure uses `?` which requires and_then, not map
                #[allow(clippy::bind_instead_of_map)]
                let result = tokio::time::timeout(DIAL_TIMEOUT, dial_future)
                    .await
                    .context("Timeout dialing peer")
                    .and_then(|r| r)
                    .and_then(|connection| {
                        // Increment connection counter
                        let stats = self.stats.clone();
                        tokio::spawn(async move {
                            stats.write().await.connections_total += 1;
                        });

                        // Track metrics
                        icn_obs::metrics::network::connections_total_inc();

                        // Spawn connection handler for outbound connection if handler is available
                        if let Some(handler) = self.incoming_handler.clone() {
                            let rate_limiter = self.rate_limiter.clone();
                            let replay_guard = self.replay_guard.clone();
                            let neighbor_sets = self.neighbor_sets.clone();
                            let topology_config = self.topology_config.clone();
                            let trust_graph = self.trust_graph.clone();
                            let session_manager = self.session_manager.clone();
                            let peer_connections = self.peer_connections.clone();
                            let blob_registry = self.blob_registry.clone();
                            let misbehavior_detector = self.misbehavior_detector.clone();
                            let identity_bundle = self.identity_bundle.clone();
                            let own_did = self.own_did.clone();

                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(
                                    connection.clone(),
                                    handler,
                                    rate_limiter,
                                    replay_guard,
                                    neighbor_sets,
                                    topology_config,
                                    trust_graph,
                                    session_manager,
                                    peer_connections,
                                    blob_registry,
                                    misbehavior_detector,
                                    identity_bundle,
                                    own_did,
                                )
                                .await
                                {
                                    warn!("Outbound connection handler error: {}", e);
                                }
                            });
                        }

                        // Send Hello message with DID-TLS binding and X25519 public key
                        let binding_info = self.identity_bundle.binding_info();
                        let x25519_public = *self.identity_bundle.x25519_public_bytes();
                        let version_info =
                            crate::VersionInfo::new(format!("icnd-{}", env!("CARGO_PKG_VERSION")));
                        let topology_info =
                            self.topology_config.as_ref().map(|topo_cfg| crate::TopologyInfo {
                                region: topo_cfg.region.clone(),
                                cluster_id: topo_cfg.cluster_id.clone(),
                                role: topo_cfg.role,
                            });

                        // Build Hello message with PQ binding proof if available
                        #[cfg(feature = "post-quantum")]
                        let hello_msg = {
                            let keypair = self
                                .identity_bundle
                                .keypair()
                                .context("Failed to load keypair for PQ binding")?;
                            let ml_dsa = keypair.pq_public_key().map(|pk| pk.as_bytes().to_vec());
                            let ml_kem = self
                                .identity_bundle
                                .kem_pq_public_bytes()
                                .map(|b| b.to_vec());

                            // Use hello_with_binding to include DID-PQ binding proof
                            NetworkMessage::hello_with_binding(
                                self.own_did.clone(),
                                did.clone(),
                                binding_info,
                                version_info,
                                topology_info,
                                x25519_public,
                                ml_dsa,
                                ml_kem,
                                &keypair,
                            )
                        };

                        #[cfg(not(feature = "post-quantum"))]
                        let hello_msg = NetworkMessage::hello(
                            self.own_did.clone(),
                            did.clone(),
                            binding_info,
                            version_info,
                            topology_info,
                            x25519_public,
                            None,
                            None,
                        );

                        let session_mgr = self.session_manager.clone();
                        tokio::spawn(async move {
                            if let Err(e) =
                                send_handshake_internal(session_mgr, &did, hello_msg).await
                            {
                                warn!("Failed to send Hello to {}: {}", did, e);
                            }
                        });
                        Ok(())
                    });

                let _ = response.send(result);
            }

            NetworkMsg::SendMessage {
                did,
                message,
                response,
            } => {
                let result = self.send_message_to_peer(&did, message).await;
                let _ = response.send(result);
            }

            NetworkMsg::Broadcast { message, response } => {
                let result = self.broadcast_message(message).await;
                let _ = response.send(result);
            }

            NetworkMsg::GetStats(tx) => {
                // Calculate stats on-demand
                let peers = self.discovery.peers().await;
                let connections = self.session_manager.read().await.connections().await;
                let total = self.stats.read().await.connections_total;

                let stats = NetworkStats {
                    peers_discovered: peers.len(),
                    connections_active: connections.len(),
                    connections_total: total,
                };

                // Update gauge metrics
                icn_obs::metrics::network::peers_discovered_set(stats.peers_discovered as u64);
                icn_obs::metrics::network::connections_active_set(stats.connections_active as u64);

                let _ = tx.send(stats);
            }
        }
    }

    /// Send a message to a specific peer
    #[instrument(skip(self, message), fields(peer_did = %did, message_type = message.payload.variant_name()))]
    pub(super) async fn send_message_to_peer(
        &self,
        did: &Did,
        message: NetworkMessage,
    ) -> Result<()> {
        // Timeout for the entire send operation (10 seconds)
        const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

        // Look up peer's negotiated capabilities for encoding selection
        let (use_postcard, use_compression) = {
            let connections = self.peer_connections.read().await;
            connections
                .get(did)
                .map(|peer_info| peer_info.encoding_flags())
                .unwrap_or((false, false)) // No peer info yet (pre-Hello) - use legacy encoding
        };

        let send_future = async {
            // Get connection for this peer
            let connections = self.session_manager.read().await.connections().await;
            let connection = connections
                .iter()
                .find(|(peer_did, _)| peer_did == did.as_str())
                .map(|(_, conn)| conn.clone())
                .context("No connection to peer")?;

            // Open a new stream (with timeout to prevent hanging on stream open)
            let (mut send, _recv) =
                tokio::time::timeout(std::time::Duration::from_secs(5), connection.open_bi())
                    .await
                    .context("Timeout opening stream to peer")?
                    .context("Failed to open stream")?;

            // Write message with negotiated encoding (with timeout to prevent hanging on slow writes)
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                write_message_negotiated(&mut send, &message, use_postcard, use_compression),
            )
            .await
            .context("Timeout writing message to peer")?
            .context("Failed to write message")?;

            send.finish().context("Failed to finish stream")?;

            Ok::<(), anyhow::Error>(())
        };

        // Apply overall timeout to the entire operation
        tokio::time::timeout(SEND_TIMEOUT, send_future)
            .await
            .context("Timeout sending message to peer")?
            .context("Failed to send message")?;

        // Track metrics
        icn_obs::metrics::network::messages_sent_inc();

        Ok(())
    }

    /// Broadcast a message to all connected peers
    #[instrument(skip(self, message), fields(message_type = message.payload.variant_name()))]
    pub(super) async fn broadcast_message(&self, message: NetworkMessage) -> Result<()> {
        // Timeout for each peer send operation (5 seconds)
        const PEER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

        let connections = self.session_manager.read().await.connections().await;

        // Get snapshot of peer capabilities for encoding selection
        let peer_caps = self.peer_connections.read().await;

        // Send to all connected peers
        let mut sent_count = 0;
        for (did_str, connection) in connections {
            // Look up peer's negotiated capabilities for encoding selection
            let (use_postcard, use_compression) = did_str
                .parse::<Did>()
                .ok()
                .and_then(|did| peer_caps.get(&did))
                .map(|peer_info| peer_info.encoding_flags())
                .unwrap_or((false, false)); // No peer info or invalid DID - use legacy encoding

            // Use timeout for each peer to prevent one slow peer from blocking broadcast
            let send_result = tokio::time::timeout(PEER_TIMEOUT, async {
                // Open a new stream and send the message with negotiated encoding
                let (mut send, _recv) = connection.open_bi().await?;
                write_message_negotiated(&mut send, &message, use_postcard, use_compression)
                    .await?;
                send.finish()?;
                Ok::<(), anyhow::Error>(())
            })
            .await;

            if send_result.is_ok() {
                sent_count += 1;
            } else {
                // Log timeout or error but continue with other peers
                warn!("Failed to broadcast to peer (timeout or error)");
            }
        }

        // Track metrics (one increment per successful send)
        for _ in 0..sent_count {
            icn_obs::metrics::network::messages_sent_inc();
        }

        Ok(())
    }
}

/// Send handshake message to a peer
pub(super) async fn send_handshake_internal(
    session_manager: Arc<RwLock<SessionManager>>,
    peer_did: &Did,
    handshake_msg: NetworkMessage,
) -> Result<()> {
    let connections = session_manager.read().await.connections().await;
    let connection = connections
        .iter()
        .find(|(did, _)| did == peer_did.as_str())
        .map(|(_, conn)| conn.clone())
        .context("No connection to peer")?;

    let (mut send, _recv) = connection
        .open_bi()
        .await
        .context("Failed to open stream")?;
    write_message(&mut send, &handshake_msg).await?;
    send.finish().context("Failed to finish stream")?;

    info!("Sent handshake to {}", peer_did);
    Ok(())
}
