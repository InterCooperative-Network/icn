# Multi-Device Identity Design (Phase 11)

**Status**: Design Phase
**Started**: 2025-01-13
**Target**: 3-4 weeks to implementation complete

## Problem Statement

**Current State**: ICN identity = single Ed25519 keypair stored in Age-encrypted keystore.

**Failure Modes**:
1. Device dies → lose entire economic history
2. Device stolen → must rotate key, invalidate all trust edges
3. Multiple devices → can't use ICN on phone + laptop
4. Key compromise → no recovery path

**Real-World Blocker**: No community will trust their economic coordination to a system where hardware failure = total loss.

---

## Design Goals

1. **Multi-Device Support**: Same DID usable from multiple devices
2. **Key Rotation**: Clean rotation without breaking trust/ledger history
3. **Recovery**: Survive total device loss via social recovery or backup
4. **Backward Compatible**: Migrate existing v2.1 keystores without data loss
5. **Security**: No single point of failure, clear revocation semantics

---

## Architecture Overview

### Core Components

```
┌─────────────────────────────────────────────────────────────┐
│                      DID Document v2                         │
│  (Canonical list of authorized keys + capabilities)         │
└─────────────────────────────────────────────────────────────┘
                           │
                           │ references
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                  Verification Methods                        │
│  - device-1: Ed25519 key (laptop) [sign, rotate, recover]   │
│  - device-2: Ed25519 key (phone) [sign]                     │
│  - device-3: X25519 key (encryption)                         │
└─────────────────────────────────────────────────────────────┘
                           │
                           │ stored in
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                  Keystore v3 Format                          │
│  - Primary identity bundle (device-specific)                │
│  - DID Document (shared identity state)                     │
│  - Rotation chain (audit log)                               │
└─────────────────────────────────────────────────────────────┘
                           │
                           │ synced via
                           ▼
┌─────────────────────────────────────────────────────────────┐
│               Identity Sync Protocol (NEW)                   │
│  - Gossip topic: "identity:updates"                         │
│  - Broadcasts: DID Document changes, rotation events        │
│  - Trust-gated: Only visible to existing trust edges        │
└─────────────────────────────────────────────────────────────┘
```

---

## Data Structures

### DID Document v2

```rust
/// DID Document version 2: Multi-device identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidDocument {
    /// The DID this document describes (did:icn:base58-pubkey)
    /// This is the *original* keypair's DID, remains stable across rotations
    pub id: DID,

    /// Version number (incremented on each update)
    pub version: u64,

    /// Timestamp of last update
    pub updated_at: u64,

    /// List of authorized verification methods (keys)
    pub verification_method: Vec<VerificationMethod>,

    /// Which keys can authenticate (sign messages)
    pub authentication: Vec<String>,  // refs to verification_method IDs

    /// Optional recovery configuration
    pub recovery: Option<RecoveryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMethod {
    /// Unique ID within this DID Document (e.g., "device-1", "device-2")
    pub id: String,

    /// Human-readable label
    pub label: String,  // e.g., "Matt's Laptop", "Phone"

    /// Key type
    pub key_type: KeyType,

    /// The actual public key
    pub public_key: PublicKey,

    /// What this key is authorized to do
    pub capabilities: Vec<Capability>,

    /// When this key was added
    pub added_at: u64,

    /// Optional: When this key was revoked
    pub revoked_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyType {
    Ed25519,   // Signing
    X25519,    // Encryption
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Capability {
    /// Can sign messages on behalf of this DID
    Sign,

    /// Can add new devices
    AddDevice,

    /// Can revoke devices (including self)
    RevokeDevice,

    /// Can rotate keys
    RotateKey,

    /// Can participate in recovery
    Recover,

    /// Can encrypt/decrypt messages
    Encrypt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    pub method: RecoveryMethod,
    pub threshold: u8,
    pub trustees: Vec<DID>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryMethod {
    /// M-of-N social recovery
    Social { m: u8, n: u8 },

    /// Encrypted backup seed (offline)
    BackupSeed,

    /// No recovery (accept total loss risk)
    None,
}
```

