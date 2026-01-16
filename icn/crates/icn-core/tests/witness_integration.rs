#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Witness Signature Integration Tests (Issue #676)
//!
//! Tests for multi-party witness signatures on ledger entries:
//! - Entry rejection without required witnesses
//! - Witnessed entry fork resolution (more signers wins)
//! - Witness policy enforcement
//! - Gossip synchronization of witnessed entries

#![allow(
    unknown_lints,
    dead_code,
    unused_imports,
    unused_variables,
    clippy::cloned_ref_to_slice_refs
)]

use anyhow::Result;
use icn_identity::{Did, KeyPair};
use icn_ledger::types::{
    ContentHash, WitnessConfig, WitnessPolicy, WitnessSignature, WitnessedEntry,
};
use icn_ledger::{
    entry::JournalEntryBuilder, Fork, ForkResolution, ForkResolutionStrategy, ForkResolver, Ledger,
    LedgerError,
};
use icn_store::{SledStore, Store};
use std::sync::Arc;
use std::time::SystemTime;
use tempfile::TempDir;
use tracing::info;

/// Helper to create a test ledger with witness config
fn create_witnessed_ledger(config: WitnessConfig) -> Result<(Ledger, TempDir)> {
    let temp_dir = TempDir::new()?;
    let store = Arc::new(SledStore::open(temp_dir.path())?);
    let mut ledger = Ledger::new(store)?;
    ledger.set_witness_config(config);
    Ok((ledger, temp_dir))
}

/// Helper to create a balanced journal entry
fn create_test_entry(
    author: &KeyPair,
    from: &Did,
    to: &Did,
    currency: &str,
    amount: i64,
) -> Result<icn_ledger::JournalEntry> {
    let entry = JournalEntryBuilder::new(author.did().clone())
        .debit(from.clone(), currency.to_string(), amount)
        .credit(to.clone(), currency.to_string(), amount)
        .build()?;
    Ok(entry)
}

/// Helper to create a witness signature for an entry
fn create_witness_signature(witness_kp: &KeyPair, entry_hash: &ContentHash) -> WitnessSignature {
    let signature = witness_kp.sign(entry_hash.as_bytes());
    WitnessSignature::new(
        witness_kp.did().clone(),
        signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    )
}

#[tokio::test]
async fn test_entry_rejected_without_required_witnesses() -> Result<()> {
    info!("=== Test: Entry Rejected Without Required Witnesses ===");

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let charlie = KeyPair::generate()?;

    // Create ledger with counterparty witness requirement
    let config = WitnessConfig {
        default_policy: WitnessPolicy::Counterparty,
        threshold: None,
        collection_timeout_secs: 300,
    };
    let (ledger, _temp) = create_witnessed_ledger(config)?;

    // Create entry: Alice pays Bob
    let entry = create_test_entry(&alice, alice.did(), bob.did(), "hours", 50)?;
    let entry_hash = entry.id.clone().unwrap();

    // Create witnessed entry WITHOUT bob's signature (only has charlie who is not a counterparty)
    let charlie_sig = create_witness_signature(&charlie, &entry_hash);
    let witnessed = WitnessedEntry {
        entry,
        witness_signatures: vec![charlie_sig],
        policy_applied: WitnessPolicy::Counterparty,
    };

    // Validation should fail - charlie is not a counterparty
    assert!(
        !witnessed.has_sufficient_signatures(),
        "Should require counterparty (bob) signature, charlie is not a counterparty"
    );

    info!("Entry correctly rejected without counterparty witness");
    Ok(())
}

#[tokio::test]
async fn test_entry_accepted_with_required_witnesses() -> Result<()> {
    info!("=== Test: Entry Accepted With Required Witnesses ===");

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;

    // Create ledger with counterparty witness requirement
    let config = WitnessConfig {
        default_policy: WitnessPolicy::Counterparty,
        threshold: None,
        collection_timeout_secs: 300,
    };
    let (mut ledger, _temp) = create_witnessed_ledger(config)?;

    // Create entry: Alice pays Bob
    let entry = create_test_entry(&alice, alice.did(), bob.did(), "hours", 50)?;
    let entry_hash = entry.id.clone().unwrap();

    // Create witnessed entry WITH bob's signature (bob is the counterparty)
    let bob_sig = create_witness_signature(&bob, &entry_hash);
    let witnessed = WitnessedEntry {
        entry,
        witness_signatures: vec![bob_sig],
        policy_applied: WitnessPolicy::Counterparty,
    };

    // Validation should pass
    assert!(
        witnessed.has_sufficient_signatures(),
        "Should pass with counterparty (bob) signature"
    );

    // Append should succeed
    let result = ledger.append_witnessed_entry(witnessed).await;
    assert!(result.is_ok(), "Witnessed entry append should succeed");

    info!("Entry correctly accepted with counterparty witness");
    Ok(())
}

