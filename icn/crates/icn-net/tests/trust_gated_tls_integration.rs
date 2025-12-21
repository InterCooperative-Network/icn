//! Integration tests for trust-gated TLS verification
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! This test validates that the TLS certificate verifier enforces trust-based
//! access control by rejecting connections from peers with insufficient trust.

use anyhow::Result;
use icn_identity::{IdentityBundle, KeyPair};
use icn_net::NetworkActor;
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

/// Test that connections from trusted peers are accepted
#[tokio::test]
#[ignore] // Requires network interfaces and QUIC handshake
async fn test_trusted_peer_connection_accepted() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing for test debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_test_writer()
        .try_init();

    info!("=== Testing trusted peer connection acceptance ==");

    // Create two keypairs
    let alice_keypair = KeyPair::generate()?;
    let bob_keypair = KeyPair::generate()?;
    let alice_did = alice_keypair.did().clone();
    let bob_did = bob_keypair.did().clone();

    info!("Alice DID: {}", alice_did);
    info!("Bob DID: {}", bob_did);

    // Create trust graphs
    let alice_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::temporary()?);
    let mut alice_trust_graph = TrustGraph::new(alice_store, alice_did.clone());

    // Alice trusts Bob with high score (0.8)
    let trust_edge = TrustEdge::new(alice_did.clone(), bob_did.clone(), 0.8);
    alice_trust_graph.add_edge(trust_edge)?;

    let alice_trust = Arc::new(RwLock::new(alice_trust_graph));

    // Create shutdown channels
    let (alice_shutdown_tx, _) = tokio::sync::broadcast::channel(16);

    // Create IdentityBundle for Alice
    let alice_identity_bundle = IdentityBundle::from_keypair(alice_keypair).unwrap();

    // Spawn Alice's network actor with trust-gated TLS (min threshold 0.4 = Partner)
    let _alice_handle = NetworkActor::spawn(
        alice_identity_bundle,
        "127.0.0.1:15400".parse()?,
        alice_shutdown_tx.clone(),
        None,
        Some(alice_trust.clone()),
        None,
        None,
        None,
        None, // No STUN servers for tests
        None, // No TURN config for tests
        None, // No misbehavior detector for tests
    )
    .await?;

    info!("Alice network actor started");

    // Give Alice time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Bob dials Alice - should succeed because Alice trusts Bob with score 0.8
    let bob_shutdown_tx = tokio::sync::broadcast::channel(16).0;
    let bob_identity_bundle = IdentityBundle::from_keypair(bob_keypair).unwrap();
    let bob_handle = NetworkActor::spawn(
        bob_identity_bundle,
        "127.0.0.1:15401".parse()?,
        bob_shutdown_tx.clone(),
        None,
        None, // Bob has no trust graph (doesn't need to verify Alice)
        None,
        None,
        None,
        None, // No STUN servers for tests
        None, // No TURN config for tests
        None, // No misbehavior detector for tests
    )
    .await?;

    info!("Bob network actor started, attempting to dial Alice...");

    // Bob dials Alice
    let result = bob_handle
        .dial("127.0.0.1:15400".parse()?, alice_did.clone())
        .await;

    assert!(
        result.is_ok(),
        "Connection should succeed when peer has sufficient trust (score: 0.8 >= threshold: 0.0)"
    );

    info!("✓ Trusted peer connection accepted");

    // Cleanup
    let _ = alice_shutdown_tx.send(());
    let _ = bob_shutdown_tx.send(());
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(())
}

