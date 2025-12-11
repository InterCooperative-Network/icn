# SDIS Threat Model

**Version**: 1.0
**Status**: Pre-audit preparation
**Last Updated**: 2025-12-10

## 1. System Overview

SDIS (Sovereign Digital Identity System) provides a three-tier identity verification system built on ICN's infrastructure:

```
┌─────────────────────────────────────────────────────────────────┐
│                     SDIS Architecture                            │
├─────────────────────────────────────────────────────────────────┤
│  Layer 5: Applications         │ Mobile apps, PoS terminals    │
│  Layer 4: Presentation         │ QR, NFC, BLE, HTTP            │
│  Layer 3: Verification         │ L1/L2/L3 verification tiers   │
│  Layer 2: Cryptography         │ Ed25519, ML-DSA, STARK        │
│  Layer 1: Identity             │ Anchors, KeyBundles, VUI      │
│  Layer 0: Network              │ ICN gossip, steward network   │
└─────────────────────────────────────────────────────────────────┘
```

## 2. Trust Boundaries

### 2.1 Holder Trust Boundary
- User's device (phone, wallet)
- Local keystore (Age-encrypted)
- Ephemeral key generation

### 2.2 Verifier Trust Boundary
- Verifier's device (scanner, terminal)
- Replay cache (local LRU)
- Network connectivity (for L3)

### 2.3 Steward Network Trust Boundary
- Threshold secret sharing (pepper shares)
- VUI computation
- Enrollment token issuance
- Key recovery ceremonies

### 2.4 External Systems
- Mobile OS security model
- NFC/BLE hardware
- Network infrastructure

## 3. Assets

### 3.1 Critical Assets
| Asset | Sensitivity | Impact if Compromised |
|-------|-------------|----------------------|
| Anchor ID | High | Identity takeover |
| KeyBundle private keys | Critical | Full impersonation |
| VUI | Critical | Sybil attacks possible |
| Steward pepper shares | Critical | VUI linkability |
| Enrollment tokens | High | Unauthorized identity creation |

### 3.2 Moderate Assets
| Asset | Sensitivity | Impact if Compromised |
|-------|-------------|----------------------|
| Ephemeral proofs | Low | Short-term impersonation |
| Replay cache | Low | DoS via replay |
| Steward stats | Low | Reputation gaming |

## 4. Threat Actors

### 4.1 External Attackers
- **Capability**: Network access, commodity hardware
- **Motivation**: Identity fraud, credential theft
- **Examples**: Credential stuffing, phishing, replay attacks

### 4.2 Malicious Verifiers
- **Capability**: Physical access to verification devices
- **Motivation**: Privacy violation, tracking
- **Examples**: Harvesting credentials, building profiles

### 4.3 Rogue Stewards
- **Capability**: Steward node access, pepper share
- **Motivation**: Financial gain, targeted attacks
- **Examples**: VUI linkage, enrollment manipulation

### 4.4 Nation-State Actors
- **Capability**: Advanced resources, quantum computing
- **Motivation**: Mass surveillance, targeted individuals
- **Examples**: Cryptographic attacks, infrastructure compromise

### 4.5 Insiders
- **Capability**: Code access, operational knowledge
- **Motivation**: Various
- **Examples**: Backdoors, key exfiltration

## 5. Threat Categories

### 5.1 Cryptographic Threats

#### T-CRYPTO-1: Quantum Attack on Ed25519
- **Description**: Future quantum computer breaks Ed25519 signatures
- **Affected**: L1 verification (QR only)
- **Likelihood**: Medium (5-15 years)
- **Impact**: Critical
- **Mitigation**: Hybrid signatures (Ed25519 + ML-DSA) at L2/L3
- **Residual Risk**: L1 remains classical-only by design (speed tradeoff)

#### T-CRYPTO-2: ML-DSA Implementation Vulnerability
- **Description**: Bug in post-quantum signature implementation
- **Affected**: L2/L3 verification
- **Likelihood**: Low
- **Impact**: High
- **Mitigation**: Use audited library (pqcrypto-dilithium), fallback to classical
- **Residual Risk**: Accept for FIPS-certified library

