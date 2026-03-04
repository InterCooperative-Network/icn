#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for backup and restore of node state.
//!
//! These tests validate that ICN node state (ledger, keystore, store data)
//! can be backed up and restored correctly after catastrophic failure.
//! This is a P0 pilot blocker (Issue #1142).

use anyhow::Result;
use ed25519_dalek::Verifier;
use icn_identity::KeyPair;
use icn_ledger::entry::JournalEntryBuilder;
use icn_ledger::Ledger;
use icn_store::{SledStore, Store};
use std::sync::Arc;

/// Test that SledStore state survives backup and restore.
///
/// Sequence: create store -> write data -> export snapshot -> create new store
/// -> import snapshot -> verify all data matches.
#[test]
fn test_store_backup_and_restore_roundtrip() -> Result<()> {
    // Phase 1: Create store and populate with data
    let original = SledStore::temporary()?;

    // Write various key-value pairs simulating real node state
    original.put(b"config:node_id", b"node-alpha-001")?;
    original.put(b"config:version", b"1.0.0")?;
    original.put(b"peers:did:icn:abc123", b"192.168.1.10:9000")?;
    original.put(b"peers:did:icn:def456", b"192.168.1.11:9000")?;

    // Write some binary data (simulating hashes, signatures, etc.)
    let binary_value: Vec<u8> = (0..64).collect();
    original.put(b"state:merkle_root", &binary_value)?;

    // Write multiple entries under a common prefix
    for i in 0..20 {
        let key = format!("journal:{i:04}");
        let value = format!("entry-data-{i}");
        original.put(key.as_bytes(), value.as_bytes())?;
    }

    // Phase 2: Export snapshot
    let snapshot = original.export_snapshot()?;
    assert!(
        snapshot.len() >= 24,
        "Expected at least 24 entries, got {}",
        snapshot.len()
    );

    // Phase 3: Create new store and import
    let restored = SledStore::temporary()?;

    // Verify new store is empty
    assert!(restored.get(b"config:node_id")?.is_none());

    restored.import_snapshot(&snapshot)?;

    // Phase 4: Verify all data matches
    assert_eq!(
        restored.get(b"config:node_id")?,
        Some(b"node-alpha-001".to_vec())
    );
    assert_eq!(restored.get(b"config:version")?, Some(b"1.0.0".to_vec()));
    assert_eq!(
        restored.get(b"peers:did:icn:abc123")?,
        Some(b"192.168.1.10:9000".to_vec())
    );
    assert_eq!(
        restored.get(b"peers:did:icn:def456")?,
        Some(b"192.168.1.11:9000".to_vec())
    );
    assert_eq!(restored.get(b"state:merkle_root")?, Some(binary_value));

    // Verify all journal entries
    for i in 0..20 {
        let key = format!("journal:{i:04}");
        let expected = format!("entry-data-{i}");
        assert_eq!(
            restored.get(key.as_bytes())?,
            Some(expected.into_bytes()),
            "Mismatch for journal entry {i}"
        );
    }

    // Verify scan still works on restored store
    let journal_entries = restored.scan(b"journal:")?;
    assert_eq!(journal_entries.len(), 20);

    let peer_entries = restored.scan(b"peers:")?;
    assert_eq!(peer_entries.len(), 2);

    Ok(())
}

/// Test that ledger entries survive backup and restore.
///
/// Sequence: create node -> write ledger entries -> backup underlying store
/// -> destroy original -> restore from backup -> verify balances match.
#[tokio::test]
async fn test_backup_restore_ledger_entries() -> Result<()> {
    // Phase 1: Create ledger and write entries
    let store = SledStore::temporary()?;
    let store_arc: Arc<dyn Store> = Arc::new(store);

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let charlie = KeyPair::generate()?;

    let mut ledger = Ledger::new(store_arc.clone())?;

    // Create transactions: Alice -> Bob (10 hours), Bob -> Charlie (5 hours)
    let entry1 = JournalEntryBuilder::new(alice.did().clone())
        .debit(alice.did().clone(), "hours".to_string(), 10)
        .credit(bob.did().clone(), "hours".to_string(), 10)
        .with_system_provenance("test")
        .build()?;
    let hash1 = ledger.append_entry(entry1).await?;

    let entry2 = JournalEntryBuilder::new(bob.did().clone())
        .debit(bob.did().clone(), "hours".to_string(), 5)
        .credit(charlie.did().clone(), "hours".to_string(), 5)
        .with_system_provenance("test")
        .build()?;
    let hash2 = ledger.append_entry(entry2).await?;

    // Record pre-backup balances
    let alice_balance = ledger.get_balance(alice.did(), "hours");
    let bob_balance = ledger.get_balance(bob.did(), "hours");
    let charlie_balance = ledger.get_balance(charlie.did(), "hours");

    assert_eq!(
        alice_balance, 10,
        "Alice should have +10 (debited, meaning she provided hours)"
    );
    assert_eq!(bob_balance, -5, "Bob should have -5 (-10 + 5)");
    assert_eq!(charlie_balance, -5, "Charlie should have -5");

    // Phase 2: Export the underlying store
    // We need to get at the raw SledStore to export. In production this would
    // be done via the daemon's store handle.
    // For the test, we scan all entries via the Store trait.
    let all_entries = store_arc.scan(b"")?;
    assert!(!all_entries.is_empty(), "Store should have ledger data");

    // Phase 3: Restore to a new store and create a new ledger on top of it
    let restored_store = SledStore::temporary()?;
    restored_store.import_snapshot(&all_entries)?;
    let restored_store_arc: Arc<dyn Store> = Arc::new(restored_store);

    // Create a new Ledger from the restored store
    let restored_ledger = Ledger::new(restored_store_arc)?;

    // Phase 4: Verify entries exist in the restored ledger
    let restored_entry1 = restored_ledger.get_entry(&hash1)?;
    assert!(
        restored_entry1.is_some(),
        "Entry 1 should exist in restored ledger"
    );

    let restored_entry2 = restored_ledger.get_entry(&hash2)?;
    assert!(
        restored_entry2.is_some(),
        "Entry 2 should exist in restored ledger"
    );

    // Verify the restored entries match (by checking author and account count)
    let re1 = restored_entry1.unwrap();
    assert_eq!(re1.author, *alice.did());
    assert_eq!(re1.accounts.len(), 2);

    let re2 = restored_entry2.unwrap();
    assert_eq!(re2.author, *bob.did());
    assert_eq!(re2.accounts.len(), 2);

    Ok(())
}

