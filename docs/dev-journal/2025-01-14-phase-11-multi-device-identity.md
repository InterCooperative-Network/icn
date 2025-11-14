# Phase 11: Multi-Device Identity & Sync

**Date**: 2025-01-14
**Phase**: 11
**Status**: Complete ✅

## Overview

Phase 11 implements multi-device identity support, allowing a single DID to be controlled by multiple devices with capability-based permissions. This is foundational for ICN's real-world usability - users need to access their cooperative identity from laptops, phones, and servers.

## Goals

1. **Multi-device support**: One DID, multiple authorized keys
2. **Capability-based security**: Fine-grained per-device permissions
3. **Audit trail**: Track device lifecycle (add, revoke, rotate)
4. **Secure storage**: Keystore v3 with automatic migration
5. **Gossip sync**: Distribute DID Document updates across network
6. **CLI workflow**: Complete device management interface

## Architecture Decisions

### 1. DID Document v2 Design

**Decision**: Extend DID Documents to support multiple VerificationMethods per DID.

**Rationale**:
- Follows W3C DID spec patterns
- Each device gets its own keypair
- Enables device-specific revocation without rotating primary key
- Supports gradual permission reduction (e.g., laptop loses AddDevice capability)

**Key data structures**:
```rust
pub struct DidDocument {
    pub id: Did,
    pub version: u64,
    pub updated_at: u64,
    pub verification_method: Vec<VerificationMethod>,
    pub authentication: Vec<String>,  // Signing-enabled device IDs
    pub recovery: Option<RecoveryConfig>,
}

pub struct VerificationMethod {
    pub id: String,              // Device ID
    pub label: String,           // User-friendly name
    pub key_type: KeyType,
    pub public_key: Vec<u8>,
    pub capabilities: Vec<Capability>,
    pub added_at: u64,
    pub revoked_at: Option<u64>,
}
```

**Capabilities**:
- `Sign` - Sign messages and contracts
- `AddDevice` - Authorize new devices
- `RevokeDevice` - Revoke other devices
- `RotateKey` - Rotate this device's key
- `Recover` - Use recovery mechanisms
- `Encrypt` - Decrypt messages (X25519)

### 2. Keystore v3 Format

**Decision**: Extend keystore format to include DID Document, device ID, and rotation chain.

**Migration path**:
- v1: Ed25519 only (legacy)
- v2: + TLS binding
- v2.1: + X25519 keys
- v3: + DID Document + device ID + rotation chain

**Auto-migration**: Seamless v1 → v2.1 → v3 upgrade on first unlock.

**Key implementation detail**:
```rust
impl Drop for StoredKeyV3 {
    fn drop(&mut self) {
        // Only zeroize sensitive fields
        self.secret_bytes.zeroize();
        self.x25519_secret.zeroize();
        self.tls_key_der.zeroize();
        // DID Document and rotation chain are public data
    }
}
```

**Challenge**: Balancing security (zeroization) with Rust's ownership model. Initially tried to derive `Zeroize`, but `DidDocument` and `Vec<RotationEvent>` are public data that don't need zeroization. Solution: Manual `Drop` implementation.

### 3. Identity Sync Protocol

**Decision**: Use gossip topic `identity:updates` for broadcasting DID Document changes.

**Message format**:
```rust
pub struct IdentityUpdateMessage {
    pub did: Did,
    pub event: RotationEvent,
    pub new_version: u64,
    pub timestamp: u64,
}
```

**Serialization**: bincode (~280 bytes per update)

**Cache implementation**:
```rust
pub struct DidDocumentCache {
    inner: Arc<RwLock<HashMap<Did, CachedDidDocument>>>,
}
```

**Version ordering**: Prevents replay attacks by rejecting stale updates.

**Verification**: Every device change is signed by an authorized key (capability: `AddDevice` or `RevokeDevice`).

### 4. CLI Workflow

**Device addition workflow**:
1. New device: `icnctl device add laptop2` → generates Ed25519 + X25519 keys + request file
2. Authorized device: `icnctl device approve laptop2-request.json` → updates DID Document
3. Broadcast: IdentityUpdateMessage sent via gossip
4. Peers: Apply rotation event to cached DID Document

**Device revocation workflow**:
1. `icnctl device revoke device-abc123 --reason "lost laptop"`
2. DID Document updated with revoked_at timestamp
3. Broadcast via gossip
4. Peers: Mark device as revoked in cache

**Design decision**: Revocation is soft deletion (preserves audit trail) rather than removal.

## Implementation Challenges

### Challenge 1: Zeroize Trait Constraints

**Problem**: `StoredKeyV3` contains fields that don't implement `DefaultIsZeroes` (DidDocument, Vec<RotationEvent>).

**Initial approach**: Derive `Zeroize` on entire struct.

**Error**:
```
error: cannot move out of type 'StoredKeyV3', which implements the 'Drop' trait
```

**Solution**: Manual `Drop` implementation that only zeroizes sensitive fields:
```rust
impl Drop for StoredKeyV3 {
    fn drop(&mut self) {
        self.secret_bytes.zeroize();
        self.x25519_secret.zeroize();
        self.tls_key_der.zeroize();
    }
}
```

