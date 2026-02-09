# Workshop 5: Network and Gossip Deep Dive

## Learning Objectives

By the end of this workshop, you will be able to:

1. **Trace message flow** - Follow a message from QUIC transport to gossip handler
2. **Explain vector clocks** - Understand how causality is tracked between nodes
3. **Configure topics** - Create topics with appropriate access control
4. **Debug sync issues** - Use anti-entropy to diagnose and repair partition gaps
5. **Write gossip tests** - Create multi-node integration tests using TestNode

## Goal
Understand how messages flow from the transport layer through the gossip protocol,
including topic subscriptions, vector clocks, and anti-entropy synchronization.

## Prerequisites
- Completed [Module 5: Networking](../reference/module-05-network-gossip.md)
- ICN binaries built (`cargo build`)
- Two terminal windows available

## Estimated time
2-3 hours

## Related Materials
- [Module 5: Networking](../reference/module-05-network-gossip.md) - Background reading
- [Workshop 6: Ledger and Contracts](./workshop-06-ledger-contracts.md) - Uses gossip for ledger sync
- [CLAUDE.md Gossip Protocol](../../CLAUDE.md) - Protocol reference

## Part 1: Network Actor Structure

### Steps
1. Open `icn/crates/icn-net/src/actor.rs`
2. Find the `NetworkActor` struct definition
3. Identify the key components: endpoint, sessions, router

### Questions to answer
1. What QUIC library is used for transport?
2. How are active peer sessions tracked?
3. What happens when a new connection arrives?

### Code to find
```rust
pub struct NetworkActor {
    endpoint: quinn::Endpoint,
    sessions: HashMap<Did, PeerSession>,
    // ...
}
```

### Checkpoint
- [ ] You can identify the QUIC endpoint
- [ ] You understand how sessions are managed

## Part 2: Message Protocol

### Steps
1. Open `icn/crates/icn-net/src/protocol.rs`
2. Find the `NetworkMessage` struct
3. List all `MessagePayload` variants

### Expected variants
- `Gossip` - Gossip protocol messages
- `Rpc` - RPC request/response
- `Subscribe` - Topic subscription
- `Hello` - Initial handshake
- `Signed` - Signed envelope wrapper

### Questions to answer
1. How is the sender's DID included in messages?
2. What serialization format is used?
3. How are messages framed on the wire?

### Code to examine
```rust
pub struct NetworkMessage {
    pub from_did: Did,
    pub to_did: Option<Did>,
    pub payload: MessagePayload,
    pub timestamp: u64,
}
```

### Checkpoint
- [ ] You can list all message payload types
- [ ] You understand the message envelope structure

## Part 3: Network-to-Gossip Bridge

### Steps
1. Open `icn/crates/icn-core/src/supervisor/mod.rs`
2. Find where the incoming handler is wired
3. Trace how gossip messages are delivered

### Diagram: Message Flow
```
┌─────────────┐     QUIC      ┌─────────────┐
│  Remote Peer│──────────────►│ NetworkActor│
└─────────────┘               └──────┬──────┘
                                     │
                              NetworkMessage
                                     │
                                     ▼
                          ┌─────────────────────┐
                          │ IncomingMsgHandler  │
                          │   (callback)        │
                          └──────────┬──────────┘
                                     │
                              GossipMessage
                                     │
                                     ▼
                          ┌─────────────────────┐
                          │   GossipActor       │
                          │   handle_message()  │
                          └─────────────────────┘
```

### Questions to answer
1. What type is `IncomingMessageHandler`?
2. How does error handling work in the callback?
3. Can the callback block the network actor?

### Checkpoint
- [ ] You can explain the bridge between network and gossip
- [ ] You understand callback-based decoupling

## Part 4: Gossip Topics and Subscriptions

### Steps
1. Open `icn/crates/icn-gossip/src/gossip.rs`
2. Find the subscription management code
3. Identify how topics are organized

### Topic naming convention
```
namespace:purpose
```
Examples:
- `ledger:entries` - Ledger transactions
- `trust:edges` - Trust graph updates
- `governance:votes` - Voting events
- `coop:<id>:announcements` - Cooperative-specific messages

### Questions to answer
1. How are subscriptions stored?
2. What is `AccessControl` and how does it work?
3. How are subscription notifications delivered?

### Exercise
Search for topic definitions:
```bash
grep -r "topic\|Topic" icn/crates/icn-gossip/src/ --include="*.rs" | grep -v "^Binary" | head -20
```

### Checkpoint
- [ ] You understand topic naming conventions
- [ ] You can explain subscription management

## Part 5: Vector Clocks

### Steps
1. Open `icn/crates/icn-gossip/src/sync/` (or search for VectorClock)
2. Find the `VectorClock` implementation
3. Understand how causality is tracked

### How vector clocks work
```
Node A: {A: 1, B: 0}  sends message
Node B: {A: 0, B: 1}  receives, merges to {A: 1, B: 1}
Node A: {A: 2, B: 0}  sends another
Node B: {A: 2, B: 1}  merges, knows it missed one
```

### Questions to answer
1. How does `happened_before()` comparison work?
2. What happens when clocks are concurrent (neither before the other)?
3. How do vector clocks prevent processing duplicates?

### Code to find
```rust
impl VectorClock {
    pub fn merge(&mut self, other: &VectorClock) {
        for (did, &time) in &other.clock {
            let entry = self.clock.entry(did.clone()).or_insert(0);
            *entry = (*entry).max(time);
        }
    }
}
```

### Checkpoint
- [ ] You can explain vector clock merge semantics
- [ ] You understand how causality is tracked

## Part 6: Anti-Entropy Protocol

