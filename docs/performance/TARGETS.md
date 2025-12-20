# ICN Performance Targets

This document defines performance targets and SLOs for ICN components.

## Service Level Objectives (SLOs)

### Transaction Processing

| Metric | P50 Target | P95 Target | P99 Target |
|--------|------------|------------|------------|
| Transaction signing | 10 µs | 20 µs | 50 µs |
| Transaction verification | 20 µs | 50 µs | 100 µs |
| Ledger append (with signature) | 50 µs | 100 µs | 200 µs |
| Balance query | 1 µs | 10 µs | 100 µs |

### Trust Graph

| Metric | P50 Target | P95 Target | P99 Target |
|--------|------------|------------|------------|
| Trust score computation | 50 ns | 1 µs | 10 µs |
| Trust edge add | 50 µs | 150 µs | 500 µs |
| Trust edge remove | 50 µs | 150 µs | 500 µs |
| Transitive trust (3 hops) | 100 ns | 1 µs | 10 µs |

### Gossip Protocol

| Metric | P50 Target | P95 Target | P99 Target |
|--------|------------|------------|------------|
| Message serialization | 500 ns | 2 µs | 10 µs |
| Message deserialization | 1 µs | 5 µs | 20 µs |
| Vector clock merge | 50 ns | 200 ns | 1 µs |
| Compression (1 KB) | 5 µs | 15 µs | 50 µs |

### Network

| Metric | Target | Notes |
|--------|--------|-------|
| Gossip propagation (10 nodes) | < 500 ms | Full network convergence |
| Gossip propagation (100 nodes) | < 2 sec | Full network convergence |
| Connection establishment | < 100 ms | QUIC handshake |
| Message round-trip | < 50 ms | Same datacenter |

## Throughput Targets

### Single Node

| Operation | Target (ops/sec) | Notes |
|-----------|------------------|-------|
| Transaction verification | 40,000 | Single core |
| Trust computations | 10,000,000 | Cached |
| Gossip messages | 100,000 | Serialization only |
| Balance queries | 1,000,000 | In-memory |

### Multi-Core (8 cores)

| Operation | Target (ops/sec) | Notes |
|-----------|------------------|-------|
| Transaction verification | 150,000 | With parallel verification |
| Mixed workload | 50,000 | Realistic production load |

## Resource Limits

### Memory

| Component | Limit | Notes |
|-----------|-------|-------|
| Base process | 256 MB | Without data |
| Per 1000 trust edges | 10 MB | Graph storage |
| Per 10,000 ledger entries | 20 MB | Journal + indices |
| Trust cache | 64 MB | LRU cache |

### Disk I/O

| Operation | Target | Notes |
|-----------|--------|-------|
| Ledger write | < 1 ms | fsync included |
| Trust edge update | < 200 µs | Batched writes |
| Checkpoint | < 5 sec | Full state snapshot |

### Network Bandwidth

| Direction | Limit | Notes |
|-----------|-------|-------|
| Inbound gossip | 10 MB/sec | Per peer |
| Outbound gossip | 10 MB/sec | Per peer |
| Total bandwidth | 100 MB/sec | All peers |

## Regression Thresholds

### CI Enforcement

| Threshold | Action |
|-----------|--------|
| < 5% | Pass (within noise) |
| 5-10% | Pass with warning |
| 10-25% | Requires review |
| > 25% | CI failure |

### Alerting (Production)

| Metric | Warning | Critical |
|--------|---------|----------|
| P95 latency | > 2x target | > 5x target |
| Error rate | > 0.1% | > 1% |
| Memory usage | > 80% limit | > 95% limit |
| CPU usage | > 70% | > 90% |

## Scalability Targets

### Network Size

| Nodes | Gossip Convergence | Trust Computation |
|-------|-------------------|-------------------|
| 10 | < 500 ms | < 100 ns |
| 100 | < 2 sec | < 100 ns |
| 1,000 | < 10 sec | < 1 µs |
| 10,000 | < 60 sec | < 10 µs |

### Data Volume

| Metric | Current Capacity | Target |
|--------|------------------|--------|
| Ledger entries | 1M | 10M |
| Trust edges | 100K | 1M |
| Active peers | 100 | 1,000 |

## Optimization Priorities

### P0 (Critical Path)

1. Ed25519 verification - dominates transaction throughput
2. Trust score computation - affects connection acceptance
3. Gossip serialization - affects message propagation

### P1 (Important)

1. Ledger balance computation - affects API latency
2. Trust edge updates - affects trust graph consistency
3. Message compression - affects bandwidth usage

### P2 (Nice to Have)

1. Checkpoint creation - affects recovery time
2. Cache warming - affects cold start
3. Batch verification - future optimization

## Monitoring

### Key Metrics to Track

```prometheus
# Transaction latency histogram
icn_transaction_duration_seconds_bucket{operation="verify"}

# Trust computation latency
icn_trust_computation_duration_seconds_bucket

# Gossip propagation time
icn_gossip_propagation_seconds_bucket

# Memory usage
icn_process_memory_bytes

# Active connections
icn_network_active_connections
```

### Dashboards

- [Grafana: ICN Performance](https://grafana.local/d/icn-perf)
- [Prometheus: ICN Metrics](https://prometheus.local/targets)

## Review Schedule

- **Weekly**: Review P95 latency trends
- **Monthly**: Full benchmark run comparison
- **Quarterly**: Update targets based on requirements

## History

| Date | Change | Impact |
|------|--------|--------|
| 2025-12-16 | Initial baseline | Established targets |
| 2025-12-20 | Added SLOs | Formalized requirements |