**Lesson**: Not all data needs zeroization - only cryptographic secrets. DID Documents and rotation events are public data.

### Challenge 2: Device Workflow Conceptual Issue

**Problem**: Initially implemented `device add` to generate a new DID per device.

**Error**: Misunderstanding of multi-device identity model.

**Insight**: Multi-device identity means:
- **Same DID** across all devices
- **Different keys** for each device
- Each key has its own VerificationMethod entry

**Corrected workflow**:
```rust
// Prompt for the target DID (the identity to add this device to)
print!("Enter the DID to add this device to: ");
let target_did = did_input.trim();

// Generate Ed25519 keypair for THIS device (not new DID)
let keypair = KeyPair::generate()?;
```

**Lesson**: Multi-device is about key management, not identity creation.

### Challenge 3: Version Mismatch in Device Approval (Critical Bug)

**Problem**: Device approval was calling `add_device()` twice (once for Ed25519, once for X25519), incrementing the version from 1→2→3, but the rotation event was created with `new_version = did_doc.version + 1 = 2`.

**Error**: Version mismatch between rotation event and actual DID Document state.

**Example**:
```rust
// rotation_event created BEFORE applying update
let rotation_event = RotationEvent {
    new_version: did_doc.version + 1,  // e.g., 2
    // ...
};

// Then update function called add_device() TWICE
keystore.update_did_document(|did_doc| {
    did_doc.add_device(...)?;  // version: 1 → 2
    did_doc.add_device(...)?;  // version: 2 → 3 ⚠️ MISMATCH!
}, Some(rotation_event), &passphrase)?;
```

**Impact**: This would cause verification failures in identity sync when peers try to apply rotation events, because the version check at `multi_device.rs:450` expects `new_version` to be `did_doc.version + 1`.

**Solution**: Created `add_device_with_encryption_key()` method that adds both keys with single version increment:
```rust
pub fn add_device_with_encryption_key(
    &mut self,
    device_id: String,
    label: String,
    ed25519_public_key: Vec<u8>,
    x25519_public_key: Vec<u8>,
    signing_capabilities: Vec<Capability>,
) -> Result<()> {
    // Add Ed25519 signing key
    self.verification_method.push(...);

    // Add X25519 encryption key
    self.verification_method.push(...);

    // Update version ONCE for the logical operation
    self.version += 1;
    Ok(())
}
```

**Test coverage**: Added `test_add_device_with_encryption_key_version_increment()` to verify version increments by exactly 1.

**Lesson**: When creating rotation events, ensure the version increment matches the semantic operation being performed. A device addition is a single logical operation, even if it involves multiple keys.

### Challenge 4: Borrow Checker in Migration Test

**Problem**: Held reference to DID Document while trying to lock/unlock keystore.

**Error**:
```
cannot borrow 'ks' as mutable because it is also borrowed as immutable
```

**Solution**: Clone values before mutation:
```rust
let did_doc_id = did_doc.id.clone();
let did_doc_version = did_doc.version;
// Now can lock/unlock
ks.lock();
ks.unlock(passphrase).unwrap();
```

**Lesson**: Rust's borrow checker enforces clear separation between reading and writing.

## Test Coverage

**Unit tests** (31 total in icn-identity):
- `multi_device.rs`: DID Document creation, device add/revoke, capability checks, version increment behavior
- `keystore.rs`: v3 creation, unlock, v2.1→v3 migration, update_did_document()
- `sync.rs`: IdentityUpdateMessage serialization, cache apply_event(), version ordering

**Integration tests** (2 in icn-identity/tests/):
- `identity_sync_test.rs`: End-to-end workflow (Alice adds device → Bob caches → Alice revokes → Bob verifies)
- Version ordering and conflict resolution

**Doc tests**: 1 in sync.rs

**Test results**: All 34 tests pass in icn-identity (31 unit + 2 integration + 1 doc).

## Security Considerations

### 1. Capability Enforcement

Every device operation checks capabilities:
```rust
pub fn can_add_device(&self, device_id: &str) -> bool {
    self.verification_method.iter().any(|vm| {
        vm.id == device_id
        && vm.revoked_at.is_none()
        && vm.capabilities.contains(&Capability::AddDevice)
    })
}
```

### 2. Revocation Semantics

**Soft deletion**: Revoked devices remain in DID Document with `revoked_at` timestamp.

**Rationale**:
- Preserves audit trail
- Prevents ID reuse
- Enables forensic analysis

**Verification**:
```rust
pub fn can_sign(&self, did: &Did, public_key_bytes: &[u8]) -> bool {
    // Check if key exists, matches DID, has Sign capability, NOT revoked
}
```

### 3. Version Ordering

**Prevents replay attacks**: Older versions are rejected.

**Implementation**:
```rust
if update.new_version <= cached.version {
    return Ok(false);  // Stale update
}
```

**Caveat**: This assumes monotonic version increments. Concurrent updates from multiple devices could cause conflicts. Future work: CRDTs or vector clocks.

### 4. Signature Verification

