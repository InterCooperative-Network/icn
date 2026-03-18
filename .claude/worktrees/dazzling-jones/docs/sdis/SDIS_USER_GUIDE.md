# SDIS User Guide

**Sovereign Digital Identity System - User Documentation**

This guide explains how to use the SDIS credential presentation system for identity verification.

## Overview

SDIS provides a three-level verification system for identity credentials:

| Level | Channel | Speed | Network | Security |
|-------|---------|-------|---------|----------|
| L1 | QR Scan | <2s | None | Classical Ed25519 |
| L2 | NFC/BLE | ~5s | None | Hybrid (Ed25519 + ML-DSA) |
| L3 | Network | ~10s | Required | STARK proofs |

## Quick Start

### Generating an Ephemeral Proof

```rust
use icn_gateway::api::sdis::{EphemeralProof, Channel, encode_for_qr};
use icn_identity::{Anchor, KeyBundle};
use icn_zkp::ProofType;
use std::time::Duration;

// Create identity
let anchor = Anchor::genesis("my-identity");
let keybundle = KeyBundle::generate(anchor, 1)?;

// Generate ephemeral proof for age verification
let (proof, binding) = EphemeralProof::generate(
    &keybundle,
    ProofType::AgeAtLeast { threshold: 21 },
    Duration::from_secs(3600), // 1 hour validity
    vec![Channel::Nfc, Channel::Ble], // Available upgrade channels
)?;

// Encode for QR code display
let qr_data = encode_for_qr(&proof)?;
// qr_data is 137 bytes - suitable for QR Version 5-10
```

### Verifying a Credential

```rust
use icn_gateway::api::sdis::{EphemeralVerifier, decode_from_qr};

let verifier = EphemeralVerifier::new();

// Level 1: QR scan only (fast, offline)
let proof = decode_from_qr(&qr_bytes)?;
let result = verifier.verify_level1(&proof);
if result.valid {
    println!("Verified: {:?}", result.proof_type);
}

// Level 2: With binding (more secure, offline)
let result = verifier.verify_level2(&proof, &binding);
if result.valid && result.level == 2 {
    println!("Hybrid signature verified");
}
```

## Verification Levels

### Level 1: QR Scan Only

**Use Case**: Quick age verification at point of sale, event entry

**How it works**:
1. User displays QR code on their device
2. Verifier scans QR code
3. Ed25519 signature is verified locally
4. Nonce is checked against replay cache

**Guarantees**:
- Proof was signed by valid ephemeral key
- Proof has not expired
- Same proof cannot be replayed

**Limitations**:
- Cannot verify identity binding (ephemeral key could be shared)
- Classical crypto only (not post-quantum secure)

### Level 2: Binding Verification

**Use Case**: Higher-value transactions, access control

**How it works**:
1. All Level 1 checks
2. Binding is transferred via NFC or BLE
3. Hybrid signature (Ed25519 + ML-DSA) is verified
4. Binding proves ephemeral key is bound to identity anchor

**Guarantees**:
- All Level 1 guarantees
- Ephemeral key is bound to a specific identity
- Post-quantum resistant (if ML-DSA signature is valid)

**Requirements**:
- NFC or BLE capability on both devices
- Binding data (~2.5KB)

### Level 3: Full Network Verification

**Use Case**: High-security applications, regulatory compliance

**How it works**:
1. All Level 2 checks
2. STARK proof is verified (proves attribute without revealing data)
3. Non-revocation is checked against accumulator
4. Requires network access

**Guarantees**:
- All Level 2 guarantees
- Zero-knowledge proof of attribute
- Credential not revoked

**Requirements**:
- Network connectivity
- Access to revocation accumulator

## Proof Types

### Age Verification

```rust
ProofType::AgeAtLeast { threshold: 21 }
```

Proves the holder is at least `threshold` years old without revealing birthdate.

### Citizenship

```rust
ProofType::Citizenship { country_code: [b'U', b'S'] }
```

Proves citizenship without revealing other identity details.

### Membership

```rust
ProofType::Membership { org_did: did }
```

Proves membership in an organization.

### Non-Revocation

```rust
ProofType::NonRevocation
```

Proves the credential has not been revoked.

## API Endpoints

### Health Check

```
GET /v1/sdis/health
```

Response:
```json
{
  "status": "ok",
  "service": "sdis",
  "version": "1.0"
}
```

### Level 1 Verification

```
POST /v1/sdis/verify/level1
Content-Type: application/json

{
  "qr_data": "<base64-encoded QR data>"
}
```

Response:
```json
{
  "valid": true,
  "level": 1,
  "proof_type": "AgeAtLeast { threshold: 21 }",
  "warnings": [],
  "verified_at": 1733861234
}
```

