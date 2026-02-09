# 08: Network and Gossip — Eventual Consistency Without Consensus

> **Phase**: 3 | **Tier**: Builder  
> **Patterns introduced**: [Serialization](#serialization), [Anti-Entropy](#anti-entropy)  
> **Prerequisite**: 07-identity-and-crypto.md

## Why This Matters

ICN achieves **eventual consistency** through gossip, not global consensus. Nodes exchange state via push announcements, pull requests, and periodic anti-entropy sync. Understanding gossip is essential for debugging replication issues and adding new replicated state.

→ See `manual.md` § "Gossip Without Consensus" for design rationale.

## What You'll Read

### 1. Transport vs Gossip Separation

**Transport** (`icn-net`): How bytes move between nodes  
**Gossip** (`icn-gossip`): What bytes represent (application-level state)

**Why separate**: Allows transport changes (QUIC → HTTP/3) without touching gossip logic.

### 2. QUIC Protocol Rationale

**Why QUIC**:
- Multiplexing: Multiple streams over one connection
- Built-in TLS: Encryption by default
- Connection migration: Survives IP changes
- Low latency: 0-RTT for established connections

**File**: `icn-net/src/protocol.rs`

### 3. mDNS Peer Discovery

**File**: `icn-net/src/mdns.rs`

Local network discovery without central registry:
```rust
// Announce self
mdns.announce_service("_icn._tcp.local", port)?;

// Discover peers
let peers = mdns.discover_peers(timeout)?;
```

### 4. GossipMessage Enum

**File**: `icn-gossip/src/types.rs`

```rust
pub enum GossipMessage {
    Announcement { topic: String, entry_id: EntryId, data: Vec<u8> },
    PullRequest { topic: String, missing: Vec<EntryId> },
    PullResponse { topic: String, entries: Vec<GossipEntry> },
    AntiEntropy { topic: String, bloom_filter: BloomFilter },
}
```

**Push**: Announce new content to all subscribers  
**Pull**: Request missing content by ID  
**Anti-entropy**: Periodic Bloom filter exchange to detect missing entries

### 5. Vector Clocks for Causality

**File**: `icn-gossip/src/vector_clock.rs`

Tracks happens-before relationships:
```rust
pub struct VectorClock {
    clocks: HashMap<Did, u64>,
}

impl VectorClock {
    pub fn increment(&mut self, did: &Did) {
        *self.clocks.entry(did.clone()).or_insert(0) += 1;
    }
    
    pub fn merge(&mut self, other: &VectorClock) {
        for (did, &count) in &other.clocks {
            let entry = self.clocks.entry(did.clone()).or_insert(0);
            *entry = (*entry).max(count);
        }
    }
}
```

Prevents duplicate processing: "Have I seen this entry before?"

### 6. Topic-Based Routing

**Access control**:
- `Public`: Anyone can subscribe
- `Private`: Invitation-only
- `TrustGated`: Requires minimum trust score

**File**: `icn-gossip/src/gossip.rs`

```rust
pub enum AccessControl {
    Public,
    Private { allowed: Vec<Did> },
    TrustGated { min_trust: f64 },
}
```

## What You'll Build

→ **Lab**: `labs/lab-07-gossip-sync/`

Implement vector clocks + anti-entropy sync between two in-memory nodes:
- Node A announces entry
- Node B pulls missing entry
- Both converge on same state

**Done when**: Two nodes converge after partition healing.

## Checkpoint

You've completed this layer when you can:

1. **Trace gossip flow**: Diagram push → pull → anti-entropy for one entry
2. **Explain vector clocks**: Show how they prevent duplicate processing
3. **Debug replication**: Given "node B never receives entry X", diagnose cause
4. **Implement anti-entropy**: Add Bloom filter sync to your lab

**Artifact**: Lab-07 complete with convergence tests.

## Deep Reference

→ `reference/module-05-network-gossip.md` — Full protocol details, QUIC config  
→ `icn-gossip/src/gossip.rs` — Source code, subscription model  
→ [QUIC RFC 9000](https://www.rfc-editor.org/rfc/rfc9000.html) — Protocol spec
