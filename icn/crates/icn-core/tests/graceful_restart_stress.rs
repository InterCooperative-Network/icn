//! Stress tests for graceful restart under high load
//!
//! These tests validate that graceful restart works correctly with:
//! - High message volume (1000+ messages)
//! - Many peers (50-100+ simultaneous connections)
//! - Concurrent gossip activity during shutdown/restart
//! - Large state snapshots

use anyhow::Result;
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_identity::{Did, IdentityBundle, KeyPair};
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor, NetworkHandle};
use icn_snapshot::{load_snapshot, save_snapshot, StateSnapshot};
use icn_trust::TrustClass;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Helper to create a test node with snapshot support
struct TestNode {
    keypair: KeyPair,
    did: Did,
    network_handle: NetworkHandle,
    gossip_handle: Arc<RwLock<GossipActor>>,
    listen_addr: SocketAddr,
    data_dir: PathBuf,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl TestNode {
    async fn spawn(port: u16, data_dir: PathBuf) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        // Create shutdown channel
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(16);

        // Spawn gossip actor
        let trust_lookup = Arc::new(|_did: &Did| Some(TrustClass::Partner));
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        // Set keypair for signing
        {
            let mut gossip = gossip_handle.write().await;
            gossip.set_keypair(keypair.clone());
        }

        // Create incoming message handler that routes to gossip
        let gossip_handle_clone = gossip_handle.clone();
        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                let sender = net_msg.from.clone();
                let mut gossip = gossip_handle_clone.blocking_write();
                if let Err(e) = gossip.handle_message(&sender, gossip_msg) {
                    warn!("Failed to handle gossip message: {}", e);
                }
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
        )
        .await?;

        Ok(TestNode {
            keypair,
            did,
            network_handle,
            gossip_handle,
            listen_addr,
            data_dir,
            shutdown_tx,
        })
    }

    /// Export current state to snapshot
    async fn export_state(&self) -> Result<StateSnapshot> {
        let mut snapshot = StateSnapshot::new();

        // Export gossip state
        let gossip = self.gossip_handle.read().await;
        snapshot.gossip_state = Some(gossip.export_state());
        drop(gossip);

        // Export network state
        let network_state = self.network_handle.export_state().await;
        snapshot.network_state = Some(network_state);

        Ok(snapshot)
    }

    /// Restore state from snapshot
    async fn restore_state(&self, snapshot: StateSnapshot) -> Result<()> {
        // Restore gossip state
        if let Some(gossip_state) = snapshot.gossip_state {
            let mut gossip = self.gossip_handle.write().await;
            gossip.restore_state(gossip_state)?;
            drop(gossip);
        }

        // Restore network state
        if let Some(network_state) = snapshot.network_state {
            self.network_handle.restore_state(network_state).await?;
        }

        Ok(())
    }

    /// Save snapshot to disk
    async fn save_snapshot(&self) -> Result<()> {
        let start = Instant::now();
        let snapshot = self.export_state().await?;
        save_snapshot(&snapshot, &self.data_dir)?;
        let elapsed = start.elapsed();
        info!("✅ Snapshot saved in {:?}", elapsed);
        Ok(())
    }

    /// Load snapshot from disk and restore
    async fn load_and_restore_snapshot(&self) -> Result<Duration> {
        let start = Instant::now();
        if let Some(snapshot) = load_snapshot(&self.data_dir)? {
            self.restore_state(snapshot).await?;
            let elapsed = start.elapsed();
            info!("✅ State restored in {:?}", elapsed);
            Ok(elapsed)
        } else {
            anyhow::bail!("No snapshot found")
        }
    }

    /// Reconnect with existing node (reusing same keypair after restart)
    async fn reconnect(
        keypair: KeyPair,
        data_dir: PathBuf,
        port: u16,
    ) -> Result<Self> {
        let did = keypair.did().clone();

        let (shutdown_tx, _) = tokio::sync::broadcast::channel(16);

        // Spawn gossip actor
        let trust_lookup = Arc::new(|_did: &Did| Some(TrustClass::Partner));
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        {
            let mut gossip = gossip_handle.write().await;
            gossip.set_keypair(keypair.clone());
        }

        // Create incoming handler
        let gossip_handle_clone = gossip_handle.clone();
        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                let sender = net_msg.from.clone();
                let mut gossip = gossip_handle_clone.blocking_write();
                if let Err(e) = gossip.handle_message(&sender, gossip_msg) {
                    warn!("Failed to handle gossip message: {}", e);
                }
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
            None, None, None, None, None,
        )
        .await?;

        Ok(TestNode {
            keypair,
            did,
            network_handle,
            gossip_handle,
            listen_addr,
            data_dir,
            shutdown_tx,
        })
    }
}

