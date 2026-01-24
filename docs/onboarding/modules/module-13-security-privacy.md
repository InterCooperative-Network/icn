# Module 13: Security and Privacy Layers

## Overview
This module explains ICN's multi-layer security model and privacy primitives.
It focuses on how authenticity, integrity, and confidentiality are enforced and
why trust-native systems still require rigorous security boundaries.

## Objectives
- Understand ICN's three-layer security model
- Learn how signed envelopes and replay guards work
- Understand trust-gated rate limiting and access control
- Learn where privacy protections are applied

## Prerequisites
- Module 2 (Architecture Overview)
- Module 4 (Identity and Trust)
- Module 5 (Network and Gossip)

## Key Reading
- `icn/crates/icn-security/` - Security utilities
- `icn/crates/icn-privacy/` - Privacy primitives
- `icn/crates/icn-net/src/envelope.rs` - Signed envelopes
- `docs/production-hardening.md` - Security hardening guidance

---

## Walkthrough

### 1. The Three-Layer Security Model

ICN security is layered so that each layer can fail independently without
compromising the entire system:

1. **Transport security**: QUIC/TLS with DID binding
2. **Message security**: Signed envelopes and replay protection
3. **Application security**: End-to-end encryption for sensitive payloads

### 2. Signed Envelopes

Messages are wrapped in signed envelopes to prove authenticity and integrity.
This also enables replay protection with monotonically increasing sequences.

Why it exists:
- Prevents tampering and impersonation
- Enables auditability of message provenance

### 3. Replay Guard

The replay guard tracks the last seen sequence per sender, rejecting old or
duplicate messages. This prevents attackers from reusing valid messages.

### 4. Trust-Gated Controls

Trust classes (isolated, known, partner, federated) gate message rates and
access permissions. This reduces abuse while preserving openness.

### 5. Privacy Primitives

Privacy tools are used when payloads need confidentiality beyond transport:

- Selective disclosure for identity claims
- Encrypted envelopes for sensitive data
- Minimization of metadata in public gossip topics

---

## Exercises

1. **Trace a signed envelope**
   - Open `icn/crates/icn-net/src/envelope.rs`
   - Identify the fields used for signature and replay protection

2. **Find rate limiting logic**
   - Open `icn/crates/icn-security/` and locate trust class rate limits
   - Describe how trust score affects permitted throughput

3. **Map security layers to code**
   - Identify where transport security is enforced
   - Identify where message security is enforced
   - Identify where application-level privacy is applied

4. **Threat modeling**
   - List three attack vectors that ICN's layered security prevents
   - Explain which layer mitigates each

---

## Checkpoints

- [ ] You can explain ICN's three-layer security model
- [ ] You can describe how signed envelopes prevent tampering
- [ ] You understand replay protection and its purpose
- [ ] You can explain trust-gated rate limiting
- [ ] You know where privacy primitives live in the codebase

---

## Notes and gotchas

- Trust-based access is not a substitute for cryptographic verification.
- Replay guards require reliable sequence management across restarts.
- Privacy features should be explicit and opt-in to avoid hidden complexity.
