//! Integration tests for trust-based witness validation
//!
//! Tests the trust graph integration for witness signatures added in
//! the feature request for witness validation with trust requirements.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::KeyPair;
use icn_kernel_api::services::TrustService;
use icn_ledger::{
    entry::JournalEntryBuilder, Ledger, WitnessConfig, WitnessPolicy, WitnessSignature,
    WitnessedEntry,
};
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph, TrustScore};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

/// A test TrustService that wraps a TrustGraph for witness trust tests
struct TestTrustService {
    graph: Arc<RwLock<TrustGraph>>,
}

impl TestTrustService {
    fn new(graph: Arc<RwLock<TrustGraph>>) -> Self {
        Self { graph }
    }
}

impl TrustService for TestTrustService {
    fn oracle(&self) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
        unimplemented!("oracle not needed for witness trust tests")
    }

    fn trust_score(&self, actor: &icn_kernel_api::types::Did) -> f64 {
        // Use try_read for non-blocking access (tests don't have write contention)
        if let Ok(g) = self.graph.try_read() {
            if let Ok(did) = actor.parse::<icn_identity::Did>() {
                return g.compute_trust_score(&did).unwrap_or(0.0);
            }
        }
        0.0
    }

    fn record_event(
        &self,
        _actor: &icn_kernel_api::types::Did,
        _event: icn_kernel_api::services::TrustEvent,
    ) {
    }
}

/// Create a test ledger with trust service backed by a TrustGraph owned by a specific DID
fn create_ledger_with_trust_for_owner(
    owner_did: icn_identity::Did,
) -> (Ledger, TempDir, Arc<RwLock<TrustGraph>>) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

    // Create trust graph for the specified owner
    let trust_store = Arc::new(SledStore::open(temp_dir.path().join("trust")).unwrap());
    let trust_graph = TrustGraph::new(trust_store, owner_did);
    let trust_graph_arc = Arc::new(RwLock::new(trust_graph));

    // Create ledger with TrustService (wrapping TrustGraph)
    let trust_service: Arc<dyn TrustService> =
        Arc::new(TestTrustService::new(trust_graph_arc.clone()));
    let mut ledger = Ledger::new(store).unwrap();
    ledger.set_trust_service(trust_service);

    // Disable entry author trust validation for these tests
    // (we're testing witness trust validation, not entry author validation)
    ledger.set_min_trust_for_entry(0.0);

    (ledger, temp_dir, trust_graph_arc)
}

#[tokio::test]
async fn test_witness_trust_validation_sufficient_trust() {
    // Create keypairs for alice and bob
    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Create ledger with trust graph owned by Alice
    let (mut ledger, _temp, trust_graph_arc) = create_ledger_with_trust_for_owner(alice.clone());

    // Add trust edges (bob has partner-level trust from Alice's perspective)
    // Use 0.6 to account for trust computation factors
    {
        let mut trust_graph = trust_graph_arc.write().await;
        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::new(0.6).unwrap());
        trust_graph.add_edge(edge).unwrap();
    }

    // Configure witness requirement with trust threshold
    ledger.set_witness_config(WitnessConfig::counterparty_with_trust(0, 0.4));

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create witness signature from bob (the counterparty who has sufficient trust)
    let signature = bob_kp.sign(hash.as_bytes());
    let witness_sig = WitnessSignature::new(
        bob.clone(),
        signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    // Create witnessed entry
    let mut witnessed = WitnessedEntry::new(entry, WitnessPolicy::Counterparty);
    witnessed.add_signature(witness_sig);

    // Append should succeed with sufficient trust
    let result_hash = ledger.append_witnessed_entry(witnessed).await.unwrap();
    assert_eq!(result_hash, hash);

    // Verify entry was stored
    assert!(ledger.get_entry(&hash).unwrap().is_some());
    assert_eq!(ledger.get_balance(&alice, "hours"), 10);

    println!("✓ Witness with sufficient trust accepted");
}

#[tokio::test]
async fn test_witness_trust_validation_insufficient_trust() {
    // Create keypairs for alice and bob
    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Create ledger with trust graph owned by Alice
    let (mut ledger, _temp, trust_graph_arc) = create_ledger_with_trust_for_owner(alice.clone());

    // Add trust edge with insufficient trust (below partner threshold)
    {
        let mut trust_graph = trust_graph_arc.write().await;
        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::new(0.2).unwrap());
        trust_graph.add_edge(edge).unwrap();
    }

    // Configure witness requirement with trust threshold
    ledger.set_witness_config(WitnessConfig::counterparty_with_trust(0, 0.4));

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create witness signature from bob (who has insufficient trust)
    let signature = bob_kp.sign(hash.as_bytes());
    let witness_sig = WitnessSignature::new(
        bob.clone(),
        signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    // Create witnessed entry
    let mut witnessed = WitnessedEntry::new(entry, WitnessPolicy::Counterparty);
    witnessed.add_signature(witness_sig);

    // Append should fail due to insufficient trust
    let result = ledger.append_witnessed_entry(witnessed).await;
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }
    assert!(result.is_err(), "Expected error for insufficient trust");
    let err_msg = result.unwrap_err().to_string();
    eprintln!("Error message: {err_msg}");
    assert!(
        err_msg.to_lowercase().contains("trust") || err_msg.to_lowercase().contains("insufficient"),
        "Error should mention trust or insufficient: got '{err_msg}'"
    );

    // Verify entry was NOT stored
    assert!(ledger.get_entry(&hash).unwrap().is_none());

    println!("✓ Witness with insufficient trust rejected");
}

