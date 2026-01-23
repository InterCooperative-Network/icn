//! Connection handling for the network actor

use anyhow::Result;
use icn_identity::{Did, IdentityBundle};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

use crate::{
    handlers::ConnectionContext,
    protocol::{read_message, MessagePayload},
    replay_guard::ReplayGuard,
    topology::{NeighborSets, TopologyConfig},
    IncomingMessageHandler, SessionManager,
};

use super::PeerConnectionInfo;

use super::NetworkActor;

impl NetworkActor {
    /// Handle incoming QUIC connections
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_incoming_connections(
        session_manager: Arc<RwLock<SessionManager>>,
        handler: IncomingMessageHandler,
        rate_limiter: Arc<crate::rate_limit::RateLimiter>,
        replay_guard: Arc<RwLock<ReplayGuard>>,
        neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
        topology_config: Option<TopologyConfig>,
        trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
        peer_connections: Arc<RwLock<std::collections::HashMap<Did, PeerConnectionInfo>>>,
        blob_registry: Option<Arc<RwLock<crate::BlobLocationRegistry>>>,
        misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,
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
                Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    // Continue to accept connections
                }
            }

            // Accept with timeout to periodically check shutdown
            let conn_result = {
                let guard = session_manager.read().await;
                tokio::time::timeout(tokio::time::Duration::from_millis(100), guard.accept())
                    .await
                    .ok() // Convert timeout error to None
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
                        let peer_connections_clone = peer_connections.clone();
                        let blob_registry_clone = blob_registry.clone();
                        let misbehavior_detector_clone = misbehavior_detector.clone();
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
                                peer_connections_clone,
                                blob_registry_clone,
                                misbehavior_detector_clone,
                                identity_bundle_clone,
                                own_did_clone,
                            )
                            .await
                            {
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

    /// Handle a single QUIC connection (process all incoming streams)
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip_all, fields(remote_addr = %connection.remote_address()))]
    pub(super) async fn handle_connection(
        connection: quinn::Connection,
        handler: IncomingMessageHandler,
        rate_limiter: Arc<crate::rate_limit::RateLimiter>,
        replay_guard: Arc<RwLock<ReplayGuard>>,
        neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
        topology_config: Option<TopologyConfig>,
        trust_graph: Option<Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>>,
        session_manager: Arc<RwLock<SessionManager>>,
        peer_connections: Arc<RwLock<std::collections::HashMap<Did, PeerConnectionInfo>>>,
        blob_registry: Option<Arc<RwLock<crate::BlobLocationRegistry>>>,
        misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,
        identity_bundle: IdentityBundle,
        own_did: Did,
    ) -> Result<()> {
        info!("Handling connection from {}", connection.remote_address());

        // Create connection context for handlers (clone shared state)
        let ctx = ConnectionContext::new(
            handler.clone(),
            rate_limiter.clone(),
            replay_guard.clone(),
            neighbor_sets.clone(),
            topology_config.clone(),
            trust_graph.clone(),
            session_manager.clone(),
            peer_connections.clone(),
            blob_registry.clone(),
            misbehavior_detector.clone(),
            identity_bundle.clone(),
            own_did.clone(),
        );

        loop {
            // Accept incoming bidirectional stream
            match connection.accept_bi().await {
                Ok((mut send, mut recv)) => {
                    // Read network message
                    match read_message(&mut recv).await {
                        Ok((message, bytes_read)) => {
                            // Track bandwidth contribution (aggregate, no per-DID tracking)
                            icn_obs::metrics::contribution::total_bandwidth_bytes_add(
                                bytes_read as u64,
                            );

                            // Check rate limit BEFORE processing message
                            // Uses dual-path rate limiting: per-DID and per-anchor (if Sybil resistance enabled)
                            let (did_allowed, anchor_allowed) = rate_limiter
                                .check_rate_limit_with_personhood(&message.from)
                                .await;

                            if !did_allowed {
                                warn!(
                                    "Rate limited message from {} (per-DID limit exceeded)",
                                    message.from
                                );

                                // Track rate limiting metric
                                icn_obs::metrics::network::messages_rate_limited_inc();

                                // Close stream before continuing to avoid resource leak
                                if let Err(e) = send.finish() {
                                    tracing::debug!("Stream finish error during rate limit: {}", e);
                                }

                                // Drop the message (don't call handler)
                                continue;
                            }

                            if !anchor_allowed {
                                warn!(
                                    "Rate limited message from {} (per-person limit exceeded - Sybil mitigation)",
                                    message.from
                                );

                                // Track Sybil-specific rate limiting metric
                                icn_obs::metrics::network::messages_rate_limited_by_anchor_inc();

                                // Close stream before continuing to avoid resource leak
                                if let Err(e) = send.finish() {
                                    tracing::debug!("Stream finish error during rate limit: {}", e);
                                }

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

                            // Dispatch to handlers
                            match &message.payload {
                                MessagePayload::Hello {
                                    binding_info,
                                    version_info,
                                    topology_info,
                                    x25519_public,
                                    ml_dsa_public,
                                    ml_kem_public,
                                    pq_binding_proof,
                                } => {
                                    ctx.handle_hello(
                                        &connection,
                                        &message.from,
                                        binding_info,
                                        version_info,
                                        topology_info,
                                        x25519_public,
                                        ml_dsa_public.clone(),
                                        ml_kem_public.clone(),
                                        pq_binding_proof.clone(),
                                    )
                                    .await?;
                                }
                                MessagePayload::Handshake {
                                    region,
                                    cluster_id,
                                    role,
                                } => {
                                    ctx.handle_handshake(
                                        &connection,
                                        &message.from,
                                        region,
                                        cluster_id,
                                        role,
                                    )
                                    .await;
                                }
                                MessagePayload::HandshakeAck => {
                                    ctx.handle_handshake_ack(&message.from);
                                }
                                MessagePayload::Ping { sent_at } => {
                                    ctx.handle_ping(
                                        &connection,
                                        message.clone(),
                                        &message.from,
                                        *sent_at,
                                    )
                                    .await;
                                }
                                MessagePayload::Pong {
                                    ping_sent_at,
                                    pong_sent_at,
                                } => {
                                    ctx.handle_pong(&message.from, *ping_sent_at, *pong_sent_at)
                                        .await;
                                }
                                MessagePayload::Gossip(ref gossip_msg) => {
                                    // Extract BlobAnnounce from gossip messages for data locality tracking
                                    if let icn_gossip::types::GossipMessage::BlobAnnounce {
                                        blob_hash,
                                        peer_did,
                                        size_bytes,
                                    } = gossip_msg
                                    {
                                        debug!(
                                            peer_did = %peer_did,
                                            blob_hash_len = blob_hash.len(),
                                            size_bytes = size_bytes,
                                            "Received blob announcement via gossip"
                                        );

                                        // Update blob location registry
                                        if let Some(ref registry) = blob_registry {
                                            if let Err(e) = registry.write().await.announce_blob(
                                                *blob_hash,
                                                peer_did.clone(),
                                                *size_bytes,
                                            ) {
                                                debug!(
                                                    error = %e,
                                                    peer_did = %peer_did,
                                                    blob_size = size_bytes,
                                                    "Rejected blob announcement from peer"
                                                );
                                            }
                                        }
                                    }

                                    // Forward gossip message to external handler
                                    handler(message);
                                }
                                MessagePayload::Signed(ref envelope) => {
                                    ctx.handle_signed(message.clone(), envelope).await;
                                }
                                MessagePayload::PeerExchange(ref peer_msg) => {
                                    ctx.handle_peer_exchange(
                                        &connection,
                                        message.clone(),
                                        &message.from,
                                        peer_msg,
                                    )
                                    .await;
                                }
                                MessagePayload::Onion(ref onion_msg) => {
                                    ctx.handle_onion(onion_msg).await;
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
                    if let Err(e) = send.finish() {
                        tracing::debug!("Stream finish error (normal during disconnect): {}", e);
                    }
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
