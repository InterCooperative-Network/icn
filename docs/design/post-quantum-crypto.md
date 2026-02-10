# Post-Quantum Cryptography in ICN

## Overview

ICN supports **hybrid post-quantum cryptography** that combines classical Ed25519 signatures with ML-DSA (Module-Lattice Digital Signature Algorithm, aka Dilithium3) to provide quantum-resistant security.

This implementation follows the defense-in-depth principle: **both signatures must be valid** for verification to succeed. An attacker must break BOTH classical and post-quantum algorithms to forge a signature.

## Architecture

### Two-Tier PQ Support

ICN implements post-quantum cryptography at two levels:

1. **SDIS/Anchor Identities** (Always PQ): The `KeyBundle` used by Anchor-based identities (`did:icn:<anchor-id>`) has **always** used hybrid signatures since v4.
   
2. **Core Identities** (Optional PQ): Traditional Ed25519-based DIDs (`did:icn:<pubkey>`) can be **upgraded** to use hybrid signatures via the `post-quantum` feature flag.

### Feature Flag

The `post-quantum` feature is **optional** to keep ICN lightweight for resource-constrained nodes:

```toml
# Cargo.toml
[features]
default = []
post-quantum = []

[dependencies]
icn-crypto-pq.workspace = true
```

When disabled, core identities use classical Ed25519 only. When enabled, newly generated identities automatically get ML-DSA keys, and existing identities can be upgraded.

## Cryptographic Primitives

### Signature Algorithms

| Algorithm | Key Size | Signature Size | Security Level |
|-----------|----------|----------------|----------------|
| Ed25519 (Classical) | 32 bytes | 64 bytes | ~128-bit classical |
| ML-DSA-65 (Post-Quantum) | 1952 bytes (public)<br>4032 bytes (secret) | 3309 bytes | ~128-bit post-quantum |
| **Hybrid** | **~2 KB** | **~3.4 KB** | **Both** |

### Key Encapsulation

For encryption, ICN uses **hybrid KEM**:
- **Classical**: X25519
- **Post-Quantum**: ML-KEM (Kyber768)
- **Combined**: XOR or KDF mix of both shared secrets

## Identity Structure

### KeyPair with PQ Support

```rust
pub struct KeyPair {
    secret_bytes: Zeroizing<[u8; 32]>,
    verifying_key: VerifyingKey,
    did: Did,
    
    // Only present when post-quantum feature is enabled
    #[cfg(feature = "post-quantum")]
    pq_keypair: Option<icn_crypto_pq::MlDsaKeypair>,
}
```

### Hybrid Signature Format

```rust
pub struct HybridSignature {
    classical: Vec<u8>,  // 64 bytes (Ed25519)
    pq: Vec<u8>,         // 3309 bytes (ML-DSA)
}
```

### Verification Rules

1. **Classical-only identity** signing with `.sign()` → Classical Ed25519 signature (64 bytes)
2. **Hybrid identity** signing with `.sign()` → Still returns Ed25519 signature (backward compatible)
3. **Hybrid identity** signing with `.sign_hybrid()` → Returns `HybridSignatureOrClassical` enum

```rust
pub enum HybridSignatureOrClassical {
    Hybrid(icn_crypto_pq::HybridSignature),
    Classical(ed25519_dalek::Signature),
}
```

Verification logic:
- `Classical` variant: Verify with Ed25519 only
- `Hybrid` variant: **BOTH** Ed25519 and ML-DSA must verify successfully

## DID Format

### Design Decision: Non-Breaking DID Format

**Option A (Implemented)**: Keep DID derived from Ed25519 key only
- DID: `did:icn:<base58-ed25519-pubkey>`
- PQ public key discovered via `IdentityBundle` or DID Document
- **Advantage**: Existing identities can be upgraded without changing DID
- **Downside**: DID string doesn't cryptographically bind to PQ key

**Option B (Not Implemented)**: Hash combined keys
- DID: `did:icn:<hash(ed25519||ml-dsa)>`
- **Advantage**: Locks PQ key to identity at birth
- **Downside**: Breaks backward compatibility, forces all nodes to upgrade