#[tokio::test]
async fn test_witness_trust_validation_unknown_witness() {
    // Create keypairs for alice and bob
    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Create ledger with trust graph owned by Alice
    let (mut ledger, _temp, _trust_graph_arc) = create_ledger_with_trust_for_owner(alice.clone());

    // Don't add any trust edges - bob is completely unknown

    // Configure witness requirement with trust threshold
    ledger.set_witness_config(WitnessConfig::counterparty_with_trust(0, 0.4));

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create witness signature from unknown bob
    let signature = bob_kp.sign(hash.as_bytes());
    let witness_sig = WitnessSignature::new(
        bob.clone(),
        signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    // Create witnessed entry
    let mut witnessed = WitnessedEntry::new(entry, WitnessPolicy::Counterparty);
    witnessed.add_signature(witness_sig);

    // Append should fail due to unknown witness (trust score = 0.0)
    let result = ledger.append_witnessed_entry(witnessed).await;
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }
    assert!(result.is_err(), "Expected error for unknown witness");
    let err_msg = result.unwrap_err().to_string();
    eprintln!("Error message: {err_msg}");
    assert!(
        err_msg.to_lowercase().contains("trust") || err_msg.to_lowercase().contains("insufficient"),
        "Error should mention trust or insufficient: got '{err_msg}'"
    );

    // Verify entry was NOT stored
    assert!(ledger.get_entry(&hash).unwrap().is_none());

    println!("✓ Unknown witness (trust score 0.0) rejected");
}

#[tokio::test]
async fn test_witness_trust_validation_backward_compatible() {
    // Create keypairs for alice and bob
    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Create ledger with trust graph owned by Alice
    let (mut ledger, _temp, _trust_graph_arc) = create_ledger_with_trust_for_owner(alice.clone());

    // Configure witness requirement WITHOUT trust threshold (backward compatible)
    ledger.set_witness_config(WitnessConfig::counterparty_above(0));

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create witness signature from bob
    let signature = bob_kp.sign(hash.as_bytes());
    let witness_sig = WitnessSignature::new(
        bob.clone(),
        signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    // Create witnessed entry
    let mut witnessed = WitnessedEntry::new(entry, WitnessPolicy::Counterparty);
    witnessed.add_signature(witness_sig);

    // Append should succeed even without trust validation (backward compatible)
    let result_hash = ledger.append_witnessed_entry(witnessed).await.unwrap();
    assert_eq!(result_hash, hash);

    // Verify entry was stored
    assert!(ledger.get_entry(&hash).unwrap().is_some());

    println!(
        "✓ Backward compatibility maintained (no trust validation when min_witness_trust = None)"
    );
}

#[tokio::test]
async fn test_witness_trust_validation_no_trust_graph() {
    // Create ledger WITHOUT trust graph
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
    let mut ledger = Ledger::new(store).unwrap();

    // Create keypairs for alice and bob
    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Configure witness requirement with trust threshold
    ledger.set_witness_config(WitnessConfig::counterparty_with_trust(0, 0.4));

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create witness signature from bob
    let signature = bob_kp.sign(hash.as_bytes());
    let witness_sig = WitnessSignature::new(
        bob.clone(),
        signature.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    // Create witnessed entry
    let mut witnessed = WitnessedEntry::new(entry, WitnessPolicy::Counterparty);
    witnessed.add_signature(witness_sig);

    // Append should succeed (trust validation skipped with warning when no trust graph)
    let result_hash = ledger.append_witnessed_entry(witnessed).await.unwrap();
    assert_eq!(result_hash, hash);

    // Verify entry was stored
    assert!(ledger.get_entry(&hash).unwrap().is_some());

    println!("✓ Trust validation skipped when trust graph not available (logged warning)");
}

#[tokio::test]
async fn test_witness_trust_validation_quorum_with_trust() {
    // Create keypairs for alice, bob, and three witnesses
    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let witness1_kp = KeyPair::generate().unwrap();
    let witness2_kp = KeyPair::generate().unwrap();
    let witness3_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();
    let witness1 = witness1_kp.did().clone();
    let witness2 = witness2_kp.did().clone();
    let witness3 = witness3_kp.did().clone();

    // Create ledger with trust graph owned by Alice
    let (mut ledger, _temp, trust_graph_arc) = create_ledger_with_trust_for_owner(alice.clone());

    // Add trust edges (witnesses 1 and 2 have sufficient trust, witness 3 does not)
    // Use higher values to account for trust computation weighting
    {
        let mut trust_graph = trust_graph_arc.write().await;
        trust_graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                witness1.clone(),
                TrustScore::new(0.65).unwrap(),
            ))
            .unwrap();
        trust_graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                witness2.clone(),
                TrustScore::new(0.7).unwrap(),
            ))
            .unwrap();
        trust_graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                witness3.clone(),
                TrustScore::new(0.2).unwrap(),
            ))
            .unwrap();
    }

    // Configure quorum requirement (2 of 3) with trust threshold
    ledger.set_witness_config(WitnessConfig::quorum_with_trust(
        2,
        vec![witness1.clone(), witness2.clone(), witness3.clone()],
        0.4,
    ));

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 100)
        .credit(bob.clone(), "hours".to_string(), 100)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create signatures from witnesses 1 and 2 (who have sufficient trust)
    let sig1 = witness1_kp.sign(hash.as_bytes());
    let sig2 = witness2_kp.sign(hash.as_bytes());

    let witness_sig1 = WitnessSignature::new(
        witness1.clone(),
        sig1.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );
    let witness_sig2 = WitnessSignature::new(
        witness2.clone(),
        sig2.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    // Create witnessed entry with 2 trusted witnesses
    let mut witnessed = WitnessedEntry::new(
        entry,
        WitnessPolicy::Quorum {
            required: 2,
            witnesses: vec![witness1, witness2, witness3],
        },
    );
    witnessed.add_signature(witness_sig1);
    witnessed.add_signature(witness_sig2);

    // Append should succeed with 2 trusted witnesses
    let result_hash = ledger.append_witnessed_entry(witnessed).await.unwrap();
    assert_eq!(result_hash, hash);

    // Verify entry was stored
    assert!(ledger.get_entry(&hash).unwrap().is_some());
    assert_eq!(ledger.get_balance(&alice, "hours"), 100);

    println!("✓ Quorum with trust validation accepted (2 trusted witnesses)");
}

