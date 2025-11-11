//! Integration test for ledger synchronization via gossip

use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_ledger::{entry::JournalEntryBuilder, Ledger, LedgerSyncMessage};
use icn_store::SledStore;
use icn_trust::TrustClass;
use std::sync::Arc;
use tempfile::TempDir;

/// Create a test ledger with gossip integration
fn create_test_node() -> (Ledger, TempDir, Arc<tokio::sync::RwLock<GossipActor>>) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    // Create gossip actor
    let trust_lookup = Arc::new(|_: &icn_identity::Did| Some(TrustClass::Partner));
    let gossip = GossipActor::spawn(did.clone(), trust_lookup);

    // Create ledger with gossip
    let mut ledger = Ledger::new(store).unwrap();
    ledger.set_gossip(gossip.clone());

    (ledger, temp_dir, gossip)
}

/// Create a test ledger WITHOUT gossip integration
fn create_simple_ledger() -> (Ledger, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
    let ledger = Ledger::new(store).unwrap();
    (ledger, temp_dir)
}

#[test]
fn test_direct_sync_message() {
    // Create two simple ledgers without gossip
    let (mut ledger1, _temp1) = create_simple_ledger();
    let (mut ledger2, _temp2) = create_simple_ledger();

    let alice = KeyPair::generate().unwrap().did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();

    // Create entry on ledger1
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();
    ledger1.append_entry(entry.clone()).unwrap();

    // Verify ledger1 has it
    assert!(ledger1.get_entry(&hash).unwrap().is_some());
    assert_eq!(ledger1.get_balance(&alice, "hours"), 10);

    // Manually create sync message and send to ledger2
    let sync_msg = LedgerSyncMessage::NewEntry {
        hash: hash.clone(),
        entry,
    };

    ledger2.handle_sync_message(sync_msg).unwrap();

    // Verify ledger2 now has it
    assert!(ledger2.get_entry(&hash).unwrap().is_some());
    assert_eq!(ledger2.get_balance(&alice, "hours"), 10);
    assert_eq!(ledger2.get_balance(&bob, "hours"), -10);

    println!("✓ Direct sync message handling successful");
}

#[test]
fn test_ledger_publishes_to_gossip() {
    // Create a node with gossip integration
    let (mut ledger, _temp, gossip) = create_test_node();

    let alice = KeyPair::generate().unwrap().did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();

    // Create and append an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();
    ledger.append_entry(entry.clone()).unwrap();

    // Verify ledger has the entry
    assert!(ledger.get_entry(&hash).unwrap().is_some());

    // Verify the entry was published to gossip
    let gossip_actor = gossip.blocking_read();
    let entries = gossip_actor.get_entries("ledger:hours");

    assert_eq!(entries.len(), 1, "Expected 1 entry in gossip");

    // Verify we can deserialize the gossip entry data as a LedgerSyncMessage
    let gossip_entry = &entries[0];
    let sync_msg: LedgerSyncMessage =
        serde_json::from_slice(&gossip_entry.data).expect("Failed to deserialize");

    match sync_msg {
        LedgerSyncMessage::NewEntry {
            hash: msg_hash,
            entry: _msg_entry,
        } => {
            assert_eq!(msg_hash, hash, "Hash in sync message should match");
        }
        _ => panic!("Expected NewEntry message"),
    }

    println!("✓ Ledger publishes to gossip successfully");
}

#[test]
fn test_multiple_entries_to_gossip() {
    // Create a node with gossip
    let (mut ledger, _temp, gossip) = create_test_node();

    let alice = KeyPair::generate().unwrap().did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();
    let charlie = KeyPair::generate().unwrap().did().clone();

    // Create multiple entries
    let entry1 = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let entry2 = JournalEntryBuilder::new(bob.clone())
        .debit(bob.clone(), "hours".to_string(), 5)
        .credit(charlie.clone(), "hours".to_string(), 5)
        .build()
        .unwrap();

    // Append both entries
    ledger.append_entry(entry1).unwrap();
    ledger.append_entry(entry2).unwrap();

    // Verify balances
    assert_eq!(ledger.get_balance(&alice, "hours"), 10);
    assert_eq!(ledger.get_balance(&bob, "hours"), -5); // -10 + 5
    assert_eq!(ledger.get_balance(&charlie, "hours"), -5);

    // Verify both entries were published to gossip
    let gossip_actor = gossip.blocking_read();
    let entries = gossip_actor.get_entries("ledger:hours");

    assert_eq!(entries.len(), 2, "Expected 2 entries in gossip");

    println!("✓ Multiple entries published to gossip successfully");
}

#[test]
fn test_duplicate_entry_handling() {
    // Create a simple ledger
    let (mut ledger, _temp) = create_simple_ledger();

    let alice = KeyPair::generate().unwrap().did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();

    // Create and append an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();
    ledger.append_entry(entry.clone()).unwrap();

    // Try to handle the same entry again via sync message
    let sync_msg = LedgerSyncMessage::NewEntry {
        hash: hash.clone(),
        entry: entry.clone(),
    };

    // This should succeed but not duplicate the entry
    ledger.handle_sync_message(sync_msg).unwrap();

    // Verify balance is still correct (not doubled)
    assert_eq!(ledger.get_balance(&alice, "hours"), 10);
    assert_eq!(ledger.get_balance(&bob, "hours"), -10);

    // Verify only one entry in the ledger
    let all_entries = ledger.get_all_entries().unwrap();
    assert_eq!(all_entries.len(), 1);

    println!("✓ Duplicate entry handling successful");
}
