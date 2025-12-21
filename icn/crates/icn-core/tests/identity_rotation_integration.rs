#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Identity Rotation Integration Test (Issue #121)
//!
//! Tests proactive identity key rotation and its effects on:
//! - Trust graph edge updates (old DID -> new DID)
//! - Ledger balance migration
//!
//! This differs from recovery (which handles lost keys) - rotation is
//! a planned key change while still having access to the old key.

use anyhow::Result;
use icn_identity::{Did, KeyPair};
use icn_ledger::{entry::JournalEntryBuilder, Ledger};
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::info;

/// Helper to get balance for a DID from ledger
async fn get_balance(ledger: &RwLock<Ledger>, did: &Did, currency: &str) -> i64 {
    let ledger = ledger.read().await;
    ledger.get_account_balances(did).get(currency)
}

/// Test node with identity, trust graph, and ledger
struct RotationTestNode {
    #[allow(dead_code)]
    keypair: KeyPair,
    did: Did,
    trust_graph: Arc<RwLock<TrustGraph>>,
    ledger: Arc<RwLock<Ledger>>,
    _temp_dir: TempDir,
}

impl RotationTestNode {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        // Create trust graph store
        let trust_path = temp_dir.path().join("trust");
        let trust_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&trust_path)?);
        let trust_graph = Arc::new(RwLock::new(TrustGraph::new(trust_store, did.clone())));

        // Create ledger store
        let ledger_path = temp_dir.path().join("ledger");
        let ledger_store = Arc::new(SledStore::open(&ledger_path)?);
        let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store)?));

        Ok(Self {
            keypair,
            did,
            trust_graph,
            ledger,
            _temp_dir: temp_dir,
        })
    }

    /// Rotate to a new keypair, returning the new DID
    fn rotate_identity(&mut self) -> Result<Did> {
        let new_keypair = KeyPair::generate()?;
        let new_did = new_keypair.did().clone();
        let old_did = self.did.clone();

        info!("Rotating identity: {} -> {}", old_did, new_did);

        self.keypair = new_keypair;
        self.did = new_did.clone();

        Ok(new_did)
    }
}

/// Test that trust edges can be added and queried for rotation migration
#[tokio::test]
async fn test_trust_edge_migration_on_rotation() -> Result<()> {
    // Create Alice and Bob
    let mut alice = RotationTestNode::new()?;
    let bob = RotationTestNode::new()?;

    let alice_old_did = alice.did.clone();

    // Bob records a trust edge for Alice in his graph
    {
        let mut bob_trust = bob.trust_graph.write().await;
        bob_trust.add_edge(TrustEdge::new(bob.did.clone(), alice_old_did.clone(), 0.9))?;
    }

    // Alice rotates her identity
    let alice_new_did = alice.rotate_identity()?;

    // Bob adds a trust edge for Alice's new DID
    {
        let mut bob_trust = bob.trust_graph.write().await;
        bob_trust.add_edge(TrustEdge::new(bob.did.clone(), alice_new_did.clone(), 0.9))?;
    }

    // Verify both edges exist in Bob's trust graph
    // (In production, the old DID would have reduced trust after rotation announcement)
    {
        let bob_trust = bob.trust_graph.read().await;

        // Check edge to old DID exists
        let old_edge = bob_trust.get_edge(&bob.did, &alice_old_did)?;
        assert!(old_edge.is_some(), "Edge to old DID should exist");

        // Check edge to new DID exists
        let new_edge = bob_trust.get_edge(&bob.did, &alice_new_did)?;
        assert!(new_edge.is_some(), "Edge to new DID should exist");
        assert!(
            (new_edge.unwrap().score - 0.9).abs() < 0.001,
            "New edge should have correct score"
        );
    }

    info!("Trust edge migration test passed");
    Ok(())
}

