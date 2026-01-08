# Emergency Node Restart

## Summary

Emergency procedure for restarting an unresponsive or misbehaving ICN node.

**Use when**:
- Node stops responding to RPC/API calls
- Metrics show unhealthy state
- Memory or CPU runaway
- Deadlock suspected

**Do NOT use when**:
- Routine maintenance (use rolling restart)
- Data corruption suspected (use Data Recovery runbook)

## Prerequisites

- [ ] Access to node (SSH or kubectl)
- [ ] Keystore passphrase available
- [ ] Backup exists (check with `icnctl verify-backup`)

## Procedure

### Kubernetes Deployment

#### 1. Assess Current State

```bash
# Check pod status
kubectl -n icn get pods -o wide

# Check recent events
kubectl -n icn get events --sort-by='.lastTimestamp' | tail -20

# Check resource usage
kubectl -n icn top pod
```

#### 2. Capture Diagnostics (if possible)

```bash
# Save logs before restart
kubectl -n icn logs deployment/icn-daemon --tail=1000 > /tmp/icn-pre-restart.log

# Save metrics snapshot
curl -s http://<node-ip>:9100/metrics > /tmp/icn-pre-restart-metrics.txt
```

#### 3. Graceful Restart (Preferred)

```bash
# Rolling restart - zero downtime
kubectl -n icn rollout restart deployment/icn-daemon

# Wait for completion
kubectl -n icn rollout status deployment/icn-daemon --timeout=300s
```

#### 4. Force Restart (If Graceful Fails)

```bash
# Delete pod to force recreation
kubectl -n icn delete pod -l app=icn-daemon --force --grace-period=0

# Verify new pod starts
kubectl -n icn get pods -w
```

### Systemd Deployment

#### 1. Assess Current State

```bash
# Check service status
systemctl status icnd

# Check recent logs
journalctl -u icnd -n 100 --no-pager

# Check resource usage
ps aux | grep icnd
```

#### 2. Capture Diagnostics

```bash
# Save logs
journalctl -u icnd -n 5000 > /tmp/icn-pre-restart.log
```

#### 3. Graceful Restart

```bash
# Send SIGTERM for graceful shutdown
systemctl restart icnd

# Wait and verify
sleep 10
systemctl status icnd
```

#### 4. Force Restart

```bash
# If graceful fails, force kill
systemctl kill -s SIGKILL icnd
systemctl start icnd
```

## Verification

After restart, verify node health:

```bash
# 1. Check process is running
pgrep -f icnd || kubectl -n icn get pods

# 2. Check identity accessible
icnctl id show
# OR: kubectl -n icn exec deploy/icn-daemon -- icnctl id show

# 3. Check metrics endpoint
curl -s http://localhost:9100/metrics | grep icn_supervisor_state
# Should show: icn_supervisor_state 2 (running)

# 4. Check gossip connectivity
curl -s http://localhost:9100/metrics | grep icn_gossip_peers
# Should show peers > 0 (unless single node)

# 5. Check RPC responding
icnctl status
```

## Rollback

If restart causes worse problems:

### Kubernetes
```bash
# Rollback to previous deployment
kubectl -n icn rollout undo deployment/icn-daemon

# Verify rollback
kubectl -n icn rollout status deployment/icn-daemon
```

### Systemd
```bash
# If new binary was deployed, revert
# (Assumes previous binary saved)
cp /usr/local/bin/icnd.backup /usr/local/bin/icnd
systemctl restart icnd
```

## Post-Incident

1. [ ] Document what triggered the restart
2. [ ] Review captured logs for root cause
3. [ ] Create issue if bug found
4. [ ] Update monitoring if detection was slow

## Related

- [Data Recovery](./02-data-recovery.md) - If data corruption suspected
- [Troubleshooting](./05-troubleshooting.md) - Common issues
- [Version Upgrade](./03-version-upgrade.md) - If upgrade caused issue