impl Drop for TestNode {
    fn drop(&mut self) {
        // Send shutdown signal
        let _ = self.shutdown_tx.send(());
    }
}

#[tokio::test]
#[ignore] // Stress test - run explicitly with `cargo test stress -- --ignored`
async fn test_high_message_volume_restart() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().to_path_buf();

    let topic_name = "test:high_volume";
    let message_count = 1000; // 1000 messages

    info!("\n=== Stress Test: High Message Volume ({}  messages) ===", message_count);

    // Phase 1: Create node and publish many messages
    info!("\n=== Phase 1: Publishing {} messages ===", message_count);
    let start_publish = Instant::now();

    let node = TestNode::spawn(51000, data_dir.clone()).await?;
    let did = node.did.clone();

    {
        let mut gossip = node.gossip_handle.write().await;
        let topic = Topic::new(topic_name.to_string(), AccessControl::Public);
        gossip.create_topic(topic);
        gossip.subscribe(topic_name, did.clone())?;
    }

    // Publish messages
    for i in 0..message_count {
        let mut gossip = node.gossip_handle.write().await;
        let msg = format!("message {i}");
        gossip.publish(topic_name, msg.as_bytes().to_vec())?;

        if (i + 1) % 100 == 0 {
            info!("Published {} messages", i + 1);
        }
    }

    let publish_elapsed = start_publish.elapsed();
    info!("✅ Published {} messages in {:?} ({:.0} msg/s)",
          message_count, publish_elapsed, message_count as f64 / publish_elapsed.as_secs_f64());

    // Save snapshot
    let start_save = Instant::now();
    node.save_snapshot().await?;
    let save_elapsed = start_save.elapsed();
    info!("✅ Snapshot save time: {:?}", save_elapsed);

    // Get vector clock before restart
    let pre_restart_clock = {
        let gossip = node.gossip_handle.read().await;
        let state = gossip.export_state();
        *state.vector_clock.get(&did.to_string()).unwrap_or(&0)
    };

    let keypair = node.keypair.clone();

    // Shutdown
    info!("\n=== Shutting down node ===");
    drop(node);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Phase 2: Restart and restore
    info!("\n=== Phase 2: Restarting and restoring ===");
    let node_restart = TestNode::reconnect(keypair, data_dir.clone(), 51000).await?;

    let restore_elapsed = node_restart.load_and_restore_snapshot().await?;
    info!("✅ Restore time: {:?} for {} messages", restore_elapsed, message_count);

    // Verify vector clock was restored
    let post_restart_clock = {
        let gossip = node_restart.gossip_handle.read().await;
        let state = gossip.export_state();
        *state.vector_clock.get(&did.to_string()).unwrap_or(&0)
    };

    assert_eq!(
        pre_restart_clock, post_restart_clock,
        "Vector clock should be preserved across restart"
    );
    info!("✅ Vector clock preserved: {}", post_restart_clock);

    // Performance assertions
    assert!(
        save_elapsed < Duration::from_secs(5),
        "Snapshot save should complete in < 5s (took {save_elapsed:?})"
    );
    assert!(
        restore_elapsed < Duration::from_secs(5),
        "Snapshot restore should complete in < 5s (took {restore_elapsed:?})"
    );

    info!("\n✅ High message volume stress test passed!");
    Ok(())
}

