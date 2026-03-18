# ICN Service Level Objectives (SLOs)

**Last Updated**: 2026-01-08
**Review Cadence**: Quarterly

## Overview

This document defines Service Level Objectives for ICN daemon deployments. SLOs provide measurable reliability targets that guide operations and incident response.

## SLO Summary

| Service | Metric | Target | Error Budget (30d) |
|---------|--------|--------|-------------------|
| Gateway API | Availability | 99.9% | 43.2 min |
| Gossip Network | Message Delivery | 99.5% | 3.6 hours |
| Ledger Sync | Consistency | 99.9% | 43.2 min |
| API Latency | p99 Response Time | < 500ms | N/A |
| Recovery | RTO | < 15 min | N/A |
| Recovery | RPO | < 1 min | N/A |

---

## Availability SLOs

### Gateway API Availability

**Target**: 99.9% uptime (monthly)

**Definition**: Percentage of time the Gateway API responds to health checks successfully.

**Measurement**:
```promql
# Availability over 30 days
1 - (
  sum(rate(icn_gateway_requests_total{status=~"5.."}[30d])) /
  sum(rate(icn_gateway_requests_total[30d]))
)
```

**Error Budget**: 43.2 minutes per month

**Alert Thresholds**:
| Severity | Condition |
|----------|-----------|
| Warning | < 99.95% (7-day rolling) |
| Critical | < 99.9% (7-day rolling) |
| Page | < 99.5% (1-hour rolling) |

---

### Gossip Message Delivery

**Target**: 99.5% message delivery

**Definition**: Percentage of gossip messages successfully delivered to at least one peer.

**Measurement**:
```promql
sum(rate(icn_gossip_messages_delivered_total[30d])) /
sum(rate(icn_gossip_messages_sent_total[30d]))
```

**Error Budget**: 3.6 hours of message loss per month

**Alert Thresholds**:
| Severity | Condition |
|----------|-----------|
| Warning | Delivery < 99% (1-hour) |
| Critical | Delivery < 95% (15-min) |

---

### Ledger Consistency

**Target**: 99.9% consistency

**Definition**: Percentage of time ledger state matches across connected peers.

**Measurement**:
```promql
# Peers with matching entry counts
sum(icn_ledger_peer_sync_match) / count(icn_ledger_peer_sync_match)
```

**Error Budget**: 43.2 minutes of inconsistency per month

---

## Latency SLOs

### API Response Time

**Target**: p99 < 500ms

**Definition**: 99th percentile response time for Gateway API requests.

**Measurement**:
```promql
histogram_quantile(0.99,
  sum(rate(icn_gateway_request_duration_seconds_bucket[5m])) by (le)
)
```

**Alert Thresholds**:
| Severity | Condition |
|----------|-----------|
| Warning | p99 > 400ms (5-min) |
| Critical | p99 > 500ms (5-min) |
| Page | p99 > 1s (5-min) |

---

### Gossip Propagation

**Target**: p95 < 2 seconds

**Definition**: Time for a message to reach 95% of subscribed peers.

**Measurement**:
```promql
histogram_quantile(0.95,
  sum(rate(icn_gossip_propagation_seconds_bucket[5m])) by (le)
)
```

---

### Trust Computation

**Target**: p99 < 100ms

**Definition**: Time to compute transitive trust score for a DID.

**Measurement**:
```promql
histogram_quantile(0.99,
  sum(rate(icn_trust_compute_duration_seconds_bucket[5m])) by (le)
)
```

---

### Ledger Entry Creation

**Target**: p95 < 1 second

**Definition**: Time from entry submission to local persistence.

**Measurement**:
```promql
histogram_quantile(0.95,
  sum(rate(icn_ledger_entry_duration_seconds_bucket[5m])) by (le)
)
```

---

## Data SLOs

### Ledger Eventual Consistency

**Target**: Convergence within 30 seconds

**Definition**: Maximum time for all peers to reflect a committed entry.

**Measurement**:
```promql
max(icn_ledger_sync_lag_seconds)
```

**Alert Threshold**: Lag > 60 seconds

---

### Trust Score Freshness

**Target**: < 5 minute staleness

**Definition**: Maximum age of cached trust scores.

**Measurement**:
```promql
max(icn_trust_cache_age_seconds)
```

---

### No Data Loss

**Target**: 0 committed entries lost

**Definition**: Once an entry is acknowledged, it must never be lost.

**Measurement**: Compare entry hashes across backups and peers.

**Alert**: Any discrepancy is Critical.

---

## Recovery SLOs

### Recovery Time Objective (RTO)

**Target**: < 15 minutes

**Definition**: Maximum time from failure detection to service restoration.

**Includes**:
- Detection time (< 2 min with alerting)
- Response time (< 5 min for on-call)
- Recovery time (< 8 min for restart/restore)

---

### Recovery Point Objective (RPO)

**Target**: < 1 minute

**Definition**: Maximum data loss in a failure scenario.

**Achieved via**:
- Gossip replication (real-time)
- State snapshots (configurable interval)
- Backups (recommended: hourly)

---

## Incident Severity Mapping

| Severity | SLO Impact | Response |
|----------|------------|----------|
| **P1 Critical** | Multiple SLOs breached | Immediate page, all hands |
| **P2 High** | Single SLO breached | Page on-call |
| **P3 Medium** | Error budget depleted >50% | Respond within 4 hours |
| **P4 Low** | Error budget depleted >25% | Respond within 24 hours |

---

## Error Budget Policy

### When Error Budget is Healthy (> 50%)

- Normal development velocity
- Deploy at will during business hours
- Experiment with new features

### When Error Budget is Low (25-50%)

- Prioritize reliability work
- Require rollback plans for deploys
- No non-critical experiments

### When Error Budget is Exhausted (< 25%)

- Freeze non-critical changes
- Focus exclusively on reliability
- Incident review required before resuming

---

## SLO Dashboard

Grafana dashboard should display:

1. **Current SLO Status** - Green/Yellow/Red for each SLO
2. **Error Budget Remaining** - Percentage and time remaining
3. **Burn Rate** - How fast error budget is depleting
4. **SLO Trends** - 7-day and 30-day compliance

---

## Review Process

### Monthly Review

- [ ] Calculate actual vs target for each SLO
- [ ] Review incidents that impacted SLOs
- [ ] Adjust thresholds if needed
- [ ] Update error budgets

### Quarterly Review

- [ ] Assess if SLO targets are appropriate
- [ ] Review new capabilities that need SLOs
- [ ] Update documentation
- [ ] Communicate changes to stakeholders

---

## Appendix: Prometheus Alerts

```yaml
# Example alert rules for SLOs
groups:
  - name: slo-alerts
    rules:
      - alert: GatewayAvailabilityLow
        expr: |
          (1 - sum(rate(icn_gateway_requests_total{status=~"5.."}[1h])) /
               sum(rate(icn_gateway_requests_total[1h]))) < 0.999
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: Gateway availability below 99.9%

      - alert: APILatencyHigh
        expr: |
          histogram_quantile(0.99,
            sum(rate(icn_gateway_request_duration_seconds_bucket[5m])) by (le)
          ) > 0.5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: API p99 latency above 500ms
```

---

*These SLOs are targets for pilot deployment. Production deployments may have stricter requirements.*
