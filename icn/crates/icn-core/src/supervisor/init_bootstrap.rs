//! Network bootstrap initialization
//!
//! Handles network bootstrapping tasks:
//! - Dialing bootstrap peers for WAN connectivity
//! - Requesting peer exchange from connected peers
//! - Announcing connection candidates for NAT traversal
//! - Publishing node profile for peer capability discovery

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_net::NetworkHandle;

use crate::config::{FederationConfig, SupervisorConfig};
use crate::node::NodeProfile;

/// Configuration for bootstrap operations
pub struct BootstrapConfig {
    /// List of bootstrap peer URLs
    pub bootstrap_peers: Vec<String>,
    /// Whether federation is enabled
    pub federation_enabled: bool,
    /// Network name for filtering peer exchange
    pub network_name: String,
    /// Delay before requesting peer exchange (ms)
    pub peer_exchange_delay_ms: u64,
    /// Maximum peers to request in exchange
    pub peer_exchange_max_peers: usize,
}

impl BootstrapConfig {
    /// Create from component configs
    pub fn from_configs(
        bootstrap_peers: Vec<String>,
        federation: &FederationConfig,
        supervisor: &SupervisorConfig,
    ) -> Self {
        Self {
            bootstrap_peers,
            federation_enabled: federation.enabled,
            network_name: federation.network_name.clone(),
            peer_exchange_delay_ms: supervisor.peer_exchange_delay_ms,
            peer_exchange_max_peers: supervisor.peer_exchange_max_peers,
        }
    }
}

/// Dial bootstrap peers and optionally request peer exchange
///
/// Returns the list of successfully connected peer DIDs.
pub async fn dial_bootstrap_peers(
    config: &BootstrapConfig,
    network_handle: &NetworkHandle,
) -> Vec<Did> {
    if config.bootstrap_peers.is_empty() {
        return Vec::new();
    }

    info!("Dialing {} bootstrap peers", config.bootstrap_peers.len());
    let mut connected_peers = Vec::new();

    for peer_url in &config.bootstrap_peers {
        match super::parse_bootstrap_peer(peer_url).await {
            Ok((peer_did, peer_addr)) => {
                info!(
                    "Connecting to bootstrap peer: {} at {}",
                    peer_did, peer_addr
                );
                match network_handle.dial(peer_addr, peer_did.clone()).await {
                    Ok(_) => {
                        info!("✓ Connected to bootstrap peer: {}", peer_did);
                        connected_peers.push(peer_did);
                    }
                    Err(e) => {
                        warn!("Failed to connect to bootstrap peer {}: {}", peer_did, e)
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Failed to parse/resolve bootstrap peer URL '{}': {}",
                    peer_url, e
                );
            }
        }
    }

    connected_peers
}

/// Request peer exchange from connected bootstrap peers
pub async fn request_peer_exchange(
    config: &BootstrapConfig,
    network_handle: &NetworkHandle,
    connected_peers: Vec<Did>,
) {
    if !config.federation_enabled || connected_peers.is_empty() {
        return;
    }

    info!(
        "Federation enabled - requesting peer exchange from {} bootstrap peers",
        connected_peers.len()
    );

    let network_filter = if config.network_name != "icn-mainnet" {
        Some(config.network_name.clone())
    } else {
        None
    };

    let peer_exchange_delay = Duration::from_millis(config.peer_exchange_delay_ms);

    for peer_did in connected_peers {
        // Small delay to allow Hello handshake to complete
        tokio::time::sleep(peer_exchange_delay).await;

        match network_handle
            .request_peer_exchange(
                &peer_did,
                Some(config.peer_exchange_max_peers),
                network_filter.clone(),
            )
            .await
        {
            Ok(_) => info!("✓ Requested peer exchange from {}", peer_did),
            Err(e) => {
                debug!("Failed to request peer exchange from {}: {}", peer_did, e)
            }
        }
    }
}

/// Announce connection candidate for NAT traversal
pub async fn announce_connection_candidate(
    network_handle: &NetworkHandle,
    gossip_handle: &Arc<RwLock<GossipActor>>,
) {
    info!("Announcing connection candidate for NAT traversal...");

    match network_handle.connection_candidate().await {
        Ok(candidate) => {
            info!(
                "Connection candidate: local={}, public={:?}, relay={:?}",
                candidate.local_addr, candidate.public_addr, candidate.relay_addr
            );

            // Serialize candidate and publish to gossip
            match serde_json::to_vec(&candidate) {
                Ok(candidate_bytes) => {
                    let mut gossip = gossip_handle.write().await;
                    match gossip
                        .publish(
                            super::init_gossip::NETWORK_CANDIDATES_TOPIC,
                            candidate_bytes,
                        )
                        .await
                    {
                        Ok(_) => info!("✓ Published connection candidate to gossip"),
                        Err(e) => {
                            warn!("Failed to publish connection candidate: {}", e)
                        }
                    }
                }
                Err(e) => warn!("Failed to serialize connection candidate: {}", e),
            }
        }
        Err(e) => warn!("Failed to get connection candidate: {}", e),
    }
}

/// Subscribe to node profiles topic and announce our profile
pub async fn announce_node_profile(
    gossip_handle: &Arc<RwLock<GossipActor>>,
    did: &Did,
    node_profile: &NodeProfile,
) {
    let mut gossip = gossip_handle.write().await;

    // Subscribe to network:profiles topic for peer capability discovery
    if let Err(e) = gossip
        .subscribe(crate::node::TOPIC_NODE_PROFILES, did.clone())
        .await
    {
        warn!("Failed to subscribe to network:profiles topic: {}", e);
    } else {
        info!("Subscribed to network:profiles topic");
    }

    // Publish our node profile announcement
    let profile_msg = crate::node::ProfileMessage::Announce(node_profile.clone());
    match serde_json::to_vec(&profile_msg) {
        Ok(profile_bytes) => {
            match gossip
                .publish(crate::node::TOPIC_NODE_PROFILES, profile_bytes)
                .await
            {
                Ok(_) => {
                    info!(
                        "✓ Published node profile: {} roles ({:?}), {} extended capabilities",
                        node_profile.roles.len(),
                        node_profile.roles_sorted(),
                        node_profile.extended.capabilities.len(),
                    );
                }
                Err(e) => warn!("Failed to publish node profile: {}", e),
            }
        }
        Err(e) => warn!("Failed to serialize node profile: {}", e),
    }
}

/// Perform all bootstrap operations
///
/// This is a convenience function that runs all bootstrap steps in order:
/// 1. Dial bootstrap peers
/// 2. Request peer exchange (if federation enabled)
/// 3. Announce connection candidate
/// 4. Announce node profile
pub async fn run_bootstrap(
    config: &BootstrapConfig,
    network_handle: &NetworkHandle,
    gossip_handle: &Arc<RwLock<GossipActor>>,
    did: &Did,
    node_profile: &NodeProfile,
) {
    // Dial bootstrap peers
    let connected_peers = dial_bootstrap_peers(config, network_handle).await;

    // Request peer exchange from connected peers
    request_peer_exchange(config, network_handle, connected_peers).await;

    // Announce connection candidate for NAT traversal
    announce_connection_candidate(network_handle, gossip_handle).await;

    // Announce node profile
    announce_node_profile(gossip_handle, did, node_profile).await;
}