#### T-CRYPTO-3: STARK Soundness Error
- **Description**: Zero-knowledge proof accepts invalid witness
- **Affected**: L3 attribute verification
- **Likelihood**: Low
- **Impact**: High (false attribute claims)
- **Mitigation**: Conservative parameters, multiple proof systems
- **Residual Risk**: Accept with 128-bit security margin

### 5.2 Protocol Threats

#### T-PROTO-1: Replay Attack
- **Description**: Captured proof reused at different verifier
- **Affected**: All verification levels
- **Likelihood**: High (easy to attempt)
- **Impact**: Medium (limited by expiry)
- **Mitigation**: 16-byte nonces, LRU replay cache, short expiry
- **Residual Risk**: Window of validity (default 1 hour)

#### T-PROTO-2: Proof Sharing
- **Description**: User shares QR code with unauthorized party
- **Affected**: L1 verification
- **Likelihood**: Medium
- **Impact**: Medium (impersonation)
- **Mitigation**: L2 binding verification, short expiry
- **Residual Risk**: L1 intentionally allows transfer (design choice)

#### T-PROTO-3: Man-in-the-Middle on NFC/BLE
- **Description**: Attacker intercepts L2 binding transfer
- **Affected**: L2 verification channel
- **Likelihood**: Low (requires proximity)
- **Impact**: Medium
- **Mitigation**: Binding includes nonce from proof, signature verification
- **Residual Risk**: Relay attacks possible in theory

#### T-PROTO-4: Denial of Service via Replay Cache
- **Description**: Attacker fills replay cache with garbage
- **Affected**: All verification
- **Likelihood**: Medium
- **Impact**: Low (performance degradation)
- **Mitigation**: LRU eviction, rate limiting per source
- **Residual Risk**: Accept with monitoring

### 5.3 Identity Threats

#### T-ID-1: Sybil Attack (Multiple Identities)
- **Description**: One person creates multiple VUIs
- **Affected**: Identity uniqueness
- **Likelihood**: Low (steward verification)
- **Impact**: Critical
- **Mitigation**: VUI via threshold PRF, steward attestation
- **Residual Risk**: Depends on steward integrity

#### T-ID-2: Anchor Takeover
- **Description**: Attacker gains control of victim's anchor
- **Affected**: Identity ownership
- **Likelihood**: Low
- **Impact**: Critical
- **Mitigation**: Hybrid signatures, recovery ceremonies
- **Residual Risk**: Social engineering of stewards

#### T-ID-3: Key Compromise
- **Description**: Private key extracted from device
- **Affected**: All user operations
- **Likelihood**: Medium
- **Impact**: High
- **Mitigation**: Age encryption, key rotation, recovery
- **Residual Risk**: Device security dependent

### 5.4 Steward Network Threats

#### T-STEWARD-1: Collusion Attack
- **Description**: Threshold of stewards collude to compute arbitrary VUI
- **Affected**: VUI uniqueness
- **Likelihood**: Low (requires 3+ stewards)
- **Impact**: Critical
- **Mitigation**: Geographic distribution, bond requirements, term limits
- **Residual Risk**: Governance monitoring required

#### T-STEWARD-2: Pepper Share Leakage
- **Description**: Steward's pepper share is compromised
- **Affected**: VUI privacy
- **Likelihood**: Medium
- **Impact**: Medium (one share insufficient)
- **Mitigation**: Threshold scheme (t-of-n), share rotation
- **Residual Risk**: Multiple compromises required

#### T-STEWARD-3: Enrollment Token Forgery
- **Description**: Attacker creates valid enrollment token
- **Affected**: Identity issuance
- **Likelihood**: Low (requires blind signature break)
- **Impact**: High
- **Mitigation**: Blind signatures, token expiry (7 days)
- **Residual Risk**: Accept with auditing