### Key Rotation Event

```rust
/// Represents a key rotation or device change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationEvent {
    /// The DID being rotated
    pub did: DID,

    /// Event type
    pub event_type: RotationEventType,

    /// Signature by the authorized key performing this action
    pub proof: Signature,

    /// Which key signed this (must have RotateKey or RevokeDevice capability)
    pub signed_by: String,  // verification method ID

    /// Timestamp
    pub timestamp: u64,

    /// New DID Document version
    pub new_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationEventType {
    /// Add a new device
    AddDevice {
        device_id: String,
        public_key: PublicKey,
        capabilities: Vec<Capability>,
    },

    /// Revoke a device
    RevokeDevice {
        device_id: String,
        reason: RevocationReason,
    },

    /// Rotate a key (change the underlying keypair for a device)
    RotateKey {
        device_id: String,
        old_key: PublicKey,
        new_key: PublicKey,
    },

    /// Full recovery (new root key after total loss)
    Recover {
        new_root_key: PublicKey,
        recovery_proofs: Vec<RecoveryProof>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RevocationReason {
    /// Normal removal
    Removed,

    /// Device compromised
    Compromised,

    /// Device lost/stolen
    Lost,

    /// Key rotation
    Rotated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryProof {
    /// Trustee DID
    pub trustee: DID,

    /// Trustee's signature on recovery request
    pub signature: Signature,

    /// Timestamp
    pub timestamp: u64,
}
```

### Keystore v3 Format

```rust
/// Keystore version 3: Multi-device with DID Document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeystoreV3 {
    /// Format version
    pub version: u8,  // = 3

    /// This device's identity bundle
    pub identity: IdentityBundle,

    /// The canonical DID Document (shared across devices)
    pub did_document: DidDocument,

    /// This device's ID in the DID Document
    pub device_id: String,

    /// Rotation event history (audit log)
    pub rotation_chain: Vec<RotationEvent>,
}
```

---

## Key Workflows

### 1. Initial Setup (Fresh Identity)

```
User runs: icnctl id init --device-name "Laptop"

1. Generate Ed25519 keypair (primary)
2. Generate X25519 keypair (encryption)
3. Create DID from Ed25519 pubkey: did:icn:abc123
4. Create DID Document v2:
   - verification_method = [
       { id: "device-1", label: "Laptop", key_type: Ed25519, capabilities: [Sign, AddDevice, RevokeDevice, RotateKey, Recover] },
       { id: "enc-1", label: "Encryption Key", key_type: X25519, capabilities: [Encrypt] }
     ]
   - authentication = ["device-1"]
   - recovery = None (default, can configure later)
5. Create keystore v3
6. Encrypt and save to ~/.icn/keystore.age
```

**Output**: User has a working identity, ready to add more devices.

### 2. Add a Second Device

```
User runs on NEW device: icnctl device add --name "Phone"

Flow:
1. Generate new Ed25519 keypair on phone
2. Create AddDevice request:
   {
     device_id: "device-2",
     public_key: <phone's pubkey>,
     capabilities: [Sign, Encrypt]  // Note: NOT AddDevice/RevokeDevice (limited trust)
   }
3. Export request as QR code or file
4. On EXISTING device (laptop):
   icnctl device approve <request-file>
   - Verifies request
   - Updates DID Document (adds device-2)
   - Signs RotationEvent with device-1's key
   - Broadcasts update to network via gossip
5. Phone receives updated DID Document
6. Phone creates its own keystore v3 with:
   - New identity bundle (phone's keys)
   - Shared DID Document
   - device_id = "device-2"
```

**Result**: Both laptop and phone can sign as the same DID.

### 3. Revoke a Device