#[tokio::test]
async fn test_quorum_policy_enforcement() -> Result<()> {
    info!("=== Test: Quorum Policy Enforcement ===");

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let witness1 = KeyPair::generate()?;
    let witness2 = KeyPair::generate()?;
    let witness3 = KeyPair::generate()?;

    // Create ledger with 2-of-3 quorum requirement
    let config = WitnessConfig {
        default_policy: WitnessPolicy::Quorum {
            required: 2,
            witnesses: vec![
                witness1.did().clone(),
                witness2.did().clone(),
                witness3.did().clone(),
            ],
        },
        threshold: None,
        collection_timeout_secs: 300,
    };
    let (mut ledger, _temp) = create_witnessed_ledger(config.clone())?;

    // Create entry
    let entry = create_test_entry(&alice, alice.did(), bob.did(), "hours", 100)?;
    let entry_hash = entry.id.clone().unwrap();

    // Test with only 1 witness (should fail)
    let sig1 = create_witness_signature(&witness1, &entry_hash);
    let witnessed_insufficient = WitnessedEntry {
        entry: entry.clone(),
        witness_signatures: vec![sig1.clone()],
        policy_applied: config.default_policy.clone(),
    };

    assert!(
        !witnessed_insufficient.has_sufficient_signatures(),
        "1-of-3 should not satisfy 2-of-3 quorum"
    );

    // Test with 2 witnesses (should pass)
    let sig2 = create_witness_signature(&witness2, &entry_hash);
    let witnessed_sufficient = WitnessedEntry {
        entry: entry.clone(),
        witness_signatures: vec![sig1, sig2],
        policy_applied: config.default_policy.clone(),
    };

    assert!(
        witnessed_sufficient.has_sufficient_signatures(),
        "2-of-3 should satisfy 2-of-3 quorum"
    );

    // Append should succeed
    let result = ledger.append_witnessed_entry(witnessed_sufficient).await;
    assert!(result.is_ok(), "Witnessed entry with quorum should succeed");

    info!("Quorum policy enforcement test passed");
    Ok(())
}

#[tokio::test]
async fn test_all_parties_policy_enforcement() -> Result<()> {
    info!("=== Test: AllParties Policy Enforcement ===");

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let charlie = KeyPair::generate()?;

    // Create ledger with AllParties requirement
    let config = WitnessConfig {
        default_policy: WitnessPolicy::AllParties,
        threshold: None,
        collection_timeout_secs: 300,
    };
    let (mut ledger, _temp) = create_witnessed_ledger(config)?;

    // Create entry involving three parties: Alice debits, Bob and Charlie receive
    let entry = JournalEntryBuilder::new(alice.did().clone())
        .debit(alice.did().clone(), "hours".to_string(), 20)
        .credit(bob.did().clone(), "hours".to_string(), 10)
        .credit(charlie.did().clone(), "hours".to_string(), 10)
        .build()?;
    let entry_hash = entry.id.clone().unwrap();

    // Test with only bob's signature (should fail - missing charlie)
    let bob_sig = create_witness_signature(&bob, &entry_hash);
    let witnessed_partial = WitnessedEntry {
        entry: entry.clone(),
        witness_signatures: vec![bob_sig.clone()],
        policy_applied: WitnessPolicy::AllParties,
    };

    assert!(
        !witnessed_partial.has_sufficient_signatures(),
        "AllParties should require both bob and charlie"
    );

    // Test with both bob and charlie (should pass)
    let charlie_sig = create_witness_signature(&charlie, &entry_hash);
    let witnessed_complete = WitnessedEntry {
        entry: entry.clone(),
        witness_signatures: vec![bob_sig, charlie_sig],
        policy_applied: WitnessPolicy::AllParties,
    };

    assert!(
        witnessed_complete.has_sufficient_signatures(),
        "AllParties should pass with all counterparties"
    );

    // Append should succeed
    let result = ledger.append_witnessed_entry(witnessed_complete).await;
    assert!(result.is_ok(), "AllParties witnessed entry should succeed");

    info!("AllParties policy enforcement test passed");
    Ok(())
}

