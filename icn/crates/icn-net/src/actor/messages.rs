//! Message handling for the network actor
//!
//! This module implements the message dispatch logic for [`NetworkActor`].
//! It handles incoming [`NetworkMsg`] commands from the actor's mailbox,
//! including:
//!
//! - Peer discovery queries (`GetPeers`, `GetConnectedPeers`)
//! - Connection management (`Dial`, `Disconnect`)
//! - Message sending (`Send`, `Broadcast`)
//! - State queries (`GetBlobRegistry`, `GetPeerConnectionInfo`)
//!
//! Messages are processed sequentially in the actor's event loop.
//! Timeouts are applied to prevent resource exhaustion:
//! - Dial timeout: 30 seconds default (configurable via `NetworkHandle::set_dial_timeout`)
//! - `RELAY_TIMEOUT`: 15 seconds for relay connection attempts
//! - `SEND_TIMEOUT`: 10 seconds for sending messages to peers
//! - `PEER_TIMEOUT`: 5 seconds for peer lookup operations

use anyhow::{Context, Result};
use icn_identity::Did;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, instrument, warn};

use crate::{
    protocol::{write_message, write_message_negotiated, NetworkMessage},
    NetworkMsg, NetworkStats, SessionManager, TraversalMode,
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
                peer_relay_addr,
                response,
            } => {
                let result = self.handle_dial(addr, did, peer_relay_addr).await;
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

            NetworkMsg::GetNatStatus(tx) => {
                let mut status = self.nat_status.read().await.clone();
                // Populate live fields from session manager
                let sm = self.session_manager.read().await;
                status.public_endpoint = sm.public_endpoint().await;
                status.relay_addr = sm.relay_addr().await;
                drop(sm);
                status.active_relay_count = self.relay_proxies.read().await.len();
                let _ = tx.send(status);
            }
        }
    }

    /// Handle a dial request with direct-then-relay fallback.
    ///
    /// 1. Try direct connection with timeout
    /// 2. On failure, if peer_relay_addr is provided and we have TURN configured,
    ///    create a relay proxy and connect through it
    async fn handle_dial(
        &mut self,
        addr: std::net::SocketAddr,
        did: Did,
        peer_relay_addr: Option<std::net::SocketAddr>,
    ) -> Result<()> {
        // Read configurable dial timeout (default 30s, overridable via NetworkHandle::set_dial_timeout)
        let dial_timeout_ms = self
            .dial_timeout_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        let dial_timeout = std::time::Duration::from_millis(dial_timeout_ms);
        /// Timeout for relay dial (15 seconds)
        const RELAY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

        // ── Step 1: Try direct connection ────────────────────────────
        let direct_result = tokio::time::timeout(dial_timeout, {
            let sm = self.session_manager.clone();
            let did_str = did.as_str().to_string();
            async move { sm.read().await.dial(addr, did_str).await }
        })
        .await
        .context("Timeout dialing peer (direct)")
        .and_then(|r| r);

        match direct_result {
            Ok(connection) => {
                // Direct connection succeeded
                self.wire_new_connection(connection, &did);

                let mut ns = self.nat_status.write().await;
                ns.last_traversal_mode = TraversalMode::Direct;
                ns.last_direct_error = None;
                drop(ns);

                Ok(())
            }
            Err(direct_err) => {
                // Record the error
                let direct_err_msg = format!("{direct_err:#}");
                {
                    let mut ns = self.nat_status.write().await;
                    ns.last_direct_error = Some(direct_err_msg.clone());
                }
                info!(
                    peer = %did,
                    error = %direct_err_msg,
                    "Direct dial failed, checking relay fallback"
                );

                // ── Step 2: Check if relay fallback is viable ────────
                let peer_relay = match peer_relay_addr {
                    Some(pr) => pr,
                    None => {
                        return Err(direct_err.context(
                            "direct dial failed and no peer relay candidate provided; cannot TURN-relay",
                        ));
                    }
                };

                let our_relay = self.session_manager.read().await.relay_addr().await;

                if our_relay.is_none() {
                    return Err(direct_err.context(
                        "direct dial failed and this node has no TURN allocation; cannot relay",
                    ));
                }

                let turn_config = match self.session_manager.read().await.turn_config().await {
                    Some(tc) => tc,
                    None => {
                        return Err(direct_err.context(
                            "direct dial failed and no TURN server configured; cannot relay",
                        ));
                    }
                };

                let turn_server = turn_config.server;

                // ── Step 3: Start relay proxy ────────────────────────
                info!(
                    peer = %did,
                    turn_server = %turn_server,
                    peer_relay = %peer_relay,
                    "Attempting TURN relay fallback"
                );

                let turn_client = Arc::new(crate::TurnClient::new(turn_config));

                let proxy =
                    crate::relay_proxy::TurnRelayProxy::start(turn_server, peer_relay, turn_client)
                        .await
                        .context("failed to start TURN relay proxy")?;

                let proxy_local_addr = proxy.local_addr();

                // ── Step 4: Create a NEW Quinn endpoint through proxy ─
                let relay_endpoint = self
                    .create_relay_endpoint()
                    .context("failed to create relay Quinn endpoint")?;

                let relay_result = tokio::time::timeout(RELAY_TIMEOUT, async {
                    relay_endpoint
                        .connect(proxy_local_addr, "localhost")
                        .context("failed to initiate relay QUIC connection")?
                        .await
                        .context("relay QUIC handshake failed")
                })
                .await
                .context("Timeout dialing peer (relay)")
                .and_then(|r| r);

                match relay_result {
                    Ok(connection) => {
                        // Store the connection in session manager so send_message works
                        self.session_manager
                            .read()
                            .await
                            .store_incoming_connection(did.as_str().to_string(), connection.clone())
                            .await;

                        // Store proxy handle so it stays alive
                        self.relay_proxies.write().await.insert(did.clone(), proxy);

                        // Wire up connection handler + Hello
                        self.wire_new_connection(connection, &did);

                        let mut ns = self.nat_status.write().await;
                        ns.last_traversal_mode = TraversalMode::Relayed;
                        ns.last_relay_error = None;
                        drop(ns);

                        info!(peer = %did, "TURN relay connection established");
                        Ok(())
                    }
                    Err(relay_err) => {
                        let relay_err_msg = format!("{relay_err:#}");

                        // Clean up proxy
                        if let Err(e) = proxy.shutdown().await {
                            warn!(error = %e, "failed to shutdown relay proxy after failure");
                        }

                        // Close the temporary endpoint
                        relay_endpoint.close(0u32.into(), b"relay-failed");

                        let mut ns = self.nat_status.write().await;
                        ns.last_relay_error = Some(relay_err_msg.clone());
                        ns.last_traversal_mode = TraversalMode::Unknown;
                        drop(ns);

                        Err(anyhow::anyhow!(
                            "Direct: {direct_err_msg}; Relay: {relay_err_msg}"
                        ))
                    }
                }
            }
        }
    }

    /// Wire up a newly-established connection: stats, connection handler, Hello message.
    ///
    /// This is called for both direct and relayed connections.
    fn wire_new_connection(&self, connection: quinn::Connection, did: &Did) {
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
        let version_info = crate::VersionInfo::new(format!("icnd-{}", env!("CARGO_PKG_VERSION")));
        let topology_info = self
            .topology_config
            .as_ref()
            .map(|topo_cfg| crate::TopologyInfo {
                region: topo_cfg.region.clone(),
                cluster_id: topo_cfg.cluster_id.clone(),
                role: topo_cfg.role,
            });

        // Build Hello message with PQ binding proof if available
        #[cfg(feature = "post-quantum")]
        let hello_msg_result: Result<NetworkMessage> = {
            let keypair = self
                .identity_bundle
                .keypair()
                .context("Failed to load keypair for PQ binding");
            match keypair {
                Ok(keypair) => {
                    let ml_dsa = keypair.pq_public_key().map(|pk| pk.as_bytes().to_vec());
                    let ml_kem = self
                        .identity_bundle
                        .kem_pq_public_bytes()
                        .map(|b| b.to_vec());
                    Ok(NetworkMessage::hello_with_binding(
                        self.own_did.clone(),
                        did.clone(),
                        binding_info,
                        version_info,
                        topology_info,
                        x25519_public,
                        ml_dsa,
                        ml_kem,
                        &keypair,
                    ))
                }
                Err(e) => Err(e),
            }
        };

        #[cfg(not(feature = "post-quantum"))]
        let hello_msg_result: Result<NetworkMessage> = Ok(NetworkMessage::hello(
            self.own_did.clone(),
            did.clone(),
            binding_info,
            version_info,
            topology_info,
            x25519_public,
            None,
            None,
        ));

        match hello_msg_result {
            Ok(hello_msg) => {
                let session_mgr = self.session_manager.clone();
                let did_clone = did.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        send_handshake_internal(session_mgr, &did_clone, hello_msg).await
                    {
                        warn!("Failed to send Hello to {}: {}", did_clone, e);
                    }
                });
            }
            Err(e) => {
                warn!("Failed to build Hello message: {}", e);
            }
        }
    }

    /// Create a new Quinn endpoint for relay connections.
    ///
    /// This endpoint is separate from the main session_manager endpoint.
    /// It binds to a loopback address and connects through the relay proxy's
    /// local_addr, which transparently wraps/unwraps TURN framing.
    fn create_relay_endpoint(&self) -> Result<quinn::Endpoint> {
        let std_socket = std::net::UdpSocket::bind("127.0.0.1:0")
            .context("failed to bind relay endpoint socket")?;
        std_socket
            .set_nonblocking(true)
            .context("failed to set nonblocking")?;

        let runtime = quinn::default_runtime()
            .ok_or_else(|| anyhow::anyhow!("no async runtime for relay endpoint"))?;

        let mut endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            None, // client-only
            std_socket,
            runtime,
        )
        .context("failed to create relay Quinn endpoint")?;

        let certs = vec![self.identity_bundle.tls_cert().clone()];
        let key = self.identity_bundle.tls_key();
        let tls_config = crate::tls::create_tofu_client_config(certs, key)
            .context("failed to create TOFU client config for relay endpoint")?;
        let mut client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)
                .map_err(|e| anyhow::anyhow!("failed to create QUIC client config: {e}"))?,
        ));
        client_config.transport_config(Arc::new(crate::session::create_transport_config()));
        endpoint.set_default_client_config(client_config);

        Ok(endpoint)
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
