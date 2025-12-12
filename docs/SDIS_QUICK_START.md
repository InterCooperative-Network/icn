# SDIS Quick Start Guide

## 🚀 Getting Started with SDIS

SDIS (Secure Distributed Identity System) enables secure multi-device identity management and recovery for ICN.

---

## For End Users

### Enrolling a New Device

1. **On your NEW device**:
   - Open Pilot UI
   - Navigate to "Identity" → "Enroll Device"
   - Enter your root DID
   - Choose a device name
   - Click "Start Enrollment"

2. **Transfer the challenge**:
   - Scan the QR code with your existing device, OR
   - Copy/paste the challenge code

3. **On your EXISTING device**:
   - Open enrollment approval page
   - Paste challenge (if not scanned)
   - Click "Approve Enrollment"

4. **Back on NEW device**:
   - Wait for approval confirmation
   - ✅ Device is now enrolled!

### Adding Recovery Anchors

**Why?** Recovery anchors let you recover your identity if you lose all devices.

1. Open Pilot UI → "Identity" → "Recovery Anchors"
2. Click "Add Anchor"
3. Choose type:
   - **Device**: Another device you own
   - **Contact**: A trusted friend's device
4. Enter label (e.g., "My Phone")
5. Paste the public key
6. Click "Add Anchor"

**Best Practice**: Add at least 2-3 anchors (mix of devices and contacts).

### Recovering Your Identity

**If you lose all your devices**:

1. On a new device, open Pilot UI
2. Navigate to "Identity" → "Recover Identity"
3. Enter your root DID
4. The system will notify your recovery anchors
5. Ask your trusted contacts to approve on their devices
6. Once threshold is met (e.g., 2 of 3), recovery completes
7. ✅ Your identity is restored!

---

## For Developers

### API Examples

#### Enroll a Device

```bash
# Step 1: Request enrollment
curl -X POST http://localhost:8080/api/v1/sdis/enroll \
  -H "Content-Type: application/json" \
  -d '{
    "root_did": "did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH",
    "device_did": "did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktX",
    "device_label": "My Laptop",
    "device_pubkey": "MCowBQYDK2VwAyEA..."
  }'

# Response:
{
  "enrollment_id": "enr_abc123",
  "challenge": "YXNkZmFzZGZhc2RmYXNkZg==",
  "expires_at": 1704067200
}

# Step 2: Approve (on existing device)
curl -X POST http://localhost:8080/api/v1/sdis/enroll/enr_abc123/approve \
  -H "Content-Type: application/json" \
  -d '{
    "signature": "base64_ed25519_signature"
  }'

# Response:
{
  "status": "approved",
  "proof_id": "proof_xyz789"
}

# Step 3: Verify (on new device)
curl http://localhost:8080/api/v1/sdis/enroll/enr_abc123

# Response:
{
  "enrollment_id": "enr_abc123",
  "approved": true,
  "approved_at": 1704063600
}
```

#### Add Recovery Anchor

```bash
curl -X POST http://localhost:8080/api/v1/sdis/anchors \
  -H "Content-Type: application/json" \
  -d '{
    "anchor_type": "device",
    "label": "My Phone",
    "pubkey": "MCowBQYDK2VwAyEA..."
  }'

# Response:
{
  "anchor_id": "anc_def456",
  "created_at": 1704063600
}
```

#### List Anchors

```bash
curl http://localhost:8080/api/v1/sdis/anchors

# Response:
{
  "anchors": [
    {
      "anchor_id": "anc_def456",
      "owner_did": "did:icn:z6Mk...",
      "anchor_type": "device",
      "label": "My Phone",
      "created_at": 1704063600,
      "revoked_at": null
    }
  ]
}
```

### JavaScript SDK

```javascript
import { ICNClient } from '@icn/sdk';

const client = new ICNClient({ baseURL: 'http://localhost:8080' });

// Enroll device
const enrollment = await client.sdis.enroll({
  rootDid: 'did:icn:z6Mk...',
  deviceDid: 'did:icn:z6Mk...',
  deviceLabel: 'My Laptop',
  devicePubkey: 'MCowBQYDK2VwAyEA...'
});

console.log('Challenge:', enrollment.challenge);

// Approve enrollment
await client.sdis.approveEnrollment(enrollment.enrollmentId, {
  signature: signedChallenge
});

// Add anchor
const anchor = await client.sdis.addAnchor({
  anchorType: 'device',
  label: 'My Phone',
  pubkey: 'MCowBQYDK2VwAyEA...'
});

// List anchors
const anchors = await client.sdis.listAnchors();
```

