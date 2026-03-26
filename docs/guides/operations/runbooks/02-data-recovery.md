# Data Recovery Procedure

## Summary

Procedure for recovering ICN node data from backup after data loss or corruption.

**Use when**:
- Database corruption detected
- Accidental data deletion
- Hardware failure with data loss
- Failed upgrade corrupted data

**Do NOT use when**:
- Node is running fine (backup first!)
- Just need to restart (use Emergency Restart)

## Prerequisites

- [ ] Valid backup file (`.tar` archive from `icnctl backup`)
- [ ] Keystore passphrase
- [ ] Node is stopped
- [ ] Sufficient disk space (2x backup size)

## Procedure

### Step 1: Stop the Node

**Kubernetes**:
```bash
kubectl -n icn scale deployment/icn-daemon --replicas=0
kubectl -n icn get pods  # Confirm no pods running
```

**Systemd**:
```bash
systemctl stop icnd
systemctl status icnd  # Confirm stopped
```

### Step 2: Verify Backup Integrity

```bash
# Verify backup before restoring
icnctl verify-backup /path/to/backup.tar

# Expected output:
# ✓ BACKUP VERIFICATION PASSED

# If verification fails, try older backup
icnctl verify-backup /path/to/backup-older.tar
```

### Step 3: Preserve Current Data (Optional)

If there's any chance current data is recoverable:

```bash
# Move current data aside
mv ~/.icn ~/.icn.corrupted.$(date +%Y%m%d)

# Or for K8s with PVC
kubectl -n icn exec deploy/icn-daemon -- mv /data /data.corrupted
```

### Step 4: Restore from Backup

```bash
# Restore to data directory
icnctl --data-dir ~/.icn restore /path/to/backup.tar

# If data directory exists, use --force
icnctl --data-dir ~/.icn restore /path/to/backup.tar --force
```

**For Kubernetes** (restore to PVC):
```bash
# Create temporary pod to access PVC
kubectl -n icn run restore-pod --image=alpine --restart=Never \
  --overrides='{"spec":{"containers":[{"name":"restore","image":"alpine","command":["sleep","3600"],"volumeMounts":[{"name":"data","mountPath":"/data"}]}],"volumes":[{"name":"data","persistentVolumeClaim":{"claimName":"icn-data"}}]}}'

# Wait for pod
kubectl -n icn wait --for=condition=Ready pod/restore-pod --timeout=60s

# Copy backup into pod and extract
kubectl -n icn cp backup.tar restore-pod:/tmp/backup.tar
kubectl -n icn exec restore-pod -- tar -xf /tmp/backup.tar -C /data

# Cleanup
kubectl -n icn delete pod restore-pod
```

### Step 5: Verify Restored Data

```bash
# Check identity is accessible
ICN_PASSPHRASE="your-passphrase" icnctl --data-dir ~/.icn id show

# Verify ledger integrity (if had transactions)
icnctl verify-backup /path/to/backup.tar --verify-ledger
```

### Step 6: Restart Node

**Kubernetes**:
```bash
kubectl -n icn scale deployment/icn-daemon --replicas=1
kubectl -n icn logs -f deployment/icn-daemon
```

**Systemd**:
```bash
systemctl start icnd
journalctl -u icnd -f
```

### Step 7: Verify Node Health

```bash
# Check metrics
curl -s http://localhost:9100/metrics | grep icn_supervisor_state

# Check gossip sync is happening
watch -n5 'curl -s http://localhost:9100/metrics | grep icn_gossip'

# Verify expected data present
icnctl ledger balance  # If had transactions
icnctl trust list      # If had trust edges
```

## Replay from Peers

If backup is old or unavailable, data can be recovered from peers via gossip:

1. **Start fresh node** with same identity (keystore)
2. **Connect to peers** - they will sync:
   - Ledger entries (via gossip)
   - Trust edges (via gossip)
   - Governance proposals (via gossip)

```bash
# After starting with keystore only:
icnctl status

# Monitor sync progress
watch -n5 'curl -s http://localhost:9100/metrics | grep -E "icn_ledger_entries|icn_trust_edges"'
```

**Note**: Some local state (e.g., pending proposals you created) may not sync from peers.

## Verification Checklist

- [ ] Node starts without errors
- [ ] Identity matches expected DID
- [ ] Metrics endpoint responding
- [ ] Gossip connecting to peers
- [ ] Ledger balance correct (if applicable)
- [ ] Trust edges present (if applicable)

## Rollback

If restored data causes problems:

```bash
# Stop node
systemctl stop icnd  # or scale to 0

# Restore the corrupted data we preserved
rm -rf ~/.icn
mv ~/.icn.corrupted.* ~/.icn

# Try different recovery approach
```

## Related

- [Emergency Restart](./01-emergency-restart.md) - If just need restart
- [Version Upgrade](./03-version-upgrade.md) - If upgrade caused corruption
- [Troubleshooting](./05-troubleshooting.md) - Common issues