```
User runs: icnctl device revoke device-2 --reason "Lost phone"

1. Check: Does current device have RevokeDevice capability? (Yes, device-1 does)
2. Update DID Document:
   - Set device-2.revoked_at = now()
3. Create RotationEvent:
   {
     event_type: RevokeDevice { device_id: "device-2", reason: Lost },
     signed_by: "device-1",
     proof: <signature by device-1>
   }
4. Broadcast to network
5. All nodes see the update, stop accepting signatures from device-2
```

**Result**: Old device can no longer act as this DID.

### 4. Key Rotation (Proactive Security)

```
User runs: icnctl key rotate --device device-1

1. Generate new Ed25519 keypair
2. Create RotationEvent:
   {
     event_type: RotateKey {
       device_id: "device-1",
       old_key: <current pubkey>,
       new_key: <new pubkey>
     },
     signed_by: "device-1",
     proof: <signed by OLD key>
   }
3. Update DID Document:
   - Replace device-1's public_key with new key
   - Increment version
4. Broadcast to network
5. Update local keystore with new keypair
```

**Result**: Underlying key changed, but DID and trust/ledger history intact.

### 5. Social Recovery (Total Device Loss)

```
Setup (one-time):
icnctl recovery setup --method social --threshold 3 --trustees did:icn:alice,did:icn:bob,did:icn:carol

User loses ALL devices, gets a new one:
icnctl recovery initiate --did did:icn:abc123

1. Generate new Ed25519 keypair on new device
2. Create recovery request:
   {
     did: did:icn:abc123,
     new_root_key: <new device's pubkey>,
     requested_at: now()
   }
3. Send recovery request to trustees (out-of-band: email, Signal, in-person)
4. Each trustee runs:
   icnctl recovery approve <request> --trustee-key <their key>
   - Verifies identity (out-of-band: "Is this really Matt?")
   - Signs recovery proof
5. Once M-of-N signatures collected, user runs:
   icnctl recovery finalize <recovery-proofs>
   - Creates RotationEvent with Recover type
   - Includes all recovery proofs
   - Broadcasts to network
6. Network validates:
   - Check M-of-N threshold met
   - Verify trustee signatures
   - Accept new root key
7. New DID Document:
   - Old devices revoked
   - New device becomes device-1 with full capabilities
```

**Result**: User regains control of their DID without losing history.

---

## Security Considerations

### 1. **Capability Hierarchy**

Not all devices are equal:
- **Primary device** (e.g., laptop): Full capabilities (AddDevice, RevokeDevice, RotateKey)
- **Secondary device** (e.g., phone): Limited to Sign, Encrypt
- **Rationale**: If phone is lost, can't be used to add malicious devices

### 2. **Revocation Checking**

Every signature verification must:
1. Extract DID from signature
2. Fetch current DID Document for that DID
3. Check signing key is in `verification_method` AND not revoked
4. Check key has `Sign` capability

**Implementation**: NetworkActor caches DID Documents, gossip keeps them synced.

### 3. **Replay Protection Across Devices**

Current `ReplayGuard` uses per-peer sequence numbers. With multi-device:
- **Option A**: Shared sequence counter across devices (requires coordination)
- **Option B**: Per-device sequences, check against revocation
- **Recommendation**: Option B (simpler, no coordination overhead)

### 4. **Recovery Attack Prevention**

Malicious actor could try to initiate fake recovery:
- **Mitigation**: Recovery requires M-of-N trustees to verify identity out-of-band
- **Threshold**: Recommend M ≥ 3, N ≥ 5 (balance security vs. availability)
- **Trustee selection**: Should be high-trust, long-term relationships

### 5. **DID Document Sync**

DID Documents propagate via gossip:
- **Topic**: `identity:updates` (trust-gated, only visible to trust edges)
- **Conflict resolution**: Highest version number wins
- **Byzantine resistance**: Accept updates only if:
  1. Signed by a key with appropriate capability
  2. Version increments by 1 (no skipping)
  3. Rotation chain is valid

---

## Migration Path (v2.1 → v3)

### Automatic Migration on First Unlock

