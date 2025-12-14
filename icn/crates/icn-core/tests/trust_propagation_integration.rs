//! Integration test for trust attestation propagation
//!
//! This test validates end-to-end trust propagation over the gossip network:
//! - Alice creates a trust edge for Bob
//! - Alice broadcasts a signed trust attestation
//! - Bob receives and verifies the attestation
//! - Bob applies the trust edge to his local graph
//! - Both nodes can query the distributed trust state

use anyhow::Result;
use icn_core::trust_propagation::{broadcast_trust_attestation, handle_trust_attestation_entry};
use icn_gossip::GossipActor;
use icn_identity::{IdentityBundle, KeyPair};
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor, NetworkMessage};
use icn_store::SledStore;
use icn_trust::{TrustAttestation, TrustEdge, TrustGraph};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Helper to create a test node with trust graph
struct TestNode {
    keypair: KeyPair,
    did: icn_identity::Did,
    network_handle: icn_net::NetworkHandle,
    gossip_handle: Arc<RwLock<GossipActor>>,
    trust_graph: Arc<RwLock<TrustGraph>>,
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

        // Create trust graph
        let trust_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::temporary()?);
        let trust_graph = TrustGraph::new(trust_store, did.clone());
        let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

        // Create trust lookup closure for gossip actor
        let trust_graph_for_lookup = trust_graph_handle.clone();
        let trust_lookup = Arc::new(move |peer_did: &icn_identity::Did| {
            // Use a blocking task since we're in a sync context
            let graph = trust_graph_for_lookup.clone();
            let peer = peer_did.clone();
            tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let graph = graph.read().await;
                    graph.trust_class(&peer).ok()
                })
            })
        });

        // Spawn gossip actor
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        info!("Gossip actor spawned");

        // Set up notification callback for trust attestations
        {
            let mut gossip = gossip_handle.write().await;
            let trust_graph_for_notifications = trust_graph_handle.clone();
            let own_did_for_notifications = did.clone();

            let notification_callback: icn_gossip::EntryNotificationCallback =
                Arc::new(move |topic, entry, _subscriber_did| {
                    if topic == icn_core::TRUST_ATTESTATIONS_TOPIC {
                        let trust_graph = trust_graph_for_notifications.clone();
                        let own_did = own_did_for_notifications.clone();

                        tokio::spawn(async move {
                            if let Err(e) = handle_trust_attestation_entry(
                                &entry,
                                &trust_graph,
                                &own_did,
                                None, // No rate limiting in tests
                            )
                            .await
                            {
                                warn!("Failed to handle trust attestation: {}", e);
                            }
                        });
                    }
                });

            gossip.set_notification_callback(notification_callback);

            // Subscribe to trust attestations topic
            gossip
                .subscribe(icn_core::TRUST_ATTESTATIONS_TOPIC, did.clone())
                .expect("Failed to subscribe to trust attestations");

            info!("Subscribed to trust:attestations topic");
        }

        // Create incoming message handler that routes to gossip
        let gossip_handle_clone = gossip_handle.clone();
        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                let sender = net_msg.from.clone();
                let gossip_clone = gossip_handle_clone.clone();
                tokio::spawn(async move {
                    let mut gossip = gossip_clone.write().await;
                    if let Err(e) = gossip.handle_message(&sender, gossip_msg) {
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
            None, // No trust graph for rate limiting in tests
            None, // No trust-gated config for tests
            None, // No fallback config for tests
            None, // No topology config
            None, // No STUN servers
            None, // No TURN config
            None, // No misbehavior detector for tests
        )
        .await?;

        // Wire up gossip send callback
        {
            let mut gossip = gossip_handle.write().await;
            let network_handle_clone = network_handle.clone();
            let own_did_clone = did.clone();

            let send_callback: icn_gossip::SendMessageCallback =
                Arc::new(move |recipient, gossip_msg| {
                    let net_handle = network_handle_clone.clone();
                    let from_did = own_did_clone.clone();

                    tokio::spawn(async move {
                        let result = if let Some(target_did) = recipient {
                            let net_msg = NetworkMessage::gossip(
                                from_did,
                                Some(target_did.clone()),
                                gossip_msg,
                            );
                            net_handle.send_message(target_did, net_msg).await
                        } else {
                            let net_msg = NetworkMessage::gossip(from_did, None, gossip_msg);
                            net_handle.broadcast(net_msg).await
                        };

                        if let Err(e) = result {
                            warn!("Failed to send gossip message: {}", e);
                        }
                    });
                });

            gossip.set_send_callback(send_callback);
        }

        info!("Network actor spawned on {}", listen_addr);

        Ok(TestNode {
            keypair,
            did,
            network_handle,
            gossip_handle,
            trust_graph: trust_graph_handle,
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
#[ignore] // Requires network interfaces and QUIC handshake
async fn test_trust_attestation_propagation() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing for test debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_test_writer()
        .try_init();

    info!("=== Starting trust attestation propagation test ===");

    // Spawn two nodes
    let alice = TestNode::spawn(15100).await?;
    let bob = TestNode::spawn(15101).await?;

    info!("Alice DID: {}", alice.did);
    info!("Bob DID: {}", bob.did);

    // Give nodes time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Alice dials Bob
    info!("Alice dialing Bob...");
    alice
        .network_handle
        .dial(bob.listen_addr, bob.did.clone())
        .await?;

    info!("Connection established");

    // Give connection time to stabilize
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Alice creates a trust edge for Bob with score 0.75
    info!("Alice creating trust edge for Bob...");
    let edge = TrustEdge::new(alice.did.clone(), bob.did.clone(), 0.75)
        .with_label("test-partner")
        .with_evidence("integration-test");

    // Add edge locally to Alice's graph
    {
        let mut alice_graph = alice.trust_graph.write().await;
        alice_graph.add_edge(edge.clone())?;
    }

    info!("Alice added trust edge locally");

    // Create and broadcast attestation
    let mut attestation = TrustAttestation::from_trust_edge(&edge);
    broadcast_trust_attestation(&mut attestation, &alice.keypair, &alice.gossip_handle).await?;

    info!("Alice broadcasted trust attestation");

    // Wait for propagation
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify Bob received and applied the attestation
    let bob_has_edge = {
        let bob_graph = bob.trust_graph.read().await;
        bob_graph.get_edge(&alice.did, &bob.did)?
    };

    assert!(
        bob_has_edge.is_some(),
        "Bob should have received Alice's trust edge"
    );

    let bob_edge = bob_has_edge.unwrap();
    assert_eq!(bob_edge.score, 0.75, "Trust score should match");
    assert_eq!(bob_edge.labels, vec!["test-partner"], "Labels should match");

    info!("✓ Bob successfully received and applied Alice's trust attestation");

    // Verify Bob can query his trust in Alice
    let trust_score = {
        let bob_graph = bob.trust_graph.read().await;
        bob_graph.compute_trust_score(&alice.did)?
    };

    info!("Bob's computed trust score for Alice: {}", trust_score);

    // Clean up
    alice.shutdown().await;
    bob.shutdown().await;

    info!("=== Trust attestation propagation test complete ===");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires network interfaces and QUIC handshake
async fn test_three_node_transitive_trust() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_test_writer()
        .try_init();

    info!("=== Starting three-node transitive trust test ===");

    // Spawn three nodes
    let alice = TestNode::spawn(15200).await?;
    let bob = TestNode::spawn(15201).await?;
    let carol = TestNode::spawn(15202).await?;

    info!("Alice DID: {}", alice.did);
    info!("Bob DID: {}", bob.did);
    info!("Carol DID: {}", carol.did);

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Establish connections: Alice <-> Bob <-> Carol
    info!("Establishing connections...");
    alice
        .network_handle
        .dial(bob.listen_addr, bob.did.clone())
        .await?;
    bob.network_handle
        .dial(carol.listen_addr, carol.did.clone())
        .await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Alice trusts Bob (0.8)
    info!("Alice trusting Bob...");
    let edge_alice_bob = TrustEdge::new(alice.did.clone(), bob.did.clone(), 0.8);
    {
        let mut alice_graph = alice.trust_graph.write().await;
        alice_graph.add_edge(edge_alice_bob.clone())?;
    }
    let mut attestation_alice_bob = TrustAttestation::from_trust_edge(&edge_alice_bob);
    broadcast_trust_attestation(
        &mut attestation_alice_bob,
        &alice.keypair,
        &alice.gossip_handle,
    )
    .await?;

    // Bob trusts Carol (0.6)
    info!("Bob trusting Carol...");
    let edge_bob_carol = TrustEdge::new(bob.did.clone(), carol.did.clone(), 0.6);
    {
        let mut bob_graph = bob.trust_graph.write().await;
        bob_graph.add_edge(edge_bob_carol.clone())?;
    }
    let mut attestation_bob_carol = TrustAttestation::from_trust_edge(&edge_bob_carol);
    broadcast_trust_attestation(&mut attestation_bob_carol, &bob.keypair, &bob.gossip_handle)
        .await?;

    // Wait for propagation
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Verify Bob received Alice's trust
    let bob_has_alice_edge = {
        let bob_graph = bob.trust_graph.read().await;
        bob_graph.get_edge(&alice.did, &bob.did)?
    };
    assert!(
        bob_has_alice_edge.is_some(),
        "Bob should have Alice's trust edge"
    );

    // Verify Carol received Bob's trust
    let carol_has_bob_edge = {
        let carol_graph = carol.trust_graph.read().await;
        carol_graph.get_edge(&bob.did, &carol.did)?
    };
    assert!(
        carol_has_bob_edge.is_some(),
        "Carol should have Bob's trust edge"
    );

    info!("✓ All trust attestations propagated successfully");

    // Verify Alice can compute transitive trust to Carol
    // Expected: 0 * 0.7 (no direct) + (0.8 * 0.6) * 0.3 (transitive) = 0.144
    {
        let mut alice_graph = alice.trust_graph.write().await;

        // Alice needs to add Bob's trust of Carol to compute transitive trust
        // In a real system, Alice would receive this attestation via gossip
        alice_graph.add_edge(edge_bob_carol.clone())?;

        let transitive_score = alice_graph.compute_trust_score(&carol.did)?;
        info!(
            "Alice's transitive trust score for Carol: {}",
            transitive_score
        );

        // Should be approximately 0.144 (0.8 * 0.6 * 0.3)
        assert!(
            (0.14..=0.15).contains(&transitive_score),
            "Transitive trust score should be ~0.144, got {transitive_score}"
        );
    }

    info!("✓ Transitive trust computation works correctly");

    // Clean up
    alice.shutdown().await;
    bob.shutdown().await;
    carol.shutdown().await;

    info!("=== Three-node transitive trust test complete ===");

    Ok(())
}
