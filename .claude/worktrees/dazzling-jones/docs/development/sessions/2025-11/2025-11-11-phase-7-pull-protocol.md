# Phase 7 - Pull Protocol Implementation

**Date**: 2025-11-11
**Status**: Complete ✅
**Effort**: ~2 hours

## Overview

Completed the pull protocol (Request/Response) flow for gossip synchronization, enabling nodes to request missing entries on-demand rather than relying solely on push-based Announce messages.

## Problem Statement

Phase 6 implemented the push-based gossip protocol where nodes broadcast Announce messages when they have new entries. However, the pull mechanism was incomplete:

1. When receiving an Announce for a missing entry, nodes didn't request it
2. When receiving a Request, nodes didn't send back the full entry
3. RequestBloomFilter and RequestMissing handlers were stubs
4. No bidirectional communication between GossipActor and NetworkActor

The challenge was that `GossipActor` had no way to send messages back - it could only receive them via `handle_message()`.

## Architecture Challenge: Circular Dependencies

The naive approach would be to add `NetworkHandle` to `GossipActor`:

```rust
pub struct GossipActor {
    network_handle: Option<NetworkHandle>,  // ❌ Creates circular dependency
    // ...
}
```

This creates a circular dependency:
- `icn-gossip` would need to depend on `icn-net` (for NetworkHandle)
- `icn-net` already depends on `icn-gossip` (for GossipMessage types)

## Solution: Callback-Based Architecture

Instead of tight coupling, we use a callback pattern similar to the existing `trust_lookup`:

```rust
pub type SendMessageCallback = Arc<dyn Fn(Option<Did>, GossipMessage) + Send + Sync>;

pub struct GossipActor {
    send_callback: Option<SendMessageCallback>,
    // ...
}
```

Benefits:
1. **No circular dependency** - gossip doesn't depend on net
2. **Flexible** - callback can be any implementation (mock in tests, real network in production)
3. **Consistent** - matches existing `trust_lookup` pattern
4. **Testable** - easy to inject test callbacks to verify messages sent

## Implementation

### 1. GossipActor Changes ([gossip.rs:15-101](../crates/icn-gossip/src/gossip.rs#L15-L101))

Added callback infrastructure:

```rust
pub type SendMessageCallback = Arc<dyn Fn(Option<Did>, GossipMessage) + Send + Sync>;

pub struct GossipActor {
    // ... existing fields
    send_callback: Option<SendMessageCallback>,
}

impl GossipActor {
    pub fn set_send_callback(&mut self, callback: SendMessageCallback) {
        self.send_callback = Some(callback);
    }

    fn send_message(&self, recipient: Option<Did>, message: GossipMessage) {
        if let Some(callback) = &self.send_callback {
            callback(recipient, message);
        } else {
            debug!("Cannot send message - no send callback set");
        }
    }
}
```

### 2. Message Handler Implementation ([gossip.rs:257-403](../crates/icn-gossip/src/gossip.rs#L257-L403))

Completed all TODO handlers:

#### Announce Handler
```rust
GossipMessage::Announce { hash, author, .. } => {
    // Check if we already have this entry
    if let Some(entries) = self.entries.get(&topic) {
        if entries.contains_key(&hash) {
            return Ok(()); // Already have it
        }
    }

    // Request full entry if missing
    self.send_message(Some(author), GossipMessage::Request { hash });
    Ok(())
}
```

#### Request Handler
```rust
GossipMessage::Request { hash } => {
    // Find entry across all topics
    for (_topic_name, entries) in &self.entries {
        if let Some(entry) = entries.get(&hash) {
            // Send Response with the entry
            self.send_message(None, GossipMessage::Response {
                entry: entry.clone(),
            });
            return Ok(());
        }
    }
    Ok(())
}
```

#### RequestBloomFilter Handler
```rust
GossipMessage::RequestBloomFilter { topic } => {
    if let Some(filter) = self.bloom_filters.get(&topic) {
        let filter_data = filter.to_data();
        self.send_message(None, GossipMessage::SendBloomFilter {
            topic: topic.clone(),
            filter: filter_data,
        });
    }
    Ok(())
}
```

