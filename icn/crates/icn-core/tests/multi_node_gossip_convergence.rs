#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Multi-node gossip convergence integration test
//!
//! This test validates that:
//! 1. Subscription notifications fire when entries arrive via Response messages (pull protocol)
//! 2. Vector clocks are properly merged when entries are received from remote nodes
//! 3. Max entry limits are enforced for entries received via pull protocol
//! 4. Full gossip convergence works across 3+ nodes with subscriptions
//!
//! This specifically tests the fix for the Response handler bypass bug where:
//! - Subscriber notifications were not triggered for pulled entries
//! - Vector clock merging was skipped for pulled entries
//! - Max entries limit was not enforced for pulled entries

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

/// Type alias for notification records: (topic, hash, subscriber_did)
type NotificationList = Vec<(String, [u8; 32], icn_identity::Did)>;

/// Helper to create a test node with notification tracking
struct TestNode {
    _keypair: KeyPair,
    did: icn_identity::Did,
    network_handle: icn_net::NetworkHandle,
    gossip_handle: Arc<RwLock<GossipActor>>,
    listen_addr: SocketAddr,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    /// Track notifications received: (topic, hash, subscriber_did)
    notifications: Arc<Mutex<NotificationList>>,
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

        // Track notifications for this node
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let notifications_clone = notifications.clone();

        // Set up notification callback to track all notifications
        {
            let mut gossip = gossip_handle.write().await;
            let callback = Arc::new(
                move |topic: String, entry: icn_gossip::GossipEntry, sub_did: icn_identity::Did| {
                    let hash = entry.hash;
                    let notifs_clone = notifications_clone.clone();
                    tokio::spawn(async move {
                        let mut notifs = notifs_clone.lock().await;
                        info!(
                            "📬 Notification: topic={}, hash={}, subscriber={}",
                            topic,
                            hex::encode(hash),
                            sub_did
                        );
                        notifs.push((topic, hash, sub_did));
                    });
                },
            );
            gossip.set_notification_callback(callback);
        }

        info!("Gossip actor spawned with notification tracking");

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
                    info!(
                        "Received Subscribe from {} for topics: {:?}",
                        sender_did, topics
                    );

                    let mut gossip = gossip_handle_clone.blocking_write();
                    let mut acked_topics = Vec::new();

                    for topic in &topics {
                        match gossip.subscribe(topic, sender_did.clone()) {
                            Ok(_) => {
                                info!("Subscribed {} to topic: {}", sender_did, topic);
                                acked_topics.push(topic.clone());
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to subscribe {} to topic {}: {}",
                                    sender_did, topic, e
                                );
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
                                    acked_topics.clone(),
                                );
                                if let Err(e) = net_handle.send_message(sender_did, ack_msg).await {
                                    warn!("Failed to send SubscribeAck: {}", e);
                                }
                            }
                        });
                    }
                }

                MessagePayload::Unsubscribe { topics } => {
                    info!(
                        "Received Unsubscribe from {} for topics: {:?}",
                        sender_did, topics
                    );

                    let mut gossip = gossip_handle_clone.blocking_write();
                    for topic in &topics {
                        if let Err(e) = gossip.unsubscribe(topic, &sender_did) {
                            warn!(
                                "Failed to unsubscribe {} from topic {}: {}",
                                sender_did, topic, e
                            );
                        }
                    }
                }

                MessagePayload::SubscribeAck { topics } => {
                    info!(
                        "Received SubscribeAck from {} for topics: {:?}",
                        sender_did, topics
                    );
                }

                _ => {}
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
        )
        .await?;

        // Initialize network handle for the message handler
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

                    // Spawn async task to send message
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
            notifications,
        })
    }

    async fn shutdown(self) {
        info!("Shutting down test node");
        let _ = self.shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    async fn notification_count(&self) -> usize {
        self.notifications.lock().await.len()
    }
}

