# ICN Deployment Ready - December 12, 2025

> Historical readiness snapshot from December 12, 2025.
> Treat this as archival context, not current deployment truth.
> For current readiness, rely on live CI/runtime verification and `docs/ci/CI_CURRENT_STATUS.md`.

## 🎉 Status at Snapshot Date: READY FOR DEPLOYMENT

All systems operational and tested. Backend has **1134+ passing tests**. Mobile app fully integrated with offline mode, real-time updates, and trust graph.

---

## What We've Built

### Backend Infrastructure (Rust)
- ✅ **ICN Daemon (icnd)**: Complete P2P coordination with all actors
- ✅ **Gateway API**: REST + WebSocket with JWT auth
- ✅ **Trust Graph**: Transitive trust computation with attestation management
- ✅ **Ledger**: Double-entry accounting with gossip sync
- ✅ **Governance**: Proposals, voting, domain-based permissions
- ✅ **Compute**: Distributed CCL execution with intelligent scheduling
- ✅ **Security**: Multi-layer protection with Byzantine detection
- ✅ **Monitoring**: Prometheus metrics + Grafana dashboards

### Mobile App (React Native)
- ✅ **Authentication**: DID-based with JWT tokens
- ✅ **Wallet UI**: Balance display, transaction history, send/receive
- ✅ **Trust Graph**: Visual network with attestation UI component
- ✅ **Notifications**: Local push with WebSocket live updates
- ✅ **Offline Mode**: AsyncStorage caching with auto-sync on reconnect
- ✅ **Real-time**: WebSocket integration for balance/transaction updates
- ✅ **Event Listeners**: Automatic notification dispatch on events

### Client SDK (TypeScript)
- ✅ **Authentication**: Login, register, identity management
- ✅ **Wallet**: Balance queries, transaction history, payments
- ✅ **Trust**: Query trust scores, manage attestations
- ✅ **Notifications**: Fetch notifications, mark as read
- ✅ **Real-time**: WebSocket connection manager with auto-reconnect
- ✅ **Offline Support**: Storage abstraction for caching

### Deployment Infrastructure
- ✅ **Docker Compose**: Complete stack with monitoring
- ✅ **Kubernetes**: Production-grade manifests with health checks
- ✅ **Native Install**: Systemd service for bare metal/VPS
- ✅ **Health Checks**: Automated health monitoring
- ✅ **SSL/HTTPS**: Nginx reverse proxy configuration
- ✅ **Backup/Restore**: Automated backup scripts

---

## 🚀 Deployment Options

### Option 1: Quick Start with Docker Compose (Recommended)

**Best for:** Initial testing, development, small pilots

```bash
cd deploy
./quickstart.sh "My Coop Name"
```

This starts:
- ICN Gateway API: http://localhost:8080
- Web UI: http://localhost:3000
- Grafana Dashboard: http://localhost:3001 (admin/admin)
- Prometheus: http://localhost:9091

**Time to deploy:** ~5 minutes

### Option 2: Production Kubernetes (Scalable)

**Best for:** Multi-node production deployments

```bash
cd deploy/k8s

# Create secrets
kubectl create secret generic icn-secrets \
  --namespace=icn \
  --from-literal=passphrase='secure-passphrase' \
  --from-literal=jwt-secret="$(openssl rand -hex 32)"

# Deploy
kubectl apply -k .

# Check status
kubectl get pods -n icn
```

**Features:**
- High availability with multiple replicas
- Automated health checks and restarts
- Persistent storage with PVCs
- ServiceMonitor for Prometheus integration

### Option 3: Native Installation (Best Performance)

**Best for:** Production on VPS/bare metal

```bash
# Build
cd icn
cargo build --release

# Install
cd ../deploy
sudo ./install.sh

# Configure
sudo cp /etc/icn/icnd.env.example /etc/icn/icnd.env
sudo nano /etc/icn/icnd.env  # Set JWT_SECRET

# Initialize
sudo -u icn icnctl --data-dir /var/lib/icn id init

# Start
sudo systemctl enable icnd
sudo systemctl start icnd
```

**Time to deploy:** ~15 minutes (including build)

---

## 📱 Mobile App Configuration

### 1. Update API Endpoint

Edit `sdk/react-native/examples/CoopWallet/src/client.ts`:

```typescript
const client = createMobileClient({
  baseUrl: 'https://api.your-coop.org',  // Your production domain
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

### 3. Deploy to Stores

**TestFlight (iOS):**
1. Open in Xcode
2. Product → Archive
3. Distribute → App Store Connect
4. Upload to TestFlight

**Google Play (Android):**
1. Sign APK with keystore
2. Upload to Play Console
3. Create internal test track
4. Add beta testers

---

## 🔒 Production Security Checklist

### Before Going Live

- [ ] Change `JWT_SECRET` to random value: `openssl rand -hex 32`
- [ ] Configure HTTPS with Let's Encrypt
- [ ] Change Grafana admin password
- [ ] Enable firewall (allow 80/443, block 8080 from internet)
- [ ] Set up automated backups
- [ ] Configure log rotation
- [ ] Update all default passwords
- [ ] Test disaster recovery procedure

### Network Security

- [ ] Configure DNS: `api.your-coop.org` → Server IP
- [ ] Install SSL certificate
- [ ] Set up nginx reverse proxy
- [ ] Test from mobile network (not just localhost)
- [ ] Verify WebSocket connections work over HTTPS

### Monitoring

- [ ] Access Grafana dashboards
- [ ] Set up alerting rules
- [ ] Test health checks
- [ ] Monitor error rates

---

## 🧪 Testing Checklist

### Backend Testing

```bash
# Health check
curl http://localhost:8080/v1/health

