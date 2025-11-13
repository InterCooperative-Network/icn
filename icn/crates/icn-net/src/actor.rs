//! Network actor - coordinates discovery and session management
//!
//! The network actor provides a unified interface for:
//! - Peer discovery via mDNS
//! - QUIC session management
//! - Automatic connection establishment to discovered peers
//! - Connection lifecycle management

use anyhow::{Context, Result};
use icn_identity::{Did, KeyPair};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, warn};

use crate::{
    protocol::{NetworkMessage, read_message, write_message, MessagePayload},
    rate_limit::{RateLimitConfig, RateLimiter},
    topology::{NeighborLimitsConfig, NeighborSets, NodeRole, PeerId, TopologyConfig, TopologyInfo},
    Discovery, PeerInfo, SessionManager,
};

/// Callback for handling incoming network messages
pub type IncomingMessageHandler = Arc<dyn Fn(NetworkMessage) + Send + Sync>;

/// Messages that can be sent to the network actor
#[derive(Debug)]
pub enum NetworkMsg {
    /// Get all discovered peers
    GetPeers(oneshot::Sender<Vec<PeerInfo>>),

    /// Dial a specific peer
    Dial {
        addr: SocketAddr,
        did: Did,
        response: oneshot::Sender<Result<()>>,
    },

    /// Send a network message to a specific peer
    SendMessage {
        did: Did,
        message: NetworkMessage,
        response: oneshot::Sender<Result<()>>,
    },

    /// Broadcast a message to all connected peers
    Broadcast {
        message: NetworkMessage,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get connection statistics
    GetStats(oneshot::Sender<NetworkStats>),
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub peers_discovered: usize,
    pub connections_active: usize,
    pub connections_total: u64,
}

/// Handle to interact with the network actor
#[derive(Clone)]
pub struct NetworkHandle {
    tx: mpsc::Sender<NetworkMsg>,
}

impl NetworkHandle {
    /// Get all discovered peers
    pub async fn get_peers(&self) -> Result<Vec<PeerInfo>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::GetPeers(tx))
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")
    }

    /// Dial a peer
    pub async fn dial(&self, addr: SocketAddr, did: Did) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::Dial {
                addr,
                did,
                response: tx,
            })
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Send a network message to a specific peer
    pub async fn send_message(&self, did: Did, message: NetworkMessage) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::SendMessage {
                did,
                message,
                response: tx,
            })
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Broadcast a message to all connected peers
    pub async fn broadcast(&self, message: NetworkMessage) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::Broadcast {
                message,
                response: tx,
            })
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Get network statistics
    pub async fn get_stats(&self) -> Result<NetworkStats> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::GetStats(tx))
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")
    }
}

/// Network actor state
pub struct NetworkActor {
    discovery: Discovery,
    session_manager: Arc<RwLock<SessionManager>>,
    stats: Arc<RwLock<NetworkStats>>,
    rx: mpsc::Receiver<NetworkMsg>,
    incoming_handler: Option<IncomingMessageHandler>,
    rate_limiter: Arc<RateLimiter>,
    own_did: Did,
    neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
    topology_config: Option<TopologyConfig>,
    trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
}

