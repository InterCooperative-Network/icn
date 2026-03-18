# ICN Deployment - Complete Guide

**Date:** 2025-12-12
**Status:** Historical Snapshot (not current deployment truth)

> Historical deployment snapshot from 2025-12-12.
> Validate current status with live checks and `docs/ci/CI_CURRENT_STATUS.md` before using this as an operational runbook.

## 🎯 What We've Built

### Backend (Rust)
- **ICN Daemon (icnd)**: Complete P2P coordination layer with all actors
- **Gateway API**: REST + WebSocket with JWT auth, real-time events
- **Trust Graph**: Transitive trust computation with edge management
- **Ledger**: Double-entry accounting with gossip sync
- **Governance**: Proposals, voting, domain-based permissions
- **Compute**: Distributed CCL execution with intelligent scheduling
- **Security**: Multi-layer (transport, message, application) with Byzantine detection

### Mobile App (React Native)
- **Authentication**: DID-based with JWT tokens
- **Wallet UI**: Balance display, transaction history, send/receive
- **Trust Graph**: Visual network with attestation UI
- **Notifications**: Local push with WebSocket live updates
- **Offline Mode**: AsyncStorage caching with sync on reconnect
- **Real-time**: WebSocket integration for live balance/transaction updates

### Infrastructure
- **Docker Compose**: Complete stack (daemon, Prometheus, Grafana, Web UI)
- **Kubernetes**: Production-grade manifests with health checks
- **Monitoring**: Full observability with metrics and dashboards
- **Native Install**: Systemd service for bare metal/VPS

---

## 🚀 Quick Deployment (3 Options)

### Option 1: Docker Compose (Recommended for Testing)

```bash
# 1. Navigate to deploy directory
cd deploy

# 2. Run quickstart script
./quickstart.sh "My Coop Name"

# This will:
# - Build Docker images
# - Start all services (daemon, Prometheus, Grafana, Web UI)
# - Initialize identity
# - Provide access URLs and next steps

# Services available:
# - Gateway API: http://localhost:8080
# - Web UI: http://localhost:3000
# - Grafana: http://localhost:3001 (admin/admin)
# - Prometheus: http://localhost:9091
```

**Configuration:**
```bash
# Edit .env file (created by quickstart.sh)
JWT_SECRET=<random-hex-generated>  # Already set by script
GRAFANA_PASSWORD=admin             # Change for production
```

### Option 2: Native Installation (Best Performance)

```bash
# 1. Build binaries (already done!)
cd icn
cargo build --release

# 2. Install systemd service
cd ../deploy
sudo ./install.sh

# 3. Configure
sudo cp /etc/icn/icnd.env.example /etc/icn/icnd.env
sudo nano /etc/icn/icnd.env  # Set JWT_SECRET

# 4. Initialize identity
sudo -u icn icnctl --data-dir /var/lib/icn id init

# 5. Start service
sudo systemctl enable icnd
sudo systemctl start icnd
sudo systemctl status icnd

# 6. Test
curl http://localhost:8080/v1/health
```

### Option 3: Kubernetes (For Scale)

```bash
cd deploy/k8s

# 1. Create secrets
kubectl create secret generic icn-secrets \
  --namespace=icn \
  --from-literal=passphrase='secure-passphrase' \
  --from-literal=jwt-secret="$(openssl rand -hex 32)"

# 2. Deploy
kubectl apply -k .

# 3. Check status
kubectl get pods -n icn
kubectl get svc -n icn

# 4. Port forward for testing
kubectl port-forward -n icn svc/icn-gateway 8080:8080
```

---

## 📱 Mobile App Configuration

### 1. Update API Endpoint

Edit `sdk/react-native/examples/CoopWallet/src/client.ts`:

```typescript
const client = createMobileClient({
  baseUrl: 'http://YOUR_SERVER_IP:8080',  // Change from localhost
  wallet,
  storage,
});
```

For production with SSL:
```typescript
const client = createMobileClient({
  baseUrl: 'https://api.your-coop.org',
  wallet,
  storage,
});
```

### 2. Build Mobile App

**iOS:**
```bash
cd sdk/react-native/examples/CoopWallet
npm install
cd ios && pod install && cd ..
npx react-native run-ios --configuration Release
```

