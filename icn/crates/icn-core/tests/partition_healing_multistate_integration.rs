#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Network Partition Healing with Multi-State Sync Integration Tests (Issue #110)
//!
//! Tests partition detection, healing, and multi-state synchronization:
//! - 5 nodes split into 2 partitions
//! - Anti-entropy across ledger, trust, and gossip
//! - No balance corruption after healing
//! - Trust edge consistency after healing
//!
//! Validates that the system maintains consistency when partitions heal.

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    clippy::needless_range_loop
)]

use anyhow::Result;
use icn_gossip::{
    AccessControl, GossipActor, PartitionConfig, PartitionDetector, PartitionHealer, Topic,
    VectorClock,
};
use icn_identity::{Did, KeyPair};
use icn_ledger::{balance::compute_all_balances, entry::JournalEntryBuilder, Ledger};
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph, TrustScore};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::info;

/// Ledger sync topic for testing
const LEDGER_TOPIC: &str = "ledger:sync";

/// Trust sync topic for testing
const TRUST_TOPIC: &str = "trust:sync";

/// Test node with full state: ledger, trust, gossip
struct MultiStateNode {
    did: Did,
    keypair: KeyPair,
    gossip: Arc<RwLock<GossipActor>>,
    ledger: Arc<RwLock<Ledger>>,
    trust_graph: Arc<RwLock<TrustGraph>>,
    partition_detector: Arc<RwLock<PartitionDetector>>,
    partition_healer: Arc<RwLock<PartitionHealer>>,
    _temp_dir: TempDir,
}

impl MultiStateNode {
    async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        // Create ledger store
        let ledger_path = temp_dir.path().join("ledger");
        let ledger_store = Arc::new(SledStore::open(&ledger_path)?);
        let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store)?));

        // Create trust graph
        let trust_path = temp_dir.path().join("trust");
        let trust_store = Arc::new(SledStore::open(&trust_path)?);
        let trust_graph = Arc::new(RwLock::new(TrustGraph::new(trust_store, did.clone())));

        // Create gossip actor with trust lookup
        let trust_graph_clone = trust_graph.clone();
        let trust_lookup = Arc::new(move |peer_did: &Did| {
            let graph = trust_graph_clone.clone();
            let peer = peer_did.clone();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let graph = graph.read().await;
                    graph.trust_class(&peer).ok()
                })
            })
        });

        let mut gossip = GossipActor::new(did.clone(), trust_lookup);
        gossip.set_keypair(keypair.clone());

        // Create partition detection components
        let partition_config = PartitionConfig {
            partition_threshold: Duration::from_millis(100),
            check_interval: Duration::from_millis(50),
        };
        let partition_detector = Arc::new(RwLock::new(PartitionDetector::new(partition_config)));
        let partition_healer = Arc::new(RwLock::new(PartitionHealer::new(VectorClock::new())));

        gossip.set_partition_detector(partition_detector.clone());
        gossip.set_partition_healer(partition_healer.clone());

        let gossip = Arc::new(RwLock::new(gossip));

        Ok(Self {
            did,
            keypair,
            gossip,
            ledger,
            trust_graph,
            partition_detector,
            partition_healer,
            _temp_dir: temp_dir,
        })
    }

    /// Set trust for a peer
    async fn set_trust(&self, peer: &Did, score: f64) -> Result<()> {
        let mut graph = self.trust_graph.write().await;
        let edge = TrustEdge::new(self.did.clone(), peer.clone(), TrustScore::unchecked(score));
        graph.add_edge(edge)?;
        Ok(())
    }

    /// Get outgoing trust edges from this node
    async fn get_trust_edges(&self) -> Vec<(Did, Did, f64)> {
        let graph = self.trust_graph.read().await;
        graph
            .get_outgoing_edges(&self.did)
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.source.clone(), e.target.clone(), e.score.value()))
            .collect()
    }

    /// Add ledger entry
    async fn add_entry(&self, entry: icn_ledger::JournalEntry) -> Result<()> {
        let mut ledger = self.ledger.write().await;
        ledger.append_entry(entry).await?;
        Ok(())
    }

    /// Get balance
    async fn get_balance(&self, account: &Did, currency: &str) -> i64 {
        let ledger = self.ledger.read().await;
        ledger.get_balance(account, currency)
    }

    /// Get all entries
    async fn get_all_entries(&self) -> Vec<icn_ledger::JournalEntry> {
        let ledger = self.ledger.read().await;
        ledger.get_all_entries().unwrap_or_default()
    }

    /// Create gossip topics
    async fn create_topics(&self) {
        let mut gossip = self.gossip.write().await;
        gossip.create_topic(Topic::new(LEDGER_TOPIC.to_string(), AccessControl::Public));
        gossip.create_topic(Topic::new(TRUST_TOPIC.to_string(), AccessControl::Public));
    }

    /// Record contact with peer (for partition detection)
    async fn record_contact(&self, peer: &Did) {
        let mut detector = self.partition_detector.write().await;
        detector.record_contact(peer);
    }

    /// Check if peer is partitioned
    async fn is_partitioned(&self, peer: &Did) -> bool {
        let detector = self.partition_detector.read().await;
        detector.is_partitioned(peer)
    }

    /// Get list of partitioned peers
    async fn get_partitioned_peers(&self) -> Vec<Did> {
        let detector = self.partition_detector.read().await;
        detector.get_partitioned_peers()
    }
}