/// Test that ledger balances can be migrated on rotation
#[tokio::test]
async fn test_ledger_balance_migration_on_rotation() -> Result<()> {
    let mut alice = RotationTestNode::new()?;
    let bank = RotationTestNode::new()?;

    let alice_old_did = alice.did.clone();

    // Give Alice some balance (bank credits Alice, debits itself)
    {
        let mut ledger = alice.ledger.write().await;
        let entry = JournalEntryBuilder::new(bank.did.clone())
            .credit(alice_old_did.clone(), "hours".to_string(), 100)
            .debit(bank.did.clone(), "hours".to_string(), 100)
            .build()?;
        ledger.append_entry(entry)?;
    }

    // Verify initial balance
    let alice_balance = get_balance(&alice.ledger, &alice_old_did, "hours").await;
    assert_eq!(alice_balance, -100, "Alice should have -100 hours (credit)");

    // Alice rotates her identity
    let alice_new_did = alice.rotate_identity()?;

    // Transfer balance from old DID to new DID
    // This would normally be signed by the old key to prove ownership
    {
        let mut ledger = alice.ledger.write().await;
        let entry = JournalEntryBuilder::new(alice_old_did.clone()) // Signed by old key
            .debit(alice_old_did.clone(), "hours".to_string(), 100)
            .credit(alice_new_did.clone(), "hours".to_string(), 100)
            .build()?;
        ledger.append_entry(entry)?;
    }

    // Verify balance migrated
    let old_balance = get_balance(&alice.ledger, &alice_old_did, "hours").await;
    let new_balance = get_balance(&alice.ledger, &alice_new_did, "hours").await;

    assert_eq!(old_balance, 0, "Old DID should have 0 hours");
    assert_eq!(new_balance, -100, "New DID should have -100 hours (credit)");

    info!("Ledger balance migration test passed");
    Ok(())
}

/// Test complete rotation flow: trust + ledger
#[tokio::test]
async fn test_complete_rotation_flow() -> Result<()> {
    let mut alice = RotationTestNode::new()?;
    let bob = RotationTestNode::new()?;
    let charlie = RotationTestNode::new()?;

    let alice_old_did = alice.did.clone();

    // Setup: Bob and Charlie both add trust edges for Alice
    for node in [&bob, &charlie] {
        let mut trust = node.trust_graph.write().await;
        trust.add_edge(TrustEdge::new(node.did.clone(), alice_old_did.clone(), 0.9))?;
    }

    // Setup: Alice has balance
    {
        let mut ledger = alice.ledger.write().await;
        let entry = JournalEntryBuilder::new(bob.did.clone())
            .credit(alice_old_did.clone(), "credits".to_string(), 50)
            .debit(bob.did.clone(), "credits".to_string(), 50)
            .build()?;
        ledger.append_entry(entry)?;
    }

    // Alice rotates
    let alice_new_did = alice.rotate_identity()?;

    // In production, Alice would:
    // 1. Create a signed rotation announcement (signed by old key)
    // 2. Broadcast via gossip
    // 3. Each node would verify and update their trust graphs

    // Simulate each node adding trust edge for new DID
    for node in [&bob, &charlie] {
        let mut trust = node.trust_graph.write().await;
        trust.add_edge(TrustEdge::new(node.did.clone(), alice_new_did.clone(), 0.9))?;
    }

    // Migrate ledger balance
    {
        let mut ledger = alice.ledger.write().await;
        let entry = JournalEntryBuilder::new(alice_old_did.clone())
            .debit(alice_old_did.clone(), "credits".to_string(), 50)
            .credit(alice_new_did.clone(), "credits".to_string(), 50)
            .build()?;
        ledger.append_entry(entry)?;
    }

    // Verify final state - trust edges exist
    for node in [&bob, &charlie] {
        let trust = node.trust_graph.read().await;
        let edge = trust.get_edge(&node.did, &alice_new_did)?;
        assert!(edge.is_some(), "Edge to new DID should exist");
    }

    // Verify final state - balance migrated
    let new_balance = get_balance(&alice.ledger, &alice_new_did, "credits").await;
    assert_eq!(new_balance, -50, "Balance should be on new DID");

    info!("Complete rotation flow test passed");
    Ok(())
}
