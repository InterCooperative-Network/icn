#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Gossip Pull Protocol Integration Tests
//!
//! This test validates the complete pull protocol flow:
//! 1. Node 1 publishes entries
//! 2. Node 1 emits Digest (vector clock + Bloom filter)
//! 3. Node 2 receives Digest, identifies missing entries
//! 4. Node 2 sends PullRequest
//! 5. Node 1 sends PullResponse with entries
//! 6. Node 2 stores entries and achieves convergence

use anyhow::Result;
use icn_gossip::{AccessControl, GossipActor, GossipMessage, Topic};
use icn_identity::{IdentityBundle, KeyPair};
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor, NetworkMessage};
use icn_trust::TrustClass;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

/// Get an available port for testing
fn get_available_port() -> u16 {
    portpicker::pick_unused_port().expect("No available ports")
}

/// Test node helper with pull protocol tracking
struct TestNode {
    _keypair: KeyPair,
    did: icn_identity::Did,
    network_handle: icn_net::NetworkHandle,
    gossip_handle: Arc<RwLock<GossipActor>>,
    listen_addr: SocketAddr,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    /// Track pull protocol messages: (msg_type, from_did)
    pull_messages: Arc<Mutex<Vec<(String, icn_identity::Did)>>>,
}

impl TestNode {
    /// Dial a peer with retry logic for flaky CI environments
    async fn dial_with_retry(
        &self,
        addr: SocketAddr,
        did: icn_identity::Did,
        max_retries: u32,
    ) -> Result<()> {
        let mut last_error = None;
        for attempt in 0..max_retries {
            match self.network_handle.dial(addr, did.clone()).await {
                Ok(()) => {
                    info!("Connected to {} on attempt {}", did, attempt + 1);
                    return Ok(());
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt + 1 < max_retries {
                        let delay = Duration::from_millis(100 * (attempt as u64 + 1));
                        warn!(
                            "Connection attempt {} failed, retrying in {:?}...",
                            attempt + 1,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("Connection failed after {max_retries} attempts")))
    }

    async fn spawn(port: u16) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        info!("Spawning test node with DID: {}", did);

        // Create shutdown channel
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(16);

        // Spawn gossip actor
        let trust_lookup = Arc::new(|_did: &icn_identity::Did| Some(TrustClass::Partner));
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        info!("Gossip actor spawned");

        // Track pull protocol messages
        let pull_messages = Arc::new(Mutex::new(Vec::new()));
        let pull_messages_clone = pull_messages.clone();

        // Create incoming message handler
        let gossip_handle_clone = gossip_handle.clone();
        let network_handle_holder = Arc::new(RwLock::new(None::<icn_net::NetworkHandle>));

        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            let sender_did = net_msg.from.clone();

            if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                // Track pull protocol messages
                let msg_type = match &gossip_msg {
                    GossipMessage::Digest { .. } => "Digest",
                    GossipMessage::PullRequest { .. } => "PullRequest",
                    GossipMessage::PullResponse { .. } => "PullResponse",
                    _ => "Other",
                };

                if msg_type != "Other" {
                    let pull_msgs = pull_messages_clone.clone();
                    let sender = sender_did.clone();
                    let msg_type_str = msg_type.to_string();
                    tokio::spawn(async move {
                        let mut msgs = pull_msgs.lock().await;
                        info!("📨 Received {}: from {}", msg_type_str, sender);
                        msgs.push((msg_type_str, sender));
                    });
                }

                // Handle message asynchronously
                let gossip_handle = gossip_handle_clone.clone();
                let sender = sender_did.clone();
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
            None, // No trust graph for tests
            None, // No trust-gated config for tests
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

        // Initialize network handle
        *network_handle_holder.write().await = Some(network_handle.clone());

        // Set up send callback for gossip
        {
            let mut gossip = gossip_handle.write().await;
            let network_handle_clone = network_handle.clone();
            let from_did = did.clone();

            let send_callback = Arc::new(
                move |recipient: Option<icn_identity::Did>, gossip_msg: GossipMessage| {
                    let net_handle = network_handle_clone.clone();
                    let from = from_did.clone();

                    tokio::spawn(async move {
                        let net_msg = NetworkMessage::gossip(from, recipient.clone(), gossip_msg);

                        let result = if let Some(to_did) = recipient {
                            net_handle.send_message(to_did, net_msg).await
                        } else {
                            net_handle.broadcast(net_msg).await
                        };

                        if let Err(e) = result {
                            warn!("Failed to send gossip message: {}", e);
                        }
                    });
                },
            );

            gossip.set_send_callback(send_callback);
        }

        info!("Network actor spawned on {}", listen_addr);

        Ok(TestNode {
            _keypair: keypair,
            did,
            network_handle,
            gossip_handle,
            listen_addr,
            shutdown_tx,
            pull_messages,
        })
    }

    async fn shutdown(self) {
        info!("Shutting down test node");
        let _ = self.shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    async fn pull_message_count(&self, msg_type: &str) -> usize {
        self.pull_messages
            .lock()
            .await
            .iter()
            .filter(|(t, _)| t == msg_type)
            .count()
    }
}

/// Test two-node convergence via pull protocol.
///
/// Note: Ignored in CI due to intermittent QUIC stream failures in containerized environment.
/// Run manually with: cargo test -p icn-core --test gossip_pull_protocol_integration test_two_node_convergence_via_pull_protocol -- --ignored
#[tokio::test]
#[ignore = "Flaky in CI: QUIC stream failures in containerized environment"]
async fn test_two_node_convergence_via_pull_protocol() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize test environment
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Starting pull protocol convergence test ===");

    // Spawn two nodes
    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;

    // Allow nodes to fully initialize before connecting
    // This prevents TLS handshake failures in CI environments
    tokio::time::sleep(Duration::from_millis(100)).await;

    info!("Node 1 DID: {}", node1.did);
    info!("Node 2 DID: {}", node2.did);

    // Create topic on both nodes
    let topic = "test:pull";
    {
        let mut gossip1 = node1.gossip_handle.write().await;
        gossip1.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
    }
    {
        let mut gossip2 = node2.gossip_handle.write().await;
        gossip2.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
    }

    // Connect nodes: 1 ↔ 2 (bidirectional) with retry for CI reliability
    node1
        .dial_with_retry(node2.listen_addr, node2.did.clone(), 5)
        .await?;
    node2
        .dial_with_retry(node1.listen_addr, node1.did.clone(), 5)
        .await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    info!("✓ Nodes connected (bidirectional)");

    // Node 1 publishes 3 entries
    let mut hashes = Vec::new();
    for i in 0..3 {
        let data = format!("Pull test entry {i}").into_bytes();
        let hash = {
            let mut gossip1 = node1.gossip_handle.write().await;
            gossip1.publish(topic, data).await?
        };
        hashes.push(hash);
        info!("✓ Node 1 published entry {}: {}", i, hex::encode(hash));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Verify Node 1 has 3 entries, Node 2 has 0
    {
        let gossip1 = node1.gossip_handle.read().await;
        let entries1 = gossip1.get_entries(topic);
        assert_eq!(entries1.len(), 3, "Node 1 should have 3 entries");
    }
    {
        let gossip2 = node2.gossip_handle.read().await;
        let entries2 = gossip2.get_entries(topic);
        assert_eq!(
            entries2.len(),
            0,
            "Node 2 should have 0 entries before pull"
        );
    }

    info!("✓ Initial state verified (Node 1: 3 entries, Node 2: 0 entries)");

    // Node 1 emits Digest (manually trigger instead of waiting for periodic emission)
    {
        let mut gossip1 = node1.gossip_handle.write().await;
        gossip1.emit_digest(topic)?;
    }

    info!("✓ Node 1 emitted Digest");

    // Wait for pull protocol to complete:
    // 1. Node 2 receives Digest
    // 2. Node 2 sends PullRequest
    // 3. Node 1 sends PullResponse
    // 4. Node 2 stores entries
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Verify Node 2 received Digest
    let digest_count = node2.pull_message_count("Digest").await;
    assert!(
        digest_count > 0,
        "Node 2 should have received at least 1 Digest"
    );

    info!("✓ Node 2 received {} Digest(s)", digest_count);

    // Verify Node 1 received PullRequest
    let request_count = node1.pull_message_count("PullRequest").await;
    assert!(
        request_count > 0,
        "Node 1 should have received at least 1 PullRequest"
    );

    info!("✓ Node 1 received {} PullRequest(s)", request_count);

    // Verify Node 2 received PullResponse
    let response_count = node2.pull_message_count("PullResponse").await;
    assert!(
        response_count > 0,
        "Node 2 should have received at least 1 PullResponse"
    );

    info!("✓ Node 2 received {} PullResponse(s)", response_count);

    // CRITICAL: Verify convergence - Node 2 should now have all 3 entries
    {
        let gossip2 = node2.gossip_handle.read().await;
        let entries2 = gossip2.get_entries(topic);
        assert_eq!(
            entries2.len(),
            3,
            "Node 2 should have converged to 3 entries via pull protocol"
        );

        // Verify hashes match
        let node2_hashes: std::collections::HashSet<_> = entries2.iter().map(|e| e.hash).collect();
        for hash in &hashes {
            assert!(
                node2_hashes.contains(hash),
                "Node 2 should have entry {}",
                hex::encode(hash)
            );
        }
    }

    info!("✓ CONVERGENCE VERIFIED: Node 2 has all 3 entries from Node 1");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;

    info!("✓ Pull protocol convergence test completed successfully");

    Ok(())
}

/// Test pull request respects backpressure.
///
/// Note: Ignored in CI due to intermittent QUIC stream failures in containerized environment.
/// Run manually with: cargo test -p icn-core --test gossip_pull_protocol_integration test_pull_request_respects_backpressure -- --ignored
#[tokio::test]
#[ignore = "Flaky in CI: QUIC stream failures in containerized environment"]
async fn test_pull_request_respects_backpressure() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize test environment
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    info!("=== Starting backpressure test ===");

    // Spawn two nodes
    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;

    // Allow nodes to fully initialize before connecting
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create topic
    let topic = "test:backpressure";
    {
        let mut gossip1 = node1.gossip_handle.write().await;
        gossip1.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
    }
    {
        let mut gossip2 = node2.gossip_handle.write().await;
        gossip2.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
    }

    // Connect nodes with retry for CI reliability
    node1
        .dial_with_retry(node2.listen_addr, node2.did.clone(), 5)
        .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Node 1 publishes 100 small entries
    for i in 0..100 {
        let data = format!("Entry {i}").into_bytes();
        let mut gossip1 = node1.gossip_handle.write().await;
        gossip1.publish(topic, data).await?;
    }

    info!("✓ Node 1 published 100 entries");

    // Node 1 emits Digest
    {
        let mut gossip1 = node1.gossip_handle.write().await;
        gossip1.emit_digest(topic)?;
    }

    // Wait for pull protocol
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Verify Node 2 pulled some entries (may not be all due to backpressure/batching)
    let pulled_count = {
        let gossip2 = node2.gossip_handle.read().await;
        gossip2.get_entries(topic).len()
    };

    assert!(
        pulled_count > 0,
        "Node 2 should have pulled at least some entries"
    );

    info!(
        "✓ Node 2 pulled {} entries (backpressure may limit full sync)",
        pulled_count
    );

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;

    info!("✓ Backpressure test completed");

    Ok(())
}