#### RequestMissing Handler
```rust
GossipMessage::RequestMissing { hashes } => {
    for hash in hashes {
        for (_topic_name, entries) in &self.entries {
            if let Some(entry) = entries.get(&hash) {
                self.send_message(None, GossipMessage::Response {
                    entry: entry.clone(),
                });
                break;
            }
        }
    }
    Ok(())
}
```

### 3. Supervisor Integration ([supervisor.rs:95-124](../crates/icn-core/src/supervisor.rs#L95-L124))

Wire up the callback using NetworkHandle:

```rust
// Set send callback on gossip actor to enable request/response
{
    let mut gossip = gossip_handle.write().await;
    let network_handle_clone = network_handle.clone();
    let own_did_clone = did.clone();

    let send_callback: icn_gossip::SendMessageCallback = Arc::new(move |recipient, gossip_msg| {
        let net_handle = network_handle_clone.clone();
        let from_did = own_did_clone.clone();

        // Spawn async task to send message
        tokio::spawn(async move {
            let result = if let Some(target_did) = recipient {
                // Unicast
                let net_msg = icn_net::NetworkMessage::gossip(from_did, Some(target_did.clone()), gossip_msg);
                net_handle.send_message(target_did, net_msg).await
            } else {
                // Broadcast
                let net_msg = icn_net::NetworkMessage::gossip(from_did, None, gossip_msg);
                net_handle.broadcast(net_msg).await
            };

            if let Err(e) = result {
                warn!("Failed to send gossip message: {}", e);
            }
        });
    });

    gossip.set_send_callback(send_callback);
}
```

Key points:
- Callback clones NetworkHandle and DIDs to move into closure
- Spawns async task for each send (callback itself is synchronous)
- Handles both unicast (Some(did)) and broadcast (None) routing

### 4. Integration Tests ([gossip.rs:554-668](../crates/icn-gossip/src/gossip.rs#L554-L668))

Added two comprehensive tests:

#### test_pull_protocol_request_response
Tests the full Announce → Request → Response flow:
1. Node 1 publishes entry, Node 2 doesn't have it
2. Node 2 receives Announce and sends Request
3. Node 1 receives Request and sends Response
4. Verifies correct messages sent at each step

#### test_request_missing_handler
Tests RequestMissing message handling:
1. Node publishes 3 entries
2. Receives RequestMissing for 2 entries
3. Sends 2 Response messages
4. Verifies responses contain correct entries

## Message Flow Example

Complete pull protocol sequence:

```
Node A                                    Node B
  |                                         |
  | --- Announce(hash=X, author=A) -----> |
  |                                         | (checks local store)
  |                                         | (doesn't have hash X)
  | <---- Request(hash=X) ---------------- |
  |                                         |
  | (finds entry X in local store)         |
  | --- Response(entry=X) ---------------> |
  |                                         | (stores entry X)
  |                                         | (updates bloom filter)
```

## Design Decisions

### 1. Synchronous Callback with Async Spawn

The callback itself is `Fn` (synchronous), but spawns async tasks:

```rust
Arc<dyn Fn(Option<Did>, GossipMessage) + Send + Sync>
```

**Why?**
- GossipActor's `handle_message()` is `&mut self` (not async)
- Can't await in a sync function
- Solution: Spawn async task inside callback

**Alternative considered**: Make callback `async Fn`
- Would require `handle_message()` to be async
- Would propagate async through entire call chain
- More invasive change

### 2. Broadcast vs. Unicast

When to use each:

**Unicast (Some(did))**:
- Request messages (we know who has the data)
- Direct responses to specific peers

