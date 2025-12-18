//! Topology-aware networking integration tests
//!
//! Tests multi-node scenarios with regional topology configuration:
//! - Handshake protocol exchange
//! - NeighborSets categorization (LocalCluster, Regional, Backbone)
//! - Scope-aware gossip fanout
//! - Trust + topology combined peer selection

use anyhow::Result;
use icn_gossip::{GossipActor, Scope};
use icn_identity::{Did, IdentityBundle, KeyPair};
use icn_net::{FanoutConfig, NeighborLimitsConfig, TopologyConfig};
use icn_net::{
    IncomingMessageHandler, MessagePayload, NetworkActor, NetworkHandle, NetworkMessage,
};
use icn_trust::TrustGraph;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

/// Type alias for trust lookup function
type TrustLookup = Arc<dyn Fn(&Did) -> Option<icn_trust::TrustClass> + Send + Sync>;

/// Test node with full stack
struct TestNode {
    _keypair: KeyPair,
    did: Did,
    listen_addr: SocketAddr,
    network_handle: NetworkHandle,
    _gossip_handle: Arc<RwLock<GossipActor>>,
    _trust_graph: Arc<tokio::sync::RwLock<TrustGraph>>,
    _shutdown_tx: broadcast::Sender<()>,
}

impl TestNode {
    async fn spawn(port: u16, region: String, cluster_id: String) -> Result<Self> {
        Self::spawn_with_limits(port, region, cluster_id, None).await
    }

    async fn spawn_with_limits(
        port: u16,
        region: String,
        cluster_id: String,
        custom_limits: Option<NeighborLimitsConfig>,
    ) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();
        let (shutdown_tx, _shutdown_rx) = broadcast::channel(1);

        // Create trust graph
        let store = Arc::new(icn_store::SledStore::temporary()?);
        let trust_graph = Arc::new(tokio::sync::RwLock::new(TrustGraph::new(
            store,
            did.clone(),
        )));

        // Create gossip actor with trust lookup
        let trust_lookup: TrustLookup = Arc::new(|_| Some(icn_trust::TrustClass::Partner)); // Allow all peers for testing
        let gossip_handle = Arc::new(RwLock::new(GossipActor::new(did.clone(), trust_lookup)));

