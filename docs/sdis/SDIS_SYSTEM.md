# SDIS: Secure Distributed Identity System

> Design snapshot: this document mixes implemented flows and planned endpoint models.
> For currently wired gateway routes, use `icn/crates/icn-gateway/src/api/sdis/mod.rs`.

## Overview

SDIS is ICN's multi-device identity management and recovery system. It allows users to:
- Enroll new devices securely
- Manage trusted recovery anchors
- Recover identities after device loss
- Maintain cryptographic proof chains
- Delegate recovery authority to trusted contacts

## Architecture

### Components

1. **Enrollment System**: Secure device onboarding with challenge-response
2. **Anchor Management**: Trusted device/contact registration for recovery
3. **Proof System**: Cryptographic verification of identity claims
4. **Recovery Mechanism**: Multi-anchor identity restoration

### Data Model

#### Enrollment Record
```rust
struct EnrollmentRecord {
    enrollment_id: String,          // Unique enrollment identifier
    root_did: String,               // Primary identity DID
    device_did: String,             // New device DID
    device_label: String,           // Human-readable device name
    device_pubkey: Vec<u8>,         // Ed25519 public key
    challenge: Vec<u8>,             // Enrollment challenge
    challenge_expires_at: u64,      // Challenge expiration timestamp
    approved: bool,                 // Enrollment approval status
    approved_at: Option<u64>,       // Approval timestamp
    created_at: u64,                // Creation timestamp
}
```

#### Recovery Anchor
```rust
struct RecoveryAnchor {
    anchor_id: String,              // Unique anchor identifier
    owner_did: String,              // Identity owning this anchor
    anchor_type: AnchorType,        // Device or Contact
    label: String,                  // Human-readable label
    pubkey: Vec<u8>,                // Ed25519 public key
    contact_did: Option<String>,    // DID if contact anchor
    created_at: u64,                // Creation timestamp
    revoked_at: Option<u64>,        // Revocation timestamp
}

enum AnchorType {
    Device,                         // User's own device
    Contact,                        // Trusted contact's device
}
```

#### Proof Chain
```rust
struct ProofRecord {
    proof_id: String,               // Unique proof identifier
    claim_type: ClaimType,          // Type of claim being proven
    subject_did: String,            // DID making the claim
    issuer_did: String,             // DID verifying the claim
    signature: Vec<u8>,             // Ed25519 signature
    created_at: u64,                // Creation timestamp
    expires_at: Option<u64>,        // Optional expiration
}

enum ClaimType {
    DeviceOwnership,                // Claim: "I own this device"
    DeviceEnrollment,               // Claim: "This device is enrolled"
    RecoveryCapability,             // Claim: "I can recover this identity"
    IdentityBinding,                // Claim: "These DIDs belong together"
}
```

## Enrollment Flow

> Route note: some endpoint examples in this design section use legacy naming.
> For currently wired route shapes, see `docs/sdis/SDIS_API_GUIDE.md` and `docs/sdis/SDIS_STATUS.md`.

### 1. Initiate Enrollment (New Device)

The new device generates a keypair and requests enrollment:

```
POST /v1/sdis/enroll
{
  "root_did": "did:icn:z6Mk...",
  "device_did": "did:icn:z6Mk...",
  "device_label": "My Laptop",
  "device_pubkey": "base64_encoded_public_key"
}

Response:
{
  "enrollment_id": "enr_abc123",
  "challenge": "base64_encoded_challenge",
  "expires_at": 1704067200
}
```

### 2. Approve Enrollment (Existing Device)

User approves on an already-enrolled device:

```
POST /v1/sdis/enroll/{enrollment_id}/approve
{
  "signature": "base64_ed25519_signature"
}

Response:
{
  "status": "approved",
  "proof_id": "proof_xyz789"
}
```

### 3. Complete Enrollment (New Device)

New device verifies and saves the approval:

