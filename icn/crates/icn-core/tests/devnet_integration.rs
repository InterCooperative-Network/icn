#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Devnet integration tests (T3 #1064)
//!
//! These tests verify multi-node gossip convergence, service discovery
//! propagation, and node restart/re-join behavior using lightweight
//! in-process GossipActor instances.
//!
//! Unlike the full network integration tests (which use QUIC connections
//! and are marked `#[ignore]`), these tests connect gossip actors via
//! direct `handle_message()` calls, making them fully deterministic and
//! suitable for CI.
//!
//! The tests manually drive the gossip protocol steps (Announce -> Request ->
//! Response) to avoid dependency on background task scheduling, ensuring
//! complete determinism without timing-dependent assertions.
//!
//! Run with: `cargo test -p icn-core --test devnet_integration`

use icn_gossip::{
    service_discovery_topics, AccessControl, GossipActor, GossipEntry, GossipMessage, Topic,
};
use icn_identity::KeyPair;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};

// ============================================================================
// Test Node Helper
// ============================================================================

/// Notification record: (topic, hash, data)
type NotificationList = Arc<Mutex<Vec<(String, [u8; 32], Vec<u8>)>>>;

/// Outbox record: (recipient, message)
type OutboxList = Arc<Mutex<Vec<(Option<icn_identity::Did>, GossipMessage)>>>;

/// Lightweight test node wrapping a GossipActor with notification tracking.
///
/// This avoids the full NetworkActor/QUIC stack. Gossip messages are routed
/// between nodes manually via `deliver()`, which calls `handle_message()`
/// directly on the node's GossipActor.
struct TestNode {
    did: icn_identity::Did,
    gossip: Arc<RwLock<GossipActor>>,
    /// Notifications received: (topic, hash, data)
    notifications: NotificationList,
    /// Outbound messages captured from send_callback: (recipient, message)
    outbox: OutboxList,
}

impl TestNode {
    /// Create a new test node with a random identity, notification tracking,
    /// and outbox capture.
    async fn new() -> Self {
        let keypair = KeyPair::generate().expect("keygen");
        let did = keypair.did().clone();
        let gossip = GossipActor::spawn(did.clone(), None);

        let notifications: NotificationList = Arc::new(Mutex::new(Vec::new()));
        let notif_clone = notifications.clone();

        let outbox: OutboxList = Arc::new(Mutex::new(Vec::new()));
        let outbox_clone = outbox.clone();

        {
            let mut g = gossip.write().await;

            // Install notification callback to record incoming entries
            let cb = Arc::new(
                move |topic: String, entry: GossipEntry, _sub_did: icn_identity::Did| {
                    let data = entry.data.clone();
                    let hash = entry.hash;
                    let n = notif_clone.clone();
                    tokio::spawn(async move {
                        n.lock().await.push((topic, hash, data));
                    });
                },
            );
            g.add_notification_callback(cb);

            // Install send callback that captures outbound messages
            let send_cb = Arc::new(
                move |recipient: Option<icn_identity::Did>, msg: GossipMessage| {
                    let ob = outbox_clone.clone();
                    tokio::spawn(async move {
                        ob.lock().await.push((recipient, msg));
                    });
                },
            );
            g.set_send_callback(send_cb);
        }

        TestNode {
            did,
            gossip,
            notifications,
            outbox,
        }
    }

    /// Create a topic on this node.
    async fn create_topic(&self, name: &str) {
        let mut g = self.gossip.write().await;
        g.create_topic(Topic::new(name.to_string(), AccessControl::Public));
    }

    /// Subscribe a remote DID to a topic on this node.
    async fn add_subscriber(&self, topic: &str, subscriber: &icn_identity::Did) {
        let mut g = self.gossip.write().await;
        g.subscribe(topic, subscriber.clone())
            .await
            .expect("subscribe");
    }

    /// Publish data to a topic and return the content hash.
    async fn publish(&self, topic: &str, data: Vec<u8>) -> [u8; 32] {
        let mut g = self.gossip.write().await;
        g.publish(topic, data).await.expect("publish")
    }

    /// Get all entries stored for a topic on this node.
    async fn get_entries(&self, topic: &str) -> Vec<GossipEntry> {
        let g = self.gossip.read().await;
        g.get_entries(topic)
    }

    /// Get a specific entry by hash.
    async fn get_entry(&self, topic: &str, hash: &[u8; 32]) -> Option<GossipEntry> {
        let g = self.gossip.read().await;
        g.get_entry(topic, hash)
    }

