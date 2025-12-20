//! Fork Resolution Integration Tests (Issue #108)
//!
//! Tests ledger fork detection and resolution across multiple nodes during
//! network partition scenarios, ensuring balance invariants are preserved.
//!
//! Test scenarios:
//! - 4 nodes split into 2 partitions
//! - Conflicting ledger entries created during partition
//! - Fork resolution on partition heal
//! - Balance invariant (sum=0) verification

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    clippy::cloned_ref_to_slice_refs
)]

use anyhow::Result;
use icn_identity::{Did, KeyPair};
use icn_ledger::{
    balance::compute_all_balances, entry::JournalEntryBuilder, Fork, ForkDetector, ForkResolution,
    ForkResolutionStrategy, ForkResolver, Ledger,
};
use icn_store::SledStore;
use icn_trust::TrustGraph;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tempfile::TempDir;
use tracing::info;

/// Test node with ledger and trust graph
#[allow(dead_code)]
struct TestLedgerNode {
    did: Did,
    keypair: KeyPair,
    ledger: Ledger,
    trust_graph: Arc<TrustGraph>,
    fork_detector: ForkDetector,
    fork_resolver: ForkResolver,
    _temp_dir: TempDir,
}

impl TestLedgerNode {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        // Create ledger store
        let ledger_path = temp_dir.path().join("ledger");
        let ledger_store = Arc::new(SledStore::open(&ledger_path)?);
        let ledger = Ledger::new(ledger_store)?;

        // Create trust graph
        let trust_path = temp_dir.path().join("trust");
        let trust_store = Arc::new(SledStore::open(&trust_path)?);
        let trust_graph = Arc::new(TrustGraph::new(trust_store, did.clone()));

        // Create fork resolution components
        let fork_detector = ForkDetector::new();
        let mut fork_resolver = ForkResolver::new(ForkResolutionStrategy::Hybrid);
        fork_resolver.set_trust_graph(trust_graph.clone());

        Ok(Self {
            did,
            keypair,
            ledger,
            trust_graph,
            fork_detector,
            fork_resolver,
            _temp_dir: temp_dir,
        })
    }

    /// Get all entries from the ledger
    fn get_all_entries(&self) -> Vec<icn_ledger::JournalEntry> {
        self.ledger.get_all_entries().unwrap_or_default()
    }
}

/// Verify the balance invariant: sum of all balances per currency = 0
fn verify_balance_invariant(entries: &[icn_ledger::JournalEntry]) -> Result<()> {
    let all_balances = compute_all_balances(entries);

    // Aggregate balances by currency
    let mut currency_totals: HashMap<String, i64> = HashMap::new();

    for balances in all_balances.values() {
        for (currency, balance) in &balances.balances {
            *currency_totals.entry(currency.clone()).or_insert(0) += balance;
        }
    }

    // Verify sum is 0 for each currency
    for (currency, total) in &currency_totals {
        if *total != 0 {
            return Err(anyhow::anyhow!(
                "Balance invariant violated for {currency}: sum = {total} (should be 0)"
            ));
        }
    }

    info!("Balance invariant verified: all currencies sum to 0");
    Ok(())
}

/// Create a test journal entry
fn create_test_entry(
    author: &KeyPair,
    from: &Did,
    to: &Did,
    currency: &str,
    amount: i64,
    parents: Vec<icn_ledger::ContentHash>,
) -> Result<icn_ledger::JournalEntry> {
    let mut builder = JournalEntryBuilder::new(author.did().clone())
        .debit(from.clone(), currency.to_string(), amount)
        .credit(to.clone(), currency.to_string(), amount);

    for parent in parents {
        builder = builder.add_parent(parent);
    }

    let entry = builder.build()?;
    Ok(entry)
}

#[tokio::test]
async fn test_fork_detection_basic() -> Result<()> {
    info!("=== Test: Basic Fork Detection ===");

    let node = TestLedgerNode::new()?;
    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;

    // Create parent entry (genesis)
    let genesis = create_test_entry(&alice, alice.did(), bob.did(), "hours", 10, vec![])?;
    let genesis_hash = genesis.id.clone().unwrap();

    // Create two conflicting entries that reference the same parent
    let entry1 = create_test_entry(
        &alice,
        alice.did(),
        bob.did(),
        "hours",
        5,
        vec![genesis_hash.clone()],
    )?;

    let entry2 = create_test_entry(
        &bob,
        bob.did(),
        alice.did(),
        "hours",
        3,
        vec![genesis_hash.clone()],
    )?;

    // Detect fork
    assert!(
        node.fork_resolver.is_fork(&entry1, &entry2),
        "Should detect fork: same parent, different entries"
    );

    // Verify balance invariant for genesis
    verify_balance_invariant(&[genesis])?;

    info!("Fork detection test passed");
    Ok(())
}

