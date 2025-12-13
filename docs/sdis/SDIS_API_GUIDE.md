# SDIS API Guide

Complete guide to using the Secure Distributed Identity System (SDIS) API.

## Base URL

```
http://10.8.10.40:30080/v1/sdis
```

## Authentication

Most SDIS endpoints require authentication via Bearer token:

```
Authorization: Bearer <your-token>
```

To get a token:
```bash
POD=$(kubectl get pods -n icn -l component=daemon -o jsonpath='{.items[0].metadata.name}')
kubectl exec -it -n icn $POD -- icnctl auth token --coop-id <COOP_ID> --gateway http://localhost:8080
```

## API Endpoints

### 1. Health Check

Check if SDIS is operational.

```bash
curl http://10.8.10.40:30080/v1/sdis/health
```

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2025-12-12T22:00:00Z"
}
```

---

### 2. Enrollment

#### Start Enrollment

Begin the SDIS enrollment process to create a new identity.

**POST** `/v1/sdis/enrollment/start`

```bash
curl -X POST http://10.8.10.40:30080/v1/sdis/enrollment/start \
  -H "Content-Type: application/json" \
  -d '{
    "identity_name": "Alice",
    "coop_id": "my-coop"
  }'
```

**Response:**
```json
{
  "enrollment_id": "enroll_abc123...",
  "expires_at": "2025-12-12T23:00:00Z",
  "qr_code": "data:image/png;base64,...",
  "verification_code": "VERIFY-1234"
}
```

#### Complete Enrollment

Finalize enrollment after verification.

**POST** `/v1/sdis/enrollment/complete`

```bash
curl -X POST http://10.8.10.40:30080/v1/sdis/enrollment/complete \
  -H "Content-Type: application/json" \
  -d '{
    "enrollment_id": "enroll_abc123...",
    "ephemeral_did": "did:icn:z...",
    "ephemeral_signature": "base64-signature",
    "device_info": {
      "device_type": "smartphone",
      "os": "Android",
      "app_version": "1.0.0"
    }
  }'
```

**Response:**
```json
{
  "did": "did:icn:z9AWguvsTEkAVXkpQrHWthPuK86Tw3c8DunToVWLJeP4s",
  "recovery_codes": ["CODE1", "CODE2", "CODE3"],
  "auth_token": "Bearer eyJ..."
}
```

---

### 3. Verification Levels

#### Level 1 Verification (Basic)

Verify device possession via QR scan.

**POST** `/v1/sdis/verify/level1`

```bash
curl -X POST http://10.8.10.40:30080/v1/sdis/verify/level1 \
  -H "Content-Type: application/json" \
  -d '{
    "enrollment_id": "enroll_abc123...",
    "device_proof": "base64-proof"
  }'
```

#### Level 2 Verification (Steward Vouching)

Get vouched by a trusted steward.

**POST** `/v1/sdis/verify/level2`

```bash
curl -X POST http://10.8.10.40:30080/v1/sdis/verify/level2 \
  -H "Authorization: Bearer <steward-token>" \
  -H "Content-Type: application/json" \
  -d '{
    "enrollment_id": "enroll_abc123...",
    "vouch_statement": "I vouch for this person"
  }'
```

---

### 4. Anchor Device Management

#### List Anchors

Get all anchor devices for your identity.

**GET** `/v1/sdis/anchors`

```bash
curl http://10.8.10.40:30080/v1/sdis/anchors \
  -H "Authorization: Bearer <your-token>"
```

**Response:**
```json
{
  "anchors": [
    {
      "device_id": "device_001",
      "device_name": "Primary Phone",
      "device_type": "smartphone",
      "added_at": "2025-12-01T10:00:00Z",
      "last_seen": "2025-12-12T22:00:00Z",
      "is_primary": true
    }
  ]
}
```

#### Add Anchor

Add a new anchor device.

**POST** `/v1/sdis/anchors`

```bash
curl -X POST http://10.8.10.40:30080/v1/sdis/anchors \
  -H "Authorization: Bearer <your-token>" \
  -H "Content-Type: application/json" \
  -d '{
    "device_name": "Backup Phone",
    "device_type": "smartphone",
    "ephemeral_did": "did:icn:z...",
    "ephemeral_signature": "base64-signature"
  }'
```

#### Remove Anchor

Remove an anchor device.

**DELETE** `/v1/sdis/anchors/{device_id}`

```bash
curl -X DELETE http://10.8.10.40:30080/v1/sdis/anchors/device_001 \
  -H "Authorization: Bearer <your-token>"
```

#### Promote Anchor

Promote an anchor to primary.

**POST** `/v1/sdis/anchors/{device_id}/promote`

```bash
curl -X POST http://10.8.10.40:30080/v1/sdis/anchors/device_001/promote \
  -H "Authorization: Bearer <your-token>"
```

---

### 5. Recovery

#### Initiate Recovery

Start identity recovery process using recovery codes.

**POST** `/v1/sdis/recovery/initiate`

```bash
curl -X POST http://10.8.10.40:30080/v1/sdis/recovery/initiate \
  -H "Content-Type: application/json" \
  -d '{
    "recovery_code": "CODE1",
    "new_device_did": "did:icn:z...",
    "new_device_signature": "base64-signature"
  }'
