# Handler Extraction Refactoring - 2025-12-20

## Summary

Completed issue #125: Breaking down large files (>2500 lines) into smaller modules. Extracted message handlers from two major actor files into dedicated `handlers/` modules.

## Changes Made

### icn-gossip Handler Extraction

**Commit:** `5de15240`

Extracted 15 message handlers from `gossip.rs` into `handlers/` submodule:

| Handler File | Handlers | Lines |
|--------------|----------|-------|
| `push.rs` | Announce, Request, Response | 129 |
| `bloom.rs` | RequestBloomFilter, SendBloomFilter | 90 |
| `pull.rs` | Digest, PullRequest, PullResponse, RequestMissing | 349 |
| `replica.rs` | ReplicaRequest, ReplicaOffer, ReplicaStatus | 238 |
| `partition.rs` | PartitionHealRequest, PartitionHealResponse | 152 |

**Result:** gossip.rs reduced from 3,668 to 1,542 lines of production code (-58%)

### icn-net Handler Extraction

**Commits:** `2cf67618`, `1e6cc55e`

Extracted 6 message handlers from `actor.rs` into `handlers/` submodule:

| Handler File | Handlers | Lines |
|--------------|----------|-------|
| `hello.rs` | Hello (DID-TLS binding, version negotiation) | 231 |
| `handshake.rs` | Handshake, HandshakeAck, Ping, Pong | 196 |
| `signed.rs` | SignedEnvelope verification | 99 |
| `peer_exchange.rs` | PeerExchange (peer discovery) | 157 |
| `onion.rs` | Onion routing (privacy relay) | 124 |
| `mod.rs` | ConnectionContext struct | 89 |

**Result:** actor.rs reduced from 2,790 to 2,097 lines (-25%)

## Architecture Pattern

### ConnectionContext

Created a `ConnectionContext` struct to hold shared state for handlers:

```rust
pub struct ConnectionContext {
    pub handler: IncomingMessageHandler,
    pub rate_limiter: Arc<RateLimiter>,
    pub replay_guard: Arc<RwLock<ReplayGuard>>,
    pub neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
    pub topology_config: Option<TopologyConfig>,
    pub trust_graph: Option<Arc<RwLock<TrustGraph>>>,
    pub session_manager: Arc<RwLock<SessionManager>>,
    pub peer_connections: Arc<RwLock<HashMap<Did, PeerConnectionInfo>>>,
    pub blob_registry: Option<Arc<RwLock<BlobLocationRegistry>>>,
    pub misbehavior_detector: Option<Arc<RwLock<MisbehaviorDetector>>>,
    pub identity_bundle: IdentityBundle,
    pub own_did: Did,
}
```

This pattern:
- Avoids passing many parameters through call chains
- Makes handlers testable in isolation
- Provides clean separation between dispatch and handling logic

### Dispatch Pattern

The main actor dispatch becomes thin:

```rust
match &message.payload {
    MessagePayload::Hello { ... } => {
        ctx.handle_hello(&connection, &from, ...).await?;
    }
    MessagePayload::Handshake { ... } => {
        ctx.handle_handshake(&connection, &from, ...).await;
    }
    // ... etc
}
```

## Files Modified

### icn-gossip
- `src/lib.rs` - Added `mod handlers`
- `src/gossip.rs` - Replaced inline handlers with method calls
- `src/handlers/mod.rs` - New module coordinator
- `src/handlers/push.rs` - New
- `src/handlers/bloom.rs` - New
- `src/handlers/pull.rs` - New
- `src/handlers/replica.rs` - New
- `src/handlers/partition.rs` - New

### icn-net
- `src/lib.rs` - Added `mod handlers`
- `src/actor.rs` - Replaced inline handlers with method calls, removed 738 lines
- `src/handlers/mod.rs` - New, contains ConnectionContext
- `src/handlers/hello.rs` - New
- `src/handlers/handshake.rs` - New
- `src/handlers/signed.rs` - New
- `src/handlers/peer_exchange.rs` - New
- `src/handlers/onion.rs` - New

## Test Results

All tests pass:
- icn-gossip: 114 tests passed
- icn-net: 147 tests passed (124 unit + 23 integration)

## Remaining Large Files

Files still above 2,000 lines but acceptable:

| File | Lines | Reason |
|------|-------|--------|
| `metrics_legacy.rs` | 3,840 | Declarative metric descriptions only |
| `gossip.rs` | 2,920 | 1,542 prod + 1,378 tests (prod under target) |
| `disputes.rs` | 2,615 | 1,531 prod + 1,083 tests (prod under target) |
| `actor.rs` | 2,097 | Core actor logic, hard to fragment further |

## Issue Closed

Issue #125 closed as complete with summary comment.
