# Troubleshooting Guide

## Summary

Common issues and their solutions for ICN daemon operations.

## Quick Diagnostics

```bash
# One-liner health check
echo "=== ICN Health Check ===" && \
icnctl status 2>&1 | head -5 && \
curl -s http://localhost:9100/metrics | grep -E "icn_supervisor_state|icn_gossip_peers" && \
echo "=== Done ==="
```

## Common Issues

### Node Won't Start

#### Symptom
```
Error: Failed to unlock keystore
```

#### Cause
Incorrect passphrase or corrupted keystore.

#### Solution
```bash
# Verify passphrase
ICN_PASSPHRASE="your-passphrase" icnctl id show

# If keystore corrupted, restore from backup
icnctl restore /path/to/backup.tar --force
```

---

#### Symptom
```
Error: Address already in use (port 7777)
```

#### Cause
Another process using the port, or previous instance didn't shut down cleanly.

#### Solution
```bash
# Find what's using the port
lsof -i :7777
# OR
ss -tulpn | grep 7777

# Kill if it's a zombie icnd
pkill -9 icnd

# Then restart
systemctl start icnd
```

---

#### Symptom
```
Error: Permission denied: ~/.icn/
```

#### Cause
Data directory has wrong permissions.

#### Solution
```bash
# Fix ownership
sudo chown -R $(whoami):$(whoami) ~/.icn

# Fix permissions
chmod 700 ~/.icn
chmod 600 ~/.icn/identity.age
```

---

### No Peers Connecting

#### Symptom
```bash
curl -s http://localhost:9100/metrics | grep icn_gossip_peers
# Returns 0 or no output
```

#### Causes & Solutions

**1. Firewall blocking**
```bash
# Check if port is open
sudo ufw status | grep 7777
# OR
sudo iptables -L -n | grep 7777

# Allow UDP port
sudo ufw allow 7777/udp
```

**2. No bootstrap peers configured**
```bash
# Check config
grep bootstrap ~/.icn/config.toml

# Add bootstrap peer
# [network]
# bootstrap_peers = ["icn://did:icn:...@1.2.3.4:7777"]
```

**3. mDNS not working (LAN only)**
```bash
# Check if mDNS is enabled
grep mdns ~/.icn/config.toml

# mDNS requires same subnet and multicast allowed
# For cross-network, use explicit bootstrap peers
```

---

### High Memory Usage

#### Symptom
```bash
ps aux | grep icnd
# Shows high RSS/VSZ
```

#### Causes & Solutions

**1. Gossip buffer buildup**
```bash
# Check message queue
curl -s http://localhost:9100/metrics | grep icn_gossip_queue

# If very high, peers may be slow - restart may help
systemctl restart icnd
```

**2. Trust cache unbounded**
```bash
# Check cache size
curl -s http://localhost:9100/metrics | grep trust_cache

# Reduce cache in config
# [trust]
# cache_size = 1000
```

**3. Log buffer (if debug logging)**
```bash
# Reduce log level
# [observability]
# log_level = "info"
```

---

### Ledger Sync Issues

#### Symptom
```bash
icnctl ledger balance
# Shows unexpected balance or errors
```

#### Causes & Solutions

**1. Network partition (missed entries)**
```bash
# Check entry count vs peers
curl -s http://localhost:9100/metrics | grep icn_ledger_entries

# Trigger anti-entropy sync
icnctl ledger sync --force
```

**2. Entry rejected**
```bash
# Check quarantine
curl -s http://localhost:9100/metrics | grep quarantine

# Review quarantined entries
icnctl ledger quarantine list
```

**3. Credit limit exceeded**
```bash
# Check limits
icnctl ledger limits show

# Recent transactions may have hit limit
```

---

### Trust Graph Problems

#### Symptom
```bash
icnctl trust score <did>
# Returns unexpected score or error
```

#### Causes & Solutions

**1. Trust edge not synced**
```bash
# Check edge count
curl -s http://localhost:9100/metrics | grep icn_trust_edges

# Force sync
icnctl trust sync
```

**2. Transitive trust computation timeout**
```bash
# For large graphs, computation may be slow
# Check compute time
curl -s http://localhost:9100/metrics | grep trust_compute_duration
```

---

### Governance Proposal Issues

#### Symptom
- Proposal stuck in voting
- Votes not counting
- Quorum never reached

#### Solutions
```bash
# Check proposal status
icnctl gov proposals show <id>

# Check vote counts
icnctl gov votes list --proposal <id>

# Check quorum requirements
icnctl gov config show
```

---

### RPC/API Not Responding

#### Symptom
```bash
icnctl status
# Timeout or connection refused
```

#### Causes & Solutions

**1. RPC port not listening**
```bash
# Check if RPC is bound
ss -tuln | grep 5601

# Check config
grep rpc ~/.icn/config.toml
```

**2. Gateway not enabled**
```bash
# Check gateway status
curl -s http://localhost:8080/health

# Enable in config
# [gateway]
# enabled = true
```

**3. TLS certificate issue**
```bash
# Check cert validity
openssl x509 -in ~/.icn/tls-cert.pem -noout -dates
```

---

## Metrics Reference

| Metric | Healthy Value | Action if Unhealthy |
|--------|---------------|---------------------|
| `icn_supervisor_state` | 2 | Restart node |
| `icn_gossip_peers` | > 0 | Check network/bootstrap |
| `icn_gossip_queue_size` | < 1000 | Check slow peers |
| `icn_ledger_entries_total` | Growing | Check sync |
| `icn_trust_edges_total` | > 0 | Check trust sync |
| `icn_misbehavior_violations` | 0 | Investigate peers |

## Log Analysis

### Find Errors
```bash
journalctl -u icnd | grep -i error | tail -20
```

### Find Warnings
```bash
journalctl -u icnd | grep -i warn | tail -20
```

### Find Specific Component
```bash
journalctl -u icnd | grep -i "gossip\|ledger\|trust" | tail -50
```

### Real-time Monitoring
```bash
journalctl -u icnd -f | grep -E "ERROR|WARN|connected|disconnected"
```

## Getting Help

1. Check this runbook
2. Search GitHub Issues
3. Check `#icn-ops` Slack/Discord
4. File new issue with:
   - ICN version (`icnctl --version`)
   - Error message
   - Relevant logs
   - Steps to reproduce

## Related

- [Emergency Restart](./01-emergency-restart.md)
- [Data Recovery](./02-data-recovery.md)
- [Version Upgrade](./03-version-upgrade.md)
- [Security Incident](./04-security-incident.md)