---

## For Operators

### Deployment

SDIS is automatically deployed with the ICN Gateway:

```bash
cd deploy/k8s
make build deploy
```

### Configuration

Edit `config/icn.toml`:

```toml
[sdis]
# Challenge expiration (seconds)
challenge_ttl = 300

# Recovery threshold (N of M anchors required)
recovery_threshold = 0.67  # 67% = 2 of 3

# Maximum enrollments per identity
max_devices = 10

# Proof expiration (optional, 0 = never)
proof_expiration = 0
```

### Monitoring

**Key metrics** (Prometheus):

```
sdis_enrollments_total
sdis_enrollments_approved
sdis_enrollments_failed
sdis_anchors_total
sdis_anchors_revoked
sdis_recovery_initiated
sdis_recovery_completed
```

**View in Grafana**: `http://<cluster-ip>:30082`

### Troubleshooting

**Problem**: Challenge expired  
**Solution**: Re-initiate enrollment (challenges last 5 minutes)

**Problem**: Signature verification failed  
**Solution**: Ensure device_pubkey matches the key used to sign

**Problem**: Insufficient recovery anchors  
**Solution**: User needs to add more anchors before recovery

---

## Security Best Practices

### For Users
1. ✅ Add 2-3 recovery anchors minimum
2. ✅ Mix device and contact anchors
3. ✅ Revoke anchors for lost/stolen devices immediately
4. ✅ Use descriptive labels for devices
5. ✅ Review anchor list monthly

### For Developers
1. ✅ Always verify Ed25519 signatures
2. ✅ Enforce challenge expiration
3. ✅ Rate-limit enrollment attempts
4. ✅ Log all SDIS operations
5. ✅ Never transmit private keys

### For Operators
1. ✅ Monitor enrollment success rate
2. ✅ Alert on unusual recovery patterns
3. ✅ Backup proof chains regularly
4. ✅ Set appropriate thresholds
5. ✅ Provide user support channels

---

## Common Workflows

### Scenario 1: Adding a Phone

```
User has: Laptop (enrolled)
User wants: Phone enrolled

1. Open Pilot UI on phone
2. Click "Enroll Device"
3. Scan QR with laptop webcam
4. Approve on laptop
5. ✅ Phone enrolled
```

### Scenario 2: Lost Phone

```
User has: Lost phone, Laptop
User needs: Revoke phone anchor

1. Open Pilot UI on laptop
2. Go to "Recovery Anchors"
3. Find "My Phone"
4. Click "Revoke"
5. ✅ Phone can't approve recoveries
```

### Scenario 3: Lost All Devices

```
User has: Nothing
User needs: Recover identity

1. Get new device
2. Open Pilot UI
3. Click "Recover Identity"
4. Enter root DID
5. Wait for anchor approvals
6. ✅ Identity recovered
```

---

## API Quick Reference

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/v1/sdis/enroll` | POST | Start enrollment |
| `/api/v1/sdis/enroll/{id}/approve` | POST | Approve enrollment |
| `/api/v1/sdis/enroll/{id}` | GET | Check status |
| `/api/v1/sdis/anchors` | POST | Add anchor |
| `/api/v1/sdis/anchors` | GET | List anchors |
| `/api/v1/sdis/anchors/{id}/revoke` | POST | Revoke anchor |
| `/api/v1/sdis/recover/initiate` | POST | Start recovery |
| `/api/v1/sdis/recover/{id}/approve` | POST | Approve recovery |
| `/api/v1/sdis/recover/{id}` | GET | Check recovery |

---

## UI Component Reference

| Component | File | Purpose |
|-----------|------|---------|
| Enrollment Wizard | `components/enrollment-wizard.js` | Enroll new devices |
| Identity Viewer | `components/identity-viewer.js` | View enrolled devices |
| Anchor Manager | `components/anchor-manager.js` | Manage recovery anchors |
| Recovery Assistant | `components/recovery-assistant.js` | Recover lost identity |

---

## Resources

📖 **Full Documentation**: `docs/SDIS_SYSTEM.md`  
🏗️ **Architecture**: `docs/ARCHITECTURE.md`  
🔐 **Security Model**: `docs/security-model.md`  
🐛 **Report Issues**: `github.com/InterCooperative-Network/icn/issues`  
💬 **Get Help**: Join ICN Discord #sdis channel

---

**Last Updated**: December 12, 2025  
**Version**: 1.0.0  
**Status**: Production Ready ✅
