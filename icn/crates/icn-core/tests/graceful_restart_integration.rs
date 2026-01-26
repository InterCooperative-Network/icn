//! Integration test for graceful restart with state snapshot
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! This test validates that gossip and network state can be persisted and restored
//! across daemon restarts, maintaining vector clocks, subscriptions, and X25519 keys.

use anyhow::Result;
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_identity::{Did, IdentityBundle, KeyPair};
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor, NetworkHandle};
use icn_snapshot::{load_snapshot, save_snapshot, StateSnapshot};
use icn_trust::TrustClass;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Get an available port for testing
fn get_available_port() -> u16 {
    portpicker::pick_unused_port().expect("No available ports")
}

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

        info!("Spawning test node with DID: {} on port {}", did, port);

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
            info!("✅ Gossip state restored");
        }

        // Restore network state
        if let Some(network_state) = snapshot.network_state {
            self.network_handle.restore_state(network_state).await?;
            info!("✅ Network state restored");
        }

        Ok(())
    }

    /// Save snapshot to disk
    async fn save_snapshot(&self) -> Result<()> {
        let snapshot = self.export_state().await?;
        save_snapshot(&snapshot, &self.data_dir)?;
        info!("✅ Snapshot saved to {}", self.data_dir.display());
        Ok(())
    }

    /// Load snapshot from disk and restore
    async fn load_and_restore_snapshot(&self) -> Result<()> {
        if let Some(snapshot) = load_snapshot(&self.data_dir)? {
            info!(
                "Found snapshot (version {}, created at {})",
                snapshot.version, snapshot.created_at
            );
            self.restore_state(snapshot).await?;
            Ok(())
        } else {
            anyhow::bail!("No snapshot found")
        }
    }
}

impl Drop for TestNode {
    fn drop(&mut self) {
        // Send shutdown signal
        let _ = self.shutdown_tx.send(());
    }
}

