# ICN Operations Guide

Comprehensive operational procedures and workflows for running ICN nodes in production.

## Table of Contents

1. [Overview](#overview)
2. [Daily Operations](#daily-operations)
3. [Monitoring & Health Checks](#monitoring--health-checks)
4. [Backup Procedures](#backup-procedures)
5. [Upgrade Procedures](#upgrade-procedures)
6. [Incident Response](#incident-response)
7. [Capacity Planning](#capacity-planning)
8. [Performance Tuning](#performance-tuning)
9. [Operational Command Reference](#operational-command-reference)
10. [Troubleshooting Workflows](#troubleshooting-workflows)

---

## Overview

This guide provides operational procedures for ICN node operators. It assumes you have already:
- Deployed ICN following [deployment-guide.md](deployment-guide.md)
- Configured systemd service (or equivalent)
- Set up monitoring dashboard and Prometheus

**Document Relationships:**
- **Deployment Guide**: Initial setup and installation
- **Operations Guide** (this document): Day-to-day operations and workflows
- **Incident Response**: Emergency procedures and troubleshooting
- **Architecture**: Technical design and system internals

---

## Daily Operations

### Morning Health Check (5 minutes)

**1. Check node status:**
```bash
# Verify daemon is running
systemctl status icnd

# Check current identity
icnctl id show

# View peer connections
icnctl status
```

**2. Review monitoring dashboard:**
- Navigate to `http://localhost:8080/` (or your configured monitoring port)
- Verify status is "Healthy" (green banner)
- Check active peer count (should be > 0 for connected nodes)
- Review gossip topics and entries

**3. Check metrics endpoint:**
```bash
# Verify Prometheus metrics are being exported
curl -s http://localhost:9100/metrics | grep icn_network_connections_active
```

**Expected output:**
```
icn_network_connections_active 3
```

**4. Review logs:**
```bash
# Check recent logs for errors
journalctl -u icnd --since "1 hour ago" | grep -i error

# Check for warnings
journalctl -u icnd --since "1 hour ago" | grep -i warn
```

**Action if issues found:**
- See [Troubleshooting Workflows](#troubleshooting-workflows)
- For critical issues, see [Incident Response](incident-response.md)

---

### Weekly Maintenance (15-30 minutes)

**1. Create backup:**
```bash
# Create backup (tarball contains encrypted keystore)
sudo -u icn icnctl backup ~/backups/icn-backup-$(date +%Y%m%d).tar

# Verify backup was created
ls -lh ~/backups/

# Copy to off-site storage
rsync -av ~/backups/ remote-server:/backup/icn/
```

**2. Review metrics trends:**
- Check dashboard for growth patterns
- Monitor ledger quarantine size (should be < 100)
- Review rate-limited messages (should be low)
- Check trust cache hit rate (should be > 80%)

**3. Check disk usage:**
```bash
# ICN data directory size
du -sh ~/.icn/

# Log file sizes
du -sh /var/log/icnd/
```

**Action if disk usage is high:**
- See [Capacity Planning](#capacity-planning)
- Consider log rotation or archival

**4. Update system packages:**
```bash
# Update OS packages (includes security updates)
sudo apt update && sudo apt upgrade -y

# Reboot if kernel updated (plan maintenance window)
```

---

### Monthly Tasks

**1. Review and archive old backups:**
```bash
# Keep last 4 weekly backups, delete older
find ~/backups/ -name "icn-backup-*.tar" -mtime +30 -delete
```

**2. Audit device list:**
```bash
# List all devices associated with identity
icnctl device list

# Revoke any compromised or unused devices
icnctl device revoke <device-id>
```

**3. Review operational metrics:**
- Average uptime
- Peer connection stability
- Ledger transaction volume
- Gossip entry growth rate
- Quarantine entry rate (should be near zero)

**4. Check for ICN updates:**
```bash
# Check current version
icnd --version

# Check repository for new releases
git fetch origin
git log --oneline HEAD..origin/main
```

---

## Monitoring & Health Checks

### Dashboard Overview

The monitoring dashboard (`http://localhost:8080/`) provides real-time visibility into:

**Status Banner:**
- **Healthy** (green): All systems operational
- **Degraded** (yellow): Non-critical issues detected (e.g., 100+ quarantine entries)
- **Unhealthy** (red): Critical issues (e.g., 1000+ quarantine entries)

**Key Metrics:**
- **Network Peers**: Active connections (should match expected community size)
- **Gossip Topics**: Number of active topics
- **Gossip Entries**: Total entries in gossip store
- **Ledger Quarantine**: Conflicting entries awaiting resolution

### Health Check Endpoint

The `/health` endpoint returns JSON status for external monitoring:

```bash
# Check health status
curl http://localhost:8080/health | jq

# Example healthy response:
{
  "status": "healthy",
  "uptime_seconds": 86400,
  "active_connections": 5,
  "gossip_topics": 3,
  "ledger_quarantine_size": 0,
  "timestamp": 1705234567
}
```

**HTTP Status Codes:**
- `200 OK`: Healthy or degraded (still operational)
- `503 Service Unavailable`: Unhealthy (critical issues)

**Integration with external monitoring:**
```yaml
# Example: Prometheus alerting rule
groups:
  - name: icn_health
    rules:
      - alert: ICNNodeUnhealthy
        expr: icn_health_status{status="unhealthy"} > 0
        for: 5m
        annotations:
          summary: "ICN node {{ $labels.instance }} is unhealthy"
```

### Key Metrics to Monitor

**Network Health:**
- `icn_network_connections_active`: Should be > 0 for connected nodes
- `icn_network_messages_rate_limited_total`: Should be low (< 1% of sent)
- `icn_network_bytes_sent_total` / `icn_network_bytes_received_total`: Balanced I/O

**Gossip Health:**
- `icn_gossip_topics_total`: Should match expected subscriptions
- `icn_gossip_entries_total`: Steadily growing
- `icn_gossip_subscriptions_total`: Should match community size

**Ledger Health:**
- `icn_ledger_quarantine_size`: Should be < 100 (warning), < 1000 (critical)
- `icn_ledger_merge_conflicts_total`: Should be low (occasional conflicts are normal)
- `icn_ledger_transactions_total`: Steadily growing with economic activity

**Trust Graph Health:**
- `icn_trust_cache_hits_total` / (`icn_trust_cache_hits_total` + `icn_trust_cache_misses_total`): Should be > 80%
- `icn_trust_edges_total`: Growing with community relationships

**System Health:**
- Disk usage: `~/.icn/` should not exceed available space
- Memory usage: ICNd should stay within configured limits
- CPU usage: Should be low except during signature verification bursts

---

## Backup Procedures

### Creating Backups

**Standard backup (weekly):**
```bash
# Create backup (tarball contains encrypted keystore) with timestamp
icnctl backup ~/backups/icn-backup-$(date +%Y%m%d).tar

# Backup includes:
# - Identity keystore (~/.icn/keystore.age)
# - Persistent store (~/.icn/store/)
# - Configuration (~/.icn/config.toml)
# - Device documents (~/.icn/devices/)
```

**Emergency backup (before risky operations):**
```bash
# Before upgrades, migrations, or major changes
icnctl backup ~/backups/icn-pre-upgrade-$(date +%Y%m%d-%H%M%S).tar
```

### Backup Storage Strategy

**Local retention:**
- Daily backups: Keep last 7 days
- Weekly backups: Keep last 4 weeks
- Monthly backups: Keep last 12 months

**Off-site storage:**
```bash
# Copy to remote server via rsync
rsync -av --delete ~/backups/ backup-server:/mnt/backup/icn/

# Or use cloud storage
rclone copy ~/backups/ remote:icn-backups/
```

**Backup verification:**
```bash
# Test restore in isolated environment
mkdir /tmp/icn-restore-test
cd /tmp/icn-restore-test
icnctl restore ~/backups/icn-backup-latest.tar --data-dir /tmp/icn-restore-test
# Verify keystore can be unlocked
ICN_DATA_DIR=/tmp/icn-restore-test icnctl id show
```

### Restoring from Backup

**Full restoration:**
```bash
# Stop ICNd
sudo systemctl stop icnd

# Backup current state (if any)
mv ~/.icn ~/.icn.old-$(date +%Y%m%d-%H%M%S)

# Restore from backup
icnctl restore ~/backups/icn-backup-20250114.tar
# Enter passphrase when prompted

# Verify identity
icnctl id show

# Start ICNd
sudo systemctl start icnd

# Verify node is healthy
icnctl status
```

**Partial restoration (keystore only):**
```bash
# Extract keystore from backup
icnctl restore ~/backups/icn-backup-20250114.tar --keystore-only
```

**See also:**
- [Incident Response: Ledger Corruption](incident-response.md#ledger-corruption-detected-p1)
- [Deployment Guide: Backup & Recovery](deployment-guide.md#backup--recovery)

---

## Upgrade Procedures

### Current Version: v0.1.x (Manual Process)

ICN is pre-v1.0 and does not yet have automated upgrade mechanisms. Upgrades require manual steps.

**Before upgrading:**
1. **Create backup** (see [Backup Procedures](#backup-procedures))
2. **Review release notes** for breaking changes
3. **Plan maintenance window** (5-15 minutes downtime)
4. **Notify community members** if running a shared node

**Upgrade steps:**

```bash
# 1. Stop ICNd
sudo systemctl stop icnd

# 2. Create pre-upgrade backup
icnctl backup ~/backups/icn-pre-upgrade-$(date +%Y%m%d-%H%M%S).tar

# 3. Pull latest code
cd ~/projects/icn/icn/
git fetch origin
git checkout <new-version-tag>  # e.g., v0.1.3

# 4. Build new binaries
cargo build --release

# 5. Install new binaries
sudo cp target/release/{icnd,icnctl} /usr/local/bin/

# 6. Run migrations (if required by release notes)
# Example: icnctl migrate --from v0.1.2 --to v0.1.3
# (Not yet implemented - manual migration steps in release notes)

# 7. Start ICNd
sudo systemctl start icnd

# 8. Verify node health
icnctl status
journalctl -u icnd -f  # Watch logs for errors

# 9. Check monitoring dashboard
# Navigate to http://localhost:8080/ and verify status is "Healthy"
```

**Rollback if issues occur:**
```bash
# Stop ICNd
sudo systemctl stop icnd

# Restore from pre-upgrade backup
icnctl restore ~/backups/icn-pre-upgrade-<timestamp>.tar

# Reinstall old binaries
cd ~/projects/icn/icn/
git checkout <old-version-tag>
cargo build --release
sudo cp target/release/{icnd,icnctl} /usr/local/bin/

# Start ICNd
sudo systemctl start icnd
```

### Future: Automated Upgrade Mechanism (v0.2+)

**Planned features (Track B1: Upgrade Mechanism):**
- Versioned network protocol (automatic version negotiation)
- ✅ **Graceful restart semantics** (IMPLEMENTED - preserves vector clocks, subscriptions, X25519 keys)
- `icnctl migrate` command for schema changes
- Rolling upgrade strategy for multi-node communities

**Target upgrade workflow:**
```bash
# Download new version
icnctl upgrade --check
# "New version v0.2.0 available"

# Perform automated upgrade
icnctl upgrade --to v0.2.0
# - Downloads new binaries
# - Verifies signatures
# - Creates automatic backup
# - Runs migrations
# - Restarts daemon with graceful handoff
# - Verifies health
# - Rolls back automatically if health check fails
```

**Rolling upgrades for communities:**
- Upgrade nodes one at a time
- Protocol version compatibility window (e.g., v0.2.x can talk to v0.1.x)
- Coordinated upgrade scheduling

---

## Incident Response

For emergency situations, see the comprehensive [Incident Response Playbook](incident-response.md).

**Quick reference:**

| Incident | Severity | First Action |
|----------|----------|--------------|
| Node Compromise | P0 | Isolate node immediately (`systemctl stop icnd`) |
| Ledger Corruption | P1 | Assess quarantine size, restore from backup if needed |
| Key Suspected Stolen | P0 | Revoke device immediately (`icnctl device revoke`) |
| Network Partition | P1 | Check connectivity, restart ICNd |
| Gossip Storm | P2 | Verify rate limiting, check peer health |
| Quarantine Growth | P2 | Inspect entries, identify patterns |

**Emergency contacts:**
- ICN maintainers: (To be established)
- Community coordinator: (Community-specific)
- Incident response on-call: (Community-specific)

---

## Capacity Planning

### Storage Growth Estimates

**Gossip store:**
- Small community (10-50 nodes): ~10 MB/month
- Medium community (50-200 nodes): ~100 MB/month
- Large community (200+ nodes): ~500+ MB/month

**Ledger:**
- Low activity (10 transactions/day): ~1 MB/month
- Medium activity (100 transactions/day): ~10 MB/month
- High activity (1000 transactions/day): ~100 MB/month

**Logs:**
- Default INFO level: ~10 MB/day
- DEBUG level (not recommended for production): ~100+ MB/day

**Recommendations:**
- Minimum 10 GB disk for production nodes
- Monitor disk usage weekly
- Set up log rotation:
  ```bash
  # /etc/logrotate.d/icnd
  /var/log/icnd/*.log {
      daily
      rotate 7
      compress
      missingok
      notifempty
  }
  ```

### Memory Requirements

**Base usage:** ~100 MB (idle node)

**Growth factors:**
- Peer connections: ~5 MB per active peer
- Gossip topics: ~10 MB per topic (with typical entry volume)
- Ledger cache: ~50 MB (configurable)
- Trust graph cache: ~20 MB (configurable)

**Recommendations:**
- Small community: 512 MB RAM minimum
- Medium community: 1-2 GB RAM
- Large community: 2-4 GB RAM

### Network Bandwidth

**Typical usage:**
- Small community: 1-5 Mbps peak
- Medium community: 5-20 Mbps peak
- Large community: 20-50+ Mbps peak

**Bandwidth spikes:**
- New node sync: Can use 50+ Mbps temporarily
- Anti-entropy: Periodic bursts (every 5-10 minutes)
- Gossip storms: Can spike to 100+ Mbps (should be rare with rate limiting)

### When to Scale

**Indicators:**
- Disk usage > 80%
- Memory usage consistently > 80%
- CPU usage consistently > 70%
- Network bandwidth consistently > 70% of available
- Peer connection count approaching configured limit

**Scaling options:**
1. **Vertical scaling**: Add more resources to existing node
2. **Horizontal scaling**: Split community into multiple federated nodes (requires Phase 16+)
3. **Optimize**: Tune configuration parameters (see [Performance Tuning](#performance-tuning))

---

## Performance Tuning

### Configuration Options

**Network tuning (`~/.icn/config.toml`):**
```toml
[network]
max_peers = 100              # Limit concurrent connections
dial_timeout_secs = 30       # Timeout for new connections
send_timeout_secs = 10       # Timeout for sending messages
max_concurrent_streams = 10  # QUIC streams per connection
stream_window_bytes = 1048576 # 1MB per stream
```

**Gossip tuning:**
```toml
[gossip]
announce_interval_secs = 60    # How often to announce new entries
anti_entropy_interval_secs = 300  # How often to sync with peers
max_entries_per_topic = 1000   # Entry limit per topic (bounded growth)
bloom_filter_capacity = 10000  # Bloom filter size (affects false positive rate)
```

**Ledger tuning:**
```toml
[ledger]
quarantine_warning_threshold = 100   # Warn at this size
quarantine_critical_threshold = 1000 # Critical at this size
cache_size_mb = 50  # In-memory ledger cache
```

**Trust graph tuning:**
```toml
[trust]
cache_size = 1000  # Number of cached trust computations
cache_ttl_secs = 300  # Cache entry lifetime
```

### Rate Limiting

**Trust-based rate limits are automatic:**
- **Isolated peers** (trust < 0.1): 10 msg/sec, burst 2
- **Known peers** (trust 0.1-0.4): 50 msg/sec, burst 10
- **Partner peers** (trust 0.4-0.7): 100 msg/sec, burst 20
- **Federated peers** (trust 0.7+): 200 msg/sec, burst 50

**Override default limits (if needed):**
```toml
[network.rate_limiting]
default_limit_per_sec = 100
default_burst_size = 20
```

### Optimizing for Different Use Cases

**Low-resource environments (Raspberry Pi, old hardware):**
```toml
[network]
max_peers = 20
[gossip]
max_entries_per_topic = 500
[ledger]
cache_size_mb = 20
```

**High-throughput environments (large communities):**
```toml
[network]
max_peers = 200
max_concurrent_streams = 20
[gossip]
max_entries_per_topic = 5000
bloom_filter_capacity = 50000
[ledger]
cache_size_mb = 200
```

**Bandwidth-constrained environments:**
```toml
[gossip]
announce_interval_secs = 120  # Reduce announcement frequency
compression_threshold_bytes = 512  # Compress smaller entries
```

---

## Operational Command Reference

### Identity Management

```bash
# Show current identity
icnctl id show

# Initialize new identity
icnctl id init

# Rotate key (scheduled rotation)
icnctl id rotate

# Export identity (encrypted backup)
icnctl id export ~/identity-backup.age

# Import identity
icnctl id import ~/identity-backup.age
```

### Device Management

```bash
# List all devices
icnctl device list

# Add new device (from new device)
icnctl device add --name "laptop"

# Revoke device (from authorized device)
icnctl device revoke <device-id>

# Show device details
icnctl device show <device-id>
```

### Node Operations

```bash
# Show node status
icnctl status

# Restart node gracefully
sudo systemctl restart icnd

# Stop node
sudo systemctl stop icnd

# Start node
sudo systemctl start icnd

# View logs
journalctl -u icnd -f

# View logs from last hour
journalctl -u icnd --since "1 hour ago"
```

### Graceful Restart & State Persistence

ICN nodes automatically preserve critical runtime state across restarts, enabling zero-downtime maintenance and upgrades.

**What's Preserved:**
- **Vector Clocks**: Causal ordering state prevents duplicate message processing
- **Topic Subscriptions**: No need to re-subscribe after restart
- **Topic Metadata**: Topic names, access control policies (Public/Private/TrustGated/Participants)
- **Peer X25519 Keys**: Immediate end-to-end encrypted communication after restart

**What's NOT Preserved (by design):**
- Gossip entries (fetched from peers via anti-entropy within seconds)
- Active network connections (re-established via mDNS within ~5s)
- Connection statistics (acceptable reset)

**Snapshot Location:**
```bash
# Default location
~/.icn/state.snapshot

# Custom location (via config)
<data-dir>/state.snapshot
```

**Restart Best Practices:**

```bash
# 1. Check node health before restart
icnctl status
curl http://localhost:8080/health | jq

# 2. Check recent metrics (optional)
curl -s http://localhost:9100/metrics | grep icn_snapshot

# 3. Perform graceful restart
sudo systemctl restart icnd

# 4. Verify state restoration in logs
journalctl -u icnd --since "1 minute ago" | grep -E "(snapshot|restored)"
# Look for: "State snapshot saved" and "Gossip/Network state restored"

# 5. Verify health after restart
icnctl status
curl http://localhost:8080/health | jq

# 6. Check that subscriptions were restored
icnctl gossip topics
```

**Monitoring State Snapshots:**

Prometheus metrics for operational visibility:
```bash
# Snapshot operation timing
curl -s http://localhost:9100/metrics | grep icn_snapshot_save_duration
curl -s http://localhost:9100/metrics | grep icn_snapshot_load_duration

# Snapshot contents
curl -s http://localhost:9100/metrics | grep icn_snapshot_gossip_vector_clock_entries
curl -s http://localhost:9100/metrics | grep icn_snapshot_gossip_subscriptions
curl -s http://localhost:9100/metrics | grep icn_snapshot_network_x25519_keys

# Snapshot file size
curl -s http://localhost:9100/metrics | grep icn_snapshot_size_bytes

# Operation counters
curl -s http://localhost:9100/metrics | grep icn_snapshot_save_total
curl -s http://localhost:9100/metrics | grep icn_snapshot_load_total
curl -s http://localhost:9100/metrics | grep icn_snapshot_errors_total
```

**Snapshot Alerts (Recommended):**

Add these Prometheus alerts to detect snapshot issues:
```yaml
# Alert if snapshot save fails
- alert: SnapshotSaveFailed
  expr: rate(icn_snapshot_save_errors_total[5m]) > 0
  annotations:
    summary: "Snapshot save operations failing"
    description: "Node {{ $labels.instance }} failed to save state snapshot"

# Alert if snapshot is very large (possible issue)
- alert: SnapshotTooLarge
  expr: icn_snapshot_size_bytes > 10485760  # 10MB
  annotations:
    summary: "State snapshot unusually large"
    description: "Snapshot size {{ $value }} bytes on {{ $labels.instance }}"

# Alert if snapshot save is slow
- alert: SnapshotSaveSlow
  expr: histogram_quantile(0.99, rate(icn_snapshot_save_duration_seconds_bucket[5m])) > 1.0
  annotations:
    summary: "Snapshot saves taking over 1 second"
    description: "P99 snapshot save time: {{ $value }}s"
```

**Troubleshooting:**

```bash
# Snapshot not being created
# 1. Check disk space
df -h ~/.icn
# 2. Check permissions
ls -la ~/.icn/state.snapshot
# 3. Check logs for errors
journalctl -u icnd | grep -i "snapshot.*error"

# Snapshot not being restored
# 1. Check if snapshot exists
ls -lh ~/.icn/state.snapshot
# 2. Verify JSON format
jq . ~/.icn/state.snapshot
# 3. Check for corruption
cat ~/.icn/state.snapshot | jq '.version'

# State not preserved after restart
# 1. Verify snapshot was created before shutdown
journalctl -u icnd | grep "State snapshot saved"
# 2. Verify snapshot was loaded on startup
journalctl -u icnd | grep "State snapshot restored"
# 3. Check snapshot contents
jq '.gossip_state.vector_clock' ~/.icn/state.snapshot
```

**Security Considerations:**

The state snapshot contains **public information only**:
- DIDs (public identifiers)
- Vector clock counters
- Topic names and access control policies
- X25519 public keys (not private keys)

**No sensitive data** is persisted:
- Private keys remain in encrypted keystore
- Passphrases never written to disk
- Message content not persisted

**File permissions** use OS defaults (typically 644). For defense-in-depth:
```bash
# Tighten permissions (optional)
chmod 600 ~/.icn/state.snapshot
```

### Backup & Restore

```bash
# Create backup (includes state.snapshot automatically)
icnctl backup <output-path>

# Restore from backup (includes state.snapshot)
icnctl restore <backup-path>

# Verify backup integrity
icnctl backup verify <backup-path>

# Verify state.snapshot is included
tar -tf backup.tar | grep state.snapshot
```

**Notes:**
- Backups automatically include `state.snapshot` for full state restoration
- When you restore from backup, both your identity and runtime state (vector clocks, subscriptions, X25519 keys) are restored together
- **Security**: Backup tarballs are **not encrypted**, but the keystore inside (`identity.age`) is Age-encrypted with your passphrase
- **Storage**: Store backups securely with appropriate file permissions (recommended: `chmod 600 backup.tar`)

### Network Diagnostics

```bash
# Show peer connections
icnctl peers list

# Connect to specific peer
icnctl peers connect <did> <address>

# Disconnect from peer
icnctl peers disconnect <did>

# Show network stats
icnctl network stats
```

### Gossip Operations

```bash
# List subscribed topics
icnctl gossip topics

# Subscribe to topic
icnctl gossip subscribe <topic-name>

# Unsubscribe from topic
icnctl gossip unsubscribe <topic-name>

# Show entries in topic
icnctl gossip entries <topic-name>
```

### Ledger Operations

```bash
# Show account balance
icnctl ledger balance <did>

# Show account history
icnctl ledger history <did>

# Create transaction
icnctl ledger transfer --from <did> --to <did> --amount <amount> --memo "<description>"

# Show quarantine entries
icnctl ledger quarantine list

# Resolve quarantine entry (manual review)
icnctl ledger quarantine resolve <entry-id> --action <keep|discard>
```

### Metrics & Monitoring

```bash
# Query Prometheus metrics
curl http://localhost:9100/metrics

# Query specific metric
curl -s http://localhost:9100/metrics | grep icn_network_connections_active

# Check health status
curl http://localhost:8080/health | jq

# Open monitoring dashboard
xdg-open http://localhost:8080/
```

---

## Troubleshooting Workflows

### Node Won't Start

**Symptoms:**
- `systemctl status icnd` shows failed state
- Logs show errors during startup

**Diagnosis:**
```bash
# Check detailed error
journalctl -u icnd -n 50

# Common issues:
# 1. Port already in use
sudo netstat -tulpn | grep 4433

# 2. Keystore file missing or corrupted
ls -la ~/.icn/keystore.age

# 3. Permissions issue
ls -la ~/.icn/
```

**Solutions:**
1. **Port conflict**: Change port in `~/.icn/config.toml` or kill conflicting process
2. **Missing keystore**: Restore from backup or run `icnctl id init`
3. **Permissions**: Fix with `chown -R icn:icn ~/.icn/`

### No Peer Connections

**Symptoms:**
- `icn_network_connections_active` metric is 0
- Dashboard shows "Network Peers: 0"

**Diagnosis:**
```bash
# Check network configuration
icnctl status

# Check firewall
sudo iptables -L -n | grep 4433

# Check if port is accessible
sudo netstat -tulpn | grep icnd
```

**Solutions:**
1. **mDNS not working**: Manually dial peers with `icnctl peers connect <did> <address>`
2. **Firewall blocking**: Open UDP port 4433: `sudo ufw allow 4433/udp`
3. **Wrong network interface**: Check `bind_addr` in config
4. **TLS certificate issue**: Check logs for certificate verification errors

### High Quarantine Size

**Symptoms:**
- `icn_ledger_quarantine_size` > 100
- Dashboard shows "Degraded" status

**Diagnosis:**
```bash
# List quarantine entries
icnctl ledger quarantine list

# Check for patterns (repeated accounts, timestamps, etc.)
icnctl ledger quarantine list --format json | jq '.[] | .account' | sort | uniq -c
```

**Solutions:**
1. **Concurrent transaction conflicts**: Normal in high-activity periods, will resolve automatically
2. **Clock skew**: Check system time with `timedatectl`, sync with NTP
3. **Network partition**: Check connectivity to peers
4. **Attack/malicious entries**: Investigate and potentially block peer

**See also:** [Incident Response: Quarantine Growth](incident-response.md#quarantine-growth-p2)

### High Memory Usage

**Symptoms:**
- ICNd consuming > 2 GB RAM
- OOM killer terminating ICNd

**Diagnosis:**
```bash
# Check process memory
ps aux | grep icnd

# Check metrics for growth indicators
curl -s http://localhost:9100/metrics | grep -E "(gossip_entries_total|ledger_accounts_total|trust_edges_total)"
```

**Solutions:**
1. **Gossip store growth**: Reduce `max_entries_per_topic` in config
2. **Ledger cache**: Reduce `cache_size_mb` in config
3. **Too many peers**: Reduce `max_peers` in config
4. **Memory leak**: Check for version with known leaks, upgrade if needed

### Slow Transaction Processing

**Symptoms:**
- Transactions taking > 10 seconds to confirm
- High `icn_ledger_merge_conflicts_total`

**Diagnosis:**
```bash
# Check ledger metrics
curl -s http://localhost:9100/metrics | grep icn_ledger

# Check gossip sync status
icnctl gossip stats
```

**Solutions:**
1. **Network latency**: Check peer connectivity and latency
2. **High conflict rate**: Reduce concurrent transactions from same account
3. **Quarantine backlog**: Resolve quarantine entries (see above)
4. **Disk I/O bottleneck**: Check disk performance with `iostat`

---

## Additional Resources

- **Deployment Guide**: [deployment-guide.md](deployment-guide.md)
- **Incident Response**: [incident-response.md](incident-response.md)
- **Architecture**: [ARCHITECTURE.md](../ARCHITECTURE.md)
- **Changelog**: [CHANGELOG.md](../CHANGELOG.md)
- **Roadmap**: [ROADMAP.md](../ROADMAP.md)

---

**Document Version**: 1.0
**Last Updated**: 2025-01-14
**Applies To**: ICN v0.1.x
