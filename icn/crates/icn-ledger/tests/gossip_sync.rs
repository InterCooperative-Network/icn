//! Integration test for ledger synchronization via gossip
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_kernel_api::authz::{ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest};
use icn_ledger::{
    entry::JournalEntryBuilder, Ledger, LedgerSyncMessage, WitnessConfig, WitnessPolicy,
    WitnessSignature, WitnessedEntry,
};
use icn_store::SledStore;
use std::sync::Arc;
use tempfile::TempDir;

struct MockPolicyOracle;
impl PolicyOracle for MockPolicyOracle {
    fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
        let mut constraints = ConstraintSet::default();
        // Mimic Partner trust settings
        constraints = constraints
            .with_max_subscriptions(500)
            .with_max_outstanding_requests(3)
            .with_max_message_size(1024 * 1024);
        constraints
            .custom
            .insert("trust_score".to_string(), 0.5.into());
        PolicyDecision::Allow { constraints }
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }
}

/// Create a test ledger with gossip integration
async fn create_test_node() -> (Ledger, TempDir, Arc<tokio::sync::RwLock<GossipActor>>) {
    use icn_gossip::{AccessControl, Topic};

    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    // Create gossip actor
    let oracle = Arc::new(MockPolicyOracle);
    let gossip = GossipActor::spawn(did.clone(), Some(oracle));

    // Create the ledger topic before use (required with strict access control defaults)
    {
        let mut actor = gossip.write().await;
        let topic = Topic::new(
            "ledger:hours".to_string(),
            AccessControl::MinTrustScore(0.1),
        );
        actor.create_topic(topic);
    }

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

#[tokio::test]
async fn test_direct_sync_message() {
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
    ledger1.append_entry(entry.clone()).await.unwrap();

    // Verify ledger1 has it
    assert!(ledger1.get_entry(&hash).unwrap().is_some());
    assert_eq!(ledger1.get_balance(&alice, "hours"), 10);

    // Manually create sync message and send to ledger2
    let sync_msg = LedgerSyncMessage::NewEntry {
        hash: hash.clone(),
        entry,
    };

    ledger2.handle_sync_message(sync_msg).await.unwrap();

    // Verify ledger2 now has it
    assert!(ledger2.get_entry(&hash).unwrap().is_some());
    assert_eq!(ledger2.get_balance(&alice, "hours"), 10);
    assert_eq!(ledger2.get_balance(&bob, "hours"), -10);

    println!("Direct sync message handling successful");
}

#[tokio::test]
async fn test_ledger_publishes_to_gossip() {
    // Create a node with gossip integration
    let (mut ledger, _temp, gossip) = create_test_node().await;

    let alice = KeyPair::generate().unwrap().did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();

    // Create and append an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();
    ledger.append_entry(entry.clone()).await.unwrap();

    // Verify ledger has the entry
    assert!(ledger.get_entry(&hash).unwrap().is_some());

    // Verify the entry was published to gossip
    let gossip_actor = gossip.read().await;
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

    println!("Ledger publishes to gossip successfully");
}

#[tokio::test]
async fn test_multiple_entries_to_gossip() {
    // Create a node with gossip
    let (mut ledger, _temp, gossip) = create_test_node().await;

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
    ledger.append_entry(entry1).await.unwrap();
    ledger.append_entry(entry2).await.unwrap();

    // Verify balances
    assert_eq!(ledger.get_balance(&alice, "hours"), 10);
    assert_eq!(ledger.get_balance(&bob, "hours"), -5); // -10 + 5
    assert_eq!(ledger.get_balance(&charlie, "hours"), -5);

    // Verify both entries were published to gossip
    let gossip_actor = gossip.read().await;
    let entries = gossip_actor.get_entries("ledger:hours");

    assert_eq!(entries.len(), 2, "Expected 2 entries in gossip");

    println!("Multiple entries published to gossip successfully");
}

#[tokio::test]
async fn test_duplicate_entry_handling() {
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
    ledger.append_entry(entry.clone()).await.unwrap();

    // Try to handle the same entry again via sync message
    let sync_msg = LedgerSyncMessage::NewEntry {
        hash: hash.clone(),
        entry: entry.clone(),
    };

    // This should succeed but not duplicate the entry
    ledger.handle_sync_message(sync_msg).await.unwrap();

    // Verify balance is still correct (not doubled)
    assert_eq!(ledger.get_balance(&alice, "hours"), 10);
    assert_eq!(ledger.get_balance(&bob, "hours"), -10);

    // Verify only one entry in the ledger
    let all_entries = ledger.get_all_entries().unwrap();
    assert_eq!(all_entries.len(), 1);

    println!("Duplicate entry handling successful");
}

// ============================================================================
// Witness Signature Tests (Issue #676)
// ============================================================================

#[tokio::test]
async fn test_witnessed_entry_with_valid_signature() {
    // Create a simple ledger with witness config
    let (mut ledger, _temp) = create_simple_ledger();
    ledger.set_witness_config(WitnessConfig::counterparty_above(0)); // Require witness for all

    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create witness signature from Bob
    let signature = bob_kp.sign(hash.as_bytes());
    let witness_sig = WitnessSignature::new(
        bob.clone(),
        signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    // Create witnessed entry with counterparty policy
    let mut witnessed = WitnessedEntry::new(entry, WitnessPolicy::Counterparty);
    witnessed.add_signature(witness_sig);

    // Append should succeed
    let result_hash = ledger.append_witnessed_entry(witnessed).await.unwrap();
    assert_eq!(result_hash, hash);

    // Verify entry was stored
    assert!(ledger.get_entry(&hash).unwrap().is_some());
    assert_eq!(ledger.get_balance(&alice, "hours"), 10);

    println!("Witnessed entry with valid signature accepted");
}

#[tokio::test]
async fn test_witnessed_entry_with_invalid_signature() {
    // Create a simple ledger
    let (mut ledger, _temp) = create_simple_ledger();
    ledger.set_witness_config(WitnessConfig::counterparty_above(0));

    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let wrong_kp = KeyPair::generate().unwrap(); // Different key
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create signature with wrong key but claim it's from Bob
    let wrong_signature = wrong_kp.sign(hash.as_bytes());
    let witness_sig = WitnessSignature::new(
        bob.clone(), // Claims to be Bob
        wrong_signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    let mut witnessed = WitnessedEntry::new(entry, WitnessPolicy::Counterparty);
    witnessed.add_signature(witness_sig);

    // Append should fail due to invalid signature
    let result = ledger.append_witnessed_entry(witnessed).await;
    assert!(result.is_err(), "Expected error for invalid signature");

    // Verify entry was NOT stored
    assert!(ledger.get_entry(&hash).unwrap().is_none());

    println!("Witnessed entry with invalid signature rejected");
}

#[tokio::test]
async fn test_witnessed_entry_insufficient_signatures() {
    let (mut ledger, _temp) = create_simple_ledger();

    // Require 2-of-3 quorum
    let witness1 = KeyPair::generate().unwrap().did().clone();
    let witness2 = KeyPair::generate().unwrap().did().clone();
    let witness3 = KeyPair::generate().unwrap().did().clone();

    ledger.set_witness_config(WitnessConfig::quorum(
        2,
        vec![witness1.clone(), witness2.clone(), witness3.clone()],
    ));

    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    // Create witnessed entry with only one signature (need 2)
    let witnessed = WitnessedEntry::new(
        entry,
        WitnessPolicy::Quorum {
            required: 2,
            witnesses: vec![witness1, witness2, witness3],
        },
    );
    // No signatures added

    // Should fail due to insufficient signatures
    let result = ledger.append_witnessed_entry(witnessed).await;
    assert!(
        result.is_err(),
        "Expected error for insufficient signatures"
    );

    println!("Witnessed entry with insufficient signatures rejected");
}

#[tokio::test]
async fn test_witnessed_entry_sync_message() {
    // Create two simple ledgers
    let (mut ledger1, _temp1) = create_simple_ledger();
    let (mut ledger2, _temp2) = create_simple_ledger();

    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create witness signature from Bob
    let signature = bob_kp.sign(hash.as_bytes());
    let witness_sig = WitnessSignature::new(
        bob.clone(),
        signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    let mut witnessed = WitnessedEntry::new(entry, WitnessPolicy::Counterparty);
    witnessed.add_signature(witness_sig);

    // Append to ledger1 directly (without witness validation in this case)
    ledger1.append_entry(witnessed.entry.clone()).await.unwrap();

    // Create sync message for ledger2
    let sync_msg = LedgerSyncMessage::NewWitnessedEntry {
        hash: hash.clone(),
        witnessed,
    };

    // Send to ledger2
    ledger2.handle_sync_message(sync_msg).await.unwrap();

    // Verify ledger2 has the entry
    assert!(ledger2.get_entry(&hash).unwrap().is_some());
    assert_eq!(ledger2.get_balance(&alice, "hours"), 10);

    println!("Witnessed entry sync message handled successfully");
}

#[tokio::test]
async fn test_witness_policy_threshold() {
    let (mut ledger, _temp) = create_simple_ledger();

    // Only require witnesses for entries above 100 units
    ledger.set_witness_config(WitnessConfig::counterparty_above(100));

    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Small entry (10 units) - should NOT require witnesses
    let small_entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    assert!(!ledger.requires_witnesses(&small_entry));

    // Large entry (200 units) - SHOULD require witnesses
    let large_entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 200)
        .credit(bob.clone(), "hours".to_string(), 200)
        .build()
        .unwrap();

    assert!(ledger.requires_witnesses(&large_entry));

    // Small entry can be appended without witnesses
    ledger.append_entry(small_entry).await.unwrap();
    assert_eq!(ledger.get_balance(&alice, "hours"), 10);

    println!("Witness threshold policy works correctly");
}

#[tokio::test]
async fn test_witnessed_entry_invalid_signature_quarantined_via_sync() {
    let (mut ledger, _temp) = create_simple_ledger();
    ledger.set_witness_config(WitnessConfig::counterparty_above(0));

    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let wrong_kp = KeyPair::generate().unwrap(); // Different key
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create signature with wrong key but claim it's from Bob
    let wrong_signature = wrong_kp.sign(hash.as_bytes());
    let witness_sig = WitnessSignature::new(
        bob.clone(), // Claims to be Bob
        wrong_signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    let mut witnessed = WitnessedEntry::new(entry, WitnessPolicy::Counterparty);
    witnessed.add_signature(witness_sig);

    // Send via sync message (simulates receiving from gossip)
    let sync_msg = LedgerSyncMessage::NewWitnessedEntry {
        hash: hash.clone(),
        witnessed,
    };

    // Handle sync message - should quarantine the entry
    ledger.handle_sync_message(sync_msg).await.unwrap();

    // Verify entry was NOT stored in main ledger
    assert!(
        ledger.get_entry(&hash).unwrap().is_none(),
        "Entry with invalid signature should not be stored"
    );

    // Verify entry was quarantined
    let quarantine_items = ledger.quarantine().list().unwrap();
    assert_eq!(quarantine_items.len(), 1, "Entry should be quarantined");

    // Verify quarantine reason
    let quarantined = &quarantine_items[0];
    assert!(
        matches!(
            quarantined.reason,
            icn_ledger::QuarantineReason::WitnessValidationFailed(_)
        ),
        "Quarantine reason should be WitnessValidationFailed"
    );

    println!("Witnessed entry with invalid signature quarantined via sync");
}

#[tokio::test]
async fn test_witnessed_entry_insufficient_signatures_quarantined_via_sync() {
    let (mut ledger, _temp) = create_simple_ledger();

    // Require 2-of-3 quorum
    let witness1 = KeyPair::generate().unwrap().did().clone();
    let witness2 = KeyPair::generate().unwrap().did().clone();
    let witness3 = KeyPair::generate().unwrap().did().clone();

    ledger.set_witness_config(WitnessConfig::quorum(
        2,
        vec![witness1.clone(), witness2.clone(), witness3.clone()],
    ));

    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create witnessed entry with zero signatures (need 2)
    let witnessed = WitnessedEntry::new(
        entry,
        WitnessPolicy::Quorum {
            required: 2,
            witnesses: vec![witness1, witness2, witness3],
        },
    );
    // No signatures added

    // Send via sync message (simulates receiving from gossip)
    let sync_msg = LedgerSyncMessage::NewWitnessedEntry {
        hash: hash.clone(),
        witnessed,
    };

    // Handle sync message - should quarantine the entry
    ledger.handle_sync_message(sync_msg).await.unwrap();

    // Verify entry was NOT stored in main ledger
    assert!(
        ledger.get_entry(&hash).unwrap().is_none(),
        "Entry with insufficient signatures should not be stored"
    );

    // Verify entry was quarantined
    let quarantine_items = ledger.quarantine().list().unwrap();
    assert_eq!(quarantine_items.len(), 1, "Entry should be quarantined");

    // Verify quarantine reason
    let quarantined = &quarantine_items[0];
    assert!(
        matches!(
            quarantined.reason,
            icn_ledger::QuarantineReason::InsufficientWitnesses {
                have: 0,
                required: 2
            }
        ),
        "Quarantine reason should be InsufficientWitnesses with have=0, required=2"
    );

    println!("Witnessed entry with insufficient signatures quarantined via sync");
}
