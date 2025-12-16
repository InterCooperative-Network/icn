# ICN Production Deployment Guide

**Version**: 1.0  
**Last Updated**: 2025-12-16  
**Status**: Production Ready  

---

## Prerequisites

Before deploying ICN to production, ensure you have:

- [ ] Linux server (Ubuntu 22.04+ or Debian 12+ recommended)
- [ ] 4+ CPU cores
- [ ] 8+ GB RAM
- [ ] 100+ GB SSD storage
- [ ] Static IP address or dynamic DNS
- [ ] Firewall access to required ports
- [ ] TLS certificates (Let's Encrypt recommended)
- [ ] Backup storage solution
- [ ] Monitoring infrastructure (Prometheus + Grafana)

---

## Security Hardening Checklist

### System Level

- [ ] **Disable root SSH login**: `PermitRootLogin no` in `/etc/ssh/sshd_config`
- [ ] **Use SSH keys only**: `PasswordAuthentication no`
- [ ] **Configure UFW firewall**:
  ```bash
  ufw default deny incoming
  ufw default allow outgoing
  ufw allow 22/tcp    # SSH
  ufw allow 7777/tcp  # ICN P2P
  ufw allow 8080/tcp  # ICN Health
  ufw allow 8443/tcp  # ICN Gateway (TLS)
  ufw allow 9100/tcp  # Prometheus metrics (restrict to monitoring IP)
  ufw enable
  ```
- [ ] **Enable automatic security updates**:
  ```bash
  apt install unattended-upgrades
  dpkg-reconfigure -plow unattended-upgrades
  ```
- [ ] **Install fail2ban**:
  ```bash
  apt install fail2ban
  systemctl enable fail2ban
  ```
- [ ] **Disable unnecessary services**: Review `systemctl list-units --type=service`
- [ ] **Set up audit logging**: `apt install auditd`

### ICN Daemon Level

- [ ] **Run as non-root user**:
  ```bash
  useradd -r -s /bin/false icn
  chown -R icn:icn /var/lib/icn
  ```
- [ ] **Use strong passphrase for keystore** (20+ characters, random)
- [ ] **Store JWT secret in environment variable** (never in config file):
  ```bash
  export ICN_GATEWAY_JWT_SECRET=$(openssl rand -hex 32)
  ```
- [ ] **Enable TLS for gateway** (see Gateway TLS section below)
- [ ] **Set minimum trust threshold**:
  ```toml
  [network]
  min_trust_threshold = 0.3  # Don't accept untrusted peers
  ```
- [ ] **Enable rate limiting** (already default, verify in config)
- [ ] **Configure trust-gated topics** for sensitive gossip

### Network Level

- [ ] **Use private network for inter-node communication** if possible
- [ ] **Deploy behind reverse proxy** (nginx) for gateway API
- [ ] **Enable DDoS protection** (CloudFlare, AWS Shield, etc.)
- [ ] **Set up VPN** for administrative access
- [ ] **Monitor network traffic** for anomalies

---

## Deployment Architecture

### Single Node (Development/Small Coop)

```
┌─────────────────────────────────────┐
│         Internet                    │
└──────────────┬──────────────────────┘
               │
               ▼
      ┌────────────────┐
      │     Nginx      │ (TLS termination)
      │  (reverse proxy)│
      └────────┬───────┘
               │
               ▼
      ┌────────────────┐
      │     icnd       │
      │  (ICN daemon)  │
      └────────┬───────┘
               │
               ▼
      ┌────────────────┐
      │  Sled Database │
      │  /var/lib/icn  │
      └────────────────┘
```

### Multi-Node (Production Coop)

```
┌─────────────────────────────────────┐
│         Load Balancer               │
│       (nginx/haproxy)               │
└──────┬─────────────┬─────────────┬──┘
       │             │             │
       ▼             ▼             ▼
   ┌─────┐       ┌─────┐       ┌─────┐
   │icnd1│◄─────►│icnd2│◄─────►│icnd3│
   └──┬──┘       └──┬──┘       └──┬──┘
      │             │             │
      ▼             ▼             ▼
   [sled1]       [sled2]       [sled3]
```

**Note**: ICN uses P2P gossip for state synchronization. Each node maintains its own database and syncs via gossip protocol.

---

## Installation

### 1. Install ICN Binaries

```bash
# Download and verify release
wget https://github.com/InterCooperative-Network/icn/releases/download/v0.1.0/icn-linux-x86_64.tar.gz
wget https://github.com/InterCooperative-Network/icn/releases/download/v0.1.0/icn-linux-x86_64.tar.gz.sha256

# Verify checksum
sha256sum -c icn-linux-x86_64.tar.gz.sha256

# Extract
tar -xzf icn-linux-x86_64.tar.gz
sudo cp icnd icnctl /usr/local/bin/
sudo chmod +x /usr/local/bin/icnd /usr/local/bin/icnctl
```

### 2. Create ICN User and Directories

```bash
sudo useradd -r -s /bin/false icn
sudo mkdir -p /etc/icn /var/lib/icn /var/log/icn
sudo chown -R icn:icn /var/lib/icn /var/log/icn
```

### 3. Generate Identity

```bash
# As the icn user
sudo -u icn icnctl id init --data-dir /var/lib/icn

# Store the DID securely
sudo -u icn icnctl id show --data-dir /var/lib/icn > /etc/icn/node-did.txt
sudo chmod 400 /etc/icn/node-did.txt
```

### 4. Create Configuration

```bash
sudo tee /etc/icn/icn.toml << 'EOF'
data_dir = "/var/lib/icn"

[network]
listen_addr = "0.0.0.0:7777"
rpc_port = 5601
mdns_enabled = false  # Disable mDNS in production
bootstrap_peers = [
    "icn-bootstrap.example.com:7777",
]
min_trust_threshold = 0.3

[observability]
metrics_port = 9100
health_port = 8080
log_level = "info"

[rate_limiting]
enabled = true
refill_interval_ms = 100

[rate_limiting.isolated]
max_messages_per_second = 5
burst_capacity = 1

[rate_limiting.known]
max_messages_per_second = 20
burst_capacity = 5

[rate_limiting.partner]
max_messages_per_second = 50
burst_capacity = 10

[rate_limiting.federated]
max_messages_per_second = 100
burst_capacity = 20

[gateway]
enabled = true
bind_addr = "127.0.0.1:8443"  # Only localhost, nginx proxies
# jwt_secret loaded from environment variable
EOF

sudo chmod 644 /etc/icn/icn.toml
```

### 5. Create Systemd Service

```bash
sudo tee /etc/systemd/system/icnd.service << 'EOF'
[Unit]
Description=ICN Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=icn
Group=icn
WorkingDirectory=/var/lib/icn

# Load environment variables
EnvironmentFile=/etc/icn/icnd.env

# Start daemon
ExecStart=/usr/local/bin/icnd --config /etc/icn/icn.toml

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/icn /var/log/icn
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictRealtime=true
RestrictNamespaces=true
LockPersonality=true
MemoryDenyWriteExecute=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX

# Resource limits
LimitNOFILE=65536
LimitNPROC=512

# Restart policy
Restart=always
RestartSec=10
StartLimitInterval=200
StartLimitBurst=5

[Install]
WantedBy=multi-user.target
EOF

# Create environment file
sudo tee /etc/icn/icnd.env << EOF
ICN_GATEWAY_JWT_SECRET=$(openssl rand -hex 32)
RUST_LOG=icn=info,warn
RUST_BACKTRACE=1
EOF

sudo chmod 600 /etc/icn/icnd.env

# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable icnd
sudo systemctl start icnd
```

### 6. Verify Installation

```bash
# Check service status
sudo systemctl status icnd

# Check health endpoint
curl http://localhost:8080/health

# Check logs
sudo journalctl -u icnd -f
```

---

## Gateway TLS Configuration

### Option A: nginx Reverse Proxy (Recommended)

```bash
# Install nginx
sudo apt install nginx certbot python3-certbot-nginx

# Obtain Let's Encrypt certificate
sudo certbot --nginx -d icn.example.com

# Configure nginx
sudo tee /etc/nginx/sites-available/icn << 'EOF'
upstream icn_gateway {
    server 127.0.0.1:8443;
    keepalive 32;
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name icn.example.com;

    # TLS configuration
    ssl_certificate /etc/letsencrypt/live/icn.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/icn.example.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;
    ssl_prefer_server_ciphers on;
    ssl_session_cache shared:SSL:10m;
    ssl_session_timeout 10m;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Frame-Options "DENY" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=icn_api:10m rate=10r/s;
    limit_req zone=icn_api burst=20 nodelay;

    # Proxy to ICN gateway
    location / {
        proxy_pass http://icn_gateway;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }

    # Health check endpoint (no auth required)
    location /health {
        proxy_pass http://localhost:8080/health;
        access_log off;
    }
}

# Redirect HTTP to HTTPS
server {
    listen 80;
    listen [::]:80;
    server_name icn.example.com;
    return 301 https://$server_name$request_uri;
}
EOF

sudo ln -s /etc/nginx/sites-available/icn /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

### Option B: Direct TLS (Not Recommended for Production)

If you must terminate TLS directly in icnd, configure rustls certificates in the config.

---

## Monitoring Setup

### 1. Prometheus Configuration

```yaml
# /etc/prometheus/prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'icn'
    static_configs:
      - targets: ['localhost:9100']
    relabel_configs:
      - source_labels: [__address__]
        target_label: instance
        replacement: 'icn-node1'
```

### 2. Key Metrics to Monitor

| Metric | Alert Threshold | Description |
|--------|----------------|-------------|
| `icn_gossip_peers_total` | < 3 | Number of connected peers |
| `icn_ledger_entries_total` | Rate of change | Transaction volume |
| `icn_ledger_balance_total` | Balance limits | Economic health |
| `icn_trust_edges_total` | Decreasing | Trust network health |
| `icn_network_bytes_received_total` | Spikes | Network anomalies |
| `icn_gateway_requests_total` | Error rate > 5% | API health |
| `icn_compute_tasks_pending` | > 100 | Task queue depth |

### 3. Alerting Rules

```yaml
# /etc/prometheus/alerts/icn.yml
groups:
  - name: icn
    interval: 30s
    rules:
      - alert: ICNPeerCountLow
        expr: icn_gossip_peers_total < 3
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "ICN node has few peers"
          description: "Node {{ $labels.instance }} has only {{ $value }} peers"

      - alert: ICNHighErrorRate
        expr: rate(icn_gateway_requests_errors_total[5m]) > 0.05
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "ICN API error rate high"
          description: "Error rate is {{ $value }} on {{ $labels.instance }}"
```

---

## Backup and Recovery

### Automated Backups

```bash
# Create backup script
sudo tee /usr/local/bin/icn-backup.sh << 'EOF'
#!/bin/bash
set -e

BACKUP_DIR="/var/backups/icn"
DATA_DIR="/var/lib/icn"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/icn-backup-$TIMESTAMP.tar.gz.enc"

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Stop daemon temporarily (or use icnctl backup if available)
systemctl stop icnd

# Create encrypted backup
tar -czf - -C "$DATA_DIR" . | \
  openssl enc -aes-256-cbc -salt -pbkdf2 \
  -pass file:/etc/icn/backup-key.txt \
  -out "$BACKUP_FILE"

# Restart daemon
systemctl start icnd

# Keep only last 7 days
find "$BACKUP_DIR" -name "icn-backup-*.tar.gz.enc" -mtime +7 -delete

echo "Backup completed: $BACKUP_FILE"
EOF

sudo chmod +x /usr/local/bin/icn-backup.sh

# Generate backup encryption key
openssl rand -hex 32 | sudo tee /etc/icn/backup-key.txt
sudo chmod 400 /etc/icn/backup-key.txt
```

### Cron Schedule

```bash
# Daily backup at 2 AM
sudo crontab -e
0 2 * * * /usr/local/bin/icn-backup.sh >> /var/log/icn/backup.log 2>&1
```

### Recovery Procedure

```bash
# Stop daemon
sudo systemctl stop icnd

# Decrypt and restore
sudo openssl enc -aes-256-cbc -d -pbkdf2 \
  -pass file:/etc/icn/backup-key.txt \
  -in /var/backups/icn/icn-backup-YYYYMMDD_HHMMSS.tar.gz.enc | \
  sudo tar -xzf - -C /var/lib/icn

# Fix permissions
sudo chown -R icn:icn /var/lib/icn

# Start daemon
sudo systemctl start icnd
```

---

## Disaster Recovery Plan

### RTO/RPO Targets

- **RPO (Recovery Point Objective)**: 1 hour (daily backups + gossip sync)
- **RTO (Recovery Time Objective)**: 30 minutes (restore from backup)

### DR Procedure

1. **Detect Failure**: Monitoring alerts trigger
2. **Assess Impact**: Check health endpoints, logs
3. **Decide Recovery Method**:
   - Minor: Restart service
   - Major: Restore from backup
   - Catastrophic: Rebuild from gossip sync
4. **Execute Recovery**: Follow procedure above
5. **Verify Restoration**: Health checks, peer count, ledger sync
6. **Post-Mortem**: Document incident, update runbooks

### Failover Strategy

**Multi-Node Deployment**:
- Nodes sync via gossip automatically
- Load balancer detects failed nodes and routes around them
- No manual intervention needed for node failure
- Data loss: None (replicated across nodes)

**Single-Node Deployment**:
- Restore from latest backup
- Re-sync missing data from network via gossip
- Expected data loss: 1 hour (since last backup)

---

## Capacity Planning

### Resource Requirements by Network Size

| Network Size | CPU | RAM | Disk | Bandwidth |
|--------------|-----|-----|------|-----------|
| 10 nodes | 2 cores | 4 GB | 50 GB | 10 Mbps |
| 50 nodes | 4 cores | 8 GB | 100 GB | 50 Mbps |
| 100 nodes | 8 cores | 16 GB | 200 GB | 100 Mbps |
| 500 nodes | 16 cores | 32 GB | 500 GB | 500 Mbps |

### Scaling Considerations

- **Gossip fanout**: Adjust `topology.fanout.*` in config
- **Trust computation**: Scales O(N²) for full mesh, cache aggressively
- **Ledger storage**: Grows linearly with transactions (~1 KB per entry)
- **Bandwidth**: Gossip traffic scales with network size and message rate

---

## Troubleshooting

### Common Issues

**Issue**: Node can't connect to peers
- Check firewall: `sudo ufw status`
- Verify bootstrap peers are reachable: `telnet bootstrap.example.com 7777`
- Check logs: `sudo journalctl -u icnd | grep "connection"`

**Issue**: High CPU usage
- Check peer count: `icnctl network peers`
- Review trust computation cache hits in metrics
- Consider reducing gossip fanout

**Issue**: Disk space growing rapidly
- Check ledger entry count: `icnctl ledger stats`
- Review gossip content storage
- Implement archival strategy for old entries

**Issue**: Gateway API not responding
- Check gateway is enabled in config
- Verify JWT secret is set: `env | grep ICN_GATEWAY_JWT_SECRET`
- Check nginx proxy: `sudo nginx -t && sudo systemctl status nginx`

---

## Security Incident Response

1. **Detect**: Monitoring alerts, user reports
2. **Isolate**: Firewall the affected node
3. **Investigate**: Review logs, network traffic, system calls
4. **Contain**: Stop icnd, snapshot disk for forensics
5. **Eradicate**: Remove malicious content, patch vulnerabilities
6. **Recover**: Restore from clean backup, re-sync from network
7. **Lessons Learned**: Document incident, improve defenses

### Contacts

- **Security Team**: security@example.com
- **On-Call**: +1-555-ONCALL
- **ICN Community**: #security on Discord

---

## Compliance

### GDPR Considerations

- **Data Minimization**: ICN stores only DIDs and transaction data
- **Right to Erasure**: Not directly supported (immutable ledger)
- **Data Portability**: Use `icnctl backup` to export data
- **Consent**: Obtain explicit consent before adding users to ledger

### Audit Logging

```bash
# Enable audit logging
sudo auditctl -w /var/lib/icn -p wa -k icn_data
sudo auditctl -w /usr/local/bin/icnd -p x -k icn_binary
```

---

## Maintenance Windows

- **Updates**: Weekly, Wednesday 2-4 AM UTC
- **Backups**: Daily, 2 AM local time
- **Certificate Renewal**: Automatic (Let's Encrypt)
- **Security Patches**: Within 24 hours of release

---

## Support

- **Documentation**: https://icn.coop/docs
- **Community**: https://discord.gg/icn
- **Bug Reports**: https://github.com/InterCooperative-Network/icn/issues
- **Security Reports**: security@icn.coop (GPG: 0xABCD1234)

---

**Document Revision History**:
- 2025-12-16: Initial production deployment guide created
