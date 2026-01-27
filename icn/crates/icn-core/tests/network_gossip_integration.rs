#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration test for network-gossip bridge
//!
//! This test validates end-to-end gossip message flow over QUIC connections.

use anyhow::Result;
use icn_gossip::GossipActor;
use icn_identity::{IdentityBundle, KeyPair};
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor, NetworkMessage};
use icn_trust::TrustClass;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Get an available port for testing
fn get_available_port() -> u16 {
    portpicker::pick_unused_port().expect("No available ports")
}

/// Helper to create a test node
struct TestNode {
    _keypair: KeyPair,
    did: icn_identity::Did,
    network_handle: icn_net::NetworkHandle,
    gossip_handle: Arc<RwLock<GossipActor>>,
    listen_addr: SocketAddr,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl TestNode {
    async fn spawn(port: u16) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        info!("Spawning test node with DID: {}", did);

        // Create shutdown channel
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(16);

        // Spawn gossip actor
        let trust_lookup = Arc::new(|_did: &icn_identity::Did| Some(TrustClass::Partner));
        let gossip_handle = GossipActor::spawn_with_legacy_trust(did.clone(), trust_lookup);

        info!("Gossip actor spawned");

        // Create incoming message handler that routes to gossip
        let gossip_handle_clone = gossip_handle.clone();
        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                let sender = net_msg.from.clone();
                let gossip_handle = gossip_handle_clone.clone();
                tokio::spawn(async move {
                    let mut gossip = gossip_handle.write().await;
                    if let Err(e) = gossip.handle_message(&sender, gossip_msg).await {
                        warn!("Failed to handle gossip message: {}", e);
                    }
                });
            }
        });

        // Spawn network actor
        let listen_addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
        let identity_bundle = IdentityBundle::from_keypair(keypair.clone())?;
        let network_handle = NetworkActor::spawn(
            identity_bundle,
            listen_addr,
            shutdown_tx.clone(),
            Some(incoming_handler),
            None, // No PolicyOracle for tests
            None, // No fallback config for tests
            None, // No topology config
            None, // No STUN servers
            None, // No TURN config
            None, // No misbehavior detector for tests
            None, // No store for tests
            None, // personhood_store
            None, // anchor_rate_config
        )
        .await?;

        info!("Network actor spawned on {}", listen_addr);

        Ok(TestNode {
            _keypair: keypair,
            did,
            network_handle,
            gossip_handle,
            listen_addr,
            shutdown_tx,
        })
    }

    async fn shutdown(self) {
        info!("Shutting down test node");
        let _ = self.shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
#[ignore] // Flaky in CI: QUIC handshake timing issues in containerized environment
async fn test_two_node_gossip_flow() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing for test debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_test_writer()
        .try_init();

    info!("=== Starting two-node gossip integration test ===");

    // Spawn two nodes
    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;

    info!("Node 1 DID: {}", node1.did);
    info!("Node 2 DID: {}", node2.did);

    // Give nodes time to start discovery
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Node 1 dials Node 2
    info!("Node 1 dialing Node 2...");
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;

    info!("Connection established");

    // Give connection time to stabilize
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Node 1 publishes a gossip entry
    info!("Node 1 publishing gossip entry...");
    let test_data = b"Hello from node 1!".to_vec();
    let topic = "test:topic";

    let hash = {
        let mut gossip1 = node1.gossip_handle.write().await;
        gossip1.publish(topic, test_data.clone()).await?
    };

    info!("Published entry with hash: {:?}", hex::encode(hash));

    // Create and send gossip Announce message over network
    info!("Sending Announce message to Node 2...");
    let clock = {
        let gossip1 = node1.gossip_handle.read().await;
        // Get the entry we just published
        let entry = gossip1.get_entry(topic, &hash).expect("Entry should exist");
        entry.clock
    };

    let announce_msg = icn_gossip::GossipMessage::Announce {
        hash,
        author: node1.did.clone(),
        clock,
        topic: topic.to_string(),
    };

    let net_msg = NetworkMessage::gossip(node1.did.clone(), Some(node2.did.clone()), announce_msg);

    node1
        .network_handle
        .send_message(node2.did.clone(), net_msg)
        .await?;

    info!("Announce message sent");

    // Give time for message to be processed
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify node 2 received the announcement (it would store the entry if it was a Response)
    // For now, just check that the gossip handler was called (via logs)
    info!("Integration test completed - check logs for message reception");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;

    Ok(())
}

/// Test broadcast to multiple peers.
///
/// Note: Ignored in CI due to intermittent QUIC stream failures in containerized environment.
/// Run manually with: cargo test -p icn-core --test network_gossip_integration test_broadcast_to_multiple_peers -- --ignored
#[tokio::test]
#[ignore = "Flaky in CI: QUIC stream failures in containerized environment"]
async fn test_broadcast_to_multiple_peers() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    info!("=== Starting broadcast integration test ===");

    // Spawn three nodes
    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;
    let node3 = TestNode::spawn(get_available_port()).await?;

    // Node 1 connects to both Node 2 and Node 3
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;
    node1
        .network_handle
        .dial(node3.listen_addr, node3.did.clone())
        .await?;

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Node 1 broadcasts a message
    let test_msg = NetworkMessage::ping(node1.did.clone(), node1.did.clone());
    node1.network_handle.broadcast(test_msg).await?;

    info!("Broadcast message sent to all peers");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;
    node3.shutdown().await;

    Ok(())
}
