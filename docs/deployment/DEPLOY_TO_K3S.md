# ICN Invite System - K3s Deployment Guide

**Infrastructure:** K3s on Hyperion (10.8.10.40-42)  
**Date:** 2025-12-12  
**Status:** Ready to Deploy

---

## 🎯 Quick Deploy (5 Minutes)

### Prerequisites Check

```bash
# 1. SSH access works
ssh ubuntu@10.8.10.40 "echo 'SSH OK'"

# 2. In ICN repo with latest code
cd /home/matt/projects/icn
git status  # Should show: Your branch is up to date with 'origin/main'

# 3. Docker is running
docker --version
```

### Deploy Everything (One Command)

```bash
cd /home/matt/projects/icn/deploy/k8s

# Build + Deploy ICN daemon + Pilot UI
make full-deploy-with-ui
```

This single command will:
1. ✅ Build `icn:latest` Docker image from source
2. ✅ Build `icn-pilot-ui:latest` Docker image
3. ✅ Sync both images to all K3s nodes (10.8.10.40-42)
4. ✅ Deploy ICN daemon to Kubernetes
5. ✅ Deploy Pilot UI to Kubernetes
6. ✅ Apply all configs, secrets, services

### Verify Deployment

```bash
# Check status
make status

# Watch logs (daemon)
make logs

# Check Pilot UI
make ui-logs

# Test access
curl http://10.8.10.40:30080/v1/health  # Gateway API
curl http://10.8.10.40:30030/           # Pilot UI
```

---

## 📋 Step-by-Step Deployment

If you prefer to do it step-by-step or if something fails:

### Step 1: Build Images (2 min)

```bash
cd /home/matt/projects/icn/deploy/k8s

# Build ICN daemon image
make build-image

# Build Pilot UI image
make build-ui-image

# Verify
docker images | grep icn
```

Expected output:
```
icn-pilot-ui    latest    169e2ecb0455    3 minutes ago    44.7MB
icn             latest    a1b2c3d4e5f6    5 minutes ago    450MB
```

### Step 2: Sync to K3s Nodes (1 min)

```bash
# Sync daemon image to all nodes
make sync-image

# Sync UI image to all nodes
make sync-ui-image

# Or sync both at once
make sync-all-images
```

This copies images to:
- k3s-control (10.8.10.40)
- k3s-worker-1 (10.8.10.41)
- k3s-worker-2 (10.8.10.42)

### Step 3: Configure Secrets (First Time Only)

```bash
cd /home/matt/projects/icn/deploy/k8s

# Create secret file (if not exists)
if [ ! -f secret.yaml ]; then
  cp secret.yaml.example secret.yaml
  # Edit with your passphrase
  nano secret.yaml
fi

# Deploy secrets
ssh ubuntu@10.8.10.40 "sudo kubectl apply -f -" < secret.yaml
```

### Step 4: Deploy to Kubernetes (1 min)

```bash
# Deploy everything
make deploy

# Or deploy individually:
make deploy-daemon    # ICN daemon
make deploy-ui        # Pilot UI
```

### Step 5: Verify Everything Works

```bash
# Check all pods are running
ssh ubuntu@10.8.10.40 "sudo kubectl get pods -n icn"

# Expected output:
# NAME                              READY   STATUS    RESTARTS   AGE
# icn-daemon-xxxxxxxxxx-xxxxx       1/1     Running   0          2m
# pilot-ui-xxxxxxxxxx-xxxxx         1/1     Running   0          2m

# Check services
ssh ubuntu@10.8.10.40 "sudo kubectl get svc -n icn"

# Expected output:
# NAME              TYPE        CLUSTER-IP      EXTERNAL-IP   PORT(S)
# icn-rpc           ClusterIP   10.43.x.x       <none>        9090/TCP
# icn-gateway       NodePort    10.43.x.x       <none>        8080:30080/TCP
# icn-metrics       ClusterIP   10.43.x.x       <none>        9091/TCP
# pilot-ui          NodePort    10.43.x.x       <none>        3000:30030/TCP
```