```

**Response:**
```json
{
  "recovery_id": "recovery_xyz...",
  "challenge": "base64-challenge",
  "expires_at": "2025-12-12T23:00:00Z"
}
```

#### Complete Recovery

Finalize recovery with challenge response.

**POST** `/v1/sdis/recovery/complete`

```bash
curl -X POST http://10.8.10.40:30080/v1/sdis/recovery/complete \
  -H "Content-Type: application/json" \
  -d '{
    "recovery_id": "recovery_xyz...",
    "challenge_response": "base64-response"
  }'
```

---

### 6. Ephemeral DIDs

#### Generate Ephemeral DID

Get a temporary DID for device pairing.

**POST** `/v1/sdis/ephemeral/generate`

```bash
curl -X POST http://10.8.10.40:30080/v1/sdis/ephemeral/generate \
  -H "Content-Type: application/json" \
  -d '{
    "purpose": "enrollment",
    "ttl_seconds": 3600
  }'
```

**Response:**
```json
{
  "ephemeral_did": "did:icn:z...",
  "private_key": "base64-key",
  "expires_at": "2025-12-12T23:00:00Z"
}
```

---

## Verification Levels Explained

### Level 0: Unverified
- New enrollments
- No trust in the network
- Limited capabilities

### Level 1: Device Verified
- QR code scanned successfully
- Device possession proven
- Basic network access

### Level 2: Steward Vouched
- Verified by trusted member
- Enhanced trust score
- Full network capabilities

### Level 3: Multi-Steward (Future)
- Multiple independent vouches
- Highest trust level
- Governance participation

---

## QR Code Format

QR codes contain enrollment data:

```json
{
  "type": "icn-enrollment",
  "enrollment_id": "enroll_abc123...",
  "challenge": "base64-challenge",
  "gateway_url": "http://10.8.10.40:30080"
}
```

---

## Error Responses

All errors follow this format:

```json
{
  "error": "error_code",
  "message": "Human readable description",
  "details": {}
}
```

Common errors:
- `401`: Unauthorized (invalid/missing token)
- `404`: Enrollment/device not found
- `409`: Conflict (already exists)
- `422`: Validation error
- `500`: Internal server error

---

## Security Notes

1. **Ephemeral Keys**: Used only for device pairing, never stored long-term
2. **Recovery Codes**: Store securely offline, single-use only
3. **Bearer Tokens**: Short-lived, refresh regularly
4. **Anchor Devices**: Minimum 2 recommended for redundancy
5. **Primary Anchor**: Can add/remove other anchors

---

## Integration Flow

### New User Enrollment

1. User requests enrollment → `POST /enrollment/start`
2. Display QR code to user
3. User scans QR with mobile app
4. App generates ephemeral DID
5. App calls `POST /verify/level1` with device proof
6. User gets vouched by steward → `POST /verify/level2`
7. Complete enrollment → `POST /enrollment/complete`
8. User receives DID, recovery codes, and auth token

### Device Recovery

1. User lost primary device
2. User enters recovery code → `POST /recovery/initiate`
3. System provides challenge
4. User's other anchor signs challenge
5. Complete recovery → `POST /recovery/complete`
6. New device becomes anchor

### Adding Backup Device

1. User authenticated on primary
2. Generate QR on primary device
3. Scan QR with new device
4. New device generates ephemeral DID
5. Add anchor → `POST /anchors`
6. New device becomes trusted anchor

---

## Testing

Test the full flow:

```bash
# 1. Health check
curl http://10.8.10.40:30080/v1/sdis/health

# 2. Start enrollment
ENROLLMENT=$(curl -X POST http://10.8.10.40:30080/v1/sdis/enrollment/start \
  -H "Content-Type: application/json" \
  -d '{"identity_name":"TestUser","coop_id":"test-coop"}' | jq -r '.enrollment_id')

echo "Enrollment ID: $ENROLLMENT"

# 3. Generate ephemeral DID for device
EPHEMERAL=$(curl -X POST http://10.8.10.40:30080/v1/sdis/ephemeral/generate \
  -H "Content-Type: application/json" \
  -d '{"purpose":"enrollment","ttl_seconds":3600}' | jq -r '.ephemeral_did')

echo "Ephemeral DID: $EPHEMERAL"

# 4. Complete enrollment (requires steward vouching in between)
# ... continue with verification and completion
```

---

## Next Steps

1. **Mobile App Integration**: Implement QR scanning in CoopWallet
2. **Pilot UI**: Add enrollment wizard to web interface
3. **Steward Dashboard**: Tools for stewards to vouch for new members
4. **Recovery UX**: User-friendly recovery flow
5. **Multi-device Sync**: Sync identity state across anchors

---

For more information, see:
- `docs/sdis-architecture.md` - Technical architecture
- `SDIS_STEWARD_ROADMAP.md` - Steward system design
- `icn/crates/icn-gateway/src/api/sdis/` - Implementation code
