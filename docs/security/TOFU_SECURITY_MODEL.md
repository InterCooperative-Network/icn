# TOFU (Trust-On-First-Use) Security Model

**Date**: December 18, 2025  
**Status**: Implemented  
**Component**: `icn-net` TLS layer

## Overview

ICN implements a Trust-On-First-Use (TOFU) security model that balances network openness with security verification. This document describes the architecture, rationale, and security properties of the TOFU implementation.

## Architecture

### Three-Layer Security Model

1. **TLS Layer (Transport)**: Mutual TLS with permissive client cert acceptance
2. **Application Layer (Protocol)**: DID-TLS binding verification in Hello messages
3. **Authorization Layer (Access Control)**: Trust graph-based authorization

### TLS Handshake Flow

```
Client                          Server
  |                               |
  |-- ClientHello --------------->|
  |<----------- ServerHello ------|
  |<----------- Certificate ------|
  |<----- CertificateRequest -----|  (requests client cert)
  |                               |
  |-- Certificate --------------->|  (client sends self-signed cert)
  |-- CertificateVerify --------->|
  |-- Finished ------------------>|
  |<----------- Finished ---------|
  |                               |
  [TLS session established]       |
  |                               |
  |-- Hello (DID + X25519) ------>|
  |                               | [Verify DID-TLS binding]
  |                               | [Check trust score]
  |<----------- HelloAck ---------|
```

## Security Properties

### What TOFU Provides

1. **Cryptographic Authentication**: Ed25519 signatures verify TLS handshake integrity
2. **Confidentiality**: TLS 1.3 encryption protects all traffic
3. **Certificate Binding**: DID is bound to TLS certificate in SAN field
4. **Trust-Based Authorization**: Trust graph enforces access control
5. **Replay Protection**: Sequence numbers prevent replay attacks

### What TOFU Doesn't Provide

1. **Pre-Connection Trust Verification**: First connection happens before trust verification
2. **PKI Infrastructure**: No certificate authorities or trust chains
3. **Revocation**: Requires application-layer mechanisms

## Implementation Details

### TofuCertificateVerifier

Located in `icn-net/src/tls.rs`, this implements `rustls::server::danger::ClientCertVerifier`:

```rust
#[derive(Debug)]
struct TofuCertificateVerifier {}

impl ClientCertVerifier for TofuCertificateVerifier {
    fn offer_client_auth(&self) -> bool {
        true  // Request client certificates
    }

    fn client_auth_mandatory(&self) -> bool {
        false  // Don't fail TLS if client doesn't send cert
    }

    fn verify_client_cert(&self, ...) -> Result<ClientCertVerified, Error> {
        // Accept any certificate - verification deferred to Hello handler
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls13_signature(&self, ...) -> Result<HandshakeSignatureValid, Error> {
        // Verify Ed25519 signature on TLS handshake
        verifying_key.verify(message, &signature)?;
        Ok(HandshakeSignatureValid::assertion())
    }
}
```

### Hello Message Verification

The application layer verifies DID-TLS binding:

1. Extract DID from Hello message
2. Extract DID from peer TLS certificate SAN field
3. Verify they match
4. Verify X25519 encryption key signature
5. Query trust graph for authorization decision

## Threat Model

### Threats Mitigated

| Threat | Mitigation |
|--------|------------|
| Man-in-the-Middle | TLS 1.3 mutual authentication |
| Replay Attacks | Sequence numbers + Bloom filter |
| Identity Spoofing | DID-TLS binding verification |
| Unauthorized Access | Trust graph authorization |
| Traffic Analysis | TLS encryption |

### Threats Requiring Additional Defenses

| Threat | Defense Mechanism |
|--------|-------------------|
| Sybil Attacks | Trust graph + rate limiting |
| Byzantine Behavior | MisbehaviorDetector quarantine |
| Resource Exhaustion | Connection limits + timeouts |
| First-Use Attacks | Bootstrap trust establishment |

## Security Rationale

### Why TOFU Instead of Strict Pre-Verification?

1. **Network Bootstrap**: Nodes need to connect before establishing trust
2. **Decentralization**: No central authority to verify certificates
3. **Flexibility**: Trust relationships evolve over time
4. **User Experience**: Reduces friction for legitimate connections

### Why Not PKI or Certificate Authorities?

1. **Single Point of Failure**: CA compromise affects all nodes
2. **Centralization**: Contradicts ICN's decentralized architecture
3. **Cost & Complexity**: Requires certificate management infrastructure
4. **Trust Mismatch**: Social trust != hierarchical CA trust

## Comparison with Other Models

| Model | Bootstrap | Flexibility | Security | Implementation |
|-------|-----------|-------------|----------|----------------|
| PKI | Medium | Low | High | Complex |
| Web-of-Trust | Easy | High | Medium | Medium |
| TOFU (ICN) | Easy | High | Medium-High | Simple |
| Pre-Shared Keys | Hard | Low | High | Simple |

## Testing

All previously flaky tests now pass reliably:
- `test_contract_deployment_integration` (7 tests)
- `test_topology_integration` (6 tests)

These tests verify:
- Multi-node gossip synchronization
- Contract deployment across trust boundaries
- Regional topology formation
- Peer sampling with trust awareness

## Future Enhancements

### Potential Improvements

1. **Cert Pinning**: Remember first-seen certificates
2. **Trust Attestations**: Third-party trust vouchers
3. **Revocation Lists**: Distributed revocation mechanism
4. **Threshold Signatures**: Multi-party cert issuance
5. **Post-Quantum Crypto**: Hybrid key exchange

### Migration Path

If stricter security is needed:

1. Add `min_trust_threshold` to TLS verifier (>0.0)
2. Implement cert pinning for known peers
3. Add trust attestation gossip protocol
4. Deploy distributed revocation system

## References

- [TOFU Security Model](https://en.wikipedia.org/wiki/Trust_on_first_use)
- [TLS 1.3 Specification](https://datatracker.ietf.org/doc/html/rfc8446)
- [Decentralized Trust Systems](https://www.w3.org/TR/did-core/)
- [ICN Trust Graph Documentation](../TRUST_GRAPH.md)

## See Also

- `docs/ARCHITECTURE.md` - Overall system architecture
- `docs/production-hardening.md` - Security hardening measures
- `icn/crates/icn-net/src/tls.rs` - TLS implementation
- `icn/crates/icn-net/src/actor.rs` - Hello message handling