#[tokio::test]
async fn test_witness_threshold_respected() -> Result<()> {
    info!("=== Test: Witness Threshold Respected ===");

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;

    // Create ledger with counterparty requirement only above 100 units
    let config = WitnessConfig {
        default_policy: WitnessPolicy::Counterparty,
        threshold: Some(100),
        collection_timeout_secs: 300,
    };
    let (_ledger, _temp) = create_witnessed_ledger(config.clone())?;

    // Low-value entry (50 < 100 threshold) - should not require witnesses
    let low_value_entry = create_test_entry(&alice, alice.did(), bob.did(), "hours", 50)?;

    // Check effective policy for low value
    let effective_policy = config.effective_policy(50);
    assert!(
        matches!(effective_policy, WitnessPolicy::None),
        "Low-value entries should have no witness requirement"
    );

    // High-value entry (150 > 100 threshold) - should require witnesses
    let effective_policy_high = config.effective_policy(150);
    assert!(
        matches!(effective_policy_high, WitnessPolicy::Counterparty),
        "High-value entries should require counterparty witness"
    );

    info!("Witness threshold test passed");
    Ok(())
}

#[tokio::test]
async fn test_fork_resolution_prefers_more_witnesses() -> Result<()> {
    info!("=== Test: Fork Resolution Prefers More Witnesses ===");

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let witness1 = KeyPair::generate()?;
    let witness2 = KeyPair::generate()?;

    let parent = ContentHash::from_bytes([0u8; 32]);

    // Entry 1: Has 1 witness signature
    let mut entry1 = create_test_entry(&alice, alice.did(), bob.did(), "hours", 30)?;
    entry1.timestamp = 1000;
    let entry1_hash = entry1.id.clone().unwrap();
    let sig1 = create_witness_signature(&witness1, &entry1_hash);

    // Entry 2: Has 2 witness signatures (should win)
    let mut entry2 = create_test_entry(&bob, bob.did(), alice.did(), "hours", 25)?;
    entry2.timestamp = 2000; // Later timestamp, but more witnesses
    let entry2_hash = entry2.id.clone().unwrap();
    let sig2a = create_witness_signature(&witness1, &entry2_hash);
    let sig2b = create_witness_signature(&witness2, &entry2_hash);

    // Create fork with MajoritySignatures strategy
    let temp_dir = TempDir::new()?;
    let store = Arc::new(SledStore::open(temp_dir.path())?);
    let mut resolver = ForkResolver::new(ForkResolutionStrategy::MajoritySignatures);
    resolver.set_store(store.clone());

    // Store witness signatures for entry2
    let key2 = format!("ledger:witnesses:{}", entry2_hash.to_hex());
    let sigs2 = vec![sig2a, sig2b];
    store.put(key2.as_bytes(), &serde_json::to_vec(&sigs2)?)?;

    // Store witness signatures for entry1
    let key1 = format!("ledger:witnesses:{}", entry1_hash.to_hex());
    let sigs1 = vec![sig1];
    store.put(key1.as_bytes(), &serde_json::to_vec(&sigs1)?)?;

    // Create fork
    let fork = Fork {
        common_parents: vec![parent],
        entry1: entry1.clone(),
        entry2: entry2.clone(),
        detected_at: SystemTime::now(),
    };

    // Resolve - entry2 should win (2 witnesses vs 1)
    let resolution = resolver.resolve_fork(&fork)?;

    assert_eq!(
        resolution,
        ForkResolution::KeepSecond,
        "Entry with more witnesses (entry2) should win"
    );

    info!("Fork resolution with witnesses test passed");
    Ok(())
}

#[tokio::test]
async fn test_witnessed_entry_sync_message_serialization() -> Result<()> {
    info!("=== Test: Witnessed Entry Sync Message Serialization ===");

    use icn_ledger::{deserialize_sync_message, serialize_sync_message, LedgerSyncMessage};

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;

    let entry = create_test_entry(&alice, alice.did(), bob.did(), "hours", 50)?;
    let entry_hash = entry.id.clone().unwrap();
    let bob_sig = create_witness_signature(&bob, &entry_hash);

    let witnessed = WitnessedEntry {
        entry,
        witness_signatures: vec![bob_sig],
        policy_applied: WitnessPolicy::Counterparty,
    };

    // Create sync message
    let msg = LedgerSyncMessage::NewWitnessedEntry {
        hash: entry_hash.clone(),
        witnessed: witnessed.clone(),
    };

    // Serialize
    let data = serialize_sync_message(&msg)?;
    assert!(!data.is_empty(), "Serialized message should not be empty");

    // Deserialize
    let msg2 = deserialize_sync_message(&data)?;

    match msg2 {
        LedgerSyncMessage::NewWitnessedEntry {
            hash,
            witnessed: w2,
        } => {
            assert_eq!(hash, entry_hash, "Hash should match");
            assert_eq!(
                w2.witness_signatures.len(),
                1,
                "Should have 1 witness signature"
            );
            assert!(
                matches!(w2.policy_applied, WitnessPolicy::Counterparty),
                "Policy should be Counterparty"
            );
        }
        _ => panic!("Wrong message type after deserialization"),
    }

    info!("Witnessed entry sync message serialization test passed");
    Ok(())
}