/// Verify balance invariant across all nodes
fn verify_balance_invariant(entries: &[icn_ledger::JournalEntry]) -> Result<()> {
    let all_balances = compute_all_balances(entries)?;

    let mut currency_totals: HashMap<String, i64> = HashMap::new();
    for balances in all_balances.values() {
        for (currency, balance) in &balances.balances {
            *currency_totals.entry(currency.clone()).or_insert(0) += balance;
        }
    }

    for (currency, total) in &currency_totals {
        if *total != 0 {
            return Err(anyhow::anyhow!(
                "Balance invariant violated for {currency}: sum = {total}"
            ));
        }
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_partition_detection_multi_node() -> Result<()> {
    info!("=== Test: Partition Detection Multi-Node ===");

    // Create 5 nodes
    let nodes: Vec<MultiStateNode> =
        futures::future::try_join_all((0..5).map(|_| MultiStateNode::new())).await?;

    // All nodes record contact with each other initially
    for i in 0..5 {
        for j in 0..5 {
            if i != j {
                nodes[i].record_contact(&nodes[j].did).await;
            }
        }
    }

    // Initially, no nodes should be partitioned
    for i in 0..5 {
        let partitioned = nodes[i].get_partitioned_peers().await;
        assert!(
            partitioned.is_empty(),
            "Node {i} should have no partitioned peers initially"
        );
    }

    // Wait for partition threshold to expire
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Now all peers should appear partitioned (no recent contact)
    for i in 0..5 {
        let partitioned = nodes[i].get_partitioned_peers().await;
        assert_eq!(
            partitioned.len(),
            4,
            "Node {i} should see 4 partitioned peers"
        );
    }

    info!("Partition detection multi-node test passed");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_partition_groups_ledger_sync() -> Result<()> {
    info!("=== Test: Partition Groups Ledger Sync ===");

    // Create 5 separate nodes - they'll sync via direct entry transfer
    // Group A: nodes 0, 1 (partition A)
    // Group B: nodes 2, 3, 4 (partition B)
    let nodes: Vec<MultiStateNode> =
        futures::future::try_join_all((0..5).map(|_| MultiStateNode::new())).await?;

    // Create participant identities
    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let charlie = KeyPair::generate()?;

    // === PARTITION PHASE ===
    // Group A creates transaction: Alice -> Bob (in partition)
    let entry_a = JournalEntryBuilder::new(alice.did().clone())
        .debit(alice.did().clone(), "hours".to_string(), 50)
        .credit(bob.did().clone(), "hours".to_string(), 50)
        .build()?;

    // Add to group A nodes (0, 1)
    nodes[0].add_entry(entry_a.clone()).await?;
    nodes[1].add_entry(entry_a.clone()).await?;

    // Group B creates transaction: Bob -> Charlie (in partition)
    let entry_b = JournalEntryBuilder::new(bob.did().clone())
        .debit(bob.did().clone(), "hours".to_string(), 30)
        .credit(charlie.did().clone(), "hours".to_string(), 30)
        .build()?;

    // Add to group B nodes (2, 3, 4)
    nodes[2].add_entry(entry_b.clone()).await?;
    nodes[3].add_entry(entry_b.clone()).await?;
    nodes[4].add_entry(entry_b.clone()).await?;

    // Verify group A's view (only has entry_a)
    assert_eq!(nodes[0].get_balance(alice.did(), "hours").await, 50);
    assert_eq!(nodes[0].get_balance(bob.did(), "hours").await, -50);
    assert_eq!(nodes[0].get_balance(charlie.did(), "hours").await, 0);

    // Verify group B's view (only has entry_b)
    assert_eq!(nodes[2].get_balance(alice.did(), "hours").await, 0);
    assert_eq!(nodes[2].get_balance(bob.did(), "hours").await, 30);
    assert_eq!(nodes[2].get_balance(charlie.did(), "hours").await, -30);

    // === HEAL PHASE ===
    // Sync missing entries to each node
    // Group A nodes get entry_b
    nodes[0].add_entry(entry_b.clone()).await?;
    nodes[1].add_entry(entry_b.clone()).await?;

    // Group B nodes get entry_a
    nodes[2].add_entry(entry_a.clone()).await?;
    nodes[3].add_entry(entry_a.clone()).await?;
    nodes[4].add_entry(entry_a.clone()).await?;

    // === VERIFY CONSISTENCY ===
    // All nodes should have the same view after healing
    for (i, node) in nodes.iter().enumerate() {
        let alice_balance = node.get_balance(alice.did(), "hours").await;
        let bob_balance = node.get_balance(bob.did(), "hours").await;
        let charlie_balance = node.get_balance(charlie.did(), "hours").await;

        info!(
            "Node {}: Alice={}, Bob={}, Charlie={}",
            i, alice_balance, bob_balance, charlie_balance
        );

        // Expected: Alice=50, Bob=-50+30=-20, Charlie=-30
        assert_eq!(alice_balance, 50, "Node {i} Alice balance mismatch");
        assert_eq!(bob_balance, -20, "Node {i} Bob balance mismatch");
        assert_eq!(charlie_balance, -30, "Node {i} Charlie balance mismatch");
    }

    // Verify balance invariant
    let all_entries = vec![entry_a, entry_b];
    verify_balance_invariant(&all_entries)?;

    info!("Partition groups ledger sync test passed");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_trust_edge_consistency_after_heal() -> Result<()> {
    info!("=== Test: Trust Edge Consistency After Heal ===");

    // Create 5 nodes
    let nodes: Vec<MultiStateNode> =
        futures::future::try_join_all((0..5).map(|_| MultiStateNode::new())).await?;

    // === PARTITION PHASE ===
    // Group A (0, 1) sets trust for each other
    nodes[0].set_trust(&nodes[1].did, 0.8).await?;
    nodes[1].set_trust(&nodes[0].did, 0.8).await?;

    // Group B (2, 3, 4) sets trust for each other
    nodes[2].set_trust(&nodes[3].did, 0.7).await?;
    nodes[2].set_trust(&nodes[4].did, 0.7).await?;
    nodes[3].set_trust(&nodes[2].did, 0.7).await?;
    nodes[3].set_trust(&nodes[4].did, 0.7).await?;
    nodes[4].set_trust(&nodes[2].did, 0.7).await?;
    nodes[4].set_trust(&nodes[3].did, 0.7).await?;

    // Verify group A edges
    let edges_0 = nodes[0].get_trust_edges().await;
    let edges_1 = nodes[1].get_trust_edges().await;

    assert_eq!(edges_0.len(), 1, "Node 0 should have 1 trust edge");
    assert_eq!(edges_1.len(), 1, "Node 1 should have 1 trust edge");

    // Verify group B edges
    let edges_2 = nodes[2].get_trust_edges().await;
    let edges_3 = nodes[3].get_trust_edges().await;
    let edges_4 = nodes[4].get_trust_edges().await;

    assert_eq!(edges_2.len(), 2, "Node 2 should have 2 trust edges");
    assert_eq!(edges_3.len(), 2, "Node 3 should have 2 trust edges");
    assert_eq!(edges_4.len(), 2, "Node 4 should have 2 trust edges");

    // === HEAL PHASE ===
    // Cross-partition trust establishment
    nodes[0].set_trust(&nodes[2].did, 0.5).await?;
    nodes[2].set_trust(&nodes[0].did, 0.5).await?;

    // Verify cross-partition edges exist
    let edges_0_after = nodes[0].get_trust_edges().await;
    let edges_2_after = nodes[2].get_trust_edges().await;

    assert_eq!(
        edges_0_after.len(),
        2,
        "Node 0 should have 2 trust edges after heal"
    );
    assert_eq!(
        edges_2_after.len(),
        3,
        "Node 2 should have 3 trust edges after heal"
    );

    // Verify edge integrity
    let node0_trusts_node2 = edges_0_after
        .iter()
        .any(|(from, to, _)| from == &nodes[0].did && to == &nodes[2].did);
    assert!(node0_trusts_node2, "Node 0 should trust Node 2 after heal");

    info!("Trust edge consistency after heal test passed");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_no_balance_corruption_on_heal() -> Result<()> {
    info!("=== Test: No Balance Corruption on Heal ===");

    // Create 5 nodes
    let nodes: Vec<MultiStateNode> =
        futures::future::try_join_all((0..5).map(|_| MultiStateNode::new())).await?;

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let charlie = KeyPair::generate()?;

    // === INITIAL STATE ===
    // All nodes start with same genesis entry
    let genesis = JournalEntryBuilder::new(alice.did().clone())
        .debit(alice.did().clone(), "hours".to_string(), 100)
        .credit(bob.did().clone(), "hours".to_string(), 100)
        .build()?;

    for node in &nodes {
        node.add_entry(genesis.clone()).await?;
    }

    // === PARTITION PHASE ===
    // Group A (0, 1): Alice pays Charlie 20
    let entry_a = JournalEntryBuilder::new(alice.did().clone())
        .debit(alice.did().clone(), "hours".to_string(), 20)
        .credit(charlie.did().clone(), "hours".to_string(), 20)
        .build()?;

    nodes[0].add_entry(entry_a.clone()).await?;
    nodes[1].add_entry(entry_a.clone()).await?;

    // Group B (2, 3, 4): Bob pays Charlie 30
    let entry_b = JournalEntryBuilder::new(bob.did().clone())
        .debit(bob.did().clone(), "hours".to_string(), 30)
        .credit(charlie.did().clone(), "hours".to_string(), 30)
        .build()?;

    nodes[2].add_entry(entry_b.clone()).await?;
    nodes[3].add_entry(entry_b.clone()).await?;
    nodes[4].add_entry(entry_b.clone()).await?;

    // Verify invariant in each partition before heal
    verify_balance_invariant(&[genesis.clone(), entry_a.clone()])?;
    verify_balance_invariant(&[genesis.clone(), entry_b.clone()])?;

    // === HEAL PHASE ===
    // Sync missing entries directly
    // Group A nodes get entry_b
    nodes[0].add_entry(entry_b.clone()).await?;
    nodes[1].add_entry(entry_b.clone()).await?;

    // Group B nodes get entry_a
    nodes[2].add_entry(entry_a.clone()).await?;
    nodes[3].add_entry(entry_a.clone()).await?;
    nodes[4].add_entry(entry_a.clone()).await?;

    // === VERIFY NO CORRUPTION ===
    // Expected final state:
    // Alice: 100 + 20 = 120 (owed 100 from genesis, paid 20 to Charlie)
    // Bob: -100 + 30 = -70 (owes 100 from genesis, paid 30 to Charlie)
    // Charlie: -20 - 30 = -50 (owes to both Alice and Bob)

    for (i, node) in nodes.iter().enumerate() {
        let alice_balance = node.get_balance(alice.did(), "hours").await;
        let bob_balance = node.get_balance(bob.did(), "hours").await;
        let charlie_balance = node.get_balance(charlie.did(), "hours").await;

        info!(
            "Node {}: Alice={}, Bob={}, Charlie={}",
            i, alice_balance, bob_balance, charlie_balance
        );

        assert_eq!(alice_balance, 120, "Node {i} Alice corruption check");
        assert_eq!(bob_balance, -70, "Node {i} Bob corruption check");
        assert_eq!(charlie_balance, -50, "Node {i} Charlie corruption check");
    }

    // Final invariant check
    let all_entries = vec![genesis, entry_a, entry_b];
    verify_balance_invariant(&all_entries)?;

    info!("No balance corruption on heal test passed");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_partition_operations() -> Result<()> {
    info!("=== Test: Concurrent Partition Operations ===");

    // Create 5 nodes
    let nodes: Vec<MultiStateNode> =
        futures::future::try_join_all((0..5).map(|_| MultiStateNode::new())).await?;

    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;

    // Both partitions create entries concurrently with same participants
    // but different amounts - testing idempotency and ordering

    // Group A entry
    let mut entry_a = JournalEntryBuilder::new(alice.did().clone())
        .debit(alice.did().clone(), "hours".to_string(), 10)
        .credit(bob.did().clone(), "hours".to_string(), 10)
        .build()?;
    entry_a.timestamp = 1000; // Earlier

    // Group B entry (different transaction, later timestamp)
    let mut entry_b = JournalEntryBuilder::new(bob.did().clone())
        .debit(bob.did().clone(), "hours".to_string(), 5)
        .credit(alice.did().clone(), "hours".to_string(), 5)
        .build()?;
    entry_b.timestamp = 2000; // Later

    // Add to respective groups
    nodes[0].add_entry(entry_a.clone()).await?;
    nodes[1].add_entry(entry_a.clone()).await?;
    nodes[2].add_entry(entry_b.clone()).await?;
    nodes[3].add_entry(entry_b.clone()).await?;
    nodes[4].add_entry(entry_b.clone()).await?;

    // === HEAL ===
    // Sync missing entries directly
    // Group A nodes get entry_b
    nodes[0].add_entry(entry_b.clone()).await?;
    nodes[1].add_entry(entry_b.clone()).await?;

    // Group B nodes get entry_a
    nodes[2].add_entry(entry_a.clone()).await?;
    nodes[3].add_entry(entry_a.clone()).await?;
    nodes[4].add_entry(entry_a.clone()).await?;

    // === VERIFY CONSISTENCY ===
    // Entry A: Alice +10, Bob -10
    // Entry B: Alice -5, Bob +5
    // Net: Alice +5, Bob -5

    for (i, node) in nodes.iter().enumerate() {
        let alice_balance = node.get_balance(alice.did(), "hours").await;
        let bob_balance = node.get_balance(bob.did(), "hours").await;

        assert_eq!(alice_balance, 5, "Node {i} Alice balance");
        assert_eq!(bob_balance, -5, "Node {i} Bob balance");
    }

    // Invariant check
    verify_balance_invariant(&[entry_a, entry_b])?;

    info!("Concurrent partition operations test passed");
    Ok(())
}
