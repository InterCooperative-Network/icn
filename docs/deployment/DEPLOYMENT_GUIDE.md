# ICN Deployment Guide

**Status:** Production-ready backend with enhanced mobile app

This guide covers deploying the ICN gateway backend for your mobile app and web pilots.

---

## 🚀 Quick Start (Recommended for Testing)

### Option 1: Docker Compose (Easiest)

**What you get:** Complete stack with ICN gateway, monitoring, and web UI

```bash
# 1. Clone repository
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/deploy

# 2. Configure environment
cp .env.example .env
# Edit .env and set JWT_SECRET to a random value:
openssl rand -hex 32  # Generate secret, then paste into .env

# 3. Start the stack
docker-compose up -d

# 4. Check health
curl http://localhost:8080/v1/health
```

**Services Available:**
- Gateway API: http://localhost:8080
- Web UI: http://localhost:3000
- Grafana (monitoring): http://localhost:3001 (admin/admin)
- Prometheus: http://localhost:9091

**Mobile App Configuration:**
Update your mobile app's `client.ts`:
```typescript
const client = createMobileClient({
  baseUrl: 'http://YOUR_SERVER_IP:8080',  // Change from localhost
  wallet,
  storage,
});
```

---

### Option 2: Quick Start Script (Automated)

**Includes demo data seeding:**

```bash
cd icn/deploy
./quickstart.sh "My Timebank"
```

This script:
1. Builds and starts Docker containers
2. Initializes ICN identity
3. Creates demo cooperative
4. Seeds test data (members, transactions)
5. Provides authentication tokens

---

## 🏭 Production Deployment

### Option 1: Native Linux Installation (Best Performance)

**For production deployments on bare metal or VPS:**

```bash
# 1. Build from source
cd icn/icn
cargo build --release

# 2. Install (requires root)
sudo ../deploy/install.sh

# 3. Configure
sudo cp /etc/icn/icnd.env.example /etc/icn/icnd.env
sudo nano /etc/icn/icnd.env  # Set JWT_SECRET

# 4. Initialize identity
sudo -u icn icnctl --data-dir /var/lib/icn id init

# 5. Start service
sudo systemctl enable icnd
sudo systemctl start icnd
sudo systemctl status icnd

# 6. Check health
curl http://localhost:8080/v1/health
```

**Files installed:**
- `/usr/local/bin/icnd` - ICN daemon
- `/usr/local/bin/icnctl` - CLI tool
- `/etc/icn/icnd.env` - Configuration
- `/var/lib/icn/` - Data directory
- `/etc/systemd/system/icnd.service` - Systemd service

**Systemd commands:**
```bash
sudo systemctl start icnd    # Start
sudo systemctl stop icnd     # Stop
sudo systemctl restart icnd  # Restart
sudo systemctl status icnd   # Status
journalctl -u icnd -f        # View logs
```

---

### Option 2: Kubernetes (For Scale)

**For orchestrated deployments:**

```bash
cd icn/deploy/k8s

# 1. Create namespace
kubectl apply -f namespace.yaml

# 2. Create secrets
kubectl create secret generic icn-secrets \
  --namespace=icn \
  --from-literal=passphrase='your-secure-passphrase' \
  --from-literal=jwt-secret='your-jwt-secret'

# 3. Deploy
kubectl apply -k .

# 4. Check status
kubectl get pods -n icn
kubectl get services -n icn

# 5. Port forward for testing
kubectl port-forward -n icn svc/icn-gateway 8080:8080
```

**Includes:**
- Deployment with liveness/readiness probes
- Service for gateway API
- ConfigMap for configuration
- PersistentVolumeClaim for data
- ServiceMonitor for Prometheus

---

## 📱 Mobile App Deployment

### iOS (TestFlight)

1. **Build the app:**
   ```bash
   cd sdk/react-native/examples/CoopWallet
   
   # Configure API endpoint
   # Edit src/client.ts and set baseUrl to your server
   
   # Install dependencies
   npm install
   cd ios && pod install && cd ..
   
   # Open in Xcode
   npx react-native run-ios --configuration Release
   ```

2. **TestFlight deployment:**
   - Open in Xcode
   - Select "Any iOS Device" as target
   - Product → Archive
   - Distribute → App Store Connect
   - Upload to TestFlight
   - Add beta testers

### Android (Google Play Console)

