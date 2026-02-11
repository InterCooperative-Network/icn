# ICN Operations Runbooks

Production runbooks for ICN daemon operations.

## Runbook Index

| Runbook | When to Use |
|---------|-------------|
| [Emergency Restart](./01-emergency-restart.md) | Node unresponsive, need immediate restart |
| [Data Recovery](./02-data-recovery.md) | Data loss or corruption detected |
| [Version Upgrade](./03-version-upgrade.md) | Deploying new ICN version |
| [Security Incident](./04-security-incident.md) | Suspected security breach |
| [Troubleshooting](./05-troubleshooting.md) | Common issues and fixes |
| [Secrets Rotation](./06-secrets-rotation.md) | Rotating keys, passphrases, certificates |

## Quick Reference

### Check Node Status
```bash
# K8s deployment
kubectl -n icn get pods
kubectl -n icn logs -f deployment/icn-daemon

# Systemd deployment
systemctl status icnd
journalctl -u icnd -f
```

### Emergency Stop
```bash
# K8s
kubectl -n icn scale deployment/icn-daemon --replicas=0

# Systemd
systemctl stop icnd
```

### View Metrics
```bash
# Prometheus endpoint
curl http://localhost:9100/metrics | grep icn_

# Key metrics to check
curl -s http://localhost:9100/metrics | grep -E "icn_gossip_messages|icn_ledger_entries|icn_trust_edges"
```

## Runbook Template

All runbooks follow this structure:

1. **Summary** - What this runbook addresses
2. **Prerequisites** - What you need before starting
3. **Procedure** - Step-by-step instructions
4. **Verification** - How to confirm success
5. **Rollback** - How to undo if needed
6. **Related** - Links to related runbooks

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ICN_KEYSTORE_PASSPHRASE` | Daemon keystore passphrase (preferred for `icnd`) | (optional) |
| `ICN_PASSPHRASE` | CLI passphrase (`icnctl`) and daemon legacy fallback | (optional) |
| `ICN_GATEWAY_JWT_SECRET` | Gateway JWT secret (when gateway enabled) | (required if gateway enabled) |
| `KUBECONFIG` | K8s config (if K8s) | `~/.kube/config` |

Use CLI flags for paths:
- `icnd --data-dir /path --config /path/config.toml`
- `icnctl --data-dir /path`

## Contact

- **On-call**: Check PagerDuty/Slack
- **Escalation**: See incident response runbook