/// Test that connections from untrusted peers are rejected
#[tokio::test]
#[ignore] // Requires network interfaces and QUIC handshake
async fn test_untrusted_peer_connection_rejected() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_test_writer()
        .try_init();

    info!("=== Testing untrusted peer connection rejection ===");

    // Create two keypairs
    let alice_keypair = KeyPair::generate()?;
    let mallory_keypair = KeyPair::generate()?;
    let alice_did = alice_keypair.did().clone();
    let mallory_did = mallory_keypair.did().clone();

    info!("Alice DID: {}", alice_did);
    info!("Mallory DID: {} (untrusted)", mallory_did);

    // Create Alice's trust graph (does NOT trust Mallory)
    let alice_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::temporary()?);
    let alice_trust_graph = TrustGraph::new(alice_store, alice_did.clone());
    let alice_trust = Arc::new(RwLock::new(alice_trust_graph));

    // Create shutdown channels
    let (alice_shutdown_tx, _) = tokio::sync::broadcast::channel(16);

    // Spawn Alice with trust-gated TLS (min threshold 0.1 = Known)
    use icn_net::rate_limit::TrustGatedRateLimitConfig;
    let alice_identity_bundle = IdentityBundle::from_keypair(alice_keypair).unwrap();
    let _alice_handle = NetworkActor::spawn(
        alice_identity_bundle,
        "127.0.0.1:15500".parse()?,
        alice_shutdown_tx.clone(),
        None,
        Some(alice_trust.clone()),
        Some(TrustGatedRateLimitConfig {
            min_trust_threshold: 0.1, // Reject isolated peers (score < 0.1)
            ..Default::default()
        }),
        None,
        None,
        None, // No STUN servers for tests
        None, // No TURN config for tests
        None, // No misbehavior detector for tests
    )
    .await?;

    info!("Alice network actor started with min_trust_threshold=0.1");

    // Give Alice time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Mallory dials Alice - should FAIL because Alice doesn't trust Mallory
    let mallory_shutdown_tx = tokio::sync::broadcast::channel(16).0;
    let mallory_identity_bundle = IdentityBundle::from_keypair(mallory_keypair).unwrap();
    let mallory_handle = NetworkActor::spawn(
        mallory_identity_bundle,
        "127.0.0.1:15501".parse()?,
        mallory_shutdown_tx.clone(),
        None,
        None,
        None,
        None,
        None,
        None, // No STUN servers for tests
        None, // No TURN config for tests
        None, // No misbehavior detector for tests
    )
    .await?;

    info!("Mallory network actor started, attempting to dial Alice...");

    // Mallory attempts to dial Alice - should be rejected during TLS handshake
    let result = mallory_handle
        .dial("127.0.0.1:15500".parse()?, alice_did.clone())
        .await;

    // The connection should fail because Alice's TLS verifier rejects Mallory
    assert!(
        result.is_err(),
        "Connection should be rejected when peer has insufficient trust (score: 0.0 < threshold: 0.1)"
    );

    info!(
        "✓ Untrusted peer connection rejected: {:?}",
        result.unwrap_err()
    );

    // Cleanup
    let _ = alice_shutdown_tx.send(());
    let _ = mallory_shutdown_tx.send(());
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(())
}

/// Test that trust threshold configuration works correctly
#[tokio::test]
#[ignore] // Requires network interfaces and QUIC handshake
async fn test_trust_threshold_boundary() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_test_writer()
        .try_init();

    info!("=== Testing trust threshold boundary conditions ===");

    // Create keypairs
    let alice_keypair = KeyPair::generate()?;
    let bob_keypair = KeyPair::generate()?;
    let alice_did = alice_keypair.did().clone();
    let bob_did = bob_keypair.did().clone();

    info!("Alice DID: {}", alice_did);
    info!("Bob DID: {}", bob_did);

    // Create Alice's trust graph with Bob at exact threshold (0.4 = Partner minimum)
    let alice_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::temporary()?);
    let mut alice_trust_graph = TrustGraph::new(alice_store, alice_did.clone());
    let trust_edge = TrustEdge::new(alice_did.clone(), bob_did.clone(), 0.4);
    alice_trust_graph.add_edge(trust_edge)?;
    let alice_trust = Arc::new(RwLock::new(alice_trust_graph));

    // Spawn Alice with threshold 0.4
    let (alice_shutdown_tx, _) = tokio::sync::broadcast::channel(16);
    use icn_net::rate_limit::TrustGatedRateLimitConfig;
    let alice_identity_bundle = IdentityBundle::from_keypair(alice_keypair).unwrap();
    let _alice_handle = NetworkActor::spawn(
        alice_identity_bundle,
        "127.0.0.1:15600".parse()?,
        alice_shutdown_tx.clone(),
        None,
        Some(alice_trust.clone()),
        Some(TrustGatedRateLimitConfig {
            min_trust_threshold: 0.4,
            ..Default::default()
        }),
        None,
        None,
        None, // No STUN servers for tests
        None, // No TURN config for tests
        None, // No misbehavior detector for tests
    )
    .await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Bob dials Alice - should succeed at exact threshold
    let bob_shutdown_tx = tokio::sync::broadcast::channel(16).0;
    let bob_identity_bundle = IdentityBundle::from_keypair(bob_keypair).unwrap();
    let bob_handle = NetworkActor::spawn(
        bob_identity_bundle,
        "127.0.0.1:15601".parse()?,
        bob_shutdown_tx.clone(),
        None,
        None,
        None,
        None,
        None,
        None, // No STUN servers for tests
        None, // No TURN config for tests
        None, // No misbehavior detector for tests
    )
    .await?;

    let result = bob_handle
        .dial("127.0.0.1:15600".parse()?, alice_did.clone())
        .await;

    assert!(
        result.is_ok(),
        "Connection should succeed when trust score equals threshold (0.4 >= 0.4)"
    );

    info!("✓ Connection accepted at exact threshold boundary");

    // Cleanup
    let _ = alice_shutdown_tx.send(());
    let _ = bob_shutdown_tx.send(());
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(())
}