### Level 2 Verification

```
POST /v1/sdis/verify/level2
Content-Type: application/json

{
  "qr_data": "<base64-encoded QR data>",
  "binding": "<base64-encoded binding data>"
}
```

Response:
```json
{
  "valid": true,
  "level": 2,
  "proof_type": "AgeAtLeast { threshold: 21 }",
  "warnings": ["Hybrid signature present (PQ-resistant)"],
  "verified_at": 1733861234
}
```

## QR Code Encoding

The compact QR encoding is 137 bytes:

| Offset | Size | Field |
|--------|------|-------|
| 0-1 | 2 | Magic bytes "IC" |
| 2 | 1 | Version |
| 3 | 1 | Proof type |
| 4-19 | 16 | Truncated anchor |
| 20-51 | 32 | Ephemeral public key |
| 52-55 | 4 | Relative expiry |
| 56-71 | 16 | Nonce |
| 72-135 | 64 | Ed25519 signature |
| 136 | 1 | Channels bitmap |

This fits in QR Version 5 (37x37 modules) with error correction level M.

## Upgrade Channels

The `channels` field indicates available upgrade paths:

| Bit | Channel | Description |
|-----|---------|-------------|
| 0 | NFC | Near-Field Communication |
| 1 | BLE | Bluetooth Low Energy |
| 2 | WiFi Direct | WiFi Direct |
| 3 | HTTP | Network (requires connectivity) |

Example: `channels = 0b00000011` means NFC and BLE are available.

## Security Considerations

### Replay Protection

- Each proof contains a unique 16-byte nonce
- Verifiers maintain an LRU cache of seen nonces
- Replay attempts are rejected

### Expiry

- Proofs have configurable validity (max 24 hours)
- Expired proofs are rejected
- Use short validity for high-security scenarios

### Binding Security

- Level 2 binding uses hybrid signatures (Ed25519 + ML-DSA)
- Provides post-quantum security
- Binding expires with the proof

### Best Practices

1. **Use appropriate level**: L1 for convenience, L2/L3 for security
2. **Short validity**: Use 1-hour validity for most cases
3. **Fresh proofs**: Generate new proofs for each session
4. **Verify binding**: Use L2 when identity binding matters
5. **Check warnings**: Review warning messages in responses

## Error Handling

| Error | Meaning | Resolution |
|-------|---------|------------|
| "Proof expired" | Validity period passed | Request new proof |
| "Replay detected" | Same proof presented twice | Generate new proof |
| "Invalid signature" | Signature verification failed | Check proof integrity |
| "Binding mismatch" | Binding doesn't match proof | Use correct binding |
| "Binding expired" | Binding validity passed | Request new binding |

## Integration Examples

### Mobile App (Holder)

```rust
// Daily refresh of ephemeral proof
async fn refresh_proof(keybundle: &KeyBundle) -> Result<(Vec<u8>, EphemeralBinding)> {
    let (proof, binding) = EphemeralProof::generate(
        keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(86400), // 24 hours
        vec![Channel::Nfc, Channel::Ble],
    )?;

    let qr_data = encode_for_qr(&proof)?;
    Ok((qr_data, binding))
}
```

### Point of Sale (Verifier)

```rust
// Quick age check
async fn check_age(qr_bytes: &[u8], min_age: u8) -> bool {
    let proof = match decode_from_qr(qr_bytes) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let verifier = EphemeralVerifier::new();
    let result = verifier.verify_level1(&proof);

    result.valid && matches!(
        result.proof_type,
        Some(ProofType::AgeAtLeast { threshold }) if threshold >= min_age
    )
}
```

### Access Control (Higher Security)

```rust
// L2 verification with binding
async fn verify_access(
    qr_bytes: &[u8],
    binding_bytes: &[u8],
    verifier: &EphemeralVerifier,
) -> Result<bool> {
    let proof = decode_from_qr(qr_bytes)?;
    let binding: EphemeralBinding = bincode::deserialize(binding_bytes)?;

    let result = verifier.verify_level2(&proof, &binding);

    Ok(result.valid && result.level >= 2)
}
```

## Troubleshooting

### QR Code Not Scanning

- Ensure proper lighting
- Check QR code size (should be at least 3cm x 3cm)
- Verify QR code is not damaged or cropped

### Verification Failing

1. Check proof expiry
2. Verify proof was generated with correct proof type
3. Ensure binding matches proof (for L2)
4. Check for replay (don't reuse same proof)

### Performance Issues

- Use L1 for high-volume scenarios
- Pre-warm verifier caches
- Consider caching bindings for repeat customers