        // Set up incoming message handler for network actor
        let gossip_handle_clone = gossip_handle.clone();
        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                let sender = &net_msg.from;
                let mut gossip = gossip_handle_clone.blocking_write();
                if let Err(e) = gossip.handle_message(sender, gossip_msg) {
                    warn!("Failed to handle gossip message: {}", e);
                }
            }
        });

        // Create topology config with custom or default limits
        let topology_region = region.clone();
        let topology_cluster = cluster_id.clone();
        let neighbor_limits = custom_limits.unwrap_or(NeighborLimitsConfig {
            max_local_cluster: 10,
            max_regional: 10,
            max_backbone: 5,
            max_trusted: 20,
        });
        let topology_config = TopologyConfig {
            region,
            cluster_id,
            role: icn_net::NodeRole::Edge,
            neighbor_limits,
            fanout: FanoutConfig {
                local_cluster: 8,
                regional: 6,
                global: 4,
            },
        };

        // Spawn network actor with topology config
        let listen_addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
        let identity_bundle = IdentityBundle::from_keypair(keypair.clone())?;
        let network_handle = NetworkActor::spawn(
            identity_bundle,
            listen_addr,
            shutdown_tx.clone(),
            Some(incoming_handler),
            Some(trust_graph.clone()),
            None,                  // No trust-gated config for tests
            None,                  // No fallback config for tests
            Some(topology_config), // Enable topology
            None,                  // No STUN servers for tests
            None,                  // No TURN config
            None,                  // No misbehavior detector for tests
        )
        .await?;

        // Wire up gossip send callback
        {
            let mut gossip = gossip_handle.write().await;
            let network_handle_clone = network_handle.clone();
            let did_clone = did.clone();

            let send_callback: icn_gossip::SendMessageCallback =
                Arc::new(move |recipient_opt, gossip_msg| {
                    let net_handle = network_handle_clone.clone();
                    let from = did_clone.clone();

                    // Spawn async task to send message
                    tokio::spawn(async move {
                        let net_msg = NetworkMessage::new(
                            from.clone(),
                            recipient_opt.clone(), // Option<Did>
                            MessagePayload::Gossip(gossip_msg),
                        );

                        let result = if let Some(recipient) = recipient_opt {
                            net_handle.send_message(recipient, net_msg).await
                        } else {
                            net_handle.broadcast(net_msg).await
                        };

                        if let Err(e) = result {
                            warn!("Failed to send gossip message: {}", e);
                        }
                    });
                });

            gossip.set_send_callback(send_callback);
        }

        // Wire up peer sampling callback for scope-aware fanout
        {
            let mut gossip = gossip_handle.write().await;
            let network_handle_clone = network_handle.clone();

            let peer_sampling_callback: icn_gossip::PeerSamplingCallback =
                Arc::new(move |scope, count| {
                    let net_handle = network_handle_clone.clone();
                    tokio::task::block_in_place(move || {
                        tokio::runtime::Handle::current()
                            .block_on(async move { net_handle.sample_peers(scope, count).await })
                    })
                });

            gossip.set_peer_sampling(peer_sampling_callback);
        }

        info!(
            "Node spawned: {} on {} (region: {}, cluster: {})",
            did, listen_addr, topology_region, topology_cluster
        );

        Ok(TestNode {
            _keypair: keypair,
            did,
            listen_addr,
            network_handle,
            _gossip_handle: gossip_handle,
            _trust_graph: trust_graph,
            _shutdown_tx: shutdown_tx,
        })
    }

    /// Dial another node
    async fn dial(&self, other: &TestNode) -> Result<()> {
        self.network_handle
            .dial(other.listen_addr, other.did.clone())
            .await?;
        info!("{} dialed {}", self.did, other.did);
        Ok(())
    }

    /// Get count of peers in each neighbor category
    async fn get_neighbor_counts(&self) -> NeighborCounts {
        if let Some(ref sets) = self.network_handle.neighbor_sets() {
            let sets_read = sets.read().await;
            NeighborCounts {
                local_cluster: sets_read.local_cluster_count(),
                regional: sets_read.regional_count(),
                backbone: sets_read.backbone_count(),
                trusted: sets_read.trusted_count(),
            }
        } else {
            NeighborCounts::default()
        }
    }
}

#[derive(Debug, Default, PartialEq)]
struct NeighborCounts {
    local_cluster: usize,
    regional: usize,
    backbone: usize,
    trusted: usize,
}

#[tokio::test]
async fn test_local_cluster_categorization() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt::try_init();

    // Spawn two nodes in the same local cluster
    let node_a = TestNode::spawn(23000, "us-west".to_string(), "sfo-1".to_string()).await?;
    let node_b = TestNode::spawn(23001, "us-west".to_string(), "sfo-1".to_string()).await?;

    // Node A dials Node B
    node_a.dial(&node_b).await?;

    // Wait for handshake to complete
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify both nodes categorize each other as LocalCluster
    let counts_a = node_a.get_neighbor_counts().await;
    let counts_b = node_b.get_neighbor_counts().await;

    info!("Node A neighbor counts: {:?}", counts_a);
    info!("Node B neighbor counts: {:?}", counts_b);

    assert_eq!(
        counts_a.local_cluster, 1,
        "Node A should have 1 local cluster peer"
    );
    assert_eq!(counts_a.regional, 0, "Node A should have 0 regional peers");
    assert_eq!(counts_a.backbone, 0, "Node A should have 0 backbone peers");

    assert_eq!(
        counts_b.local_cluster, 1,
        "Node B should have 1 local cluster peer"
    );
    assert_eq!(counts_b.regional, 0, "Node B should have 0 regional peers");
    assert_eq!(counts_b.backbone, 0, "Node B should have 0 backbone peers");

    Ok(())
}

#[tokio::test]
async fn test_regional_categorization() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt::try_init();

    // Spawn two nodes in same region but different clusters
    let node_a = TestNode::spawn(23100, "us-west".to_string(), "sfo-1".to_string()).await?;
    let node_b = TestNode::spawn(23101, "us-west".to_string(), "lax-1".to_string()).await?;

    // Node A dials Node B
    node_a.dial(&node_b).await?;

    // Wait for handshake
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify both nodes categorize each other as Regional
    let counts_a = node_a.get_neighbor_counts().await;
    let counts_b = node_b.get_neighbor_counts().await;

    info!("Node A neighbor counts: {:?}", counts_a);
    info!("Node B neighbor counts: {:?}", counts_b);

    assert_eq!(
        counts_a.local_cluster, 0,
        "Node A should have 0 local cluster peers"
    );
    assert_eq!(counts_a.regional, 1, "Node A should have 1 regional peer");
    assert_eq!(counts_a.backbone, 0, "Node A should have 0 backbone peers");

    assert_eq!(
        counts_b.local_cluster, 0,
        "Node B should have 0 local cluster peers"
    );
    assert_eq!(counts_b.regional, 1, "Node B should have 1 regional peer");
    assert_eq!(counts_b.backbone, 0, "Node B should have 0 backbone peers");

    Ok(())
}