/// Test graceful restart preserves state.
///
/// Note: Ignored in CI due to intermittent QUIC stream failures in containerized environment.
/// Run manually with: cargo test -p icn-core --test graceful_restart_integration test_graceful_restart_preserves_state -- --ignored
#[tokio::test]
#[ignore = "Flaky in CI: QUIC stream failures in containerized environment"]
async fn test_graceful_restart_preserves_state() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing for test output
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    // Create temporary directory for snapshot
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().to_path_buf();
    info!("Using data directory: {}", data_dir.display());

    let topic_name = "test:restart";

    // ===== Phase 1: Initial node with state =====
    info!("\n=== Phase 1: Creating initial node ===");
    let node1 = TestNode::spawn(50100, data_dir.clone()).await?;
    let did1 = node1.did.clone();

    // Create topic and subscribe
    {
        let mut gossip = node1.gossip_handle.write().await;
        let topic = Topic::new(topic_name.to_string(), AccessControl::Public);
        gossip.create_topic(topic);
        gossip.subscribe(topic_name, did1.clone()).await?;
        info!("✅ Created topic '{}' and subscribed", topic_name);
    }

    // Publish messages to create vector clock state
    let num_messages = 3;
    for i in 0..num_messages {
        let mut gossip = node1.gossip_handle.write().await;
        let msg = format!("message {i}");
        let entry_id = gossip.publish(topic_name, msg.as_bytes().to_vec()).await?;
        info!("✅ Published entry {}: {:?}", i, entry_id);
    }

    // Save snapshot
    node1.save_snapshot().await?;
    info!("✅ Snapshot saved before shutdown");

    // Verify snapshot file exists
    let snapshot_path = data_dir.join("state.snapshot");
    assert!(snapshot_path.exists(), "Snapshot file should exist");

    // Extract keypair before dropping node1
    let keypair1 = node1.keypair.clone();

    // Shutdown node1
    info!("\n=== Shutting down node1 ===");
    drop(node1);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ===== Phase 2: Restart node with same identity =====
    info!("\n=== Phase 2: Restarting node with state restoration ===");

    // Create new node (simulating daemon restart)
    // We'll manually construct it with the same keypair to simulate identity persistence
    let did2 = keypair1.did().clone();
    let (shutdown_tx2, _) = tokio::sync::broadcast::channel(16);

    // Spawn gossip actor
    let trust_lookup = Arc::new(|_did: &Did| Some(TrustClass::Partner));
    let gossip_handle2 = GossipActor::spawn(did2.clone(), trust_lookup);

    // Set keypair
    {
        let mut gossip = gossip_handle2.write().await;
        gossip.set_keypair(keypair1.clone());
    }

    // Create incoming handler
    let gossip_handle2_clone = gossip_handle2.clone();
    let incoming_handler2: IncomingMessageHandler = Arc::new(move |net_msg| {
        if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
            let sender = net_msg.from.clone();
            let gossip_handle = gossip_handle2_clone.clone();
            tokio::spawn(async move {
                let mut gossip = gossip_handle.write().await;
                if let Err(e) = gossip.handle_message(&sender, gossip_msg).await {
                    warn!("Failed to handle gossip message: {}", e);
                }
            });
        }
    });

    // Spawn network actor
    let listen_addr2: SocketAddr = "127.0.0.1:50100".parse()?;
    let identity_bundle2 = IdentityBundle::from_keypair(keypair1.clone())?;
    let network_handle2 = NetworkActor::spawn(
        identity_bundle2,
        listen_addr2,
        shutdown_tx2.clone(),
        Some(incoming_handler2),
        None, // No PolicyOracle
        None, // No fallback config
        None, // No topology config
        None, // No STUN servers
        None, // No TURN config
        None, // No misbehavior detector for tests
        None, // No store for tests
        None, // personhood_store
        None, // anchor_rate_config
    )
    .await?;

    // Create node2 wrapper
    let node2 = TestNode {
        keypair: keypair1,
        did: did2.clone(),
        network_handle: network_handle2,
        gossip_handle: gossip_handle2,
        listen_addr: listen_addr2,
        data_dir: data_dir.clone(),
        shutdown_tx: shutdown_tx2,
    };

    // Load and restore snapshot
    node2.load_and_restore_snapshot().await?;
    info!("✅ State restored from snapshot");

    // ===== Phase 3: Verify state restoration =====
    info!("\n=== Phase 3: Verifying state restoration ===");

    // Verify state was restored by checking the snapshot
    let restored_state = node2.export_state().await?;
    let restored_gossip = restored_state.gossip_state.unwrap();

    // Verify vector clock was restored (should have count = num_messages)
    let restored_clock_count = restored_gossip
        .vector_clock
        .get(&did2.to_string())
        .unwrap_or(&0);
    info!(
        "Restored vector clock count for {}: {}",
        did2, restored_clock_count
    );
    assert_eq!(
        *restored_clock_count, num_messages,
        "Vector clock should match original value"
    );

    // Verify topic was restored
    assert!(
        restored_gossip.topics.contains_key(topic_name),
        "Topic should be restored"
    );
    info!("✅ Topic '{}' was restored", topic_name);

    // Verify subscription was restored
    let subs = restored_gossip.subscriptions.get(topic_name).unwrap();
    assert!(
        subs.contains(&did2.to_string()),
        "Subscription should be restored"
    );
    info!("✅ Subscription to '{}' was restored", topic_name);

    // Publish another message to verify state continuity
    {
        let mut gossip = node2.gossip_handle.write().await;
        let entry_id = gossip
            .publish(topic_name, b"post-restart message".to_vec())
            .await?;
        info!("✅ Published post-restart entry: {:?}", entry_id);
    }

    // Verify vector clock incremented from restored state
    let final_state = node2.export_state().await?;
    let final_gossip = final_state.gossip_state.unwrap();
    let final_clock_count = final_gossip
        .vector_clock
        .get(&did2.to_string())
        .unwrap_or(&0);
    info!(
        "Final vector clock count for {}: {}",
        did2, final_clock_count
    );
    assert_eq!(
        *final_clock_count,
        num_messages + 1,
        "Vector clock should increment from restored state"
    );

    info!("\n✅ All state verification checks passed!");
    Ok(())
}