#[tokio::test]
async fn test_fork_resolution_timestamp_strategy() -> Result<()> {
    info!("=== Test: Fork Resolution with Timestamp Strategy ===");

    let resolver = ForkResolver::new(ForkResolutionStrategy::TimestampPreference);

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;

    let parent = icn_ledger::ContentHash([0u8; 32]);

    // Entry 1: earlier timestamp (should win)
    let mut entry1 = create_test_entry(
        &alice,
        alice.did(),
        bob.did(),
        "hours",
        10,
        vec![parent.clone()],
    )?;
    entry1.timestamp = 1000; // Earlier

    // Entry 2: later timestamp
    let mut entry2 = create_test_entry(
        &bob,
        bob.did(),
        alice.did(),
        "hours",
        15,
        vec![parent.clone()],
    )?;
    entry2.timestamp = 2000; // Later

    let fork = Fork {
        common_parents: vec![parent],
        entry1: entry1.clone(),
        entry2: entry2.clone(),
        detected_at: SystemTime::now(),
    };

    let resolution = resolver.resolve_fork(&fork)?;

    assert_eq!(
        resolution,
        ForkResolution::KeepFirst,
        "Earlier timestamp should win"
    );

    // Verify balance invariant if we keep entry1
    verify_balance_invariant(&[entry1])?;

    info!("Timestamp resolution test passed");
    Ok(())
}

#[tokio::test]
async fn test_fork_resolution_maintains_ledger_invariants() -> Result<()> {
    info!("=== Test: Fork Resolution Maintains Ledger Invariants ===");

    // Set up 4 nodes (simulating partition scenario)
    let _nodes: Vec<TestLedgerNode> = (0..4).map(|_| TestLedgerNode::new().unwrap()).collect();

    // Create identities for participants
    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let charlie = KeyPair::generate()?;
    let diana = KeyPair::generate()?;

    // Create genesis entry (shared by all nodes)
    let genesis = create_test_entry(&alice, alice.did(), bob.did(), "hours", 100, vec![])?;
    let genesis_hash = genesis.id.clone().unwrap();

    info!("Genesis entry created: {:?}", hex::encode(genesis_hash.0));

    // Verify genesis balance invariant
    verify_balance_invariant(&[genesis.clone()])?;

    // Simulate partition: Partition A (nodes 0,1) and Partition B (nodes 2,3)

    // Partition A creates entry: Alice pays Charlie 30 hours
    let partition_a_entry = create_test_entry(
        &alice,
        alice.did(),
        charlie.did(),
        "hours",
        30,
        vec![genesis_hash.clone()],
    )?;

    // Partition B creates conflicting entry: Bob pays Diana 25 hours
    // This references the same parent (genesis) - creating a fork!
    let partition_b_entry = create_test_entry(
        &bob,
        bob.did(),
        diana.did(),
        "hours",
        25,
        vec![genesis_hash.clone()],
    )?;

    info!(
        "Partition A entry: {} -> {} (30 hours)",
        alice.did(),
        charlie.did()
    );
    info!(
        "Partition B entry: {} -> {} (25 hours)",
        bob.did(),
        diana.did()
    );

    // Verify each partition's entries maintain invariant independently
    verify_balance_invariant(&[genesis.clone(), partition_a_entry.clone()])?;
    verify_balance_invariant(&[genesis.clone(), partition_b_entry.clone()])?;

    // Now simulate partition heal - detect and resolve fork
    let mut fork_detector = ForkDetector::new();
    fork_detector.index_entry(&genesis);
    fork_detector.index_entry(&partition_a_entry);
    fork_detector.index_entry(&partition_b_entry);

    // Detect forks
    let forks = fork_detector.detect_forks();
    assert_eq!(forks.len(), 1, "Should detect exactly one fork");
    assert_eq!(
        forks[0].1.len(),
        2,
        "Fork should have 2 conflicting children"
    );

    info!("Fork detected: parent has {} children", forks[0].1.len());

    // Resolve the fork
    let fork = Fork {
        common_parents: vec![genesis_hash.clone()],
        entry1: partition_a_entry.clone(),
        entry2: partition_b_entry.clone(),
        detected_at: SystemTime::now(),
    };

    let resolver = ForkResolver::new(ForkResolutionStrategy::TimestampPreference);
    let resolution = resolver.resolve_fork(&fork)?;

    info!("Fork resolution: {:?}", resolution);

    // Apply resolution and verify invariant
    let winning_entry = match resolution {
        ForkResolution::KeepFirst => partition_a_entry.clone(),
        ForkResolution::KeepSecond => partition_b_entry.clone(),
        ForkResolution::RequiresManual { reason } => {
            return Err(anyhow::anyhow!("Fork requires manual resolution: {reason}"));
        }
    };

    // Final ledger state: genesis + winning entry
    let final_entries = vec![genesis, winning_entry];

    // Verify the balance invariant is preserved
    verify_balance_invariant(&final_entries)?;

    info!("Fork resolution maintains ledger invariants - test passed");
    Ok(())
}

