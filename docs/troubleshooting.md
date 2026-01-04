# ICN Troubleshooting Runbooks

This document provides step-by-step procedures for diagnosing and resolving common operational issues in ICN deployments.

**Audience**: ICN node operators, system administrators, SRE teams

**Related Documents**:
- [Incident Response Playbook](incident-response.md) - For security incidents and critical failures
- [Production Hardening](production-hardening.md) - Security configuration reference

> **Note**: Commands marked with "(future feature)" are planned but not yet implemented.
> These are included for completeness and will be available in future ICN releases.

---

## Table of Contents

1. [Runbook Template](#runbook-template)
2. [High Memory Usage](#high-memory-usage)
3. [Gossip Convergence Failure](#gossip-convergence-failure)
4. [Ledger Sync Lag](#ledger-sync-lag)
5. [Trust Computation Errors](#trust-computation-errors)
6. [Gateway Rate Limiting](#gateway-rate-limiting)
7. [Node Won't Start](#node-wont-start)
8. [Quick Reference: Key Metrics](#quick-reference-key-metrics)
9. [Escalation Paths](#escalation-paths)

---

## Runbook Template

Each runbook follows this standard structure:

```
## [Issue Name]

**Severity**: P1-P3
**Typical Duration**: Time to resolve
**Skills Required**: What expertise is needed

### Symptoms
- Observable indicators of the problem

### Detection
- How to identify this issue (metrics, logs, alerts)

### Diagnosis Steps
1. Step-by-step investigation
2. Decision tree for root cause

### Resolution Actions
- Specific commands and procedures

### Prevention
- How to avoid recurrence

### Escalation
- When and how to escalate
```

---

## High Memory Usage

**Severity**: P2 (can escalate to P1 if OOM)
**Typical Duration**: 15-60 minutes
**Skills Required**: Linux administration, Kubernetes basics

### Symptoms

- Pod approaching memory limits
- Slow response times
- OOMKilled container restarts
- Alertmanager: `ICNHighMemory` alert firing
- Grafana: Memory usage graph trending upward

### Detection

```bash
# Check current memory usage (K3s)
sudo kubectl -n icn top pods

# Check container limits vs usage
sudo kubectl -n icn describe pod -l app=icn | grep -A 5 "Limits:"

# Check for OOMKilled events
sudo kubectl -n icn get events --field-selector reason=OOMKilled

# Prometheus query
container_memory_working_set_bytes{namespace="icn", container="icnd"} / container_spec_memory_limit_bytes{namespace="icn", container="icnd"}
```

### Diagnosis Steps

1. **Identify memory consumers**:
   ```bash
   # SSH to pod and check memory breakdown
   sudo kubectl -n icn exec deploy/icn-daemon -- sh -c "cat /proc/meminfo"

   # Check Sled database cache size
   sudo kubectl -n icn exec deploy/icn-daemon -- ls -lh /data/
   ```

2. **Check for memory leaks** (sustained growth over time):
   - Review Grafana memory dashboard for trends
   - Check if memory grows without corresponding workload increase
   - Look for task/thread count growth

3. **Identify workload patterns**:
   ```bash
   # Check gossip message rate
   curl -s http://10.8.10.40:30100/metrics | grep icn_gossip_messages

   # Check active connections
   curl -s http://10.8.10.40:30100/metrics | grep icn_network_connections
   ```

4. **Decision tree**:
   ```
   Memory growing steadily?
   ├── Yes → Possible memory leak → Collect heap profile, restart pod
   └── No → Memory spikes?
       ├── Yes → Workload spike → Check gossip/ledger activity
       └── No → Normal operation → Consider increasing limits
   ```

### Resolution Actions

**Immediate relief** (buy time):
```bash
# Restart pod to reclaim memory
sudo kubectl -n icn rollout restart deployment/icn-daemon
```

**If Sled cache is too large**:
```bash
# Check data directory size
ssh atlas "du -sh /mnt/storage/k8s/icn-data/*"

# Sled will auto-compact, but can trigger compaction via restart
```

**If limits are insufficient**:
```bash
# Edit deployment to increase memory limit
sudo kubectl -n icn edit deployment icn-daemon
# Change: resources.limits.memory: 2Gi → 4Gi
```

**If suspected memory leak**:
1. Collect memory diagnostics from the host:
   ```bash
   # Capture memory map of the process
   sudo kubectl -n icn exec deploy/icn-daemon -- cat /proc/1/smaps > smaps.txt

   # Capture current metrics
   curl -s http://10.8.10.40:30100/metrics > metrics-snapshot.txt
   ```
2. Report to ICN developers with:
   - Memory diagnostics (smaps, metrics snapshot)
   - Memory growth timeline from Grafana
   - Workload characteristics
   - ICN version

### Prevention

- **Set appropriate limits**: Base on observed peak + 20% buffer
- **Enable memory alerting**: Alert at 80% to catch before OOM
- **Regular monitoring**: Check weekly memory trends
- **Keep ICN updated**: Memory optimizations in new releases

### Escalation

- **P1 escalation** if: OOM restarts > 3 in 1 hour
- **Contact**: ICN development team via GitHub issue
- **Provide**: Memory profiles, Prometheus data export, container logs

---

## Gossip Convergence Failure

**Severity**: P2
**Typical Duration**: 30-90 minutes
**Skills Required**: Distributed systems understanding, networking basics

### Symptoms

- Nodes have divergent views of the network
- Messages not propagating to all nodes
- Entry counts differ significantly between nodes
- Dashboard shows uneven topic coverage
- Inconsistent data across cooperative members

### Detection

```bash
# Check gossip metrics
curl -s http://10.8.10.40:30100/metrics | grep icn_gossip

# Key metrics to watch:
# - icn_gossip_entries_received_total (incoming entries)
# - icn_gossip_entries_published_total (outgoing entries)
# - icn_gossip_entries_total (total stored entries)
# - icn_gossip_subscriptions_total (active subscriptions)
# - icn_gossip_subscriptions_rejected_total (rejected subscriptions)
```

### Diagnosis Steps

1. **Check network connectivity**:
   ```bash
   # Verify peer count
   curl http://10.8.10.40:30080/v1/health | jq '.active_connections'

   # Check if peers are reachable
   sudo kubectl -n icn exec deploy/icn-daemon -- curl -s localhost:9100/metrics | grep icn_network
   ```

2. **Check topic subscriptions**:
   ```bash
   # List subscribed topics
   curl -s http://10.8.10.40:30100/metrics | grep icn_gossip_subscriptions

   # Compare subscription counts across nodes (if multi-node)
   ```

3. **Check message flow balance**:
   - Compare sent vs received message counts
   - Large imbalance indicates sync issues
   ```bash
   # Check message flow balance
   curl -s http://10.8.10.40:30100/metrics | grep -E "icn_gossip_(entries_received|entries_published)_total"
   ```

4. **Check for rejected messages**:
   ```bash
   # High rejection rate indicates trust or rate limiting issues
   curl -s http://10.8.10.40:30100/metrics | grep rejected
   ```

5. **Decision tree**:
   ```
   Peers connected?
   ├── No → Network issue → Check network policies, firewall
   └── Yes → Messages flowing?
       ├── No → Check subscriptions, topic configuration
       └── Yes → Convergence slow?
           ├── Yes → Compare entry counts across nodes, check anti-entropy
           └── No → False alarm, verify with manual check
   ```

### Resolution Actions

**If network connectivity issue**:
```bash
# Check network policies
sudo kubectl -n icn get networkpolicies

# Verify QUIC port is accessible (default 5600)
sudo kubectl -n icn get svc

# Test peer reachability from pod
sudo kubectl -n icn exec deploy/icn-daemon -- nc -zv <peer-ip> 5600
```

**If subscription issue**:
```bash
# Restart to re-establish subscriptions
sudo kubectl -n icn rollout restart deployment/icn-daemon
```

**If clock drift**:
```bash
# Check system time on nodes
sudo kubectl -n icn exec deploy/icn-daemon -- date
date

# Ensure NTP is configured on host
timedatectl status
```

**Force anti-entropy sync** (future feature):
```bash
# Trigger full sync with a peer
icnctl gossip sync --peer did:icn:<peer-did>
```

### Prevention

- **Monitor sync lag**: Alert on `icn_gossip_sync_lag_seconds > 60`
- **Multiple peers**: Ensure at least 3 peers for redundancy
- **NTP configured**: System clocks synchronized
- **Network monitoring**: Watch for packet loss or latency spikes

### Escalation

- **P1 escalation** if: No convergence after 2 hours
- **Contact**: ICN development team
- **Provide**: Gossip metrics from all nodes, network topology

---

## Ledger Sync Lag

**Severity**: P2
**Typical Duration**: 30-120 minutes
**Skills Required**: Understanding of distributed ledgers, database basics

### Symptoms

- Balance queries return stale data
- New transactions not appearing
- `icn_ledger_sync_lag_seconds > 300` alert
- Dashboard shows growing sync lag
- Members report transaction visibility delays

### Detection

```bash
# Check ledger metrics
curl -s http://10.8.10.40:30100/metrics | grep icn_ledger

# Key metrics:
# - icn_ledger_sync_lag_seconds
# - icn_ledger_entries_total
# - icn_ledger_quarantine_size
# - icn_ledger_merge_conflicts_total
```

### Diagnosis Steps

1. **Check peer connectivity**:
   ```bash
   curl http://10.8.10.40:30080/v1/health | jq '.active_connections'
   ```

2. **Check quarantine size** (indicates problematic entries):
   ```bash
   curl -s http://10.8.10.40:30100/metrics | grep quarantine
   ```

3. **Verify entry propagation**:
   ```bash
   # Compare entry counts across nodes
   # Node 1:
   curl -s http://node1:9100/metrics | grep icn_ledger_entries_total
   # Node 2:
   curl -s http://node2:9100/metrics | grep icn_ledger_entries_total
   ```

4. **Check for forks**:
   - Different entry counts on different nodes
   - Conflicting entries for same account

5. **Decision tree**:
   ```
   Quarantine growing?
   ├── Yes → Conflicting entries → See Incident Response for quarantine handling
   └── No → Entries propagating?
       ├── No → Check gossip → See Gossip Convergence runbook
       └── Yes → Just slow?
           ├── Yes → High volume → Normal during catch-up
           └── No → Check for network issues
   ```

### Resolution Actions

**If slow propagation** (high volume):
```bash
# Monitor progress - should improve over time
watch -n 5 "curl -s http://10.8.10.40:30100/metrics | grep icn_ledger_entries_total"
```

**If quarantine issues**:
```bash
# List quarantined entries (future command)
icnctl ledger quarantine list

# See incident-response.md for quarantine resolution
```

**Force sync from healthy peer** (future feature):
```bash
icnctl ledger sync --from did:icn:<healthy-peer>
```

**If persistent lag with no progress**:
```bash
# Restart to reinitialize sync state
sudo kubectl -n icn rollout restart deployment/icn-daemon
```

### Prevention

- **Monitor sync lag**: Alert on > 5 minutes
- **Regular health checks**: Compare entry counts weekly
- **Backup before upgrades**: Ensure restore capability
- **Test recovery**: Monthly sync recovery drill

### Escalation

- **P1 escalation** if: Sync lag > 1 hour or balances incorrect
- **Contact**: ICN development team and cooperative coordinators
- **Provide**: Ledger metrics, entry counts from all nodes

---

## Trust Computation Errors

**Severity**: P2
**Typical Duration**: 30-60 minutes
**Skills Required**: Graph algorithms understanding, database basics

### Symptoms

- Trust scores returning errors or unexpected values
- Rate limiting affecting legitimate users
- `icn_trust_computation_errors_total` incrementing
- Access control decisions failing
- Users reporting permission issues

### Detection

```bash
# Check trust metrics
curl -s http://10.8.10.40:30100/metrics | grep icn_trust

# Key metrics:
# - icn_trust_computation_errors_total
# - icn_trust_cache_hits_total / icn_trust_cache_misses_total
# - icn_trust_edges_total
# - icn_trust_computation_duration_seconds
```

### Diagnosis Steps

1. **Check error types**:
   ```bash
   sudo kubectl -n icn logs deployment/icn-daemon | grep -i "trust.*error" | tail -20
   ```

2. **Verify graph consistency**:
   ```bash
   # Check edge count
   curl -s http://10.8.10.40:30100/metrics | grep icn_trust_edges

   # Check for cycles or invalid edges in logs
   sudo kubectl -n icn logs deployment/icn-daemon | grep -i "trust.*cycle\|invalid.*edge"
   ```

3. **Check computation performance**:
   ```bash
   # High computation time indicates graph issues
   curl -s http://10.8.10.40:30100/metrics | grep icn_trust_computation_duration
   ```

4. **Decision tree**:
   ```
   Errors in logs?
   ├── Yes → What type?
   │   ├── "invalid edge" → Check edge data integrity
   │   ├── "cycle detected" → Graph has loops → May need cleanup
   │   └── "computation timeout" → Graph too large → Check cache
   └── No → Cache issues?
       ├── High cache miss rate → Cache not warming → Check config
       └── Normal → Transient issue → Monitor
   ```

### Resolution Actions

**Clear computation cache** (force recalculation):
```bash
# Restart pod to clear in-memory cache
sudo kubectl -n icn rollout restart deployment/icn-daemon
```

**Verify edge data**:
```bash
# List trust edges (future command)
icnctl trust list --format json | jq '.edges | length'
```

**If graph corruption suspected**:
1. Export current state for analysis:
   ```bash
   icnctl trust export > trust-graph-backup.json
   ```
2. Contact ICN team with graph data
3. May need to rebuild from source edges

**If single edge causing issues**:
```bash
# Remove problematic edge (future command)
icnctl trust remove --from did:icn:<from> --to did:icn:<to>
```

### Prevention

- **Monitor error rate**: Alert on > 1 error/minute
- **Cache tuning**: Ensure cache is properly sized
- **Edge validation**: Validate edges on creation
- **Regular audits**: Weekly trust graph consistency check

### Escalation

- **P1 escalation** if: Trust computation affecting access control
- **Contact**: ICN development team
- **Provide**: Trust graph export, error logs, computation metrics

---

## Gateway Rate Limiting

**Severity**: P3
**Typical Duration**: 15-30 minutes
**Skills Required**: HTTP/API understanding, rate limiting concepts

### Symptoms

- Clients receiving 429 Too Many Requests
- Legitimate operations being blocked
- `icn_gateway_rate_limit_exceeded_total` incrementing
- User complaints about "too many requests" errors
- API latency spikes due to queuing

### Detection

```bash
# Check rate limiting metrics
curl -s http://10.8.10.40:30100/metrics | grep rate_limit

# Check gateway response codes
curl -s http://10.8.10.40:30100/metrics | grep icn_gateway_requests | grep 429
```

### Diagnosis Steps

1. **Identify rate-limited clients**:
   ```bash
   # Check logs for rate limit events
   sudo kubectl -n icn logs deployment/icn-daemon | grep -i "rate.limit\|429" | tail -20
   ```

2. **Check client trust scores**:
   ```bash
   # Low trust = lower rate limits
   # Check trust metrics for affected DIDs
   ```

3. **Analyze request patterns**:
   ```bash
   # Check request rate by endpoint
   curl -s http://10.8.10.40:30100/metrics | grep icn_gateway_requests_total
   ```

4. **Decision tree**:
   ```
   Single client rate limited?
   ├── Yes → Check if legitimate
   │   ├── Legitimate → Consider whitelist or trust boost
   │   └── Abuse → Keep limits, consider block
   └── No → Multiple clients affected?
       ├── Yes → Rate limits too aggressive → Adjust globally
       └── No → Spike in traffic → Normal protection working
   ```

### Resolution Actions

**Adjust rate limits** (if too aggressive):
```bash
# Edit gateway configuration
sudo kubectl -n icn edit configmap icn-config
# Find rate_limit section and adjust values

# Restart to apply
sudo kubectl -n icn rollout restart deployment/icn-daemon
```

**Whitelist trusted client** (future feature):
```bash
icnctl gateway whitelist add did:icn:<client-did>
```

**If abuse detected**:
```bash
# Block abusive client (future feature)
icnctl gateway block did:icn:<abusive-did>

# Or reduce trust to minimize rate limit
icnctl trust set did:icn:<client-did> --score 0.1
```

**Temporary rate limit increase**:
```bash
# Environment variable override
sudo kubectl -n icn set env deployment/icn-daemon ICN_RATE_LIMIT_MULTIPLIER=2.0
```

### Prevention

- **Monitor rate limiting**: Alert on sustained high rate limit events
- **Trust-based limits**: Ensure trust scores correctly reflect client reliability
- **Capacity planning**: Ensure adequate resources for expected load
- **Client education**: Document rate limits for API consumers

### Escalation

- **P1 escalation** if: Rate limiting affecting critical cooperative operations
- **Contact**: ICN operations team, then development if config changes needed
- **Provide**: Rate limit metrics, affected client DIDs, traffic patterns

---

## Node Won't Start

**Severity**: P1
**Typical Duration**: 15-60 minutes
**Skills Required**: Linux administration, Kubernetes debugging

### Symptoms

- Pod stuck in CrashLoopBackOff or Error state
- Container exits immediately after start
- No health endpoint response
- Startup logs show errors
- Previous container logs show crash

### Detection

```bash
# Check pod status
sudo kubectl -n icn get pods

# Check pod events
sudo kubectl -n icn describe pod -l app=icn

# Check container logs
sudo kubectl -n icn logs deployment/icn-daemon --previous
```

### Diagnosis Steps

1. **Check container status**:
   ```bash
   sudo kubectl -n icn get pods -o jsonpath='{.items[0].status.containerStatuses[0]}'
   ```

2. **Review startup logs**:
   ```bash
   sudo kubectl -n icn logs deployment/icn-daemon --previous | head -50
   ```

3. **Check configuration**:
   ```bash
   # View current config
   sudo kubectl -n icn get configmap icn-config -o yaml
   ```

4. **Verify keystore**:
   ```bash
   # Check if keystore file exists
   sudo kubectl -n icn exec deploy/icn-daemon -- ls -la /data/keystore* 2>/dev/null || echo "No keystore"
   ```

5. **Check port availability**:
   ```bash
   # Ensure ports aren't already bound
   sudo kubectl -n icn exec deploy/icn-daemon -- ss -tlnp
   ```

6. **Decision tree**:
   ```
   Container starting?
   ├── No → Exit code?
   │   ├── 1 → Config error → Check config syntax
   │   ├── 137 → OOMKilled → Increase memory
   │   └── Other → Check logs for error
   └── Yes → Crashing after start?
       ├── Yes → Runtime error
       │   ├── "keystore" error → Keystore issue
       │   ├── "bind" error → Port conflict
       │   └── "permission" error → File permissions
       └── No → Health check failing?
           ├── Yes → Slow startup → Increase probe delays
           └── No → Should be working → Verify service routing
   ```

### Resolution Actions

**If configuration error**:
```bash
# Validate config syntax
sudo kubectl -n icn get configmap icn-config -o jsonpath='{.data.config\.toml}' | head -20

# Fix and reapply
sudo kubectl -n icn edit configmap icn-config
sudo kubectl -n icn rollout restart deployment/icn-daemon
```

**If keystore issue**:
```bash
# Check keystore accessibility
ssh atlas "ls -la /mnt/storage/k8s/icn-data/"

# If missing, restore from backup
# See incident-response.md for restore procedure
```

**If port conflict**:
```bash
# Check what's using the port
sudo kubectl -n icn exec deploy/icn-daemon -- ss -tlnp | grep 5600

# May need to kill stuck process or wait for cleanup
```

**If permission error**:
```bash
# Check data directory permissions
ssh atlas "ls -la /mnt/storage/k8s/icn-data/"

# Fix permissions if needed
ssh atlas "sudo chown -R 1000:1000 /mnt/storage/k8s/icn-data/"
```

**If OOMKilled**:
```bash
# Increase memory limit
sudo kubectl -n icn patch deployment icn-daemon -p '{"spec":{"template":{"spec":{"containers":[{"name":"icnd","resources":{"limits":{"memory":"4Gi"}}}]}}}}'
```

**If slow startup** (health check timeout):
```bash
# Increase probe delays
sudo kubectl -n icn edit deployment icn-daemon
# Adjust: initialDelaySeconds, periodSeconds, failureThreshold
```

### Prevention

- **Config validation**: Validate config before deployment
- **Backup keystore**: Regular encrypted backups
- **Resource monitoring**: Track resource usage trends
- **Staged rollouts**: Deploy changes incrementally

### Escalation

- **Immediate escalation** if: Cannot start after 30 minutes
- **Contact**: ICN development team
- **Provide**: Full pod logs, describe output, config dump (sanitized)

---

## Quick Reference: Key Metrics

| Issue | Key Metric | Alert Threshold |
|-------|-----------|-----------------|
| High Memory | `container_memory_working_set_bytes` | > 85% limit |
| Gossip Issues | `icn_gossip_subscriptions_rejected_total` | increasing rate |
| Ledger Lag | `icn_ledger_sync_lag_seconds` | > 300 seconds |
| Trust Errors | `icn_trust_computation_errors_total` | > 10/minute |
| Rate Limiting | `icn_gateway_rate_limit_exceeded_total` | > 100/minute |
| Restarts | `kube_pod_container_status_restarts_total` | > 3/hour |

### Quick Prometheus Queries

```promql
# Memory usage percentage
container_memory_working_set_bytes{namespace="icn"} / container_spec_memory_limit_bytes{namespace="icn"}

# Request error rate
rate(icn_gateway_requests_total{status=~"5.."}[5m]) / rate(icn_gateway_requests_total[5m])

# Gossip entry receive rate
rate(icn_gossip_entries_received_total[5m])

# Rate limit events per minute
rate(icn_gateway_rate_limit_exceeded_total[5m]) * 60

# Gossip message latency p99
histogram_quantile(0.99, rate(icn_gossip_message_latency_seconds_bucket[5m]))
```

---

## Escalation Paths

### When to Escalate

| Severity | Criteria | Response Time |
|----------|----------|---------------|
| P1 | Service down, data loss risk | Immediate |
| P2 | Degraded service, user impact | 1 hour |
| P3 | Minor issues, no user impact | Next business day |

### Escalation Contacts

1. **On-call operator**: First responder for all issues
2. **ICN development team**: GitHub issues for bugs/features
3. **Cooperative coordinators**: For user-facing impact

### Information to Gather Before Escalating

- [ ] Pod status and recent logs
- [ ] Relevant Prometheus metrics
- [ ] Timeline of events
- [ ] Actions already taken
- [ ] Current impact assessment

---

## Version History

- **2026-01-04**: Initial version with 6 runbooks (#221)
