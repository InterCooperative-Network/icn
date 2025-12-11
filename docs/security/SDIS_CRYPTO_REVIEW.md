# SDIS Cryptographic Review

**Version**: 1.0
**Status**: Pre-audit preparation
**Last Updated**: 2025-12-10

## 1. Executive Summary

This document provides a cryptographic review of the SDIS implementation, covering algorithm selection, parameter choices, and implementation considerations.

### Algorithms Used

| Purpose | Algorithm | Security Level | Standard |
|---------|-----------|----------------|----------|
| Classical signature | Ed25519 | 128-bit | RFC 8032 |
| Post-quantum signature | ML-DSA-65 | Level 3 (~143-bit) | FIPS 204 |
| Key encapsulation | ML-KEM-768 | Level 3 | FIPS 203 |
| Hash function | SHA3-256 | 128-bit | FIPS 202 |
| Symmetric encryption | Age (X25519+ChaCha20) | 128-bit | age-encryption.org |
| Zero-knowledge | STARK | 128-bit | winterfell |
| Secret sharing | Shamir | Information-theoretic | VSSS |

---

## 2. Hybrid Signature Scheme

### 2.1 Design Rationale

SDIS uses a hybrid signature scheme combining Ed25519 (classical) with ML-DSA-65 (post-quantum):

```
HybridSignature = Ed25519(message) || ML-DSA-65(message)
```

**Why hybrid?**
- **Defense in depth**: If either algorithm is broken, the other provides security
- **Transition period**: Smooth migration as PQ algorithms mature
- **Compatibility**: Ed25519 alone for constrained environments (L1)

### 2.2 Verification Logic

```rust
fn verify_hybrid(message: &[u8], sig: &HybridSignature, pk: &HybridPublicKey) -> bool {
    let classical_valid = ed25519_verify(message, &sig.classical, &pk.classical);
    let pq_valid = ml_dsa_verify(message, &sig.pq, &pk.pq);

    classical_valid && pq_valid  // Both must verify
}
```

**Security Claim**: An attacker must break BOTH Ed25519 AND ML-DSA-65 to forge a signature.

### 2.3 Key Generation

```rust
fn generate_hybrid_keypair() -> HybridKeypair {
    let classical = Ed25519::generate(OsRng);
    let pq = MlDsa65::keygen(OsRng);
    HybridKeypair { classical, pq }
}
```

**Notes**:
- Independent key generation for each algorithm
- Single entropy source (OsRng) is acceptable (distinct algorithms)
- No seed reuse between algorithms

### 2.4 Signature Sizes

| Component | Size (bytes) |
|-----------|--------------|
| Ed25519 signature | 64 |
| ML-DSA-65 signature | 3,293 |
| **Total hybrid** | **3,357** |
| Ed25519 public key | 32 |
| ML-DSA-65 public key | 1,952 |
| **Total hybrid public** | **1,984** |

---

## 3. Threshold PRF (VUI Computation)

### 3.1 Protocol Overview

VUI is computed via a threshold PRF to ensure:
1. Deterministic output for same identity data
2. No single steward learns the full VUI
3. Distributed trust across steward network

```
VUI = PRF(pepper, IdDataHash)
    = Reconstruct([PRF_partial(share_i, IdDataHash) for i in threshold])
```

### 3.2 Implementation

```rust
// Shamir secret sharing of pepper
let shares = shamir::split_secret(&pepper, threshold, total);

// Each steward computes partial PRF
fn compute_partial(share: &Share, input: &[u8]) -> PrfPartial {
    let blinded_input = blind(input);
    let partial = prf_eval(share, blinded_input);
    PrfPartial { partial, proof }
}

// User combines partials
fn combine_partials(partials: &[PrfPartial]) -> Vui {
    let combined = lagrange_interpolate(partials);
    Vui::from_bytes(combined)
}
```

### 3.3 Security Properties