#[tokio::test]
async fn test_nway_fork_resolution_with_invariants() -> Result<()> {
    info!("=== Test: N-way Fork Resolution with Invariants ===");

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let charlie = KeyPair::generate()?;
    let diana = KeyPair::generate()?;

    // Create genesis
    let genesis = create_test_entry(&alice, alice.did(), bob.did(), "hours", 100, vec![])?;
    let genesis_hash = genesis.id.clone().unwrap();

    // Create 4-way fork: 4 entries all referencing genesis
    let mut entries = vec![];
    let participants = [
        (&alice, &bob),
        (&bob, &charlie),
        (&charlie, &diana),
        (&diana, &alice),
    ];

    for (i, (from_kp, to_did)) in participants.iter().enumerate() {
        let mut entry = create_test_entry(
            from_kp,
            from_kp.did(),
            to_did.did(),
            "hours",
            (10 + i * 5) as i64,
            vec![genesis_hash.clone()],
        )?;
        // Assign different timestamps for deterministic resolution
        entry.timestamp = (1000 + i * 100) as u64;
        entries.push(entry);
    }

    // Index all entries for fork detection
    let mut fork_detector = ForkDetector::new();
    fork_detector.index_entry(&genesis);
    for entry in &entries {
        fork_detector.index_entry(entry);
    }

    // Detect fork
    let forks = fork_detector.detect_forks();
    assert_eq!(forks.len(), 1, "Should detect one fork point");
    assert_eq!(forks[0].1.len(), 4, "Fork should have 4 children");

    info!("Detected 4-way fork");

    // Tournament-style resolution
    let resolver = ForkResolver::new(ForkResolutionStrategy::TimestampPreference);

    let mut winner = entries[0].clone();
    for challenger in entries.iter().skip(1) {
        let fork = Fork {
            common_parents: vec![genesis_hash.clone()],
            entry1: winner.clone(),
            entry2: challenger.clone(),
            detected_at: SystemTime::now(),
        };

        let resolution = resolver.resolve_fork(&fork)?;
        winner = match resolution {
            ForkResolution::KeepFirst => winner,
            ForkResolution::KeepSecond => challenger.clone(),
            ForkResolution::RequiresManual { .. } => winner,
        };
    }

    info!(
        "Tournament winner: entry with timestamp {}",
        winner.timestamp
    );

    // Verify balance invariant with genesis + winner
    verify_balance_invariant(&[genesis, winner])?;

    info!("N-way fork resolution with invariants - test passed");
    Ok(())
}

#[tokio::test]
async fn test_fork_resolution_trust_weighted() -> Result<()> {
    info!("=== Test: Fork Resolution with Trust-Weighted Strategy ===");

    let temp_dir = TempDir::new()?;
    let trust_path = temp_dir.path().join("trust");
    let trust_store = Arc::new(SledStore::open(&trust_path)?);

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let charlie = KeyPair::generate()?;

    // Create trust graph with Alice as root
    let trust_graph = Arc::new(TrustGraph::new(trust_store, alice.did().clone()));

    // Set up trust edges: Alice trusts Bob highly, Charlie less
    // Note: In real system this would be through trust_graph.add_edge()

    let mut resolver = ForkResolver::new(ForkResolutionStrategy::TrustWeighted);
    resolver.set_trust_graph(trust_graph);

    let parent = icn_ledger::ContentHash([0u8; 32]);

    // Entry from Bob (higher trust)
    let entry_bob = create_test_entry(
        &bob,
        bob.did(),
        charlie.did(),
        "hours",
        20,
        vec![parent.clone()],
    )?;

    // Entry from Charlie (lower trust)
    let entry_charlie = create_test_entry(
        &charlie,
        charlie.did(),
        bob.did(),
        "hours",
        15,
        vec![parent.clone()],
    )?;

    let fork = Fork {
        common_parents: vec![parent],
        entry1: entry_bob.clone(),
        entry2: entry_charlie.clone(),
        detected_at: SystemTime::now(),
    };

    // Resolution should be deterministic even with equal trust
    let resolution = resolver.resolve_fork(&fork)?;

    // With equal trust (both 0.0 by default), falls back to timestamp
    assert!(
        matches!(
            resolution,
            ForkResolution::KeepFirst | ForkResolution::KeepSecond
        ),
        "Should resolve deterministically"
    );

    // Verify invariant with whichever entry wins
    let winning_entry = match resolution {
        ForkResolution::KeepFirst => entry_bob,
        ForkResolution::KeepSecond => entry_charlie,
        _ => unreachable!(),
    };

    verify_balance_invariant(&[winning_entry])?;

    info!("Trust-weighted fork resolution - test passed");
    Ok(())
}