/// Test X25519 keys persist across restart.
///
/// Note: Ignored in CI due to intermittent QUIC stream failures in containerized environment.
/// Run manually with: cargo test -p icn-core --test graceful_restart_integration test_x25519_keys_persist_across_restart -- --ignored
#[tokio::test]
#[ignore = "Flaky in CI: QUIC stream failures in containerized environment"]
async fn test_x25519_keys_persist_across_restart() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().to_path_buf();

    // Create node-specific data directories
    let node1_data_dir = data_dir.join("node1");
    let node2_data_dir = data_dir.join("node2");
    std::fs::create_dir_all(&node1_data_dir)?;
    std::fs::create_dir_all(&node2_data_dir)?;

    // Get available ports dynamically
    let port1 = get_available_port();
    let port2 = get_available_port();

    // ===== Phase 1: Create two nodes and exchange keys =====
    info!("\n=== Phase 1: Two-node key exchange ===");

    let node1 = TestNode::spawn(port1, node1_data_dir.clone()).await?;
    let node2 = TestNode::spawn(port2, node2_data_dir.clone()).await?;

    let did1 = node1.did.clone();
    let did2 = node2.did.clone();

    // Dial node2 from node1 to trigger key exchange
    info!("Node1 dialing node2...");
    node1
        .network_handle
        .dial(node2.listen_addr, did2.clone())
        .await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify keys were exchanged (via Hello protocol)
    let node1_has_node2_key = node1
        .network_handle
        .get_peer_x25519_key(&did2)
        .await
        .is_some();
    let node2_has_node1_key = node2
        .network_handle
        .get_peer_x25519_key(&did1)
        .await
        .is_some();

    info!("Node1 has Node2's X25519 key: {}", node1_has_node2_key);
    info!("Node2 has Node1's X25519 key: {}", node2_has_node1_key);

    assert!(node1_has_node2_key, "Node1 should have Node2's X25519 key");
    assert!(node2_has_node1_key, "Node2 should have Node1's X25519 key");

    // Get the original key for comparison
    let original_node2_key = node1
        .network_handle
        .get_peer_x25519_key(&did2)
        .await
        .unwrap();
    info!(
        "Original Node2 X25519 key (first 8 bytes): {:?}",
        &original_node2_key[..8]
    );

    // Save snapshot for node1
    node1.save_snapshot().await?;
    info!("✅ Node1 snapshot saved");

    // Extract node1's keypair
    let node1_keypair = node1.keypair.clone();

    // Shutdown node1
    info!("\n=== Shutting down node1 ===");
    drop(node1);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ===== Phase 2: Restart node1 and verify X25519 key persistence =====
    info!("\n=== Phase 2: Restarting node1 ===");

    // Reconstruct node1 with same identity
    let did1_restart = node1_keypair.did().clone();
    let (shutdown_tx_restart, _) = tokio::sync::broadcast::channel(16);

    let trust_lookup = Arc::new(|_did: &Did| Some(TrustClass::Partner));
    let gossip_handle_restart = GossipActor::spawn(did1_restart.clone(), trust_lookup);

    {
        let mut gossip = gossip_handle_restart.write().await;
        gossip.set_keypair(node1_keypair.clone());
    }

    let gossip_clone = gossip_handle_restart.clone();
    let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
        if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
            let sender = net_msg.from.clone();
            let gossip_handle = gossip_clone.clone();
            tokio::spawn(async move {
                let mut gossip = gossip_handle.write().await;
                if let Err(e) = gossip.handle_message(&sender, gossip_msg).await {
                    warn!("Failed to handle gossip message: {}", e);
                }
            });
        }
    });

    // Use a new available port for restart (the state persistence test doesn't require same port)
    let port_restart = get_available_port();
    let listen_addr_restart: SocketAddr = format!("127.0.0.1:{port_restart}").parse()?;
    let identity_bundle_restart = IdentityBundle::from_keypair(node1_keypair.clone())?;
    let network_handle_restart = NetworkActor::spawn(
        identity_bundle_restart,
        listen_addr_restart,
        shutdown_tx_restart.clone(),
        Some(incoming_handler),
        None, // No PolicyOracle
        None, // No fallback config
        None, // No topology config
        None, // No STUN servers
        None, // No TURN config
        None, // No misbehavior detector for tests
        None, // No store for tests
        None, // personhood_store
        None, // anchor_rate_config
    )
    .await?;

    let node1_restart = TestNode {
        keypair: node1_keypair,
        did: did1_restart.clone(),
        network_handle: network_handle_restart,
        gossip_handle: gossip_handle_restart,
        listen_addr: listen_addr_restart,
        data_dir: node1_data_dir.clone(),
        shutdown_tx: shutdown_tx_restart,
    };

    // Load and restore snapshot
    node1_restart.load_and_restore_snapshot().await?;
    info!("✅ Node1 state restored");

    // ===== Phase 3: Verify X25519 key was restored =====
    info!("\n=== Phase 3: Verifying X25519 key persistence ===");

    let restored_node2_key = node1_restart
        .network_handle
        .get_peer_x25519_key(&did2)
        .await;
    assert!(
        restored_node2_key.is_some(),
        "Node2's X25519 key should be restored"
    );

    let restored_key = restored_node2_key.unwrap();
    info!(
        "Restored Node2 X25519 key (first 8 bytes): {:?}",
        &restored_key[..8]
    );

    assert_eq!(
        original_node2_key, restored_key,
        "Restored X25519 key should match original"
    );

    info!("✅ X25519 key successfully persisted across restart!");

    Ok(())
}