| Property | Guarantee |
|----------|-----------|
| **Threshold security** | < t shares reveal nothing about VUI |
| **Determinism** | Same input → same VUI |
| **Pseudorandomness** | VUI indistinguishable from random |
| **Unlinkability** | Different inputs → unrelated VUIs |

### 3.4 Parameter Recommendations

| Parameter | Default | Minimum | Rationale |
|-----------|---------|---------|-----------|
| Threshold (t) | 3 | 2 | Byzantine tolerance |
| Total shares (n) | 5 | 3 | Availability vs security |
| Share size | 32 bytes | - | 256-bit security margin |

---

## 4. Zero-Knowledge Proofs (STARK)

### 4.1 System Parameters

```rust
const SECURITY_BITS: usize = 128;
const BLOWUP_FACTOR: usize = 4;
const FRI_FOLDING_FACTOR: usize = 4;
const FIELD: Goldilocks;  // 64-bit prime field
```

### 4.2 Proof Types

#### Age Proof
```
Proves: current_date - birthdate >= threshold
Without revealing: birthdate
```

Constraint system:
```
1. age = current_date - birthdate
2. age >= threshold
3. commitment(birthdate) = claimed_commitment
```

#### Citizenship Proof
```
Proves: country_code in credential == claimed_country
Without revealing: full credential
```

#### Membership Proof
```
Proves: org_did in credential == claimed_org
Without revealing: member ID or other details
```

### 4.3 Proof Sizes and Times

| Proof Type | Size | Generation | Verification |
|------------|------|------------|--------------|
| Age | ~15 KB | ~200ms | ~50ms |
| Citizenship | ~12 KB | ~150ms | ~40ms |
| Membership | ~18 KB | ~250ms | ~60ms |

### 4.4 Security Analysis

**Soundness error**: 2^(-128)
- Achieved via appropriate repetition and FRI parameters

**Zero-knowledge**: Computational
- Witness information protected by random masking
- Proof reveals nothing beyond statement validity

---

## 5. QR Code Encoding

### 5.1 Compact Format (137 bytes)

```
┌─────────────────────────────────────────────────────────────┐
│  Magic (2) │ Ver (1) │ Type (1) │ Anchor[..16] (16)        │
├─────────────────────────────────────────────────────────────┤
│  Ephemeral PK (32) │ Expiry (4) │ Nonce (16)               │
├─────────────────────────────────────────────────────────────┤
│  Ed25519 Signature (64)                    │ Channels (1)  │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 Security Considerations

**Anchor Truncation**: First 16 bytes used (128-bit collision resistance)
- Acceptable for short-lived proofs
- L2 binding includes full 32-byte anchor

**Relative Expiry**: 4-byte seconds from generation
- Max ~136 years, practically limited to 24 hours
- Verifier must have accurate clock (±30s tolerance recommended)

**Nonce**: 16 bytes random
- 128-bit collision resistance
- Sufficient for replay protection over proof lifetime

### 5.3 Signature Coverage

Signature covers (in order):
1. Magic bytes
2. Version
3. Proof type
4. Full anchor (32 bytes, not truncated)
5. Ephemeral public key
6. Expiry timestamp
7. Nonce
8. Channels bitmap

**Note**: Full anchor signed even though truncated in QR.

---

## 6. Key Derivation and Storage

### 6.1 Keystore Encryption

```
Keystore uses Age encryption:
1. User provides passphrase
2. scrypt KDF derives encryption key
3. ChaCha20-Poly1305 encrypts keystore
```

**scrypt parameters**:
- N = 2^18 (262144)
- r = 8
- p = 1
- Output: 32 bytes

### 6.2 KeyBundle Derivation

```rust
// Each KeyBundle version uses independent keys
fn derive_keybundle(anchor: &Anchor, version: u32) -> KeyBundle {
    let seed = hkdf_expand(
        anchor.id,
        &format!("icn-keybundle-v{}", version),
        64
    );
    let hybrid = HybridKeypair::from_seed(&seed[..32]);
    let x25519 = X25519Secret::from(&seed[32..64]);
    KeyBundle { anchor, version, hybrid, x25519 }
}
```

**Note**: Actual implementation generates fresh randomness, not derived.

### 6.3 Memory Protection

```rust
// All key types implement Zeroize
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct HybridKeypair {
    classical_signing: SigningKey,
    pq_keypair: MlDsaKeypair,
}
```

---

## 7. Replay Protection

### 7.1 Nonce-Based Protection

```rust
struct ReplayCache {
    seen: LruCache<[u8; 16], ()>,  // Nonce → timestamp
    max_entries: usize,
}