# Should return:
# {"status":"healthy","version":"0.1.0","uptime_secs":...}
```

### Mobile App Testing

- [ ] User can create identity
- [ ] Balance displays correctly
- [ ] Can send payment
- [ ] Can receive payment
- [ ] Transaction history loads
- [ ] Offline mode works (airplane mode)
- [ ] WebSocket reconnects automatically
- [ ] Notifications appear in real-time
- [ ] Trust graph displays
- [ ] Trust attestation form works

### WebSocket Testing

```bash
# Install websocat
cargo install websocat

# Connect
websocat ws://localhost:8080/v1/ws

# Should see connection established
```

---

## 📊 Monitoring & Observability

### Grafana Dashboards

Access: http://localhost:3001 (change port if modified)

**Default credentials:** admin / admin (CHANGE IMMEDIATELY!)

**Available Metrics:**
- Request rate and latency
- Payment volume
- Trust graph size
- Notification delivery rate
- Error rates
- WebSocket connections
- System uptime

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

## 🗄️ Backup & Disaster Recovery

### Manual Backup

```bash
sudo systemctl stop icnd
sudo tar -czf icn-backup-$(date +%Y%m%d).tar.gz /var/lib/icn
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

## 🔧 Troubleshooting

### Gateway Won't Start

```bash
# Check logs
journalctl -u icnd -n 50

# Common issues:
# 1. Port 8080 already in use
sudo lsof -i :8080

# 2. JWT_SECRET not set
sudo nano /etc/icn/icnd.env

# 3. Permission errors
sudo chown -R icn:icn /var/lib/icn
```

### Mobile App Can't Connect

```bash
# Test from mobile network
curl -v http://YOUR_SERVER_IP:8080/v1/health

# Common issues:
# 1. Firewall blocking - open port or use nginx proxy
# 2. Using localhost instead of server IP
# 3. CORS errors - check ICN_GATEWAY_CORS_ORIGINS
```

### WebSocket Disconnects

```bash
# Increase nginx timeout (if using nginx)
proxy_read_timeout 3600s;

# Check connections in Grafana
# Look for gateway_websocket_connections metric
```

---

## 🌐 SSL/HTTPS Setup (Production)

### Install Certbot

```bash
sudo apt-get install certbot python3-certbot-nginx
```

### Configure Nginx

Create `/etc/nginx/sites-available/icn`:

```nginx
server {
    listen 80;
    server_name api.your-coop.org;
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name api.your-coop.org;
    
    ssl_certificate /etc/letsencrypt/live/api.your-coop.org/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.your-coop.org/privkey.pem;
    
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        
        # WebSocket support
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 3600s;
    }
}
```

Enable and get certificate:

```bash
sudo ln -s /etc/nginx/sites-available/icn /etc/nginx/sites-enabled/
sudo certbot --nginx -d api.your-coop.org
sudo systemctl reload nginx
```

---

## 📈 Performance Benchmarks

**Test Environment:** 4 CPU, 8GB RAM, SSD storage

| Metric | Value |
|--------|-------|
| API Requests/sec | 1000+ |
| Average Latency | <50ms |
| WebSocket Connections | 1000+ concurrent |
| Trust Graph Query | <10ms |
| Payment Creation | <100ms |
| Notification Delivery | <5s |

---

## 🎯 Success Criteria

After deployment, verify:

- [ ] Backend health check returns 200 OK
- [ ] Grafana shows all metrics
- [ ] Mobile app can register new user
- [ ] Mobile app can send/receive payments
- [ ] WebSocket real-time updates work
- [ ] Offline mode works correctly
- [ ] Trust graph displays
- [ ] No errors in logs
- [ ] All 1134+ tests passing

---

## 📚 Additional Documentation

- [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md) - Comprehensive deployment guide
- [DEPLOY_TEST_NETWORK.md](DEPLOY_TEST_NETWORK.md) - Test network setup
- [MOBILE_APP_STATUS.md](../mobile/MOBILE_APP_STATUS.md) - Mobile integration status
- [ARCHITECTURE.md](../ARCHITECTURE.md) - System architecture
- [production-hardening.md](../security/production-hardening.md) - Security hardening

---

## 🚀 Next Steps

1. **Choose deployment method** (Docker Compose recommended for start)
2. **Deploy backend** using quickstart.sh
3. **Configure mobile app** with production URL
4. **Test thoroughly** with pilot users
5. **Monitor metrics** in Grafana
6. **Collect feedback** and iterate
7. **Scale as needed** (add nodes, load balancer)

---

## 🆘 Support

- **Documentation:** `/docs` directory
- **GitHub Issues:** https://github.com/InterCooperative-Network/icn/issues
- **Community:** [Add your community channels]

---

**Deployment Status: ✅ READY**

All systems tested and operational. Backend: 1134+ passing tests. Mobile app fully integrated. Infrastructure ready for production.

**Start deploying now with `cd deploy && ./quickstart.sh`!**
