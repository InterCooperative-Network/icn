# Lab 07: Gossip Sync with Vector Clocks

## Goal
Implement vector clocks and anti-entropy sync for eventual consistency.

## Requirements

### 1. Vector Clock
```rust
pub struct VectorClock {
    clocks: HashMap<String, u64>,  // NodeID → sequence
}

impl VectorClock {
    pub fn increment(&mut self, node_id: &str) {
        *self.clocks.entry(node_id.to_string()).or_insert(0) += 1;
    }
    
    pub fn merge(&mut self, other: &VectorClock) {
        for (node_id, &count) in &other.clocks {
            let entry = self.clocks.entry(node_id.clone()).or_insert(0);
            *entry = (*entry).max(count);
        }
    }
    
    pub fn happens_before(&self, other: &VectorClock) -> bool {
        // Returns true if self <= other (all clocks)
        self.clocks.iter().all(|(k, &v)| {
            other.clocks.get(k).map_or(false, |&other_v| v <= other_v)
        })
    }
}
```

### 2. Gossip Entry
```rust
pub struct GossipEntry {
    pub id: String,
    pub data: Vec<u8>,
    pub vector_clock: VectorClock,
}
```

### 3. In-Memory Node
```rust
pub struct GossipNode {
    node_id: String,
    entries: HashMap<String, GossipEntry>,
    vector_clock: VectorClock,
}

impl GossipNode {
    pub fn announce(&mut self, data: Vec<u8>) -> String {
        let id = format!("{}-{}", self.node_id, self.vector_clock.get(&self.node_id));
        self.vector_clock.increment(&self.node_id);
        
        let entry = GossipEntry {
            id: id.clone(),
            data,
            vector_clock: self.vector_clock.clone(),
        };
        
        self.entries.insert(id.clone(), entry);
        id
    }
    
    pub fn pull(&mut self, entry: GossipEntry) {
        if !self.entries.contains_key(&entry.id) {
            self.vector_clock.merge(&entry.vector_clock);
            self.entries.insert(entry.id.clone(), entry);
        }
    }
    
    pub fn get_missing(&self, other_clock: &VectorClock) -> Vec<String> {
        self.entries.values()
            .filter(|e| !e.vector_clock.happens_before(other_clock))
            .map(|e| e.id.clone())
            .collect()
    }
}
```

## Tests to Write

### Test 1: Single Node Announcement
```rust
#[test]
fn test_single_node_announce() {
    let mut node = GossipNode::new("A");
    
    let id1 = node.announce(b"msg1".to_vec());
    let id2 = node.announce(b"msg2".to_vec());
    
    assert_eq!(node.entries.len(), 2);
    assert!(node.entries.contains_key(&id1));
    assert!(node.entries.contains_key(&id2));
}
```

### Test 2: Two Node Sync
```rust
#[test]
fn test_two_node_sync() {
    let mut node_a = GossipNode::new("A");
    let mut node_b = GossipNode::new("B");
    
    // Node A announces
    let id1 = node_a.announce(b"msg1".to_vec());
    
    // Node B pulls from A
    let entry = node_a.entries.get(&id1).unwrap().clone();
    node_b.pull(entry);
    
    // Node B should have the entry
    assert!(node_b.entries.contains_key(&id1));
}
```

### Test 3: Anti-Entropy Convergence
```rust
#[test]
fn test_anti_entropy_convergence() {
    let mut node_a = GossipNode::new("A");
    let mut node_b = GossipNode::new("B");
    
    // Node A creates 3 entries
    node_a.announce(b"a1".to_vec());
    node_a.announce(b"a2".to_vec());
    node_a.announce(b"a3".to_vec());
    
    // Node B creates 2 entries
    node_b.announce(b"b1".to_vec());
    node_b.announce(b"b2".to_vec());
    
    // Anti-entropy: A detects B is missing entries
    let missing_on_b = node_a.get_missing(&node_b.vector_clock);
    
    // B pulls missing entries from A
    for id in missing_on_b {
        let entry = node_a.entries.get(&id).unwrap().clone();
        node_b.pull(entry);
    }
    
    // B detects A is missing entries
    let missing_on_a = node_b.get_missing(&node_a.vector_clock);
    
    // A pulls missing entries from B
    for id in missing_on_a {
        let entry = node_b.entries.get(&id).unwrap().clone();
        node_a.pull(entry);
    }
    
    // Both nodes should converge
    assert_eq!(node_a.entries.len(), 5);
    assert_eq!(node_b.entries.len(), 5);
}
```

### Test 4: Partition Healing
```rust
#[test]
fn test_partition_healing() {
    let mut node_a = GossipNode::new("A");
    let mut node_b = GossipNode::new("B");
    
    // Initial sync
    let id1 = node_a.announce(b"before-partition".to_vec());
    node_b.pull(node_a.entries.get(&id1).unwrap().clone());
    
    // PARTITION: nodes can't communicate
    
    // Node A continues
    node_a.announce(b"a-during".to_vec());
    
    // Node B continues
    node_b.announce(b"b-during".to_vec());
    
    // PARTITION HEALED: anti-entropy runs
    let missing_on_b = node_a.get_missing(&node_b.vector_clock);
    for id in missing_on_b {
        node_b.pull(node_a.entries.get(&id).unwrap().clone());
    }
    
    let missing_on_a = node_b.get_missing(&node_a.vector_clock);
    for id in missing_on_a {
        node_a.pull(node_b.entries.get(&id).unwrap().clone());
    }
    
    // Both nodes converge
    assert_eq!(node_a.entries.len(), 3);
    assert_eq!(node_b.entries.len(), 3);
}
```

## Done When
- Vector clocks track causality correctly
- Nodes detect missing entries via clock comparison
- Anti-entropy sync achieves convergence
- Partition healing works (nodes diverge then reconverge)
- All tests pass

## Resources
- `icn-gossip/src/vector_clock.rs`
- `icn-gossip/src/gossip.rs` (anti-entropy logic)
- [Vector Clock Wikipedia](https://en.wikipedia.org/wiki/Vector_clock)
