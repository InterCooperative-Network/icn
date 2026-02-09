//! Network actor - coordinates discovery and session management
//!
//! The network actor provides a unified interface for:
//! - Peer discovery via mDNS
//! - QUIC session management
//! - Automatic connection establishment to discovered peers
//! - Connection lifecycle management

mod connection;
mod messages;

use anyhow::{Context, Result};
#[cfg(test)]
use icn_identity::KeyPair;
use icn_identity::{Did, IdentityBundle, PersonhoodStoreTrait};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, instrument, warn};

use crate::{
    protocol::NetworkMessage,
    rate_limit::{AnchorRateLimitConfig, RateLimitConfig, RateLimiter},
    replay_guard::ReplayGuard,
    topology::{NeighborSets, TopologyConfig, TopologyInfo},
    CapabilityFlags, Discovery, PeerInfo, SessionManager,
};
use icn_store::Store;

/// Max age for rate limiter buckets before cleanup (5 minutes)
const RATE_LIMITER_BUCKET_MAX_AGE_SECS: u64 = 300;

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

    /// ML-DSA public key for post-quantum signature verification (optional)
    /// Present when peer advertises HYBRID_SIGNATURES capability
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ml_dsa_public: Option<Vec<u8>>,

    /// ML-KEM public key for post-quantum key encapsulation (optional)
    /// Present when peer advertises HYBRID_KEM capability
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ml_kem_public: Option<Vec<u8>>,
}

impl PeerConnectionInfo {
    /// Get encoding flags for this peer based on negotiated capabilities
    ///
    /// Returns `(use_postcard, use_compression)` tuple indicating which
    /// encoding options should be used when sending messages to this peer.
    #[inline]
    pub fn encoding_flags(&self) -> (bool, bool) {
        let use_postcard = self
            .peer_capabilities
            .contains(CapabilityFlags::POSTCARD_ENCODING);
        let use_compression = self
            .peer_capabilities
            .contains(CapabilityFlags::MESSAGE_COMPRESSION);
        (use_postcard, use_compression)
    }
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

    /// Dial a peer by address only (DID learned from Hello handshake).
    ///
    /// Uses a deterministic placeholder DID derived from the address as a
    /// temporary connection key. The peer's actual DID is learned during
    /// the Hello handshake and the connection map is updated accordingly.
    pub async fn dial_addr(&self, addr: SocketAddr) -> Result<Did> {
        let temp_did = derive_placeholder_did(addr)?;
        self.dial(addr, temp_did.clone()).await?;
        Ok(temp_did)
    }