```
GET /v1/sdis/enroll/{enrollment_id}

Response:
{
  "enrollment_id": "enr_abc123",
  "root_did": "did:icn:z6Mk...",
  "device_did": "did:icn:z6Mk...",
  "approved": true,
  "approved_at": 1704063600
}
```

## Anchor Management

### Add Recovery Anchor

```
POST /v1/sdis/anchors
{
  "anchor_type": "device",          // or "contact"
  "label": "My Phone",
  "pubkey": "base64_encoded_key",
  "contact_did": "did:icn:z6Mk..."  // optional, for contact anchors
}

Response:
{
  "anchor_id": "anc_def456",
  "created_at": 1704063600
}
```

### List Anchors

```
GET /v1/sdis/anchors

Response:
{
  "anchors": [
    {
      "anchor_id": "anc_def456",
      "owner_did": "did:icn:z6Mk...",
      "anchor_type": "device",
      "label": "My Phone",
      "pubkey": "...",
      "created_at": 1704063600,
      "revoked_at": null
    }
  ]
}
```

### Revoke Anchor

```
POST /v1/sdis/anchors/{anchor_id}/revoke

Response:
{
  "status": "revoked",
  "revoked_at": 1704067200
}
```

## Recovery Process

### 1. Initiate Recovery

Lost device or new device initiates recovery:

```
POST /v1/sdis/recover/initiate
{
  "root_did": "did:icn:z6Mk...",
  "new_device_did": "did:icn:z6Mk...",
  "new_device_pubkey": "...",
  "anchor_pubkeys": ["...", "..."]   // Public keys of recovery anchors
}

Response:
{
  "recovery_id": "rec_ghi789",
  "required_approvals": 2,            // Threshold (e.g., 2 of 3)
  "pending_approvals": []
}
```

### 2. Approve Recovery (Anchor Devices)

Each recovery anchor approves:

```
POST /v1/sdis/recover/{recovery_id}/approve
{
  "anchor_id": "anc_def456",
  "signature": "base64_signature"     // Sign recovery_id + new_device_did
}

Response:
{
  "approvals": 1,
  "required": 2,
  "status": "pending"                 // or "approved" when threshold met
}
```

### 3. Complete Recovery

Once threshold is met, new device completes recovery:

```
GET /v1/sdis/recover/{recovery_id}

Response:
{
  "recovery_id": "rec_ghi789",
  "status": "approved",
  "new_device_did": "did:icn:z6Mk...",
  "approvals": [
    {
      "anchor_id": "anc_def456",
      "approved_at": 1704063600
    },
    {
      "anchor_id": "anc_xyz123",
      "approved_at": 1704063700
    }
  ]
}
```

## Security Model

### Challenge-Response Protocol

1. Server generates random challenge (32 bytes)
2. Approving device signs challenge with its private key
3. Server verifies signature against stored public key
4. Prevents replay attacks with short-lived challenges (5 minutes)

### Multi-Anchor Recovery

- Threshold signatures required (e.g., 2 of 3 anchors)
- Prevents single point of failure
- Balances security and usability
- Anchors can be device OR trusted contacts

### Proof Chains

Every identity operation creates a cryptographic proof:
- Enrollment proof: Root device → New device
- Recovery proof: Anchor devices → Recovered device
- Anchor registration proof: Root device → Anchor device

Proofs are:
- Signed with Ed25519
- Timestamped
- Immutable once created
- Optionally expirable

### Revocation

- Anchors can be revoked at any time
- Revoked anchors cannot approve new recoveries
- Existing approvals from revoked anchors remain valid
- Consider compromised device scenarios

## Integration Points

### Gateway API

- **Enrollment**: `/v1/sdis/enrollment/*` and steward workflow routes (`/v1/sdis/status/*`, `/v1/sdis/vouch/*`, `/v1/sdis/reject/*`)
- **Anchors**: `/v1/sdis/anchor/*`
- **Recovery**: `/v1/sdis/recovery/*`
- **Proofs**: `/v1/sdis/ephemeral/*` and verification routes (`/v1/sdis/verify/*`)