### Steps
1. Search for anti-entropy code:
   ```bash
   grep -r "anti.entropy\|AntiEntropy" icn/crates/icn-gossip/ --include="*.rs"
   ```
2. Find the periodic sync mechanism
3. Understand how gaps are detected and filled

### Anti-entropy phases
1. **Digest Exchange**: Nodes share Bloom filter of known entries
2. **Gap Detection**: Compare digests to find missing entries
3. **Pull Request**: Request specific missing entries
4. **Data Transfer**: Send requested entries

### Questions to answer
1. How often does anti-entropy run?
2. What information is in a sync digest?
3. How are Bloom filters used to minimize bandwidth?

### Checkpoint
- [ ] You understand the anti-entropy protocol
- [ ] You can explain how partitions are healed

## Part 7: Two-Node Gossip Test

### Steps
1. Find the gossip integration tests:
   ```bash
   ls icn/crates/icn-gossip/tests/ 2>/dev/null || ls icn/crates/icn-core/tests/
   ```

2. Read a multi-node test case
3. Understand how test nodes are set up

### Test pattern

From `icn/crates/icn-testkit/src/node.rs` - the actual TestNode API:

```rust
#[tokio::test]
async fn test_two_node_gossip() -> anyhow::Result<()> {
    use icn_gossip::AccessControl;

    let node1 = TestNode::new(4001).await?;
    let node2 = TestNode::new(4002).await?;

    // Connect nodes
    node1.connect(&node2).await?;

    // Create topic on both nodes
    node1.create_topic("test:topic", AccessControl::Public).await?;
    node2.create_topic("test:topic", AccessControl::Public).await?;

    // Subscribe using each node's DID
    let did1 = node1.did();
    let did2 = node2.did();
    node1.subscribe("test:topic", &did1).await?;
    node2.subscribe("test:topic", &did2).await?;

    // Publish and verify propagation
    let hash = node1.publish("test:topic", b"hello").await?;

    // Wait for gossip propagation
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(node2.has_entry("test:topic", &hash).await?);

    Ok(())
}
```

**Key differences from pseudocode**:
- `connect(&node2)` not `connect_to(&node2)`
- `subscribe()` requires the subscriber DID
- `publish()` returns a hash, not the message
- Use `has_entry()` to verify propagation

### Questions to answer
1. How are unique ports assigned to test nodes?
2. How does the test wait for gossip propagation?
3. What cleanup happens after the test?

### Checkpoint
- [ ] You can read and understand gossip tests
- [ ] You know how to set up multi-node test scenarios

## Part 8: Observing Gossip (Optional)

If you have two terminals and want to see gossip in action:

### Prerequisites
First, verify the ports are available:
```bash
# Check if ports 4001 and 4002 are free
lsof -i :4001 -i :4002 2>/dev/null || echo "Ports available"
```

### Steps
1. Terminal 1 - Start first node:
   ```bash
   export ICN_DATA=$(mktemp -d)
   export ICN_PASSPHRASE="node1"
   cd icn && ./target/debug/icnctl --data-dir "$ICN_DATA" id init
   RUST_LOG=icn_gossip=debug ./target/debug/icnd --data-dir "$ICN_DATA" --port 4001
   ```

2. Terminal 2 - Start second node:
   ```bash
   export ICN_DATA=$(mktemp -d)
   export ICN_PASSPHRASE="node2"
   cd icn && ./target/debug/icnctl --data-dir "$ICN_DATA" id init
   RUST_LOG=icn_gossip=debug ./target/debug/icnd --data-dir "$ICN_DATA" --port 4002
   ```

3. Watch for gossip log messages when nodes discover each other via mDNS

### Expected observations
- "New peer discovered" messages
- "Subscribing to topic" messages
- Anti-entropy sync events

### Checkpoint
- [ ] You observed gossip messages in logs
- [ ] You understand what each log message means

## Summary

After completing this workshop you should be able to:
- Trace message flow from network to gossip layer
- Understand topic subscription and access control
- Explain vector clocks and causality tracking
- Describe the anti-entropy protocol
- Read and write gossip integration tests

## Key Takeaways

| Concept | Key Point |
|---------|-----------|
| **QUIC Transport** | Quinn library provides reliable, encrypted streams over UDP |
| **Vector Clocks** | Each node maintains `{DID: counter}` map; merge takes max of each entry |
| **Topics** | Namespaced strings (`ledger:entries`); access control per topic |
| **Anti-Entropy** | Periodic Bloom filter exchange detects and fills gaps |
| **Message Flow** | Network → IncomingHandler callback → GossipActor |

## Try It Yourself

**Challenge 1**: Simulate a network partition
1. Start three nodes (ports 4001, 4002, 4003)
2. Create a topic and subscribe all three
3. Block traffic between node 1 and node 3 (e.g., firewall rule)
4. Publish messages from node 1
5. Watch anti-entropy heal the partition when you unblock

**Challenge 2**: Write a custom gossip test
Using the `TestNode` pattern from Part 7, write a test that:
1. Creates 3 nodes in a chain (1↔2↔3, but not 1↔3)
2. Publishes a message from node 1
3. Verifies the message reaches node 3 via node 2
4. Measures propagation delay

**Challenge 3**: Explore vector clock behavior
Add logging to trace vector clock evolution:
```bash
RUST_LOG=icn_gossip::sync=trace ./target/debug/icnd --data-dir "$ICN_DATA"
```
Observe how clocks merge when messages are received.

## Troubleshooting

### Nodes don't discover each other
mDNS may be blocked. Try explicit peer connection or check firewall settings.

### Messages not propagating
Check that both nodes are subscribed to the same topic with matching access control.

### Vector clock inconsistencies
Verify that node identities (DIDs) are unique across all nodes.

## Next steps
Proceed to Workshop 6: Ledger and Contract Flow
