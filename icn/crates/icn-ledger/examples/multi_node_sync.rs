//! Multi-node ledger synchronization example
//!
//! This example demonstrates how multiple nodes synchronize ledger entries
//! via the gossip protocol. It simulates a small cooperative where members
//! exchange services and track them using mutual credit.
//!
//! Run with: cargo run --package icn-ledger --example multi_node_sync

use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_ledger::{entry::JournalEntryBuilder, Ledger, LedgerSyncMessage};
use icn_store::SledStore;
use icn_trust::TrustClass;
use std::sync::Arc;
use tempfile::TempDir;

struct Node {
    _name: String,
    ledger: Ledger,
    gossip: Arc<tokio::sync::RwLock<GossipActor>>,
    _temp_dir: TempDir,
}

impl Node {
    fn new(name: &str) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create gossip actor
        let trust_lookup = Arc::new(|_: &icn_identity::Did| Some(TrustClass::Partner));
        let gossip = GossipActor::spawn(did, trust_lookup);

        // Create ledger with gossip
        let mut ledger = Ledger::new(store).unwrap();
        ledger.set_gossip(gossip.clone());

        Node {
            _name: name.to_string(),
            ledger,
            gossip,
            _temp_dir: temp_dir,
        }
    }

    fn get_balance(&self, account: &icn_identity::Did, currency: &str) -> i64 {
        self.ledger.get_balance(account, currency)
    }

    fn get_entry_count(&self) -> usize {
        self.ledger.get_all_entries().unwrap().len()
    }

    /// Simulate receiving gossip messages from another node
    fn receive_from(&mut self, other: &Node, topic: &str) {
        let other_gossip = other.gossip.blocking_read();
        let entries = other_gossip.get_entries(topic);

        for gossip_entry in entries {
            if let Ok(sync_msg) = serde_json::from_slice::<LedgerSyncMessage>(&gossip_entry.data) {
                // Silently handle sync messages (errors are logged internally)
                let _ = self.ledger.handle_sync_message(sync_msg);
            }
        }
    }
}