    /// Deliver a gossip message to this node from a given sender.
    async fn deliver(&self, sender: &icn_identity::Did, msg: GossipMessage) {
        let mut g = self.gossip.write().await;
        g.handle_message(sender, msg).await.expect("handle_message");
    }

    /// Drain all messages from this node's outbox.
    async fn drain_outbox(&self) -> Vec<(Option<icn_identity::Did>, GossipMessage)> {
        // Give spawned tasks a moment to enqueue messages
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        let mut ob = self.outbox.lock().await;
        ob.drain(..).collect()
    }

    /// Return the number of notifications received so far.
    async fn notification_count(&self) -> usize {
        // Give spawned tasks a moment to enqueue notifications
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        self.notifications.lock().await.len()
    }

    /// Get subscribers for a topic.
    async fn get_subscribers(&self, topic: &str) -> Vec<icn_identity::Did> {
        let g = self.gossip.read().await;
        g.get_subscribers(topic)
    }
}

/// Route messages from one node's outbox to the appropriate target nodes.
///
/// Returns the number of messages delivered.
async fn route_messages(from: &TestNode, nodes: &[&TestNode]) -> usize {
    let messages = from.drain_outbox().await;
    let count = messages.len();

    for (recipient, msg) in messages {
        match recipient {
            Some(ref to_did) => {
                // Targeted delivery
                for node in nodes {
                    if node.did == *to_did {
                        node.deliver(&from.did, msg.clone()).await;
                        break;
                    }
                }
            }
            None => {
                // Broadcast to all other nodes
                for node in nodes {
                    if node.did != from.did {
                        node.deliver(&from.did, msg.clone()).await;
                    }
                }
            }
        }
    }

    count
}

/// Drain and route all pending messages across all nodes until no more
/// messages are in flight (quiescence). Returns total messages routed.
///
/// This simulates a full round of gossip protocol exchange.
/// Maximum of 20 rounds to prevent infinite loops.
async fn route_until_quiescent(nodes: &[&TestNode]) -> usize {
    let mut total = 0;
    for _round in 0..20 {
        let mut round_count = 0;
        for node in nodes {
            round_count += route_messages(node, nodes).await;
        }
        total += round_count;
        if round_count == 0 {
            break; // Quiescent
        }
    }
    total
}

// ============================================================================
// Test 1: Three-Node Gossip Convergence
// ============================================================================

/// Verify that a message published on node A reaches nodes B and C through
/// the Announce -> Request -> Response gossip protocol flow.
///
/// Protocol chain:
/// 1. A publishes locally, then we send Announce to B and C
/// 2. B and C receive Announce, send Request back to A (via outbox)
/// 3. We route those Requests to A
/// 4. A sends Response to B and C (via outbox)
/// 5. We route those Responses to B and C
/// 6. B and C store the entry
#[tokio::test]
async fn test_three_node_gossip_convergence() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let topic = "devnet:convergence";

    // Create three nodes with the topic
    let node_a = TestNode::new().await;
    let node_b = TestNode::new().await;
    let node_c = TestNode::new().await;

    node_a.create_topic(topic).await;
    node_b.create_topic(topic).await;
    node_c.create_topic(topic).await;

    // B and C subscribe on A (so A's notification callback fires for them)
    node_a.add_subscriber(topic, &node_b.did).await;
    node_a.add_subscriber(topic, &node_c.did).await;

    // Verify subscriptions
    let subs = node_a.get_subscribers(topic).await;
    assert!(subs.contains(&node_b.did), "B should be subscribed on A");
    assert!(subs.contains(&node_c.did), "C should be subscribed on A");

    // Node A publishes a message
    let test_data = b"Hello from node A".to_vec();
    let hash = node_a.publish(topic, test_data.clone()).await;

    // Verify A has the entry locally
    assert!(
        node_a.get_entry(topic, &hash).await.is_some(),
        "A should have the entry locally after publish"
    );

    // Build Announce message and deliver to B and C (simulating gossip push)
    let entry = node_a.get_entry(topic, &hash).await.unwrap();
    let announce = GossipMessage::Announce {
        hash,
        author: node_a.did.clone(),
        clock: entry.clock.clone(),
        topic: topic.to_string(),
    };

    node_b.deliver(&node_a.did, announce.clone()).await;
    node_c.deliver(&node_a.did, announce).await;

    // B and C should have sent Request messages to the author (A)
    // Route all messages until quiescent (Request -> Response chain)
    let all_nodes: Vec<&TestNode> = vec![&node_a, &node_b, &node_c];
    let total_routed = route_until_quiescent(&all_nodes).await;

    assert!(
        total_routed >= 4,
        "Expected at least 4 messages (2 Requests + 2 Responses), got {}",
        total_routed
    );

    // Verify both B and C have the entry
    let b_entry = node_b
        .get_entry(topic, &hash)
        .await
        .expect("B should have the entry after convergence");
    let c_entry = node_c
        .get_entry(topic, &hash)
        .await
        .expect("C should have the entry after convergence");

    assert_eq!(b_entry.data, test_data, "B's entry data should match");
    assert_eq!(c_entry.data, test_data, "C's entry data should match");
    assert_eq!(b_entry.author, node_a.did, "B's entry author should be A");
    assert_eq!(c_entry.author, node_a.did, "C's entry author should be A");

    // Verify notifications fired on A (from store_entry when publishing).
    // A has subscribers B and C, so store_entry fires notifications for them.
    let a_notifications = node_a.notification_count().await;
    assert!(
        a_notifications >= 2,
        "A should have fired at least 2 notifications (for B and C), got {}",
        a_notifications
    );
}

