# ICN Replication Operations Guide

**Phase 17: Storage Hardening & Replication**
**Version**: 1.0
**Last Updated**: 2025-11-24

## Overview

ICN's replication system provides fault-tolerant data storage through trust-weighted peer replication. This guide covers configuration, monitoring, and troubleshooting for production deployments.

## Architecture

### Components

1. **ReplicationManager Actor**
   - Background health monitoring (runs every 60 seconds by default)
   - Detects under-replicated content (count < target)
   - Selects trusted peers for replication
   - Sends replica requests via gossip protocol

2. **Gossip Protocol Extensions**
   - `ReplicaRequest`: Request peers to store content
   - `ReplicaOffer`: Peer confirms willingness to replicate
   - `ReplicaStatus`: Batch health status updates

3. **Storage Layer**
   - Replica metadata tracking per content hash
   - Health status: Healthy, Stale, Unreachable
   - Last-seen timestamps for staleness detection

### Data Flow

```
Content Published
    ↓
ReplicationManager detects under-replication
    ↓
Select trusted peers (trust-weighted)
    ↓
Send ReplicaRequest via gossip
    ↓
Peers respond with ReplicaOffer
    ↓
Metadata updated in store
    ↓
Health monitoring continues (60s loop)
```

## Configuration

### Default Configuration

The ReplicationManager uses sensible defaults suitable for most deployments:

```rust
ReplicationConfig {
    target_replicas: 3,                    // Aim for 3 copies
    min_trust_class: TrustClass::Partner,  // Require Partner+ trust (0.4+)
    health_check_interval_secs: 60,        // Check every minute
    stale_threshold_secs: 300,             // 5 minutes = Stale
    unreachable_threshold_secs: 900,       // 15 minutes = Unreachable
}
```

### Custom Configuration

For production environments with specific requirements, configure via environment or code:

#### Environment Variables

```bash
# Increase replica target for critical data
export ICN_REPLICATION_TARGET_REPLICAS=5

# Require higher trust for replicas
export ICN_REPLICATION_MIN_TRUST_CLASS=Federated

# More frequent health checks
export ICN_REPLICATION_HEALTH_CHECK_INTERVAL_SECS=30
```

#### Programmatic Configuration

```rust
use icn_core::ReplicationConfig;
use icn_trust::TrustClass;

let config = ReplicationConfig {
    target_replicas: 5,
    min_trust_class: TrustClass::Federated,
    health_check_interval_secs: 30,
    stale_threshold_secs: 600,              // 10 minutes
    unreachable_threshold_secs: 1800,       // 30 minutes
};

// ReplicationManager spawned with custom config in supervisor
```

## Trust Classes and Selection

### Trust Class Hierarchy

| Trust Class | Score Range | Typical Relationship |
|-------------|-------------|---------------------|
| Isolated    | 0.0 - 0.1   | Unknown, untrusted |
| Known       | 0.1 - 0.4   | Basic interaction |
| **Partner** | **0.4 - 0.7** | **Default minimum for replicas** |
| Federated   | 0.7 - 1.0   | Deep collaboration |

### Selection Algorithm

When ReplicationManager detects under-replication:

1. **Query gossip for known peers** (subscribers + vector clock peers)
2. **Filter by minimum trust class** (default: Partner+)
3. **Exclude existing replicas** (avoid duplicates)
4. **Sort by trust score** (descending - higher trust first)
5. **Select top N candidates** (limit to 5 requests per check)

**Example:**
```
Peers:
- Alice: trust=0.2 (Known) → REJECTED (below Partner threshold)
- Bob: trust=0.5 (Partner) → Selected #3
- Carol: trust=0.6 (Partner) → Selected #2
- Dave: trust=0.8 (Federated) → Selected #1 (highest trust)

Request order: Dave, Carol, Bob (up to 5 total)
```

## Health Monitoring

### Health States

| State | Condition | Action |
|-------|-----------|--------|
| **Healthy** | last_seen < 5 minutes | Normal operation |
| **Stale** | last_seen 5-15 minutes | Warning, may need refresh |
| **Unreachable** | last_seen > 15 minutes | Likely offline, trigger re-replication |

### Monitoring Loop

Every 60 seconds (configurable), ReplicationManager:

