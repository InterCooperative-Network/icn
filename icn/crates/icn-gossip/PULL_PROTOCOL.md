# Gossip Pull Protocol

## Overview

The pull protocol extends ICN's gossip system with efficient, trust-aware anti-entropy synchronization. It uses vector clocks and Bloom filters to identify missing entries, then batch-requests them with flow control.

## Message Flow

```
Node A                           Node B
  |                                |
  | ---- Digest ------------------> |   (A announces what it has)
  |      (vector clock + bloom)     |
  |                                 |
  | <--- PullRequest -------------- |   (B requests missing entries)
  |      (want_ids, max_bytes)      |
  |                                 |
  | ---- PullResponse ------------> |   (A sends batch, may truncate)
  |      (entries[], truncated)     |
  |                                 |
```

## Protocol Details

### 1. Digest (Anti-Entropy Hint)

Node periodically broadcasts `Digest` for each topic:
- **Vector clock**: Causal dependency tracking (map<DID, sequence>)
- **Bloom filter**: Probabilistic set of entry hashes (8KB max)
- **hint_count**: Cardinality hint for adaptive bloom sizing
- **nonce**: Correlation ID for request/response matching

**Purpose**: Let peers discover missing entries efficiently without sending full entry lists.

### 2. PullRequest (Targeted Batch Fetch)

On receiving Digest, node compares local state:
1. **Vector clock diff**: Identify potentially missing sequences
2. **Bloom filter check**: Filter out entries we already have
3. **Trust-gated selection**: Cap request size by `TrustResourceLimits.max_pull_bytes`
4. **Backpressure check**: Only send if `peer_state.can_send()` passes

Sends `PullRequest` with:
- **want_ids**: Specific entry hashes to fetch (max determined by trust)
- **max_bytes**: Backpressure hint (responder must honor)
- **nonce**: Echo from Digest for correlation

### 3. PullResponse (Batch Delivery)

Responder looks up requested entries and sends batch:
- **Collect entries** until `total_bytes >= max_bytes`
- **Set truncated flag** if hit limit (requester can retry for remainder)
- **Track metrics**: `bytes_pushed`, `pull_responses_sent`

Requester receives and processes:
- **Store entries** via `store_entry()` (triggers notifications, enforces limits)
- **Update deficit**: `peer_state.record_response(bytes_received)`
- **Track metrics**: `bytes_pulled`, `entries_received`

## Backpressure Management

### Deficit Accounting

Each peer has a `deficit_bytes` counter (token bucket style):

```rust
// Sending data (creates "debt")
peer_state.debit_bytes(1000);  // deficit = -1000

// Receiving data (double credit rewards progress)
peer_state.credit_bytes(500);  // deficit = -1000 + 1000 = 0
```

**Backpressure threshold**: If `deficit < -10000`, pause pulls until recovery.

**Why double credit?** Encourages progress. Small successful syncs offset earlier failures, preventing starvation.

### Exponential Backoff

Retry timing adapts to trust class:

| TrustClass | Initial Backoff | Max Backoff | Outstanding Requests |
|------------|-----------------|-------------|----------------------|
| Isolated   | 1500ms          | 5000ms      | 1                    |
| Known      | 800ms           | 2500ms      | 2                    |
| Partner    | 300ms           | 1200ms      | 3                    |
| Federated  | 300ms           | 1200ms      | 3                    |

Backoff **doubles** on each retry (300ms → 600ms → 1200ms), **resets** on success.

### Trust-Gated Limits

`can_send()` enforces three conditions:
1. **Deficit OK**: `deficit > -10000` (not backpressured)
2. **Request limit**: `outstanding < max_outstanding_reqs`
3. **Backoff elapsed**: `now - last_retry >= current_backoff`

Lower trust peers get stricter limits to prevent abuse.

## Adaptive Bloom Sizing

Bloom filters size adaptively based on topic cardinality:

```rust
// Heuristic: m = min(65536, next_pow2(8 * expected_ids))
let size = (8 * expected_ids).next_power_of_two().min(65536);
```

**Rationale**:
- 8x overprovisioning → ~10-15% false positive rate
- 8KB cap prevents unbounded growth
- Power-of-2 sizing aligns with allocators

**Trade-off**: Higher FP rate for large topics (10k+ entries) but bounded overhead. Since digests are hints, false positives only waste bandwidth, not correctness.

## Metrics & Observability

### Key Metrics

```promql
# Backpressure events
rate(icn_gossip_pull_truncated_total[5m])

# Per-peer deficit (negative = backpressured)
icn_gossip_peer_deficit_bytes < -10000

# Pull bandwidth
rate(icn_gossip_bytes_pulled_total[5m])
rate(icn_gossip_bytes_pushed_total[5m])

# Bloom false positive rate
histogram_quantile(0.95, icn_gossip_bloom_fp_rate)
```

### Alerting

- **`pull_truncated_total` spike** → Network congestion or backpressure
- **`peer_deficit_bytes` persistently negative** → Peer offline or misbehaving
- **`bloom_fp_rate > 0.3`** → Adaptive sizing may need tuning

## Security Considerations

### Addressed

1. **Resource limits**: All requests respect `TrustResourceLimits.max_pull_bytes`
2. **Bounded growth**: Bloom filters capped at 8KB
3. **Backpressure**: Deficit tracking prevents bandwidth monopolization
4. **Retry limits**: Exponential backoff prevents retry storms

### Outstanding

1. **Digest spam**: Rate limit digest processing (max 1 per 5s per peer)
2. **Bloom manipulation**: Validate `hint_count` vs bloom cardinality estimate
3. **Nonce collision**: Track recent nonces, reject duplicates
4. **Trust downgrade**: Need mechanism to lower trust for misbehaving peers

## Performance

### Latency

- **Best case (LAN)**: 3 RTT (~30ms) for full sync
- **Worst case (WAN, backpressured)**: 1-5 seconds with retries
- **Target convergence**: <10s LAN, <60s WAN

### Bandwidth

- **Digest overhead**: ~9 KB per digest (8KB bloom + 1KB vector clock)
- **Digest cadence**: 1 per 10s = ~0.9 KB/s per peer per topic
- **Large deployment (100 peers, 10 topics)**: ~900 KB/s digest overhead

**Optimization ideas** (future):
- Compress blooms with zstd
- Send vector clock deltas instead of full snapshots
- Adaptive cadence (slow down if no new entries)

## Implementation Status

**✅ Complete**:
- Message types (Digest, PullRequest, PullResponse)
- PeerSyncState with deficit tracking
- Exponential backoff with trust-aware timing
- Adaptive Bloom filter sizing
- Full Digest handler with bloom intersection and backpressure
- PullRequest/PullResponse handlers with peer state updates
- Comprehensive metrics
- 47 passing unit tests

**🚧 In Progress**:
- Periodic digest emission
- Integration tests (convergence, backpressure, FP rate)
- Network layer wiring

## Code Locations

- **Message types**: `icn-gossip/src/types.rs:120-144`
- **Trust limits**: `icn-gossip/src/types.rs:234-287`
- **PeerSyncState**: `icn-gossip/src/sync.rs:77-155`
- **Backoff**: `icn-gossip/src/sync.rs:12-63`
- **Adaptive bloom**: `icn-gossip/src/bloom.rs:40-70`
- **Handlers**: `icn-gossip/src/gossip.rs:505-591`
- **Metrics**: `icn-obs/src/metrics.rs:98-141, 225-268`

## References

- Dev journal: `docs/dev-journal/2025-01-11-phase-7-gossip-pull-protocol.md`
- Architecture: `docs/ARCHITECTURE.md`
- Tests: `icn-gossip/src/sync.rs:233-327`