We chose **Option A** for easier migration.

## Keystore Format

### v4 Format (SDIS)

Already supports hybrid signatures via `KeyBundle`:

```rust
struct StoredKeyV4 {
    version: u8,
    
    // Classical identity keys
    secret_bytes: [u8; 32],
    public_bytes: [u8; 32],
    did: String,
    
    // TLS binding
    tls_cert_der: Vec<u8>,
    tls_key_der: Vec<u8>,
    tls_binding_sig: Vec<u8>,
    
    // X25519 encryption
    x25519_secret: Vec<u8>,
    x25519_public: [u8; 32],
    
    // Multi-device
    did_document: DidDocument,
    device_id: String,
    rotation_chain: Vec<RotationEvent>,
    
    // SDIS
    anchor: Option<Anchor>,
    keybundles: Vec<StoredKeyBundleV4>,
    
    // PQ keys for core identity (v5 addition)
    #[cfg(feature = "post-quantum")]
    pq_secret: Option<Vec<u8>>,
    #[cfg(feature = "post-quantum")]
    pq_public: Option<Vec<u8>>,
}
```

### Backward Compatibility

- **v1/v2/v3 keystores**: Load as classical-only identities
- **v4 keystores without PQ**: Load as classical-only
- **v4 keystores with PQ**: Load with hybrid support
- **Upgrade path**: `icnctl id upgrade-pq` adds PQ keys without changing DID

## CLI Commands

### Generate PQ Identity

When the `post-quantum` feature is enabled, newly generated identities automatically include ML-DSA keys:

```bash
# Build with PQ support
cargo build --features post-quantum

# Initialize new identity (automatically hybrid)
./target/debug/icnctl id init
```

### Upgrade Existing Identity

Existing classical identities can be upgraded:

```bash
# Upgrade to post-quantum
./target/debug/icnctl id upgrade-pq

# Output:
# ✓ Post-quantum upgrade successful!
#   DID: did:icn:z... (unchanged)
#   Classical: Ed25519 (32-byte keys, 64-byte signatures)
#   Post-Quantum: ML-DSA-65 (~2KB keys, ~3.3KB signatures)
#   Security: Hybrid (both signatures required)
```

### Check PQ Status

```bash
./target/debug/icnctl id show

# Output includes:
# PQ Keys: Yes / No
```

## Network Protocol Integration

### SignedEnvelope

The `SignedEnvelope` used for network messages accepts variable-length signatures:

```rust
pub struct SignedEnvelope {
    pub from: Did,
    pub payload: Vec<u8>,
    pub signature: Vec<u8>,  // Can hold 64 bytes or ~3.4 KB
    pub timestamp: u64,
}
```

QUIC handles fragmentation automatically, so large signatures don't break the protocol.

### EncryptedEnvelope

For end-to-end encryption, integrate ML-KEM:

```rust
// Current (classical only)
let shared_secret = x25519_ecdh(my_secret, peer_public);

// Hybrid (when PQ enabled)
let classical_secret = x25519_ecdh(my_x25519_secret, peer_x25519_public);
let pq_secret = ml_kem_decapsulate(ciphertext, my_ml_kem_secret);
let combined_secret = kdf([classical_secret, pq_secret]);
```

This requires:
1. Extending `IdentityBundle` to include ML-KEM public key
2. Updating `EncryptedEnvelope` creation in `icn-net`
3. Negotiating hybrid encryption during QUIC handshake

**Status**: Not yet implemented. Current encryption remains X25519-only even with PQ signatures enabled.

## Protocol Negotiation

This section documents how nodes negotiate post-quantum capabilities during connection establishment.

### Capability Flags

Two capability flags control PQ behavior (defined in `icn-net/src/version.rs`):

| Flag | Bit Value | Purpose |
|------|-----------|---------|
| `HYBRID_SIGNATURES` | `0b1000000000` | Ed25519 + ML-DSA signatures |
| `HYBRID_KEM` | `0b10000000000` | X25519 + ML-KEM encryption |

