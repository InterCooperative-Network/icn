# SDIS Code Audit Checklist

**Version**: 1.0
**Status**: Pre-audit preparation
**Last Updated**: 2025-12-10

## Audit Overview

This checklist guides security auditors through the SDIS implementation in ICN.

**Estimated Audit Time**: 2-3 weeks
**Crates in Scope**: 6
**Lines of Code**: ~8,000 (core SDIS components)

---

## 1. Cryptographic Operations (`icn-crypto-pq`)

### 1.1 Hybrid Signatures
- [ ] **Signature Generation**
  - [ ] Both Ed25519 and ML-DSA signatures computed
  - [ ] Signatures computed over identical message
  - [ ] No information leakage between algorithms
  - [ ] Constant-time operations where applicable

- [ ] **Signature Verification**
  - [ ] Both signatures must verify (AND logic)
  - [ ] Verification fails fast on invalid input
  - [ ] No timing side-channels

- [ ] **Key Generation**
  - [ ] Proper entropy source (rand::rngs::OsRng)
  - [ ] Keys properly zeroized on drop
  - [ ] No key material in logs/errors

**Files**: `crates/icn-crypto-pq/src/hybrid.rs`

### 1.2 Threshold PRF
- [ ] **Secret Sharing**
  - [ ] Shamir shares correctly computed
  - [ ] Threshold reconstruction works for t-of-n
  - [ ] Insufficient shares reveal nothing

- [ ] **PRF Computation**
  - [ ] PRF is deterministic for same input
  - [ ] PRF output is pseudorandom
  - [ ] Partial evaluation doesn't leak full PRF

**Files**: `crates/icn-crypto-pq/src/threshold.rs`

### 1.3 ML-DSA Wrapper
- [ ] **Parameter Selection**
  - [ ] ML-DSA-65 (NIST Level 3) used correctly
  - [ ] Proper domain separation

- [ ] **Memory Handling**
  - [ ] Private keys zeroized after use
  - [ ] No heap allocations with sensitive data

**Files**: `crates/icn-crypto-pq/src/ml_dsa.rs`

---

## 2. Identity Types (`icn-identity`)

### 2.1 Anchor
- [ ] **Anchor Creation**
  - [ ] Anchor ID computed correctly: H(VUI || genesis)
  - [ ] VUI commitment is binding (no substitution)
  - [ ] Timestamp is authentic (not manipulated)

- [ ] **Immutability**
  - [ ] Anchor fields cannot be modified after creation
  - [ ] No serialization/deserialization mutations

**Files**: `crates/icn-identity/src/anchor.rs`

### 2.2 KeyBundle
- [ ] **Version Monotonicity**
  - [ ] Version only increases
  - [ ] Cannot create bundle with lower version

- [ ] **Anchor Binding**
  - [ ] KeyBundle strongly bound to anchor
  - [ ] Cannot transfer KeyBundle to different anchor

- [ ] **Key Rotation**
  - [ ] Old key signs rotation to new key
  - [ ] Cannot skip versions
  - [ ] Rotation reason is preserved

**Files**: `crates/icn-identity/src/keybundle.rs`

### 2.3 VUI Types
- [ ] **Commitment Scheme**
  - [ ] Commitment is hiding (reveals nothing)
  - [ ] Commitment is binding (cannot open to different value)

- [ ] **IdDataHash**
  - [ ] Personal data never leaves device
  - [ ] Hash is deterministic

**Files**: `crates/icn-identity/src/vui.rs`

### 2.4 Keystore v4
- [ ] **Encryption**
  - [ ] Age encryption used correctly
  - [ ] Passphrase-based key derivation secure
  - [ ] No plaintext key material on disk

- [ ] **Migration**
  - [ ] v1→v2→v3→v4 migration preserves data
  - [ ] Downgrade attacks prevented
  - [ ] Migration atomic (no partial states)

**Files**: `crates/icn-identity/src/keystore.rs`

---

## 3. Zero-Knowledge Proofs (`icn-zkp`)

### 3.1 STARK Parameters
- [ ] **Security Level**
  - [ ] 128-bit security achieved
  - [ ] Blowup factor adequate (4)
  - [ ] FRI parameters conservative

### 3.2 Proof Generation
- [ ] **Witness Privacy**
  - [ ] Witness data not in proof
  - [ ] No witness leakage via timing

- [ ] **Constraint System**
  - [ ] Constraints are complete (valid witness → proof)
  - [ ] Constraints are sound (no false proofs)

### 3.3 Proof Verification
- [ ] **Soundness**
  - [ ] Invalid proofs rejected
  - [ ] Malformed proofs don't crash

- [ ] **Performance**
  - [ ] Verification time bounded
  - [ ] Memory usage bounded

**Files**: `crates/icn-zkp/src/`

---

## 4. Credential Presentation (`icn-gateway/api/sdis`)

### 4.1 Ephemeral Proof
- [ ] **Proof Structure**
  - [ ] All fields correctly serialized
  - [ ] Signature covers all fields
  - [ ] Nonce is cryptographically random

- [ ] **QR Encoding**
  - [ ] 137-byte format maintained
  - [ ] Magic bytes validated on decode
  - [ ] Version checked before processing

