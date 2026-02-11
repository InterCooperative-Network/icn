# ICN Deployment & Operations Guide

A practical guide for deploying and operating ICN nodes in production environments.

## Table of Contents

1. [Quick Start](#quick-start)
2. [System Requirements](#system-requirements)
3. [Installation](#installation)
4. [Configuration](#configuration)
5. [Running as a Service](#running-as-a-service)
6. [Monitoring](#monitoring)
7. [Backup & Recovery](#backup--recovery)
8. [Troubleshooting](#troubleshooting)
9. [Security Best Practices](#security-best-practices)

---

## Quick Start

For testing on a local machine:

```bash
# Build release binaries
cd icn/
cargo build --release

# Initialize identity
./target/release/icnctl id init
# Enter passphrase when prompted

# Start daemon
./target/release/icnd
```

The daemon will:
1. Load identity from `~/.icn/keystore.age`
2. Start QUIC listener on `0.0.0.0:4433` (default)
3. Enable mDNS discovery on LAN
4. Expose Prometheus metrics on `:9090/metrics`

---

## System Requirements

### Minimum (Development/Testing)
- **CPU:** 2 cores
- **RAM:** 512 MB
- **Disk:** 1 GB
- **Network:** 1 Mbps, low-latency LAN

### Recommended (Production)
- **CPU:** 4+ cores (for signature verification)
- **RAM:** 2 GB (for gossip buffers, connection pools)
- **Disk:** 10 GB SSD (for ledger, fast I/O)
- **Network:** 10+ Mbps, public IPv4/IPv6

### Operating System
- **Linux**: Ubuntu 22.04+, Debian 12+, Fedora 38+ (primary)
- **macOS**: 12+ (development)
- **Windows**: WSL2 (development only)

---

## Installation

### From Source

```bash
# Clone repository
git clone https://github.com/your-org/icn.git
cd icn/icn

# Build release binaries
cargo build --release

# Install to /usr/local/bin (optional)
sudo cp target/release/{icnd,icnctl} /usr/local/bin/
```

### Using Docker

```dockerfile
# Dockerfile
FROM rust:1.75 AS builder
WORKDIR /app
COPY icn/ ./
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/icnd /usr/local/bin/
COPY --from=builder /app/target/release/icnctl /usr/local/bin/

# Data directory
VOLUME ["/data"]
ENV ICN_DATA_DIR=/data

EXPOSE 4433/udp 9090/tcp
ENTRYPOINT ["icnd"]
```

Build and run:
```bash
docker build -t icnd:latest .
docker run -d \
  --name icnd \
  -v icn-data:/data \
  -p 4433:4433/udp \
  -p 9090:9090 \
  icnd:latest
```

---

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ICN_DATA_DIR` | `~/.icn` | Data directory (keystore, ledger, state) |
| `ICN_LISTEN_ADDR` | `0.0.0.0:4433` | QUIC listener address |
| `ICN_METRICS_PORT` | `9090` | Prometheus metrics port |
| `ICN_LOG_LEVEL` | `info` | Log level (trace, debug, info, warn, error) |
| `ICN_KEYSTORE_PASSPHRASE` | (none) | Keystore passphrase for automated deployments (preferred) |
| `ICN_PASSPHRASE` | (none) | Keystore passphrase (legacy, use ICN_KEYSTORE_PASSPHRASE) |
| `ICN_GATEWAY_JWT_SECRET` | (none) | JWT secret for Gateway API authentication |

**Passphrase for Automated Deployments:**

For systemd, Docker, or Kubernetes deployments where interactive prompts aren't available:

```bash
# Option 1: Set in environment (preferred)
export ICN_KEYSTORE_PASSPHRASE="your-secure-passphrase"
icnd --config /etc/icn/icn.toml

# Option 2: Use systemd EnvironmentFile
# /etc/icn/icn.env (chmod 600)
ICN_KEYSTORE_PASSPHRASE=your-secure-passphrase

# Option 3: Docker/K8s secrets
docker run -e ICN_KEYSTORE_PASSPHRASE="$(cat /run/secrets/icn_passphrase)" icn:latest
```

**Security Warning:** Store passphrases in secrets management systems (HashiCorp Vault, K8s Secrets, AWS Secrets Manager), not plain text files.

### Configuration File

ICN supports TOML configuration files. See [config/](../../../config/README.md) for examples:

```toml
# ~/.icn/icn.toml
data_dir = "/home/user/.icn"

[network]
listen_addr = "0.0.0.0:4433"
mdns_enabled = true
bootstrap_peers = []

[observability]
metrics_port = 9090
log_level = "info"

[security]
rate_limit_msg_per_sec = 100
rate_limit_burst = 20
quic_max_streams = 10

[observability]
metrics_port = 9090
log_level = "info"
log_format = "json"

[storage]
data_dir = "/var/lib/icn"
```

### CLI Flags

```bash
icnd --help

# Common flags:
icnd --config /etc/icn/icn.toml
icnd --data-dir /var/lib/icn
icnd --log-level debug
```

---

## Running as a Service

### systemd (Linux)

Create `/etc/systemd/system/icnd.service`:

```ini
[Unit]
Description=ICN Daemon
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=icn
Group=icn
ExecStart=/usr/local/bin/icnd
Restart=on-failure
RestartSec=5s

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/icn

# Environment
Environment="ICN_DATA_DIR=/var/lib/icn"
Environment="ICN_LOG_LEVEL=info"
Environment="RUST_LOG=icn=info"

# Limits
LimitNOFILE=65536
LimitNPROC=512

[Install]
WantedBy=multi-user.target
```

Setup and start:
```bash
# Create user
sudo useradd -r -s /bin/false icn
sudo mkdir -p /var/lib/icn
sudo chown icn:icn /var/lib/icn

# Initialize identity (as icn user)
sudo -u icn /usr/local/bin/icnctl --data-dir /var/lib/icn id init

# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable icnd
sudo systemctl start icnd

# Check status
sudo systemctl status icnd
sudo journalctl -u icnd -f
```

### Docker Compose

See [docker/](../../../docker/README.md) for complete Docker Compose examples.

```yaml
version: '3.9'

services:
  icnd:
    image: icn:latest
    container_name: icn-alpha
    restart: unless-stopped
    ports:
      - "4433:4433/udp"
      - "5050:5050"
      - "9090:9090"
    volumes:
      - icn-data:/data
    environment:
      - ICN_DATA_DIR=/data
      - ICN_OBSERVABILITY_LOG_LEVEL=info
    networks:
      - icn-net

  prometheus:
    image: prom/prometheus:latest
    container_name: prometheus
    restart: unless-stopped
    ports:
      - "9091:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus-data:/prometheus
    networks:
      - icn-net

volumes:
  icn-data:
  prometheus-data:

networks:
  icn-net:
```

---

## Monitoring

### Prometheus Configuration

`prometheus.yml`:
```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'icn'
    static_configs:
      - targets: ['icnd:9090']
```

### Key Metrics

**Network Health:**
```promql
# Connection rate
rate(icn_network_connections_total[5m])

# Active connections
icn_network_connections_active

# Rate limiting events (potential attack)
rate(icn_network_messages_rate_limited_total[5m]) > 10
```

**Gossip Protocol:**
```promql
# Message throughput
rate(icn_gossip_announces_received_total[5m])
rate(icn_gossip_requests_received_total[5m])

# Total gossip entries
icn_gossip_entries_total

# Subscriptions
icn_gossip_subscriptions_total
```

**Ledger:**
```promql
# Transaction rate
rate(icn_ledger_transactions_total[5m])

# Account count
icn_ledger_accounts_total

# Currencies in use
icn_ledger_currencies_total
```

### Grafana Dashboard (Example)

Import this JSON or create panels manually:

```json
{
  "panels": [
    {
      "title": "Message Rate",
      "targets": [{
        "expr": "rate(icn_network_messages_received_total[5m])"
      }]
    },
    {
      "title": "Active Connections",
      "targets": [{
        "expr": "icn_network_connections_active"
      }]
    },
    {
      "title": "Rate Limited (Security Alert)",
      "targets": [{
        "expr": "rate(icn_network_messages_rate_limited_total[5m])"
      }],
      "alert": { "condition": "> 10" }
    }
  ]
}
```

### Log Monitoring

```bash
# Follow logs (systemd)
journalctl -u icnd -f

# Search for errors
journalctl -u icnd --priority=err

# Security events
journalctl -u icnd | grep -E "Rate limited|Certificate verification|SECURITY"

# Export logs (structured JSON)
journalctl -u icnd -o json > icnd-logs.json
```

---

## Backup & Recovery

### Identity Backup

**CRITICAL**: The keystore contains your node's cryptographic identity. Loss = permanent identity loss.

```bash
# Backup keystore (encrypted with passphrase)
cp ~/.icn/keystore.age ~/backup/keystore-$(date +%F).age

# Or use icnctl
icnctl id export ~/backup/identity-backup.age

# Store backup in multiple locations:
# - Encrypted USB drive
# - Password manager (as attachment)
# - Offline storage (paper wallet for recovery phrase - future)
```

**Recovery:**
```bash
# Restore from backup
icnctl id import ~/backup/identity-backup.age

# Or copy directly
cp ~/backup/keystore.age ~/.icn/keystore.age
chmod 600 ~/.icn/keystore.age
```

### Ledger Backup

The ledger is a Merkle-DAG synced via gossip, so data is distributed. However, local backups prevent data loss:

```bash
# Backup entire data directory
tar czf icn-backup-$(date +%F).tar.gz ~/.icn/

# Exclude keystore if backing up separately
tar czf icn-ledger-backup-$(date +%F).tar.gz \
  --exclude='keystore.age' \
  ~/.icn/
```

**Automated backups (systemd timer):**

`/etc/systemd/system/icn-backup.service`:
```ini
[Unit]
Description=ICN Backup

[Service]
Type=oneshot
User=icn
ExecStart=/usr/local/bin/icn-backup.sh
```

`/etc/systemd/system/icn-backup.timer`:
```ini
[Unit]
Description=ICN Backup Timer

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

Enable: `sudo systemctl enable --now icn-backup.timer`

---

## Troubleshooting

### Node won't start

**Symptom:** `icnd` exits immediately

**Checklist:**
1. Identity initialized? Run `icnctl id init`
2. Port already in use? Check with `sudo lsof -i :4433`
3. Permissions? Ensure data directory is readable/writable
4. Logs? Check `journalctl -u icnd -n 50`

**Common errors:**
```
Error: Failed to unlock keystore
→ Passphrase incorrect or keystore corrupted

Error: Address already in use
→ Another instance running or port conflict

Error: Permission denied
→ User lacks write access to data directory
```

### No peers discovered

**Symptom:** `icnctl status` shows 0 peers

**Checklist:**
1. mDNS working? Check with `avahi-browse -a` (Linux) or `dns-sd -B` (macOS)
2. Firewall blocking? Open UDP 4433
3. Multiple networks? mDNS doesn't cross subnets
4. Docker network mode? Use `host` mode for mDNS

**Manual peer dialing:**
```bash
icnctl network dial did:icn:abc123... 192.168.1.100:4433
```

### High memory usage

**Symptom:** `icnd` using >2GB RAM

**Potential causes:**
1. **Too many gossip entries**: Check `icn_gossip_entries_total` metric
2. **Connection leak**: Check `icn_network_connections_active`
3. **Message flood**: Check `icn_network_messages_rate_limited_total`

**Actions:**
1. Review Prometheus metrics for anomalies
2. Check for misbehaving peers in logs
3. Restart daemon (graceful shutdown preserves state)

### Certificate verification errors

**Symptom:** Connections failing with TLS errors

**Checklist:**
1. System clock correct? TLS checks certificate validity
2. Certificate expired? Rotate with `icnctl id rotate`
3. DID format invalid? Check logs for format errors

**Note:** Trust graph integration pending - currently accepts all valid DIDs.

---

## Security Best Practices

### 1. Secure Keystore

**DO:**
- Use strong passphrase (20+ characters, mix of types)
- Store backup in encrypted location (KeePass, 1Password)
- Use separate passphrase from system password
- Restrict permissions: `chmod 600 ~/.icn/keystore.age`

**DON'T:**
- Share keystore file (contains identity)
- Store passphrase in plain text
- Use weak/common passphrases
- Commit keystore to version control

### 2. Network Exposure

**DO:**
- Use firewall (ufw, iptables) to limit exposure
- Enable rate limiting (on by default)
- Monitor metrics for anomalies
- Use VPN or WireGuard for WAN peers

**DON'T:**
- Expose metrics port (9090) publicly
- Disable rate limiting without good reason
- Run as root (use dedicated user)

### 3. Updates

**DO:**
- Subscribe to security announcements (GitHub watch)
- Test updates on staging before production
- Review changelogs for breaking changes
- Use versioned releases (not main branch)

**DON'T:**
- Auto-update in production without testing
- Skip patch releases (may contain security fixes)
- Run multiple versions in same network (protocol mismatch)

### 4. Monitoring

**DO:**
- Set up alerts for rate limiting spikes
- Monitor connection count and patterns
- Review logs regularly for suspicious activity
- Track trust graph changes

**DON'T:**
- Ignore rate limiting metrics
- Run without monitoring in production
- Expose Prometheus publicly without auth

---

## Additional Resources

- [Architecture Documentation](../../ARCHITECTURE.md)
- [Production Hardening Details](../../security/production-hardening.md)
- [Topic Subscriptions API](../../reference/api/topic-subscriptions-api.md)
- [GitHub Repository](https://github.com/your-org/icn)
- [Community Forum](https://forum.intercooperative.network) (future)

---

## Support

For issues:
1. Check this guide and architecture docs
2. Search GitHub issues
3. Open new issue with logs and system info
4. Security issues: security@intercooperative.network (future)

**Include in bug reports:**
- ICN version (`icnd --version`)
- OS and architecture
- Relevant logs (`journalctl -u icnd -n 100`)
- Metrics snapshots (if available)