// ============================================================================
// Test 2: Service Announcement Propagation
// ============================================================================

/// Verify that a service announcement published on one node propagates to
/// a subscribing peer via the gossip Announce -> Request -> Response chain.
///
/// This test validates the service discovery gossip path:
/// 1. Node A publishes a serialized ServiceDiscoveryMessage to services:announce
/// 2. An Announce is delivered to Node B
/// 3. After the Request/Response chain, Node B has the entry
/// 4. Node B deserializes and verifies the service details
#[tokio::test]
async fn test_service_announcement_propagation() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let topic = service_discovery_topics::SERVICES_ANNOUNCE;

    let node_a = TestNode::new().await;
    let node_b = TestNode::new().await;

    node_a.create_topic(topic).await;
    node_b.create_topic(topic).await;

    // B subscribes on A
    node_a.add_subscriber(topic, &node_b.did).await;

    // Construct a service announcement payload
    let service_id = "svc-ledger-01";
    let announcement = icn_gossip::ServiceDiscoveryMessage::Announce {
        endpoint: icn_kernel_api::naming::ServiceEndpoint {
            service_id: service_id.to_string(),
            provider: node_a.did.to_string(),
            endpoint_type: icn_kernel_api::naming::EndpointType::Http,
            service_type: icn_kernel_api::naming::ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![icn_kernel_api::types::Endpoint::new(
                "https",
                "node-a.example.com",
                8443,
            )],
            addresses: vec![],
            capabilities: vec!["read".to_string(), "write".to_string()],
            trust_threshold: 0.1,
            scope_visibility: icn_kernel_api::scope::ScopeLevel::Org,
            cell_id: None,
            ttl_secs: 3600,
            signature: icn_kernel_api::types::Signature::new(vec![0; 64]),
            created_at: 1700000000,
            updated_at: 1700000000,
        },
    };

    // Serialize and publish on A
    let encoded = icn_encoding::encode(&announcement).expect("encode announcement");
    let hash = node_a.publish(topic, encoded.clone()).await;

    // Build Announce and deliver to B
    let entry = node_a.get_entry(topic, &hash).await.unwrap();
    let announce_msg = GossipMessage::Announce {
        hash,
        author: node_a.did.clone(),
        clock: entry.clock.clone(),
        topic: topic.to_string(),
    };
    node_b.deliver(&node_a.did, announce_msg).await;

    // Route Request/Response chain
    let all_nodes: Vec<&TestNode> = vec![&node_a, &node_b];
    route_until_quiescent(&all_nodes).await;

    // Verify B has the entry
    let b_entry = node_b
        .get_entry(topic, &hash)
        .await
        .expect("B should have the service announcement entry");

    // Deserialize on B and verify service details
    let decoded: icn_gossip::ServiceDiscoveryMessage =
        icn_encoding::decode(&b_entry.data).expect("decode announcement");

    match decoded {
        icn_gossip::ServiceDiscoveryMessage::Announce { endpoint } => {
            assert_eq!(endpoint.service_id, service_id);
            assert_eq!(endpoint.provider, node_a.did.to_string());
            assert_eq!(endpoint.service_type.name, "ledger");
            assert_eq!(
                endpoint.capabilities,
                vec!["read".to_string(), "write".to_string()]
            );
        }
        other => panic!("Expected Announce, got {:?}", other),
    }
}

