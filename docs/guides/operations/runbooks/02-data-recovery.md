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

**Governance state must be checked explicitly.** It does not replay from peers (see
"Replay from Peers" below), and a node with missing governance state starts cleanly and
reports healthy — the absence is silent. Compare against what this node is expected to
hold *before* declaring the restore complete:

```bash
# Domains this node should know about
icnctl gov domain list

# Proposals per domain (repeat for each expected domain)
icnctl gov proposal list --domain-id "coop:your-domain"
```

If either list is short or empty against the expected set, **stop**: the backup predates
those records, or the governance store was not included. Restore from a governance-bearing
backup rather than starting the node and waiting for peers to fill the gap — they will not.

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
icnctl gov domain list # Governance domains -- must match the pre-restore expectation
```

## Replay from Peers

If backup is old or unavailable, some data can be recovered from peers via gossip:

1. **Start fresh node** with same identity (keystore)
2. **Connect to peers** - they will sync:
   - Ledger entries (via gossip)
   - Trust edges (via gossip)

> **Governance state does NOT replay from peers.** Domains, proposals, votes and
> delegations arriving over gossip are refused before they are applied, because a
> `GossipEntry` carries no signature binding its claimed author to its contents, so a
> replicated governance message cannot be distinguished from a forged one. See
> issue #2469 for the authenticated-replication work that will restore this.
>
> **Consequence for recovery:** a node restored from an old or missing backup will come
> back with *silently incomplete* governance state — it will not error, it will simply
> never learn the governance records it is missing. **Governance state must be recovered
> from a backup.** Treat the governance store as backup-only until #2469 lands, and verify
> it explicitly after any restore — see the governance check in Step 5 and the governance
> items in the Verification Checklist.

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
- [ ] Governance domains match the expected set (`icnctl gov domain list`) — these do **not**
      replay from peers, so a short list means data loss, not a pending sync
- [ ] Governance proposals present per domain (`icnctl gov proposal list --domain-id <id>`)

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