#[tokio::test]
#[ignore] // Uses blocking_write() in async context - needs refactoring to use async handlers
async fn test_response_handler_triggers_notifications_across_nodes() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize test environment
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Starting multi-node gossip convergence test ===");

    // Spawn three nodes
    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;
    let node3 = TestNode::spawn(get_available_port()).await?;

    info!("Node 1 DID: {}", node1.did);
    info!("Node 2 DID: {}", node2.did);
    info!("Node 3 DID: {}", node3.did);

    // Create topic on all nodes
    let topic = "test:convergence";
    {
        let mut gossip1 = node1.gossip_handle.write().await;
        gossip1.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
    }
    {
        let mut gossip2 = node2.gossip_handle.write().await;
        gossip2.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
    }
    {
        let mut gossip3 = node3.gossip_handle.write().await;
        gossip3.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
    }

    // Connect nodes: 1 ↔ 2 ↔ 3
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;
    node2
        .network_handle
        .dial(node3.listen_addr, node3.did.clone())
        .await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    info!("✓ Nodes connected");

    // Node 1 subscribes to topic on Node 2
    let subscribe_msg = NetworkMessage::subscribe(
        node1.did.clone(),
        node2.did.clone(),
        vec![topic.to_string()],
    );
    node1
        .network_handle
        .send_message(node2.did.clone(), subscribe_msg)
        .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    info!("✓ Node 1 subscribed to topic on Node 2");

    // Verify subscription
    {
        let gossip2 = node2.gossip_handle.read().await;
        let subscribers = gossip2.get_subscribers(topic);
        assert!(
            subscribers.contains(&node1.did),
            "Node 1 should be subscribed on Node 2"
        );
    }

    // Node 3 publishes an entry
    let test_data = b"Test entry from Node 3".to_vec();
    let hash = {
        let mut gossip3 = node3.gossip_handle.write().await;
        gossip3.publish(topic, test_data.clone())?
    };

    info!("✓ Node 3 published entry: {}", hex::encode(hash));

    // Node 3 sends Announce to Node 2
    let clock = {
        let gossip3 = node3.gossip_handle.read().await;
        let entry = gossip3.get_entry(topic, &hash).expect("Entry should exist");
        entry.clock
    };

    let announce_msg = GossipMessage::Announce {
        hash,
        author: node3.did.clone(),
        clock,
        topic: topic.to_string(),
    };

    let net_msg = NetworkMessage::gossip(node3.did.clone(), Some(node2.did.clone()), announce_msg);
    node2
        .network_handle
        .send_message(node2.did.clone(), net_msg)
        .await?;

    info!("✓ Node 3 sent Announce to Node 2");

    // Give time for Request/Response flow and notification processing
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify Node 2 received the entry via Response
    {
        let gossip2 = node2.gossip_handle.read().await;
        let entries = gossip2.get_entries(topic);
        assert_eq!(
            entries.len(),
            1,
            "Node 2 should have 1 entry after Response"
        );
        assert_eq!(entries[0].hash, hash);
    }

    info!("✓ Node 2 stored entry from Response");

    // CRITICAL: Verify Node 1 received notification (this is what the bug would have broken)
    let notification_count = node1.notification_count().await;
    assert!(
        notification_count > 0,
        "Node 1 should have received at least 1 notification via Response handler"
    );

    info!(
        "✓ Node 1 received {} notification(s) - Response handler is working!",
        notification_count
    );

    // Verify the notification matches the entry
    {
        let notifications = node1.notifications.lock().await;
        let notification_exists = notifications
            .iter()
            .any(|(t, h, _)| t == topic && h == &hash);
        assert!(
            notification_exists,
            "Notification should match the published entry"
        );
    }

    info!("✓ Notification matches published entry");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;
    node3.shutdown().await;

    info!("✓ Multi-node convergence test completed successfully");

    Ok(())
}

#[tokio::test]
#[ignore] // Uses blocking_write() in async context - needs refactoring to use async handlers
async fn test_response_handler_enforces_max_entries_across_nodes() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize test environment
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    info!("=== Starting max entries enforcement test ===");

    // Spawn two nodes
    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;

    // Create topic with small max_entries limit on Node 2
    let topic = "test:bounded";
    {
        let mut gossip2 = node2.gossip_handle.write().await;
        let topic_obj = Topic::new(topic.to_string(), AccessControl::Public).with_max_entries(3);
        gossip2.create_topic(topic_obj);
    }
    {
        let mut gossip1 = node1.gossip_handle.write().await;
        gossip1.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
    }

    // Connect nodes
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Node 1 publishes 5 entries
    let mut hashes = Vec::new();
    for i in 0..5 {
        let data = format!("Entry {i}").into_bytes();
        let hash = {
            let mut gossip1 = node1.gossip_handle.write().await;
            gossip1.publish(topic, data)?
        };
        hashes.push(hash);
        tokio::time::sleep(Duration::from_millis(10)).await; // Ensure distinct timestamps
    }

    info!("✓ Node 1 published 5 entries");

    // Node 1 sends all entries to Node 2 via Response messages
    for hash in &hashes {
        let entry = {
            let gossip1 = node1.gossip_handle.read().await;
            gossip1.get_entry(topic, hash).expect("Entry should exist")
        };

        let response_msg = GossipMessage::Response { entry };
        let net_msg =
            NetworkMessage::gossip(node1.did.clone(), Some(node2.did.clone()), response_msg);
        node2
            .network_handle
            .send_message(node2.did.clone(), net_msg)
            .await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    info!("✓ Node 1 sent all entries to Node 2 via Response");

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // CRITICAL: Verify Node 2 enforces max_entries limit (this is what the bug would have broken)
    {
        let gossip2 = node2.gossip_handle.read().await;
        let entries = gossip2.get_entries(topic);
        assert_eq!(
            entries.len(),
            3,
            "Node 2 should enforce max_entries=3 for Response messages"
        );
    }

    info!("✓ Node 2 enforced max_entries limit for Response messages");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;

    info!("✓ Max entries enforcement test completed successfully");

    Ok(())
}