// ============================================================================
// Test 3: Node Restart and Re-join
// ============================================================================

/// Verify that after a node "restarts" (drops its GossipActor and creates a
/// fresh one), it can receive messages published after the restart, and does
/// NOT have messages from before the restart.
///
/// Steps:
/// 1. Create nodes A and B, A publishes M1, deliver to B
/// 2. "Restart" B: create a new TestNode B' with a fresh GossipActor
/// 3. A publishes M2, deliver to B'
/// 4. Verify B' has M2 but NOT M1
#[tokio::test]
async fn test_node_restart_and_rejoin() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let topic = "devnet:restart";

    // Phase 1: Create two nodes, publish M1
    let node_a = TestNode::new().await;
    let node_b_v1 = TestNode::new().await;

    node_a.create_topic(topic).await;
    node_b_v1.create_topic(topic).await;

    // Publish M1 on A
    let m1_data = b"message before restart".to_vec();
    let m1_hash = node_a.publish(topic, m1_data.clone()).await;

    // Deliver M1 to B via Announce -> Request -> Response
    let m1_entry = node_a.get_entry(topic, &m1_hash).await.unwrap();
    let announce_m1 = GossipMessage::Announce {
        hash: m1_hash,
        author: node_a.did.clone(),
        clock: m1_entry.clock.clone(),
        topic: topic.to_string(),
    };
    node_b_v1.deliver(&node_a.did, announce_m1).await;

    let nodes_v1: Vec<&TestNode> = vec![&node_a, &node_b_v1];
    route_until_quiescent(&nodes_v1).await;

    // Verify B has M1
    assert!(
        node_b_v1.get_entry(topic, &m1_hash).await.is_some(),
        "B should have M1 before restart"
    );

    // Phase 2: "Restart" node B by creating a new node
    // (The old B is simply dropped -- in a real system the actor would be stopped)
    let node_b_v2 = TestNode::new().await;
    node_b_v2.create_topic(topic).await;

    // Phase 3: A publishes M2 after B restarted
    let m2_data = b"message after restart".to_vec();
    let m2_hash = node_a.publish(topic, m2_data.clone()).await;

    // Deliver M2 to new B' via Announce -> Request -> Response
    let m2_entry = node_a.get_entry(topic, &m2_hash).await.unwrap();
    let announce_m2 = GossipMessage::Announce {
        hash: m2_hash,
        author: node_a.did.clone(),
        clock: m2_entry.clock.clone(),
        topic: topic.to_string(),
    };
    node_b_v2.deliver(&node_a.did, announce_m2).await;

    let nodes_v2: Vec<&TestNode> = vec![&node_a, &node_b_v2];
    route_until_quiescent(&nodes_v2).await;

    // Verify B' has M2 with correct data
    let entry = node_b_v2
        .get_entry(topic, &m2_hash)
        .await
        .expect("B' should have M2");
    assert_eq!(entry.data, m2_data);

    // Verify B' does NOT have M1 (it was published before the restart)
    assert!(
        node_b_v2.get_entry(topic, &m1_hash).await.is_none(),
        "Restarted node should NOT have messages from before its restart"
    );
}

// ============================================================================
// Test 4: Vector Clock Merge Across Nodes
// ============================================================================

