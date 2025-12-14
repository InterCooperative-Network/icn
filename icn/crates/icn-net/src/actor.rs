//! Network actor - coordinates discovery and session management
//!
//! The network actor provides a unified interface for:
//! - Peer discovery via mDNS
//! - QUIC session management
//! - Automatic connection establishment to discovered peers
//! - Connection lifecycle management

use anyhow::{Context, Result};
#[cfg(test)]
use icn_identity::KeyPair;
use icn_identity::{Did, IdentityBundle};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, instrument, warn};

use crate::{
    protocol::{read_message, write_message, MessagePayload, NetworkMessage},
    rate_limit::{RateLimitConfig, RateLimiter},
    replay_guard::ReplayGuard,
    topology::{
        NeighborLimitsConfig, NeighborSets, NodeRole, PeerId, TopologyConfig, TopologyInfo,
    },
    CapabilityFlags, Discovery, PeerInfo, SessionManager,
};

/// Per-peer connection metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerConnectionInfo {
    /// Peer's DID
    pub did: Did,

    /// Negotiated protocol version for this connection
    pub negotiated_version: u32,

    /// Peer's announced capabilities
    pub peer_capabilities: CapabilityFlags,

    /// Peer's software version string
    pub peer_software: String,

    /// X25519 public key for end-to-end encryption
    pub x25519_key: [u8; 32],
}

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
    peer_connections: Option<Arc<RwLock<std::collections::HashMap<Did, PeerConnectionInfo>>>>,
    session_manager: Arc<RwLock<SessionManager>>,
    own_did: Did,
    blob_registry: Option<Arc<RwLock<crate::BlobLocationRegistry>>>,
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
    /// 1. Checks if the peer supports E2E encryption
    /// 2. Retrieves the peer's X25519 public key
    /// 3. Encrypts the payload using EncryptedEnvelope
    /// 4. Signs the encrypted envelope
    /// 5. Sends the message
    ///
    /// Returns an error if:
    /// - The peer doesn't support E2E_ENCRYPTION capability
    /// - The peer's X25519 key is not available (no Hello exchange yet)
    pub async fn send_encrypted_message(
        &self,
        recipient: &Did,
        sender_keypair: &icn_identity::KeyPair,
        sender_x25519_secret: &x25519_dalek::StaticSecret,
        sequence: u64,
        payload: &[u8],
    ) -> Result<()> {
        use crate::{EncryptedEnvelope, NetworkMessage, SignedEnvelope};

        // Check if peer supports E2E encryption
        if !self
            .peer_has_capability(recipient, CapabilityFlags::E2E_ENCRYPTION)
            .await
        {
            anyhow::bail!(
                "Peer {recipient} does not support E2E_ENCRYPTION capability (or handshake not complete)"
            );
        }

        // Get recipient's X25519 public key
        let recipient_public_key_bytes = self
            .get_peer_x25519_key(recipient)
            .await
            .context("Recipient X25519 public key not available - no Hello exchange yet")?;

        let recipient_public_key = x25519_dalek::PublicKey::from(recipient_public_key_bytes);

        // Create encrypted envelope
        let encrypted = EncryptedEnvelope::encrypt(
            sender_keypair.did(),
            recipient,
            sequence,
            sender_x25519_secret,
            &recipient_public_key,
            payload,
        )?;

        // Sign the encrypted envelope
        let signed = SignedEnvelope::new(
            sender_keypair.did(),
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
            sets_read
                .sample(scope, count)
                .into_iter()
                .map(|peer_id| peer_id.0)
                .collect()
        } else {
            // No topology support - return empty list (fall back to broadcast)
            Vec::new()
        }
    }

    /// Get a peer's X25519 public key for end-to-end encryption
    ///
    /// Returns None if the peer's key hasn't been received yet (no Hello message exchange)
    pub async fn get_peer_x25519_key(&self, did: &Did) -> Option<[u8; 32]> {
        let connections = self.peer_connections.as_ref()?;
        connections
            .read()
            .await
            .get(did)
            .map(|info| info.x25519_key)
    }

    /// Get this node's connection candidate for NAT traversal
    ///
    /// Returns a ConnectionCandidate that can be published to the
    /// `network:candidates` gossip topic to advertise connection information.
    ///
    /// Returns an error if the session manager hasn't been started yet.
    pub async fn connection_candidate(&self) -> Result<crate::ConnectionCandidate> {
        let session_mgr = self.session_manager.read().await;
        session_mgr.connection_candidate(self.own_did.clone()).await
    }

    /// Get a peer's connection info (version, capabilities, X25519 key)
    ///
    /// Returns None if no Hello exchange has occurred yet
    pub async fn get_peer_connection_info(&self, did: &Did) -> Option<PeerConnectionInfo> {
        let connections = self.peer_connections.as_ref()?;
        connections.read().await.get(did).cloned()
    }

    /// Check if a peer supports a specific capability
    ///
    /// Returns false if the peer hasn't completed Hello handshake yet
    ///
    /// # Example
    /// ```rust,ignore
    /// if network_handle.peer_has_capability(&peer_did, CapabilityFlags::E2E_ENCRYPTION).await {
    ///     // Use encrypted communication
    ///     network_handle.send_encrypted_message(/* ... */).await?;
    /// } else {
    ///     // Fall back to signed-only communication
    ///     network_handle.send_message(peer_did, signed_msg).await?;
    /// }
    /// ```
    pub async fn peer_has_capability(&self, did: &Did, capability: CapabilityFlags) -> bool {
        if let Some(ref connections) = self.peer_connections {
            if let Some(info) = connections.read().await.get(did) {
                return info.peer_capabilities.contains(capability);
            }
        }
        false
    }

    /// Get the negotiated protocol version for a peer
    ///
    /// Returns None if the peer hasn't completed Hello handshake yet
    pub async fn get_peer_protocol_version(&self, did: &Did) -> Option<u32> {
        if let Some(ref connections) = self.peer_connections {
            connections
                .read()
                .await
                .get(did)
                .map(|info| info.negotiated_version)
        } else {
            None
        }
    }

    /// Get all peers that support a specific capability
    ///
    /// Useful for broadcasting feature-specific messages only to capable peers
    ///
    /// # Example
    /// ```rust,ignore
    /// let encrypted_peers = network_handle.get_peers_with_capability(
    ///     CapabilityFlags::E2E_ENCRYPTION
    /// ).await;
    /// for peer in encrypted_peers {
    ///     network_handle.send_encrypted_message(&peer, /* ... */).await?;
    /// }
    /// ```
    pub async fn get_peers_with_capability(&self, capability: CapabilityFlags) -> Vec<Did> {
        if let Some(ref connections) = self.peer_connections {
            connections
                .read()
                .await
                .iter()
                .filter(|(_, info)| info.peer_capabilities.contains(capability))
                .map(|(did, _)| did.clone())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get reference to neighbor sets (for testing and inspection)
    pub fn neighbor_sets(&self) -> &Option<Arc<RwLock<NeighborSets>>> {
        &self.neighbor_sets
    }

    /// Get reference to blob location registry (for testing and inspection)
    pub fn blob_registry(&self) -> &Option<Arc<RwLock<crate::BlobLocationRegistry>>> {
        &self.blob_registry
    }

    /// Query peers that have a specific blob
    ///
    /// Returns list of peers with blob locations (DID, size_bytes), sorted by freshness.
    /// Only returns non-expired locations (24-hour TTL).
    pub async fn get_peers_with_blob(
        &self,
        blob_hash: &icn_gossip::types::ContentHash,
    ) -> Vec<crate::BlobLocation> {
        if let Some(ref registry) = self.blob_registry {
            registry.read().await.get_peers_with_blob(blob_hash)
        } else {
            Vec::new()
        }
    }

    /// Find peers that have all requested blobs (data locality optimization)
    ///
    /// Returns peers sorted by number of matching blobs (descending).
    /// Useful for placing tasks near their input data.
    pub async fn find_peers_with_all(
        &self,
        blob_hashes: &[icn_gossip::types::ContentHash],
    ) -> Vec<(icn_identity::Did, usize)> {
        if let Some(ref registry) = self.blob_registry {
            registry.read().await.find_peers_with_all(blob_hashes)
        } else {
            Vec::new()
        }
    }

    /// Announce blob availability to the network
    ///
    /// Broadcasts a BlobAnnounce message via gossip protocol to notify other peers
    /// that this node has a specific blob. The announcement is automatically
    /// recorded in the local BlobLocationRegistry.
    ///
    /// # Arguments
    /// * `blob_hash` - Content-addressed hash of the blob (32 bytes)
    /// * `peer_did` - DID of the peer announcing the blob (typically own_did)
    /// * `size_bytes` - Size of the blob in bytes
    ///
    /// # Usage
    ///
    /// Call this method when storing a blob locally to announce its availability:
    ///
    /// ```no_run
    /// # use icn_net::NetworkHandle;
    /// # use icn_identity::Did;
    /// # async fn example(network: &NetworkHandle, own_did: &Did) {
    /// let blob_hash = [0u8; 32]; // Content hash of stored blob
    /// let size = 1024; // Blob size in bytes
    ///
    /// network.announce_blob_availability(blob_hash, own_did.clone(), size).await;
    /// # }
    /// ```
    ///
    /// # Integration Pattern
    ///
    /// Future blob storage components should call this method after successfully
    /// storing a blob:
    ///
    /// ```no_run
    /// # use icn_net::NetworkHandle;
    /// # use icn_identity::Did;
    /// # async fn example(network: &NetworkHandle, own_did: &Did) {
    /// // 1. Store blob to local storage
    /// // store.put(blob_hash, blob_data)?;
    ///
    /// // 2. Announce availability to network
    /// let blob_hash = [0u8; 32]; // compute_hash(blob_data)
    /// let size = 1024; // blob_data.len()
    /// network.announce_blob_availability(blob_hash, own_did.clone(), size).await;
    /// # }
    /// ```
    pub async fn announce_blob_availability(
        &self,
        blob_hash: icn_gossip::types::ContentHash,
        peer_did: icn_identity::Did,
        size_bytes: u64,
    ) {
        // Update local registry
        if let Some(ref registry) = self.blob_registry {
            registry
                .write()
                .await
                .announce_blob(blob_hash, peer_did.clone(), size_bytes);
        }

        // Broadcast BlobAnnounce message via gossip
        let gossip_msg = icn_gossip::types::GossipMessage::BlobAnnounce {
            blob_hash,
            peer_did,
            size_bytes,
        };

        let network_msg = crate::protocol::NetworkMessage::new(
            self.own_did.clone(),
            None, // Broadcast to all peers
            crate::protocol::MessagePayload::Gossip(gossip_msg),
        );

        // Broadcast via network (best-effort, ignore errors)
        let _ = self.broadcast(network_msg).await;
    }

    /// Export network state for persistence
    ///
    /// This exports peer connection info (version, capabilities, X25519 keys)
    /// so encrypted communication can resume immediately after restart without
    /// new key exchange and version negotiation.
    ///
    /// Peer addresses are not exported (rediscovered via mDNS).
    pub async fn export_state(&self) -> icn_snapshot::NetworkState {
        let peer_connections = if let Some(ref connections) = self.peer_connections {
            connections
                .read()
                .await
                .iter()
                .map(|(did, info)| {
                    let snapshot_info = icn_snapshot::PeerConnectionInfo {
                        did: did.to_string(),
                        negotiated_version: info.negotiated_version,
                        peer_capabilities: info.peer_capabilities.bits(),
                        peer_software: info.peer_software.clone(),
                        x25519_key: info.x25519_key,
                    };
                    (did.to_string(), snapshot_info)
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        };

        // Legacy format (empty for new deployments)
        let peer_x25519_keys = std::collections::HashMap::new();
        let peer_addresses = std::collections::HashMap::new();

        icn_snapshot::NetworkState {
            peer_connections,
            peer_x25519_keys,
            peer_addresses,
        }
    }

    /// Restore network state from persistence
    ///
    /// This restores peer connection info (version, capabilities, X25519 keys)
    /// so encrypted communication works immediately after restart without new
    /// key exchange and version negotiation.
    ///
    /// Supports both modern (peer_connections) and legacy (peer_x25519_keys) formats.
    /// Peer addresses are not restored (rediscovered via mDNS).
    pub async fn restore_state(&self, state: icn_snapshot::NetworkState) -> Result<()> {
        if let Some(ref connections) = self.peer_connections {
            let mut connections_write = connections.write().await;

            // Restore modern format (peer_connections)
            for (did_str, snapshot_info) in state.peer_connections {
                let did =
                    Did::from_str(&did_str).context("Failed to parse DID from network state")?;

                let connection_info = PeerConnectionInfo {
                    did: did.clone(),
                    negotiated_version: snapshot_info.negotiated_version,
                    peer_capabilities: crate::CapabilityFlags::from_bits_truncate(
                        snapshot_info.peer_capabilities,
                    ),
                    peer_software: snapshot_info.peer_software,
                    x25519_key: snapshot_info.x25519_key,
                };

                connections_write.insert(did, connection_info);
            }

            // Legacy migration: restore old peer_x25519_keys format
            // (for backward compatibility with old snapshots)
            for (did_str, key) in state.peer_x25519_keys {
                let did = Did::from_str(&did_str)
                    .context("Failed to parse DID from legacy network state")?;

                // Only restore if not already present from modern format
                connections_write
                    .entry(did.clone())
                    .or_insert_with(|| PeerConnectionInfo {
                        did,
                        negotiated_version: 1, // Assume v1 for legacy
                        peer_capabilities: crate::CapabilityFlags::empty(),
                        peer_software: "legacy-unknown".to_string(),
                        x25519_key: key,
                    });
            }

            tracing::info!(
                "✅ Restored {} peer connections from snapshot",
                connections_write.len()
            );
        }
        Ok(())
    }

    /// Request peer exchange from a connected peer
    ///
    /// Sends a PeerExchangeMessage::Request to the specified peer, asking for
    /// a list of known peers. This is used for cross-network discovery in
    /// federation scenarios where mDNS doesn't work (WAN connectivity).
    ///
    /// # Arguments
    /// * `peer_did` - DID of the peer to request peers from
    /// * `max_peers` - Maximum number of peers to request (default: 50)
    /// * `network_filter` - Optional network name filter
    ///
    /// # Example
    /// ```rust,ignore
    /// // Request up to 20 peers from a bootstrap node
    /// network_handle.request_peer_exchange(&bootstrap_did, Some(20), None).await?;
    ///
    /// // Request peers from a specific federation network
    /// network_handle.request_peer_exchange(&peer_did, Some(50), Some("my-coop-network")).await?;
    /// ```
    pub async fn request_peer_exchange(
        &self,
        peer_did: &Did,
        max_peers: Option<usize>,
        network_filter: Option<String>,
    ) -> Result<()> {
        let request_msg = NetworkMessage::peer_exchange_request(
            self.own_did.clone(),
            peer_did.clone(),
            max_peers.unwrap_or(50),
            network_filter,
        );

        let result = self.send_message(peer_did.clone(), request_msg).await;
        if result.is_ok() {
            icn_obs::metrics::peer_exchange::requests_sent_inc();
        }
        result
    }

    /// Announce a new peer to connected peers
    ///
    /// Broadcasts a PeerExchangeMessage::Announce to inform connected peers
    /// about a new peer. This enables peer introductions in federation.
    ///
    /// # Arguments
    /// * `peer` - KnownPeer information to announce
    pub async fn announce_peer(&self, peer: crate::KnownPeer) -> Result<()> {
        let announce_msg = NetworkMessage::peer_announce(self.own_did.clone(), peer);

        // Broadcast to all connected peers
        let result = self.broadcast(announce_msg).await;
        if result.is_ok() {
            icn_obs::metrics::peer_exchange::announces_sent_inc();
        }
        result
    }

    /// Unannounce a peer (notify when peer disconnects)
    ///
    /// Broadcasts a PeerExchangeMessage::Unannounce to inform connected peers
    /// that a peer is no longer available.
    ///
    /// # Arguments
    /// * `did` - DID of the peer that disconnected
    pub async fn unannounce_peer(&self, did: &str) -> Result<()> {
        let unannounce_msg = NetworkMessage::peer_unannounce(self.own_did.clone(), did.to_string());

        // Broadcast to all connected peers
        let result = self.broadcast(unannounce_msg).await;
        if result.is_ok() {
            icn_obs::metrics::peer_exchange::unannounces_sent_inc();
        }
        result
    }

    /// Send a message via onion routing for privacy-preserving communication
    ///
    /// Wraps the message in multiple layers of encryption and routes it through
    /// relay nodes to hide the sender/receiver relationship.
    ///
    /// # Arguments
    /// * `x25519_secret` - This node's X25519 secret key for creating the onion
    /// * `relay_dids` - DIDs of relay nodes (in order)
    /// * `recipient` - Final recipient DID
    /// * `payload` - Message payload to send (will be serialized)
    ///
    /// # Example
    /// ```rust,ignore
    /// // Send a message through 2 relays for privacy
    /// let relays = vec![relay1_did.clone(), relay2_did.clone()];
    /// network_handle.send_onion_message(
    ///     &x25519_secret,
    ///     relays,
    ///     recipient_did.clone(),
    ///     &my_message,
    /// ).await?;
    /// ```
    ///
    /// # Security Properties
    /// - Sender anonymity: Recipient doesn't know original sender
    /// - Receiver anonymity: Relays don't know final destination
    /// - Unlinkability: Can't correlate sender → recipient
    /// - Traffic analysis resistance: Multiple hops hide patterns
    pub async fn send_onion_message(
        &self,
        x25519_secret: x25519_dalek::StaticSecret,
        relay_dids: Vec<Did>,
        recipient: Did,
        payload: &NetworkMessage,
    ) -> Result<()> {
        use icn_privacy::OnionRouter;

        // Create OnionRouter with our identity
        let mut router = OnionRouter::new(self.own_did.clone(), x25519_secret);

        // Add peer public keys from our connection info
        if let Some(ref connections) = self.peer_connections {
            let conns = connections.read().await;

            // Add relay keys
            for relay_did in &relay_dids {
                if let Some(info) = conns.get(relay_did) {
                    router.add_peer_key(
                        relay_did.clone(),
                        x25519_dalek::PublicKey::from(info.x25519_key),
                    );
                } else {
                    anyhow::bail!("Missing X25519 key for relay {relay_did}");
                }
            }

            // Add recipient key
            if let Some(info) = conns.get(&recipient) {
                router.add_peer_key(
                    recipient.clone(),
                    x25519_dalek::PublicKey::from(info.x25519_key),
                );
            } else {
                anyhow::bail!("Missing X25519 key for recipient {recipient}");
            }
        } else {
            anyhow::bail!("No peer connections available for onion routing");
        }

        // Create circuit
        let circuit = router
            .create_circuit(relay_dids.clone(), recipient.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create onion circuit: {e}"))?;

        // Serialize the payload
        let payload_bytes =
            bincode::serialize(payload).context("Failed to serialize onion payload")?;

        // Wrap message in onion layers
        let onion_msg = router
            .wrap_message(&circuit, &payload_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to wrap onion message: {e}"))?;

        // Get first hop to send to
        let first_hop = if !relay_dids.is_empty() {
            relay_dids[0].clone()
        } else {
            recipient.clone() // Direct send if no relays
        };

        // Create network message
        let net_msg = NetworkMessage::onion(first_hop.clone(), onion_msg);

        // Send to first hop
        self.send_message(first_hop, net_msg).await
    }

    /// Select relay nodes for onion routing based on trust scores
    ///
    /// Returns an ordered list of relay DIDs suitable for creating an onion circuit.
    /// Uses the trust graph to select nodes with sufficient trust.
    ///
    /// # Arguments
    /// * `trust_graph` - Trust graph for scoring peers
    /// * `num_hops` - Number of relay hops desired (typically 2-3)
    /// * `min_trust` - Minimum trust score for relay selection (e.g., 0.3)
    ///
    /// # Returns
    /// Empty vec if insufficient trusted relays are available.
    pub async fn select_onion_relays(
        &self,
        trust_graph: &Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>,
        num_hops: usize,
        min_trust: f64,
    ) -> Vec<Did> {
        // Get connected peers as candidates
        let candidates: Vec<Did> = if let Some(ref connections) = self.peer_connections {
            connections.read().await.keys().cloned().collect()
        } else {
            return vec![];
        };

        // Get trust scores
        let graph = trust_graph.read().await;
        let mut trust_scores = std::collections::HashMap::new();

        for did in &candidates {
            if let Ok(score) = graph.compute_trust_score(did) {
                trust_scores.insert(did.clone(), score);
            }
        }

        // Use icn_privacy's relay selection
        icn_privacy::select_relays(&candidates, &trust_scores, num_hops, min_trust)
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
    /// Per-peer connection metadata (version, capabilities, X25519 keys)
    peer_connections: Arc<RwLock<std::collections::HashMap<Did, PeerConnectionInfo>>>,
    /// Blob location registry for data locality (Phase 16C Week 2)
    blob_registry: Option<Arc<RwLock<crate::BlobLocationRegistry>>>,
    /// Byzantine fault detector (Phase 18 Week 1-2)
    misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,
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
        stun_servers: Option<Vec<SocketAddr>>,
        turn_config: Option<crate::TurnConfig>,
        misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,
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
        let tls_trust_threshold = trust_gated_config
            .as_ref()
            .map(|cfg| cfg.min_trust_threshold);
        session_manager
            .start(
                identity_bundle.keypair(),
                listen_addr,
                trust_graph.clone(),
                tls_trust_threshold,
                stun_servers,
                turn_config,
            )
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

        // Create peer connection info store (version, capabilities, X25519 keys)
        let peer_connections = Arc::new(RwLock::new(std::collections::HashMap::new()));

        // Initialize neighbor sets if topology is enabled
        let neighbor_sets = if let Some(ref topo_cfg) = topology_config {
            let own_topology = TopologyInfo {
                region: topo_cfg.region.clone(),
                cluster_id: topo_cfg.cluster_id.clone(),
                role: topo_cfg.role,
            };
            info!(
                "Topology-aware networking enabled: region={}, cluster={}",
                topo_cfg.region, topo_cfg.cluster_id
            );
            Some(Arc::new(RwLock::new(NeighborSets::new(own_topology))))
        } else {
            info!("Topology-aware networking disabled");
            None
        };

        // Initialize blob location registry (Phase 16C Week 2)
        let blob_registry = Some(Arc::new(RwLock::new(crate::BlobLocationRegistry::new())));
        info!("Blob location registry initialized for data locality tracking");

        // Use provided misbehavior detector or create a new one (Phase 18 Week 1-2)
        let misbehavior_detector = misbehavior_detector.or_else(|| {
            info!("Creating local Byzantine fault detector (not shared with other actors)");
            Some(Arc::new(RwLock::new(
                icn_security::MisbehaviorDetector::new(
                    icn_security::MisbehaviorThresholds::default(),
                ),
            )))
        });
        info!("Byzantine fault detection enabled");

        // Spawn incoming connection handler if handler is provided
        if let Some(handler) = incoming_handler.clone() {
            let session_manager_clone = session_manager.clone();
            let rate_limiter_clone = rate_limiter.clone();
            let replay_guard_clone = replay_guard.clone();
            let neighbor_sets_clone = neighbor_sets.clone();
            let topology_config_clone = topology_config.clone();
            let trust_graph_clone = trust_graph.clone();
            let peer_connections_clone = peer_connections.clone();
            let blob_registry_clone = blob_registry.clone();
            let misbehavior_detector_clone = misbehavior_detector.clone();
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
                    peer_connections_clone,
                    blob_registry_clone,
                    misbehavior_detector_clone,
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

        // Spawn RTT refresh task if topology is enabled
        if let Some(ref sets) = neighbor_sets {
            let neighbor_sets_clone = sets.clone();
            let session_manager_clone = session_manager.clone();
            let own_did_clone = did.clone();
            let mut shutdown_rx = shutdown_tx.subscribe();

            tokio::spawn(async move {
                info!("RTT refresh task starting (60s interval)");

                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            // Get peers with stale RTT measurements
                            let peers = {
                                let sets = neighbor_sets_clone.read().await;
                                sets.peers_needing_rtt_refresh()
                            };

                            if !peers.is_empty() {
                                info!("Refreshing RTT for {} peers with stale measurements", peers.len());

                                for peer in peers {
                                    let peer_did = peer.0.clone();

                                    // Send Ping message
                                    let ping_msg = NetworkMessage::ping(own_did_clone.clone(), peer_did.clone());

                                    // Get connection and send
                                    let session_mgr = session_manager_clone.read().await;
                                    let connections = session_mgr.connections().await;
                                    if let Some((_did_str, conn)) = connections.iter().find(|(did, _)| did == peer_did.as_str()) {
                                        match conn.open_bi().await {
                                            Ok((mut send, _recv)) => {
                                                if let Err(e) = crate::protocol::write_message(&mut send, &ping_msg).await {
                                                    warn!("Failed to send Ping to {}: {}", peer_did, e);
                                                } else if let Err(e) = send.finish() {
                                                    warn!("Failed to finish Ping stream to {}: {}", peer_did, e);
                                                } else {
                                                    info!("Sent Ping to {} for RTT refresh", peer_did);
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Failed to open stream for Ping to {}: {}", peer_did, e);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            info!("RTT refresh task received shutdown signal");
                            break;
                        }
                    }
                }

                info!("RTT refresh task stopped");
            });
        }

        // Create actor
        let actor = NetworkActor {
            discovery,
            session_manager: session_manager.clone(),
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
            peer_connections: peer_connections.clone(),
            blob_registry: blob_registry.clone(),
            misbehavior_detector: misbehavior_detector.clone(),
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
            peer_connections: Some(peer_connections),
            session_manager,
            own_did: did,
            blob_registry: blob_registry.clone(),
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
                            self.topology_config.as_ref().map(|topo_cfg| TopologyInfo {
                                region: topo_cfg.region.clone(),
                                cluster_id: topo_cfg.cluster_id.clone(),
                                role: topo_cfg.role,
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
                            if let Err(e) =
                                Self::send_handshake_internal(session_mgr, &did, hello_msg).await
                            {
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
            let (mut send, _recv) =
                tokio::time::timeout(std::time::Duration::from_secs(5), connection.open_bi())
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

    /// Handle incoming QUIC connections
    #[allow(clippy::too_many_arguments)]
    async fn handle_incoming_connections(
        session_manager: Arc<RwLock<SessionManager>>,
        handler: IncomingMessageHandler,
        rate_limiter: Arc<RateLimiter>,
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

        let (mut send, _recv) = connection
            .open_bi()
            .await
            .context("Failed to open stream")?;
        write_message(&mut send, &handshake_msg).await?;
        send.finish().context("Failed to finish stream")?;

        info!("Sent handshake to {}", peer_did);
        Ok(())
    }

    /// Handle a single QUIC connection (process all incoming streams)
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip_all, fields(remote_addr = %connection.remote_address()))]
    async fn handle_connection(
        connection: quinn::Connection,
        handler: IncomingMessageHandler,
        rate_limiter: Arc<RateLimiter>,
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

        loop {
            // Accept incoming bidirectional stream
            match connection.accept_bi().await {
                Ok((mut send, mut recv)) => {
                    // Read network message
                    match read_message(&mut recv).await {
                        Ok((message, bytes_read)) => {
                            // Track bandwidth contribution (Phase 21.1)
                            icn_obs::metrics::contribution::bandwidth_bytes_add(
                                own_did.as_str(),
                                bytes_read as u64,
                            );

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
                                MessagePayload::Hello {
                                    binding_info: _,
                                    version_info,
                                    topology_info,
                                    x25519_public,
                                } => {
                                    // NOTE: DID-TLS binding was already verified during TLS handshake
                                    // by DidCertificateVerifier. No need to re-verify here.
                                    // The TLS layer guarantees that the peer's DID matches their certificate.

                                    // Perform version negotiation
                                    let local_version_info = crate::VersionInfo::new(format!(
                                        "icnd-{}",
                                        env!("CARGO_PKG_VERSION")
                                    ));

                                    // Handle legacy nodes that don't send version_info (pre-version-negotiation)
                                    let (negotiated_version, common_caps, peer_software) =
                                        match version_info {
                                            Some(remote_info) => {
                                                // Modern node with version info
                                                let negotiated = match crate::negotiate_version(
                                                    &local_version_info,
                                                    remote_info,
                                                ) {
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
                                                        return Err(anyhow::anyhow!(
                                                            "Incompatible protocol version"
                                                        ));
                                                    }
                                                };

                                                // Track successful negotiation
                                                icn_obs::metrics::network::version_negotiation_success_inc(negotiated);

                                                // Calculate common capabilities
                                                let caps = crate::common_capabilities(
                                                    &local_version_info,
                                                    remote_info,
                                                );
                                                (
                                                    negotiated,
                                                    caps,
                                                    remote_info.software_version.clone(),
                                                )
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
                                                (
                                                    1,
                                                    crate::CapabilityFlags::empty(),
                                                    "legacy-node".to_string(),
                                                )
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

                                    // Store peer connection info (version, capabilities, X25519 key)
                                    {
                                        let connection_info = PeerConnectionInfo {
                                            did: message.from.clone(),
                                            negotiated_version,
                                            peer_capabilities: common_caps,
                                            peer_software: peer_software.clone(),
                                            x25519_key: *x25519_public,
                                        };

                                        let mut connections = peer_connections.write().await;
                                        connections.insert(message.from.clone(), connection_info);
                                        info!(
                                            peer_did = %message.from,
                                            negotiated_version = negotiated_version,
                                            peer_software = %peer_software,
                                            capabilities = ?common_caps.describe(),
                                            "Stored peer connection info"
                                        );
                                    }

                                    // Store the incoming QUIC connection in session_manager
                                    // This enables us to send messages back to the peer on this connection
                                    // Get the connections Arc without holding the outer lock across the await
                                    use std::collections::hash_map::Entry;
                                    let connections_arc =
                                        session_manager.read().await.connections_arc();
                                    let peer_did = message.from.to_string();
                                    let mut connections = connections_arc.write().await;
                                    match connections.entry(peer_did) {
                                        Entry::Occupied(entry) => {
                                            info!("Connection already exists for {}, not overwriting with incoming connection from {}",
                                                  entry.key(), connection.remote_address());
                                        }
                                        Entry::Vacant(entry) => {
                                            info!(
                                                "Storing incoming connection from {} at {}",
                                                entry.key(),
                                                connection.remote_address()
                                            );
                                            entry.insert(connection.clone());
                                        }
                                    }

                                    // Add peer to neighbor sets if topology is enabled
                                    if let Some(ref sets) = neighbor_sets {
                                        if let Some(peer_topology) = topology_info {
                                            // Get trust score if available
                                            let trust_score = if let Some(ref tg) = trust_graph {
                                                tg.read()
                                                    .await
                                                    .compute_trust_score(&message.from)
                                                    .unwrap_or(0.0)
                                                    as f32
                                            } else {
                                                0.5 // Default if no trust graph
                                            };

                                            // Add to neighbor sets
                                            let limits = topology_config
                                                .as_ref()
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
                                    let version_info = crate::VersionInfo::new(format!(
                                        "icnd-{}",
                                        env!("CARGO_PKG_VERSION")
                                    ));
                                    let topology_info =
                                        topology_config.as_ref().map(|topo_cfg| TopologyInfo {
                                            region: topo_cfg.region.clone(),
                                            cluster_id: topo_cfg.cluster_id.clone(),
                                            role: topo_cfg.role,
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
                                                if let Err(e) = crate::protocol::write_message(
                                                    &mut send,
                                                    &hello_response,
                                                )
                                                .await
                                                {
                                                    warn!("Failed to write Hello response: {}", e);
                                                } else {
                                                    info!("Sent Hello response with X25519 public key");
                                                }
                                            }
                                            Err(e) => {
                                                warn!(
                                                    "Failed to open stream for Hello response: {}",
                                                    e
                                                );
                                            }
                                        }
                                    });

                                    info!("Processed Hello from {}", message.from);
                                }
                                MessagePayload::Handshake {
                                    region,
                                    cluster_id,
                                    role,
                                } => {
                                    info!(
                                        "Received handshake from {} (region={}, cluster={})",
                                        message.from, region, cluster_id
                                    );

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
                                            tg.read()
                                                .await
                                                .compute_trust_score(&message.from)
                                                .unwrap_or(0.0)
                                                as f32
                                        } else {
                                            0.5 // Default if no trust graph
                                        };

                                        // Add to neighbor sets
                                        let limits = topology_config
                                            .as_ref()
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
                                                let response_msg =
                                                    if let Some(ref topo_cfg) = topo_cfg_clone {
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
                                                        NetworkMessage::handshake_ack(
                                                            own_did_clone,
                                                            from_did.clone(),
                                                        )
                                                    };

                                                if let Err(e) =
                                                    write_message(&mut new_send, &response_msg)
                                                        .await
                                                {
                                                    warn!("Failed to send handshake response to {}: {}", from_did, e);
                                                } else if let Err(e) = new_send.finish() {
                                                    warn!("Failed to finish handshake response stream to {}: {}", from_did, e);
                                                } else {
                                                    info!(
                                                        "Sent handshake response to {}",
                                                        from_did
                                                    );
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
                                MessagePayload::Ping { sent_at } => {
                                    info!(
                                        "Received Ping from {} (sent_at={}ms)",
                                        message.from, sent_at
                                    );

                                    // Send Pong response with timestamp echo
                                    let pong_msg = NetworkMessage::pong(
                                        own_did.clone(),
                                        message.from.clone(),
                                        *sent_at,
                                    );

                                    let connection_clone = connection.clone();
                                    let from_did = message.from.clone();
                                    tokio::spawn(async move {
                                        match connection_clone.open_bi().await {
                                            Ok((mut new_send, _new_recv)) => {
                                                if let Err(e) =
                                                    write_message(&mut new_send, &pong_msg).await
                                                {
                                                    warn!(
                                                        "Failed to send Pong to {}: {}",
                                                        from_did, e
                                                    );
                                                } else if let Err(e) = new_send.finish() {
                                                    warn!(
                                                        "Failed to finish Pong stream to {}: {}",
                                                        from_did, e
                                                    );
                                                } else {
                                                    info!("Sent Pong to {}", from_did);
                                                }
                                            }
                                            Err(e) => {
                                                warn!(
                                                    "Failed to open stream for Pong to {}: {}",
                                                    from_did, e
                                                );
                                            }
                                        }
                                    });

                                    // Forward Ping to external handler for observability
                                    handler(message);
                                }
                                MessagePayload::Pong {
                                    ping_sent_at,
                                    pong_sent_at,
                                } => {
                                    // Calculate RTT
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis()
                                        as u64;
                                    let rtt_ms = now.saturating_sub(*ping_sent_at);

                                    info!(
                                        peer_did = %message.from,
                                        rtt_ms = rtt_ms,
                                        ping_sent_at = ping_sent_at,
                                        pong_sent_at = pong_sent_at,
                                        "Received Pong - RTT measured"
                                    );

                                    // Record RTT in neighbor sets if available
                                    if let Some(ref sets) = neighbor_sets {
                                        sets.write()
                                            .await
                                            .record_rtt(&PeerId(message.from.clone()), rtt_ms);

                                        // Update metrics
                                        icn_obs::metrics::topology::rtt_observe(rtt_ms as f64);
                                    }
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
                                            registry.write().await.announce_blob(
                                                *blob_hash,
                                                peer_did.clone(),
                                                *size_bytes,
                                            );
                                        }
                                    }

                                    // Forward gossip message to external handler
                                    handler(message);
                                }
                                MessagePayload::Signed(ref envelope) => {
                                    // Verify signature and age first (separate from replay check)
                                    // This allows us to distinguish InvalidSignature from ReplayAttack without string matching
                                    let sig_result = envelope.verify(300);

                                    if let Err(e) = sig_result {
                                        warn!(
                                            "Signature/age verification failed from {}: {}",
                                            envelope.from, e
                                        );

                                        // Record InvalidSignature violation
                                        if let Some(ref detector) = misbehavior_detector {
                                            let message_hash = {
                                                use sha2::{Digest, Sha256};
                                                let mut hasher = Sha256::new();
                                                hasher.update(envelope.sequence.to_be_bytes());
                                                hasher.update(envelope.from.as_str().as_bytes());
                                                hasher.finalize().to_vec()
                                            };

                                            let violation =
                                                icn_security::Violation::InvalidSignature {
                                                    message_hash: message_hash
                                                        .clone()
                                                        .try_into()
                                                        .unwrap_or([0u8; 32]),
                                                };

                                            detector.write().await.record_violation(
                                                &envelope.from,
                                                violation,
                                                message_hash,
                                            );
                                        }
                                        // Drop message (don't forward to handler)
                                    } else {
                                        // Signature valid, now check for replay attack
                                        match replay_guard.write().await.check(envelope) {
                                            Ok(()) => {
                                                info!(
                                                    "Verified signed message from {} (seq={})",
                                                    envelope.from, envelope.sequence
                                                );
                                                // Forward verified message to handler
                                                handler(message);
                                            }
                                            Err(e) => {
                                                warn!(
                                                    "Replay attack detected from {}: {}",
                                                    envelope.from, e
                                                );

                                                // Record ReplayAttack violation
                                                if let Some(ref detector) = misbehavior_detector {
                                                    let message_hash = {
                                                        use sha2::{Digest, Sha256};
                                                        let mut hasher = Sha256::new();
                                                        hasher.update(
                                                            envelope.sequence.to_be_bytes(),
                                                        );
                                                        hasher.update(
                                                            envelope.from.as_str().as_bytes(),
                                                        );
                                                        hasher.finalize().to_vec()
                                                    };

                                                    let violation =
                                                        icn_security::Violation::ReplayAttack {
                                                            message_hash: message_hash
                                                                .clone()
                                                                .try_into()
                                                                .unwrap_or([0u8; 32]),
                                                            sequence: envelope.sequence,
                                                        };

                                                    detector.write().await.record_violation(
                                                        &envelope.from,
                                                        violation,
                                                        message_hash,
                                                    );
                                                }
                                                // Drop message (don't forward to handler)
                                            }
                                        }
                                    }
                                }
                                MessagePayload::PeerExchange(ref peer_msg) => {
                                    // Handle peer exchange messages for cross-network discovery
                                    use crate::protocol::{KnownPeer, PeerExchangeMessage};

                                    match peer_msg {
                                        PeerExchangeMessage::Request {
                                            max_peers,
                                            network_filter,
                                        } => {
                                            info!("Received peer exchange request from {} (max={}, filter={:?})",
                                                  message.from, max_peers, network_filter);
                                            icn_obs::metrics::peer_exchange::requests_received_inc(
                                            );

                                            // Gather known peers from session manager (currently connected peers)
                                            let sm = session_manager.read().await;
                                            let connections = sm.connections().await;
                                            drop(sm); // Release lock after getting connections

                                            let now_secs = std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap_or_default()
                                                .as_secs();

                                            let mut known_peers: Vec<KnownPeer> = Vec::new();

                                            // Add connected peers from session manager
                                            for (did_str, conn) in connections {
                                                // Skip the requesting peer
                                                if did_str == message.from.as_str() {
                                                    continue;
                                                }

                                                known_peers.push(KnownPeer {
                                                    did: did_str.to_string(),
                                                    addresses: vec![conn
                                                        .remote_address()
                                                        .to_string()],
                                                    version: "0.1.0".to_string(),
                                                    network_name: network_filter.clone(),
                                                    observed_trust: None,
                                                    last_seen: now_secs,
                                                    is_local: false,
                                                });

                                                if known_peers.len() >= *max_peers {
                                                    break;
                                                }
                                            }

                                            let total_known = known_peers.len();

                                            // Send response
                                            let response = NetworkMessage::peer_exchange_response(
                                                own_did.clone(),
                                                message.from.clone(),
                                                known_peers,
                                                total_known,
                                            );

                                            // Open a new stream to send the response
                                            let connection_clone = connection.clone();
                                            let from_did = message.from.clone();
                                            tokio::spawn(async move {
                                                match connection_clone.open_bi().await {
                                                    Ok((mut send_stream, _recv)) => {
                                                        if let Err(e) =
                                                            crate::protocol::write_message(
                                                                &mut send_stream,
                                                                &response,
                                                            )
                                                            .await
                                                        {
                                                            warn!("Failed to send peer exchange response: {}", e);
                                                        } else if let Err(e) = send_stream.finish()
                                                        {
                                                            warn!("Failed to finish peer exchange response stream: {}", e);
                                                        } else {
                                                            info!(
                                                                "Sent {} peers to {}",
                                                                total_known, from_did
                                                            );
                                                            icn_obs::metrics::peer_exchange::responses_sent_inc();
                                                        }
                                                    }
                                                    Err(e) => {
                                                        warn!("Failed to open stream for peer exchange response: {}", e);
                                                    }
                                                }
                                            });
                                        }
                                        PeerExchangeMessage::Response { peers, total_known } => {
                                            info!(
                                                "Received {} peers from {} (total known: {})",
                                                peers.len(),
                                                message.from,
                                                total_known
                                            );
                                            icn_obs::metrics::peer_exchange::responses_received_inc(
                                            );
                                            icn_obs::metrics::peer_exchange::peers_discovered_add(
                                                peers.len() as u64,
                                            );

                                            // Forward to handler for processing
                                            // The supervisor can dial these peers if configured for federation
                                            handler(message);
                                        }
                                        PeerExchangeMessage::Announce { peer } => {
                                            info!(
                                                "Peer announced: {} at {:?}",
                                                peer.did, peer.addresses
                                            );
                                            icn_obs::metrics::peer_exchange::announces_received_inc(
                                            );
                                            // Forward to handler for processing
                                            handler(message);
                                        }
                                        PeerExchangeMessage::Unannounce { did } => {
                                            info!("Peer unannounced: {}", did);
                                            icn_obs::metrics::peer_exchange::unannounces_received_inc();
                                            // Forward to handler for processing
                                            handler(message);
                                        }
                                    }
                                }
                                MessagePayload::Onion(ref onion_msg) => {
                                    // Handle onion-routed messages
                                    use icn_privacy::OnionRouter;

                                    debug!(
                                        "Received onion-routed message (hop_index={})",
                                        onion_msg.hop_index
                                    );
                                    icn_obs::metrics::privacy::onion_messages_received_inc();

                                    // Create temporary OnionRouter for this node
                                    let x25519_secret = identity_bundle.x25519_secret();
                                    let router = OnionRouter::new(own_did.clone(), x25519_secret);

                                    // Peel one layer off the onion
                                    match router.peel_layer(onion_msg.clone()) {
                                        Ok(Some((next_hop, peeled_onion))) => {
                                            // We are a relay - forward to next hop
                                            info!(
                                                "Onion relay: forwarding to {} (hop_index={})",
                                                next_hop, peeled_onion.hop_index
                                            );
                                            icn_obs::metrics::privacy::onion_hops_forwarded_inc();

                                            // Get connection to next hop
                                            let sm = session_manager.read().await;
                                            if let Some(conn) = sm
                                                .connections()
                                                .await
                                                .iter()
                                                .find(|(did, _)| *did == next_hop.as_str())
                                                .map(|(_, conn)| conn.clone())
                                            {
                                                drop(sm); // Release lock

                                                // Forward the peeled onion
                                                let forward_msg = NetworkMessage::onion(
                                                    next_hop.clone(),
                                                    peeled_onion,
                                                );

                                                match conn.open_bi().await {
                                                    Ok((mut send, _recv)) => {
                                                        if let Err(e) =
                                                            crate::protocol::write_message(
                                                                &mut send,
                                                                &forward_msg,
                                                            )
                                                            .await
                                                        {
                                                            warn!(
                                                                "Failed to forward onion to {}: {}",
                                                                next_hop, e
                                                            );
                                                        } else if let Err(e) = send.finish() {
                                                            warn!(
                                                                "Failed to finish onion stream to {}: {}",
                                                                next_hop, e
                                                            );
                                                        } else {
                                                            debug!(
                                                                "Forwarded onion to next hop: {}",
                                                                next_hop
                                                            );
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
                                                drop(sm);
                                                warn!(
                                                    "No connection to next hop {} for onion relay",
                                                    next_hop
                                                );
                                            }
                                        }
                                        Ok(None) => {
                                            // We are the final destination - extract payload
                                            match router.extract_payload(onion_msg) {
                                                Ok(payload) => {
                                                    info!(
                                                        "Onion message arrived at destination ({} bytes)",
                                                        payload.len()
                                                    );
                                                    icn_obs::metrics::privacy::onion_messages_delivered_inc();

                                                    // Attempt to deserialize as NetworkMessage and forward
                                                    match bincode::deserialize::<NetworkMessage>(
                                                        &payload,
                                                    ) {
                                                        Ok(inner_msg) => {
                                                            debug!(
                                                                "Extracted inner message: {:?}",
                                                                inner_msg.payload.variant_name()
                                                            );
                                                            // Forward the inner message to handler
                                                            handler(inner_msg);
                                                        }
                                                        Err(e) => {
                                                            warn!(
                                                                "Failed to deserialize onion payload: {}",
                                                                e
                                                            );
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    warn!("Failed to extract onion payload: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Failed to peel onion layer: {}", e);
                                            icn_obs::metrics::privacy::onion_routing_errors_inc();
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
        // Export peer connection info (version, capabilities, X25519 keys)
        let peer_connections: std::collections::HashMap<String, icn_snapshot::PeerConnectionInfo> =
            self.peer_connections
                .read()
                .await
                .iter()
                .map(|(did, info)| {
                    let snapshot_info = icn_snapshot::PeerConnectionInfo {
                        did: did.to_string(),
                        negotiated_version: info.negotiated_version,
                        peer_capabilities: info.peer_capabilities.bits(),
                        peer_software: info.peer_software.clone(),
                        x25519_key: info.x25519_key,
                    };
                    (did.to_string(), snapshot_info)
                })
                .collect();

        // Legacy formats (empty for new deployments)
        let peer_x25519_keys: std::collections::HashMap<String, [u8; 32]> =
            std::collections::HashMap::new();
        let peer_addresses: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        icn_snapshot::NetworkState {
            peer_connections,
            peer_x25519_keys,
            peer_addresses,
        }
    }

    /// Restore network actor state from persistence
    ///
    /// This restores:
    /// - Peer connection info (version, capabilities, X25519 keys)
    /// - Known peer addresses (as a hint for reconnection)
    ///
    /// Supports both modern (peer_connections) and legacy (peer_x25519_keys) formats.
    /// Note: Connections are NOT automatically re-established - that happens
    /// through normal discovery and connection management processes.
    pub async fn restore_state(&self, state: icn_snapshot::NetworkState) -> Result<()> {
        info!(
            "Restoring network state: {} peer connections, {} legacy keys, {} peer addresses",
            state.peer_connections.len(),
            state.peer_x25519_keys.len(),
            state.peer_addresses.len()
        );

        // Restore peer connections
        let mut connections = self.peer_connections.write().await;

        // Restore modern format (peer_connections)
        for (did_str, snapshot_info) in state.peer_connections {
            let did =
                Did::from_str(&did_str).context("Failed to parse DID from peer connections")?;

            let connection_info = PeerConnectionInfo {
                did: did.clone(),
                negotiated_version: snapshot_info.negotiated_version,
                peer_capabilities: crate::CapabilityFlags::from_bits_truncate(
                    snapshot_info.peer_capabilities,
                ),
                peer_software: snapshot_info.peer_software,
                x25519_key: snapshot_info.x25519_key,
            };

            connections.insert(did, connection_info);
        }

        // Legacy migration: restore old peer_x25519_keys format
        // (for backward compatibility with old snapshots)
        for (did_str, key) in state.peer_x25519_keys {
            let did = Did::from_str(&did_str)
                .context("Failed to parse DID from legacy peer X25519 keys")?;

            // Only restore if not already present from modern format
            connections
                .entry(did.clone())
                .or_insert_with(|| PeerConnectionInfo {
                    did,
                    negotiated_version: 1, // Assume v1 for legacy
                    peer_capabilities: crate::CapabilityFlags::empty(),
                    peer_software: "legacy-unknown".to_string(),
                    x25519_key: key,
                });
        }
        drop(connections);

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
    async fn test_network_actor_start() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let keypair = KeyPair::generate().unwrap();
        let identity_bundle = IdentityBundle::from_keypair(keypair).unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        let handle = NetworkActor::spawn(
            identity_bundle,
            addr,
            shutdown_tx.clone(),
            None, // incoming_handler
            None, // trust_graph
            None, // trust_gated_config
            None, // fallback_config
            None, // topology_config
            None, // stun_servers
            None, // turn_config
            None, // misbehavior_detector
        )
        .await
        .unwrap();

        // Should be able to get stats
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.peers_discovered, 0);
        assert_eq!(stats.connections_active, 0);

        // Shutdown
        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_peer_capability_checking() {
        // Create a mock NetworkHandle with peer_connections
        let peer_connections = Arc::new(RwLock::new(std::collections::HashMap::new()));

        let alice_keypair = KeyPair::generate().unwrap();
        let alice_did = alice_keypair.did();
        let bob_keypair = KeyPair::generate().unwrap();
        let bob_did = bob_keypair.did();

        // Alice has E2E encryption
        peer_connections.write().await.insert(
            alice_did.clone(),
            PeerConnectionInfo {
                did: alice_did.clone(),
                negotiated_version: 1,
                peer_capabilities: CapabilityFlags::E2E_ENCRYPTION
                    | CapabilityFlags::SIGNED_MESSAGES,
                peer_software: "icnd-0.1.0".to_string(),
                x25519_key: [1u8; 32],
            },
        );

        // Bob has no encryption (legacy)
        peer_connections.write().await.insert(
            bob_did.clone(),
            PeerConnectionInfo {
                did: bob_did.clone(),
                negotiated_version: 1,
                peer_capabilities: CapabilityFlags::SIGNED_MESSAGES,
                peer_software: "icnd-0.0.5".to_string(),
                x25519_key: [2u8; 32],
            },
        );

        let (tx, _rx) = mpsc::channel(1);
        let test_session_mgr = Arc::new(RwLock::new(SessionManager::new()));
        let test_did = icn_identity::KeyPair::generate().unwrap().did().clone();
        let handle = NetworkHandle {
            tx,
            neighbor_sets: None,
            peer_connections: Some(peer_connections),
            session_manager: test_session_mgr,
            own_did: test_did,
            blob_registry: None,
        };

        // Test peer_has_capability
        assert!(
            handle
                .peer_has_capability(alice_did, CapabilityFlags::E2E_ENCRYPTION)
                .await
        );
        assert!(
            handle
                .peer_has_capability(alice_did, CapabilityFlags::SIGNED_MESSAGES)
                .await
        );
        assert!(
            !handle
                .peer_has_capability(alice_did, CapabilityFlags::GRACEFUL_RESTART)
                .await
        );

        assert!(
            !handle
                .peer_has_capability(bob_did, CapabilityFlags::E2E_ENCRYPTION)
                .await
        );
        assert!(
            handle
                .peer_has_capability(bob_did, CapabilityFlags::SIGNED_MESSAGES)
                .await
        );

        // Unknown peer
        let charlie_keypair = KeyPair::generate().unwrap();
        let charlie_did = charlie_keypair.did();
        assert!(
            !handle
                .peer_has_capability(charlie_did, CapabilityFlags::E2E_ENCRYPTION)
                .await
        );
    }

    #[tokio::test]
    async fn test_get_peers_with_capability() {
        let peer_connections = Arc::new(RwLock::new(std::collections::HashMap::new()));

        let alice_keypair = KeyPair::generate().unwrap();
        let alice_did = alice_keypair.did();
        let bob_keypair = KeyPair::generate().unwrap();
        let bob_did = bob_keypair.did();
        let charlie_keypair = KeyPair::generate().unwrap();
        let charlie_did = charlie_keypair.did();

        // Alice: E2E + Signed
        peer_connections.write().await.insert(
            alice_did.clone(),
            PeerConnectionInfo {
                did: alice_did.clone(),
                negotiated_version: 1,
                peer_capabilities: CapabilityFlags::E2E_ENCRYPTION
                    | CapabilityFlags::SIGNED_MESSAGES,
                peer_software: "icnd-0.1.0".to_string(),
                x25519_key: [1u8; 32],
            },
        );

        // Bob: Only Signed
        peer_connections.write().await.insert(
            bob_did.clone(),
            PeerConnectionInfo {
                did: bob_did.clone(),
                negotiated_version: 1,
                peer_capabilities: CapabilityFlags::SIGNED_MESSAGES,
                peer_software: "icnd-0.0.5".to_string(),
                x25519_key: [2u8; 32],
            },
        );

        // Charlie: E2E + Signed + Graceful Restart
        peer_connections.write().await.insert(
            charlie_did.clone(),
            PeerConnectionInfo {
                did: charlie_did.clone(),
                negotiated_version: 1,
                peer_capabilities: CapabilityFlags::E2E_ENCRYPTION
                    | CapabilityFlags::SIGNED_MESSAGES
                    | CapabilityFlags::GRACEFUL_RESTART,
                peer_software: "icnd-0.2.0".to_string(),
                x25519_key: [3u8; 32],
            },
        );

        let (tx, _rx) = mpsc::channel(1);
        let test_session_mgr = Arc::new(RwLock::new(SessionManager::new()));
        let test_did = icn_identity::KeyPair::generate().unwrap().did().clone();
        let handle = NetworkHandle {
            tx,
            neighbor_sets: None,
            peer_connections: Some(peer_connections),
            session_manager: test_session_mgr,
            own_did: test_did,
            blob_registry: None,
        };

        // Get peers with E2E encryption (should be Alice and Charlie)
        let encrypted_peers = handle
            .get_peers_with_capability(CapabilityFlags::E2E_ENCRYPTION)
            .await;
        assert_eq!(encrypted_peers.len(), 2);
        assert!(encrypted_peers.contains(alice_did));
        assert!(encrypted_peers.contains(charlie_did));

        // Get peers with Signed Messages (should be all three)
        let signed_peers = handle
            .get_peers_with_capability(CapabilityFlags::SIGNED_MESSAGES)
            .await;
        assert_eq!(signed_peers.len(), 3);

        // Get peers with Graceful Restart (should be only Charlie)
        let restart_peers = handle
            .get_peers_with_capability(CapabilityFlags::GRACEFUL_RESTART)
            .await;
        assert_eq!(restart_peers.len(), 1);
        assert!(restart_peers.contains(charlie_did));

        // Get peers with capability no one has
        let quantum_peers = handle
            .get_peers_with_capability(CapabilityFlags::MULTI_DEVICE)
            .await;
        assert_eq!(quantum_peers.len(), 0);
    }

    #[tokio::test]
    async fn test_get_peer_protocol_version() {
        let peer_connections = Arc::new(RwLock::new(std::collections::HashMap::new()));

        let alice_keypair = KeyPair::generate().unwrap();
        let alice_did = alice_keypair.did();
        let bob_keypair = KeyPair::generate().unwrap();
        let bob_did = bob_keypair.did();

        // Alice on v1
        peer_connections.write().await.insert(
            alice_did.clone(),
            PeerConnectionInfo {
                did: alice_did.clone(),
                negotiated_version: 1,
                peer_capabilities: CapabilityFlags::SIGNED_MESSAGES,
                peer_software: "icnd-0.1.0".to_string(),
                x25519_key: [1u8; 32],
            },
        );

        // Bob on v2
        peer_connections.write().await.insert(
            bob_did.clone(),
            PeerConnectionInfo {
                did: bob_did.clone(),
                negotiated_version: 2,
                peer_capabilities: CapabilityFlags::E2E_ENCRYPTION,
                peer_software: "icnd-0.2.0".to_string(),
                x25519_key: [2u8; 32],
            },
        );

        let (tx, _rx) = mpsc::channel(1);
        let test_session_mgr = Arc::new(RwLock::new(SessionManager::new()));
        let test_did = icn_identity::KeyPair::generate().unwrap().did().clone();
        let handle = NetworkHandle {
            tx,
            neighbor_sets: None,
            peer_connections: Some(peer_connections),
            session_manager: test_session_mgr,
            own_did: test_did,
            blob_registry: None,
        };

        // Get versions
        assert_eq!(handle.get_peer_protocol_version(alice_did).await, Some(1));
        assert_eq!(handle.get_peer_protocol_version(bob_did).await, Some(2));

        // Unknown peer
        let charlie_keypair = KeyPair::generate().unwrap();
        let charlie_did = charlie_keypair.did();
        assert_eq!(handle.get_peer_protocol_version(charlie_did).await, None);
    }

    #[tokio::test]
    async fn test_get_peer_connection_info() {
        let peer_connections = Arc::new(RwLock::new(std::collections::HashMap::new()));

        let alice_keypair = KeyPair::generate().unwrap();
        let alice_did = alice_keypair.did();

        let alice_info = PeerConnectionInfo {
            did: alice_did.clone(),
            negotiated_version: 2,
            peer_capabilities: CapabilityFlags::E2E_ENCRYPTION | CapabilityFlags::GRACEFUL_RESTART,
            peer_software: "icnd-0.2.5".to_string(),
            x25519_key: [42u8; 32],
        };

        peer_connections
            .write()
            .await
            .insert(alice_did.clone(), alice_info.clone());

        let (tx, _rx) = mpsc::channel(1);
        let test_session_mgr = Arc::new(RwLock::new(SessionManager::new()));
        let test_did = icn_identity::KeyPair::generate().unwrap().did().clone();
        let handle = NetworkHandle {
            tx,
            neighbor_sets: None,
            peer_connections: Some(peer_connections),
            session_manager: test_session_mgr,
            own_did: test_did,
            blob_registry: None,
        };

        // Get full connection info
        let info = handle.get_peer_connection_info(alice_did).await.unwrap();
        assert_eq!(info.did, alice_did.clone());
        assert_eq!(info.negotiated_version, 2);
        assert_eq!(info.peer_software, "icnd-0.2.5");
        assert_eq!(info.x25519_key, [42u8; 32]);
        assert!(info
            .peer_capabilities
            .contains(CapabilityFlags::E2E_ENCRYPTION));
        assert!(info
            .peer_capabilities
            .contains(CapabilityFlags::GRACEFUL_RESTART));

        // Unknown peer
        let bob_keypair = KeyPair::generate().unwrap();
        let bob_did = bob_keypair.did();
        assert!(handle.get_peer_connection_info(bob_did).await.is_none());
    }

    #[tokio::test]
    async fn test_announce_blob_availability() {
        // Setup blob registry
        let blob_registry = Arc::new(RwLock::new(crate::BlobLocationRegistry::new()));

        // Setup NetworkHandle with mock channel
        let (tx, mut rx) = mpsc::channel(10);
        let test_session_mgr = Arc::new(RwLock::new(SessionManager::new()));
        let own_did = KeyPair::generate().unwrap().did().clone();

        let handle = NetworkHandle {
            tx,
            neighbor_sets: None,
            peer_connections: None,
            session_manager: test_session_mgr,
            own_did: own_did.clone(),
            blob_registry: Some(blob_registry.clone()),
        };

        // Spawn task to handle response channel (prevents hanging)
        tokio::spawn(async move {
            if let Some(NetworkMsg::Broadcast { response, .. }) = rx.recv().await {
                let _ = response.send(Ok(()));
            }
        });

        // Announce blob availability
        let blob_hash = [42u8; 32];
        let size_bytes = 1024;
        handle
            .announce_blob_availability(blob_hash, own_did.clone(), size_bytes)
            .await;

        // Verify blob was recorded in local registry
        let peers = blob_registry.read().await.get_peers_with_blob(&blob_hash);
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].peer_did, own_did);
        assert_eq!(peers[0].size_bytes, size_bytes);

        // Test query API
        let query_peers = handle.get_peers_with_blob(&blob_hash).await;
        assert_eq!(query_peers.len(), 1);
        assert_eq!(query_peers[0].peer_did, own_did);

        // Test find_peers_with_all API
        let peer_matches = handle.find_peers_with_all(&[blob_hash]).await;
        assert_eq!(peer_matches.len(), 1);
        assert_eq!(peer_matches[0].0, own_did);
        assert_eq!(peer_matches[0].1, 1); // 1 matching blob
    }
}