impl NetworkActor {
    /// Start the network actor
    ///
    /// Initializes discovery and session management on the given address.
    /// If trust_graph is provided, enables trust-gated rate limiting with different
    /// limits for different trust classes.
    /// If topology_config is provided, enables topology-aware neighbor management.
    pub async fn spawn(
        keypair: &KeyPair,
        listen_addr: SocketAddr,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
        incoming_handler: Option<IncomingMessageHandler>,
        trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
        trust_gated_config: Option<crate::rate_limit::TrustGatedRateLimitConfig>,
        fallback_config: Option<RateLimitConfig>,
        topology_config: Option<TopologyConfig>,
    ) -> Result<NetworkHandle> {
        let did = keypair.did().clone();

        info!("Network actor starting for DID: {}", did);

        // Start discovery
        let mut discovery = Discovery::new();
        discovery
            .start(did.clone(), listen_addr)
            .await
            .context("Failed to start discovery")?;

        // Start session manager with trust-gated TLS if trust_graph provided
        let mut session_manager = SessionManager::new();
        let tls_trust_threshold = trust_gated_config.as_ref().map(|cfg| cfg.min_trust_threshold);
        session_manager
            .start(keypair, listen_addr, trust_graph.clone(), tls_trust_threshold)
            .await
            .context("Failed to start session manager")?;

        // Create stats
        let stats = Arc::new(RwLock::new(NetworkStats {
            peers_discovered: 0,
            connections_active: 0,
            connections_total: 0,
        }));

        // Create channel
        let (tx, rx) = mpsc::channel(32);

        // Wrap session_manager in Arc for sharing between tasks
        let session_manager = Arc::new(RwLock::new(session_manager));

        // Create rate limiter (trust-gated if trust_graph provided, otherwise fallback)
        let rate_limiter = if let Some(ref tg) = trust_graph {
            let config = trust_gated_config.unwrap_or_else(|| {
                info!("Using default trust-gated rate limit config");
                crate::rate_limit::TrustGatedRateLimitConfig::default()
            });
            info!("Trust-gated rate limiting enabled");
            Arc::new(RateLimiter::new_trust_gated(config, tg.clone()))
        } else {
            let config = fallback_config.unwrap_or_else(|| {
                info!("Using default fallback rate limit config");
                RateLimitConfig::default()
            });
            info!("Using fallback rate limiting (no trust graph)");
            Arc::new(RateLimiter::new(config))
        };

        // Initialize neighbor sets if topology is enabled
        let neighbor_sets = if let Some(ref topo_cfg) = topology_config {
            let own_topology = TopologyInfo {
                region: topo_cfg.region.clone(),
                cluster_id: topo_cfg.cluster_id.clone(),
                role: topo_cfg.role,
            };
            info!("Topology-aware networking enabled: region={}, cluster={}",
                  topo_cfg.region, topo_cfg.cluster_id);
            Some(Arc::new(RwLock::new(NeighborSets::new(own_topology))))
        } else {
            info!("Topology-aware networking disabled");
            None
        };

        // Spawn incoming connection handler if handler is provided
        if let Some(handler) = incoming_handler.clone() {
            let session_manager_clone = session_manager.clone();
            let rate_limiter_clone = rate_limiter.clone();
            let neighbor_sets_clone = neighbor_sets.clone();
            let topology_config_clone = topology_config.clone();
            let trust_graph_clone = trust_graph.clone();
            let own_did_clone = did.clone();
            let shutdown_rx = shutdown_tx.subscribe();
            tokio::spawn(async move {
                if let Err(e) = Self::handle_incoming_connections(
                    session_manager_clone,
                    handler,
                    rate_limiter_clone,
                    neighbor_sets_clone,
                    topology_config_clone,
                    trust_graph_clone,
                    own_did_clone,
                    shutdown_rx,
                )
                .await
                {
                    warn!("Incoming connection handler error: {}", e);
                }
            });
        }

        // Create actor
        let actor = NetworkActor {
            discovery,
            session_manager,
            stats: stats.clone(),
            rx,
            incoming_handler,
            rate_limiter,
            own_did: did.clone(),
            neighbor_sets: neighbor_sets.clone(),
            topology_config: topology_config.clone(),
            trust_graph: trust_graph.clone(),
        };

        // Spawn actor task
        let stats_clone = stats.clone();
        tokio::spawn(async move {
            if let Err(e) = actor.run(shutdown_tx, stats_clone).await {
                warn!("Network actor error: {}", e);
            }
        });

        Ok(NetworkHandle { tx })
    }