#[tokio::test]
async fn test_witness_trust_validation_quorum_insufficient_trusted_witnesses() {
    // Create keypairs for alice, bob, and three witnesses
    let alice_kp = KeyPair::generate().unwrap();
    let bob_kp = KeyPair::generate().unwrap();
    let witness1_kp = KeyPair::generate().unwrap();
    let witness2_kp = KeyPair::generate().unwrap();
    let witness3_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();
    let witness1 = witness1_kp.did().clone();
    let witness2 = witness2_kp.did().clone();
    let witness3 = witness3_kp.did().clone();

    // Create ledger with trust graph owned by Alice
    let (mut ledger, _temp, trust_graph_arc) = create_ledger_with_trust_for_owner(alice.clone());

    // Add trust edges (only witness 1 has sufficient trust)
    // Use higher value to account for trust computation weighting
    {
        let mut trust_graph = trust_graph_arc.write().await;
        trust_graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                witness1.clone(),
                TrustScore::new(0.65).unwrap(),
            ))
            .unwrap();
        trust_graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                witness2.clone(),
                TrustScore::new(0.2).unwrap(),
            ))
            .unwrap();
        trust_graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                witness3.clone(),
                TrustScore::new(0.3).unwrap(),
            ))
            .unwrap();
    }

    // Configure quorum requirement (2 of 3) with trust threshold
    ledger.set_witness_config(WitnessConfig::quorum_with_trust(
        2,
        vec![witness1.clone(), witness2.clone(), witness3.clone()],
        0.4,
    ));

    // Create an entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 100)
        .credit(bob.clone(), "hours".to_string(), 100)
        .build()
        .unwrap();

    let hash = entry.id.clone().unwrap();

    // Create signatures from witnesses 1 and 2 (only 1 has sufficient trust)
    let sig1 = witness1_kp.sign(hash.as_bytes());
    let sig2 = witness2_kp.sign(hash.as_bytes());

    let witness_sig1 = WitnessSignature::new(
        witness1.clone(),
        sig1.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );
    let witness_sig2 = WitnessSignature::new(
        witness2.clone(),
        sig2.to_bytes().to_vec(),
        icn_time::current_timestamp_secs(),
    );

    // Create witnessed entry
    let mut witnessed = WitnessedEntry::new(
        entry,
        WitnessPolicy::Quorum {
            required: 2,
            witnesses: vec![witness1, witness2, witness3],
        },
    );
    witnessed.add_signature(witness_sig1);
    witnessed.add_signature(witness_sig2);

    // Append should fail because witness 2 has insufficient trust
    let result = ledger.append_witnessed_entry(witnessed).await;
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }
    assert!(result.is_err(), "Expected error for insufficient trust");
    let err_msg = result.unwrap_err().to_string();
    eprintln!("Error message: {err_msg}");
    assert!(
        err_msg.to_lowercase().contains("trust") || err_msg.to_lowercase().contains("insufficient"),
        "Error should mention trust or insufficient: got '{err_msg}'"
    );

    // Verify entry was NOT stored
    assert!(ledger.get_entry(&hash).unwrap().is_none());

    println!("✓ Quorum with insufficient trusted witnesses rejected");
}
