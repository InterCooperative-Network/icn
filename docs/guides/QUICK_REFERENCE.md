# ICN Invite System - Quick Reference

**GitHub:** https://github.com/InterCooperative-Network/icn  
**Deployed At:** (deployment-specific URL)

---

## 📁 Key Files

### Backend (Rust)
```
icn/crates/icn-gateway/src/
  ├─ invite.rs           # InviteManager (267 lines)
  ├─ api/invites.rs      # API endpoints (172 lines)
  ├─ models.rs           # Data models (invite section)
  ├─ server.rs           # Route registration (line ~320)
  └─ validation.rs       # Role validation (line ~237)
```

### Frontend (JavaScript)
```
web/pilot-ui/
  ├─ index.html          # Join screen, invite UI
  ├─ app.js              # Invite logic (lines 4200-4475)
  └─ style.css           # Invite styles (lines 3128-3297)
```

### Mobile (TypeScript)
```
sdk/react-native/examples/CoopWallet/src/
  ├─ client.ts           # Invite API methods
  └─ config.ts           # API configuration
```

---

## 🔗 Access Points

| Service | URL | Status |
|---------|-----|--------|
| Pilot UI | (deployment-specific URL) | ✅ Running |
| Gateway API | (deployment-specific URL) | ✅ Running |
| Metrics | (deployment-specific URL)/metrics | ✅ Running |

---

## 🛠️ Common Commands

```bash
# Navigate to project (from repo root)
cd icn

# View backend code
cat crates/icn-gateway/src/invite.rs
cat crates/icn-gateway/src/api/invites.rs

# View frontend code (from repo root)
cat web/pilot-ui/app.js | grep -A 100 "Invite System"

# Check deployment
cd deploy/k8s
make status
make logs
make ui-logs

# Redeploy after changes
make full-deploy-with-ui

# View documentation
cat INVITE_SYSTEM_COMPLETE.md
cat DEPLOY_TO_K3S.md
```

---

## 🧪 Test Invite Flow

### 1. Get Admin Token
```bash
ssh ubuntu@10.8.10.40
POD=$(sudo kubectl get pods -n icn -l component=daemon -o jsonpath='{.items[0].metadata.name}')
sudo kubectl exec -it -n icn $POD -- icnctl id show
sudo kubectl exec -it -n icn $POD -- icnctl auth login \
  --gateway http://localhost:8080 --coop test-coop
```

### 2. Login to UI
- Open: http://10.8.10.40:30030
- Use credentials from step 1

### 3. Create Invite
- Members tab → Create Invite
- Select role and expiration
- Copy code

### 4. Test Join
- Incognito window → Join with Invite Code
- Paste code → Verify new identity

---

## 📊 Metrics

```bash
# Check invite metrics
curl http://10.8.10.40:30091/metrics | grep invite

# Expected output:
# icn_gateway_invites_created_total{coop_id="test-coop"} N
# icn_gateway_invites_used_total{coop_id="test-coop"} N
```

---

## 🔄 Update Workflow

When you make code changes:

```bash
# 1. Make changes (from repo root)
# ... edit files ...

# 2. Test locally
cd icn
cargo test -p icn-gateway

# 3. Commit
git add .
git commit -m "your changes"
git push origin main

# 4. Redeploy (from repo root)
cd deploy/k8s
make full-deploy-with-ui

# 5. Verify
make status
make logs
```

---

## 📖 Documentation

- `INVITE_SYSTEM_COMPLETE.md` - Implementation overview
- `DEPLOYMENT_INVITE_SYSTEM.md` - Deployment guide
- `DEPLOY_TO_K3S.md` - K3s deployment
- `INVITE_SYSTEM_FINAL_STATUS.md` - Status report

---

## 🆘 Troubleshooting

**Pods not starting?**
```bash
ssh ubuntu@10.8.10.40 "sudo kubectl get pods -n icn"
ssh ubuntu@10.8.10.40 "sudo kubectl describe pod -n icn -l component=daemon"
```

**Gateway not responding?**
```bash
curl http://10.8.10.40:30080/v1/health
ssh ubuntu@10.8.10.40 "sudo kubectl logs -n icn -l component=daemon"
```

**UI not loading?**
```bash
curl http://10.8.10.40:30030/
ssh ubuntu@10.8.10.40 "sudo kubectl logs -n icn -l component=pilot-ui"
```

**Disk space issues?**
```bash
ssh ubuntu@10.8.10.40 "df -h /"
ssh ubuntu@10.8.10.40 "sudo docker system prune -a"
```

---

## 🎯 Quick Links

- **GitHub Repo:** https://github.com/InterCooperative-Network/icn
- **Backend Code:** https://github.com/InterCooperative-Network/icn/blob/main/icn/crates/icn-gateway/src/invite.rs
- **Frontend Code:** https://github.com/InterCooperative-Network/icn/blob/main/web/pilot-ui/app.js
- **Live UI:** http://10.8.10.40:30030
- **API Docs:** See `DEPLOYMENT_INVITE_SYSTEM.md`

---

**Last Updated:** 2025-12-12  
**Version:** 0.1.0 with invite system  
**Status:** ✅ Deployed and running