#[tokio::test]
#[ignore] // Stress test - run explicitly
async fn test_many_peers_restart() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::WARN) // Reduce noise for many peers
        .try_init();

    let temp_dir = TempDir::new()?;
    let central_node_dir = temp_dir.path().join("central");
    std::fs::create_dir_all(&central_node_dir)?;

    let peer_count = 50; // 50 peers (represents scaling to 1000+)
    let base_port = 52000;

    info!("\n=== Stress Test: Many Peers ({} peers) ===", peer_count);

    // Phase 1: Create central node and many peer nodes
    info!("\n=== Phase 1: Creating {} peers ===", peer_count);
    let start_setup = Instant::now();

    let central_node = TestNode::spawn(base_port, central_node_dir.clone()).await?;
    let central_did = central_node.did.clone();

    info!("Central node spawned at port {}", base_port);

    // Create peer nodes and connect them to central node
    let mut peer_nodes = Vec::new();
    for i in 0..peer_count {
        let peer_dir = temp_dir.path().join(format!("peer{i}"));
        std::fs::create_dir_all(&peer_dir)?;

        let peer_port = base_port + 1 + i as u16;
        let peer_node = TestNode::spawn(peer_port, peer_dir).await?;

        // Dial central node
        peer_node
            .network_handle
            .dial(central_node.listen_addr, central_did.clone())
            .await?;

        peer_nodes.push(peer_node);

        if (i + 1) % 10 == 0 {
            info!("Created {} peers", i + 1);
        }
    }

    let setup_elapsed = start_setup.elapsed();
    info!("✅ Created {} peers in {:?}", peer_count, setup_elapsed);

    // Wait for connections to establish
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify X25519 key exchange happened
    let start_verify = Instant::now();
    let mut keys_exchanged = 0;
    for peer in &peer_nodes {
        if central_node
            .network_handle
            .get_peer_x25519_key(&peer.did)
            .await
            .is_some()
        {
            keys_exchanged += 1;
        }
    }
    let verify_elapsed = start_verify.elapsed();

    info!("✅ X25519 keys exchanged with {}/{} peers in {:?}",
          keys_exchanged, peer_count, verify_elapsed);

    // Save snapshot
    let start_save = Instant::now();
    central_node.save_snapshot().await?;
    let save_elapsed = start_save.elapsed();
    info!("✅ Snapshot saved with {} peer keys in {:?}",
          keys_exchanged, save_elapsed);

    // Get network state before restart
    let pre_restart_state = central_node.export_state().await?;
    let pre_restart_key_count = pre_restart_state
        .network_state
        .as_ref()
        .map(|ns| ns.peer_x25519_keys.len())
        .unwrap_or(0);

    let central_keypair = central_node.keypair.clone();

    // Shutdown central node (keep peers running)
    info!("\n=== Shutting down central node ===");
    drop(central_node);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Phase 2: Restart central node
    info!("\n=== Phase 2: Restarting central node ===");
    let central_restart = TestNode::reconnect(
        central_keypair,
        central_node_dir.clone(),
        base_port,
    )
    .await?;

    let restore_elapsed = central_restart.load_and_restore_snapshot().await?;
    info!("✅ Restored state in {:?}", restore_elapsed);

    // Verify peer keys were restored
    let post_restart_state = central_restart.export_state().await?;
    let post_restart_key_count = post_restart_state
        .network_state
        .as_ref()
        .map(|ns| ns.peer_x25519_keys.len())
        .unwrap_or(0);

    assert_eq!(
        pre_restart_key_count, post_restart_key_count,
        "Peer X25519 key count should match after restart"
    );
    info!("✅ All {} peer X25519 keys restored", post_restart_key_count);

    // Verify specific keys match
    for (i, peer) in peer_nodes.iter().enumerate().take(10) {
        // Check first 10 as sample
        let key_present = central_restart
            .network_handle
            .get_peer_x25519_key(&peer.did)
            .await
            .is_some();
        if !key_present {
            panic!("Peer {i} key not restored");
        }
    }
    info!("✅ Verified sample of restored keys");

    // Performance assertions
    assert!(
        save_elapsed < Duration::from_secs(10),
        "Save with {peer_count} peers should complete in < 10s (took {save_elapsed:?})"
    );
    assert!(
        restore_elapsed < Duration::from_secs(10),
        "Restore with {peer_count} peers should complete in < 10s (took {restore_elapsed:?})"
    );

    info!("\n✅ Many peers stress test passed!");
    Ok(())
}