#[tokio::test]
async fn test_backbone_categorization() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt::try_init();

    // Spawn two nodes in different regions
    let node_a = TestNode::spawn(23200, "us-west".to_string(), "sfo-1".to_string()).await?;
    let node_b = TestNode::spawn(23201, "eu-central".to_string(), "fra-1".to_string()).await?;

    // Node A dials Node B
    node_a.dial(&node_b).await?;

    // Wait for handshake
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify both nodes categorize each other as Backbone
    let counts_a = node_a.get_neighbor_counts().await;
    let counts_b = node_b.get_neighbor_counts().await;

    info!("Node A neighbor counts: {:?}", counts_a);
    info!("Node B neighbor counts: {:?}", counts_b);

    assert_eq!(
        counts_a.local_cluster, 0,
        "Node A should have 0 local cluster peers"
    );
    assert_eq!(counts_a.regional, 0, "Node A should have 0 regional peers");
    assert_eq!(counts_a.backbone, 1, "Node A should have 1 backbone peer");

    assert_eq!(
        counts_b.local_cluster, 0,
        "Node B should have 0 local cluster peers"
    );
    assert_eq!(counts_b.regional, 0, "Node B should have 0 regional peers");
    assert_eq!(counts_b.backbone, 1, "Node B should have 1 backbone peer");

    Ok(())
}

/// Test multi-region topology neighbor categorization.
///
/// Verifies that nodes correctly categorize peers based on topology metadata:
/// - Local cluster: same region and cluster_id
/// - Regional: same region, different cluster_id  
/// - Backbone: different region
#[tokio::test(flavor = "multi_thread")]
async fn test_multi_region_topology() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt::try_init();

    // Create a multi-region topology:
    // - 2 nodes in us-west/sfo-1 (local cluster)
    // - 1 node in us-west/lax-1 (regional)
    // - 1 node in eu-central/fra-1 (backbone)

    let node_a = TestNode::spawn(23300, "us-west".to_string(), "sfo-1".to_string()).await?;
    let node_b = TestNode::spawn(23301, "us-west".to_string(), "sfo-1".to_string()).await?;
    let node_c = TestNode::spawn(23302, "us-west".to_string(), "lax-1".to_string()).await?;
    let node_d = TestNode::spawn(23303, "eu-central".to_string(), "fra-1".to_string()).await?;

    // Node A connects to all others
    node_a.dial(&node_b).await?;
    node_a.dial(&node_c).await?;
    node_a.dial(&node_d).await?;

    // Wait for handshakes and categorization with retry logic
    // Handshakes can take variable time depending on system load
    // Use a longer timeout for CI environments (up to 10 seconds)
    let mut counts_a = node_a.get_neighbor_counts().await;
    for attempt in 1..=40 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        counts_a = node_a.get_neighbor_counts().await;
        
        // Check if all peers are categorized correctly
        if counts_a.local_cluster == 1 && counts_a.regional == 1 && counts_a.backbone == 1 {
            info!("All peers categorized correctly after {} attempts ({} ms)", 
                  attempt, attempt * 250);
            break;
        }
        
        if attempt % 5 == 0 {
            info!(
                "Attempt {}: local_cluster={}, regional={}, backbone={} (waiting...)",
                attempt, counts_a.local_cluster, counts_a.regional, counts_a.backbone
            );
        }
    }

    // Verify Node A's neighbor categorization
    info!("Final Node A neighbor counts: {:?}", counts_a);

    assert_eq!(
        counts_a.local_cluster, 1,
        "Node A should have 1 local cluster peer (node_b)"
    );
    assert_eq!(
        counts_a.regional, 1,
        "Node A should have 1 regional peer (node_c)"
    );
    assert_eq!(
        counts_a.backbone, 1,
        "Node A should have 1 backbone peer (node_d)"
    );

    Ok(())
}

