# ICN Quick Deploy Reference Card

**Last Updated:** December 12, 2025  
**Status:** ✅ DEPLOYMENT READY

---

## 🚀 Deploy in 5 Minutes

### Step 1: Clone Repository
```bash
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/deploy
```

### Step 2: Quick Start
```bash
./quickstart.sh "My Cooperative"
```

### Step 3: Access Services
- **Gateway API:** http://localhost:8080
- **Web UI:** http://localhost:3000
- **Grafana:** http://localhost:3001 (admin/admin)
- **Prometheus:** http://localhost:9091

### Step 4: Test Health
```bash
curl http://localhost:8080/v1/health
```

**Expected Response:**
```json
{"status":"healthy","version":"0.1.0","uptime_secs":...}
```

---

## 📱 Configure Mobile App

Edit `sdk/react-native/examples/CoopWallet/src/client.ts`:

```typescript
const client = createMobileClient({
  baseUrl: 'http://YOUR_SERVER_IP:8080',  // Change from localhost
  wallet,
  storage,
});
```

---

## 🔒 Production Setup

### 1. Generate JWT Secret
```bash
openssl rand -hex 32
# Copy output and set in .env
```

### 2. Configure Domain
```bash
# Point DNS to your server
api.your-coop.org → YOUR_SERVER_IP
```

### 3. Install SSL Certificate
```bash
sudo apt-get install certbot python3-certbot-nginx
sudo certbot --nginx -d api.your-coop.org
```

### 4. Update Mobile App
```typescript
baseUrl: 'https://api.your-coop.org'  // Use HTTPS
```

---

## 🧪 Quick Test

### Backend Health
```bash
curl http://localhost:8080/v1/health
```

### WebSocket Connection
```bash
# Install websocat: cargo install websocat
websocat ws://localhost:8080/v1/ws
```

### Mobile App
1. Update API endpoint
2. Build: `npx react-native run-ios`
3. Register new user
4. Send payment
5. Check notifications

---

## 📊 Monitoring

### Grafana Dashboard
- **URL:** http://localhost:3001
- **Login:** admin / admin (CHANGE THIS!)
- **Dashboards:** ICN Node Dashboard

### Key Metrics
- `gateway_requests_total` - API requests
- `gateway_websocket_connections` - Active connections
- `gateway_payments_created` - Payment volume
- `gateway_notifications_sent` - Notification delivery

---

## 🔧 Common Commands

### Docker Management
```bash
# View logs
docker logs -f icn-daemon

# Restart service
docker restart icn-daemon

# Stop all
cd deploy && docker compose down

# Remove all data (DESTRUCTIVE!)
docker compose down -v
```

### Native Service (if installed)
```bash
# Status
sudo systemctl status icnd

# Logs
journalctl -u icnd -f

# Restart
sudo systemctl restart icnd
```

---

## 🆘 Troubleshooting

### Problem: Can't connect from mobile app
```bash
# Check firewall
sudo ufw status

# Allow port (if needed)
sudo ufw allow 8080
```

### Problem: WebSocket disconnects
```bash
# Check logs
docker logs icn-daemon | grep -i websocket

# Increase nginx timeout if using reverse proxy
proxy_read_timeout 3600s;
```

### Problem: Gateway won't start
```bash
# Check logs
docker logs icn-daemon

# Common: Port already in use
sudo lsof -i :8080

# Common: JWT_SECRET not set
cat deploy/.env | grep JWT_SECRET
```

---

## 📚 Full Documentation

- **Deployment Guide:** [DEPLOYMENT_READY.md](DEPLOYMENT_READY.md)
- **Mobile Integration:** [MOBILE_APP_STATUS.md](../mobile/MOBILE_APP_STATUS.md)
- **Architecture:** [ARCHITECTURE.md](../ARCHITECTURE.md)
- **Security:** [production-hardening.md](../security/production-hardening.md)

---

## ✅ Production Checklist

- [ ] JWT_SECRET is random (not default)
- [ ] HTTPS configured with valid SSL
- [ ] Grafana password changed
- [ ] Firewall enabled (80/443 only)
- [ ] Backups configured
- [ ] DNS pointing to server
- [ ] Mobile app using production URL
- [ ] Test from mobile network
- [ ] Monitor Grafana dashboards
- [ ] Health checks passing

---

## 🎯 Success Criteria

After deployment, verify:

✅ `curl http://localhost:8080/v1/health` returns 200  
✅ Grafana shows metrics flowing  
✅ Mobile app can register user  
✅ Mobile app can send payment  
✅ WebSocket real-time updates work  
✅ Offline mode syncs correctly  
✅ No errors in `docker logs icn-daemon`  

---

## 📞 Support

- **Issues:** https://github.com/InterCooperative-Network/icn/issues
- **Docs:** `/docs` directory in repository

---

**Ready? Run this now:**

```bash
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/deploy
./quickstart.sh "My Cooperative"
```

🚀 **You're live in 5 minutes!**