---

## 🔗 Access Points

After deployment, the services are available at:

### ICN Gateway API
- **URL:** `http://10.8.10.40:30080`
- **Endpoints:**
  - Health: `http://10.8.10.40:30080/v1/health`
  - Invites: `http://10.8.10.40:30080/v1/invites`
  - WebSocket: `ws://10.8.10.40:30080/v1/ws`

### Pilot UI
- **URL:** `http://10.8.10.40:30030`
- **Features:**
  - Login screen
  - Join via invite code
  - Dashboard
  - Members management
  - Invite creation (admin)

### Metrics (Prometheus)
- **URL:** `http://10.8.10.40:30091/metrics`

---

## 🧪 Testing the Invite System

### 1. Get Admin Token

```bash
# SSH into K3s control node
ssh ubuntu@10.8.10.40

# Get pod name
POD=$(sudo kubectl get pods -n icn -l component=daemon -o jsonpath='{.items[0].metadata.name}')

# Get admin credentials (from pod logs or create new)
sudo kubectl exec -it -n icn $POD -- icnctl id show

# Login to get token
sudo kubectl exec -it -n icn $POD -- icnctl auth login \
  --gateway http://localhost:8080 \
  --coop test-coop
```

Save the token that's printed!

### 2. Create Invite via UI

1. Open browser: `http://10.8.10.40:30030`
2. Login with:
   - Gateway URL: `http://10.8.10.40:30080`
   - Coop ID: `test-coop`
   - Your DID: (from step 1)
   - Token: (from step 1)
3. Navigate to **Members** tab
4. Click **Create Invite**
5. Select role: `member`
6. Expiration: `7 days`
7. Click **Create Invite**
8. Copy the invite code

### 3. Test Join Flow

1. Open incognito window: `http://10.8.10.40:30030`
2. Click **Join with Invite Code**
3. Enter:
   - Gateway URL: `http://10.8.10.40:30080`
   - Invite Code: (paste from step 2)
4. Click **Join Cooperative**
5. Verify:
   - ✅ Credentials are generated
   - ✅ Success message appears
   - ✅ Auto-login happens
   - ✅ Can see dashboard

### 4. Verify Metrics

```bash
# Check invite metrics
curl http://10.8.10.40:30091/metrics | grep invite

# Expected:
# icn_gateway_invites_created_total{coop_id="test-coop"} 1
# icn_gateway_invites_used_total{coop_id="test-coop"} 1
```

---

## 🔧 Makefile Commands Reference

All commands from `/home/matt/projects/icn/deploy/k8s/`:

### Build & Sync
```bash
make build-image          # Build ICN daemon
make build-ui-image       # Build Pilot UI
make sync-image          # Sync daemon to K3s nodes
make sync-ui-image       # Sync UI to K3s nodes
make sync-all-images     # Sync both images
```

### Deploy
```bash
make deploy              # Deploy both daemon and UI
make deploy-daemon       # Deploy ICN daemon only
make deploy-ui           # Deploy Pilot UI only
make full-deploy-dev     # Build + sync + deploy (daemon)
make full-deploy-with-ui # Build + sync + deploy (both)
```

### Monitor
```bash
make status              # Show pod status
make logs                # Tail daemon logs
make ui-logs             # Tail UI logs
make describe            # Detailed pod info
make events              # Show cluster events
```

### Manage
```bash
make restart             # Restart daemon
make restart-ui          # Restart UI
make delete              # Delete daemon
make delete-ui           # Delete UI
make clean               # Delete all resources
```

### Debug
```bash
make shell               # Shell into daemon pod
make ui-shell            # Shell into UI pod
make port-forward        # Forward gateway to localhost:8080
make ui-port-forward     # Forward UI to localhost:3000
```

---

## 🐛 Troubleshooting

### Issue: Pods not starting

```bash
# Check pod status
ssh ubuntu@10.8.10.40 "sudo kubectl get pods -n icn"

# Check pod logs
ssh ubuntu@10.8.10.40 "sudo kubectl logs -n icn -l component=daemon"

# Check events
ssh ubuntu@10.8.10.40 "sudo kubectl get events -n icn --sort-by='.lastTimestamp'"
```