**Android:**
```bash
cd sdk/react-native/examples/CoopWallet
npm install
cd android
./gradlew assembleRelease
# APK at: android/app/build/outputs/apk/release/app-release.apk
```

### 3. Deploy to App Stores

**TestFlight (iOS):**
1. Open in Xcode
2. Select "Any iOS Device"
3. Product → Archive
4. Distribute → App Store Connect
5. Upload to TestFlight

**Play Console (Android):**
1. Sign APK with your keystore
2. Upload to Play Console
3. Create internal/beta test track
4. Add testers

---

## 🔐 Production Checklist

### Security
- [ ] Change `JWT_SECRET` to random value: `openssl rand -hex 32`
- [ ] Configure HTTPS with Let's Encrypt (see nginx config below)
- [ ] Change Grafana admin password
- [ ] Enable firewall (allow 80, 443; block 8080 from internet)
- [ ] Set up automatic backups
- [ ] Update all default passwords

### Networking
- [ ] Configure DNS: `api.your-coop.org` → Server IP
- [ ] Set up SSL certificate (Let's Encrypt)
- [ ] Configure nginx reverse proxy
- [ ] Test from mobile network (not just localhost)

### Monitoring
- [ ] Access Grafana dashboards
- [ ] Set up alerting rules
- [ ] Configure log rotation
- [ ] Test health checks

### Mobile App
- [ ] Update API endpoint to production URL
- [ ] Test registration flow
- [ ] Test transaction creation
- [ ] Test offline mode (airplane mode)
- [ ] Test WebSocket reconnection
- [ ] Verify push notifications work

---

## 🌐 SSL/HTTPS Setup with Nginx

### Install Nginx and Certbot

```bash
sudo apt-get update
sudo apt-get install nginx certbot python3-certbot-nginx
```

### Configure Nginx

Create `/etc/nginx/sites-available/icn`:

```nginx
server {
    listen 80;
    server_name api.your-coop.org;
    
    # Let's Encrypt challenge
    location /.well-known/acme-challenge/ {
        root /var/www/html;
    }
    
    # Redirect to HTTPS
    location / {
        return 301 https://$server_name$request_uri;
    }
}

server {
    listen 443 ssl http2;
    server_name api.your-coop.org;
    
    # SSL certificates (will be added by certbot)
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
        
        # Timeouts
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }
}
```

Enable site:
```bash
sudo ln -s /etc/nginx/sites-available/icn /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

### Get SSL Certificate

```bash
sudo certbot --nginx -d api.your-coop.org
```

Certbot will:
- Verify domain ownership
- Install SSL certificate
- Configure nginx automatically
- Set up auto-renewal

---

## 🧪 Testing Your Deployment

### 1. Health Check

```bash
curl http://localhost:8080/v1/health
# Should return: {"status":"healthy","version":"0.1.0",...}
```

### 2. Create Test User via Mobile App

1. Open mobile app
2. Tap "Create Identity"
3. Set username and passphrase
4. Should see wallet screen with balance

### 3. Test WebSocket Connection

```bash
# Install websocat
cargo install websocat

# Connect to WebSocket
websocat ws://localhost:8080/v1/ws

# Should see connection established
```

### 4. Test from External Network

From mobile device on cellular:
```bash
# Test API (replace with your server IP)
curl http://YOUR_SERVER_IP:8080/v1/health
```

### 5. Load Test

```bash
# Install vegeta
go install github.com/tsenart/vegeta@latest

# Run load test
echo "GET http://localhost:8080/v1/health" | \
  vegeta attack -duration=30s -rate=50 | \
  vegeta report
```

---

## 📊 Monitoring

### Grafana Dashboards

Access: http://localhost:3001 (or your domain)
- Default credentials: admin / admin (change immediately!)

**Available Metrics:**
- Request rate and latency
- Payment volume
- Trust graph size
- Notification delivery
- Error rates
- WebSocket connections

### Prometheus Metrics

Access: http://localhost:9091

**Key Metrics:**
- `gateway_requests_total` - Total API requests
- `gateway_request_duration_seconds` - Request latency
- `gateway_balance_queries` - Balance lookups
- `gateway_payments_created` - Payments created
- `gateway_notifications_sent` - Notifications sent
- `gateway_websocket_connections` - Active WebSocket connections

### Log Monitoring

```bash
# Docker
docker logs -f icn-daemon

# Systemd
journalctl -u icnd -f

# Filter for errors
journalctl -u icnd -p err -f
```

---

## 🔧 Troubleshooting

### Gateway won't start

```bash
# Check logs
journalctl -u icnd -n 50

# Common issues:
# 1. Port 8080 already in use
sudo lsof -i :8080  # Find what's using it
sudo systemctl stop other-service

# 2. JWT_SECRET not set
sudo nano /etc/icn/icnd.env  # Add JWT_SECRET

# 3. Permission errors
sudo chown -R icn:icn /var/lib/icn
```

### Mobile app can't connect

```bash
# Test from mobile network
curl -v http://YOUR_SERVER_IP:8080/v1/health

# Common issues:
# 1. Firewall blocking
sudo ufw allow 8080  # Or set up nginx proxy on port 443

# 2. Using localhost instead of server IP
# Update client.ts with actual server IP/domain

# 3. CORS errors
# Check ICN_GATEWAY_CORS_ORIGINS in .env
```

### WebSocket disconnects

```bash
# Check nginx timeout settings (if using nginx)
proxy_read_timeout 3600s;  # Increase timeout

# Check connection in Grafana
# Look for gateway_websocket_connections metric
```

### Database issues

```bash
# Verify database integrity
sudo -u icn icnctl --data-dir /var/lib/icn verify

# Backup before fixes
sudo tar -czf icn-backup-$(date +%Y%m%d).tar.gz /var/lib/icn

# If corrupted, restore from backup
sudo systemctl stop icnd
sudo rm -rf /var/lib/icn/*
sudo tar -xzf icn-backup-20251212.tar.gz -C /
sudo systemctl start icnd
```

---

## 🗄️ Backup & Disaster Recovery

### Manual Backup

```bash
# Stop service
sudo systemctl stop icnd

# Backup data directory
sudo tar -czf icn-backup-$(date +%Y%m%d).tar.gz /var/lib/icn

# Restart service
sudo systemctl start icnd
```

### Automated Backups

```bash
# Add to crontab
sudo crontab -e

# Daily backup at 2 AM
0 2 * * * systemctl stop icnd && \
  tar -czf /backups/icn-$(date +\%Y\%m\%d).tar.gz /var/lib/icn && \
  systemctl start icnd

# Keep only last 30 days
0 3 * * * find /backups -name "icn-*.tar.gz" -mtime +30 -delete
```

### Restore

```bash
sudo systemctl stop icnd
sudo tar -xzf icn-backup-20251212.tar.gz -C /
sudo chown -R icn:icn /var/lib/icn
sudo systemctl start icnd
```

---

## 🚀 Next Steps After Deployment

1. **Test thoroughly** with pilot users
2. **Monitor metrics** in Grafana
3. **Collect feedback** on mobile app UX
4. **Iterate** based on usage patterns
5. **Scale** as needed (add nodes, load balancer)

### Future Enhancements

- **Push Notifications**: FCM integration (Phase 2 complete, needs API keys)
- **Multi-node P2P**: Deploy multiple ICN nodes for redundancy
- **Advanced Governance**: Domain-based voting on mobile
- **Contract Execution**: CCL contract invocation from mobile
- **Enhanced Trust UI**: Trust graph visualization and attestation flows

---

## 📚 Additional Documentation

- [Architecture](../ARCHITECTURE.md)
- [Getting Started](../GETTING_STARTED.md)
- [API Reference](../reference/api/README.md)
- [Security Hardening](../security/production-hardening.md)
- [Mobile Integration](../mobile/MOBILE_APP_STATUS.md)

---

## 🆘 Support

- GitHub Issues: https://github.com/InterCooperative-Network/icn/issues
- Documentation: `/docs` directory
- Community: [Add your links]

---

**Deployment Status: ✅ READY**

All systems operational. Backend has 1134+ passing tests. Mobile app tested with offline mode, real-time updates, and trust graph integration.

**Start with Docker Compose for testing, then move to native or Kubernetes for production!**