1. Loads all content hashes with replica metadata
2. For each content hash:
   - Updates replica health based on last_seen timestamps
   - Counts healthy replicas
   - If count < target_replicas: trigger re-replication
3. Logs health check results

### Request Rate Limiting

To avoid spam, ReplicationManager tracks recent requests:

- **Cooldown period**: 5 minutes minimum between requests for same content
- **Request limit**: 5 peers per health check cycle
- **Exponential backoff**: If no candidates found, wait before retrying

## Prometheus Metrics

### Current Metrics (Week 3)

| Metric | Type | Description |
|--------|------|-------------|
| `icn_replication_health_checks_total` | Counter | Total health checks run |
| `icn_replication_under_replicated_total` | Counter | Content items below target |
| `icn_replication_requests_sent_total` | Counter | Replica requests sent |

### Planned Metrics (Week 4)

| Metric | Type | Description |
|--------|------|-------------|
| `icn_replication_replica_count{content_hash}` | Gauge | Current replica count per hash |
| `icn_replication_health_check_duration_seconds` | Histogram | Health check execution time |
| `icn_replication_offers_received_total` | Counter | Replica offers from peers |
| `icn_replication_failures_total{reason}` | Counter | Failed replication attempts |

### Monitoring Dashboard (Example)

```promql
# Under-replicated content alert
rate(icn_replication_under_replicated_total[5m]) > 0

# Average replica count across all content
avg(icn_replication_replica_count)

# Replication request success rate
rate(icn_replication_offers_received_total[5m])
  / rate(icn_replication_requests_sent_total[5m])
```

## Operational Procedures

### Checking Replication Health

**Via Prometheus:**
```bash
curl http://localhost:9100/metrics | grep icn_replication
```

**Via Logs:**
```bash
# Look for health check summaries
journalctl -u icnd | grep "Replication health check complete"

# Example output:
# Replication health check complete: 127 healthy, 3 under-replicated
```

### Increasing Replica Count

If data loss is a concern, increase target replicas:

**Method 1: Environment Variable (restart required)**
```bash
export ICN_REPLICATION_TARGET_REPLICAS=5
systemctl restart icnd
```

**Method 2: Runtime Configuration (future)**
```bash
# Planned for Phase 18
icnctl replication set-target 5
```

### Handling Under-Replication Alerts

**Diagnosis:**
1. Check peer connectivity: `icnctl network peers`
2. Check trust relationships: `icnctl trust list`
3. Check recent replica requests in logs

**Resolution:**
- **No trusted peers**: Add trust relationships with `icnctl trust attest`
- **Peers offline**: Wait for peers to reconnect, or find new peers
- **Insufficient capacity**: Peers may be at storage limits (Phase 18: quotas)

### Manual Re-Replication (Emergency)

If automatic re-replication fails:

```bash
# Force health check (triggers re-replication immediately)
# Planned for Phase 17 Week 4
icnctl replication health-check

# Query replica status for specific content
icnctl replication status <content_hash>

# Manually request replica from specific peer
icnctl replication request <content_hash> <peer_did>
```

## Troubleshooting

### Symptom: Replication requests never sent

**Possible Causes:**
- No trusted peers at Partner+ level
- All peers already have replicas
- Request cooldown period active (5 minute minimum)

**Diagnosis:**
```bash
# Check trust graph
icnctl trust list

# Check known peers
icnctl network peers

# Check recent requests in logs
journalctl -u icnd | grep "ReplicaRequest"
```

**Solution:**
- Add trust relationships with peers
- Lower min_trust_class (not recommended for production)
- Wait for cooldown period to expire

### Symptom: High under-replication count

**Possible Causes:**
- Network partition (peers offline)
- Storage capacity exhaustion (Phase 18)
- Insufficient trusted peers in network

**Diagnosis:**
```bash
# Check network connectivity
ping <peer_ip>
icnctl network dial <peer_did>

# Check peer trust levels
icnctl trust compute <peer_did>
```

**Solution:**
- Increase target_replicas temporarily to spread load
- Add more high-trust peers to network
- Investigate peer storage capacity (Phase 18: quotas)

### Symptom: Replicas marked as Stale/Unreachable

**Possible Causes:**
- Peer temporarily offline (network issue)
- Peer crashed (process died)
- Clock drift (timestamps incorrect)