1. **Build the app:**
   ```bash
   cd sdk/react-native/examples/CoopWallet
   
   # Configure API endpoint
   # Edit src/client.ts
   
   # Build release APK
   cd android
   ./gradlew assembleRelease
   
   # Find APK at:
   # android/app/build/outputs/apk/release/app-release.apk
   ```

2. **Play Store deployment:**
   - Sign APK with your keystore
   - Upload to Play Console
   - Create internal/beta test track
   - Add testers via email list

---

## 🔧 Configuration

### Gateway Environment Variables

Edit `/etc/icn/icnd.env` or `.env` file:

```bash
# Gateway API
ICN_GATEWAY_JWT_SECRET=your-random-secret-here
ICN_GATEWAY_PORT=8080
ICN_GATEWAY_HOST=0.0.0.0

# CORS (for web UI)
ICN_GATEWAY_CORS_ORIGINS=http://localhost:3000,https://your-domain.com

# Logging
RUST_LOG=info,icn_gateway=debug

# P2P networking
ICN_QUIC_PORT=7777
ICN_QUIC_BIND_ADDR=0.0.0.0

# Metrics
ICN_METRICS_PORT=9100
```

Note: `icnd` does not directly consume `ICN_DATA_DIR`/`ICN_QUIC_PORT`/`ICN_METRICS_PORT` in current runtime code. Set `--data-dir`, `--config`, and config file fields (for example `network.listen_addr`, `observability.metrics_port`) for authoritative behavior.

### Generate JWT Secret

```bash
# Generate a secure random secret
openssl rand -hex 32

# Or use this one-liner to update .env
sed -i "s/JWT_SECRET=.*/JWT_SECRET=$(openssl rand -hex 32)/" .env
```

---

## 🌐 Domain & SSL Setup

### Using Nginx Reverse Proxy

```nginx
# /etc/nginx/sites-available/icn

server {
    listen 80;
    server_name api.your-coop.org;

    # Redirect to HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name api.your-coop.org;

    # SSL certificates (use Let's Encrypt)
    ssl_certificate /etc/letsencrypt/live/api.your-coop.org/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.your-coop.org/privkey.pem;

    # SSL configuration
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_prefer_server_ciphers on;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;

    # Proxy to ICN gateway
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # WebSocket support
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

Enable and reload:
```bash
sudo ln -s /etc/nginx/sites-available/icn /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

### Let's Encrypt SSL

```bash
# Install certbot
sudo apt-get install certbot python3-certbot-nginx

# Get certificate
sudo certbot --nginx -d api.your-coop.org

# Auto-renewal is configured automatically
```

---

## 📊 Monitoring & Health Checks

### Health Endpoint

```bash
# Check gateway health
curl http://localhost:8080/v1/health

# Response:
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_secs": 12345
}
```

### Prometheus Metrics

Available at: http://localhost:9100/metrics

Key metrics:
- `gateway_requests_total` - Total API requests
- `gateway_request_duration_seconds` - Request latency
- `gateway_balance_queries` - Balance lookups
- `gateway_payments_created` - Payments created
- `gateway_notifications_sent` - Notifications sent

### Grafana Dashboards

Access: http://localhost:3001 (default admin/admin)

Pre-configured dashboard includes:
- Request rate and latency
- Payment volume
- Trust graph size
- Notification delivery
- Error rates

### Log Monitoring

```bash
# Docker logs
docker logs -f icn-daemon

# Systemd logs
journalctl -u icnd -f

# Log to file
RUST_LOG=info icnd 2>&1 | tee /var/log/icn/icnd.log
```

---

## 🧪 Testing Deployment

### 1. Health Check

```bash
curl http://localhost:8080/v1/health
```

### 2. Create Test User

```bash
# Using the SDK or web UI
curl -X POST http://localhost:8080/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "alice",
    "password": "test123"
  }'
```

### 3. Mobile App Test

Update mobile app to point to your server:
```typescript
// src/client.ts
const client = createMobileClient({
  baseUrl: 'https://api.your-coop.org',  // Your domain
  wallet,
  storage,
});
```

Build and test on device.

---

## 🔒 Security Checklist