These flags are only advertised when the `post-quantum` feature is enabled at compile time.

### Hello Message Exchange

The Hello message includes optional PQ fields (defined in `icn-net/src/protocol.rs`):

```rust
Hello {
    binding_info: BindingInfo,
    version_info: Option<VersionInfo>,
    topology_info: Option<TopologyInfo>,
    x25519_public: [u8; 32],
    ml_dsa_public: Option<Vec<u8>>,        // ~1952 bytes
    ml_kem_public: Option<Vec<u8>>,        // ~1184 bytes
    pq_binding_proof: Option<PqBindingProof>,
}
```

> **Note**: PQ fields use `#[serde(default)]` for backward compatibility—pre-PQ nodes can deserialize Hello messages without these fields.

### PQ Binding Proof

The `PqBindingProof` cryptographically binds the ML-DSA public key to the sender's DID, preventing key substitution attacks:

```rust
pub struct PqBindingProof {
    pub timestamp_millis: u64,
    pub signature: Vec<u8>,  // 3309 bytes (ML-DSA)
}
```

**Message format signed**: `"DID-PQ-BINDING-V1:<did>:<timestamp_millis>"`

**Validation rules**:
- Timestamp max age: 5 minutes (prevents replay of old proofs)
- Future tolerance: 1 minute (clock skew allowance)
- Signature must verify with the provided ML-DSA public key

### Negotiation Sequence

```
Node A                                    Node B
   |                                         |
   |-- Hello (ml_dsa_pub, pq_binding_proof) ->|
   |                                         |
   |<- Hello (ml_dsa_pub, pq_binding_proof) --|
   |                                         |
   |   [Compute common_capabilities()]       |
   |   [Verify PQ binding proofs]            |
   |   [Store peer PQ keys if verified]      |
   |                                         |
   |== Use hybrid signatures if both support =|
```

The `handle_hello()` handler in `icn-net/src/handlers/hello.rs` processes these steps:

1. Verify DID-TLS binding (TOFU model)
2. Verify DID-PQ binding proof (if present)
3. Negotiate protocol version
4. Compute capability intersection
5. Validate PQ key format
6. Store validated peer info

### Fallback Behavior

| Scenario | Behavior |
|----------|----------|
| Both support `HYBRID_SIGNATURES` | Use hybrid (both must verify) |
| Only sender supports PQ | Send classical signature |
| Receiver has PQ, sender doesn't | Accept classical signature |
| Invalid binding proof | **Reject connection** (fail-closed security policy) |
| Missing binding proof (legacy node) | Accept with classical-only capability (PQ keys ignored) |
| Advertises `HYBRID_SIGNATURES` but no `ml_dsa_public` | Log warning, discard capability (treat as classical) |

### Peer State Storage

Validated PQ keys are cached in `PeerConnectionInfo` for the session:

```rust
pub struct PeerConnectionInfo {
    did: Did,
    negotiated_version: u32,
    peer_capabilities: CapabilityFlags,
    peer_software: String,
    x25519_key: [u8; 32],
    ml_dsa_public: Option<Vec<u8>>,  // Only if HYBRID_SIGNATURES negotiated
    ml_kem_public: Option<Vec<u8>>,  // Only if HYBRID_KEM negotiated
}
```

When sending messages, check `peer_capabilities.contains(HYBRID_SIGNATURES)` to decide whether to use `sign_hybrid()` or classical `sign()`.

## Security Properties

### Downgrade Protection

Once a peer advertises PQ capability (via `IdentityBundle` or DID Document), verifiers **must** require hybrid signatures. Classical-only signatures are rejected.

This prevents MITM downgrade attacks where an attacker strips PQ keys.

### Quantum Timeline

- **Classical Ed25519**: Vulnerable to Shor's algorithm on a CRQC (Cryptographically Relevant Quantum Computer)
- **ML-DSA (Dilithium)**: Based on lattice problems, believed quantum-resistant
- **Hybrid Construction**: Secure as long as **either** algorithm remains unbroken

