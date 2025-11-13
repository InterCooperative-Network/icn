# 2025-11-13: Topology Integration Tests & Bidirectional Handshake

## Overview

Completed comprehensive topology integration test suite and fixed critical issues with the bidirectional handshake protocol. All 6 topology integration tests now pass consistently, validating topology-aware peer categorization, LRU eviction, and scope-aware peer sampling.

## Context

This work builds on Phase 1E-1F (topology-aware networking integration) to provide thorough test coverage for:
- Peer categorization based on region/cluster/trust
- NeighborSets with LRU eviction
- Scope-aware peer sampling for gossip fanout
- Bidirectional handshake protocol for topology exchange

## Changes Made

### 1. Bidirectional Handshake Protocol (`icn-net/src/actor.rs`)

**Problem:** Outbound connections created by `dial()` couldn't receive messages because no connection handler was spawned. This prevented peers from receiving handshake responses, leaving NeighborSets empty on the initiating peer.

**Solution:**
- Spawn connection handler for outbound connections after successful dial
- Open new bidirectional stream for handshake response (can't reuse read stream)
- Always send handshake response (either full handshake or HandshakeAck)

**Code Changes:**
```rust
// In NetworkMsg::Dial handler (lines 368-391)
.map(|connection| {
    // ... existing metrics code ...

    // Spawn connection handler for outbound connection if handler is available
    if let Some(handler) = self.incoming_handler.clone() {
        tokio::spawn(async move {
            if let Err(e) = Self::handle_connection(
                connection.clone(),
                handler,
                rate_limiter,
                neighbor_sets,
                topology_config,
                trust_graph,
                session_manager,
                own_did,
            ).await {
                warn!("Outbound connection handler error: {}", e);
            }
        });
    }
```

```rust
// In handshake message handler (lines 725-763)
// Send our own handshake back (bidirectional exchange)
// Open a new stream since the original stream is finished
let connection_clone = connection.clone();
tokio::spawn(async move {
    match connection_clone.open_bi().await {
        Ok((mut new_send, _new_recv)) => {
            let response_msg = if let Some(ref topo_cfg) = topo_cfg_clone {
                // Send full handshake if topology is enabled
                NetworkMessage::handshake(...)
            } else {
                // Send ack if topology is disabled
                NetworkMessage::handshake_ack(...)
            };
            write_message(&mut new_send, &response_msg).await?;
        }
    }
});
```

### 2. Test Infrastructure Improvements (`icn-core/tests/topology_integration.rs`)

**Problem:** The `test_neighbor_set_lru_eviction` test was creating a `TopologyConfig` with `max_local_cluster: 2` but never using it. Node A was spawned with default limits (10), so connecting to 3 peers didn't trigger LRU eviction.

**Solution:**
- Added `TestNode::spawn_with_limits()` method accepting `Option<NeighborLimitsConfig>`
- Made original `spawn()` method call `spawn_with_limits(None)` for backward compatibility
- Updated `test_neighbor_set_lru_eviction` to use custom limits

**Code Changes:**
```rust
impl TestNode {
    async fn spawn(
        port: u16,
        region: String,
        cluster_id: String,
    ) -> Result<Self> {
        Self::spawn_with_limits(port, region, cluster_id, None).await
    }

    async fn spawn_with_limits(
        port: u16,
        region: String,
        cluster_id: String,
        custom_limits: Option<NeighborLimitsConfig>,
    ) -> Result<Self> {
        // Use custom_limits if provided, otherwise defaults
        let neighbor_limits = custom_limits.unwrap_or(NeighborLimitsConfig {
            max_local_cluster: 10,
            max_regional: 10,
            max_backbone: 5,
            max_trusted: 20,
        });
        // ... rest of spawn logic
    }
}
```

### 3. Comprehensive Test Suite

Created 6 integration tests covering all topology scenarios:

1. **test_local_cluster_categorization** - Same region/cluster → LocalCluster
2. **test_regional_categorization** - Same region, different cluster → Regional
3. **test_backbone_categorization** - Different regions → Backbone
4. **test_multi_region_topology** - Mixed 4-node topology with all categories
5. **test_scope_aware_peer_sampling** - Scope-based peer selection for fanout
6. **test_neighbor_set_lru_eviction** - LRU eviction with strict limits

## Test Results

All 6 topology integration tests pass consistently:
```
test test_backbone_categorization ... ok
test test_local_cluster_categorization ... ok
test test_regional_categorization ... ok
test test_neighbor_set_lru_eviction ... ok
test test_multi_region_topology ... ok
test test_scope_aware_peer_sampling ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Full workspace: **270+ tests passing**

## Debugging Journey

### Issue 1: Handshake Response Write Failure
- **Error:** "Failed to write message length" when sending handshake response
- **Cause:** Attempting to write to a stream already used for reading
- **Fix:** Open new bidirectional stream using `connection.open_bi()` for response

### Issue 2: Node A Not Receiving Handshake Response
- **Error:** Node A sends handshake, Node B receives and adds Node A to NeighborSets, but Node A's neighbor count stays at 0
- **Cause:** Outbound connections from `dial()` didn't spawn connection handlers
- **Fix:** Spawn connection handler for outbound connections in `NetworkMsg::Dial` handler

### Issue 3: Protocol Compatibility with Mixed Topology Settings
- **User Feedback:** Handshake response must always acknowledge, regardless of topology config
- **Cause:** Response was conditional on `topology_config.is_some()`
- **Fix:** Always send response - either full handshake or HandshakeAck

### Issue 4: LRU Eviction Test Not Enforcing Limits
- **Error:** Test showed 3 neighbors when limit was 2
- **Cause:** `TopologyConfig` with strict limits was created but never applied to node
- **Fix:** Added `spawn_with_limits()` and updated test to pass custom limits

### Issue 5: Test Failure in Parallel Execution
- **Symptom:** Test passed alone but failed with "aborted by peer" when run with others
- **Resolution:** After fixing custom limits, test passes consistently in parallel

## Technical Insights

### Rate Limiting During Handshake
Bidirectional handshake (both peers send handshakes + responses) triggers rate limiting warnings:
```
WARN icn_net::actor: Rate limited message from did:icn:z24DH... (exceeded limit)
```

This is **expected and correct** - the rate limiter is protecting against handshake floods. With trust score 0.0, peers default to "isolated" tier (10 msg/sec, burst 2), which can be exceeded during rapid bidirectional handshake exchange.

### LRU Eviction Behavior
When connecting to 3 peers with `max_local_cluster: 2`:
- First connection: Added to LocalCluster (count: 1)
- Second connection: Added to LocalCluster (count: 2)
- Third connection: Added to LocalCluster, oldest evicted (count: 2)

Log output confirms proper eviction:
```
INFO Node A neighbor counts after connecting to 3 peers:
  NeighborCounts { local_cluster: 2, regional: 0, backbone: 0, trusted: 0 }
```

### Handshake Flow
```
1. Node A dials Node B
2. Node A sends handshake to Node B (on new outbound connection)
3. Node A spawns connection handler to receive messages on this connection
4. Node B receives handshake, adds Node A to NeighborSets
5. Node B opens new bidirectional stream
6. Node B sends handshake response to Node A (full handshake or ack)
7. Node A's connection handler receives handshake response
8. Node A adds Node B to NeighborSets
9. Both peers now have each other in their NeighborSets ✅
```

## Integration Points

### NeighborSets API (`icn-net/src/topology.rs`)
Provides count methods for test inspection:
- `local_cluster_count()` - Count of LocalCluster neighbors
- `regional_count()` - Count of Regional neighbors
- `backbone_count()` - Count of Backbone neighbors
- `trusted_count()` - Count of explicitly trusted neighbors

### Peer Categorization Logic
Uses region/cluster/trust for categorization:
- **LocalCluster:** Same region + same cluster + trust ≥ 0.1
- **Regional:** Same region + different cluster + trust ≥ 0.1
- **Backbone:** Different region + trust ≥ 0.4
- **Trusted:** Explicitly trusted regardless of topology

### Scope-Aware Peer Sampling
Gossip actor can request peers by scope:
- `Scope::LocalCluster` - Samples from LocalCluster neighbors
- `Scope::Regional` - Samples from LocalCluster + Regional neighbors
- `Scope::Global` - Samples from all neighbors

## Next Steps

The topology-aware networking foundation is now complete with comprehensive test coverage. Remaining federation work:

1. **Trust-gated gossip topics** - Integrate topology with access control
2. **Multi-region gossip fanout** - Use scope-aware sampling in anti-entropy
3. **Federation handshake** - Negotiate capabilities and protocol versions
4. **Cross-region optimizations** - Adaptive fanout based on RTT and reliability

## References

- Commit: `3330942` - Fix topology integration tests and handshake protocol
- Related: Phase 1E-1F topology configuration (commits `1f966a7`, `67e268b`)
- Tests: `icn/crates/icn-core/tests/topology_integration.rs`
- Implementation: `icn/crates/icn-net/src/actor.rs`
- API: `icn/crates/icn-net/src/topology.rs`

## Lessons Learned

1. **Outbound connections need handlers too** - Easy to forget that `dial()` creates connections that need to receive messages, not just send them

2. **Stream lifecycle matters** - Can't reuse a stream for both reading and writing; must open new stream for bidirectional exchange

3. **Test infrastructure pays off** - Investing in `spawn_with_limits()` makes testing edge cases much easier

4. **Rate limiting needs tuning for handshakes** - May need special handling or trust bootstrapping to avoid limiting legitimate handshakes

5. **Parallel test execution reveals issues** - Resource contention and timing issues only appear when tests run together

6. **Always acknowledge protocol messages** - Handshake protocol must work regardless of feature flags or config, otherwise it breaks mixed deployments