### Issue: Image not found

```bash
# Verify image exists on nodes
ssh ubuntu@10.8.10.40 "sudo ctr -n k8s.io images ls | grep icn"
ssh ubuntu@10.8.10.41 "sudo ctr -n k8s.io images ls | grep icn"
ssh ubuntu@10.8.10.42 "sudo ctr -n k8s.io images ls | grep icn"

# Re-sync if missing
cd /home/matt/projects/icn/deploy/k8s
make sync-all-images
```

### Issue: Can't access services

```bash
# Check services are exposed
ssh ubuntu@10.8.10.40 "sudo kubectl get svc -n icn"

# Check NodePorts
# Gateway should be on port 30080
# Pilot UI should be on port 30030

# Test from local machine
curl -v http://10.8.10.40:30080/v1/health
curl -v http://10.8.10.40:30030/
```

### Issue: Invite creation fails

```bash
# Check gateway logs
ssh ubuntu@10.8.10.40 "sudo kubectl logs -n icn -l component=daemon | grep invite"

# Verify token is valid
# Verify user has admin role
# Check gateway metrics endpoint
curl http://10.8.10.40:30091/metrics | grep auth
```

---

## 🔄 Update Workflow

When you make code changes:

```bash
# 1. Commit and push changes
git add .
git commit -m "feat: your changes"
git push origin main

# 2. Rebuild and redeploy
cd deploy/k8s
make full-deploy-with-ui

# 3. Verify
make status
make logs
```

The entire cycle takes about 3-5 minutes.

---

## 📊 Monitoring

### Check Health

```bash
# Gateway health
curl http://10.8.10.40:30080/v1/health

# Prometheus metrics
curl http://10.8.10.40:30091/metrics | grep icn_gateway
```

### View Logs

```bash
# Live daemon logs
ssh ubuntu@10.8.10.40 "sudo kubectl logs -f -n icn -l component=daemon"

# Live UI logs
ssh ubuntu@10.8.10.40 "sudo kubectl logs -f -n icn -l component=pilot-ui"

# Last 100 lines
ssh ubuntu@10.8.10.40 "sudo kubectl logs --tail=100 -n icn -l component=daemon"
```

### Resource Usage

```bash
# Pod resource usage
ssh ubuntu@10.8.10.40 "sudo kubectl top pods -n icn"

# Node resource usage
ssh ubuntu@10.8.10.40 "sudo kubectl top nodes"
```

---

## 🔐 Security Notes

- **Secrets:** Never commit `secret.yaml` to git
- **Access:** NodePorts (30080, 30030) are exposed on LAN only
- **Tokens:** Expire after 24 hours
- **Invites:** Single-use, time-limited
- **TLS:** Add reverse proxy (nginx/traefik) for HTTPS in production

---

## 🚀 Production Deployment (Future)

For production with domain names and TLS:

1. **Add Ingress Controller**
   ```bash
   # Install traefik/nginx ingress
   # Configure TLS certificates
   # Set up domains: api.yourdomain.com, app.yourdomain.com
   ```

2. **Update Configurations**
   ```bash
   # Change GATEWAY_BASE_URL in UI
   # Update CORS settings in gateway
   # Enable rate limiting
   ```

3. **Add Monitoring**
   ```bash
   # Prometheus + Grafana
   # Alert manager
   # Log aggregation
   ```

---

## 📝 Quick Reference Card

```bash
# Most common commands:
cd /home/matt/projects/icn/deploy/k8s

# Deploy everything
make full-deploy-with-ui

# Check status  
make status

# View logs
make logs

# Restart if needed
make restart

# Access points
Gateway: http://10.8.10.40:30080
UI:      http://10.8.10.40:30030
Metrics: http://10.8.10.40:30091/metrics
```

---

**Status:** Ready to deploy! Run `make full-deploy-with-ui` to get started. 🚀