NIST estimates CRQCs may exist by 2030-2035. Upgrading identities now provides forward security against "harvest now, decrypt later" attacks.

## Performance Considerations

### Signature Size Impact

- **Ed25519 signature**: 64 bytes
- **Hybrid signature**: ~3.4 KB (53× larger)
- **QUIC MTU**: 1200 bytes typical
- **Fragmentation**: Automatic via QUIC streams

Large signatures don't block the protocol, but they do increase bandwidth:
- **Gossip messages**: ~3.4 KB overhead per signed message
- **Ledger entries**: ~3.4 KB overhead per transaction
- **Network traffic**: Approximately 50× increase in signature bandwidth

### Key Generation Time

- **Ed25519**: ~100 μs
- **ML-DSA**: ~10 ms (100× slower)

Key generation is infrequent (once per identity or rotation), so the delay is acceptable.

### Verification Time

- **Ed25519**: ~50 μs
- **ML-DSA**: ~150 μs
- **Hybrid**: ~200 μs (both must verify)

Verification is fast enough for real-time use.

## Testing

### Unit Tests

All existing tests pass with and without the `post-quantum` feature:

```bash
# Classical mode (default)
cargo test -p icn-identity

# Post-quantum mode
cargo test -p icn-identity --features post-quantum
```

### Integration Tests

To test PQ upgrade workflow:

```bash
# Start node without PQ
cargo run --bin icnd

# In another terminal, upgrade identity
cargo run --bin icnctl --features post-quantum -- id upgrade-pq

# Restart node with PQ support
cargo run --bin icnd --features post-quantum
```

## Roadmap

### Phase 1: Signatures (COMPLETE)

- [x] Hybrid signature support in `icn-crypto-pq`
- [x] Feature-gated PQ keys in `KeyPair`
- [x] Keystore v4 with PQ fields
- [x] `icnctl id upgrade-pq` command
- [x] Verification with downgrade protection

### Phase 2: Encryption (TODO)

- [ ] Add ML-KEM public key to `IdentityBundle`
- [ ] Hybrid KEM in `EncryptedEnvelope`
- [ ] QUIC handshake negotiation for hybrid encryption
- [ ] `icnctl id upgrade-kem` command

### Phase 3: Default Rollout (TODO)

- [ ] Enable `post-quantum` by default in workspace
- [ ] Automatic PQ upgrade on keystore unlock
- [ ] Migration guide for existing networks
- [ ] Performance benchmarks with PQ enabled

### Phase 4: Pure PQ Mode (FUTURE)

- [ ] Optional "pure PQ" mode (ML-DSA only, no Ed25519)
- [ ] Smaller DID format using ML-DSA-44 (smaller variant)
- [ ] Migration path from hybrid → pure PQ

## References

- **NIST FIPS 204**: Module-Lattice-Based Digital Signature Standard (ML-DSA)
- **NIST FIPS 203**: Module-Lattice-Based Key Encapsulation Mechanism (ML-KEM)
- **pqcrypto-dilithium**: Rust implementation used by ICN
- **Hybrid Signature Standard**: NIST SP 800-227 (Draft)

## FAQs

**Q: Should I enable post-quantum now?**  
A: If you're starting a new network, yes. For existing networks, plan a coordinated upgrade to minimize disruption.

**Q: What happens if I upgrade but my peers haven't?**  
A: Your node will still generate classical signatures for backward compatibility via `.sign()`. Use `.sign_hybrid()` when you know the peer supports PQ.

**Q: Can I downgrade after upgrading?**  
A: No. Once a keystore has PQ keys, it cannot be downgraded without losing the PQ keys. Backup before upgrading.

**Q: Why not make PQ the default?**  
A: Resource-constrained edge nodes (e.g., IoT devices) benefit from smaller signatures. The feature flag allows opt-in based on threat model.

**Q: What about X25519 encryption?**  
A: Encryption upgrade is Phase 2. Current encryption remains X25519-only even with PQ signatures enabled.