```rust
impl Keystore {
    pub fn unlock(passphrase: &[u8]) -> Result<KeystoreV3> {
        let data = decrypt(passphrase)?;

        match detect_version(&data)? {
            2 | 21 => {
                // Old format: just IdentityBundle
                let bundle: IdentityBundle = deserialize(data)?;

                // Create v3 keystore
                let did_doc = DidDocument {
                    id: bundle.did.clone(),
                    version: 1,
                    updated_at: now(),
                    verification_method: vec![
                        VerificationMethod {
                            id: "device-1".into(),
                            label: "Primary Device".into(),
                            key_type: KeyType::Ed25519,
                            public_key: bundle.keypair.public.clone(),
                            capabilities: vec![
                                Capability::Sign,
                                Capability::AddDevice,
                                Capability::RevokeDevice,
                                Capability::RotateKey,
                                Capability::Recover,
                            ],
                            added_at: now(),
                            revoked_at: None,
                        },
                        VerificationMethod {
                            id: "enc-1".into(),
                            label: "Encryption Key".into(),
                            key_type: KeyType::X25519,
                            public_key: bundle.x25519_keypair.public.clone(),
                            capabilities: vec![Capability::Encrypt],
                            added_at: now(),
                            revoked_at: None,
                        },
                    ],
                    authentication: vec!["device-1".into()],
                    recovery: None,  // User can configure later
                };

                let v3 = KeystoreV3 {
                    version: 3,
                    identity: bundle,
                    did_document: did_doc,
                    device_id: "device-1".into(),
                    rotation_chain: vec![],
                };

                // Save upgraded keystore immediately
                v3.encrypt_and_save(passphrase)?;

                info!("✅ Migrated keystore v2.1 → v3 (multi-device support enabled)");

                Ok(v3)
            }

            3 => {
                // Already v3
                Ok(deserialize(data)?)
            }

            v => bail!("Unsupported keystore version: {}", v),
        }
    }
}
```

**Guarantees**:
- Existing identities work unchanged
- DID remains stable (derived from original Ed25519 key)
- Trust edges and ledger history preserved
- Automatic, invisible to user

---

## Implementation Plan

### Phase 11.1: Data Structures (Week 1)
**Crate**: `icn-identity`

- [ ] Define `DidDocument`, `VerificationMethod`, `Capability` types
- [ ] Define `RotationEvent` and related types
- [ ] Define `KeystoreV3` format
- [ ] Implement serialization/deserialization
- [ ] Write unit tests for data structures

**Deliverable**: Core types compile, serialize correctly.

### Phase 11.2: DID Document Management (Week 1-2)
**Crate**: `icn-identity`

- [ ] Implement `DidDocument::new()` (create initial doc)
- [ ] Implement `DidDocument::add_device()`
- [ ] Implement `DidDocument::revoke_device()`
- [ ] Implement `DidDocument::rotate_key()`
- [ ] Implement `DidDocument::verify_signature()` (check key is authorized)
- [ ] Implement rotation event validation
- [ ] Write tests for each operation

**Deliverable**: DID Document CRUD operations work.

### Phase 11.3: Keystore v3 Migration (Week 2)
**Crate**: `icn-identity`

- [ ] Implement `KeystoreV3` type
- [ ] Implement v2.1 → v3 migration in `Keystore::unlock()`
- [ ] Implement `KeystoreV3::encrypt_and_save()`
- [ ] Test migration preserves identity, DID, keys
- [ ] Test idempotence (migrating twice is safe)

**Deliverable**: Existing keystores auto-upgrade to v3.

### Phase 11.4: icnctl Device Commands (Week 2-3)
**Crate**: `icnctl`

- [ ] `icnctl device list` - Show all devices for this DID
- [ ] `icnctl device add <name>` - Initiate device add flow
- [ ] `icnctl device approve <request>` - Approve device add
- [ ] `icnctl device revoke <device-id>` - Revoke a device
- [ ] `icnctl key rotate` - Rotate current device's key
- [ ] Test workflows end-to-end

**Deliverable**: Users can manage devices via CLI.

