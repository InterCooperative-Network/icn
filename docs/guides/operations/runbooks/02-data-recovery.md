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

> **Governance state also needs checking, but not here.** `icnctl gov ...` talks to the
> daemon over RPC and the node is still stopped at this point. The governance verification
> is in Step 7, after the restart.

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

**Verify governance state explicitly.** It does not replay from peers (see "Replay from
Peers" below), and a node with missing governance state starts cleanly and reports healthy —
the absence is silent, so nothing above will surface it. Compare against what this node is
expected to hold:

```bash
# Domains this node should know about
icnctl gov domain list

# Proposals per domain (repeat for each expected domain)
icnctl gov proposal list --domain-id "coop:your-domain"

```

> **These two listings are partial checks, not proof of completeness.** Votes
> (`gov:vote:*`) and delegations (`gov:delegation:*`) are persisted independently of the
> proposals they belong to, so a backup can hold every expected domain and proposal while
> silently omitting later votes or delegations. **There is currently no supported way to
> verify that a restored governance store contains every vote and delegation** — see the
> limitation below.

If any of these is short or empty against the expected set, **stop and do not treat the
recovery as complete**: the backup predates those records, or the governance store was not
included. Stop the node again and restore from a governance-bearing backup — waiting for
peers to fill the gap will not work.

### Governance completeness cannot currently be verified

**The available CLI and RPC surfaces cannot prove a restored governance store holds every
vote and delegation.** Verified against the implementation, not the command definitions:

| Surface | Why it does not work |
|---|---|
| `icnctl gov vote show --proposal-id` | Unimplemented — exits with "Vote show command not yet supported via RPC" |
| `icnctl gov vote delegations` | Returns only the **authenticated caller's own** delegations; delegations between other members are invisible |
| `GET /gov/delegations` | Same caller-scoped restriction |
| votes over RPC / gateway | No such method or route exists |

Tracked in **#2472**.

**What this means operationally.** A restore that silently omits votes or delegations leaves
a still-open proposal to close on an **incorrect tally or delegation weight** — a wrong
governance outcome that looks like a normal decision, not an error. Inbound replication is
refused (#2469), so peers will not refill the gap, and nothing above will detect it.

**Required policy until #2472 lands:**

1. Restore governance only from a **known-complete governance-bearing backup** whose
   provenance and completeness were established *before* the failure — not inferred
   afterwards from the restored node.
2. If backup completeness is uncertain, **do not use that node to resume or close
   still-open proposals.** Recovery from an unverified backup is not supported for open
   governance processes.
3. Treat the domain and proposal listings above as useful partial checks only. They can
   prove state is *missing*; they cannot prove it is *complete*.

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
> from a backup.** Treat the governance store as backup-only until #2469 lands. Note that
> vote and delegation completeness cannot currently be verified after a restore (#2472), so
> backup provenance must be established beforehand — see Step 7 and the Verification Checklist.

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
- [ ] Backup provenance established **before** the failure as governance-complete — vote and
      delegation completeness **cannot be verified after the fact** with current surfaces (#2472)
- [ ] If backup completeness is uncertain: this node is **not** used to resume or close
      still-open proposals (an incomplete tally produces a wrong outcome, not a visible failure)

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