- [ ] **Expiry Handling**
  - [ ] Expiry correctly computed (relative → absolute)
  - [ ] Expired proofs rejected
  - [ ] No integer overflow in time calculation

**Files**: `crates/icn-gateway/src/api/sdis/proof.rs`, `crates/icn-gateway/src/api/sdis/qr.rs`

### 4.2 Verification
- [ ] **L1 Verification**
  - [ ] Ed25519 signature verified
  - [ ] Nonce checked against replay cache
  - [ ] Expiry validated against current time

- [ ] **L2 Verification**
  - [ ] L1 checks performed first
  - [ ] Binding matches proof nonce
  - [ ] Hybrid signature verified (both algorithms)

- [ ] **L3 Verification**
  - [ ] L2 checks performed first
  - [ ] STARK proof verified
  - [ ] Non-revocation checked

**Files**: `crates/icn-gateway/src/api/sdis/verify.rs`

### 4.3 Replay Protection
- [ ] **Nonce Generation**
  - [ ] 16 bytes of secure randomness
  - [ ] No nonce reuse across proofs

- [ ] **Replay Cache**
  - [ ] LRU eviction works correctly
  - [ ] Cache size bounded
  - [ ] Concurrent access safe

**Files**: `crates/icn-gateway/src/api/sdis/verify.rs` (replay cache)

---

## 5. Steward Network (`icn-steward`)

### 5.1 Steward Profile
- [ ] **Status Transitions**
  - [ ] Only valid transitions allowed
  - [ ] Suspended → Active requires governance

- [ ] **Bond Handling**
  - [ ] Bond tracked correctly
  - [ ] Slashing deducts properly

**Files**: `crates/icn-steward/src/lib.rs`

### 5.2 Enrollment Tokens
- [ ] **Blind Signatures**
  - [ ] Token blindness maintained
  - [ ] Unblinded token verifies
  - [ ] Cannot link issuance to redemption

- [ ] **Token Expiry**
  - [ ] 7-day default enforced
  - [ ] Expired tokens rejected

**Files**: `crates/icn-steward/src/token.rs`

### 5.3 VUI Registry
- [ ] **Bloom Filter**
  - [ ] False positive rate acceptable (0.01%)
  - [ ] No false negatives

- [ ] **Exact Set**
  - [ ] Fallback to exact check on Bloom hit
  - [ ] Set synchronized across stewards

**Files**: `crates/icn-steward/src/vui_registry.rs`

### 5.4 Ceremonies
- [ ] **Enrollment Ceremony**
  - [ ] Requires threshold participation
  - [ ] Partial results don't leak VUI
  - [ ] Ceremony timeout handled

- [ ] **Recovery Ceremony**
  - [ ] Requires higher threshold
  - [ ] Old keys properly revoked
  - [ ] Evidence verified

**Files**: `crates/icn-steward/src/ceremony.rs`

---

## 6. Governance Integration (`icn-governance/sdis`)

### 6.1 SDIS Proposals
- [ ] **Proposal Types**
  - [ ] All 12 types properly defined
  - [ ] Required fields enforced

- [ ] **Voting Requirements**
  - [ ] Quorum calculated correctly
  - [ ] Approval threshold enforced
  - [ ] Timeout enforced

**Files**: `crates/icn-governance/src/sdis.rs`

### 6.2 Proposal Execution
- [ ] **Execution Handler**
  - [ ] Only executes on approval
  - [ ] Idempotent execution
  - [ ] Audit trail created

**Files**: `crates/icn-core/src/supervisor.rs` (SDIS handler)

---

## 7. Cross-Cutting Concerns

### 7.1 Error Handling
- [ ] No sensitive data in error messages
- [ ] Errors don't reveal timing information
- [ ] Panics don't occur on malformed input

### 7.2 Logging
- [ ] No keys logged at any level
- [ ] No VUI/anchor logged
- [ ] Safe to enable debug logging

### 7.3 Serialization
- [ ] Deserialize validates all fields
- [ ] No arbitrary code execution
- [ ] Version fields checked

### 7.4 Dependencies
- [ ] All deps at recent versions
- [ ] Known vulnerabilities addressed
- [ ] Minimal unsafe usage in deps

---

## 8. Test Coverage

### 8.1 Unit Tests
- [ ] Crypto operations have comprehensive tests
- [ ] Edge cases covered (empty input, max size)
- [ ] Error paths tested

### 8.2 Integration Tests
- [ ] Multi-node scenarios tested
- [ ] Replay attacks tested
- [ ] Expiry boundary tested

### 8.3 Fuzz Testing
- [ ] QR decode fuzzed
- [ ] Proof verification fuzzed
- [ ] Serialization fuzzed

---

## 9. Findings Template

### Finding: [TITLE]

| Attribute | Value |
|-----------|-------|
| Severity | Critical/High/Medium/Low/Info |
| Type | Crypto/Logic/DoS/Privacy |
| Location | file:line |
| Status | Open/Fixed/Acknowledged |

**Description**: [What is the issue]

**Impact**: [What can an attacker do]

**Recommendation**: [How to fix]

**Response**: [Vendor response]

---

## 10. Sign-off

| Auditor | Date | Areas Covered |
|---------|------|---------------|
| | | |
| | | |

**Overall Assessment**: [ ] Pass [ ] Pass with conditions [ ] Fail