/// Verify that entries from independent publishers have concurrent vector
/// clocks, and that a receiving node correctly merges them.
#[tokio::test]
async fn test_vector_clock_merge_across_nodes() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let topic = "devnet:vclock";

    let node_a = TestNode::new().await;
    let node_b = TestNode::new().await;
    let node_c = TestNode::new().await;

    node_a.create_topic(topic).await;
    node_b.create_topic(topic).await;
    node_c.create_topic(topic).await;

    // A publishes entry 1
    let hash_a = node_a.publish(topic, b"from A".to_vec()).await;

    // B publishes entry 2 (independently, no knowledge of A's entry)
    let hash_b = node_b.publish(topic, b"from B".to_vec()).await;

    // Deliver both entries to C via Response messages (direct push)
    let entry_a = node_a.get_entry(topic, &hash_a).await.unwrap();
    let entry_b = node_b.get_entry(topic, &hash_b).await.unwrap();

    node_c
        .deliver(
            &node_a.did,
            GossipMessage::Response {
                entry: entry_a.clone(),
            },
        )
        .await;
    node_c
        .deliver(
            &node_b.did,
            GossipMessage::Response {
                entry: entry_b.clone(),
            },
        )
        .await;

    // Verify both entries are stored on C
    let c_entry_a = node_c
        .get_entry(topic, &hash_a)
        .await
        .expect("C should have A's entry");
    let c_entry_b = node_c
        .get_entry(topic, &hash_b)
        .await
        .expect("C should have B's entry");

    assert_eq!(c_entry_a.author, node_a.did);
    assert_eq!(c_entry_b.author, node_b.did);

    // The two entries have independent vector clocks (concurrent)
    assert!(
        c_entry_a.clock.is_concurrent(&c_entry_b.clock),
        "Entries from independent publishers should have concurrent vector clocks"
    );

    // Verify C's local vector clock has been merged from both
    {
        let g = node_c.gossip.read().await;
        let clock = g.get_clock();
        // C's clock should contain components from both A and B
        assert!(
            clock.get(&node_a.did) >= 1,
            "C's clock should track A's contributions"
        );
        assert!(
            clock.get(&node_b.did) >= 1,
            "C's clock should track B's contributions"
        );
    }
}

// ============================================================================
// Test 5: Multiple Topics Isolation
// ============================================================================

/// Verify that messages published on one topic do not leak to a different
/// topic, even when both topics exist on the same node.
#[tokio::test]
async fn test_topic_isolation_across_nodes() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let topic_x = "devnet:topic-x";
    let topic_y = "devnet:topic-y";

    let node_a = TestNode::new().await;
    let node_b = TestNode::new().await;

    node_a.create_topic(topic_x).await;
    node_a.create_topic(topic_y).await;
    node_b.create_topic(topic_x).await;
    node_b.create_topic(topic_y).await;

    // A publishes to topic_x
    let hash_x = node_a.publish(topic_x, b"topic X data".to_vec()).await;

    // A publishes to topic_y
    let hash_y = node_a.publish(topic_y, b"topic Y data".to_vec()).await;

    // Deliver only topic_x entry to B via Announce -> Request -> Response
    let entry_x = node_a.get_entry(topic_x, &hash_x).await.unwrap();
    let announce_x = GossipMessage::Announce {
        hash: hash_x,
        author: node_a.did.clone(),
        clock: entry_x.clock.clone(),
        topic: topic_x.to_string(),
    };
    node_b.deliver(&node_a.did, announce_x).await;

    let all_nodes: Vec<&TestNode> = vec![&node_a, &node_b];
    route_until_quiescent(&all_nodes).await;

    // B should have topic_x entry
    assert!(
        node_b.get_entry(topic_x, &hash_x).await.is_some(),
        "B should have the topic_x entry"
    );

    // B should NOT have topic_y entry (it was never delivered)
    assert!(
        node_b.get_entry(topic_y, &hash_y).await.is_none(),
        "B should NOT have the topic_y entry"
    );

    // Verify B has exactly 1 entry on topic_x and 0 on topic_y
    assert_eq!(
        node_b.get_entries(topic_x).await.len(),
        1,
        "Node B should have exactly 1 entry on topic_x"
    );
    assert_eq!(
        node_b.get_entries(topic_y).await.len(),
        0,
        "Node B should have 0 entries on topic_y"
    );
}

// ============================================================================
// Test 6: Duplicate Entry Deduplication
// ============================================================================

/// Verify that delivering the same entry multiple times does not create
/// duplicate entries. This tests the idempotency of the gossip entry storage.
#[tokio::test]
async fn test_duplicate_entry_deduplication() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let topic = "devnet:dedup";

    let node_a = TestNode::new().await;
    let node_b = TestNode::new().await;

    node_a.create_topic(topic).await;
    node_b.create_topic(topic).await;

    // A publishes an entry
    let hash = node_a.publish(topic, b"dedup test data".to_vec()).await;
    let entry = node_a.get_entry(topic, &hash).await.unwrap();

    // Deliver the same Response to B three times
    for _ in 0..3 {
        node_b
            .deliver(
                &node_a.did,
                GossipMessage::Response {
                    entry: entry.clone(),
                },
            )
            .await;
    }

    // B should have exactly 1 entry (not 3)
    let entries = node_b.get_entries(topic).await;
    assert_eq!(
        entries.len(),
        1,
        "Duplicate entries should be deduplicated, got {}",
        entries.len()
    );
    assert_eq!(entries[0].hash, hash);
}