- [ ] Change default JWT_SECRET to random value
- [ ] Use HTTPS in production (Let's Encrypt)
- [ ] Change Grafana admin password
- [ ] Restrict Prometheus/Grafana to internal network
- [ ] Enable firewall (allow 80, 443, block direct 8080 from internet)
- [ ] Keep Docker images updated
- [ ] Use non-root user for ICN daemon
- [ ] Back up `/var/lib/icn` data directory regularly
- [ ] Monitor logs for suspicious activity
- [ ] Set up log rotation

### Firewall Setup (ufw)

```bash
# Allow SSH
sudo ufw allow 22

# Allow HTTP/HTTPS
sudo ufw allow 80
sudo ufw allow 443

# Block direct gateway access (use nginx proxy)
# sudo ufw deny 8080

# Enable firewall
sudo ufw enable
```

---

## 🗄️ Backup & Restore

### Backup

```bash
# Stop service
sudo systemctl stop icnd

# Backup data directory
sudo tar -czf icn-backup-$(date +%Y%m%d).tar.gz /var/lib/icn

# Restart service
sudo systemctl start icnd
```

### Restore

```bash
# Stop service
sudo systemctl stop icnd

# Restore data
sudo tar -xzf icn-backup-20231212.tar.gz -C /

# Fix permissions
sudo chown -R icn:icn /var/lib/icn

# Restart service
sudo systemctl start icnd
```

### Automated Backups

```bash
# Add to crontab
sudo crontab -e

# Daily backup at 2 AM
0 2 * * * /usr/local/bin/icnctl backup --output /backups/icn-$(date +\%Y\%m\%d).tar.gz
```

---

## 🚨 Troubleshooting

### Gateway won't start

```bash
# Check logs
journalctl -u icnd -n 50

# Common issues:
# - Port 8080 already in use: Change ICN_GATEWAY_PORT
# - JWT_SECRET not set: Check /etc/icn/icnd.env
# - Permission errors: Check data directory ownership
```

### Mobile app can't connect

```bash
# Test from mobile network
curl -v http://YOUR_SERVER_IP:8080/v1/health

# Common issues:
# - Firewall blocking: Open port 8080 or use nginx proxy
# - CORS errors: Add mobile app origin to CORS_ORIGINS
# - DNS not resolving: Use IP address temporarily
```

### High memory usage

```bash
# Check resource usage
docker stats

# Adjust memory limits in docker-compose.yml:
deploy:
  resources:
    limits:
      memory: 1G
```

### Database corruption

```bash
# The database is Sled (embedded)
# To recover:
sudo systemctl stop icnd
sudo -u icn icnctl --data-dir /var/lib/icn verify
sudo systemctl start icnd
```

---

## 📈 Scaling

### Horizontal Scaling (Multiple Nodes)

ICN uses P2P networking. Deploy multiple nodes:

```bash
# Node 1
docker-compose up -d

# Node 2 (on different server)
# Edit docker-compose.yml to use different ports
# Add Node 1's address to bootstrap peers
docker-compose up -d
```

### Load Balancing (Gateway API)

Use nginx or HAProxy to distribute gateway API traffic:

```nginx
upstream icn_gateways {
    server 10.0.0.1:8080;
    server 10.0.0.2:8080;
    server 10.0.0.3:8080;
}

server {
    location / {
        proxy_pass http://icn_gateways;
    }
}
```

---

## 🎯 Production Checklist

- [ ] Backend deployed (Docker or native)
- [ ] HTTPS configured with valid SSL
- [ ] JWT_SECRET set to secure random value
- [ ] Firewall configured
- [ ] Monitoring setup (Grafana/Prometheus)
- [ ] Backup strategy implemented
- [ ] Mobile app configured with production URL
- [ ] Mobile app submitted to TestFlight/Play Console
- [ ] Test users can register and transact
- [ ] WebSocket real-time updates working
- [ ] Trust graph displaying correctly
- [ ] Offline mode tested (airplane mode)
- [ ] Push notifications configured (FCM)
- [ ] Documentation for users written

---

## 🆘 Support

- Documentation: `/docs` directory
- GitHub Issues: https://github.com/InterCooperative-Network/icn/issues
- Community: [Add your community link]

---

## 📚 Additional Resources

- [Architecture Documentation](../ARCHITECTURE.md)
- [API Reference](../reference/api/README.md)
- [Mobile App Status](../mobile/MOBILE_APP_STATUS.md)
- [Security Hardening](../security/production-hardening.md)
- [Governance Primitives](../design/governance/governance-primitives.md)

---

**Deployment Snapshot Guidance** 🚀

As of this guide revision, the backend was treated as production-capable for pilot deployments, while the mobile app still required FCM integration for full push notification support. Start with Docker Compose for quick testing, then move to native installation or Kubernetes for production.
