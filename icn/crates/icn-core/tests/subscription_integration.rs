//! Integration test for topic subscriptions over the network
//!
//! This test validates that:
//! 1. Nodes can send Subscribe messages to peers
//! 2. Peers process Subscribe messages and add subscribers
//! 3. SubscribeAck messages are sent back
//! 4. Nodes can unsubscribe from topics

use anyhow::Result;
use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor, NetworkMessage};
use icn_trust::TrustClass;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Helper to create a test node with subscription handling
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
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        info!("Gossip actor spawned");

        // Create incoming message handler that processes both Gossip and Subscribe messages
        let gossip_handle_clone = gossip_handle.clone();
        let network_handle_holder = Arc::new(RwLock::new(None::<icn_net::NetworkHandle>));
        let network_handle_holder_clone = network_handle_holder.clone();
        let own_did_clone = did.clone();

        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            let sender_did = net_msg.from.clone();

            match net_msg.payload {
                MessagePayload::Gossip(gossip_msg) => {
                    let mut gossip = gossip_handle_clone.blocking_write();
                    if let Err(e) = gossip.handle_message(&sender_did, gossip_msg) {
                        warn!("Failed to handle gossip message: {}", e);
                    }
                }

                MessagePayload::Subscribe { topics } => {
                    info!("Received Subscribe from {} for topics: {:?}", sender_did, topics);

                    let mut gossip = gossip_handle_clone.blocking_write();
                    let mut acked_topics = Vec::new();

                    for topic in &topics {
                        match gossip.subscribe(topic, sender_did.clone()) {
                            Ok(_) => {
                                info!("Subscribed {} to topic: {}", sender_did, topic);
                                acked_topics.push(topic.clone());
                            }
                            Err(e) => {
                                warn!("Failed to subscribe {} to topic {}: {}", sender_did, topic, e);
                            }
                        }
                    }

                    // Send SubscribeAck back
                    if !acked_topics.is_empty() {
                        let network_handle_lock = network_handle_holder_clone.clone();
                        let own_did = own_did_clone.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            if let Some(net_handle) = network_handle_lock.read().await.as_ref() {
                                let ack_msg = NetworkMessage::subscribe_ack(
                                    own_did,
                                    sender_did.clone(),
                                    acked_topics.clone()
                                );
                                if let Err(e) = net_handle.send_message(sender_did, ack_msg).await {
                                    warn!("Failed to send SubscribeAck: {}", e);
                                }
                            }
                        });
                    }
                }

                MessagePayload::Unsubscribe { topics } => {
                    info!("Received Unsubscribe from {} for topics: {:?}", sender_did, topics);

                    let mut gossip = gossip_handle_clone.blocking_write();
                    for topic in &topics {
                        match gossip.unsubscribe(topic, &sender_did) {
                            Ok(_) => {
                                info!("Unsubscribed {} from topic: {}", sender_did, topic);
                            }
                            Err(e) => {
                                warn!("Failed to unsubscribe {} from topic {}: {}", sender_did, topic, e);
                            }
                        }
                    }
                }

                MessagePayload::SubscribeAck { topics } => {
                    info!("Received SubscribeAck from {} for topics: {:?}", sender_did, topics);
                }

                _ => {}
            }
        });

        // Spawn network actor
        let listen_addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
        let network_handle = NetworkActor::spawn(
            &keypair,
            listen_addr,
            shutdown_tx.clone(),
            Some(incoming_handler),
            None, // No trust graph for tests
            None, // No trust-gated config for tests
            None, // No fallback config for tests
            None, // No topology config

            None, // No topology config        )
        .await?;

        // Initialize network handle for the message handler
        *network_handle_holder.write().await = Some(network_handle.clone());

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
#[ignore] // Requires network interfaces and QUIC handshake
async fn test_subscription_end_to_end() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize test environment
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    // Spawn two nodes
    let node1 = TestNode::spawn(15001).await?;
    let node2 = TestNode::spawn(15002).await?;

    info!("Node 1 DID: {}", node1.did);
    info!("Node 2 DID: {}", node2.did);

    // Node 1 dials Node 2
    node1.network_handle.dial(node2.listen_addr, node2.did.clone()).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Node 1 sends Subscribe message to Node 2
    let subscribe_msg = NetworkMessage::subscribe(
        node1.did.clone(),
        node2.did.clone(),
        vec!["global:identity".to_string(), "global:rendezvous".to_string()],
    );

    node1.network_handle.send_message(node2.did.clone(), subscribe_msg).await?;

    // Wait for subscription processing and ack
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify that Node 2 has Node 1 as a subscriber
    {
        let gossip2 = node2.gossip_handle.read().await;
        let subscribers_identity = gossip2.get_subscribers("global:identity");
        let subscribers_rendezvous = gossip2.get_subscribers("global:rendezvous");

        assert!(subscribers_identity.contains(&node1.did), "Node 1 should be subscribed to global:identity");
        assert!(subscribers_rendezvous.contains(&node1.did), "Node 1 should be subscribed to global:rendezvous");

        info!("✓ Node 1 successfully subscribed to topics on Node 2");
    }

    // Node 1 sends Unsubscribe message for one topic
    let unsubscribe_msg = NetworkMessage::unsubscribe(
        node1.did.clone(),
        node2.did.clone(),
        vec!["global:identity".to_string()],
    );

    node1.network_handle.send_message(node2.did.clone(), unsubscribe_msg).await?;

    // Wait for unsubscribe processing
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Verify that Node 1 is unsubscribed from global:identity but still subscribed to global:rendezvous
    {
        let gossip2 = node2.gossip_handle.read().await;
        let subscribers_identity = gossip2.get_subscribers("global:identity");
        let subscribers_rendezvous = gossip2.get_subscribers("global:rendezvous");

        assert!(!subscribers_identity.contains(&node1.did), "Node 1 should be unsubscribed from global:identity");
        assert!(subscribers_rendezvous.contains(&node1.did), "Node 1 should still be subscribed to global:rendezvous");

        info!("✓ Node 1 successfully unsubscribed from global:identity");
    }

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;

    info!("✓ Test completed successfully");

    Ok(())
}

#[tokio::test]
async fn test_subscription_acl_enforcement() -> Result<()> {
    // Initialize test environment
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .try_init();

    // Spawn node with restrictive trust lookup (denies all)
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();
    let (shutdown_tx, _) = tokio::sync::broadcast::channel(16);

    // Trust lookup that returns None (no trust)
    let trust_lookup = Arc::new(|_did: &icn_identity::Did| None);
    let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

    // Create a topic with TrustClass requirement
    {
        let mut gossip = gossip_handle.write().await;
        let topic = icn_gossip::Topic::new(
            "partner:only".to_string(),
            icn_gossip::AccessControl::TrustClass(TrustClass::Partner),
        );
        gossip.create_topic(topic);
    }

    // Try to subscribe with no trust - should fail
    let other_keypair = KeyPair::generate()?;
    let other_did = other_keypair.did().clone();

    {
        let mut gossip = gossip_handle.write().await;
        let result = gossip.subscribe("partner:only", other_did.clone());
        assert!(result.is_err(), "Should not be able to subscribe to partner:only without trust");
        info!("✓ ACL enforcement working correctly");
    }

    let _ = shutdown_tx.send(());

    Ok(())
}