fn main() {
    println!("=== Multi-Node Ledger Synchronization Demo ===\n");

    // Create three nodes representing members of a cooperative
    let mut node_alice = Node::new("Alice");
    let mut node_bob = Node::new("Bob");
    let mut node_charlie = Node::new("Charlie");

    // Create DIDs for participants
    let alice_did = KeyPair::generate().unwrap().did().clone();
    let bob_did = KeyPair::generate().unwrap().did().clone();
    let charlie_did = KeyPair::generate().unwrap().did().clone();

    println!("📍 Three nodes initialized: Alice, Bob, Charlie\n");

    // === Scenario 1: Alice provides service to Bob ===
    println!("Scenario 1: Alice provides 10 hours of work to Bob");

    let entry1 = JournalEntryBuilder::new(alice_did.clone())
        .debit(alice_did.clone(), "hours".to_string(), 10)
        .credit(bob_did.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    node_alice.ledger.append_entry(entry1).unwrap();

    println!("  ✓ Node Alice recorded transaction");
    println!(
        "    - Alice's balance: {} hours",
        node_alice.get_balance(&alice_did, "hours")
    );
    println!(
        "    - Bob's balance: {} hours",
        node_alice.get_balance(&bob_did, "hours")
    );
    println!(
        "    - Charlie's balance: {} hours\n",
        node_alice.get_balance(&charlie_did, "hours")
    );

    // Simulate gossip propagation
    node_bob.receive_from(&node_alice, "ledger:hours");
    node_charlie.receive_from(&node_alice, "ledger:hours");

    println!("  📡 Gossip propagation complete");
    println!("    - Node Bob entries: {}", node_bob.get_entry_count());
    println!(
        "    - Node Charlie entries: {}\n",
        node_charlie.get_entry_count()
    );

    // === Scenario 2: Bob provides service to Charlie ===
    println!("Scenario 2: Bob provides 5 hours of work to Charlie");

    let entry2 = JournalEntryBuilder::new(bob_did.clone())
        .debit(bob_did.clone(), "hours".to_string(), 5)
        .credit(charlie_did.clone(), "hours".to_string(), 5)
        .build()
        .unwrap();

    node_bob.ledger.append_entry(entry2).unwrap();

    println!("  ✓ Node Bob recorded transaction");
    println!(
        "    - Bob's balance: {} hours",
        node_bob.get_balance(&bob_did, "hours")
    );
    println!(
        "    - Charlie's balance: {} hours\n",
        node_bob.get_balance(&charlie_did, "hours")
    );

    // Simulate gossip propagation
    node_alice.receive_from(&node_bob, "ledger:hours");
    node_charlie.receive_from(&node_bob, "ledger:hours");

    println!("  📡 Gossip propagation complete\n");

    // === Scenario 3: Charlie provides service to Alice ===
    println!("Scenario 3: Charlie provides 3 hours of work to Alice");

    let entry3 = JournalEntryBuilder::new(charlie_did.clone())
        .debit(charlie_did.clone(), "hours".to_string(), 3)
        .credit(alice_did.clone(), "hours".to_string(), 3)
        .build()
        .unwrap();

    node_charlie.ledger.append_entry(entry3).unwrap();

    println!("  ✓ Node Charlie recorded transaction");
    println!(
        "    - Charlie's balance: {} hours",
        node_charlie.get_balance(&charlie_did, "hours")
    );
    println!(
        "    - Alice's balance: {} hours\n",
        node_charlie.get_balance(&alice_did, "hours")
    );

    // Simulate gossip propagation
    node_alice.receive_from(&node_charlie, "ledger:hours");
    node_bob.receive_from(&node_charlie, "ledger:hours");

    println!("  📡 Gossip propagation complete\n");

    // === Final State ===
    println!("=== Final State Across All Nodes ===\n");

    println!("Node Alice:");
    println!("  - Total entries: {}", node_alice.get_entry_count());
    println!(
        "  - Alice's balance: {} hours",
        node_alice.get_balance(&alice_did, "hours")
    );
    println!(
        "  - Bob's balance: {} hours",
        node_alice.get_balance(&bob_did, "hours")
    );
    println!(
        "  - Charlie's balance: {} hours\n",
        node_alice.get_balance(&charlie_did, "hours")
    );

    println!("Node Bob:");
    println!("  - Total entries: {}", node_bob.get_entry_count());
    println!(
        "  - Alice's balance: {} hours",
        node_bob.get_balance(&alice_did, "hours")
    );
    println!(
        "  - Bob's balance: {} hours",
        node_bob.get_balance(&bob_did, "hours")
    );
    println!(
        "  - Charlie's balance: {} hours\n",
        node_bob.get_balance(&charlie_did, "hours")
    );

    println!("Node Charlie:");
    println!("  - Total entries: {}", node_charlie.get_entry_count());
    println!(
        "  - Alice's balance: {} hours",
        node_charlie.get_balance(&alice_did, "hours")
    );
    println!(
        "  - Bob's balance: {} hours",
        node_charlie.get_balance(&bob_did, "hours")
    );
    println!(
        "  - Charlie's balance: {} hours\n",
        node_charlie.get_balance(&charlie_did, "hours")
    );

    // === Verification ===
    println!("=== Verification ===\n");

    // All nodes should have same number of entries
    assert_eq!(node_alice.get_entry_count(), 3);
    assert_eq!(node_bob.get_entry_count(), 3);
    assert_eq!(node_charlie.get_entry_count(), 3);
    println!("✓ All nodes have synchronized all 3 entries");

    // All nodes should have same balances
    // Alice: +10 (from Bob) -3 (to Charlie) = 7
    // Bob: -10 (to Alice) +5 (from Charlie) = -5
    // Charlie: -5 (to Bob) +3 (from Alice) = -2

    assert_eq!(node_alice.get_balance(&alice_did, "hours"), 7);
    assert_eq!(node_bob.get_balance(&alice_did, "hours"), 7);
    assert_eq!(node_charlie.get_balance(&alice_did, "hours"), 7);
    println!("✓ Alice's balance consistent across all nodes: 7 hours");

    assert_eq!(node_alice.get_balance(&bob_did, "hours"), -5);
    assert_eq!(node_bob.get_balance(&bob_did, "hours"), -5);
    assert_eq!(node_charlie.get_balance(&bob_did, "hours"), -5);
    println!("✓ Bob's balance consistent across all nodes: -5 hours");

    assert_eq!(node_alice.get_balance(&charlie_did, "hours"), -2);
    assert_eq!(node_bob.get_balance(&charlie_did, "hours"), -2);
    assert_eq!(node_charlie.get_balance(&charlie_did, "hours"), -2);
    println!("✓ Charlie's balance consistent across all nodes: -2 hours");

    // Conservation law: sum of all balances should be zero
    let total_alice = node_alice.get_balance(&alice_did, "hours")
        + node_alice.get_balance(&bob_did, "hours")
        + node_alice.get_balance(&charlie_did, "hours");
    assert_eq!(total_alice, 0);
    println!("✓ Conservation law verified: Σ balances = 0");

    println!("\n🎉 Multi-node synchronization successful!");
    println!("   All nodes have converged to the same ledger state.");
}
