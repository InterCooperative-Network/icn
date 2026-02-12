# ICN Invite System - Deployment Guide

**Date:** 2025-12-12  
**Status:** Historical deployment snapshot

> Historical deployment snapshot from 2025-12-12.
> Validate current invite-system readiness with live checks and `docs/ci/CI_CURRENT_STATUS.md`.

## Pre-Deployment Checklist

- [x] All tests passing (152/152 gateway tests)
- [x] Code committed to git (3 commits)
- [x] Docker images built
- [x] Documentation complete
- [x] Release build successful

## Quick Start

### Option 1: Local Docker Compose (Recommended for Testing)

```bash
cd /home/matt/projects/icn

# Start everything
docker-compose -f docker-compose.test.yml up -d

# Verify deployment
./scripts/verify-deployment.sh

# Access Pilot UI
open http://localhost:3000
```

### Option 2: Kubernetes Deployment

```bash
cd /home/matt/projects/icn/deploy/k8s

# Create namespace
kubectl create namespace icn

# Deploy gateway
kubectl apply -f deployment.yaml
kubectl apply -f services.yaml

# Deploy Pilot UI
./scripts/deploy-pilot-ui.sh

# Check status
kubectl get pods -n icn
kubectl get services -n icn
```

### Option 3: Manual Deployment

```bash
# Terminal 1: Start Gateway
cd /home/matt/projects/icn/icn
./target/release/icnd

# Terminal 2: Serve Pilot UI
cd /home/matt/projects/icn/web/pilot-ui
python3 -m http.server 3000
# Or: nginx, caddy, etc.
```

## Testing the Invite System

### 1. Create Test Invite (CLI)

```bash
# Login as admin
./target/release/icnctl auth login \
  --gateway http://localhost:8080 \
  --coop test-coop

# The CLI will print your token
# Save it for UI login
```

### 2. Create Invite via UI

1. Open http://localhost:3000
2. Login with admin credentials
3. Navigate to "Members" tab
4. Click "Create Invite"
5. Select role and expiration
6. Click "Create Invite"
7. Copy the generated code

### 3. Test Join Flow

1. Open new incognito window → http://localhost:3000
2. Click "Join with Invite Code"
3. Enter gateway URL: `http://localhost:8080`
4. Paste invite code
5. Click "Join Cooperative"
6. Verify credentials are generated
7. Verify auto-login works

### 4. Verify in UI

- Check Members tab shows new member
- Check Invites section shows invite as "Used"
- Verify new member can send hours

## API Endpoints

All endpoints are under `/v1/invites`:

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/v1/invites` | Admin | Create invite |
| GET | `/v1/invites?coop_id=X` | Admin | List invites |
| POST | `/v1/invites/join` | Public | Redeem invite |

### Example: Create Invite

```bash
curl -X POST http://localhost:8080/v1/invites \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "coop_id": "test-coop",
    "role": "member",
    "expires_in_seconds": 604800
  }'
```

### Example: Join via Invite

```bash
curl -X POST http://localhost:8080/v1/invites/join \
  -H "Content-Type: application/json" \
  -d '{
    "invite_code": "ABCD1234EFGH",
    "did": "did:icn:YOUR_DID"
  }'
```

## Monitoring

### Prometheus Metrics

Metrics exposed at `http://localhost:8080/metrics`:

```
icn_gateway_invites_created_total{coop_id="test-coop"} 5
icn_gateway_invites_used_total{coop_id="test-coop"} 3
```

### Logs

Gateway logs include:
- Invite creation events
- Join attempts (success/failure)
- Validation errors
- Rate limiting events

Example:
```
INFO icn_gateway::api::invites: Invite created code=ABC123 coop=test-coop role=member
INFO icn_gateway::api::invites: Invite redeemed code=ABC123 did=did:icn:xyz
```

## Troubleshooting

### Issue: "Invalid invite code"

**Cause:** Code doesn't exist, already used, or expired

**Fix:**
```bash
# Check invite status in UI (Members tab)
# Or query via API:
curl http://localhost:8080/v1/invites?coop_id=test-coop \
  -H "Authorization: Bearer YOUR_TOKEN"
```

### Issue: "Failed to generate keypair"

**Cause:** Browser doesn't support Web Crypto API

**Fix:** Use modern browser (Chrome, Firefox, Safari) over HTTPS or localhost

### Issue: Join button disabled

**Cause:** Gateway URL or invite code missing/invalid

**Fix:** Verify both fields filled, code is 12 characters

### Issue: Gateway not responding

**Check:**
```bash
# Verify gateway is running
curl http://localhost:8080/v1/health

# Check logs
tail -f /var/log/icnd.log

# Restart if needed
systemctl restart icnd
```

## Security Checklist

Before production deployment:

- [ ] Use HTTPS for gateway (TLS certificate)
- [ ] Use HTTPS for Pilot UI (TLS certificate)
- [ ] Configure CORS properly (gateway)
- [ ] Set rate limits appropriately
- [ ] Review invite expiration policy
- [ ] Enable audit logging
- [ ] Set up metrics monitoring
- [ ] Configure backup for invite data
- [ ] Test with multiple browsers/devices
- [ ] Verify mobile responsiveness

## Rollback Procedure

If issues occur:

```bash
# Revert git commits
git revert HEAD~3..HEAD

# Rebuild without invite system
cd icn/
cargo build --release

# Redeploy old version
docker-compose down
docker-compose up -d
```

## Performance Tuning

### Gateway

```toml
# icn.toml
[gateway]
rate_limit_capacity = 100
rate_limit_refill_rate = 10
max_invites_per_coop = 1000
```

### Pilot UI

```nginx
# Cache static assets
location ~* \.(js|css)$ {
    expires 7d;
    add_header Cache-Control "public, immutable";
}
```

## Next Steps

After successful deployment:

1. **Beta Testing**
   - Invite 5-10 users to test
   - Use beta feedback template
   - Monitor metrics for issues

2. **Documentation**
   - Update user guide with invite flow
   - Create video tutorial
   - Add FAQ entries

3. **Enhancements**
   - QR code generation
   - Email integration
   - Batch invites
   - Custom roles

## Support

For issues or questions:

- Check logs: `journalctl -u icnd -f`
- Review metrics: `http://localhost:8080/metrics`
- Check documentation: `INVITE_SYSTEM_COMPLETE.md`
- Run verification: `./scripts/verify-deployment.sh`

## Verification Script

Run pre-deployment checks:

```bash
./scripts/verify-deployment.sh
```

Expected output:
```
✓ Gateway health endpoint
✓ Gateway status is ok
✓ Pilot UI accessible
✓ Join screen present
✓ Invite creation modal present
✓ Invite JavaScript loaded
✓ Invite CSS loaded
✓ Gateway binary exists
✓ CLI binary exists
✓ Pilot UI Docker image exists
✓ No uncommitted changes
✓ Invite system committed

🎉 All checks passed! Ready for deployment.
```

---

**Status:** ✅ ALL SYSTEMS GO

Deploy with confidence! 🚀