### Phase 11.5: Identity Sync Protocol (Week 3)
**Crates**: `icn-gossip`, `icn-core`

- [ ] Define gossip topic `identity:updates`
- [ ] Implement IdentityActor (manages local DID Documents)
- [ ] On device add/revoke/rotate: broadcast RotationEvent
- [ ] On receive RotationEvent: validate, update local DID cache
- [ ] NetworkActor checks DID Documents before accepting messages
- [ ] Test: Two nodes, add device on node1, node2 sees update

**Deliverable**: DID Document changes propagate across network.

### Phase 11.6: Recovery Mechanisms (Week 3-4)
**Crate**: `icn-identity`, `icnctl`

- [ ] Implement `icnctl recovery setup`
- [ ] Implement `icnctl recovery initiate`
- [ ] Implement `icnctl recovery approve` (trustee side)
- [ ] Implement `icnctl recovery finalize`
- [ ] Test social recovery flow with 3-of-5 threshold
- [ ] Document recovery best practices

**Deliverable**: Users can recover from total device loss.

### Phase 11.7: Integration & Testing (Week 4)
**All crates**

- [ ] Update Runtime to load DID Documents on startup
- [ ] Update Supervisor to sync identity changes
- [ ] End-to-end test: Multi-device gossip + ledger transactions
- [ ] End-to-end test: Device revocation prevents signing
- [ ] End-to-end test: Social recovery restores identity
- [ ] Update documentation

**Deliverable**: Phase 11 complete, all tests pass.

---

## Open Questions

### 1. **Device Add Flow: Interactive or File-Based?**

**Option A: Interactive (QR code)**
- New device generates request, shows QR
- Existing device scans QR, approves
- Pros: Good UX, secure
- Cons: Requires camera, GUI

**Option B: File-based (copy request file)**
- New device writes `device-add-request.json`
- User copies to existing device (USB, scp, etc.)
- Existing device runs `icnctl device approve device-add-request.json`
- Pros: Works on servers, no GUI needed
- Cons: Manual file transfer

**Recommendation**: Start with Option B (simpler), add Option A later.

### 2. **Recovery: Social vs. Backup Seed?**

**Social Recovery**:
- Pros: No single point of failure, social trust
- Cons: Requires coordinating with trustees, can fail if trustees unavailable

**Backup Seed**:
- Pros: Self-sovereign, always available
- Cons: Single point of failure (if backup stolen/lost)

**Recommendation**: Support both, let users choose. Default: Social recovery.

### 3. **DID Stability: What if root key rotates?**

**Current Design**: DID = hash of original Ed25519 public key, never changes.

**Implication**: Root key can rotate (via RotationEvent), but DID stays stable.

**Alternative**: DID = hash of DID Document, changes on rotation.
- Pros: More "pure"
- Cons: Breaks trust edges, ledger references, every rotation

**Recommendation**: Stick with stable DID (original key). Rotation updates the mapping.

### 4. **Capability Granularity: Per-Device or Per-Key?**

**Current Design**: Capabilities attached to VerificationMethod (per-key).

**Alternative**: Capabilities at device level, keys inherit.

**Recommendation**: Per-key is more flexible (can have signing key + encryption key on same device with different caps).

---

## Success Criteria

Phase 11 is complete when:

1. ✅ User can add a second device to their identity
2. ✅ Both devices can sign messages as the same DID
3. ✅ User can revoke a compromised device
4. ✅ Revoked device signatures are rejected by network
5. ✅ User can recover identity after total device loss (social recovery)
6. ✅ All existing tests pass (262+)
7. ✅ New tests cover multi-device workflows
8. ✅ Documentation updated

**Target**: 3-4 weeks (end of February 2025)

---

## Next Steps (Today)

1. **Review this design doc** - Any objections? Changes?
2. **Start Phase 11.1** - Implement data structures in `icn-identity`
3. **Set up test infrastructure** - Multi-device test helpers in `icn-testkit`

Let's build this.
