//! Network bootstrap initialization
//!
//! Handles network bootstrapping tasks:
//! - Dialing bootstrap peers for WAN connectivity
//! - Requesting peer exchange from connected peers
//! - Announcing connection candidates for NAT traversal
//! - Publishing node profile for peer capability discovery

use std::net::SocketAddr;
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

/// Dial bootstrap peers.
///
/// Returns the **addresses** transport was established to, not peer identities. Neither
/// bootstrap form authenticates anyone: `parse_bootstrap_peer` yields configuration, and a
/// peer's identity is proven only by DID-TLS binding verification in the Hello handler. A
/// configured `KnownDid` is therefore an expectation, and the `AddrOnly` placeholder is not
/// even that — see [`icn_net::NetworkHandle::dial_addr`] and #2530.
///
/// The addresses are what lets [`request_peer_exchange`] find *these* peers once they
/// authenticate, without widening to every peer the node happens to be connected to.
pub async fn dial_bootstrap_peers(
    config: &BootstrapConfig,
    network_handle: &NetworkHandle,
) -> Vec<SocketAddr> {
    if config.bootstrap_peers.is_empty() {
        return Vec::new();
    }

    info!("Dialing {} bootstrap peers", config.bootstrap_peers.len());
    let mut dialed_endpoints = Vec::new();

    for peer_url in &config.bootstrap_peers {
        match super::bridge::parse_bootstrap_peer(peer_url).await {
            Ok(super::bridge::BootstrapPeer::KnownDid {
                did: peer_did,
                addr: peer_addr,
            }) => {
                info!(
                    "Connecting to bootstrap peer: {} at {}",
                    peer_did, peer_addr
                );
                match network_handle.dial(peer_addr, peer_did.clone()).await {
                    Ok(_) => {
                        // The configured DID is an expectation, not a result: this records
                        // that transport came up, and the peer is named only once Hello
                        // authenticates it (#2530).
                        info!(
                            "✓ Reached bootstrap peer at {} (expected {}; awaiting Hello)",
                            peer_addr, peer_did
                        );
                        dialed_endpoints.push(peer_addr);
                    }
                    Err(e) => {
                        warn!("Failed to connect to bootstrap peer {}: {}", peer_did, e)
                    }
                }
            }
            Ok(super::bridge::BootstrapPeer::AddrOnly { addr: peer_addr }) => {
                info!("Connecting to bootstrap peer (addr-only): {}", peer_addr);
                // Dial without a known DID. The value returned here is a
                // placeholder derived from the address, NOT the peer's
                // identity; the peer's authenticated DID is established
                // separately by the Hello handshake.
                match network_handle.dial_addr(peer_addr).await {
                    Ok(placeholder_did) => {
                        info!(
                            "✓ Connected to bootstrap peer at {} (placeholder key {}; \
                             peer identity not yet authenticated — awaiting Hello)",
                            peer_addr, placeholder_did
                        );
                        dialed_endpoints.push(peer_addr);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to connect to bootstrap peer at {}: {}",
                            peer_addr, e
                        )
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

    dialed_endpoints
}

/// Which peers this bootstrap operation may request peer exchange from.
///
/// A target must satisfy both halves: it authenticated (so it came from
/// `connected_peer_endpoints`, whose entries exist only after Hello proved a DID against the
/// connection's certificate), **and** it is reachable at an endpoint this bootstrap
/// operation actually dialed. Either half alone is wrong — a configured DID that never
/// authenticated is not a peer, and an authenticated peer we never dialed is not a bootstrap
/// peer.
///
/// Matching on the dialed address is sound here because bootstrap dialing is direct-only:
/// `NetworkHandle::dial` passes `peer_relay_addr: None`, so `handle_dial` cannot take the
/// TURN branch, and the connection's remote address is the address we dialed. (Under
/// Kubernetes DNAT the client still observes the service address it sent to, because
/// conntrack rewrites the reply source back.) If bootstrap ever gains relay fallback, a
/// relayed connection's remote address is the local proxy's, and this correlation would need
/// revisiting.
fn select_peer_exchange_targets(
    dialed_endpoints: &[SocketAddr],
    authenticated_peers: Vec<(Did, SocketAddr)>,
) -> Vec<Did> {
    let mut targets: Vec<Did> = Vec::new();
    for (did, addr) in authenticated_peers {
        if dialed_endpoints.contains(&addr) && !targets.contains(&did) {
            targets.push(did);
        }
    }
    targets
}

/// Request peer exchange from the bootstrap peers that authenticated.
///
/// Targets are resolved *after* the handshake window, by matching live authenticated
/// sessions against the addresses we actually dialed. A bootstrap entry names an endpoint
/// and, at most, an expectation about who is there; only Hello says who is really there, so
/// a configured DID must not become a peer-exchange target on its own (#2530, and the
/// "configuration is not verification" principle settled in #2529).
///
/// Matching on the dialed address keeps this scoped to bootstrap peers. Asking only for
/// "every connected peer" would quietly turn this into a request to the whole neighbourhood
/// — including inbound peers this node never chose to bootstrap from.
pub async fn request_peer_exchange(
    config: &BootstrapConfig,
    network_handle: &NetworkHandle,
    dialed_endpoints: Vec<SocketAddr>,
) {
    if !config.federation_enabled || dialed_endpoints.is_empty() {
        return;
    }

    let network_filter = if config.network_name != "icn-mainnet" {
        Some(config.network_name.clone())
    } else {
        None
    };

    let peer_exchange_delay = Duration::from_millis(config.peer_exchange_delay_ms);
    tokio::time::sleep(peer_exchange_delay).await;

    let targets = select_peer_exchange_targets(
        &dialed_endpoints,
        network_handle.connected_peer_endpoints().await,
    );

    if targets.is_empty() {
        info!(
            "Federation enabled - none of the {} bootstrap endpoints has an authenticated peer yet; \
             not requesting peer exchange",
            dialed_endpoints.len()
        );
        return;
    }

    info!(
        "Federation enabled - requesting peer exchange from {} authenticated bootstrap peers",
        targets.len()
    );

    for peer_did in targets {
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
                "Connection candidate: local={:?}, public={:?}, relay={:?}",
                candidate.local_addr(),
                candidate.public_addr(),
                candidate.relay_addr()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn did(s: &str) -> Did {
        // Real Ed25519-derived DIDs: the selection rule compares identities, so seeding
        // from distinct keys keeps the fixtures distinct the same way production ones are.
        let seed = {
            let mut bytes = [0u8; 32];
            for (i, b) in s.bytes().enumerate().take(32) {
                bytes[i] = b;
            }
            bytes
        };
        Did::from_public_key(&ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key())
    }

    fn addr(port: u16) -> SocketAddr {
        format!("10.0.0.1:{port}").parse().expect("valid addr")
    }

    /// Peer exchange goes only to peers that both authenticated **and** sit at an endpoint
    /// this bootstrap operation dialed.
    ///
    /// The two exclusions are the ones that matter. An endpoint we dialed but that never
    /// authenticated contributes nothing: transport success is not identity, so there is no
    /// DID to address (#2530). And an authenticated peer at some other endpoint is a peer of
    /// this node but not a *bootstrap* peer — including it would silently turn a bootstrap
    /// step into a request to the whole neighbourhood, including inbound peers this node
    /// never chose.
    #[test]
    fn targets_are_authenticated_peers_at_endpoints_we_dialed() {
        let bootstrap_a = addr(7001);
        let bootstrap_b = addr(7002);
        let unrelated_c = addr(7003);

        let targets = select_peer_exchange_targets(
            &[bootstrap_a, bootstrap_b],
            vec![
                (did("PeerA"), bootstrap_a),
                // C authenticated, but we never dialed it as a bootstrap peer.
                (did("PeerC"), unrelated_c),
            ],
        );

        assert_eq!(
            targets,
            vec![did("PeerA")],
            "only the authenticated peer at a dialed bootstrap endpoint may be a target"
        );
        // B was dialed but nobody authenticated there, so it contributes no DID at all.
        assert!(!targets.iter().any(|t| t.as_str().contains("PeerB")));
    }

    /// A DID reachable at two dialed endpoints is requested from once.
    ///
    /// Bootstrap lists legitimately name the same node twice — a service address and a
    /// direct one, or an IPv4 and IPv6 entry — and the peer that answers both is one peer.
    #[test]
    fn targets_are_deduplicated_across_endpoints() {
        let first = addr(7001);
        let second = addr(7002);

        let targets = select_peer_exchange_targets(
            &[first, second, first],
            vec![(did("SamePeer"), first), (did("SamePeer"), second)],
        );

        assert_eq!(targets, vec![did("SamePeer")]);
    }

    /// With nothing dialed, nothing is a bootstrap target — not even a live peer.
    #[test]
    fn no_dialed_endpoints_means_no_targets() {
        let targets =
            select_peer_exchange_targets(&[], vec![(did("SomeConnectedPeer"), addr(7009))]);
        assert!(
            targets.is_empty(),
            "an authenticated peer we did not dial is not a bootstrap peer"
        );
    }
}
