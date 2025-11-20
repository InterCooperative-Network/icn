# Production Deployment Guide - ICN Pilot UI

This guide covers deploying the ICN Pilot UI to production with TLS/HTTPS, proper security, and monitoring.

## Quick Links

- **Development Setup**: See [README.md](README.md)
- **Feature Documentation**: See [SUMMARY.md](SUMMARY.md)
- **Deployment Checklist**: See [DEPLOYMENT-CHECKLIST.md](DEPLOYMENT-CHECKLIST.md)
- **User Guides**: [QUICK-START.md](QUICK-START.md), [ADMIN-GUIDE.md](ADMIN-GUIDE.md)

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Deployment Options](#deployment-options)
3. [Option A: Docker Compose (Recommended)](#option-a-docker-compose-recommended)
4. [Option B: Bare Metal with systemd](#option-b-bare-metal-with-systemd)
5. [Option C: Reverse Proxy (nginx/Caddy)](#option-c-reverse-proxy-nginxcaddy)
6. [TLS/HTTPS Setup](#tlshttps-setup)
7. [Security Hardening](#security-hardening)
8. [Monitoring & Maintenance](#monitoring--maintenance)
9. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### System Requirements

**Minimum** (10-50 members):
- 1 CPU core
- 1 GB RAM
- 10 GB disk space
- Ubuntu 20.04+ or Debian 11+

**Recommended** (50-200 members):
- 2 CPU cores
- 4 GB RAM
- 50 GB disk space
- Ubuntu 22.04 LTS

**Production** (200+ members):
- 4+ CPU cores
- 8 GB RAM
- 100 GB disk space
- Load balancer ready

### Software Requirements

- Docker 20.10+ and Docker Compose 2.0+ (for Docker deployment)
- OR nginx 1.18+ / Caddy 2.0+ (for reverse proxy deployment)
- certbot for Let's Encrypt TLS certificates
- A domain name pointing to your server

### Network Requirements

- Ports 80 and 443 open (HTTP/HTTPS)
- Port 8080 available for ICN gateway (internal)
- Firewall configured (see Security Hardening)

---

## Deployment Options

Choose the deployment method that best fits your infrastructure:

| Method | Complexity | Best For | Pros | Cons |
|--------|-----------|----------|------|------|
| **Docker Compose** | Low | Most users | Easy setup, isolated, reproducible | Requires Docker |
| **Bare Metal** | Medium | Control freaks | Maximum performance, no overhead | More manual setup |
| **Reverse Proxy** | Medium | Existing infra | Integrates with existing setup | Requires nginx/Caddy |

---

## Option A: Docker Compose (Recommended)

### 1. Clone Repository

```bash
cd /opt
sudo git clone https://github.com/anthropics/icn.git
cd icn/deploy
```

### 2. Configure Environment

```bash
# Copy and edit environment file
cp .env.example .env

# Generate strong JWT secret
openssl rand -hex 32 > jwt-secret.txt
JWT_SECRET=$(cat jwt-secret.txt)

# Edit .env
nano .env
```

**Required settings**:
```bash
JWT_SECRET=<paste-your-generated-secret>
GRAFANA_PASSWORD=<strong-password>
COOP_NAME=Your Cooperative Name
```

### 3. Configure ICN Daemon

Edit `config/icn.toml`:

```toml
[gateway]
enabled = true
bind_addr = "0.0.0.0:8080"
jwt_secret = "${JWT_SECRET}"  # Reads from environment
token_expiry_hours = 24
challenge_ttl_minutes = 5

[network]
listen_addr = "0.0.0.0:7777"
mdns_enabled = false  # Disable for production

[observability]
metrics_port = 9090
health_port = 8080
log_level = "info"
```

### 4. Start Services

```bash
# Build and start
docker compose up -d

# Check status
docker compose ps

# View logs
docker compose logs -f icnd
```

### 5. Initialize Identity

```bash
# Create identity (use strong passphrase!)
docker compose exec icnd icnctl id init

# Show DID
docker compose exec icnd icnctl id show
```

### 6. Create Cooperative

```bash
# Get your DID from previous step
DID="did:icn:YOUR_DID_HERE"

# Get auth token
TOKEN=$(docker compose exec icnd icnctl auth login \
    --gateway http://localhost:8080 \
    --coop your-coop-id)

# Create cooperative via API or icnctl
docker compose exec icnd icnctl coops create \
    --id "your-coop-id" \
    --name "Your Cooperative Name"
```

### 7. Access UI

- Web UI: http://your-server-ip:3000
- Gateway: http://your-server-ip:8080
- Grafana: http://your-server-ip:3001

**Important**: This is HTTP only. For HTTPS, see [TLS/HTTPS Setup](#tlshttps-setup).

---

## Option B: Bare Metal with systemd

### 1. Build ICN Binaries

```bash
cd /opt
git clone https://github.com/anthropics/icn.git
cd icn/icn

# Build release binaries
cargo build --release

# Install binaries
sudo cp target/release/icnd /usr/local/bin/
sudo cp target/release/icnctl /usr/local/bin/
sudo chmod +x /usr/local/bin/icnd /usr/local/bin/icnctl
```

### 2. Create System User

```bash
sudo useradd -r -s /bin/false -m -d /var/lib/icn icn
sudo mkdir -p /etc/icn
sudo mkdir -p /var/log/icn
sudo chown icn:icn /var/lib/icn /var/log/icn
```

### 3. Configure ICN

Create `/etc/icn/icn.toml`:

```toml
[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
jwt_secret = "CHANGE-ME-USE-STRONG-SECRET"
token_expiry_hours = 24

[network]
listen_addr = "0.0.0.0:7777"

[data]
dir = "/var/lib/icn"

[observability]
metrics_port = 9090
log_level = "info"
log_file = "/var/log/icn/icnd.log"
```

### 4. Create systemd Service

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
WorkingDirectory=/var/lib/icn
ExecStart=/usr/local/bin/icnd --config /etc/icn/icn.toml
Restart=always
RestartSec=5
StandardOutput=append:/var/log/icn/icnd.log
StandardError=append:/var/log/icn/icnd-error.log

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ReadWritePaths=/var/lib/icn /var/log/icn

[Install]
WantedBy=multi-user.target
```

### 5. Start Service

```bash
sudo systemctl daemon-reload
sudo systemctl enable icnd
sudo systemctl start icnd
sudo systemctl status icnd
```

### 6. Serve Web UI with nginx

Install nginx:

```bash
sudo apt update
sudo apt install nginx
```

Copy UI files:

```bash
sudo mkdir -p /var/www/icn-pilot-ui
sudo cp -r /opt/icn/web/pilot-ui/* /var/www/icn-pilot-ui/
sudo chown -R www-data:www-data /var/www/icn-pilot-ui
```

Create nginx config (see [Option C](#option-c-reverse-proxy-nginxcaddy) for full config).

---

## Option C: Reverse Proxy (nginx/Caddy)

### nginx Configuration

Create `/etc/nginx/sites-available/icn-pilot`:

```nginx
# Redirect HTTP to HTTPS
server {
    listen 80;
    server_name timebank.example.com;
    return 301 https://$server_name$request_uri;
}

# HTTPS server
server {
    listen 443 ssl http2;
    server_name timebank.example.com;

    # TLS certificates (Let's Encrypt)
    ssl_certificate /etc/letsencrypt/live/timebank.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/timebank.example.com/privkey.pem;

    # TLS settings (Mozilla Modern)
    ssl_protocols TLSv1.3;
    ssl_prefer_server_ciphers off;
    ssl_session_timeout 1d;
    ssl_session_cache shared:SSL:50m;
    ssl_session_tickets off;

    # OCSP stapling
    ssl_stapling on;
    ssl_stapling_verify on;
    ssl_trusted_certificate /etc/letsencrypt/live/timebank.example.com/chain.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Referrer-Policy "no-referrer-when-downgrade" always;

    # Serve static web UI
    location / {
        root /var/www/icn-pilot-ui;
        index index.html;
        try_files $uri $uri/ /index.html;

        # Cache static assets
        location ~* \.(js|css|png|jpg|jpeg|gif|ico|svg)$ {
            expires 30d;
            add_header Cache-Control "public, immutable";
        }
    }

    # Proxy API requests to ICN gateway
    location /v1 {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;

        # WebSocket support
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        # Headers
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Timeouts (increase for WebSocket)
        proxy_read_timeout 86400;
        proxy_send_timeout 86400;

        # Rate limiting (optional)
        limit_req zone=api burst=20 nodelay;
    }

    # Health check endpoint
    location /health {
        proxy_pass http://127.0.0.1:8080/v1/health;
        access_log off;
    }

    # Access logs
    access_log /var/log/nginx/icn-access.log;
    error_log /var/log/nginx/icn-error.log;
}

# Rate limiting zone (define in /etc/nginx/nginx.conf http block)
# limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
```

Enable site:

```bash
sudo ln -s /etc/nginx/sites-available/icn-pilot /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

### Caddy Configuration

Create `/etc/caddy/Caddyfile`:

```caddy
timebank.example.com {
    # Serve static files
    root * /var/www/icn-pilot-ui
    file_server
    try_files {path} /index.html

    # Proxy API requests
    reverse_proxy /v1/* 127.0.0.1:8080 {
        header_up Host {host}
        header_up X-Real-IP {remote}
        header_up X-Forwarded-For {remote}
        header_up X-Forwarded-Proto {scheme}
    }

    # Security headers
    header {
        Strict-Transport-Security "max-age=63072000; includeSubDomains; preload"
        X-Frame-Options "SAMEORIGIN"
        X-Content-Type-Options "nosniff"
        X-XSS-Protection "1; mode=block"
    }

    # Automatic HTTPS via Let's Encrypt
    tls {
        protocols tls1.3
    }

    # Logging
    log {
        output file /var/log/caddy/icn-access.log
    }
}
```

Caddy automatically handles TLS certificates via Let's Encrypt!

```bash
sudo systemctl reload caddy
```

---

## TLS/HTTPS Setup

### Let's Encrypt (Recommended)

Install certbot:

```bash
# Ubuntu/Debian
sudo apt install certbot python3-certbot-nginx

# For nginx
sudo certbot --nginx -d timebank.example.com

# For manual (use with Caddy or other servers)
sudo certbot certonly --standalone -d timebank.example.com
```

Auto-renewal:

```bash
# Test renewal
sudo certbot renew --dry-run

# Certbot installs a cron job automatically
# Verify with:
sudo systemctl status certbot.timer
```

### Custom Certificates

If using custom certificates:

```bash
# Place certificates
sudo cp your-cert.pem /etc/ssl/certs/icn-pilot.pem
sudo cp your-key.pem /etc/ssl/private/icn-pilot-key.pem
sudo chmod 600 /etc/ssl/private/icn-pilot-key.pem

# Update nginx config
ssl_certificate /etc/ssl/certs/icn-pilot.pem;
ssl_certificate_key /etc/ssl/private/icn-pilot-key.pem;
```

---

## Security Hardening

### 1. Firewall Configuration

```bash
# UFW (Ubuntu)
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow ssh
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw allow 7777/udp  # ICN P2P (if public node)
sudo ufw enable

# iptables alternative
sudo iptables -A INPUT -p tcp --dport 80 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 443 -j ACCEPT
sudo iptables -A INPUT -p udp --dport 7777 -j ACCEPT
sudo iptables-save > /etc/iptables/rules.v4
```

### 2. JWT Secret Security

```bash
# Generate strong secret (32+ characters)
openssl rand -hex 32 > /etc/icn/jwt-secret.txt
chmod 600 /etc/icn/jwt-secret.txt
chown icn:icn /etc/icn/jwt-secret.txt

# Use in icn.toml
jwt_secret = "$(cat /etc/icn/jwt-secret.txt)"
```

**NEVER commit JWT secrets to version control!**

### 3. User Permissions

```bash
# ICN data directory
sudo chown -R icn:icn /var/lib/icn
sudo chmod 700 /var/lib/icn

# Web UI files
sudo chown -R www-data:www-data /var/www/icn-pilot-ui
sudo chmod 755 /var/www/icn-pilot-ui
sudo chmod 644 /var/www/icn-pilot-ui/*
```

### 4. Rate Limiting

Add to nginx config:

```nginx
# In http block
limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
limit_req_zone $binary_remote_addr zone=login:10m rate=5r/m;

# In server block
location /v1/auth {
    limit_req zone=login burst=10 nodelay;
    proxy_pass http://127.0.0.1:8080;
}

location /v1 {
    limit_req zone=api burst=20 nodelay;
    proxy_pass http://127.0.0.1:8080;
}
```

### 5. Fail2ban (Optional)

Protect against brute-force attacks:

```bash
sudo apt install fail2ban

# Create filter
sudo nano /etc/fail2ban/filter.d/icn-gateway.conf
```

```ini
[Definition]
failregex = ^.*"401".*"POST /v1/auth/verify".*"<HOST>".*$
ignoreregex =
```

```bash
# Create jail
sudo nano /etc/fail2ban/jail.d/icn-gateway.conf
```

```ini
[icn-gateway]
enabled = true
port = http,https
filter = icn-gateway
logpath = /var/log/nginx/icn-access.log
maxretry = 5
bantime = 3600
findtime = 600
```

```bash
sudo systemctl restart fail2ban
```

---

## Monitoring & Maintenance

### 1. Health Checks

```bash
# Manual check
curl https://timebank.example.com/health

# Automated monitoring (add to cron)
*/5 * * * * curl -f https://timebank.example.com/health || systemctl restart icnd
```

### 2. Log Management

```bash
# View logs
sudo journalctl -u icnd -f

# Rotate logs (logrotate config)
sudo nano /etc/logrotate.d/icnd
```

```
/var/log/icn/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 644 icn icn
    postrotate
        systemctl reload icnd
    endscript
}
```

### 3. Backups

```bash
# Automated backup script
sudo nano /usr/local/bin/icn-backup.sh
```

```bash
#!/bin/bash
BACKUP_DIR="/var/backups/icn"
DATE=$(date +%Y%m%d-%H%M%S)

mkdir -p $BACKUP_DIR

# Backup ICN data
tar -czf $BACKUP_DIR/icn-data-$DATE.tar.gz /var/lib/icn

# Backup configuration
tar -czf $BACKUP_DIR/icn-config-$DATE.tar.gz /etc/icn

# Keep last 30 days
find $BACKUP_DIR -name "*.tar.gz" -mtime +30 -delete
```

```bash
chmod +x /usr/local/bin/icn-backup.sh

# Add to cron (daily at 2 AM)
0 2 * * * /usr/local/bin/icn-backup.sh
```

### 4. Prometheus & Grafana

If using Docker Compose, Grafana is available at `:3001`.

For bare metal:

```bash
# Install Prometheus
sudo apt install prometheus

# Configure scrape target
sudo nano /etc/prometheus/prometheus.yml
```

```yaml
scrape_configs:
  - job_name: 'icn'
    static_configs:
      - targets: ['localhost:9090']
```

Install Grafana: https://grafana.com/docs/grafana/latest/setup-grafana/installation/

Import dashboard from: `/opt/icn/monitoring/grafana-dashboard.json`

---

## Troubleshooting

### UI won't load

```bash
# Check nginx status
sudo systemctl status nginx
sudo nginx -t

# Check logs
sudo tail -f /var/log/nginx/error.log

# Check web UI files
ls -la /var/www/icn-pilot-ui/
```

### API requests failing (CORS errors)

Check nginx proxy headers:

```nginx
proxy_set_header Host $host;
proxy_set_header X-Real-IP $remote_addr;
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
proxy_set_header X-Forwarded-Proto $scheme;
```

### WebSocket connection drops

Increase nginx timeouts:

```nginx
proxy_read_timeout 86400;
proxy_send_timeout 86400;
```

### Gateway not responding

```bash
# Check icnd status
sudo systemctl status icnd

# Check logs
sudo journalctl -u icnd -n 100

# Check if port is listening
sudo netstat -tlnp | grep 8080

# Restart service
sudo systemctl restart icnd
```

### Certificate renewal fails

```bash
# Test renewal manually
sudo certbot renew --dry-run

# Check certbot logs
sudo tail -f /var/log/letsencrypt/letsencrypt.log

# Ensure port 80 is accessible (Let's Encrypt needs it)
sudo ufw allow 80/tcp
```

### High memory usage

```bash
# Check icnd resource usage
ps aux | grep icnd

# Check Docker stats (if using Docker)
docker stats icn-daemon

# Consider increasing system resources or implementing:
# - Ledger entry pruning
# - Gossip entry limits (already default 1000)
# - Database compaction
```

---

## Performance Tuning

### nginx

```nginx
# Worker processes (match CPU cores)
worker_processes auto;

# Connection limits
events {
    worker_connections 2048;
    use epoll;
}

# Gzip compression
gzip on;
gzip_comp_level 6;
gzip_types text/plain text/css application/json application/javascript text/xml application/xml;

# File caching
open_file_cache max=10000 inactive=30s;
```

### ICN Daemon

Tune in `/etc/icn/icn.toml`:

```toml
[network]
max_peers = 50  # Adjust based on cooperative size
connection_timeout = 30

[gossip]
anti_entropy_interval = 300  # 5 minutes
entry_ttl = 2592000  # 30 days

[ledger]
cache_size = 10000  # Number of transactions to cache
```

---

## Maintenance Schedule

### Daily
- [ ] Check service status
- [ ] Review error logs
- [ ] Monitor disk space

### Weekly
- [ ] Review access logs
- [ ] Check backup success
- [ ] Update security patches

### Monthly
- [ ] Test backup restoration
- [ ] Review Grafana metrics
- [ ] Update dependencies
- [ ] Review firewall rules

### Quarterly
- [ ] Security audit
- [ ] Performance review
- [ ] Capacity planning
- [ ] User satisfaction survey

---

## Support Resources

- **ICN Documentation**: `/opt/icn/docs/`
- **Admin Guide**: [ADMIN-GUIDE.md](ADMIN-GUIDE.md)
- **Operations Guide**: `/opt/icn/docs/operations-guide.md`
- **GitHub Issues**: https://github.com/anthropics/icn/issues

---

## License

MIT OR Apache-2.0