### 5.5 Implementation Threats

#### T-IMPL-1: Side-Channel Attack
- **Description**: Timing/power analysis reveals secrets
- **Affected**: Key operations
- **Likelihood**: Medium
- **Impact**: High
- **Mitigation**: Constant-time operations, zeroize on drop
- **Residual Risk**: Hardware-dependent

#### T-IMPL-2: Memory Safety Vulnerability
- **Description**: Buffer overflow, use-after-free in Rust code
- **Affected**: All components
- **Likelihood**: Low (Rust safety)
- **Impact**: Critical
- **Mitigation**: Safe Rust, minimal unsafe blocks, fuzzing
- **Residual Risk**: Unsafe blocks in crypto libs

## 6. Attack Trees

### 6.1 Impersonate Identity

```
Goal: Impersonate victim's identity
├── Obtain valid ephemeral proof [OR]
│   ├── Steal proof from victim's device
│   ├── Intercept proof in transit
│   └── Social engineer victim to share
├── Compromise KeyBundle [OR]
│   ├── Extract from encrypted keystore
│   │   ├── Guess passphrase
│   │   └── Memory dump while unlocked
│   └── Obtain backup recovery
└── Take over anchor [OR]
    ├── Collude with stewards for recovery
    └── Compromise multiple steward nodes
```

### 6.2 Create False Identity

```
Goal: Create identity without valid personhood
├── Bypass VUI uniqueness [OR]
│   ├── Collude with t stewards (hard)
│   ├── Find VUI collision (infeasible)
│   └── Exploit Bloom filter false positive
└── Forge enrollment token [OR]
    ├── Break blind signature scheme
    └── Compromise steward signing key
```

## 7. Security Requirements

### 7.1 Authentication Requirements
- REQ-AUTH-1: L1 provides 128-bit classical security
- REQ-AUTH-2: L2 provides 128-bit post-quantum security
- REQ-AUTH-3: Replay window ≤ proof validity period

### 7.2 Privacy Requirements
- REQ-PRIV-1: VUI reveals no identity information
- REQ-PRIV-2: Proofs are unlinkable across sessions
- REQ-PRIV-3: Stewards cannot link enrollment to identity

### 7.3 Availability Requirements
- REQ-AVAIL-1: L1/L2 work fully offline
- REQ-AVAIL-2: Single steward failure doesn't block enrollment

### 7.4 Integrity Requirements
- REQ-INT-1: Anchors are immutable after creation
- REQ-INT-2: KeyBundle rotation is monotonic

## 8. Recommended Mitigations

### 8.1 High Priority
1. Implement rate limiting on verification endpoints
2. Add steward monitoring and alerting
3. Conduct cryptographic library audit

### 8.2 Medium Priority
4. Add hardware security module (HSM) support for stewards
5. Implement proof-of-work for enrollment (anti-spam)
6. Add geographic diversity requirements for stewards

### 8.3 Future Considerations
7. Hardware attestation for key storage
8. Formal verification of protocol
9. Threshold signatures for anchor operations

## 9. Audit Scope

### 9.1 In Scope
- `icn-crypto-pq`: Hybrid signatures, threshold PRF
- `icn-identity`: Anchor, KeyBundle, VUI types
- `icn-zkp`: STARK proof generation/verification
- `icn-gateway/sdis`: Ephemeral proof system
- `icn-steward`: Enrollment ceremonies
- `icn-governance/sdis`: SDIS governance proposals

### 9.2 Out of Scope
- Mobile app implementations
- Third-party cryptographic libraries (pqcrypto-*)
- ICN networking layer (covered separately)

## 10. References

- [NIST FIPS 204](https://csrc.nist.gov/pubs/fips/204/final) - ML-DSA Standard
- [RFC 8032](https://tools.ietf.org/html/rfc8032) - Ed25519 Signatures
- [STARK Paper](https://eprint.iacr.org/2018/046) - Scalable Zero-Knowledge
