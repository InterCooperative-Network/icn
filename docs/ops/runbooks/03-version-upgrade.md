# Version Upgrade Procedure

## Summary

Procedure for upgrading ICN daemon to a new version with minimal downtime.

**Use when**:
- New ICN version released
- Security patch needs deployment
- Bug fix required

## Prerequisites

- [ ] Backup created and verified (`icnctl backup` + `icnctl verify-backup`)
- [ ] New version tested in staging/dev
- [ ] Changelog reviewed for breaking changes
- [ ] Rollback plan ready

## Pre-Upgrade Checklist

### 1. Review Release Notes

```bash
# Check release notes on GitHub
gh release view v0.X.Y --repo InterCooperative-Network/icn

# Or check CHANGELOG
cat CHANGELOG.md | head -100
```

### 2. Create Backup

```bash
# Create backup before upgrade
icnctl --data-dir ~/.icn backup ~/icn-backup-pre-upgrade-$(date +%Y%m%d).tar

# Verify backup
icnctl verify-backup ~/icn-backup-pre-upgrade-*.tar
```

### 3. Check Current Version

```bash
icnctl --version
# Or
icnd --version
```

## Upgrade Procedure

### Kubernetes Deployment

#### Option A: Rolling Update (Recommended)

```bash
# Update image tag in deployment
kubectl -n icn set image deployment/icn-daemon icn-daemon=icn/icnd:v0.X.Y

# Watch rollout
kubectl -n icn rollout status deployment/icn-daemon --timeout=600s

# If issues, immediate rollback
kubectl -n icn rollout undo deployment/icn-daemon
```

#### Option B: Full Redeploy

```bash
cd deploy/k8s

# Update version in kustomization.yaml or values
# Then deploy
make full-deploy-dev

# Monitor
make logs
```

### Systemd Deployment

#### 1. Download New Binary

```bash
# Download from release
VERSION="v0.X.Y"
curl -LO "https://github.com/InterCooperative-Network/icn/releases/download/${VERSION}/icnd-linux-amd64"
curl -LO "https://github.com/InterCooperative-Network/icn/releases/download/${VERSION}/icnctl-linux-amd64"

# Verify checksum (if provided)
sha256sum icnd-linux-amd64
```

#### 2. Stop Service

```bash
systemctl stop icnd
```

#### 3. Backup Current Binary

```bash
cp /usr/local/bin/icnd /usr/local/bin/icnd.backup
cp /usr/local/bin/icnctl /usr/local/bin/icnctl.backup
```

#### 4. Install New Binary

```bash
chmod +x icnd-linux-amd64 icnctl-linux-amd64
mv icnd-linux-amd64 /usr/local/bin/icnd
mv icnctl-linux-amd64 /usr/local/bin/icnctl
```

#### 5. Start Service

```bash
systemctl start icnd
journalctl -u icnd -f
```

### Build from Source

```bash
cd /home/matt/projects/icn/icn
git fetch origin
git checkout v0.X.Y  # or main for latest

cargo build --release -p icnd -p icnctl

# Then follow systemd procedure above with:
# ./target/release/icnd and ./target/release/icnctl
```

## Post-Upgrade Verification

### 1. Version Check

```bash
icnctl --version
# Should show new version
```

### 2. Node Health

```bash
# Check supervisor state
curl -s http://localhost:9100/metrics | grep icn_supervisor_state
# Should be 2 (running)

# Check identity
icnctl id show
```

### 3. Connectivity

```bash
# Check gossip peers
curl -s http://localhost:9100/metrics | grep icn_gossip_peers

# Check network connections
curl -s http://localhost:9100/metrics | grep icn_net_connections
```

### 4. Functionality Smoke Test

```bash
# Check RPC responding
icnctl status

# Check ledger accessible
icnctl ledger balance

# Check trust graph
icnctl trust list
```

## Rollback Procedure

If upgrade causes problems:

### Kubernetes

```bash
# Immediate rollback
kubectl -n icn rollout undo deployment/icn-daemon

# Verify rollback
kubectl -n icn rollout status deployment/icn-daemon
```

### Systemd

```bash
# Stop service
systemctl stop icnd

# Restore old binary
cp /usr/local/bin/icnd.backup /usr/local/bin/icnd
cp /usr/local/bin/icnctl.backup /usr/local/bin/icnctl

# Restart
systemctl start icnd
```

### Data Rollback (If Needed)

If new version corrupted data:

```bash
# Stop node
systemctl stop icnd

# Restore from pre-upgrade backup
icnctl --data-dir ~/.icn restore ~/icn-backup-pre-upgrade-*.tar --force

# Restart with old version
systemctl start icnd
```

## Version-Specific Notes

### Breaking Changes Checklist

When upgrading, check for:

- [ ] Config file format changes
- [ ] API endpoint changes
- [ ] Database schema migrations
- [ ] Protocol version changes (may need coordinated upgrade)

### Database Migrations

ICN handles migrations automatically on startup. If migration fails:

1. Check logs for migration errors
2. Rollback to previous version
3. Report issue with migration logs

## Related

- [Emergency Restart](./01-emergency-restart.md) - If upgrade causes crash loop
- [Data Recovery](./02-data-recovery.md) - If upgrade corrupts data
- [Troubleshooting](./05-troubleshooting.md) - Common upgrade issues