**Broadcast (None)**:
- Response messages (we don't know who requested)
- Anti-entropy messages (everyone should see)

**Future improvement**: Add source DID to Request messages so Response can be unicast.

### 3. SendBloomFilter Limitation

Current implementation notes:

```rust
// For a full implementation, we'd need to:
// 1. Compare our bloom filter with the remote one
// 2. Identify hashes present in remote but not in ours
// 3. Request those missing hashes
//
// However, bloom filters are probabilistic - they can only tell us
// "definitely not present" or "might be present". We can't extract
// the actual hashes from a bloom filter.
//
// The proper approach is for the sender to also send their entry hashes
// or we need a different anti-entropy approach.
```

**Why the limitation?**
- Bloom filters are one-way: `insert(hash)` → can't `extract_hashes()`
- Can only test membership: `contains(hash)` → bool

**Current anti-entropy approach**:
- Use `RequestMissing` with explicit hash list
- Anti-entropy task broadcasts RequestBloomFilter
- Peers respond with SendBloomFilter
- Comparison happens externally, then RequestMissing sent

## Testing Results

All tests passing:

```bash
$ cargo test -p icn-gossip --lib
running 20 tests
test gossip::tests::test_pull_protocol_request_response ... ok
test gossip::tests::test_request_missing_handler ... ok
[18 other tests] ... ok

test result: ok. 20 passed; 0 failed
```

```bash
$ cargo test -p icn-net --lib
running 15 tests
[12 tests] ... ok
[3 tests] ... ignored

test result: ok. 12 passed; 0 failed; 3 ignored
```

```bash
$ cargo build --release
Finished `release` profile [optimized] target(s) in 19.66s
```

## Performance Considerations

1. **Message Overhead**
   - Each Announce triggers a Request (if missing)
   - Each Request triggers a Response
   - For N new entries across M nodes: O(N*M) messages

2. **Async Spawn Per Message**
   - Each callback spawns a tokio task
   - Lightweight but not free
   - Alternative: Use a channel for batching (future optimization)

3. **Bloom Filter Efficiency**
   - 1% false positive rate
   - ~9.6 bits per element
   - For 10,000 entries: ~12 KB per topic

## Limitations & Future Work

1. **No Request Deduplication**
   - Multiple Announces for same hash → multiple Requests
   - Could add "pending requests" cache

2. **No Timeout on Requests**
   - If Request is lost, entry never arrives
   - Need retry mechanism with exponential backoff

3. **Broadcast Responses**
   - Response goes to all peers, not just requester
   - Wastes bandwidth
   - Fix: Add source DID to Request message

4. **No Flow Control**
   - Large RequestMissing (1000+ hashes) → flood of Responses
   - Need batching or rate limiting

5. **Synchronous Callback**
   - Spawns task per message
   - Could be more efficient with async callback

## Files Changed

- `crates/icn-gossip/src/gossip.rs` - Added callback, implemented handlers, tests (+115 lines)
- `crates/icn-gossip/src/lib.rs` - Exported SendMessageCallback (+1 line)
- `crates/icn-core/src/supervisor.rs` - Wired send callback (+30 lines)
- `crates/icn-net/src/actor.rs` - Fixed test signature (+1 line)
- `crates/icn-gossip/src/vector_clock.rs` - Fixed unused mut warning (-1 line)

Total: ~150 lines added/modified

## Lessons Learned

1. **Callback Pattern Wins**
   - Avoided circular dependency elegantly
   - Similar to existing `trust_lookup` pattern
   - Easy to test with mock callbacks

2. **Spawn Inside Sync Callback**
   - Enables async operations from sync context
   - Alternative to propagating async everywhere
   - Common pattern in Tokio applications

3. **Message Design Matters**
   - Not including source in Request → broadcast Response
   - Small protocol change → big efficiency impact

4. **Bloom Filter Tradeoffs**
   - Great for "do you have this?" checks
   - Can't extract what's inside
   - Need complementary mechanisms (RequestMissing)

## Next Steps

Phase 7 remaining:
- [x] Metrics exporter (Prometheus) ✅
- [x] Complete pull protocol (Request/Response) ✅
- [ ] Topic subscriptions & routing
- [ ] Production hardening
- [ ] Comprehensive documentation

**Update (2025-11-11)**: Metrics implementation complete! See [2025-11-11-metrics-implementation.md](./2025-11-11-metrics-implementation.md) for full details.

Immediate next: **Topic subscriptions & routing** or **Integration testing**
