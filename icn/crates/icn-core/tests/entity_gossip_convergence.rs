#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Entity gossip convergence integration tests
//!
//! These tests validate that entity state synchronizes across nodes via gossip:
//! 1. Entity creation propagates to other nodes
//! 2. Entity updates propagate with last-write-wins semantics
//! 3. Membership changes sync across the network
//! 4. Entity deletion markers propagate to other nodes
//!
//! The tests use a simplified test node setup similar to multi_node_gossip_convergence.rs.

use anyhow::Result;
use icn_entity::{
    CooperativeEntity, EntityActor, EntityHandle, EntityType, Membership, MembershipRole,
    SledEntityRegistry, ENTITY_TOPIC,
};
use icn_gossip::{AccessControl, GossipActor, GossipMessage, Topic};
use icn_identity::{IdentityBundle, KeyPair};
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor, NetworkMessage};
use icn_trust::TrustClass;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Get an available port for testing
fn get_available_port() -> u16 {
    portpicker::pick_unused_port().expect("No available ports")
}

/// Test node with entity actor and gossip support
struct TestNode {
    keypair: KeyPair,
    did: icn_identity::Did,
    network_handle: icn_net::NetworkHandle,
    #[allow(dead_code)]
    gossip_handle: Arc<RwLock<GossipActor>>,
    entity_handle: EntityHandle,
    listen_addr: SocketAddr,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    _temp_dir: TempDir,
}

impl TestNode {
    async fn spawn(port: u16) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        info!("Spawning test node with DID: {}", did);

        // Create shutdown channel
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(16);

        // Create temp directory for entity storage
        let temp_dir = TempDir::new()?;
        let entity_store_path = temp_dir.path().join("entities");
        std::fs::create_dir_all(&entity_store_path)?;
        let db = sled::open(&entity_store_path)?;
        let registry = SledEntityRegistry::new(Arc::new(db))?;

        // Spawn gossip actor
        let trust_lookup = Arc::new(|_did: &icn_identity::Did| Some(TrustClass::Partner));
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        // Create entity topic
        {
            let mut gossip = gossip_handle.write().await;
            let topic = Topic::new(ENTITY_TOPIC.to_string(), AccessControl::Public);
            gossip.create_topic(topic);
        }

        // Spawn entity actor with gossip support
        let tx = EntityActor::spawn(registry, Some(gossip_handle.clone()));
        let entity_handle = EntityHandle::new(tx);

        // Set up notification callback for entity updates
        let entity_handle_for_notifications = entity_handle.clone();
        {
            let mut gossip = gossip_handle.write().await;
            let callback = Arc::new(
                move |topic: String,
                      entry: icn_gossip::GossipEntry,
                      _sub_did: icn_identity::Did| {
                    if topic == ENTITY_TOPIC {
                        let data = entry.data.clone();
                        let handle = entity_handle_for_notifications.clone();
                        tokio::spawn(async move {
                            if let Ok(entity) =
                                icn_encoding::decode_versioned::<CooperativeEntity>(&data)
                            {
                                if let Err(e) = handle.apply_gossip_update(entity).await {
                                    warn!("Failed to apply entity update from gossip: {}", e);
                                }
                            }
                        });
                    }
                },
            );
            gossip.set_notification_callback(callback);
        }

        // Create incoming message handler
        let gossip_handle_clone = gossip_handle.clone();
        let network_handle_holder = Arc::new(RwLock::new(None::<icn_net::NetworkHandle>));
        let network_handle_holder_clone = network_handle_holder.clone();
        let own_did_clone = did.clone();

        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            let sender_did = net_msg.from.clone();