**Diagnosis:**
```bash
# Check if peer is responsive
icnctl network ping <peer_did>

# Check peer's last message timestamp
journalctl -u icnd | grep <peer_did>
```

**Solution:**
- Wait for peer to reconnect (automatic health recovery)
- If peer permanently offline, ReplicationManager will request new replicas
- Check system clocks (Phase 19: clock synchronization)

## Best Practices

### Production Deployments

1. **Set target_replicas ≥ 3**
   - Survives single node failure
   - 99.9% durability with proper distribution

2. **Maintain Partner+ trust with multiple peers**
   - Minimum 5 Partner+ peers for redundancy
   - Distribute peers across regions/operators

3. **Monitor under-replication alerts**
   - Alert when under-replication > 5% of content
   - Daily health check summaries

4. **Regular trust graph maintenance**
   - Review trust relationships quarterly
   - Remove trust for inactive peers
   - Add trust for new collaborators

### Development/Testing

1. **Use lower health_check_interval_secs**
   - 10-30 seconds for faster iteration
   - Reset to 60s for production

2. **Use lower target_replicas**
   - 2 replicas sufficient for testing
   - Reduces resource usage in dev environments

3. **Test failure scenarios**
   - Kill node processes (simulate crash)
   - Partition networks (simulate network failure)
   - Verify automatic recovery

## Performance Considerations

### Network Overhead

Each replica request incurs:
- 1 ReplicaRequest message (~100 bytes)
- 1 ReplicaOffer response (~150 bytes)
- Total: ~250 bytes per request

**Typical overhead for 100 content items:**
- 100 items × 2 under-replicated × 3 requests/item = 600 requests
- 600 requests × 250 bytes = 150 KB bandwidth
- Spread over 60 seconds = 2.5 KB/sec (negligible)

### Storage Overhead

Replica metadata per content hash:
- Content hash: 32 bytes
- Per-replica: ~80 bytes (DID + timestamp + health)
- Total: 32 + (80 × replicas) bytes

**Example for 1,000 content items with 3 replicas each:**
- 1,000 × (32 + 80×3) = 272 KB metadata
- Negligible compared to actual content size

### CPU Overhead

Health check cycle (every 60 seconds):
- Load all replica metadata: ~1ms per 100 items
- Trust score lookups: ~10ms per 100 peers
- Total: ~11ms per cycle (0.018% CPU usage)

## Security Considerations

### Trust Requirements

**Why Partner+ trust for replicas?**
- Prevents Sybil attacks (fake nodes offering replicas)
- Ensures data stored with reliable, known peers
- Maintains consistency with trust graph semantics

**Lowering min_trust_class risks:**
- Known (0.1-0.4): Higher risk of unreliable replicas
- Isolated (0.0-0.1): **NOT RECOMMENDED** - no trust relationship

### Replica Verification

**Current (Phase 17):**
- Trust-based replica selection
- Health monitoring via last_seen timestamps

**Future (Phase 18):**
- Cryptographic verification of stored content
- Challenge-response proofs of storage
- Byzantine fault detection for corrupt replicas

## Future Enhancements

### Phase 18: Pre-Pilot Hardening

- **Storage quotas**: Per-peer capacity limits
- **Byzantine detection**: Identify corrupt/malicious replicas
- **Conflict resolution**: Handle replica disagreements

### Phase 19: Post-Pilot Improvements

- **Geo-diverse replication**: Spread replicas across regions
- **Erasure coding**: N-of-M recovery (reduce storage 3x)
- **Priority-based replication**: Critical data replicated first

## Related Documentation

- [ARCHITECTURE.md Section 7.4](../../ARCHITECTURE.md#74-data-durability--replication) - Technical design
- [ROADMAP.md Phase 17](../../development/sessions/undated/ROADMAP.md#phase-17-storage-hardening--replication-4-weeks) - Implementation timeline
- [CHANGELOG.md](../../../CHANGELOG.md) - Release notes

## Support

For operational questions or issues:
- GitHub Issues: https://github.com/InterCooperative-Network/icn/issues
- Development sessions: `docs/development/sessions/` (detailed implementation notes)
- Metrics dashboard: `http://localhost:9100/metrics` (Prometheus)

---

**Document Version**: 1.0 (Phase 17 Week 4)
**Next Review**: After Phase 17 completion and pilot deployment feedback