impl ReplayCache {
    fn check_and_insert(&mut self, nonce: &[u8; 16]) -> bool {
        if self.seen.contains(nonce) {
            return false;  // Replay detected
        }
        self.seen.put(*nonce, ());
        true
    }
}
```

### 7.2 Cache Sizing

**Assumptions**:
- 1 million verifications per day
- 1 hour proof validity (max)
- 16 bytes per entry

**Cache size**: ~42,000 entries × 16 bytes ≈ 672 KB

### 7.3 Distributed Considerations

- Each verifier maintains independent cache
- No cross-verifier synchronization needed
- Proof reuse at different verifiers is acceptable by design (L1)

---

## 8. Random Number Generation

### 8.1 Entropy Sources

| Use Case | Source | Justification |
|----------|--------|---------------|
| Key generation | OsRng | CSPRNG from OS |
| Nonce generation | OsRng | Must be unpredictable |
| Proof randomness | OsRng | ZK requires fresh randomness |

### 8.2 Implementation

```rust
use rand::rngs::OsRng;

fn generate_nonce() -> [u8; 16] {
    let mut nonce = [0u8; 16];
    OsRng.fill_bytes(&mut nonce);
    nonce
}
```

---

## 9. Side-Channel Considerations

### 9.1 Timing Attacks

| Operation | Constant-time? | Notes |
|-----------|----------------|-------|
| Ed25519 sign/verify | Yes | dalek library |
| ML-DSA sign/verify | Best-effort | pqcrypto library |
| Secret comparison | Yes | subtle crate |
| Nonce comparison | Yes | constant_time_eq |

### 9.2 Memory Access

- No secret-dependent array indexing
- No secret-dependent branches in crypto code
- Cache-timing resistant implementations preferred

### 9.3 Recommendations

1. Run crypto operations on constant-time hardware when possible
2. Avoid logging that could reveal timing
3. Use dedicated crypto coprocessor for high-security deployments

---

## 10. Recommendations

### 10.1 Critical

1. **Audit pqcrypto dependencies** - Third-party PQ implementations
2. **Implement constant-time comparison** - All secret comparisons
3. **Add HSM support** - For steward pepper shares

### 10.2 Important

4. **Parameter agility** - Support algorithm upgrade without protocol change
5. **Key rotation schedule** - Enforce regular KeyBundle rotation
6. **Entropy monitoring** - Alert on low entropy conditions

### 10.3 Nice to Have

7. **Formal verification** - Model and verify protocol
8. **Hardware attestation** - Bind keys to secure hardware
9. **Post-quantum key exchange** - ML-KEM for transport layer

---

## 11. Compliance Notes

### FIPS 140-3

- ML-DSA compliant with FIPS 204
- ML-KEM compliant with FIPS 203
- SHA3 compliant with FIPS 202
- Ed25519 not FIPS-approved (classical fallback)

### Common Criteria

- EAL4+ target for production deployment
- Crypto module boundary defined
- Key management procedures documented

---

## 12. References

1. NIST FIPS 204 - Module-Lattice-Based Digital Signature Standard
2. NIST FIPS 203 - Module-Lattice-Based Key-Encapsulation Mechanism
3. RFC 8032 - Edwards-Curve Digital Signature Algorithm (EdDSA)
4. age-encryption.org - age file encryption specification
5. winterfell - STARK proof system documentation
6. Shamir, A. (1979) - How to Share a Secret