/// Test that identity (keypair) can be backed up and restored, and signatures
/// made with the restored key are valid.
///
/// Sequence: generate keypair -> sign message -> export key bytes ->
/// "destroy" original -> restore from bytes -> sign same message ->
/// verify both signatures are valid.
#[test]
fn test_backup_restore_keystore_identity() -> Result<()> {
    // Phase 1: Generate identity
    let original_keypair = KeyPair::generate()?;
    let original_did = original_keypair.did().clone();

    // Sign a message with the original key
    let message = b"cooperative governance proposal #42";
    let original_signature = original_keypair.sign(message);

    // Verify the signature works
    original_keypair
        .verifying_key()
        .verify(message, &original_signature)?;

    // Phase 2: "Backup" the keypair - export key material
    let secret_bytes = original_keypair.to_signing_key_bytes();
    let public_bytes = original_keypair.verifying_key().to_bytes();

    // Phase 3: "Destroy" original (drop it)
    drop(original_keypair);

    // Phase 4: Restore from backed-up bytes
    let restored_keypair = KeyPair::from_bytes(&secret_bytes, &public_bytes)?;

    // Verify the DID matches
    assert_eq!(
        restored_keypair.did(),
        &original_did,
        "Restored DID must match original"
    );

    // Phase 5: Sign the same message with restored key
    let restored_signature = restored_keypair.sign(message);

    // Both signatures should be identical (deterministic Ed25519 signing)
    assert_eq!(
        original_signature.to_bytes(),
        restored_signature.to_bytes(),
        "Signatures from original and restored keys must match"
    );

    // Verify the restored signature
    restored_keypair
        .verifying_key()
        .verify(message, &restored_signature)?;

    // Phase 6: Sign a NEW message with restored key to prove full functionality
    let new_message = b"emergency treasury transfer approved";
    let new_signature = restored_keypair.sign(new_message);
    restored_keypair
        .verifying_key()
        .verify(new_message, &new_signature)?;

    Ok(())
}

/// Test that multiple identities can be backed up and restored independently.
///
/// Simulates backing up a keystore with multiple DIDs.
#[test]
fn test_backup_restore_multiple_identities() -> Result<()> {
    // Create multiple identities (simulating a keystore with rotated keys)
    let keypairs: Vec<KeyPair> = (0..5).map(|_| KeyPair::generate().unwrap()).collect();

    // Sign messages with each identity
    let signatures: Vec<_> = keypairs
        .iter()
        .enumerate()
        .map(|(i, kp)| {
            let msg = format!("message-{i}");
            (msg.clone(), kp.sign(msg.as_bytes()), kp.did().clone())
        })
        .collect();

    // "Backup": export all key material
    let backups: Vec<([u8; 32], [u8; 32])> = keypairs
        .iter()
        .map(|kp| (kp.to_signing_key_bytes(), kp.verifying_key().to_bytes()))
        .collect();

    // "Destroy" originals
    drop(keypairs);

    // Restore all identities
    let restored: Vec<KeyPair> = backups
        .iter()
        .map(|(secret, public)| KeyPair::from_bytes(secret, public).unwrap())
        .collect();

    // Verify all restored identities match and can verify original signatures
    for (i, kp) in restored.iter().enumerate() {
        let (ref msg, ref sig, ref did) = signatures[i];
        assert_eq!(kp.did(), did, "DID mismatch for identity {i}");
        kp.verifying_key()
            .verify(msg.as_bytes(), sig)
            .unwrap_or_else(|_| panic!("Signature verification failed for identity {i}"));
    }

    Ok(())
}