            match net_msg.payload {
                MessagePayload::Gossip(gossip_msg) => {
                    let gossip_handle = gossip_handle_clone.clone();
                    let sender = sender_did.clone();
                    tokio::spawn(async move {
                        let mut gossip = gossip_handle.write().await;
                        if let Err(e) = gossip.handle_message(&sender, gossip_msg).await {
                            warn!("Failed to handle gossip message: {}", e);
                        }
                    });
                }

                MessagePayload::Subscribe { topics } => {
                    debug!(
                        "Received Subscribe from {} for topics: {:?}",
                        sender_did, topics
                    );
                    let gossip_handle = gossip_handle_clone.clone();
                    let sender = sender_did.clone();
                    let network_handle_lock = network_handle_holder_clone.clone();
                    let own_did = own_did_clone.clone();
                    tokio::spawn(async move {
                        let mut gossip = gossip_handle.write().await;
                        let mut acked_topics = Vec::new();

                        for topic in &topics {
                            match gossip.subscribe(topic, sender.clone()).await {
                                Ok(_) => acked_topics.push(topic.clone()),
                                Err(e) => {
                                    warn!("Failed to subscribe {} to {}: {}", sender, topic, e)
                                }
                            }
                        }

                        if !acked_topics.is_empty() {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            if let Some(net_handle) = network_handle_lock.read().await.as_ref() {
                                let ack_msg = NetworkMessage::subscribe_ack(
                                    own_did,
                                    sender.clone(),
                                    acked_topics.clone(),
                                );
                                if let Err(e) = net_handle.send_message(sender, ack_msg).await {
                                    warn!("Failed to send SubscribeAck: {}", e);
                                }
                            }
                        }
                    });
                }

                MessagePayload::SubscribeAck { topics } => {
                    debug!(
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
            None, // No trust-gated config
            None, // No fallback config
            None, // No topology config
            None, // No STUN servers
            None, // No TURN config
            None, // No misbehavior detector
            None, // No store
            None, // personhood_store
            None, // anchor_rate_config
        )
        .await?;

        // Initialize network handle for message handler
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

        info!("Test node spawned on {} with entity support", listen_addr);

        Ok(TestNode {
            keypair,
            did,
            network_handle,
            gossip_handle,
            entity_handle,
            listen_addr,
            shutdown_tx,
            _temp_dir: temp_dir,
        })
    }

    async fn shutdown(self) {
        info!("Shutting down test node {}", self.did);
        let _ = self.shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    async fn subscribe_to_topic(&self, peer: &TestNode, topic: &str) -> Result<()> {
        let subscribe_msg =
            NetworkMessage::subscribe(self.did.clone(), peer.did.clone(), vec![topic.to_string()]);
        self.network_handle
            .send_message(peer.did.clone(), subscribe_msg)
            .await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }
}

#[tokio::test]
// Ignored: Full gossip network integration test. Requires multi-node setup with
// network discovery and may have timing issues in CI. Run locally with:
// cargo test -p icn-core --test entity_gossip_convergence -- --ignored
#[ignore]
async fn test_entity_creation_syncs_across_nodes() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    info!("=== Starting entity creation sync test ===");

    // Spawn two nodes
    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;

    info!("Node 1 DID: {}", node1.did);
    info!("Node 2 DID: {}", node2.did);

    // Connect nodes
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Node 2 subscribes to entity topic on Node 1
    node2.subscribe_to_topic(&node1, ENTITY_TOPIC).await?;

    info!("✓ Nodes connected and subscribed to entity topic");

    // Create entity on Node 1
    let entity = CooperativeEntity::cooperative("test-sync-coop", "Sync Test Coop").unwrap();
    let entity_id = entity.id.clone();

    node1.entity_handle.register(entity.clone()).await?;

    info!("✓ Entity created on Node 1: {}", entity_id);

    // Wait for gossip propagation
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify entity appears on Node 2
    let retrieved = node2.entity_handle.get(&entity_id).await?;
    assert!(
        retrieved.is_some(),
        "Entity should have synced to Node 2 via gossip"
    );
    assert_eq!(retrieved.unwrap().name, "Sync Test Coop");

    info!("✓ Entity synced to Node 2 successfully");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;

    info!("✓ Entity creation sync test completed successfully");

    Ok(())
}

#[tokio::test]
// Ignored: Full gossip network integration test. Requires multi-node setup with
// network discovery and may have timing issues in CI. Run locally with:
// cargo test -p icn-core --test entity_gossip_convergence -- --ignored
#[ignore]
async fn test_entity_update_syncs_with_last_write_wins() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    info!("=== Starting entity update sync test ===");

    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;

    // Connect nodes
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Mutual subscriptions
    node1.subscribe_to_topic(&node2, ENTITY_TOPIC).await?;
    node2.subscribe_to_topic(&node1, ENTITY_TOPIC).await?;

    // Create entity on Node 1
    let mut entity = CooperativeEntity::cooperative("update-test", "Original Name").unwrap();
    let entity_id = entity.id.clone();
    node1.entity_handle.register(entity.clone()).await?;

    // Wait for initial sync
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify it synced
    let synced = node2.entity_handle.get(&entity_id).await?.unwrap();
    assert_eq!(synced.name, "Original Name");

    // Update entity on Node 1 with a newer timestamp
    entity.name = "Updated Name".to_string();
    entity.updated_at = icn_time::current_timestamp_secs() + 1;
    node1.entity_handle.update(entity.clone()).await?;

    info!("✓ Entity updated on Node 1");

    // Wait for update to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify update synced to Node 2
    let updated = node2.entity_handle.get(&entity_id).await?.unwrap();
    assert_eq!(updated.name, "Updated Name", "Update should have synced");

    info!("✓ Entity update synced to Node 2 successfully");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;

    Ok(())
}

#[tokio::test]
// Ignored: Full gossip network integration test. Requires multi-node setup with
// network discovery and may have timing issues in CI. Run locally with:
// cargo test -p icn-core --test entity_gossip_convergence -- --ignored
#[ignore]
async fn test_membership_syncs_across_nodes() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    info!("=== Starting membership sync test ===");

    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;

    // Connect and subscribe
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    node2.subscribe_to_topic(&node1, ENTITY_TOPIC).await?;

    // Create coop and individual on Node 1
    let coop = CooperativeEntity::cooperative("membership-coop", "Membership Test Coop").unwrap();
    let coop_id = coop.id.clone();
    node1.entity_handle.register(coop).await?;

    let individual = CooperativeEntity::individual(node1.keypair.did(), "Test Worker");
    let member_id = individual.id.clone();
    node1.entity_handle.register(individual).await?;

    // Wait for initial sync
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Add membership on Node 1
    let membership = Membership::new(member_id.clone(), coop_id.clone(), MembershipRole::Worker);
    node1.entity_handle.add_membership(membership).await?;

    info!("✓ Membership added on Node 1");

    // Wait for membership sync (membership changes trigger parent entity update)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify coop synced to Node 2
    let synced_coop = node2.entity_handle.get(&coop_id).await?.unwrap();
    assert_eq!(synced_coop.name, "Membership Test Coop");

    info!("✓ Coop with membership synced to Node 2");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;

    Ok(())
}