/// Test scope-aware peer sampling with multiple topology levels.
///
/// Verifies that peer sampling respects topology scopes:
/// - LocalCluster scope returns only local cluster peers
/// - Regional scope returns regional (different cluster, same region) peers
/// - Global scope returns all connected peers
#[tokio::test(flavor = "multi_thread")]
async fn test_scope_aware_peer_sampling() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt::try_init();

    // Create topology with multiple peer types
    let node_a = TestNode::spawn(23400, "us-west".to_string(), "sfo-1".to_string()).await?;
    let node_b = TestNode::spawn(23401, "us-west".to_string(), "sfo-1".to_string()).await?; // Local
    let node_c = TestNode::spawn(23402, "us-west".to_string(), "lax-1".to_string()).await?; // Regional
    let node_d = TestNode::spawn(23403, "eu-central".to_string(), "fra-1".to_string()).await?; // Backbone

    // Node A connects to all (with delays to avoid QUIC handshake race conditions)
    node_a.dial(&node_b).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    node_a.dial(&node_c).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    node_a.dial(&node_d).await?;

    // Wait for connections to be established with retry logic for CI environments
    // CI is slower, so we poll with fixed delays until all peers connect
    // Increased attempts and delay to handle port conflicts when tests run concurrently
    let mut global_peers = Vec::new();
    for attempt in 1..=40 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        global_peers = node_a.network_handle.sample_peers(Scope::Global, 10).await;
        if global_peers.len() >= 3 {
            info!("All 3 peers connected after {} attempts", attempt);
            break;
        }
        if attempt % 5 == 0 {
            info!(
                "Attempt {}: {} peers connected, waiting...",
                attempt,
                global_peers.len()
            );
        }
    }
    assert_eq!(
        global_peers.len(),
        3,
        "Failed to connect to all 3 peers after retries"
    );

    // Test LocalCluster scope sampling
    let local_peers = node_a
        .network_handle
        .sample_peers(Scope::LocalCluster, 10)
        .await;
    info!("LocalCluster sampled peers: {:?}", local_peers);
    assert_eq!(local_peers.len(), 1, "Should sample 1 local cluster peer");
    assert_eq!(local_peers[0], node_b.did, "Should sample node_b");

    // Test Regional scope sampling
    let regional_peers = node_a
        .network_handle
        .sample_peers(Scope::Regional, 10)
        .await;
    info!("Regional sampled peers: {:?}", regional_peers);
    assert_eq!(
        regional_peers.len(),
        2,
        "Should sample 2 regional peers (local + regional)"
    );

    // Test Global scope sampling (already fetched above)
    info!("Global sampled peers: {:?}", global_peers);
    assert_eq!(global_peers.len(), 3, "Should sample 3 global peers (all)");

    Ok(())
}

#[tokio::test]
async fn test_neighbor_set_lru_eviction() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt::try_init();

    // Create node with very small limits for testing eviction
    let custom_limits = NeighborLimitsConfig {
        max_local_cluster: 2, // Only allow 2 local cluster peers
        max_regional: 2,
        max_backbone: 2,
        max_trusted: 20,
    };

    let node_a = TestNode::spawn_with_limits(
        23500,
        "us-west".to_string(),
        "sfo-1".to_string(),
        Some(custom_limits),
    )
    .await?;

    // Spawn 3 nodes in same local cluster
    let node_b = TestNode::spawn(23501, "us-west".to_string(), "sfo-1".to_string()).await?;
    let node_c = TestNode::spawn(23502, "us-west".to_string(), "sfo-1".to_string()).await?;
    let node_d = TestNode::spawn(23503, "us-west".to_string(), "sfo-1".to_string()).await?;

    // Connect to all 3
    node_a.dial(&node_b).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    node_a.dial(&node_c).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    node_a.dial(&node_d).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify that only 2 peers are kept (LRU eviction)
    let counts_a = node_a.get_neighbor_counts().await;
    info!(
        "Node A neighbor counts after connecting to 3 peers: {:?}",
        counts_a
    );

    // Note: Due to the limit of 2, one peer should have been evicted
    assert!(
        counts_a.local_cluster <= 2,
        "Should not exceed local cluster limit of 2"
    );

    Ok(())
}
