//! End-to-end integration test for social recovery
//!
//! This test validates the complete recovery flow:
//! 1. User loses device, creates new identity
//! 2. Initiates recovery via gossip
//! 3. Trustees attest to recovery
//! 4. Recovery finalized after threshold reached
//! 5. Trust graph edges migrate to new DID
//! 6. Ledger balances transfer to new DID

use anyhow::Result;
use icn_gossip::{GossipActor, GossipEntry};
use icn_identity::{
    IdentityBundle, KeyPair, RecoveryAttestation, RecoveryEvent, RecoveryMessage,
    IDENTITY_RECOVERY_TOPIC,
};
use icn_ledger::{entry::JournalEntryBuilder, Ledger};
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor, NetworkMessage};
use icn_store::{SledStore, Store};
use icn_trust::{TrustClass, TrustEdge, TrustGraph};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Helper to create a test node with full recovery infrastructure
struct RecoveryTestNode {
    keypair: KeyPair,
    did: icn_identity::Did,
    network_handle: icn_net::NetworkHandle,
    gossip_handle: Arc<RwLock<GossipActor>>,
    trust_graph: Arc<RwLock<TrustGraph>>,
    ledger: Arc<RwLock<Ledger>>,
    recovery_store: Arc<dyn Store>,
    _temp_dir: TempDir,
    listen_addr: SocketAddr,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl RecoveryTestNode {
    async fn spawn(port: u16) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        info!("Spawning recovery test node with DID: {}", did);

        // Create temporary directory for storage
        let temp_dir = TempDir::new()?;
        let trust_store_path = temp_dir.path().join("trust");
        let ledger_store_path = temp_dir.path().join("ledger");
        let recovery_store_path = temp_dir.path().join("recovery");

        // Create stores
        let trust_store: Arc<dyn Store> = Arc::new(SledStore::open(&trust_store_path)?);
        let ledger_store: Arc<dyn Store> = Arc::new(SledStore::open(&ledger_store_path)?);
        let recovery_store: Arc<dyn Store> = Arc::new(SledStore::open(&recovery_store_path)?);

        // Create trust graph
        let trust_graph = Arc::new(RwLock::new(TrustGraph::new(trust_store, did.clone())));

        // Create ledger
        let ledger_instance = Ledger::new(ledger_store)?;
        let ledger = Arc::new(RwLock::new(ledger_instance));

        // Create shutdown channel
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(16);

        // Spawn gossip actor with trust lookup
        let trust_graph_clone = trust_graph.clone();
        let trust_lookup = Arc::new(move |peer_did: &icn_identity::Did| {
            let trust = trust_graph_clone.blocking_read();
            match trust.trust_class(peer_did) {
                Ok(class) => Some(class),
                Err(_) => Some(TrustClass::Known), // Default for testing
            }
        });
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        // Subscribe to recovery topic
        {
            let mut gossip = gossip_handle.write().await;
            gossip.subscribe(IDENTITY_RECOVERY_TOPIC, did.clone())?;
        }

        info!("Gossip actor spawned and subscribed to recovery topic");

        // Create incoming message handler that routes gossip and handles recovery
        let gossip_handle_clone = gossip_handle.clone();
        let recovery_store_clone = recovery_store.clone();
        let trust_graph_clone = trust_graph.clone();
        let ledger_clone = ledger.clone();
        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            let gossip_handle = gossip_handle_clone.clone();
            let recovery_store = recovery_store_clone.clone();
            let trust_graph = trust_graph_clone.clone();
            let ledger = ledger_clone.clone();

            tokio::spawn(async move {
                if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                    let sender = net_msg.from.clone();
                    let mut gossip = gossip_handle.write().await;
                    if let Err(e) = gossip.handle_message(&sender, gossip_msg) {
                        warn!("Failed to handle gossip message: {}", e);
                    }
                }
            });
        });

        // Set up gossip notification callback for recovery messages
        let recovery_store_clone = recovery_store.clone();
        let trust_graph_clone = trust_graph.clone();
        let ledger_clone = ledger.clone();
        let notification_callback = Arc::new(
            move |topic: String, entry: GossipEntry, _subscriber_did: icn_identity::Did| {
                if topic != IDENTITY_RECOVERY_TOPIC {
                    return;
                }

                let recovery_store = recovery_store_clone.clone();
                let trust_graph = trust_graph_clone.clone();
                let ledger = ledger_clone.clone();

                // Get entry data (handles decompression)
                let entry_data = match entry.get_data() {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Failed to get entry data: {}", e);
                        return;
                    }
                };

                tokio::spawn(async move {
                    match RecoveryMessage::from_bytes(&entry_data) {
                        Ok(recovery_msg) => {
                            info!("Received recovery message: {}", recovery_msg.summary());

                            // Handle different recovery message types
                            let result: Result<bool> = (|| {
                                match &recovery_msg {
                                    RecoveryMessage::Initiated {
                                        id,
                                        old_did,
                                        new_did,
                                        threshold,
                                        delay_period,
                                        timestamp: _,
                                    } => {
                                        let recovery = RecoveryEvent::new(
                                            old_did.clone(),
                                            new_did.clone(),
                                            *threshold,
                                            *delay_period,
                                        );
                                        let key = format!("recovery:{}", id);
                                        let value = serde_json::to_vec(&recovery)?;
                                        recovery_store.put(key.as_bytes(), &value)?;
                                        info!("Stored new recovery: {} ({} → {})", id, old_did, new_did);
                                        Ok(false)
                                    }
                                    RecoveryMessage::Attestation {
                                        recovery_id,
                                        attestation,
                                        ..
                                    } => {
                                        let key = format!("recovery:{}", recovery_id);
                                        match recovery_store.get(key.as_bytes())? {
                                            Some(data) => {
                                                let mut recovery: RecoveryEvent =
                                                    serde_json::from_slice(&data)?;
                                                match recovery.add_attestation(attestation.clone()) {
                                                    Ok(threshold_reached) => {
                                                        let value = serde_json::to_vec(&recovery)?;
                                                        recovery_store.put(key.as_bytes(), &value)?;
                                                        if threshold_reached {
                                                            info!(
                                                                "Recovery {} reached threshold",
                                                                recovery_id
                                                            );
                                                        }
                                                        Ok(false)
                                                    }
                                                    Err(e) => {
                                                        warn!(
                                                            "Failed to add attestation: {}",
                                                            e
                                                        );
                                                        Err(e)
                                                    }
                                                }
                                            }
                                            None => {
                                                warn!("Unknown recovery: {}", recovery_id);
                                                Ok(false)
                                            }
                                        }
                                    }
                                    RecoveryMessage::Finalized {
                                        id,
                                        old_did,
                                        new_did,
                                        ..
                                    } => {
                                        let key = format!("recovery:{}", id);
                                        match recovery_store.get(key.as_bytes())? {
                                            Some(data) => {
                                                let mut recovery: RecoveryEvent =
                                                    serde_json::from_slice(&data)?;
                                                match recovery.finalize() {
                                                    Ok(_) => {
                                                        let value = serde_json::to_vec(&recovery)?;
                                                        recovery_store.put(key.as_bytes(), &value)?;
                                                        info!(
                                                            "✅ Recovery finalized: {} → {}",
                                                            old_did, new_did
                                                        );
                                                        Ok(true) // Trigger trust/ledger updates
                                                    }
                                                    Err(e) => {
                                                        warn!("Failed to finalize: {}", e);
                                                        Err(e)
                                                    }
                                                }
                                            }
                                            None => {
                                                warn!("Unknown recovery finalization: {}", id);
                                                Ok(false)
                                            }
                                        }
                                    }
                                    RecoveryMessage::Cancelled { .. } => Ok(false),
                                }
                            })();

                            match result {
                                Err(ref e) => {
                                    warn!("Failed to handle recovery message: {}", e);
                                }
                                Ok(true) => {
                                    // Recovery was finalized - update trust and ledger
                                    if let RecoveryMessage::Finalized {
                                        id,
                                        old_did,
                                        new_did,
                                        ..
                                    } = &recovery_msg
                                    {
                                        // Update trust graph
                                        let mut trust = trust_graph.write().await;
                                        match trust.map_did_recovery(old_did, new_did) {
                                            Ok(count) => {
                                                info!(
                                                    "Trust graph: migrated {} edges from {} to {}",
                                                    count, old_did, new_did
                                                );
                                            }
                                            Err(e) => {
                                                warn!("Failed to migrate trust: {}", e);
                                            }
                                        }
                                        drop(trust);

                                        // Update ledger
                                        let mut ledger_guard = ledger.write().await;
                                        match ledger_guard.transfer_balances_for_recovery(
                                            old_did, new_did, id,
                                        ) {
                                            Ok(count) => {
                                                info!(
                                                    "Ledger: transferred {} currencies from {} to {}",
                                                    count, old_did, new_did
                                                );
                                            }
                                            Err(e) => {
                                                warn!("Failed to transfer balances: {}", e);
                                            }
                                        }
                                        drop(ledger_guard);
                                    }
                                }
                                Ok(false) => {
                                    // Message handled but no finalization
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to deserialize recovery message: {}", e);
                        }
                    }
                });
            },
        );

        {
            let mut gossip = gossip_handle.write().await;
            gossip.set_notification_callback(notification_callback);
        }

        // Spawn network actor
        let listen_addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
        let identity_bundle = IdentityBundle::from_keypair(keypair.clone())?;
        let network_handle = NetworkActor::spawn(
            identity_bundle,
            listen_addr,
            shutdown_tx.clone(),
            Some(incoming_handler),
            None, // No trust graph for network actor
            None, // No trust-gated config
            None, // No fallback config
            None, // No topology config
        )
        .await?;

        info!("Network actor spawned on {}", listen_addr);

        // Set up gossip send callback
        let network_handle_clone = network_handle.clone();
        let from_did = did.clone();
        let send_callback = Arc::new(move |recipient_did: Option<icn_identity::Did>, gossip_msg| {
            let network_handle = network_handle_clone.clone();
            let from = from_did.clone();
            tokio::spawn(async move {
                if let Some(recipient) = recipient_did {
                    let net_msg = NetworkMessage::new(from, Some(recipient.clone()), MessagePayload::Gossip(gossip_msg));
                    let _ = network_handle.send_message(recipient, net_msg).await;
                }
            });
        });

        {
            let mut gossip = gossip_handle.write().await;
            gossip.set_send_callback(send_callback);
        }

        Ok(RecoveryTestNode {
            keypair,
            did,
            network_handle,
            gossip_handle,
            trust_graph,
            ledger,
            recovery_store,
            _temp_dir: temp_dir,
            listen_addr,
            shutdown_tx,
        })
    }

    async fn shutdown(self) {
        info!("Shutting down recovery test node");
        let _ = self.shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
#[ignore] // Requires network interfaces and QUIC handshake
async fn test_full_recovery_flow() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Starting full recovery flow integration test ===");

    // Spawn 4 nodes:
    // - Alice (will lose device)
    // - Alice2 (Alice's new identity after device loss)
    // - Bob (trustee #1)
    // - Carol (trustee #2)
    let alice = RecoveryTestNode::spawn(6001).await?;
    let alice2 = RecoveryTestNode::spawn(6002).await?;
    let bob = RecoveryTestNode::spawn(6003).await?;
    let carol = RecoveryTestNode::spawn(6004).await?;

    let alice_did = alice.did.clone();
    let alice2_did = alice2.did.clone();
    let bob_did = bob.did.clone();
    let carol_did = carol.did.clone();

    info!("Alice (original): {}", alice_did);
    info!("Alice2 (new identity): {}", alice2_did);
    info!("Bob (trustee): {}", bob_did);
    info!("Carol (trustee): {}", carol_did);

    // Connect nodes to each other
    info!("Connecting nodes...");
    alice
        .network_handle
        .dial(bob.listen_addr, bob_did.clone())
        .await?;
    alice
        .network_handle
        .dial(carol.listen_addr, carol_did.clone())
        .await?;
    alice2
        .network_handle
        .dial(bob.listen_addr, bob_did.clone())
        .await?;
    alice2
        .network_handle
        .dial(carol.listen_addr, carol_did.clone())
        .await?;

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Set up trust relationships: Alice trusts Bob and Carol
    info!("Setting up trust relationships...");
    {
        let mut trust = alice.trust_graph.write().await;
        trust.add_edge(TrustEdge {
            source: alice_did.clone(),
            target: bob_did.clone(),
            labels: vec!["friend".to_string()],
            score: 0.8,
            evidence: vec![],
            expires_at: None,
            created_at: 1234567890,
        })?;
        trust.add_edge(TrustEdge {
            source: alice_did.clone(),
            target: carol_did.clone(),
            labels: vec!["colleague".to_string()],
            score: 0.7,
            evidence: vec![],
            expires_at: None,
            created_at: 1234567890,
        })?;
    }

    // Alice has some ledger balances
    info!("Creating ledger entries for Alice...");
    {
        let mut ledger = alice.ledger.write().await;
        let entry = JournalEntryBuilder::new(alice_did.clone())
            .credit(alice_did.clone(), "hours".to_string(), 100)
            .debit(bob_did.clone(), "hours".to_string(), 100)
            .build()?;
        ledger.append_entry(entry)?;
    }

    // Verify Alice's initial state
    {
        let ledger = alice.ledger.read().await;
        let balances = ledger.get_account_balances(&alice_did);
        assert_eq!(balances.balances.get("hours"), Some(&100));
        info!("Alice has 100 hours");
    }

    // Alice loses her device and initiates recovery
    info!("Alice initiates recovery...");
    let recovery_id = "test-recovery-1".to_string();
    let recovery = RecoveryEvent::new(alice_did.clone(), alice2_did.clone(), 2, 0); // 2-of-2 threshold, no delay

    // Store recovery on Alice's node
    {
        let key = format!("recovery:{}", recovery_id);
        let value = serde_json::to_vec(&recovery)?;
        alice.recovery_store.put(key.as_bytes(), &value)?;
    }

    // Broadcast recovery initiation
    let recovery_msg = RecoveryMessage::initiated(&recovery);
    let recovery_bytes = recovery_msg.to_bytes()?;
    {
        let mut gossip = alice.gossip_handle.write().await;
        gossip.publish(IDENTITY_RECOVERY_TOPIC, recovery_bytes)?;
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Bob attests to recovery
    info!("Bob attests to recovery...");
    let bob_attestation = RecoveryAttestation::new(
        &bob.keypair,
        alice_did.clone(),
        alice2_did.clone(),
        "Video call verification".to_string(),
    )?;

    // Bob broadcasts attestation
    let attestation_msg = RecoveryMessage::attestation(recovery_id.clone(), bob_attestation.clone());
    let attestation_bytes = attestation_msg.to_bytes()?;
    {
        let mut gossip = bob.gossip_handle.write().await;
        gossip.publish(IDENTITY_RECOVERY_TOPIC, attestation_bytes)?;
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Carol attests to recovery
    info!("Carol attests to recovery...");
    let carol_attestation = RecoveryAttestation::new(
        &carol.keypair,
        alice_did.clone(),
        alice2_did.clone(),
        "In-person verification".to_string(),
    )?;

    let attestation_msg = RecoveryMessage::attestation(recovery_id.clone(), carol_attestation);
    let attestation_bytes = attestation_msg.to_bytes()?;
    {
        let mut gossip = carol.gossip_handle.write().await;
        gossip.publish(IDENTITY_RECOVERY_TOPIC, attestation_bytes)?;
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Update Alice's local recovery with attestations
    {
        let key = format!("recovery:{}", recovery_id);
        let data = alice.recovery_store.get(key.as_bytes())?.unwrap();
        let mut recovery: RecoveryEvent = serde_json::from_slice(&data)?;
        recovery.add_attestation(bob_attestation)?;
        recovery.check_delay_expired(); // No delay configured, so ready immediately
        let value = serde_json::to_vec(&recovery)?;
        alice.recovery_store.put(key.as_bytes(), &value)?;
    }

    // Finalize recovery
    info!("Finalizing recovery...");
    {
        let key = format!("recovery:{}", recovery_id);
        let data = alice.recovery_store.get(key.as_bytes())?.unwrap();
        let mut recovery: RecoveryEvent = serde_json::from_slice(&data)?;
        recovery.finalize()?;
        let value = serde_json::to_vec(&recovery)?;
        alice.recovery_store.put(key.as_bytes(), &value)?;

        // Broadcast finalization
        let finalized_msg = RecoveryMessage::finalized(&recovery)?;
        let finalized_bytes = finalized_msg.to_bytes()?;
        let mut gossip = alice.gossip_handle.write().await;
        gossip.publish(IDENTITY_RECOVERY_TOPIC, finalized_bytes)?;
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify trust graph migration on Alice's node
    info!("Verifying trust graph migration...");
    {
        let trust = alice.trust_graph.read().await;

        // Old DID should have no outgoing edges
        let old_edges = trust.get_outgoing_edges(&alice_did)?;
        assert_eq!(old_edges.len(), 0, "Old DID should have no edges after migration");

        // New DID should have the migrated edges
        let new_edges = trust.get_outgoing_edges(&alice2_did)?;
        assert_eq!(new_edges.len(), 2, "New DID should have 2 migrated edges");

        let targets: Vec<_> = new_edges.iter().map(|e| &e.target).collect();
        assert!(targets.contains(&&bob_did), "Should have edge to Bob");
        assert!(targets.contains(&&carol_did), "Should have edge to Carol");

        info!("✅ Trust graph migration verified");
    }

    // Verify ledger balance transfer on Alice's node
    info!("Verifying ledger balance transfer...");
    {
        let ledger = alice.ledger.read().await;

        // Old DID should have 0 balance
        let old_balances = ledger.get_account_balances(&alice_did);
        assert_eq!(old_balances.balances.get("hours"), Some(&0), "Old DID should have 0 hours");

        // New DID should have the transferred balance
        let new_balances = ledger.get_account_balances(&alice2_did);
        assert_eq!(new_balances.balances.get("hours"), Some(&100), "New DID should have 100 hours");

        info!("✅ Ledger balance transfer verified");
    }

    info!("=== Full recovery flow test completed successfully! ===");

    // Cleanup
    alice.shutdown().await;
    alice2.shutdown().await;
    bob.shutdown().await;
    carol.shutdown().await;

    Ok(())
}
