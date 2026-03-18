# Phase 0 Operational Monitoring

**Status**: Configured 2026-02-14
**Applies to**: Phase 0 pilot deployment on K3s

## Overview

ICN daemon exposes Prometheus metrics on port 9100. ServiceMonitor and PrometheusRule resources configure automated scraping and alerting for critical security and operational events.

## Alert Coverage

### 1. Byzantine Detection (icn.byzantine_detection)

| Alert | Metric | Threshold | Severity | Meaning |
|-------|--------|-----------|----------|---------|
| `ICNByzantineNodeQuarantined` | `icn_misbehavior_quarantined_peers` | > 0 for 1m | warning | Node quarantined for protocol violations |
| `ICNByzantineNodeAutoBanned` | `increase(icn_misbehavior_auto_bans_total[5m])` | > 0 | critical | Critical violation triggered auto-ban |
| `ICNHighViolationRate` | `rate(icn_misbehavior_violations_total[5m])` | > 1/sec for 5m | warning | High Byzantine violation rate |

**Test**:
```bash
# Trigger violation metric (requires special test mode)
kubectl exec -it -n icn deployment/icn-daemon -- \
  curl -X POST localhost:9100/test/trigger_violation
```

### 2. Authentication & Authorization (icn.auth_security)

| Alert | Metric | Threshold | Severity | Meaning |
|-------|--------|-----------|----------|---------|
| `ICNHighAuthFailureRate` | `rate(icn_gateway_auth_failures_total[5m])` | > 0.5/sec for 2m | warning | Many failed login attempts |
| `ICNAuthenticationAttack` | `rate(icn_gateway_auth_failures_total[1m])` | > 5/sec | critical | Possible credential stuffing attack |
| `ICNAuthorizationFailures` | `rate(icn_gateway_authorization_failures_total[5m])` | > 1/sec for 2m | warning | Users hitting permission boundaries |
| `ICNRateLimitAttack` | `rate(icn_gateway_rate_limit_exceeded_total[1m])` | > 10/sec for 1m | warning | Excessive API abuse |

**Test**:
```bash
# Trigger auth failure metric
for i in {1..10}; do
  curl -X POST http://gateway:8080/v1/auth/verify \
    -H "Content-Type: application/json" \
    -d '{"did": "did:icn:invalid", "challenge_id": "bad", "signature": "fake"}'
done

# Check metric
curl -s http://gateway:9100/metrics | grep icn_gateway_auth_failures_total
```

### 3. Network Health (icn.network_health)

| Alert | Metric | Threshold | Severity | Meaning |
|-------|--------|-----------|----------|---------|
| `ICNNetworkPartition` | `icn_network_connections_active` | < 2 for 2m | warning | Possible network partition |
| `ICNNodeIsolated` | `icn_network_connections_active` | == 0 for 1m | critical | Node has no network connections |
| `ICNHighMessageFailureRate` | `rate(icn_network_messages_failed_total[5m])` | > 0.1/sec for 5m | warning | Network message failures |

**Test**:
```bash
# Check current connections
curl -s http://gateway:9100/metrics | grep icn_network_connections_active
```

### 4. Ledger Consistency (icn.ledger_consistency)

| Alert | Metric | Threshold | Severity | Meaning |
|-------|--------|-----------|----------|---------|
| `ICNLedgerEntriesQuarantined` | `icn_ledger_entries_quarantined` | > 0 for 1m | critical | Ledger fork detected |
| `ICNLedgerBalanceInconsistency` | `abs(icn_ledger_balances_total)` | > 0.01 | critical | Sum of balances non-zero |

**Test**:
```bash
# Check ledger balance sum
curl -s http://gateway:9100/metrics | grep icn_ledger_balances_total
```

### 5. Compute Layer (icn.compute_layer)

| Alert | Metric | Threshold | Severity | Meaning |
|-------|--------|-----------|----------|---------|
| `ICNComputeTaskTimeout` | `rate(icn_compute_tasks_timeout_total[5m])` | > 0.1/sec for 5m | warning | Compute tasks timing out |
| `ICNComputeSignatureFailures` | `rate(icn_compute_signatures_invalid_total[5m])` | > 0 | critical | Invalid compute signatures (Byzantine executor) |

### 6. System Resources (icn.system_resources)

| Alert | Metric | Threshold | Severity | Meaning |
|-------|--------|-----------|----------|---------|
| `ICNHighMemoryUsage` | `process_resident_memory_bytes / 1GB` | > 2GB for 10m | warning | High memory usage |
| `ICNMemoryLeak` | `rate(process_resident_memory_bytes[1h])` | > 1MB/sec for 1h | critical | Possible memory leak |

## Accessing Monitoring

### Prometheus

**K3s NodePort**:
```bash
kubectl port-forward -n monitoring svc/prometheus-kube-prometheus-prometheus 9090:9090
# Open http://localhost:9090
```

**Query Examples**:
```promql
# Auth failures in last 5 minutes
rate(icn_gateway_auth_failures_total[5m])

# Byzantine violations by type
sum by (violation_type) (icn_misbehavior_violations_total)

# Active network connections
icn_network_connections_active

# Governance proposals closed
icn_gateway_governance_proposals_closed_total
```