### Pilot UI

- **Enrollment Wizard**: `components/enrollment-wizard.js`
- **Identity Viewer**: `components/identity-viewer.js`
- **Anchor Manager**: `components/anchor-manager.js`
- **Recovery Assistant**: `components/recovery-assistant.js` (TODO)

### Mobile SDK

- **iOS/Android**: React Native integration via SDK
- **Secure Enclave**: Hardware-backed key storage
- **Biometric Auth**: Face ID / Fingerprint for approvals
- **QR Code Scanning**: For enrollment challenges

## Best Practices

### For Users

1. **Register Multiple Anchors**: At least 2-3 recovery anchors
2. **Mix Anchor Types**: Combine devices AND trusted contacts
3. **Label Clearly**: Use descriptive names for devices
4. **Review Regularly**: Audit anchor list monthly
5. **Revoke Promptly**: Remove lost/stolen devices immediately

### For Developers

1. **Validate Signatures**: Always verify Ed25519 signatures
2. **Check Expiration**: Enforce challenge expiration
3. **Rate Limit**: Prevent enrollment spam
4. **Audit Log**: Record all SDIS operations
5. **Secure Storage**: Encrypt private keys at rest

### For Operators

1. **Monitor Metrics**: Track enrollment success rate
2. **Set Thresholds**: Configure anchor approval requirements
3. **Backup Proofs**: Replicate proof chains across nodes
4. **Alert on Anomalies**: Detect unusual recovery patterns
5. **Provide Support**: Help users with recovery failures

## Current Status

**Phase 1 Complete** ✅
- [x] Data models defined
- [x] Enrollment flow implemented
- [x] Anchor management implemented
- [x] Gateway API endpoints
- [x] Pilot UI components (enrollment, anchors)
- [x] Basic security measures

**Phase 2 In Progress** 🚧
- [ ] Recovery flow implementation
- [ ] Proof chain verification
- [ ] Mobile SDK integration
- [ ] QR code enrollment
- [ ] Biometric approval

**Phase 3 Planned** 📋
- [ ] Threshold signatures
- [ ] Hardware security module support
- [ ] Cross-coop recovery
- [ ] Social recovery protocols
- [ ] Advanced auditing

## API Reference

See `docs/api/sdis.md` for complete API documentation.

## Testing

```bash
# Run SDIS tests
cd icn
cargo test -p icn-identity sdis
cargo test -p icn-gateway sdis

# Integration tests
cd web/pilot-ui
npm test -- sdis
```

## Troubleshooting

### Enrollment Fails

**Problem**: Challenge expired
**Solution**: Re-initiate enrollment within 5 minutes

**Problem**: Signature verification failed
**Solution**: Ensure device_pubkey matches signing key

### Recovery Fails

**Problem**: Insufficient approvals
**Solution**: Ensure threshold anchors approve

**Problem**: Anchor revoked
**Solution**: Use different anchor or re-register

### General Issues

- Check node logs: `kubectl logs -n icn <pod-name>`
- Verify DID format: Must be `did:icn:z6Mk...`
- Confirm network connectivity between devices

## Future Enhancements

1. **Hardware Tokens**: YubiKey, Ledger support
2. **Social Recovery**: N-of-M friend recovery
3. **Time-Locked Recovery**: Delayed recovery for security
4. **Cross-Network Recovery**: Recover across different ICN networks
5. **Biometric Enrollment**: Passwordless onboarding

## References

- [DID Specification](https://www.w3.org/TR/did-core/)
- [Ed25519 Signatures](https://ed25519.cr.yp.to/)
- [SDIS Design Doc](../design/multi-device-identity-design.md)
- [Security Model](../security/threat-model.md)
