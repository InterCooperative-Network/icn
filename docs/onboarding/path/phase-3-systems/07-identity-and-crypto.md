# 07: Identity and Cryptography — Practical Crypto for ICN

> **Phase**: 3 | **Tier**: Builder  
> **Patterns introduced**: [Signed Envelope](#signed-envelope), [Replay Protection](#replay-protection)  
> **Prerequisite**: Phase 2 complete (Fixer tier)

## Why This Matters

ICN uses cryptography for **identity** (DIDs), **message integrity** (Ed25519 signatures), **confidentiality** (X25519 encryption), and **future-proofing** (post-quantum hybrid schemes). This layer teaches practical crypto — how to sign, verify, and protect against replay attacks.

→ See `manual.md` § "Cryptographic Foundations" for design rationale.

## What You'll Read

### 1. DID Format: `icn-identity/src/lib.rs`

DIDs (Decentralized Identifiers) are ICN's identity primitive:

```rust
pub struct Did(String);  // Format: did:icn:<base58-pubkey>

impl Did {
    pub fn from_public_key(pubkey: &[u8]) -> Self {
        let encoded = bs58::encode(pubkey).into_string();
        Did(format!("did:icn:{}", encoded))
    }
}
```

**Key properties**:
- Self-sovereign: No central authority
- Cryptographically verifiable: DID → public key → signature verification
- Immutable: Once created, never changes

### 2. Ed25519 Keys and Age-Encrypted Keystore

**Files**: `icn-identity/src/keystore.rs`, `icn-identity/src/bundle.rs`

ICN stores keys encrypted at rest using the `age` encryption format:

```rust
// Create keystore
let bundle = IdentityBundle::new()?;

// Encrypt and save
bundle.save_encrypted(path, passphrase)?;

// Load and decrypt
let loaded = IdentityBundle::load_encrypted(path, passphrase)?;
```

**Keystore auto-migration**: v1 → v2 → v2.1 (adds TLS binding + X25519 keys)

### 3. Post-Quantum Signing: `icn-crypto-pq/src/signature.rs`

ICN uses **hybrid signatures** for quantum resistance:

```rust
pub struct HybridSignature {
    ed25519: Ed25519Signature,    // Classical
    ml_dsa: MlDsaSignature,        // Post-quantum (Dilithium)
}
```

**Verification**: Both signatures must validate (AND logic).

**Why hybrid**: Protects against future quantum computers while maintaining backwards compatibility.

### 4. ReplayGuard: `icn-net/src/envelope.rs`

Prevents replay attacks using sequence numbers:

```rust
pub struct ReplayGuard {
    last_seen: HashMap<Did, u64>,  // DID → highest sequence seen
}

impl ReplayGuard {
    pub fn check(&mut self, sender: &Did, seq: u64) -> Result<(), ReplayError> {
        let last = self.last_seen.get(sender).unwrap_or(&0);
        if seq <= *last {
            return Err(ReplayError::StaleSequence);
        }
        self.last_seen.insert(sender.clone(), seq);
        Ok(())
    }
}
```

**Alternative**: Timestamp windows (accept messages within 5 minutes of current time).

### 5. Gossip State Persistence: `icn-gossip/src/gossip.rs`

Actors export/restore state for graceful restarts:

```rust
pub fn export_state(&self) -> GossipState {
    GossipState {
        entries: self.entries.clone(),
        vector_clocks: self.vector_clocks.clone(),
        subscriptions: self.subscriptions.keys().cloned().collect(),
    }
}

pub fn restore_state(&mut self, state: GossipState) {
    self.entries = state.entries;
    self.vector_clocks = state.vector_clocks;
    // Re-subscribe to topics
    for topic in state.subscriptions {
        self.subscribe(&topic, AccessControl::Public);
    }
}
```

**Why**: Prevents loss of gossip state during daemon restarts.

## What You'll Build

→ **Lab**: `labs/lab-06-signed-envelope/`

Implement signed envelopes with replay protection:
- Sign payload hash → verify → reject replay (nonce or timestamp window)
- Tests catch misuse: wrong key, tampered message, replayed nonce

**Done when**: Verification catches all three attack vectors.

## Checkpoint

You've completed this layer when you can:

1. **Generate and verify signatures**: Sign a message, verify it, explain the flow
2. **Identify replay attacks**: Recognize replay vulnerabilities in code
3. **Implement replay protection**: Add sequence tracking or timestamp windows
4. **Explain hybrid crypto**: Describe why ICN uses Ed25519+ML-DSA

**Artifact**: Lab-06 complete with replay attack tests.

## Deep Reference

→ `reference/module-04-identity-trust.md` — Full identity, keystore, trust graph  
→ `reference/module-13-security-privacy.md` — Security patterns, threat model  
→ `icn-crypto-pq/` — Post-quantum cryptography implementation
