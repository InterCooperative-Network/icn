//! Network actor - coordinates discovery and session management
//!
//! The network actor provides a unified interface for:
//! - Peer discovery via mDNS
//! - QUIC session management
//! - Automatic connection establishment to discovered peers
//! - Connection lifecycle management

use anyhow::{Context, Result};
use icn_identity::{Did, IdentityBundle};
#[cfg(test)]
use icn_identity::KeyPair;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, warn, instrument};

use crate::{
    protocol::{NetworkMessage, read_message, write_message, MessagePayload},
    rate_limit::{RateLimitConfig, RateLimiter},
    replay_guard::ReplayGuard,
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
    neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
    peer_x25519_keys: Option<Arc<RwLock<std::collections::HashMap<Did, [u8; 32]>>>>,
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

    /// Send an encrypted message to a specific peer
    ///
    /// This is a convenience method that:
    /// 1. Retrieves the peer's X25519 public key
    /// 2. Encrypts the payload using EncryptedEnvelope
    /// 3. Signs the encrypted envelope
    /// 4. Sends the message
    ///
    /// Returns an error if the peer's X25519 key is not available (no Hello exchange yet)
    pub async fn send_encrypted_message(
        &self,
        recipient: &Did,
        sender_keypair: &icn_identity::KeyPair,
        sender_x25519_secret: &x25519_dalek::StaticSecret,
        sequence: u64,
        payload: &[u8],
    ) -> Result<()> {
        use crate::{EncryptedEnvelope, SignedEnvelope, NetworkMessage};

        // Get recipient's X25519 public key
        let recipient_public_key_bytes = self
            .get_peer_x25519_key(recipient)
            .await
            .context("Recipient X25519 public key not available - no Hello exchange yet")?;

        let recipient_public_key = x25519_dalek::PublicKey::from(recipient_public_key_bytes);

        // Create encrypted envelope
        let encrypted = EncryptedEnvelope::encrypt(
            &sender_keypair.did(),
            recipient,
            sequence,
            sender_x25519_secret,
            &recipient_public_key,
            payload,
        )?;

        // Sign the encrypted envelope
        let signed = SignedEnvelope::new(
            &sender_keypair.did(),
            sender_keypair,
            sequence,
            crate::PayloadType::Encrypted,
            bincode::serialize(&encrypted)?,
        )?;

        // Send via network
        let message = NetworkMessage::signed(Some(recipient.clone()), signed);
        self.send_message(recipient.clone(), message).await
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

    /// Sample peers based on scope (for gossip fanout)
    /// Returns a list of peer DIDs suitable for the given scope
    pub async fn sample_peers(&self, scope: icn_gossip::Scope, count: usize) -> Vec<Did> {
        if let Some(ref sets) = self.neighbor_sets {
            let sets_read = sets.read().await;
            sets_read.sample(scope, count).into_iter().map(|peer_id| peer_id.0).collect()
        } else {
            // No topology support - return empty list (fall back to broadcast)
            Vec::new()
        }
    }

    /// Get a peer's X25519 public key for end-to-end encryption
    ///
    /// Returns None if the peer's key hasn't been received yet (no Hello message exchange)
    pub async fn get_peer_x25519_key(&self, did: &Did) -> Option<[u8; 32]> {
        let keys = self.peer_x25519_keys.as_ref()?;
        keys.read().await.get(did).copied()
    }

    /// Get reference to neighbor sets (for testing and inspection)
    pub fn neighbor_sets(&self) -> &Option<Arc<RwLock<NeighborSets>>> {
        &self.neighbor_sets
    }

    /// Export network state for persistence
    ///
    /// This exports peer X25519 public keys so encrypted communication
    /// can resume immediately after restart without new key exchange.
    ///
    /// Peer addresses are not exported (rediscovered via mDNS).
    pub async fn export_state(&self) -> icn_snapshot::NetworkState {
        let peer_x25519_keys = if let Some(ref keys) = self.peer_x25519_keys {
            keys.read().await
                .iter()
                .map(|(did, key)| (did.to_string(), *key))
                .collect()
        } else {
            std::collections::HashMap::new()
        };

        // Peer addresses not exported (rediscovered via mDNS)
        let peer_addresses = std::collections::HashMap::new();

        icn_snapshot::NetworkState {
            peer_x25519_keys,
            peer_addresses,
        }
    }

    /// Restore network state from persistence
    ///
    /// This restores peer X25519 public keys so encrypted communication
    /// works immediately after restart without new key exchange.
    ///
    /// Peer addresses are not restored (rediscovered via mDNS).
    pub async fn restore_state(&self, state: icn_snapshot::NetworkState) -> Result<()> {
        if let Some(ref keys) = self.peer_x25519_keys {
            let mut keys_write = keys.write().await;
            for (did_str, key) in state.peer_x25519_keys {
                let did = Did::from_str(&did_str)
                    .context("Failed to parse DID from network state")?;
                keys_write.insert(did, key);
            }
            tracing::info!("✅ Restored {} peer X25519 keys from snapshot", keys_write.len());
        }
        Ok(())
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
    replay_guard: Arc<RwLock<ReplayGuard>>,
    own_did: Did,
    identity_bundle: IdentityBundle,
    neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
    topology_config: Option<TopologyConfig>,
    trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
    /// Peer X25519 public keys (for end-to-end encryption)
    peer_x25519_keys: Arc<RwLock<std::collections::HashMap<Did, [u8; 32]>>>,
}

impl NetworkActor {
    /// Start the network actor
    ///
    /// Initializes discovery and session management on the given address.
    /// If trust_graph is provided, enables trust-gated rate limiting with different
    /// limits for different trust classes.
    /// If topology_config is provided, enables topology-aware neighbor management.
    pub async fn spawn(
        identity_bundle: IdentityBundle,
        listen_addr: SocketAddr,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
        incoming_handler: Option<IncomingMessageHandler>,
        trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
        trust_gated_config: Option<crate::rate_limit::TrustGatedRateLimitConfig>,
        fallback_config: Option<RateLimitConfig>,
        topology_config: Option<TopologyConfig>,
    ) -> Result<NetworkHandle> {
        let did = identity_bundle.did().clone();

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
            .start(identity_bundle.keypair(), listen_addr, trust_graph.clone(), tls_trust_threshold)
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

        // Create replay guard for signed message verification
        // 300 second clock skew tolerance, 3600 second peer age limit
        let replay_guard = Arc::new(RwLock::new(ReplayGuard::new(300, 3600)));
        info!("Replay protection enabled (300s clock skew, 3600s peer age limit)");

        // Create X25519 key store
        let peer_x25519_keys = Arc::new(RwLock::new(std::collections::HashMap::new()));

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
            let replay_guard_clone = replay_guard.clone();
            let neighbor_sets_clone = neighbor_sets.clone();
            let topology_config_clone = topology_config.clone();
            let trust_graph_clone = trust_graph.clone();
            let peer_x25519_keys_clone = peer_x25519_keys.clone();
            let identity_bundle_clone = identity_bundle.clone();
            let own_did_clone = did.clone();
            let shutdown_rx = shutdown_tx.subscribe();
            tokio::spawn(async move {
                if let Err(e) = Self::handle_incoming_connections(
                    session_manager_clone,
                    handler,
                    rate_limiter_clone,
                    replay_guard_clone,
                    neighbor_sets_clone,
                    topology_config_clone,
                    trust_graph_clone,
                    peer_x25519_keys_clone,
                    identity_bundle_clone,
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
            replay_guard: replay_guard.clone(),
            own_did: did.clone(),
            identity_bundle,
            neighbor_sets: neighbor_sets.clone(),
            topology_config: topology_config.clone(),
            trust_graph: trust_graph.clone(),
            peer_x25519_keys: peer_x25519_keys.clone(),
        };

        // Spawn actor task
        let stats_clone = stats.clone();
        tokio::spawn(async move {
            if let Err(e) = actor.run(shutdown_tx, stats_clone).await {
                warn!("Network actor error: {}", e);
            }
        });

        Ok(NetworkHandle {
            tx,
            neighbor_sets: neighbor_sets.clone(),
            peer_x25519_keys: Some(peer_x25519_keys),
        })
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
                    .map(|connection| {
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
                            let peer_x25519_keys = self.peer_x25519_keys.clone();
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
                                    peer_x25519_keys,
                                    identity_bundle,
                                    own_did,
                                ).await {
                                    warn!("Outbound connection handler error: {}", e);
                                }
                            });
                        }

                        // Send Hello message with DID-TLS binding and X25519 public key
                        let binding_info = self.identity_bundle.binding_info();
                        let x25519_public = *self.identity_bundle.x25519_public_bytes();
                        let version_info = crate::VersionInfo::new(
                            format!("icnd-{}", env!("CARGO_PKG_VERSION"))
                        );
                        let topology_info = self.topology_config.as_ref().map(|topo_cfg| {
                            TopologyInfo {
                                region: topo_cfg.region.clone(),
                                cluster_id: topo_cfg.cluster_id.clone(),
                                role: topo_cfg.role,
                            }
                        });

                        let hello_msg = NetworkMessage::hello(
                            self.own_did.clone(),
                            did.clone(),
                            binding_info,
                            version_info,
                            topology_info,
                            x25519_public,
                        );

                        let session_mgr = self.session_manager.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::send_handshake_internal(session_mgr, &did, hello_msg).await {
                                warn!("Failed to send Hello to {}: {}", did, e);
                            }
                        });
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
    #[instrument(skip(self, message), fields(message_type = message.payload.variant_name()))]
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
        replay_guard: Arc<RwLock<ReplayGuard>>,
        neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
        topology_config: Option<TopologyConfig>,
        trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
        peer_x25519_keys: Arc<RwLock<std::collections::HashMap<Did, [u8; 32]>>>,
        identity_bundle: IdentityBundle,
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
                        let replay_guard_clone = replay_guard.clone();
                        let neighbor_sets_clone = neighbor_sets.clone();
                        let topology_config_clone = topology_config.clone();
                        let trust_graph_clone = trust_graph.clone();
                        let session_mgr_clone = session_manager.clone();
                        let peer_x25519_keys_clone = peer_x25519_keys.clone();
                        let identity_bundle_clone = identity_bundle.clone();
                        let own_did_clone = own_did.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(
                                connection,
                                handler_clone,
                                rate_limiter_clone,
                                replay_guard_clone,
                                neighbor_sets_clone,
                                topology_config_clone,
                                trust_graph_clone,
                                session_mgr_clone,
                                peer_x25519_keys_clone,
                                identity_bundle_clone,
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
    #[instrument(skip_all, fields(remote_addr = %connection.remote_address()))]
    async fn handle_connection(
        connection: quinn::Connection,
        handler: IncomingMessageHandler,
        rate_limiter: Arc<RateLimiter>,
        replay_guard: Arc<RwLock<ReplayGuard>>,
        neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
        topology_config: Option<TopologyConfig>,
        trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
        _session_manager: Arc<RwLock<SessionManager>>,
        peer_x25519_keys: Arc<RwLock<std::collections::HashMap<Did, [u8; 32]>>>,
        identity_bundle: IdentityBundle,
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

                            info!(
                                peer_did = %message.from,
                                protocol = ?message.payload.variant_name(),
                                "Received network message"
                            );

                            // Track metrics
                            icn_obs::metrics::network::messages_received_inc();

                            // Handle handshake messages internally
                            match &message.payload {
                                MessagePayload::Hello { binding_info: _, version_info, topology_info, x25519_public } => {
                                    // NOTE: DID-TLS binding was already verified during TLS handshake
                                    // by DidCertificateVerifier. No need to re-verify here.
                                    // The TLS layer guarantees that the peer's DID matches their certificate.

                                    // Perform version negotiation
                                    let local_version_info = crate::VersionInfo::new(
                                        format!("icnd-{}", env!("CARGO_PKG_VERSION"))
                                    );

                                    // Handle legacy nodes that don't send version_info (pre-version-negotiation)
                                    let (negotiated_version, common_caps, peer_software) = match version_info {
                                        Some(remote_info) => {
                                            // Modern node with version info
                                            let negotiated = match crate::negotiate_version(&local_version_info, remote_info) {
                                                Ok(v) => v,
                                                Err(e) => {
                                                    warn!(
                                                        peer_did = %message.from,
                                                        local_range = format!("[{}-{}]", local_version_info.min_supported, local_version_info.max_supported),
                                                        peer_range = format!("[{}-{}]", remote_info.min_supported, remote_info.max_supported),
                                                        "Version negotiation failed: {}",
                                                        e
                                                    );
                                                    // Track failure metric
                                                    icn_obs::metrics::network::version_negotiation_failure_inc("incompatible_version");
                                                    // Drop the connection - incompatible versions
                                                    return Err(anyhow::anyhow!("Incompatible protocol version"));
                                                }
                                            };

                                            // Track successful negotiation
                                            icn_obs::metrics::network::version_negotiation_success_inc(negotiated);

                                            // Calculate common capabilities
                                            let caps = crate::common_capabilities(&local_version_info, remote_info);
                                            (negotiated, caps, remote_info.software_version.clone())
                                        }
                                        None => {
                                            // Legacy node without version info - treat as v1 with minimal capabilities
                                            info!(
                                                peer_did = %message.from,
                                                "Received Hello from legacy node (no version_info), treating as protocol v1"
                                            );
                                            // Track legacy connection (use negotiated_version=1 for metrics)
                                            icn_obs::metrics::network::version_negotiation_success_inc(1);

                                            // No capabilities for legacy nodes (empty set)
                                            (1, crate::CapabilityFlags::empty(), "legacy-node".to_string())
                                        }
                                    };

                                    info!(
                                        peer_did = %message.from,
                                        peer_software = %peer_software,
                                        negotiated_version = negotiated_version,
                                        common_capabilities = ?common_caps.describe(),
                                        has_topology = topology_info.is_some(),
                                        message_type = "Hello",
                                        "Received Hello with version negotiation"
                                    );

                                    // Store peer's X25519 public key for end-to-end encryption
                                    {
                                        let mut keys = peer_x25519_keys.write().await;
                                        keys.insert(message.from.clone(), *x25519_public);
                                        info!(
                                            peer_did = %message.from,
                                            key_size = x25519_public.len(),
                                            "Stored X25519 public key"
                                        );
                                    }

                                    // Add peer to neighbor sets if topology is enabled
                                    if let Some(ref sets) = neighbor_sets {
                                        if let Some(peer_topology) = topology_info {
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
                                                peer_topology.clone(),
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
                                    }

                                    // Send Hello response with our X25519 public key directly on this connection
                                    let binding_info = identity_bundle.binding_info();
                                    let x25519_public = *identity_bundle.x25519_public_bytes();
                                    let version_info = crate::VersionInfo::new(
                                        format!("icnd-{}", env!("CARGO_PKG_VERSION"))
                                    );
                                    let topology_info = topology_config.as_ref().map(|topo_cfg| {
                                        TopologyInfo {
                                            region: topo_cfg.region.clone(),
                                            cluster_id: topo_cfg.cluster_id.clone(),
                                            role: topo_cfg.role,
                                        }
                                    });

                                    let hello_response = NetworkMessage::hello(
                                        own_did.clone(),
                                        message.from.clone(),
                                        binding_info,
                                        version_info,
                                        topology_info,
                                        x25519_public,
                                    );

                                    // Send Hello response directly on the same connection
                                    let connection_clone = connection.clone();
                                    tokio::spawn(async move {
                                        match connection_clone.open_bi().await {
                                            Ok((mut send, _recv)) => {
                                                if let Err(e) = crate::protocol::write_message(&mut send, &hello_response).await {
                                                    warn!("Failed to write Hello response: {}", e);
                                                } else {
                                                    info!("Sent Hello response with X25519 public key");
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Failed to open stream for Hello response: {}", e);
                                            }
                                        }
                                    });

                                    info!("Processed Hello from {}", message.from);
                                }
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

                                    // Send our own handshake back (bidirectional exchange)
                                    // Open a new stream since the original stream is finished
                                    let connection_clone = connection.clone();
                                    let from_did = message.from.clone();
                                    let own_did_clone = own_did.clone();
                                    let topo_cfg_clone = topology_config.clone();

                                    tokio::spawn(async move {
                                        match connection_clone.open_bi().await {
                                            Ok((mut new_send, _new_recv)) => {
                                                let response_msg = if let Some(ref topo_cfg) = topo_cfg_clone {
                                                    // Send full handshake if topology is enabled
                                                    NetworkMessage::handshake(
                                                        own_did_clone,
                                                        from_did.clone(),
                                                        topo_cfg.region.clone(),
                                                        topo_cfg.cluster_id.clone(),
                                                        format!("{:?}", topo_cfg.role),
                                                    )
                                                } else {
                                                    // Send ack if topology is disabled
                                                    NetworkMessage::handshake_ack(own_did_clone, from_did.clone())
                                                };

                                                if let Err(e) = write_message(&mut new_send, &response_msg).await {
                                                    warn!("Failed to send handshake response to {}: {}", from_did, e);
                                                } else {
                                                    if let Err(e) = new_send.finish() {
                                                        warn!("Failed to finish handshake response stream to {}: {}", from_did, e);
                                                    } else {
                                                        info!("Sent handshake response to {}", from_did);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Failed to open stream for handshake response to {}: {}", from_did, e);
                                            }
                                        }
                                    });
                                }
                                MessagePayload::HandshakeAck => {
                                    info!("Received handshake ack from {}", message.from);
                                    // Nothing to do, just acknowledgement
                                }
                                MessagePayload::Signed(ref envelope) => {
                                    // Verify SignedEnvelope (signature + replay protection)
                                    match replay_guard.write().await.check(envelope) {
                                        Ok(()) => {
                                            info!("Verified signed message from {} (seq={})", envelope.from, envelope.sequence);
                                            // Forward verified message to handler
                                            handler(message);
                                        }
                                        Err(e) => {
                                            warn!("Rejecting signed message from {}: {}", envelope.from, e);
                                            // Drop message (don't forward to handler)
                                        }
                                    }
                                }
                                _ => {
                                    // Forward other messages to the external handler
                                    handler(message);
                                }
                            }
                        }
                        Err(e) => {
                            let err_msg = e.to_string();

                            // Track protocol version mismatches
                            if err_msg.contains("too old") {
                                warn!("Protocol version too old: {}", err_msg);
                                icn_obs::metrics::network::protocol_version_too_old_inc();
                                icn_obs::metrics::network::protocol_version_mismatch_inc();
                            } else if err_msg.contains("too new") {
                                warn!("Protocol version too new: {}", err_msg);
                                icn_obs::metrics::network::protocol_version_too_new_inc();
                                icn_obs::metrics::network::protocol_version_mismatch_inc();
                            } else {
                                warn!("Failed to read message: {}", e);
                            }
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

    /// Export network actor state for persistence
    ///
    /// This exports:
    /// - Peer X25519 public keys (for end-to-end encryption)
    /// - Known peer addresses (last known SocketAddr for reconnection)
    ///
    /// Note: Active connections are NOT persisted - they will be re-established
    /// via discovery and dialing after restart.
    pub async fn export_state(&self) -> icn_snapshot::NetworkState {
        // Export peer X25519 keys
        let peer_x25519_keys: std::collections::HashMap<String, [u8; 32]> = self
            .peer_x25519_keys
            .read()
            .await
            .iter()
            .map(|(did, key)| (did.to_string(), *key))
            .collect();

        // Peer addresses are not exported - they will be rediscovered via mDNS
        let peer_addresses: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        icn_snapshot::NetworkState {
            peer_x25519_keys,
            peer_addresses,
        }
    }

    /// Restore network actor state from persistence
    ///
    /// This restores:
    /// - Peer X25519 public keys (so encryption works immediately after restart)
    /// - Known peer addresses (as a hint for reconnection)
    ///
    /// Note: Connections are NOT automatically re-established - that happens
    /// through normal discovery and connection management processes.
    pub async fn restore_state(&self, state: icn_snapshot::NetworkState) -> Result<()> {
        info!("Restoring network state: {} peer X25519 keys, {} peer addresses",
              state.peer_x25519_keys.len(), state.peer_addresses.len());

        // Restore peer X25519 keys
        let mut keys = self.peer_x25519_keys.write().await;
        for (did_str, key) in state.peer_x25519_keys {
            let did = Did::from_str(&did_str)
                .context("Failed to parse DID from peer X25519 keys")?;
            keys.insert(did, key);
        }
        drop(keys);

        // Note: We don't restore peer addresses directly because Discovery manages
        // its own peer list. Peer addresses will be rediscovered via mDNS.
        // We could optionally pre-populate the discovery with these addresses,
        // but that adds complexity and they'll be rediscovered quickly anyway.

        info!("✅ Network state restored successfully");
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
        let identity_bundle = IdentityBundle::from_keypair(keypair).unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        let handle = NetworkActor::spawn(identity_bundle, addr, shutdown_tx.clone(), None, None, None, None, None)
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