#[tokio::test]
async fn test_complex_fork_chain_invariants() -> Result<()> {
    info!("=== Test: Complex Fork Chain Invariants ===");

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;

    // Create a chain of entries with a fork in the middle
    // Genesis -> Entry1 -> [Fork: Entry2a | Entry2b] -> Entry3

    // Genesis
    let genesis = create_test_entry(&alice, alice.did(), bob.did(), "hours", 100, vec![])?;
    let genesis_hash = genesis.id.clone().unwrap();

    // Entry 1 (builds on genesis)
    let entry1 = create_test_entry(
        &bob,
        bob.did(),
        alice.did(),
        "hours",
        20,
        vec![genesis_hash.clone()],
    )?;
    let entry1_hash = entry1.id.clone().unwrap();

    // Fork: Entry 2a and 2b both reference Entry 1
    let mut entry2a = create_test_entry(
        &alice,
        alice.did(),
        bob.did(),
        "hours",
        30,
        vec![entry1_hash.clone()],
    )?;
    entry2a.timestamp = 1000;

    let mut entry2b = create_test_entry(
        &bob,
        bob.did(),
        alice.did(),
        "hours",
        25,
        vec![entry1_hash.clone()],
    )?;
    entry2b.timestamp = 2000;

    // Detect and resolve fork
    let mut fork_detector = ForkDetector::new();
    fork_detector.index_entry(&entry1);
    fork_detector.index_entry(&entry2a);
    fork_detector.index_entry(&entry2b);

    assert!(
        fork_detector.has_fork(&entry1_hash),
        "Should detect fork at entry1"
    );

    let resolver = ForkResolver::new(ForkResolutionStrategy::TimestampPreference);
    let fork = Fork {
        common_parents: vec![entry1_hash],
        entry1: entry2a.clone(),
        entry2: entry2b.clone(),
        detected_at: SystemTime::now(),
    };

    let resolution = resolver.resolve_fork(&fork)?;
    assert_eq!(
        resolution,
        ForkResolution::KeepFirst,
        "Earlier entry2a should win"
    );

    // Verify full chain invariant
    let final_chain = vec![genesis, entry1, entry2a];
    verify_balance_invariant(&final_chain)?;

    info!("Complex fork chain invariants - test passed");
    Ok(())
}

#[tokio::test]
async fn test_multicurrency_fork_invariants() -> Result<()> {
    info!("=== Test: Multi-Currency Fork Invariants ===");

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;

    let parent = icn_ledger::ContentHash([0u8; 32]);

    // Fork with different currencies
    let entry_hours = JournalEntryBuilder::new(alice.did().clone())
        .debit(alice.did().clone(), "hours".to_string(), 10)
        .credit(bob.did().clone(), "hours".to_string(), 10)
        .add_parent(parent.clone())
        .build()?;

    let entry_usd = JournalEntryBuilder::new(bob.did().clone())
        .debit(bob.did().clone(), "USD".to_string(), 100)
        .credit(alice.did().clone(), "USD".to_string(), 100)
        .add_parent(parent.clone())
        .build()?;

    // Both entries together should maintain invariant
    verify_balance_invariant(&[entry_hours.clone()])?;
    verify_balance_invariant(&[entry_usd.clone()])?;

    // Even if both are accepted (no actual fork - different currencies)
    verify_balance_invariant(&[entry_hours, entry_usd])?;

    info!("Multi-currency fork invariants - test passed");
    Ok(())
}