### Grafana

**K3s NodePort**:
```bash
kubectl port-forward -n monitoring svc/grafana 3000:80
# Open http://localhost:3000
# Login: admin / <password from secret>
```

**Get Grafana password**:
```bash
kubectl get secret -n monitoring grafana -o jsonpath="{.data.admin-password}" | base64 -d
```

**Pre-configured Dashboards**:
- ICN Overview (dashboard ID: `icn-overview`)
- Byzantine Detection (dashboard ID: `icn-byzantine`)
- Gateway API Metrics (dashboard ID: `icn-gateway`)

## Alert Routing

Alerts are sent to:
1. **Prometheus Alertmanager** (if configured)
2. **K3s Events** (via kube-state-metrics)
3. **Logs** (via tracing span)

To configure Slack/Email alerting, edit `deploy/k8s/alertmanager-config.yaml`.

## Testing Alert Flow

### 1. Test Auth Failure Alert

```bash
# Generate 10 auth failures quickly
for i in {1..10}; do
  curl -X POST http://10.8.10.40:30080/v1/auth/verify \
    -H "Content-Type: application/json" \
    -d "{\"did\": \"did:icn:test$i\", \"challenge_id\": \"bad\", \"signature\": \"fake\"}" &
done
wait

# Wait 2 minutes (for: 2m)
sleep 120

# Check Prometheus for firing alert
curl -s 'http://localhost:9090/api/v1/alerts' | jq '.data.alerts[] | select(.labels.alertname == "ICNHighAuthFailureRate")'
```

### 2. Test Byzantine Quarantine Alert

```bash
# Check metric exists
kubectl exec -n icn deployment/icn-daemon -- \
  curl -s localhost:9100/metrics | grep icn_misbehavior_quarantined_peers

# If metric > 0, alert should fire after 1 minute
```

### 3. Test Signature Failure Alert

```bash
# This requires actual compute task execution with invalid signature
# See Phase 0 compute validation tests
```

## Metrics Endpoint

**Direct access to metrics**:
```bash
# From within cluster
kubectl exec -n icn deployment/icn-daemon -- curl -s localhost:9100/metrics

# Via port-forward
kubectl port-forward -n icn deployment/icn-daemon 9100:9100
curl http://localhost:9100/metrics
```

**Key Metric Families**:
- `icn_gateway_auth_*` — Authentication/authorization
- `icn_misbehavior_*` — Byzantine detection
- `icn_network_*` — Network connections and messages
- `icn_ledger_*` — Ledger operations and consistency
- `icn_compute_*` — Distributed compute
- `icn_gateway_governance_*` — Governance proposals and votes
- `process_*` — System resource usage (from Prometheus)

## Troubleshooting

### Alert Not Firing

1. **Check metric is exposed**:
   ```bash
   kubectl exec -n icn deployment/icn-daemon -- \
     curl -s localhost:9100/metrics | grep <metric_name>
   ```

2. **Check ServiceMonitor is active**:
   ```bash
   kubectl get servicemonitor -n icn icn-daemon
   kubectl describe servicemonitor -n icn icn-daemon
   ```

3. **Check Prometheus target**:
   ```bash
   # Open http://localhost:9090/targets
   # Look for icn/icn-daemon/0 target
   ```

4. **Check PrometheusRule**:
   ```bash
   kubectl get prometheusrule -n monitoring icn-alerts
   kubectl describe prometheusrule -n monitoring icn-alerts
   ```

### Metric Not Incrementing

1. **Verify code path executes**:
   ```bash
   kubectl logs -n icn deployment/icn-daemon | grep "auth failure\|quarantine\|signature"
   ```

2. **Check metric initialization**:
   ```bash
   # Metrics must be described in init_descriptions()
   # See icn-obs/src/metrics/gateway.rs
   ```

3. **Force metric increment** (if test endpoint exists):
   ```bash
   curl -X POST http://gateway:9100/test/increment_metric \
     -H "Content-Type: application/json" \
     -d '{"metric": "icn_gateway_auth_failures_total", "labels": {"reason": "test"}}'
   ```

### High Cardinality Warning

If you see:
```
FX clearing balance metric skipped: currency cardinality limit (100) reached
```

This is expected cardinality protection. Not a failure.

## Phase 0 Checklist

- [x] ServiceMonitor deployed and scraping metrics
- [x] PrometheusRule configured with Phase 0 alerts
- [x] Byzantine quarantine alerts active
- [x] Auth failure alerts active
- [x] Signature validation alerts active
- [x] Network partition alerts active
- [x] Ledger consistency alerts active
- [x] Grafana dashboards configured
- [ ] Alert routing to Slack/Email (optional for demo)
- [ ] Simulated violation test passed
- [ ] Simulated auth attack test passed

## References

- [ServiceMonitor YAML](../../../deploy/k8s/monitoring/servicemonitor.yaml)
- [Prometheus Metrics](../../../icn/crates/icn-obs/src/metrics/gateway.rs)
- [Phase 0 Spec](../../plans/2026-02-14-icn-consensus-stack-a-to-b.md#operational-monitoring-for-demo)