    /// Run the network actor event loop
    async fn run(
        mut self,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
        _stats: Arc<RwLock<NetworkStats>>,
    ) -> Result<()> {
        info!("Network actor running");

        let mut shutdown_rx = shutdown_tx.subscribe();

        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Some(msg) => self.handle_message(msg).await,
                        None => {
                            info!("Network actor channel closed");
                            break;
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Network actor received shutdown signal");
                    break;
                }
            }
        }

        // Shutdown services
        self.session_manager.write().await.stop().await?;
        self.discovery.stop().await?;

        info!("Network actor stopped");
        Ok(())
    }

    /// Handle a single message
    async fn handle_message(&mut self, msg: NetworkMsg) {
        match msg {
            NetworkMsg::GetPeers(tx) => {
                let peers = self.discovery.peers().await;
                let _ = tx.send(peers);
            }

            NetworkMsg::Dial { addr, did, response } => {
                // Timeout for dial operation (30 seconds to allow for slow networks)
                const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

                let dial_future = async {
                    self.session_manager
                        .read()
                        .await
                        .dial(addr, did.as_str().to_string())
                        .await
                };

                let result = tokio::time::timeout(DIAL_TIMEOUT, dial_future)
                    .await
                    .context("Timeout dialing peer")
                    .and_then(|r| r)
                    .map(|_| {
                        // Increment connection counter
                        let stats = self.stats.clone();
                        tokio::spawn(async move {
                            stats.write().await.connections_total += 1;
                        });

                        // Track metrics
                        icn_obs::metrics::network::connections_total_inc();

                        // Send handshake if topology is enabled
                        if let Some(ref topo_cfg) = self.topology_config {
                            let handshake_msg = NetworkMessage::handshake(
                                self.own_did.clone(),
                                did.clone(),
                                topo_cfg.region.clone(),
                                topo_cfg.cluster_id.clone(),
                                format!("{:?}", topo_cfg.role),
                            );

                            let session_mgr = self.session_manager.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::send_handshake_internal(session_mgr, &did, handshake_msg).await {
                                    warn!("Failed to send handshake to {}: {}", did, e);
                                }
                            });
                        }
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
    async fn send_message_to_peer(&self, did: &Did, message: NetworkMessage) -> Result<()> {
        // Timeout for the entire send operation (10 seconds)
        const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

        let send_future = async {
            // Get connection for this peer
            let connections = self.session_manager.read().await.connections().await;
            let connection = connections
                .iter()
                .find(|(peer_did, _)| peer_did == did.as_str())
                .map(|(_, conn)| conn.clone())
                .context("No connection to peer")?;

            // Open a new stream (with timeout to prevent hanging on stream open)
            let (mut send, _recv) = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                connection.open_bi(),
            )
            .await
            .context("Timeout opening stream to peer")?
            .context("Failed to open stream")?;

            // Write message (with timeout to prevent hanging on slow writes)
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                write_message(&mut send, &message),
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
    async fn broadcast_message(&self, message: NetworkMessage) -> Result<()> {
        // Timeout for each peer send operation (5 seconds)
        const PEER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

        let connections = self.session_manager.read().await.connections().await;

        // Send to all connected peers
        let mut sent_count = 0;
        for (_did, connection) in connections {
            // Use timeout for each peer to prevent one slow peer from blocking broadcast
            let send_result = tokio::time::timeout(PEER_TIMEOUT, async {
                // Open a new stream and send the message
                let (mut send, _recv) = connection.open_bi().await?;
                write_message(&mut send, &message).await?;
                send.finish()?;
                Ok::<(), anyhow::Error>(())
            }).await;

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

    /// Handle incoming QUIC connections
    async fn handle_incoming_connections(
        session_manager: Arc<RwLock<SessionManager>>,
        handler: IncomingMessageHandler,
        rate_limiter: Arc<RateLimiter>,
        neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
        topology_config: Option<TopologyConfig>,
        trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
        own_did: Did,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        info!("Starting incoming connection handler");

        loop {
            // Check for shutdown signal first (non-blocking)
            match shutdown_rx.try_recv() {
                Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    info!("Incoming connection handler received shutdown signal");
                    break;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) |
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    // Continue to accept connections
                }
            }

            // Accept with timeout to periodically check shutdown
            let conn_result = {
                let guard = session_manager.read().await;
                match tokio::time::timeout(tokio::time::Duration::from_millis(100), guard.accept()).await {
                    Ok(result) => Some(result),
                    Err(_) => None, // Timeout
                }
            };

            if let Some(conn_result) = conn_result {
                match conn_result {
                    Ok(Some(connection)) => {
                        // Spawn handler for this connection
                        let handler_clone = handler.clone();
                        let rate_limiter_clone = rate_limiter.clone();
                        let neighbor_sets_clone = neighbor_sets.clone();
                        let topology_config_clone = topology_config.clone();
                        let trust_graph_clone = trust_graph.clone();
                        let session_mgr_clone = session_manager.clone();
                        let own_did_clone = own_did.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(
                                connection,
                                handler_clone,
                                rate_limiter_clone,
                                neighbor_sets_clone,
                                topology_config_clone,
                                trust_graph_clone,
                                session_mgr_clone,
                                own_did_clone,
                            ).await {
                                warn!("Connection handler error: {}", e);
                            }
                        });
                    }
                    Ok(None) => {
                        info!("Session manager shut down");
                        break;
                    }
                    Err(e) => {
                        warn!("Failed to accept connection: {}", e);
                    }
                }
            }
        }

        info!("Incoming connection handler stopped");
        Ok(())
    }

    /// Send handshake message to a peer
    async fn send_handshake_internal(
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

        let (mut send, _recv) = connection.open_bi().await.context("Failed to open stream")?;
        write_message(&mut send, &handshake_msg).await?;
        send.finish().context("Failed to finish stream")?;

        info!("Sent handshake to {}", peer_did);
        Ok(())
    }

    /// Handle a single QUIC connection (process all incoming streams)
    async fn handle_connection(
        connection: quinn::Connection,
        handler: IncomingMessageHandler,
        rate_limiter: Arc<RateLimiter>,
        neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
        topology_config: Option<TopologyConfig>,
        trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
        session_manager: Arc<RwLock<SessionManager>>,
        own_did: Did,
    ) -> Result<()> {
        info!("Handling connection from {}", connection.remote_address());

        loop {
            // Accept incoming bidirectional stream
            match connection.accept_bi().await {
                Ok((mut send, mut recv)) => {
                    // Read network message
                    match read_message(&mut recv).await {
                        Ok(message) => {
                            // Check rate limit BEFORE processing message
                            let allowed = rate_limiter.check_rate_limit(&message.from).await;

                            if !allowed {
                                warn!(
                                    "Rate limited message from {} (exceeded limit)",
                                    message.from
                                );

                                // Track rate limiting metric
                                icn_obs::metrics::network::messages_rate_limited_inc();

                                // Drop the message (don't call handler)
                                continue;
                            }

                            info!("Received message from {}", message.from);

                            // Track metrics
                            icn_obs::metrics::network::messages_received_inc();

                            // Handle handshake messages internally
                            match &message.payload {
                                MessagePayload::Handshake { region, cluster_id, role } => {
                                    info!("Received handshake from {} (region={}, cluster={})",
                                          message.from, region, cluster_id);

                                    // Add peer to neighbor sets if topology is enabled
                                    if let Some(ref sets) = neighbor_sets {
                                        let peer_topology = TopologyInfo {
                                            region: region.clone(),
                                            cluster_id: cluster_id.clone(),
                                            role: match role.as_str() {
                                                "Edge" => NodeRole::Edge,
                                                "Rendezvous" => NodeRole::Rendezvous,
                                                "Archive" => NodeRole::Archive,
                                                _ => NodeRole::Edge,
                                            },
                                        };

                                        // Get trust score if available
                                        let trust_score = if let Some(ref tg) = trust_graph {
                                            tg.read().await.compute_trust_score(&message.from).unwrap_or(0.0) as f32
                                        } else {
                                            0.5 // Default if no trust graph
                                        };

                                        // Add to neighbor sets
                                        let limits = topology_config.as_ref()
                                            .map(|cfg| cfg.neighbor_limits.clone())
                                            .unwrap_or_else(|| NeighborLimitsConfig {
                                                max_local_cluster: 50,
                                                max_regional: 30,
                                                max_backbone: 20,
                                                max_trusted: 10,
                                            });

                                        sets.write().await.add_neighbor(
                                            PeerId(message.from.clone()),
                                            peer_topology,
                                            None, // RTT not measured yet
                                            trust_score,
                                            &limits,
                                        );

                                        // Update metrics
                                        let sets_read = sets.read().await;
                                        icn_obs::metrics::topology::neighbors_by_set_update(
                                            sets_read.local_cluster.len(),
                                            sets_read.regional.len(),
                                            sets_read.backbone.len(),
                                            sets_read.trusted.len(),
                                        );
                                    }

                                    // Send handshake ack
                                    let ack_msg = NetworkMessage::handshake_ack(own_did.clone(), message.from.clone());
                                    if let Err(e) = write_message(&mut send, &ack_msg).await {
                                        warn!("Failed to send handshake ack: {}", e);
                                    }
                                }
                                MessagePayload::HandshakeAck => {
                                    info!("Received handshake ack from {}", message.from);
                                    // Nothing to do, just acknowledgement
                                }
                                _ => {
                                    // Forward other messages to the external handler
                                    handler(message);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to read message: {}", e);
                        }
                    }

                    // Close the stream
                    let _ = send.finish();
                }
                Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                    info!("Connection closed by peer");
                    break;
                }
                Err(e) => {
                    warn!("Error accepting stream: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires network interfaces
    async fn test_network_actor_start() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let keypair = KeyPair::generate().unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        let handle = NetworkActor::spawn(&keypair, addr, shutdown_tx.clone(), None, None, None, None, None)
            .await
            .unwrap();

        // Should be able to get stats
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.peers_discovered, 0);
        assert_eq!(stats.connections_active, 0);

        // Shutdown
        let _ = shutdown_tx.send(());
    }
}