#[tokio::test]
// Ignored: Full gossip network integration test. Requires multi-node setup with
// network discovery and may have timing issues in CI. Run locally with:
// cargo test -p icn-core --test entity_gossip_convergence -- --ignored
#[ignore]
async fn test_entity_deletion_syncs_across_nodes() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    info!("=== Starting entity deletion sync test ===");

    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;

    // Connect and subscribe
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    node2.subscribe_to_topic(&node1, ENTITY_TOPIC).await?;

    // Create entity on Node 1
    let entity = CooperativeEntity::cooperative("delete-test", "To Be Deleted").unwrap();
    let entity_id = entity.id.clone();
    node1.entity_handle.register(entity).await?;

    // Wait for sync
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify it synced
    assert!(node2.entity_handle.get(&entity_id).await?.is_some());

    // Delete entity on Node 1
    node1.entity_handle.delete(&entity_id).await?;

    info!("✓ Entity deleted on Node 1");

    // Wait for deletion to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify entity is deleted on Node 2
    // Note: The deletion marker should have propagated, causing the entity to be removed
    let result = node2.entity_handle.get(&entity_id).await?;
    assert!(
        result.is_none(),
        "Entity should be deleted on Node 2 after gossip sync"
    );

    info!("✓ Entity deletion synced to Node 2 successfully");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;

    Ok(())
}

#[tokio::test]
async fn test_entity_actor_local_operations() -> Result<()> {
    //! Test basic entity actor operations without networking
    //! This test runs faster and verifies core actor functionality

    let temp_dir = TempDir::new()?;
    let db = sled::open(temp_dir.path())?;
    let registry = SledEntityRegistry::new(Arc::new(db))?;

    // Spawn actor without gossip
    let tx = EntityActor::spawn(registry, None);
    let handle = EntityHandle::new(tx);

    // Test entity lifecycle
    let entity = CooperativeEntity::cooperative("local-test", "Local Test Coop").unwrap();
    let entity_id = entity.id.clone();

    // Register
    handle.register(entity.clone()).await?;

    // Get
    let retrieved = handle.get(&entity_id).await?.unwrap();
    assert_eq!(retrieved.name, "Local Test Coop");

    // Update
    let mut updated = retrieved.clone();
    updated.name = "Updated Local Test".to_string();
    handle.update(updated).await?;

    // Verify update
    let after_update = handle.get(&entity_id).await?.unwrap();
    assert_eq!(after_update.name, "Updated Local Test");

    // List by type
    let coops = handle.list_by_type(EntityType::Cooperative).await?;
    assert_eq!(coops.len(), 1);

    // Count
    let count = handle.count().await?;
    assert_eq!(count, 1);

    // Delete
    handle.delete(&entity_id).await?;
    assert!(handle.get(&entity_id).await?.is_none());

    info!("✓ Local entity actor operations test passed");

    Ok(())
}

#[tokio::test]
async fn test_entity_handle_membership_operations() -> Result<()> {
    //! Test membership operations via entity handle

    let temp_dir = TempDir::new()?;
    let db = sled::open(temp_dir.path())?;
    let registry = SledEntityRegistry::new(Arc::new(db))?;

    let tx = EntityActor::spawn(registry, None);
    let handle = EntityHandle::new(tx);

    // Create a coop
    let coop = CooperativeEntity::cooperative("membership-local", "Membership Test").unwrap();
    let coop_id = coop.id.clone();
    handle.register(coop).await?;

    // Create an individual
    let keypair = KeyPair::generate()?;
    let individual = CooperativeEntity::individual(keypair.did(), "Test Member");
    let member_id = individual.id.clone();
    handle.register(individual).await?;

    // Add membership
    let membership = Membership::new(member_id.clone(), coop_id.clone(), MembershipRole::Worker);
    handle.add_membership(membership).await?;

    // Check member count
    assert_eq!(handle.member_count(&coop_id).await?, 1);

    // Get members
    let members = handle.get_members(&coop_id).await?;
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].member_id, member_id);
    assert_eq!(members[0].role, MembershipRole::Worker);

    // Get memberships of individual
    let memberships = handle.get_memberships_of(&member_id).await?;
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].parent_id, coop_id);

    // Remove membership
    handle.remove_membership(&member_id, &coop_id).await?;
    assert_eq!(handle.member_count(&coop_id).await?, 0);

    info!("✓ Entity handle membership operations test passed");

    Ok(())
}
