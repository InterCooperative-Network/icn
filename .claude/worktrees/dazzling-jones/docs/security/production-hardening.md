# Production Hardening - Security & Stability

This document details the production hardening measures implemented in ICN to protect against DoS attacks, resource exhaustion, and operational edge cases.

## Table of Contents

1. [Overview](#overview)
2. [Critical Security Fixes](#critical-security-fixes)
3. [High Priority Fixes](#high-priority-fixes)
4. [Configuration](#configuration)
5. [Monitoring](#monitoring)
6. [Remaining Work](#remaining-work)

---

## Overview

ICN's production hardening focuses on three primary threat vectors:

1. **Network-level attacks**: DoS via malicious peers exploiting QUIC/gossip protocols
2. **Resource exhaustion**: Memory/CPU attacks via unbounded allocations or streams
3. **Operational failures**: Edge cases like clock skew, malformed data, blocking operations

Historical claim from the Phase 7 cycle: critical and high-priority issues in that cycle were marked resolved at the time.

---

## Critical Security Fixes

### 1. Unbounded Message Allocation DoS

**Severity**: Critical
**File**: [`icn-net/src/protocol.rs:143-167`](../../icn/crates/icn-net/src/protocol.rs#L143-L167)

**Vulnerability**: Malicious peer could send a network message with an extremely large length prefix, causing the victim to allocate gigabytes of memory before validating content.

**Fix**:
```rust
// Read 4-byte length prefix
let len_u32 = u32::from_be_bytes(len_buf);

// Validate BEFORE allocation
if len_u32 == 0 {
    bail!("Invalid message: zero length");
}
if len_u32 > MAX_MESSAGE_SIZE as u32 {
    bail!("Message too large: {} bytes (max {})", len_u32, MAX_MESSAGE_SIZE);
}

// Safe to allocate after validation
let len = len_u32 as usize;
let mut buf = vec![0u8; len];
```

**Additional protections**:
- Prevents u32→usize overflow on 32-bit systems
- Rejects zero-length messages (invalid protocol state)
- Maximum message size: 10MB (`MAX_MESSAGE_SIZE`)

---

### 2. Blocking Operations in Async Context

**Severity**: Critical
**File**: [`icn-core/src/supervisor/mod.rs`](../../icn/crates/icn-core/src/supervisor/mod.rs)

**Vulnerability**: The incoming message handler used `blocking_write()` to acquire RwLock on shared state (GossipActor). Under high message load, this blocks Tokio worker threads, causing thread starvation and degraded performance across the entire runtime.

**Fix**: Replaced blocking operations with async task spawning:
```rust
// Before (blocking - BAD):
let mut gossip = gossip_handle.blocking_write();
gossip.handle_message(gossip_msg)?;

// After (async - GOOD):
tokio::spawn(async move {
    let mut gossip = gossip_handle.write().await;
    if let Err(e) = gossip.handle_message(gossip_msg) {
        warn!("Failed to handle message: {}", e);
    }
});
```

**Impact**:
- Applied to all message handlers: Gossip, Subscribe, Unsubscribe
- Prevents thread pool exhaustion under high load
- Maintains async/await best practices throughout

---

### 3. TLS Certificate Verification Disabled

**Severity**: Critical
**File**: [`icn-net/src/tls.rs:81-195`](../../icn/crates/icn-net/src/tls.rs#L81-L195)

**Vulnerability**: Custom `DidCertificateVerifier` accepted all certificates without validation, enabling trivial MITM attacks and peer impersonation.

**Fix**: Implemented comprehensive certificate validation:

```rust
impl DidCertificateVerifier {
    fn extract_did_from_cert(cert: &CertificateDer) -> Result<String, rustls::Error> {
        // Parse X.509 certificate using x509-parser
        let (_, parsed_cert) = X509Certificate::from_der(cert)?;

        // Extract DID from Subject Alternative Name (SAN)
        if let Ok(Some(san_ext)) = parsed_cert.subject_alternative_name() {
            for name in &san_ext.value.general_names {
                if let GeneralName::DNSName(dns) = name {
                    if dns.starts_with("did:icn:") {
                        return Ok(dns.to_string());
                    }
                }
            }
        }

        Err(rustls::Error::General("No DID found in certificate SAN"))
    }

    fn check_expiration(cert: &CertificateDer, now: UnixTime) -> Result<(), rustls::Error> {
        let (_, parsed_cert) = X509Certificate::from_der(cert)?;
        let current_time = UNIX_EPOCH + Duration::from_secs(now.as_secs());
        let not_before = parsed_cert.validity().not_before.to_datetime();
        let not_after = parsed_cert.validity().not_after.to_datetime();

        if current_time < not_before {
            return Err(rustls::Error::General("Certificate not yet valid"));
        }
        if current_time > not_after {
            return Err(rustls::Error::General("Certificate expired"));
        }

        Ok(())
    }
}
```

**Validation steps**:
1. Parse X.509 certificate structure
2. Extract DID from Subject Alternative Name
3. Validate DID format (`did:icn:*`)
4. Check certificate validity period (not before/after)
5. Log verification for security audit trail

**Current limitations**:
- ⚠️ Does NOT yet integrate with trust graph (TODO)
- Accepts all valid DID certificates regardless of trust score
- Self-signed certificates accepted (required for P2P architecture)

**Dependencies added**: `x509-parser = "0.16"`

---

### 8. Social Recovery Ledger Transfer Bug

**Severity**: Critical
**File**: [`icn-ledger/src/ledger.rs:469-484`](../../icn/crates/icn-ledger/src/ledger.rs#L469-L484)
**Status**: Fixed (2025-11-17)

**Vulnerability**: The `transfer_balances_for_recovery()` function had inverted debit/credit logic, causing ALL social recovery operations to incorrectly transfer balances.

**Broken behavior**:
```rust
// BEFORE (INCORRECT):
let entry = if *balance > 0 {
    JournalEntryBuilder::new(new_did.clone())
        .debit(old_did.clone(), currency.clone(), *balance)   // WRONG: increases old_did!
        .credit(new_did.clone(), currency.clone(), *balance)  // WRONG: decreases new_did!
        .build()?
}
```

**Impact**:
- **Old DID**: Balance would double instead of zeroing (100 → 200)
- **New DID**: Would receive negative balance instead of positive (-100 instead of +100)
- **Accounting**: Every recovery would create permanent accounting errors
- **User Experience**: Catastrophic - users would lose their balances during recovery

**Example failure scenario**:
1. Alice has +100 hours balance, loses device
2. Creates new identity, initiates social recovery
3. Trustees attest, recovery finalizes
4. **Expected**: Old DID: 100→0, New DID: 0→100
5. **Actual (broken)**: Old DID: 100→200, New DID: 0→-100
6. Alice now has -100 hours (debt!) instead of her 100 hours credit

**Fix**:
```rust
// AFTER (CORRECT):
let entry = if *balance > 0 {
    // old_did has positive balance (+100 means they have credit)
    // Transfer it to new_did: reduce old_did's balance, increase new_did's balance
    JournalEntryBuilder::new(new_did.clone())
        .credit(old_did.clone(), currency.clone(), *balance)  // Reduce old_did's balance ✅
        .debit(new_did.clone(), currency.clone(), *balance)   // Increase new_did's balance ✅
        .build()?
}
```

**Root cause**: Confusion between mutual credit semantics and traditional accounting:
- In mutual credit: `debit` increases assets (receiving credit)
- The transfer logic incorrectly debited the old account (giving it more credit)

**Discovery**: Found during integration test debugging for `test_full_recovery_flow`

**Test coverage**: Integration test now validates:
- Old DID balance reduces to 0 after transfer
- New DID receives full balance (100 hours)
- Trust graph edges correctly migrate (2 edges)

**Additional context**:
- This bug was introduced when social recovery was first implemented (Phase 11)
- Would have affected EVERY social recovery operation in production
- Highlights importance of end-to-end integration tests for financial operations
- Related to Phase 7 ledger semantics fix (which this test initially failed against)

---

## High Priority Fixes

### 4. Integer Overflow in Timestamp Conversion

**Severity**: High
**Files**:
- [`icn-ledger/src/entry.rs:68-73`](../../icn/crates/icn-ledger/src/entry.rs#L68-L73)
- [`icn-gossip/src/gossip.rs:127-131`](../../icn/crates/icn-gossip/src/gossip.rs#L127-L131)

**Vulnerability**: Unchecked cast from `u128` (Duration::as_millis) to `u64` causes silent wraparound if system clock is set far in the future (post year 2262).

**Fix**:
```rust
// Before (unsafe):
let timestamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)?
    .as_millis() as u64;

// After (safe):
let timestamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)?
    .as_millis()
    .try_into()
    .context("Timestamp overflow - system clock too far in future")?;
```

**Impact**: Prevents silent data corruption in ledger entries and gossip messages.

---

### 5. Bloom Filter Index Out of Bounds

**Severity**: High
**File**: [`icn-gossip/src/bloom.rs:103-149`](../../icn/crates/icn-gossip/src/bloom.rs#L103-L149)

**Vulnerability**: `BloomFilter::from_data()` didn't validate that claimed size matched actual data, allowing malicious peer to trigger index panic via crafted `BloomFilterData`.

**Fix**: Added validation before truncation:
```rust
pub fn from_data(data: &BloomFilterData) -> Self {
    // Validate non-zero size
    if data.size == 0 {
        tracing::warn!("BloomFilter: zero size, creating minimal filter");
        return BloomFilter { bits: vec![false], num_hashes: 1, size: 1 };
    }

    let mut bits = Vec::new();
    // Unpack bytes into bits...

    let unpacked_bits = bits.len();
    let claimed_size = data.size as usize;

    if claimed_size > unpacked_bits {
        // Malformed: claimed > actual
        tracing::warn!(
            "BloomFilter: claimed size {} exceeds actual {}",
            claimed_size, unpacked_bits
        );
        return BloomFilter {
            bits,
            num_hashes: data.num_hashes,
            size: unpacked_bits as u64, // Use actual size
        };
    }

    // Normal case: trim to claimed size
    bits.truncate(claimed_size);
    BloomFilter { bits, num_hashes: data.num_hashes, size: data.size }
}
```

**Protections**:
- Zero-size filter detection (prevents division by zero)
- Size mismatch handling (prevents index panic in insert/contains)
- Logging for security auditing

---

### 6. Network Message Rate Limiting

**Severity**: High
**Files**:
- [`icn-net/src/rate_limit.rs`](../../icn/crates/icn-net/src/rate_limit.rs) (new module)
- [`icn-net/src/actor/mod.rs`](../../icn/crates/icn-net/src/actor/mod.rs)

**Vulnerability**: No rate limiting allowed malicious peer to flood victim with messages, exhausting CPU and memory.

**Solution**: Implemented token bucket rate limiter with per-peer tracking.

**Algorithm**: Token Bucket
- Each peer has a bucket of tokens (burst capacity)
- Tokens refill at configurable rate
- Each message consumes 1 token
- Messages are dropped (not queued) when bucket empty

**Configuration** ([`RateLimitConfig`](../../icn/crates/icn-net/src/rate_limit.rs)):
```rust
pub struct RateLimitConfig {
    pub max_messages_per_second: u32,  // Default: 100
    pub burst_capacity: u32,            // Default: 20
    pub refill_interval: Duration,      // Default: 100ms
}
```

**Integration**:
```rust
async fn handle_connection(
    connection: quinn::Connection,
    handler: IncomingMessageHandler,
    rate_limiter: Arc<RateLimiter>,
) -> Result<()> {
    loop {
        match connection.accept_bi().await {
            Ok((mut send, mut recv)) => {
                match read_message(&mut recv).await {
                    Ok(message) => {
                        // Check rate limit BEFORE processing
                        if !rate_limiter.check_rate_limit(&message.from).await {
                            warn!("Rate limited message from {}", message.from);
                            icn_obs::metrics::network::messages_rate_limited_inc();
                            continue; // Drop message
                        }

                        // Process message normally
                        handler(message);
                    }
                    Err(e) => warn!("Failed to read message: {}", e),
                }
            }
            Err(e) => break,
        }
    }
}
```

**Metrics**: `icn_network_messages_rate_limited_total` (counter)

**Memory management**: Periodic cleanup of inactive peer buckets via `cleanup_old_buckets()`.

---

### 7. Bounded QUIC Stream Limits

**Severity**: High
**File**: [`icn-net/src/session.rs:20-44`](../../icn/crates/icn-net/src/session.rs#L20-L44)

**Vulnerability**: Default QUIC configuration allowed 100+ concurrent streams per connection, enabling resource exhaustion via stream flooding.

**Fix**: Created conservative transport configuration:
```rust
fn create_transport_config() -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();

    // Limit concurrent streams
    config.max_concurrent_bidi_streams(10u32.into());  // Was 100
    config.max_concurrent_uni_streams(0u32.into());    // Not used

    // Idle timeout and keep-alive
    config.max_idle_timeout(Some(Duration::from_secs(60).try_into().unwrap()));
    config.keep_alive_interval(Some(Duration::from_secs(30)));

    // Stream data windows
    config.stream_receive_window((1024u32 * 1024u32).into());  // 1MB per stream
    config.receive_window((10u32 * 1024u32 * 1024u32).into()); // 10MB per connection

    config
}
```

**Rationale**:
- **10 bidirectional streams**: Sufficient for gossip protocol (typically 1-3 concurrent operations)
- **0 unidirectional streams**: Not used by ICN protocol
- **60s idle timeout**: Detects and closes stale connections
- **30s keep-alive**: Proactive detection of network failures
- **1MB per stream**: Large enough for gossip messages (max 10MB), prevents memory exhaustion
- **10MB per connection**: Total receive window caps memory usage per peer

**Applied to**: Both server and client QUIC configurations.

---

### 8. Ledger Recovery Transfer Bug (BUG #30)

**Severity**: Critical
**File**: [`icn-ledger/src/ledger.rs:469-484`](../../icn/crates/icn-ledger/src/ledger.rs#L469-L484)

**Vulnerability**: The `transfer_balances_for_recovery()` function had inverted debit/credit operations, causing balance transfers during social recovery to **double the old DID's balance** instead of transferring it to the new DID. This would have allowed users to create unlimited credit by repeatedly initiating fake recoveries.

**Before (BROKEN)**:
```rust
let entry = if *balance > 0 {
    // WRONG: This increases old_did's balance instead of reducing it
    JournalEntryBuilder::new(new_did.clone())
        .debit(old_did.clone(), currency.clone(), *balance)  // Increases old_did balance
        .credit(new_did.clone(), currency.clone(), *balance) // Decreases new_did balance
        .build()?
} else {
    // Negative balance (debt) transfer also broken
    JournalEntryBuilder::new(new_did.clone())
        .credit(old_did.clone(), currency.clone(), balance.abs())
        .debit(new_did.clone(), currency.clone(), balance.abs())
        .build()?
};
```

**After (FIXED)**:
```rust
let entry = if *balance > 0 {
    // Correct: Transfer positive balance from old_did to new_did
    JournalEntryBuilder::new(new_did.clone())
        .credit(old_did.clone(), currency.clone(), *balance)  // Reduce old_did's balance
        .debit(new_did.clone(), currency.clone(), *balance)   // Increase new_did's balance
        .build()?
} else {
    // Correct: Transfer debt from old_did to new_did
    JournalEntryBuilder::new(new_did.clone())
        .debit(old_did.clone(), currency.clone(), balance.abs())  // Remove debt from old_did
        .credit(new_did.clone(), currency.clone(), balance.abs()) // Add debt to new_did
        .build()?
};
```

**Impact**:
- **Production-blocking severity**: Would have allowed unlimited credit creation via fake recovery
- **Attack vector**: User creates recovery event → finalizes it → old DID balance doubles instead of transfers
- **Repeat exploit**: Could be repeated indefinitely to create arbitrary amounts of credit
- **Economic impact**: Complete breakdown of mutual credit system integrity
- **Discovery**: Found during social recovery integration test development (2025-11-17)
- **Status**: Fixed before feature deployment - no production data affected

**Mutual Credit Semantics Reminder**:
In double-entry mutual credit accounting:
- **Debit** increases an asset account (receiving credit from someone)
- **Credit** increases a liability account (giving credit to someone)
- Positive balance = net creditor (others owe you)
- Negative balance = net debtor (you owe others)

**Test Coverage**: Integration test `test_full_recovery_lifecycle()` now validates correct balance transfers:
```rust
// Old DID starts with 100 hours credit
assert_eq!(old_balance, Some(100));

// After recovery finalization
assert_eq!(alice.ledger.read().await.balance(&alice_did, "hours"), Some(0));      // Old DID: 100 → 0
assert_eq!(alice.ledger.read().await.balance(&alice2_did, "hours"), Some(100));   // New DID: 0 → 100
```

---

## Configuration

### Default Security Settings

All production hardening features are enabled by default with conservative limits:

| Feature | Default Value | Tunable |
|---------|--------------|---------|
| Max message size | 10 MB | Yes (via `MAX_MESSAGE_SIZE`) |
| Rate limit (msg/sec) | 100 | Yes (via `RateLimitConfig`) |
| Burst capacity | 20 messages | Yes (via `RateLimitConfig`) |
| QUIC concurrent streams | 10 | Yes (via `TransportConfig`) |
| QUIC stream window | 1 MB | Yes (via `TransportConfig`) |
| QUIC connection window | 10 MB | Yes (via `TransportConfig`) |
| Connection idle timeout | 60 seconds | Yes (via `TransportConfig`) |
| Keep-alive interval | 30 seconds | Yes (via `TransportConfig`) |

### Customizing Rate Limits

To adjust rate limiting (e.g., for high-throughput scenarios):

```rust
use icn_net::{RateLimitConfig, RateLimiter};
use std::time::Duration;

let config = RateLimitConfig {
    max_messages_per_second: 200,  // Higher throughput
    burst_capacity: 50,             // Larger bursts
    refill_interval: Duration::from_millis(100),
};

let rate_limiter = Arc::new(RateLimiter::new(config));
```

**Note**: Current implementation requires modifying `NetworkActor::spawn()` to accept custom config. This is a future enhancement opportunity.

### Witness Signatures for Material Transactions

Witness signatures provide Byzantine fault tolerance by requiring multiple parties to co-sign material transactions. This prevents double-spending and provides additional security for high-value transfers.

**Configuration** (`[ledger.witness]` in TOML):

```toml
[ledger.witness]
# Witness policy: "none", "counterparty", "quorum", "all_parties"
# Default is "none". Example shows recommended production config:
default_policy = "counterparty"

# Only require witnesses for transactions above this value
threshold = 1000

# Timeout for collecting signatures (seconds)
collection_timeout_secs = 300

# For quorum policy only:
# quorum_required = 2
# quorum_witnesses = ["did:icn:abc123", "did:icn:def456", "did:icn:ghi789"]
```

**Policies**:
| Policy | Description |
|--------|-------------|
| `none` | No witness signatures required (default) |
| `counterparty` | The other party in the transaction must co-sign |
| `quorum` | N-of-M designated witnesses must sign |
| `all_parties` | All transaction participants must sign |

**Metrics**:
- `icn_ledger_witnessed_entries_accepted_total`: Witnessed entries successfully processed
- `icn_ledger_witnessed_entries_rejected_total{reason}`: Rejected entries by reason (invalid_signature, insufficient_signatures)
- `icn_ledger_witness_signature_count`: Histogram of signatures per witnessed entry

**Use Cases**:
1. **High-value transfers**: Set `threshold = 10000` with `counterparty` policy
2. **Multi-sig treasury**: Use `quorum` policy with 2-of-3 trusted witnesses
3. **Full consensus**: Use `all_parties` for unanimous agreement requirements

---

## Monitoring

### Security Metrics

The following Prometheus metrics track security-related events:

**Rate Limiting**:
- `icn_network_messages_rate_limited_total` (counter): Messages dropped due to rate limiting

**Network Health**:
- `icn_network_connections_total` (counter): Total connection attempts
- `icn_network_connections_active` (gauge): Currently active connections
- `icn_network_messages_received_total` (counter): Successfully processed messages

**Gossip Protocol**:
- `icn_gossip_entries_total` (gauge): Total gossip entries stored
- `icn_gossip_announces_received_total` (counter): Announce messages received
- `icn_gossip_requests_received_total` (counter): Request messages received

### Alerting Recommendations

Consider setting up alerts for:

1. **High rate limiting**: `rate(icn_network_messages_rate_limited_total[5m]) > 10`
   - Indicates potential DoS attack or misbehaving peer

2. **Connection churn**: `rate(icn_network_connections_total[5m]) > 100`
   - May indicate connection exhaustion attack

3. **Low message throughput**: `rate(icn_network_messages_received_total[5m]) < 1`
   - Could indicate network partition or isolation

### Log Monitoring

Security-relevant log patterns:

```bash
# Rate limiting events
grep "Rate limited message from" /var/log/icnd.log

# Certificate verification warnings
grep "SECURITY: Trust graph verification not yet implemented" /var/log/icnd.log

# Bloom filter validation warnings
grep "BloomFilter deserialization" /var/log/icnd.log

# Message validation errors
grep "Message too large\|Invalid message" /var/log/icnd.log
```

### Scope-Aware Capacity (Epic 2)

**Metrics** (added in PR #962):
- `icn_compute_task_scope_map_size` (gauge): Number of active task→scope mappings. Tracks memory overhead of scope queue tracking. Alert if >500 (warning) or >2000 (critical, possible leak).

**Alert rules**: See `deploy/prometheus/scope-capacity-alerts.yml` for the rule set used by this hardening pass.

**Operational notes**:

1. **Timeout decrement lag**: The timeout checker (`check_timeouts`) runs synchronously in the main command loop, so scope queue decrements for timed-out tasks may lag by the timeout check interval. This is acceptable because the demand adjustment loop runs every 60s — any lag within that window has no effect on budget rebalancing. If a burst of timeouts occurs, queue depths self-correct on the next check cycle.

2. **CellService not configured**: If `ComputeActor` is started without a `CellService`, a WARN log is emitted and all submitters are treated as `Commons` scope. This is the expected state during bootstrap or for nodes not yet enrolled in a cell.

3. **Demand adjustment cold start**: The demand loop requires `min_samples` (default: 5) total queued tasks before making any adjustments. On lightly loaded nodes, the default `CapacityBudget` applies indefinitely. This is by design — adjustment noise on sparse data would degrade allocation quality.

4. **Memory scaling**: At 100K concurrent tasks, `task_scope_map` consumes ~5MB. Monitor via `icn_compute_task_scope_map_size` gauge. Set alerts at 50K+ entries for production.

---

## Remaining Work

### High Priority (Not Yet Implemented)

The following issues were identified but not yet addressed:

#### Medium Priority (5 issues):
1. **No request timeouts in session management**
   - Impact: Hung requests can accumulate
   - Recommendation: Add timeout to dial/send operations

2. **Panic on invalid DID parsing**
   - File: Trust graph DID parsing
   - Impact: Malformed DID crashes process
   - Recommendation: Replace unwrap() with Result handling

3. **Unbounded vector growth in gossip subscriptions**
   - Impact: Memory exhaustion with many topics
   - Recommendation: Add max topics per node limit

4. **No compression for large gossip messages**
   - Impact: Bandwidth waste, slower sync
   - Recommendation: Add zstd compression for messages >1KB

5. **Missing input sanitization in contract interpreter**
   - Impact: Potential for crafted contracts to cause issues
   - Recommendation: Add stricter AST validation

#### Low Priority (3 issues):
1. **Inconsistent error handling patterns**
   - Some modules use panic, others use Result
   - Recommendation: Standardize on Result<T, E>

2. **Missing trace logs for debugging**
   - Hard to diagnose issues in production
   - Recommendation: Add trace! logs at key decision points

3. **TODO comments in non-critical paths**
   - Minor TODOs in test utilities and helper functions
   - Recommendation: Track as GitHub issues

### Trust-Gated TLS Verification (✓ Implemented - Phase 8B)

**Status**: COMPLETE (2025-01-12)
**File**: [`icn-net/src/tls.rs`](../../icn/crates/icn-net/src/tls.rs)

The TLS certificate verifier now integrates with the trust graph to enforce trust-based access control:

```rust
// Extract DID from certificate
let did_str = Self::extract_did_from_cert(end_entity)?;
let peer_did = Did::from_str(&did_str)?;

// Query trust graph for peer's trust score
let trust_score = {
    let graph = self.trust_graph.blocking_read();
    graph.compute_trust_score(&peer_did).unwrap_or(0.0)
};

// Enforce trust threshold
if trust_score < self.min_trust_threshold {
    warn!("🔒 Connection rejected: DID {} has insufficient trust", did_str);
    icn_obs::metrics::network::connections_rejected_untrusted_inc(&did_str, trust_score);
    return Err(rustls::Error::General(format!(
        "Peer DID {} has insufficient trust score {:.3} (required: {:.3})",
        did_str, trust_score, self.min_trust_threshold
    )));
}
```

**Security Benefits**:
- Prevents Sybil attacks from unknown/untrusted peers
- Configurable trust thresholds (default: 0.0 = development mode)
- Production recommendation: 0.1 (reject isolated peers) or 0.4 (partners only)
- Per-peer and per-trust-class rejection metrics
- Full Ed25519 signature verification on TLS 1.3 handshakes

**Configuration**:
```rust
TrustGatedRateLimitConfig {
    min_trust_threshold: 0.1,  // Reject isolated peers (score < 0.1)
    // ... rate limit settings
}
```

**Tests**: 3 comprehensive integration tests in `icn-net/tests/trust_gated_tls_integration.rs`
- Trusted peer connection acceptance
- Untrusted peer connection rejection
- Trust threshold boundary conditions

---

## Testing

All production hardening changes include comprehensive tests:

- **Rate limiter**: 4 unit tests (token consumption, refills, per-peer isolation, cleanup)
- **Bloom filter validation**: Covered by existing test suite
- **Timestamp overflow**: Implicit coverage (would fail if overflow occurred)
- **Certificate verification**: 3 unit tests (cert generation, server config, client config)

Run the full test suite:
```bash
cargo test -p icn-net -p icn-gossip -p icn-ledger -p icn-obs
```

Historical result (2025-11-17 snapshot): 64 tests passed (27 + 18 + 16 + 0)

---

## References

- [Architecture Documentation](../ARCHITECTURE.md)
- [Topic Subscriptions API](../reference/api/topic-subscriptions-api.md)
- [QUIC Transport RFC 9000](https://www.rfc-editor.org/rfc/rfc9000.html)
- [Token Bucket Algorithm](https://en.wikipedia.org/wiki/Token_bucket)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)

---

## Changelog

- **2025-11-17**: Critical ledger recovery bug fix (BUG #30)
  - Fixed inverted debit/credit in `transfer_balances_for_recovery()`
  - Production-blocking: Would have enabled unlimited credit creation
  - Discovered during social recovery integration test development
  - Fixed before feature deployment - no production impact

- **2025-01-XX**: Initial production hardening (Phase 7)
  - Fixed 3 critical security issues
  - Fixed 4 high-priority stability issues
  - Added comprehensive metrics and logging
  - 64 tests passing across modified crates