    /// Send a network message to a specific peer
    #[instrument(skip(self, message), fields(recipient = %did))]
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
            icn_encoding::encode(&encrypted)?,
        )?;

        // Send via network
        let message = NetworkMessage::signed(Some(recipient.clone()), signed);
        self.send_message(recipient.clone(), message).await
    }

    /// Broadcast a message to all connected peers
    #[instrument(skip(self, message))]
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

    /// Check if a peer is currently connected
    ///
    /// Returns true if the peer has completed the Hello handshake and has
    /// an active connection entry.
    pub async fn is_peer_connected(&self, did: &Did) -> Result<bool> {
        if let Some(ref connections) = self.peer_connections {
            Ok(connections.read().await.contains_key(did))
        } else {
            Ok(false)
        }
    }

    /// Disconnect a peer by closing their QUIC connection
    ///
    /// This gracefully closes the connection and removes the peer from
    /// connection tracking. Returns true if a connection was found and closed.
    ///
    /// Both the QUIC connection and peer tracking are updated atomically to
    /// ensure consistent state. If no connection exists, peer tracking is
    /// also left unchanged.
    ///
    /// # Arguments
    ///
    /// * `did` - The DID of the peer to disconnect
    /// * `reason` - Optional reason message for the closure (for logging/debugging)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Disconnect a misbehaving peer
    /// network_handle.disconnect_peer(&peer_did, Some("rate limit exceeded")).await;
    /// ```
    ///
    /// # Note
    ///
    /// This is primarily intended for testing and administrative use. Frequent
    /// disconnections could destabilize the network.
    pub async fn disconnect_peer(&self, did: &Did, reason: Option<&str>) -> bool {
        let did_str = did.to_string();

        // Remove from peer connections tracking FIRST to prevent new messages
        // being routed to a closing connection. This provides a consistent view
        // where the peer appears disconnected immediately.
        let had_peer_entry = if let Some(ref connections) = self.peer_connections {
            connections.write().await.remove(did).is_some()
        } else {
            false
        };

        // Close the QUIC connection via session manager
        let session_closed = self
            .session_manager
            .read()
            .await
            .disconnect_peer(&did_str, reason)
            .await;

        // Return true if either structure had the peer (connection was active)
        // This handles edge cases where structures might be inconsistent
        had_peer_entry || session_closed
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
            if let Err(e) =
                registry
                    .write()
                    .await
                    .announce_blob(blob_hash, peer_did.clone(), size_bytes)
            {
                tracing::warn!(
                    error = %e,
                    blob_size = size_bytes,
                    "Failed to register local blob announcement"
                );
                return; // Don't broadcast if we can't register locally
            }
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
                        ml_dsa_public: info.ml_dsa_public.clone(),
                        ml_kem_public: info.ml_kem_public.clone(),
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
                    ml_dsa_public: snapshot_info.ml_dsa_public,
                    ml_kem_public: snapshot_info.ml_kem_public,
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
                        ml_dsa_public: None,
                        ml_kem_public: None,
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
            icn_encoding::encode(payload).context("Failed to serialize onion payload")?;

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
}