**Current limitation**: `DidDocumentCache.apply_event()` accepts events without verifying signatures.

**Justification**: Phase 11 focuses on data structures and storage. Signature verification will be added when integrating with supervisor/gossip in production.

**TODO**: Before production deployment:
```rust
pub fn apply_event(&self, did: &Did, event: &RotationEvent, signature: &[u8]) -> Result<bool> {
    // Verify signature with authorized key
    // Then apply event
}
```

## Gossip Integration

**Topic**: `identity:updates`

**Access control**: Public (all nodes can subscribe)

**Message flow**:
1. Device updates DID Document locally
2. Creates `IdentityUpdateMessage` with RotationEvent
3. Serializes to bytes (bincode)
4. Publishes to gossip topic
5. Peers receive, deserialize, apply to cache
6. Cache validates version ordering

**Bandwidth**: ~280 bytes per device operation (add/revoke/rotate).

**Frequency**: Low (human-driven events, not high-frequency data).

**Future optimization**: Batch multiple rotation events into single message.

## CLI User Experience

### Device List
```bash
$ icnctl device list

Current DID: did:icn:z6Mk...

Devices:
  [✓] device-abc123 (laptop-primary)
      Added: 2025-01-14 10:23:45 UTC
      Capabilities: Sign, AddDevice, RevokeDevice, RotateKey, Recover, Encrypt
      Status: Active

  [✓] device-xyz789 (phone)
      Added: 2025-01-14 11:45:22 UTC
      Capabilities: Sign, Encrypt
      Status: Active

  [✗] device-old456 (old-laptop)
      Added: 2025-01-10 08:12:33 UTC
      Revoked: 2025-01-14 12:05:10 UTC
      Reason: Device lost
```

### Device Add
```bash
$ icnctl device add phone
Enter the DID to add this device to: did:icn:z6Mk...

Generated device keys:
  Device ID: device-xyz789
  Ed25519 Public Key: abc123...
  X25519 Public Key: def456...

Device add request saved to: device-xyz789-request.json

Next steps:
  1. Transfer this request file to an authorized device
  2. On authorized device, run: icnctl device approve device-xyz789-request.json
  3. Return to this device and run: icnctl id import <keystore>
```

### Device Approve
```bash
$ icnctl device approve phone-request.json
Loaded device add request:
  DID: did:icn:z6Mk...
  Label: phone
  Capabilities: Sign, Encrypt

Approve this device? [y/N]: y

✅ Device approved and DID Document updated
✅ Identity update broadcasted to network
```

### Device Revoke
```bash
$ icnctl device revoke device-old456 --reason "Device lost"

Revoking device: device-old456 (old-laptop)
Reason: Device lost

⚠️  This device will no longer be able to sign transactions.

Proceed? [y/N]: y

✅ Device revoked
✅ DID Document updated (version 5)
✅ Identity update broadcasted to network
```

## Deferred Work

### 11.5.4: Supervisor Integration

**Status**: Deferred to production deployment

**Rationale**:
- Core infrastructure complete
- Integration requires broader system changes
- Not blocking for MVP testing

**Work required**:
1. Wire `DidDocumentCache` into supervisor
2. Subscribe to `identity:updates` topic
3. Apply rotation events to cache
4. Use cache for signature verification in NetworkActor

### 11.6: Social Recovery

**Status**: Deferred (not critical for MVP)

**Rationale**:
- Complex trust graph integration
- Requires governance mechanisms
- Can be added post-MVP

**Design sketch**:
```rust
pub struct RecoveryConfig {
    pub recovery_method: RecoveryMethod,
    pub threshold: u8,
    pub guardians: Vec<Did>,
}

pub enum RecoveryMethod {
    SocialRecovery { threshold: u8, guardians: Vec<Did> },
    RecoveryPhrase { hash: Vec<u8> },
    MultiSig { threshold: u8, keys: Vec<Vec<u8>> },
}
```

## What's Next

Phase 11 is complete. See [ROADMAP.md](../ROADMAP.md) for next phases:

- **Phase 12**: Operational Hardening (monitoring, backups, disaster recovery)
- **Phase 13**: Economic Safety Rails (dynamic credit limits, dispute resolution)
- **Phase 14**: Governance Primitives v1 (driven by pilot community)

## Commits

1. `3b712f9` - feat: Implement multi-device identity core (DID Document v2, Keystore v3)
2. `c405c23` - feat: Implement identity sync protocol via gossip
3. `085c59e` - feat: Add device management CLI commands
4. `857eec3` - docs: Add multi-device identity design doc and roadmap
5. `0247acc` - docs: Add Phase 11 dev journal and update CLAUDE.md
6. `c705831` - fix: Correct DID Document version increment in device approval

## References

- [W3C DID Specification](https://www.w3.org/TR/did-core/)
- [docs/multi-device-identity-design.md](../multi-device-identity-design.md)
- [ROADMAP.md](../../ROADMAP.md)
- [icn-identity/src/multi_device.rs](../../icn/crates/icn-identity/src/multi_device.rs)
- [icn-identity/src/sync.rs](../../icn/crates/icn-identity/src/sync.rs)