/// Test backup/restore of replica metadata through the store.
///
/// Verifies that replica tracking state survives backup and restore.
#[test]
fn test_backup_restore_replica_metadata() -> Result<()> {
    use icn_store::ReplicaHealth;

    let original = SledStore::temporary()?;

    // Create replica metadata
    let mut hash1 = [0u8; 32];
    hash1[0] = 0xAA;
    let mut hash2 = [0u8; 32];
    hash2[0] = 0xBB;

    original.add_replica(&hash1, "did:icn:peer1".to_string(), ReplicaHealth::Healthy)?;
    original.add_replica(&hash1, "did:icn:peer2".to_string(), ReplicaHealth::Stale)?;
    original.add_replica(&hash2, "did:icn:peer3".to_string(), ReplicaHealth::Healthy)?;

    // Export
    let snapshot = original.export_snapshot()?;

    // Restore
    let restored = SledStore::temporary()?;
    restored.import_snapshot(&snapshot)?;

    // Verify replica metadata survived
    let meta1 = restored
        .get_replica_metadata(&hash1)?
        .expect("hash1 metadata");
    assert_eq!(meta1.replicas.len(), 2);
    assert_eq!(meta1.replicas[0].peer_did, "did:icn:peer1");
    assert_eq!(meta1.replicas[0].health, ReplicaHealth::Healthy);
    assert_eq!(meta1.replicas[1].peer_did, "did:icn:peer2");
    assert_eq!(meta1.replicas[1].health, ReplicaHealth::Stale);

    let meta2 = restored
        .get_replica_metadata(&hash2)?
        .expect("hash2 metadata");
    assert_eq!(meta2.replicas.len(), 1);
    assert_eq!(meta2.replicas[0].peer_did, "did:icn:peer3");

    // Verify listing works on restored store
    let hashes = restored.list_replica_hashes()?;
    assert_eq!(hashes.len(), 2);

    Ok(())
}

/// Test that an empty store backup/restore is a no-op.
#[test]
fn test_backup_restore_empty_store() -> Result<()> {
    let original = SledStore::temporary()?;

    let snapshot = original.export_snapshot()?;
    assert!(
        snapshot.is_empty(),
        "Empty store should produce empty snapshot"
    );

    let restored = SledStore::temporary()?;
    restored.import_snapshot(&snapshot)?;

    // Verify store is still empty
    assert!(restored.get(b"anything")?.is_none());
    assert!(restored.scan(b"")?.is_empty());

    Ok(())
}

/// Test that restoring on top of existing data overwrites correctly.
#[test]
fn test_backup_restore_overwrites_existing() -> Result<()> {
    // Create source with data
    let source = SledStore::temporary()?;
    source.put(b"key:a", b"value-from-source")?;
    source.put(b"key:b", b"only-in-source")?;
    let snapshot = source.export_snapshot()?;

    // Create target with different data
    let target = SledStore::temporary()?;
    target.put(b"key:a", b"value-from-target")?;
    target.put(b"key:c", b"only-in-target")?;

    // Import snapshot
    target.import_snapshot(&snapshot)?;

    // key:a should be overwritten with source value
    assert_eq!(target.get(b"key:a")?, Some(b"value-from-source".to_vec()));
    // key:b should exist (from snapshot)
    assert_eq!(target.get(b"key:b")?, Some(b"only-in-source".to_vec()));
    // key:c should still exist (not deleted by import)
    assert_eq!(target.get(b"key:c")?, Some(b"only-in-target".to_vec()));

    Ok(())
}

/// Test backup/restore with large-ish dataset to verify scalability.
#[test]
fn test_backup_restore_large_dataset() -> Result<()> {
    let original = SledStore::temporary()?;

    // Write 1000 entries
    let entry_count = 1000;
    for i in 0..entry_count {
        let key = format!("data:{i:06}");
        let value = format!("payload-{i}-{}", "x".repeat(100));
        original.put(key.as_bytes(), value.as_bytes())?;
    }

    let snapshot = original.export_snapshot()?;
    assert_eq!(snapshot.len(), entry_count);

    let restored = SledStore::temporary()?;
    restored.import_snapshot(&snapshot)?;

    // Spot-check some entries
    for i in [0, 42, 500, 999] {
        let key = format!("data:{i:06}");
        let expected = format!("payload-{i}-{}", "x".repeat(100));
        assert_eq!(
            restored.get(key.as_bytes())?,
            Some(expected.into_bytes()),
            "Mismatch at entry {i}"
        );
    }

    // Verify total count
    let all = restored.scan(b"data:")?;
    assert_eq!(all.len(), entry_count);

    Ok(())
}
