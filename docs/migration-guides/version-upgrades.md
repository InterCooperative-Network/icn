# ICN Version Upgrade Guide

This guide covers upgrading ICN between major, minor, and patch versions.

## Versioning Scheme

ICN follows [Semantic Versioning](https://semver.org/):
- **MAJOR.MINOR.PATCH** (e.g., 0.2.1)
- **MAJOR**: Breaking changes (incompatible API changes)
- **MINOR**: New features (backward-compatible)
- **PATCH**: Bug fixes (backward-compatible)

---

## Pre-Upgrade Checklist

Before upgrading **any** version:

- [ ] **Backup your data directory**:
  ```bash
  icnctl backup ~/icn-backup-$(date +%Y%m%d).tar.gz.age
  ```

- [ ] **Read the CHANGELOG**: Check [CHANGELOG.md](../../CHANGELOG.md) for breaking changes

- [ ] **Check system requirements**: Ensure your system meets new requirements

- [ ] **Plan maintenance window**: Schedule downtime for node restart

- [ ] **Notify cooperative members**: Let them know about upcoming downtime

- [ ] **Test on non-production node** (if you have one)

---

## Upgrade Methods

### Method 1: Package Manager (Recommended)

**If you installed via the installer script:**

```bash
# Download and run upgrade script
curl -fsSL https://raw.githubusercontent.com/InterCooperative-Network/icn/main/scripts/upgrade.sh | bash
```

This will:
1. Download latest binaries
2. Stop icnd gracefully
3. Replace binaries
4. Restart icnd
5. Verify successful start

### Method 2: Build from Source

**For development or custom builds:**

```bash
# 1. Backup
icnctl backup ~/icn-backup.tar.gz.age

# 2. Stop daemon
sudo systemctl stop icnd

# 3. Pull latest code
cd /path/to/icn
git fetch origin
git checkout v0.2.0  # Replace with desired version

# 4. Build
cd icn
cargo build --release

# 5. Install binaries
sudo cp target/release/{icnd,icnctl} /usr/local/bin/

# 6. Start daemon
sudo systemctl start icnd

# 7. Verify
icnctl --version
icnctl status
```

### Method 3: Docker

```bash
# Pull latest image
docker pull icn/icnd:latest

# Restart container (preserves volume data)
docker-compose down
docker-compose up -d

# Verify
docker exec icn-node icnctl status
```

---

## Version-Specific Upgrade Notes

### v0.1.x → v0.2.x (Hypothetical)

**Breaking Changes:**
- Protocol version bump (requires all nodes to upgrade)
- RPC API changes (update client applications)

**Migration Steps:**
1. Backup data directory
2. Stop icnd
3. Upgrade binaries
4. Run migration tool (if provided):
   ```bash
   icnctl migrate v0.2
   ```
5. Start icnd
6. Verify peers reconnect

**Rollback Plan:**
If upgrade fails:
1. Stop icnd
2. Restore from backup:
   ```bash
   icnctl restore ~/icn-backup.tar.gz.age --force
   ```
3. Downgrade binaries to v0.1.x
4. Restart icnd

---

## Upgrade Scenarios

### Patch Upgrade (e.g., 0.1.0 → 0.1.1)

**Risk**: Low
**Downtime**: ~30 seconds
**Rollback**: Easy

```bash
# 1. Backup (quick)
icnctl backup ~/icn-backup.tar.gz.age

# 2. Upgrade
sudo systemctl stop icnd
sudo cp /path/to/new/icnd /usr/local/bin/icnd
sudo systemctl start icnd

# 3. Verify
icnctl --version
journalctl -u icnd -f
```

**Rollback**:
```bash
sudo systemctl stop icnd
sudo cp /path/to/old/icnd /usr/local/bin/icnd
sudo systemctl start icnd
```

### Minor Upgrade (e.g., 0.1.0 → 0.2.0)

**Risk**: Low-Medium
**Downtime**: ~1-2 minutes
**Rollback**: Moderate (may require data migration)

```bash
# 1. Backup
icnctl backup ~/icn-backup.tar.gz.age

# 2. Review CHANGELOG for breaking changes
cat CHANGELOG.md | grep -A 20 "## [0.2.0]"

# 3. Stop daemon
sudo systemctl stop icnd

# 4. Upgrade binaries
sudo cp /path/to/new/{icnd,icnctl} /usr/local/bin/

# 5. Run migration (if required)
icnctl migrate v0.2

# 6. Start daemon
sudo systemctl start icnd

# 7. Verify
icnctl status
icnctl network peers  # Check peer connectivity
icnctl ledger balance  # Check data integrity
```

### Major Upgrade (e.g., 0.x.x → 1.0.0)

**Risk**: High
**Downtime**: 5-15 minutes
**Rollback**: Difficult (coordinate with network)

**Requires coordination**: All nodes in cooperative must upgrade together.

```bash
# 1. Coordinate with cooperative
# - Set upgrade date/time
# - Ensure all members are ready

# 2. Comprehensive backup
icnctl backup ~/icn-backup-major-upgrade.tar.gz.age
# Copy backup off-site!

# 3. Review upgrade guide
# Check docs/migration-guides/ for version-specific instructions

# 4. Stop daemon
sudo systemctl stop icnd

# 5. Upgrade binaries
sudo cp /path/to/new/{icnd,icnctl} /usr/local/bin/

# 6. Run migration tool
icnctl migrate v1.0 --verify

# 7. Start daemon
sudo systemctl start icnd

# 8. Extensive verification
icnctl status
icnctl network peers
icnctl ledger balance
icnctl trust list
icnctl gov domain list

# 9. Monitor for 24 hours
journalctl -u icnd -f
```

---

## Post-Upgrade Verification

After any upgrade, verify:

### 1. Daemon Status
```bash
icnctl status
# Expected: Daemon running, no errors
```

### 2. Network Connectivity
```bash
icnctl network peers
# Expected: Peers reconnected within 30 seconds
```

### 3. Data Integrity
```bash
icnctl id show           # DID unchanged
icnctl ledger balance    # Balance correct
icnctl trust list        # Trust relationships intact
icnctl gov domain list   # Governance domains present
```

### 4. Logs
```bash
journalctl -u icnd -f
# Expected: No errors, normal operation
```

### 5. Metrics
```bash
curl http://localhost:9090/metrics | grep icn_network_connections_active
# Expected: > 0 connections
```

---

## Rollback Procedures

### Simple Rollback (Patch/Minor)

If new version has issues:

```bash
# 1. Stop new version
sudo systemctl stop icnd

# 2. Restore old binaries
sudo cp ~/icn-old-binaries/icnd /usr/local/bin/icnd

# 3. Restore data (if needed)
icnctl restore ~/icn-backup.tar.gz.age

# 4. Start old version
sudo systemctl start icnd

# 5. Verify
icnctl status
```

### Complex Rollback (Major)

**Requires network coordination**:

1. Stop all nodes
2. Restore from backups taken before upgrade
3. Downgrade binaries on all nodes
4. Restart all nodes simultaneously
5. Verify network converges

**Prevention**: Test major upgrades on staging network first!

---

## Multi-Node Cooperative Upgrades

### Strategy 1: Rolling Upgrade (Minor versions)

Upgrade nodes one at a time:

1. **Upgrade Node 1**:
   - Backup, stop, upgrade, start
   - Verify peers reconnect
   - Wait 5 minutes

2. **Upgrade Node 2**:
   - Same process
   - Wait 5 minutes

3. **Continue** until all nodes upgraded

**Benefit**: Zero downtime for cooperative
**Requirement**: Backward-compatible protocol

### Strategy 2: Coordinated Upgrade (Major versions)

All nodes upgrade simultaneously:

1. **Pre-coordination**:
   - Set upgrade window (e.g., "Saturday 2 AM")
   - All operators ready
   - Backups taken

2. **Upgrade**:
   - Stop all nodes at same time
   - Upgrade binaries
   - Start all nodes at same time

3. **Verification**:
   - Nodes discover each other
   - Gossip converges
   - Ledger syncs

**Benefit**: Clean protocol version bump
**Downside**: Requires downtime

---

## Automated Upgrades

**NOT RECOMMENDED** for production cooperatives.

If you must automate:

```bash
#!/bin/bash
# /usr/local/bin/icn-auto-upgrade.sh

# Backup
icnctl backup /var/backups/icn/backup-$(date +%Y%m%d-%H%M%S).tar.gz.age

# Download latest
wget https://releases.icn.coop/latest/icnd -O /tmp/icnd.new
wget https://releases.icn.coop/latest/icnctl -O /tmp/icnctl.new

# Verify checksums
sha256sum -c /tmp/icn-checksums.txt || exit 1

# Stop daemon
systemctl stop icnd

# Upgrade
cp /tmp/icnd.new /usr/local/bin/icnd
cp /tmp/icnctl.new /usr/local/bin/icnctl
chmod +x /usr/local/bin/{icnd,icnctl}

# Start daemon
systemctl start icnd

# Verify
icnctl status || {
    # Rollback on failure
    systemctl stop icnd
    cp /usr/local/bin/icnd.backup /usr/local/bin/icnd
    systemctl start icnd
}
```

**Better**: Use monitoring and alerts, upgrade manually.

---

## Troubleshooting

### "Version mismatch" errors

**Symptom**: Peers disconnect after upgrade

**Cause**: Incompatible protocol versions

**Solution**:
1. Check all nodes upgraded to same version
2. Verify protocol version: `icnctl network status`
3. Coordinate upgrade if nodes out of sync

### "Migration failed"

**Symptom**: Migration tool exits with error

**Cause**: Corrupted data or incompatible format

**Solution**:
1. Restore from backup
2. Review migration logs: `journalctl -u icnd -b`
3. Report bug with logs if persistent

### "Daemon won't start after upgrade"

**Symptom**: icnd crashes on startup

**Causes**:
- Incompatible data format
- Missing migration step
- Corrupted state

**Solution**:
1. Check logs: `journalctl -u icnd -xe`
2. Try migration tool: `icnctl migrate v0.2`
3. If failing, restore backup and rollback

### "Ledger out of sync"

**Symptom**: Different balance on different nodes

**Cause**: Gossip not converged yet

**Solution**:
1. Wait 5-10 minutes for anti-entropy
2. Force sync: Restart daemon
3. Check gossip metrics: `curl localhost:9090/metrics | grep gossip`

---

## Best Practices

1. **Always backup before upgrading**
2. **Read CHANGELOG before major upgrades**
3. **Test on staging environment if possible**
4. **Schedule upgrades during low-activity periods**
5. **Monitor logs after upgrade for 24 hours**
6. **Keep old binaries** until new version proven stable
7. **Coordinate with cooperative** for major upgrades
8. **Document your upgrade process** for your coop

---

## Emergency Contacts

If upgrade fails and you need help:

1. **GitHub Issues**: https://github.com/InterCooperative-Network/icn/issues
2. **Emergency rollback**: Use backups (see [Backup & Recovery](../backup-and-recovery.md))
3. **Community support**: Check community channels

---

## Related Documentation

- [CHANGELOG.md](../../CHANGELOG.md) - Version history
- [Keystore Migration](keystore-versions.md) - Identity format changes
- [Backup & Recovery](../backup-and-recovery.md) - Backup strategies
- [Operations Guide](../operations-guide.md) - Day-to-day management

---

*Last Updated: 2025-11-21*