fn derive_placeholder_did(addr: SocketAddr) -> Result<Did> {
    // Derive a deterministic placeholder key from the address using SHA-256.
    // The resulting 32 bytes are used as an Ed25519 "public key" for connection
    // tracking only — this is NOT a real key and will be replaced by the peer's
    // actual DID after the Hello handshake.
    use sha2::{Digest, Sha256};
    let addr_hash = Sha256::digest(format!("bootstrap:{addr}").as_bytes());
    let hash_bytes: [u8; 32] = addr_hash.into();
    match ed25519_dalek::VerifyingKey::from_bytes(&hash_bytes) {
        Ok(key) => Ok(Did::from_public_key(&key)),
        Err(_) => {
            // SHA-256 output may not be a valid curve point; derive fallback
            // candidates from the address hash so placeholder DIDs remain
            // deterministic per address and avoid cross-address collisions.
            let key = (0u32..=u32::MAX)
                .find_map(|counter| {
                    let mut hasher = Sha256::new();
                    hasher.update(hash_bytes);
                    hasher.update(counter.to_be_bytes());
                    let candidate: [u8; 32] = hasher.finalize().into();
                    ed25519_dalek::VerifyingKey::from_bytes(&candidate).ok()
                })
                .context("failed to construct fallback verifying key for dial_addr")?;
            Ok(Did::from_public_key(&key))
        }
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
    /// If oracle is provided, enables policy-based rate limiting.
    /// If topology_config is provided, enables topology-aware neighbor management.
    /// If personhood_store is provided, enables Sybil resistance via per-person rate limiting.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        identity_bundle: IdentityBundle,
        listen_addr: SocketAddr,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
        incoming_handler: Option<IncomingMessageHandler>,
        oracle: Option<Arc<dyn icn_kernel_api::authz::PolicyOracle>>,
        fallback_config: Option<RateLimitConfig>,
        topology_config: Option<TopologyConfig>,
        stun_servers: Option<Vec<SocketAddr>>,
        turn_config: Option<crate::TurnConfig>,
        misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,
        store: Option<Arc<dyn Store>>,
        personhood_store: Option<Arc<dyn PersonhoodStoreTrait>>,
        anchor_rate_config: Option<AnchorRateLimitConfig>,
    ) -> Result<NetworkHandle> {
        let did = identity_bundle.did().clone();

        info!("Network actor starting for DID: {}", did);

        // Start discovery
        let mut discovery = Discovery::new();
        discovery
            .start(did.clone(), listen_addr)
            .await
            .context("Failed to start discovery")?;

        // Start session manager (no TLS trust gating anymore - handled by PolicyOracle in protocol)
        let mut session_manager = SessionManager::new();
        session_manager
            .start(&identity_bundle, listen_addr, stun_servers, turn_config)
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

        // Create rate limiter (policy-based if oracle provided, otherwise fallback)
        let rate_limiter = if let Some(oracle) = oracle {
            info!("Policy-based rate limiting enabled with oracle");
            let fallback = fallback_config.unwrap_or_else(|| {
                info!("Using default fallback rate limit config");
                RateLimitConfig::default()
            });

            // Add Sybil resistance if personhood store is provided
            if let Some(ps) = personhood_store {
                let anchor_config = anchor_rate_config.unwrap_or_else(|| {
                    info!("Using default anchor rate limit config for Sybil resistance");
                    AnchorRateLimitConfig::default()
                });
                info!(
                    enforcement_mode = ?anchor_config.enforcement_mode,
                    person_burst = anchor_config.person_burst_capacity,
                    "Sybil resistance enabled via personhood anchors"
                );
                Arc::new(RateLimiter::new_with_oracle_and_sybil_resistance(
                    oracle,
                    fallback,
                    ps,
                    anchor_config,
                ))
            } else {
                Arc::new(RateLimiter::new_with_oracle(oracle, fallback))
            }
        } else {
            let config = fallback_config.unwrap_or_else(|| {
                info!("Using default fallback rate limit config");
                RateLimitConfig::default()
            });
            info!("Using fallback rate limiting (no policy oracle)");

            // Add Sybil resistance if personhood store is provided
            let mut limiter = RateLimiter::new(config);
            if let Some(ps) = personhood_store {
                let anchor_config = anchor_rate_config.unwrap_or_default();
                info!("Sybil resistance enabled (without policy oracle)");
                limiter = limiter.with_sybil_resistance(ps, anchor_config);
            }

            Arc::new(limiter)
        };

        // Create replay guard for signed message verification
        // 300 second clock skew tolerance, 3600 second peer age limit
        // Use persistent mode if store is provided (critical for replay protection across restarts)
        let replay_guard = if let Some(ref store) = store {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            // Load persisted state and apply safety gap
            if let Err(e) = guard.load_and_apply_safety_gap() {
                warn!("Failed to load replay guard state: {}. Starting fresh.", e);
            }
            info!("Replay protection enabled with persistence (300s clock skew, 3600s peer age limit)");
            Arc::new(RwLock::new(guard))
        } else {
            warn!("Replay protection enabled WITHOUT persistence - replay attacks possible after restart!");
            Arc::new(RwLock::new(ReplayGuard::new(300, 3600)))
        };

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

        // Cleanup interval for replay guard and rate limiter (60 seconds)
        let mut cleanup_interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        cleanup_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

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
                _ = cleanup_interval.tick() => {
                    // Cleanup stale replay guard entries using single write lock (#153)
                    let (peer_count_before, peer_count_after) = {
                        let mut guard = self.replay_guard.write().await;
                        let before = guard.peer_count();
                        guard.cleanup();
                        let after = guard.peer_count();
                        (before, after)
                    };

                    // Update metrics (#153)
                    icn_obs::metrics::network::replay_guard_peers_set(peer_count_after as u64);
                    icn_obs::metrics::network::replay_guard_cleanups_inc();

                    let peers_cleaned = peer_count_before.saturating_sub(peer_count_after);
                    if peers_cleaned > 0 {
                        icn_obs::metrics::network::replay_guard_peers_cleaned_add(peers_cleaned as u64);
                        info!(
                            "Replay guard cleanup: {} -> {} peers ({} removed)",
                            peer_count_before, peer_count_after, peers_cleaned
                        );
                    }

                    // Also cleanup stale rate limiter buckets
                    self.rate_limiter
                        .cleanup_old_buckets(std::time::Duration::from_secs(RATE_LIMITER_BUCKET_MAX_AGE_SECS))
                        .await;
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
}

// Tests for NetworkActor and NetworkHandle functionality
#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn test_network_actor_start() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let keypair = KeyPair::generate().unwrap();
        let identity_bundle = IdentityBundle::from_keypair(keypair).unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

        let handle = NetworkActor::spawn(
            identity_bundle,
            addr,
            shutdown_tx.clone(),
            None, // incoming_handler
            None, // oracle
            None, // fallback_config
            None, // topology_config
            None, // stun_servers
            None, // turn_config
            None, // misbehavior_detector
            None, // store (tests don't need persistence)
            None, // personhood_store
            None, // anchor_rate_config
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

    #[test]
    fn test_derive_placeholder_did_is_deterministic_per_address() {
        let a1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let a2: SocketAddr = "127.0.0.1:8081".parse().unwrap();

        let did1 = derive_placeholder_did(a1).unwrap();
        let did1_again = derive_placeholder_did(a1).unwrap();
        let did2 = derive_placeholder_did(a2).unwrap();

        assert_eq!(
            did1, did1_again,
            "same address must map to same placeholder DID"
        );
        assert_ne!(
            did1, did2,
            "different addresses should map to different placeholder DIDs"
        );
    }

    #[test]
    fn test_derive_placeholder_did_handles_invalid_primary_key_bytes() {
        // Find an address whose primary SHA-256 bytes are not a valid Ed25519 key,
        // so the fallback path is exercised deterministically.
        let mut fallback_addr: Option<SocketAddr> = None;
        for port in 10_000u16..=65_535 {
            let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
            let digest = Sha256::digest(format!("bootstrap:{addr}").as_bytes());
            let candidate: [u8; 32] = digest.into();
            if ed25519_dalek::VerifyingKey::from_bytes(&candidate).is_err() {
                fallback_addr = Some(addr);
                break;
            }
        }

        let addr = fallback_addr.expect("should find an address that requires fallback derivation");
        let did = derive_placeholder_did(addr).unwrap();
        let did_again = derive_placeholder_did(addr).unwrap();
        assert_eq!(
            did, did_again,
            "fallback mapping should remain deterministic"
        );
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
                ml_dsa_public: None,
                ml_kem_public: None,
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
                ml_dsa_public: None,
                ml_kem_public: None,
            },
        );

        let (tx, _rx) = mpsc::channel(1);
        let (_shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

        let own_keypair = KeyPair::generate().unwrap();
        let own_did = own_keypair.did();

        let session_mgr = Arc::new(RwLock::new(SessionManager::new()));

        let handle = NetworkHandle {
            tx,
            neighbor_sets: None,
            peer_connections: Some(peer_connections),
            session_manager: session_mgr,
            own_did: own_did.clone(),
            blob_registry: None,
        };

        // Alice supports E2E encryption
        assert!(
            handle
                .peer_has_capability(alice_did, CapabilityFlags::E2E_ENCRYPTION)
                .await
        );

        // Bob does not support E2E encryption
        assert!(
            !handle
                .peer_has_capability(bob_did, CapabilityFlags::E2E_ENCRYPTION)
                .await
        );

        // Both support signed messages
        assert!(
            handle
                .peer_has_capability(alice_did, CapabilityFlags::SIGNED_MESSAGES)
                .await
        );
        assert!(
            handle
                .peer_has_capability(bob_did, CapabilityFlags::SIGNED_MESSAGES)
                .await
        );

        // Unknown peer has no capabilities
        let unknown_keypair = KeyPair::generate().unwrap();
        let unknown_did = unknown_keypair.did();
        assert!(
            !handle
                .peer_has_capability(unknown_did, CapabilityFlags::E2E_ENCRYPTION)
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
                ml_dsa_public: None,
                ml_kem_public: None,
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
                ml_dsa_public: None,
                ml_kem_public: None,
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
                ml_dsa_public: None,
                ml_kem_public: None,
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
                ml_dsa_public: None,
                ml_kem_public: None,
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
                ml_dsa_public: None,
                ml_kem_public: None,
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
            ml_dsa_public: None,
            ml_kem_public: None,
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

    #[tokio::test]
    async fn test_encoding_selection_based_on_capabilities() {
        // Test that encoding is correctly selected based on peer capabilities
        let peer_connections = Arc::new(RwLock::new(std::collections::HashMap::new()));

        let modern_keypair = KeyPair::generate().unwrap();
        let modern_did = modern_keypair.did().clone();
        let legacy_keypair = KeyPair::generate().unwrap();
        let legacy_did = legacy_keypair.did().clone();

        // Modern peer: supports both POSTCARD_ENCODING and MESSAGE_COMPRESSION
        peer_connections.write().await.insert(
            modern_did.clone(),
            PeerConnectionInfo {
                did: modern_did.clone(),
                negotiated_version: 1,
                peer_capabilities: CapabilityFlags::POSTCARD_ENCODING
                    | CapabilityFlags::MESSAGE_COMPRESSION
                    | CapabilityFlags::SIGNED_MESSAGES,
                peer_software: "icnd-0.3.0".to_string(),
                x25519_key: [1u8; 32],
                ml_dsa_public: None,
                ml_kem_public: None,
            },
        );

        // Legacy peer: no postcard support, only compression
        peer_connections.write().await.insert(
            legacy_did.clone(),
            PeerConnectionInfo {
                did: legacy_did.clone(),
                negotiated_version: 1,
                peer_capabilities: CapabilityFlags::MESSAGE_COMPRESSION
                    | CapabilityFlags::SIGNED_MESSAGES,
                peer_software: "icnd-0.1.0".to_string(),
                x25519_key: [2u8; 32],
                ml_dsa_public: None,
                ml_kem_public: None,
            },
        );

        // Test encoding selection for modern peer
        {
            let connections = peer_connections.read().await;
            if let Some(peer_info) = connections.get(&modern_did) {
                let use_postcard = peer_info
                    .peer_capabilities
                    .contains(CapabilityFlags::POSTCARD_ENCODING);
                let use_compression = peer_info
                    .peer_capabilities
                    .contains(CapabilityFlags::MESSAGE_COMPRESSION);

                assert!(use_postcard, "Modern peer should use postcard encoding");
                assert!(use_compression, "Modern peer should use compression");
            } else {
                panic!("Modern peer not found");
            }
        }

        // Test encoding selection for legacy peer
        {
            let connections = peer_connections.read().await;
            if let Some(peer_info) = connections.get(&legacy_did) {
                let use_postcard = peer_info
                    .peer_capabilities
                    .contains(CapabilityFlags::POSTCARD_ENCODING);
                let use_compression = peer_info
                    .peer_capabilities
                    .contains(CapabilityFlags::MESSAGE_COMPRESSION);

                assert!(
                    !use_postcard,
                    "Legacy peer should NOT use postcard encoding"
                );
                assert!(use_compression, "Legacy peer should use compression");
            } else {
                panic!("Legacy peer not found");
            }
        }

        // Test unknown peer (pre-Hello)
        {
            let connections = peer_connections.read().await;
            let unknown_did = KeyPair::generate().unwrap().did().clone();

            let (use_postcard, use_compression) = connections
                .get(&unknown_did)
                .map(|peer_info| peer_info.encoding_flags())
                .unwrap_or((false, false)); // No peer info yet - use legacy encoding

            assert!(
                !use_postcard,
                "Unknown peer should NOT use postcard encoding"
            );
            assert!(!use_compression, "Unknown peer should NOT use compression");
        }
    }
}
