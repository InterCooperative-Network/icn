# SDIS Quick Start Guide

> Snapshot guidance: this document contains both currently wired endpoints and forward-looking flows.
> Verify live behavior against `icn/crates/icn-gateway/src/api/sdis/mod.rs` and `docs/sdis/SDIS_STATUS.md`.

> **Self-serve enrollment is not mounted by default.** The
> `POST /v1/sdis/enrollment/*` routes are unauthenticated by construction and end
> in a credential mint, so they are registered only when the operator sets
> `ICN_ENABLE_SELF_SERVE_ENROLLMENT=true` to declare an isolated rehearsal
> deployment. **No shipped deployment profile sets it** — on production, LAN,
> evaluator and demo images these routes are absent, returning **404 or 401
> depending on route fallthrough**: the `/v1/sdis` scope nests an authenticated
> sub-scope that matches remaining paths, so an unmounted enrollment path may be
> rejected by `jwt_auth` before routing rather than 404'ing. Either way no
> enrollment handler runs. Steward and moderation routes under `/v1/sdis` are
> unaffected and remain mounted behind `jwt_auth`.
>
> Two further constraints apply wherever enrollment *is* mounted: a level-2 vouch
> requires a credential issued for the same cooperative as the enrollment, and
> completion fails rather than minting a credential if any required institutional
> write (anchor, holder, jurisdiction join, membership approval) fails. This is a
> containment tranche, not the final SDIS enrollment authority model. That decision —
> whether vouching authority derives from the trust graph or from a governance
> capability — is still open and has no accepted ADR. The live proposal is the **draft
> PR** [InterCooperative-Network/icn#2450](https://github.com/InterCooperative-Network/icn/pull/2450),
> *"docs(architecture): propose institution-scoped SDIS vouch authority"*.

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

## For Stewards

### Steward Dashboard

Stewards review and vouch for identity enrollments. Access the dashboard at:
- **Web**: `http://<gateway-host>:30030/steward-dashboard.html`
- **Mobile**: CoopWallet app → Home → Steward Dashboard

### Reviewing Enrollments

1. Open the Steward Dashboard
2. View **Pending** tab for enrollments awaiting vouch
3. Click an enrollment to see details:
   - Identity name and coop
   - Current verification level
   - Expiration time
4. Choose to **Vouch** or **Reject**

### Vouching for an Identity

1. Select a pending enrollment (must be Level 1+)
2. Click **Vouch**
3. Enter your verification statement (how you verified the person)
4. Confirm the verification checklist:
   - ✅ Identity verified in person or via video call
   - ✅ Person matches their stated identity
   - ✅ No suspicious behavior observed
5. Submit your vouch

### Rejecting an Enrollment

1. Select a pending enrollment
2. Click **Reject**
3. Enter the rejection reason
4. Confirm rejection

### Steward Statistics

View your steward metrics:
- Total vouches and monthly activity
- Rejection count
- Reputation score
- Average response time

---

## For Developers

### API Examples

#### Enroll a Device

> Step 1 returns `503` unless `GATEWAY_BASE_URL` is set — the QR it issues names an origin a
> scanning device sends credentials to, and that cannot be guessed from the request
> (#2569). See [Configuration](#configuration) under *For Operators*.

```bash
# Step 1: Request enrollment
curl -X POST http://localhost:8080/v1/sdis/enrollment/start \
  -H "Content-Type: application/json" \
  -d '{
    "identity_name": "alice",
    "coop_id": "my-coop"
  }'

# Response:
{
  "enrollment_id": "enr_abc123",
  "verification_code": "ABCD-1234",
  "expires_at": "2026-02-12T12:34:56Z"
}

# Step 2: Steward records a vouch
curl -X POST http://localhost:8080/v1/sdis/vouch/enr_abc123 \
  -H "Content-Type: application/json" \
  -d '{
    "vouch_statement": "I can verify this member identity."
  }'

# Response:
{
  "status": "vouched",
  "enrollment_id": "enr_abc123",
  "level": 2
}

# Step 3: Check enrollment status
curl http://localhost:8080/v1/sdis/status/enr_abc123

# Response:
{
  "enrollment_id": "enr_abc123",
  "status": "ready_for_completion",
  "level": 2
}
```

#### Add Recovery Anchor

```bash
curl -X POST http://localhost:8080/v1/sdis/anchor/devices/add \
  -H "Content-Type: application/json" \
  -d '{
    "anchor_id": "anc_def456",
    "device_name": "My Phone",
    "device_pubkey": "MCowBQYDK2VwAyEA..."
  }'

# Response:
{
  "device_id": "dev_123",
  "device": {
    "device_name": "My Phone"
  }
}
```

#### List Anchors

```bash
curl http://localhost:8080/v1/sdis/anchor/anc_def456/devices

# Response:
{
  "devices": [
    {
      "device_id": "dev_123",
      "device_name": "My Phone"
    }
  ]
}
```

### JavaScript SDK

The SDK surface evolves independently; treat the snippet below as conceptual and verify against current `sdk/typescript` exports.

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

#### `GATEWAY_BASE_URL` — required to issue enrollment QR material

`POST /v1/sdis/enrollment/start` returns a QR payload containing a `gateway_url`. A **second
device** scans it and posts its bearer credential there, so that origin is a credential
destination and must be an operator assertion. It is not derived from the request's `Host`,
`Forwarded`, or `X-Forwarded-*` headers, and not from the bind address (#2569).

Set `GATEWAY_BASE_URL` to the externally reachable `scheme://host[:port]` a scanning phone can
reach this gateway at — normally your reverse proxy's public origin, not the bind address. It
must have no userinfo, path, query, or fragment; it is validated at use time.

With no origin configured, enrollment start **fails closed with `503`** and logs the reason.
The daemon still starts and every other route works — a gateway that issues no QR material
needs no origin. `TRUSTED_PROXY_IPS` does not substitute for it: trusting a proxy to report
the client IP (#2567) does not authorize it to assert the advertised origin.

Where each shipped profile gets it: `deploy/k8s/configmap.yaml` (`gateway_base_url`); the LAN
appliance drop-in `deploy/appliance/lan/icnd-30-lan-origin.conf.in` (from
`ICN_APPLIANCE_LAN_ORIGIN`); `ICN_DEVNET_NODE_{A,B,C}_ORIGIN` for `deploy/devnet`, which
ADR-0086 names the canonical Compose entry point; and a commented example in
`deploy/icnd.env.example` for native installs. The QEMU demo appliance profile deliberately
does not set one — it is host-only, so no phone can reach it, and QR issuance is correctly off.

The remaining Compose and Kubernetes trees (`deploy/docker-compose.yml`, `deploy/compose/`,
`deploy/kubernetes/`, `deploy/helm/icn/`) are compatibility material under ADR-0086 and are not
wired for QR. Do not add an origin to one expecting enrollment to work: the latter two cannot
bring a gateway to Ready at all, since they probe `/health/liveness` — a route that does not
exist — and set env names the daemon never reads.

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

### Simple Enrollment (with Steward Network)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v1/sdis/enrollment/start` | POST | Start new enrollment |
| `/v1/sdis/verify/level1` | POST | Verify device (Level 1) |
| `/v1/sdis/verify/level2` | POST | Steward vouch (Level 2) |
| `/v1/sdis/enrollment/complete` | POST | Complete enrollment |
| `/v1/sdis/status/{id}` | GET | Get enrollment status |
| `/v1/sdis/pending` | GET | List pending enrollments |
| `/v1/sdis/vouch/{id}` | POST | Submit steward vouch |
| `/v1/sdis/reject/{id}` | POST | Reject enrollment |
| `/v1/sdis/steward/stats` | GET | Steward statistics |
| `/v1/sdis/steward/history` | GET | Vouch history |

### Device Enrollment (Advanced)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v1/sdis/anchor/{id}` | GET | Get anchor details |
| `/v1/sdis/anchor/rotate-keys` | POST | Rotate anchor keys |
| `/v1/sdis/anchor/{id}/history` | GET | Key rotation history |
| `/v1/sdis/anchor/devices/add` | POST | Add trusted device |
| `/v1/sdis/anchor/{id}/devices` | GET | List trusted devices |
| `/v1/sdis/recovery/start` | POST | Start recovery ceremony |
| `/v1/sdis/recovery/{id}/approve` | POST | Approve recovery |
| `/v1/sdis/recovery/{id}` | GET | Check recovery status |
| `/v1/sdis/recovery/{id}/complete` | POST | Complete recovery |

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

📖 **Full Documentation**: `docs/sdis/SDIS_SYSTEM.md`  
🏗️ **Architecture**: `docs/ARCHITECTURE.md`  
🔐 **Security Model**: `docs/security/SDIS_THREAT_MODEL.md`  
🐛 **Report Issues**: `github.com/InterCooperative-Network/icn/issues`  
💬 **Get Help**: Join ICN Discord #sdis channel

---

**Last Updated**: December 13, 2025
**Version**: 1.1.0
**Status**: Production Ready ✅

**What's New in 1.1.0**:
- Steward Dashboard for enrollment review
- Web and mobile dashboard interfaces
- Steward vouch and reject workflows
- Statistics and history tracking