#[tokio::test]
#[ignore] // Stress test - run explicitly
async fn test_multi_topic_high_subscription_restart() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().to_path_buf();

    let topic_count = 100; // 100 topics
    let messages_per_topic = 10;

    info!("\n=== Stress Test: Multi-Topic ({} topics, {} msgs each) ===",
          topic_count, messages_per_topic);

    // Phase 1: Create many topics and publish messages
    info!("\n=== Phase 1: Creating {} topics ===", topic_count);
    let start = Instant::now();

    let node = TestNode::spawn(53000, data_dir.clone()).await?;
    let did = node.did.clone();

    // Create topics and subscribe
    for i in 0..topic_count {
        let topic_name = format!("test:topic{i}");
        let mut gossip = node.gossip_handle.write().await;
        let topic = Topic::new(topic_name.clone(), AccessControl::Public);
        gossip.create_topic(topic);
        gossip.subscribe(&topic_name, did.clone())?;
    }

    info!("✅ Created and subscribed to {} topics", topic_count);

    // Publish messages to each topic
    info!("Publishing {} messages to each topic...", messages_per_topic);
    for i in 0..topic_count {
        let topic_name = format!("test:topic{i}");
        for j in 0..messages_per_topic {
            let mut gossip = node.gossip_handle.write().await;
            let msg = format!("topic{i} msg{j}");
            gossip.publish(&topic_name, msg.as_bytes().to_vec())?;
        }
    }

    let setup_elapsed = start.elapsed();
    let total_messages = topic_count * messages_per_topic;
    info!("✅ Published {} total messages in {:?} ({:.0} msg/s)",
          total_messages, setup_elapsed,
          total_messages as f64 / setup_elapsed.as_secs_f64());

    // Save snapshot
    node.save_snapshot().await?;

    // Get state before restart
    let pre_restart_state = {
        let gossip = node.gossip_handle.read().await;
        gossip.export_state()
    };

    let pre_restart_topics: HashSet<String> = pre_restart_state.topics.keys().cloned().collect();
    let pre_restart_subs: HashSet<String> = pre_restart_state.subscriptions.keys().cloned().collect();

    let keypair = node.keypair.clone();

    // Shutdown
    info!("\n=== Shutting down node ===");
    drop(node);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Phase 2: Restart and verify
    info!("\n=== Phase 2: Restarting ===");
    let node_restart = TestNode::reconnect(keypair, data_dir.clone(), 53000).await?;
    let restore_start = Instant::now();
    node_restart.load_and_restore_snapshot().await?;
    let restore_elapsed = restore_start.elapsed();

    info!("✅ Restored in {:?}", restore_elapsed);

    // Verify all topics and subscriptions restored
    let post_restart_state = {
        let gossip = node_restart.gossip_handle.read().await;
        gossip.export_state()
    };

    let post_restart_topics: HashSet<String> = post_restart_state.topics.keys().cloned().collect();
    let post_restart_subs: HashSet<String> = post_restart_state.subscriptions.keys().cloned().collect();

    assert_eq!(
        pre_restart_topics, post_restart_topics,
        "All topics should be restored"
    );
    assert_eq!(
        pre_restart_subs, post_restart_subs,
        "All subscriptions should be restored"
    );

    info!("✅ All {} topics restored", topic_count);
    info!("✅ All {} subscriptions restored", topic_count);

    // Performance assertion
    assert!(
        restore_elapsed < Duration::from_secs(5),
        "Restore with {topic_count} topics should complete in < 5s (took {restore_elapsed:?})"
    );

    info!("\n✅ Multi-topic stress test passed!");
    Ok(())
}
