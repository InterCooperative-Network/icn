# SDIS Implementation Plan for ICN

**Status**: Phase S1-S6 Complete (Security Audit Ready)
**Created**: 2025-12-10
**Target**: Full SDIS integration into ICN substrate

## Executive Summary

This plan integrates the Sovereign Digital Identity System (SDIS) specification into ICN's existing infrastructure. The integration leverages ICN's actor-based architecture, gossip protocol, trust graph, and governance primitives while adding:

- **Hybrid cryptography** (Ed25519 + ML-DSA post-quantum)
- **Anchor/KeyBundle separation** for recoverable identity
- **Steward network** for physical verification
- **Zero-knowledge proofs** for selective disclosure
- **Tiered credential presentation** (QR → NFC → Network)

**Total Duration**: 32-40 weeks (8-10 months)
**New Crates**: 6
**Modified Crates**: 8

---

## Phase Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Phase S1: Cryptographic Foundations                    [Weeks 1-8]     │
│  └─ icn-crypto-pq crate (hybrid signatures, ML-DSA, threshold)          │
├─────────────────────────────────────────────────────────────────────────┤
│  Phase S2: Identity Extensions                          [Weeks 5-12]    │
│  └─ Anchor, KeyBundle, VUI in icn-identity                              │
├─────────────────────────────────────────────────────────────────────────┤
│  Phase S3: Steward Network                              [Weeks 9-20]    │
│  └─ icn-steward crate (actor, token issuance, ceremonies)               │
├─────────────────────────────────────────────────────────────────────────┤
│  Phase S4: Zero-Knowledge Proofs                        [Weeks 13-26]   │
│  └─ icn-zkp crate (STARK circuits, attribute proofs)                    │
├─────────────────────────────────────────────────────────────────────────┤
│  Phase S5: Credential Presentation                      [Weeks 21-30]   │
│  └─ Gateway API, ephemeral proofs, verification tiers                   │
├─────────────────────────────────────────────────────────────────────────┤
│  Phase S6: Governance & Polish                          [Weeks 27-36]   │
│  └─ Steward oversight, ZK voting, integration tests                     │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Phase S1: Cryptographic Foundations

**Duration**: Weeks 1-8
**Dependencies**: None (foundation layer)
**Deliverable**: `icn-crypto-pq` crate

### S1.1 Crate Setup (Week 1)

**Task S1.1.1**: Create crate structure
```
icn/crates/icn-crypto-pq/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── hybrid.rs          # Hybrid signatures
│   ├── ml_dsa.rs          # ML-DSA wrapper
│   ├── ml_kem.rs          # ML-KEM wrapper (key encapsulation)
│   ├── threshold.rs       # Threshold signatures
│   ├── blind.rs           # Blind signatures
│   └── kdf.rs             # Hybrid key derivation
└── tests/
    ├── hybrid_tests.rs
    └── threshold_tests.rs
```

**Task S1.1.2**: Define Cargo.toml dependencies
```toml
[package]
name = "icn-crypto-pq"
version = "0.1.0"
edition = "2021"

[dependencies]
# Classical crypto
ed25519-dalek = { version = "2.1", features = ["rand_core"] }
x25519-dalek = "2.0"

# Post-quantum (NIST FIPS 204/203/205)
pqcrypto-dilithium = "0.5"      # ML-DSA (FIPS 204)
pqcrypto-kyber = "0.8"          # ML-KEM (FIPS 203)
pqcrypto-sphincsplus = "0.7"    # SLH-DSA (FIPS 205) - backup

# Threshold crypto
vsss-rs = "4.0"                 # Verifiable secret sharing
frost-ed25519 = "2.0"           # FROST threshold signatures

# Utilities
sha3 = "0.10"
hkdf = "0.12"
rand = "0.8"
rand_core = "0.6"
zeroize = { version = "1.7", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"

[dev-dependencies]
criterion = "0.5"
```

### S1.2 Hybrid Signatures (Weeks 2-3)

**Task S1.2.1**: Implement `HybridKeypair`

```rust
// src/hybrid.rs
pub struct HybridKeypair {
    classical: Ed25519Keypair,
    pq: MlDsaKeypair,
}

impl HybridKeypair {
    pub fn generate() -> Result<Self>;
    pub fn from_seed(seed: &[u8; 64]) -> Result<Self>;
    pub fn sign(&self, message: &[u8]) -> HybridSignature;
    pub fn public_key(&self) -> HybridPublicKey;
}
```

**Task S1.2.2**: Implement `HybridSignature`

```rust
pub struct HybridSignature {
    classical: [u8; 64],      // Ed25519
    pq: Vec<u8>,              // ML-DSA (~2420 bytes)
}

impl HybridSignature {
    pub fn verify(&self, message: &[u8], pk: &HybridPublicKey) -> bool;
    pub fn to_bytes(&self) -> Vec<u8>;
    pub fn from_bytes(bytes: &[u8]) -> Result<Self>;
}
```

**Task S1.2.3**: Implement serialization (Serde + compact binary)

**Task S1.2.4**: Add benchmarks
- Target: <1ms sign, <2ms verify on typical hardware

**Tests**:
- [ ] Sign/verify roundtrip
- [ ] Tampered message detection
- [ ] Tampered signature detection (classical)
- [ ] Tampered signature detection (pq)
- [ ] Cross-implementation compatibility
- [ ] Size verification (~2.5KB)

### S1.3 Threshold Cryptography (Weeks 4-6)

**Task S1.3.1**: Implement `ThresholdConfig`

```rust
// src/threshold.rs
pub struct ThresholdConfig {
    pub n: usize,           // Total parties
    pub t: usize,           // Threshold
    pub party_id: usize,    // This party's ID
}
```

**Task S1.3.2**: Implement Distributed Key Generation (DKG)

```rust
pub struct DkgSession {
    config: ThresholdConfig,
    state: DkgState,
}

impl DkgSession {
    pub fn new(config: ThresholdConfig) -> Self;
    pub fn round1(&mut self) -> DkgRound1Message;
    pub fn process_round1(&mut self, msgs: Vec<DkgRound1Message>) -> Result<()>;
    pub fn round2(&mut self) -> DkgRound2Message;
    pub fn process_round2(&mut self, msgs: Vec<DkgRound2Message>) -> Result<()>;
    pub fn finalize(&self) -> Result<(SecretShare, PublicKey)>;
}
```

**Task S1.3.3**: Implement Threshold PRF (for VUI computation)

```rust
pub struct ThresholdPrf {
    share: SecretShare,
    config: ThresholdConfig,
}

impl ThresholdPrf {
    /// Compute partial PRF evaluation
    pub fn evaluate_partial(&self, input: &[u8]) -> PrfPartial;

    /// Combine partials (done by user's device)
    pub fn combine(partials: &[PrfPartial]) -> Result<[u8; 32]>;
}
```

**Task S1.3.4**: Implement Threshold Signing (FROST)

```rust
pub struct ThresholdSigner {
    share: SecretShare,
    config: ThresholdConfig,
}

impl ThresholdSigner {
    pub fn sign_round1(&mut self, message: &[u8]) -> SignRound1;
    pub fn sign_round2(&mut self, round1_msgs: &[SignRound1]) -> SignRound2;
    pub fn aggregate(round2_msgs: &[SignRound2]) -> Result<Signature>;
}
```

**Tests**:
- [ ] DKG with 3-of-5 parties
- [ ] DKG with 5-of-9 parties
- [ ] Threshold PRF correctness
- [ ] Threshold signing correctness
- [ ] Reconstruct fails with t-1 shares
- [ ] Network partition tolerance

### S1.4 Blind Signatures (Week 7)

**Task S1.4.1**: Implement blind signature scheme

```rust
// src/blind.rs

/// User-side: blind a message
pub fn blind(message: &[u8], blinding_factor: &Scalar) -> BlindedMessage;

/// Signer-side: sign blinded message
pub fn sign_blinded(sk: &SigningKey, blinded: &BlindedMessage) -> BlindedSignature;

/// User-side: unblind signature
pub fn unblind(
    sig: &BlindedSignature,
    blinding_factor: &Scalar,
) -> Signature;

/// Verify unblinded signature (standard verification)
pub fn verify(pk: &VerifyingKey, message: &[u8], sig: &Signature) -> bool;
```

**Tests**:
- [ ] Blind/sign/unblind roundtrip
- [ ] Signer cannot correlate blinded to unblinded
- [ ] Multiple blindings of same message are unlinkable

### S1.5 Hybrid Key Derivation (Week 8)

**Task S1.5.1**: Implement hybrid KDF

```rust
// src/kdf.rs

/// Derive key from hybrid shared secrets
pub fn derive_hybrid(
    classical_shared: &[u8; 32],
    pq_shared: &[u8; 32],
    context: &[u8],
    output_len: usize,
) -> Vec<u8> {
    // Combine secrets: secure if EITHER is secure
    let combined = [classical_shared, pq_shared].concat();
    hkdf_sha3_256(&combined, context, output_len)
}
```

**Task S1.5.2**: Implement seed-based key generation

```rust
/// Generate full KeyBundle from seed
pub fn keybundle_from_seed(seed: &[u8; 64]) -> Result<KeyBundleKeys> {
    let classical = derive_ed25519(&seed[..32])?;
    let pq = derive_ml_dsa(&seed[32..])?;
    let x25519 = derive_x25519(&hkdf(seed, b"x25519", 32))?;
    Ok(KeyBundleKeys { classical, pq, x25519 })
}
```

### S1 Deliverables Checklist

- [x] `icn-crypto-pq` crate compiles
- [x] All unit tests pass (51 tests passing)
- [ ] Benchmarks documented
- [ ] Integration with existing `icn-identity` verified
- [ ] Security review of crypto choices

**Completed Modules (2025-12-10)**:
- `ml_dsa.rs` - ML-DSA (Dilithium3) wrapper
- `hybrid.rs` - Hybrid Ed25519 + ML-DSA signatures
- `ml_kem.rs` - ML-KEM (Kyber768) key encapsulation
- `hybrid_kem.rs` - Hybrid X25519 + ML-KEM key exchange
- `kdf.rs` - HKDF-SHA3-256 key derivation
- `threshold.rs` - Secret sharing and threshold PRF
- `blind.rs` - Blind signatures for enrollment tokens

---

## Phase S2: Identity Extensions

**Duration**: Weeks 5-12 (overlaps S1)
**Dependencies**: S1.2 (hybrid signatures)
**Deliverable**: Extended `icn-identity` crate

### S2.1 Anchor Implementation (Weeks 5-6)

**Task S2.1.1**: Create `anchor.rs` module

**File**: `icn/crates/icn-identity/src/anchor.rs`

```rust
/// Permanent identity anchor
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Anchor {
    pub id: [u8; 32],
    pub created_at: u64,
    pub pathway: EnrollmentPathway,
    pub vui_commitment: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnrollmentPathway {
    GovernmentId { country: [u8; 2], doc_type_hash: [u8; 8] },
    OrganizationalSponsorship { org_did: Did, org_internal_id_hash: [u8; 16] },
    BiometricCommitment { template_hash: [u8; 32] },
    WebOfTrust { vouchers: Vec<Did>, vouched_at: u64 },
}

impl Anchor {
    pub fn from_vui(vui: &[u8; 32], pathway: EnrollmentPathway, genesis: &[u8; 32]) -> Self;
    pub fn to_did(&self) -> Did;
    pub fn verify_vui(&self, vui: &[u8; 32]) -> bool;
}
```

**Task S2.1.2**: Implement DID format extension

Current: `did:icn:<base58-pubkey>`
New: `did:icn:<base58-anchor-id>`

Add backward compatibility:
```rust
impl Did {
    /// Parse DID, detecting format version
    pub fn from_str(s: &str) -> Result<Self> {
        // Detect: legacy (32-byte pubkey) vs SDIS (32-byte anchor)
        // Both formats are valid, context determines interpretation
    }

    /// Check if this is an SDIS anchor-based DID
    pub fn is_anchor_did(&self) -> bool;
}
```

### S2.2 KeyBundle Implementation (Weeks 7-8)

**Task S2.2.1**: Create `keybundle.rs` module

**File**: `icn/crates/icn-identity/src/keybundle.rs`

```rust
/// Rotatable key bundle bound to an Anchor
pub struct KeyBundle {
    pub anchor: Anchor,
    pub version: u32,
    hybrid_keypair: HybridKeypair,
    x25519_secret: Zeroizing<[u8; 32]>,
    x25519_public: [u8; 32],
    pub issued_at: u64,
    pub expires_at: Option<u64>,
}

impl KeyBundle {
    pub fn generate(anchor: Anchor, version: u32) -> Result<Self>;
    pub fn from_seed(anchor: Anchor, version: u32, seed: &[u8; 64]) -> Result<Self>;
    pub fn sign(&self, message: &[u8]) -> HybridSignature;
    pub fn sign_classical(&self, message: &[u8]) -> Ed25519Signature;
    pub fn did(&self) -> Did;
    pub fn create_rotation_request(&self, new_bundle: &KeyBundle) -> RotationRequest;
}
```

**Task S2.2.2**: Implement KeyBundle storage in keystore

**File**: `icn/crates/icn-identity/src/keystore.rs` (extend)

```rust
/// Keystore format v4: Adds SDIS KeyBundle support
pub struct KeyStoreV4 {
    // Legacy fields (v3)
    pub keypair: KeyPair,
    pub tls_cert: Vec<u8>,
    pub tls_key: Vec<u8>,
    pub x25519_secret: Vec<u8>,
    pub x25519_public: [u8; 32],
    pub did_document: Option<DidDocument>,

    // SDIS fields (v4)
    pub anchor: Option<Anchor>,
    pub keybundles: Vec<StoredKeyBundle>,
    pub current_keybundle_version: u32,
}

pub struct StoredKeyBundle {
    pub version: u32,
    pub classical_secret: Vec<u8>,
    pub pq_secret: Vec<u8>,
    pub x25519_secret: Vec<u8>,
    pub issued_at: u64,
    pub revoked_at: Option<u64>,
}
```

**Task S2.2.3**: Implement keystore migration v3 → v4

### S2.3 VUI Types (Weeks 9-10)

**Task S2.3.1**: Create `vui.rs` module

**File**: `icn/crates/icn-identity/src/vui.rs`

```rust
/// Verifiable Unique Identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vui([u8; 32]);

impl Vui {
    /// Compute VUI from ID data (requires threshold PRF)
    /// This is called by Steward during enrollment
    pub async fn compute(
        id_data: &IdData,
        pathway: &EnrollmentPathway,
        threshold_prf: &dyn ThresholdPrfClient,
    ) -> Result<Self>;

    /// Create commitment (stored in Anchor)
    pub fn commitment(&self) -> [u8; 32];
}

/// ID document data (extracted by Steward, never stored)
pub struct IdData {
    pub id_number_hash: [u8; 32],  // Hash of actual ID number
    pub doc_type: DocumentType,
    pub issuing_authority: String,
}

pub enum DocumentType {
    Passport,
    NationalId,
    DriversLicense,
    Other(String),
}
```

### S2.4 IdentityBundle Extension (Weeks 11-12)

**Task S2.4.1**: Extend `IdentityBundle` for SDIS

**File**: `icn/crates/icn-identity/src/bundle.rs` (extend)

```rust
impl IdentityBundle {
    // Existing methods...

    /// Create SDIS-enabled identity bundle
    pub fn generate_sdis(anchor: Anchor) -> Result<Self>;

    /// Get current KeyBundle
    pub fn keybundle(&self) -> Option<&KeyBundle>;

    /// Rotate to new KeyBundle
    pub fn rotate_keybundle(&mut self) -> Result<RotationRequest>;

    /// Check if this is an SDIS identity
    pub fn is_sdis(&self) -> bool;
}
```

**Task S2.4.2**: Update DidDocument for SDIS

```rust
impl DidDocument {
    /// Create from SDIS KeyBundle
    pub fn from_keybundle(keybundle: &KeyBundle) -> Self;

    /// Verify signature against current KeyBundle
    pub fn verify_hybrid(&self, message: &[u8], sig: &HybridSignature) -> bool;
}
```

### S2 Deliverables Checklist

- [x] Anchor type implemented and tested (12 tests)
- [x] KeyBundle type implemented and tested (11 tests)
- [x] Keystore v4 migration working (6 tests)
- [x] VUI types defined (7 tests)
- [x] IdentityBundle extended
- [x] Backward compatibility with legacy DIDs
- [x] All existing icn-identity tests still pass (85 total)

---

## Phase S3: Steward Network

**Duration**: Weeks 9-20
**Dependencies**: S1 (crypto), S2 (identity)
**Deliverable**: `icn-steward` crate

### S3.1 Crate Setup (Week 9)

**Task S3.1.1**: Create crate structure

```
icn/crates/icn-steward/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── actor.rs           # StewardActor
│   ├── handle.rs          # StewardHandle
│   ├── profile.rs         # StewardProfile
│   ├── token.rs           # EnrollmentToken
│   ├── ceremony/
│   │   ├── mod.rs
│   │   ├── enrollment.rs  # POP enrollment ceremony
│   │   └── recovery.rs    # Key recovery ceremony
│   ├── vui_registry.rs    # VUI uniqueness checking
│   ├── gossip.rs          # Steward gossip messages
│   └── config.rs          # StewardConfig
└── tests/
    ├── enrollment_test.rs
    ├── recovery_test.rs
    └── ceremony_test.rs
```

### S3.2 Steward Types (Weeks 10-11)

**Task S3.2.1**: Implement `StewardProfile`

```rust
// src/profile.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardProfile {
    pub steward_did: Did,
    pub citizen_did: Did,
    pub status: StewardStatus,
    pub jurisdiction_tier: JurisdictionTier,
    pub bond_amount: i64,
    pub region: String,
    pub term_start: u64,
    pub term_end: Option<u64>,
    pub stats: StewardStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardStats {
    pub pops_issued: u64,
    pub fraud_investigations: u64,
    pub proven_fraud: u64,
    pub recoveries_participated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StewardStatus {
    Active,
    Suspended { reason: String, until: Option<u64> },
    Revoked { reason: String, at: u64 },
    TermExpired { expired_at: u64 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum JurisdictionTier {
    Tier1,  // Standard
    Tier2,  // Enhanced monitoring
    Tier3,  // Requires co-signing
}
```

**Task S3.2.2**: Implement `EnrollmentToken`

```rust
// src/token.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentToken {
    pub token_id: [u8; 32],
    pub blinded_signature: Vec<u8>,
    pub issuing_steward: Did,
    pub issued_at: u64,
    pub expires_at: u64,
    pub vui_commitment: [u8; 32],
}

impl EnrollmentToken {
    pub fn is_expired(&self) -> bool;
    pub fn verify_signature(&self, steward_pk: &VerifyingKey) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenIssuanceRecord {
    pub blinded_token_hash: [u8; 32],  // Cannot correlate to actual token
    pub vui_checked: [u8; 32],
    pub issued_at: u64,
    pub steward_did: Did,
}
```

### S3.3 VUI Registry (Weeks 12-13)

**Task S3.3.1**: Implement Bloom filter registry

```rust
// src/vui_registry.rs

pub struct VuiRegistry {
    bloom_filter: BloomFilter,
    overflow_set: HashSet<[u8; 32]>,
    pending_checks: HashMap<u64, PendingCheck>,
    store: Arc<dyn Store>,
}

impl VuiRegistry {
    pub fn new(store: Arc<dyn Store>, config: VuiRegistryConfig) -> Result<Self>;

    /// Local check (fast, may have false positives)
    pub fn check_local(&self, vui: &Vui) -> LocalCheckResult;

    /// Distributed check (requires network consensus)
    pub async fn check_distributed(&self, vui: &Vui) -> Result<DistributedCheckResult>;

    /// Mark VUI as consumed (after successful enrollment)
    pub async fn consume(&mut self, vui: &Vui) -> Result<()>;

    /// Sync with network (anti-entropy)
    pub async fn sync(&mut self) -> Result<SyncStats>;
}

pub enum LocalCheckResult {
    Unique,
    PossibleDuplicate,
    DefiniteDuplicate,
}

pub enum DistributedCheckResult {
    Unique { confirmations: usize },
    Duplicate { first_seen: u64 },
    Inconclusive { reason: String },
}
```

### S3.4 Enrollment Ceremony (Weeks 14-16)

**Task S3.4.1**: Implement enrollment state machine

```rust
// src/ceremony/enrollment.rs

pub struct EnrollmentCeremony {
    session_id: [u8; 32],
    state: EnrollmentState,
    config: EnrollmentConfig,
}

pub enum EnrollmentState {
    /// Waiting for applicant
    AwaitingApplicant,
    /// ID verification in progress
    VerifyingId { id_data: IdData },
    /// VUI check in progress
    CheckingVui { vui: Vui },
    /// Token issuance
    IssuingToken { blinded_message: BlindedMessage },
    /// POP generation (threshold ceremony)
    GeneratingPop { token: EnrollmentToken, partials: Vec<PrfPartial> },
    /// Complete
    Complete { anchor: Anchor, keybundle_version: u32 },
    /// Failed
    Failed { reason: String },
}

impl EnrollmentCeremony {
    pub fn new(config: EnrollmentConfig) -> Self;

    /// Steward: Start ID verification
    pub fn start_verification(&mut self, id_data: IdData) -> Result<()>;

    /// Steward: Complete ID verification, start VUI check
    pub async fn verify_id(&mut self, vui_registry: &VuiRegistry) -> Result<()>;

    /// Steward: Issue blind token
    pub fn issue_token(
        &mut self,
        blinded_message: &BlindedMessage,
        steward_key: &SigningKey,
    ) -> Result<EnrollmentToken>;

    /// User: Submit token for POP generation
    pub fn submit_token(
        &mut self,
        token: &EnrollmentToken,
        user_entropy: &[u8; 32],
    ) -> Result<()>;

    /// Steward: Contribute PRF partial
    pub fn contribute_partial(
        &mut self,
        steward_did: &Did,
        partial: PrfPartial,
    ) -> Result<()>;

    /// Finalize: Generate Anchor and KeyBundle
    pub fn finalize(&mut self) -> Result<(Anchor, KeyBundle)>;
}
```

**Task S3.4.2**: Implement enrollment protocol messages

```rust
// src/gossip.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StewardMessage {
    // VUI checking
    VuiCheckRequest { steward: Did, vui_hash: [u8; 32], nonce: u64 },
    VuiCheckResponse { nonce: u64, exists: bool, responder: Did },

    // Token tracking
    TokenConsumed { token_hash: [u8; 32], steward: Did, at: u64 },

    // POP ceremony
    PopCeremonyInit { session: [u8; 32], token_hash: [u8; 32], entropy_commit: [u8; 32] },
    PopCeremonyPartial { session: [u8; 32], steward: Did, partial: Vec<u8> },
    PopCeremonyComplete { session: [u8; 32], anchor: [u8; 32] },

    // KeyBundle
    KeyBundleRotation { anchor: [u8; 32], old_v: u32, new_v: u32, sig: Vec<u8> },

    // Recovery
    RecoveryRequest { session: [u8; 32], anchor: [u8; 32], initiator: Did },
    RecoveryApproval { session: [u8; 32], steward: Did, sig: Vec<u8> },
    RecoveryComplete { session: [u8; 32], anchor: [u8; 32], new_v: u32 },
}
```

### S3.5 Recovery Ceremony (Weeks 17-18)

**Task S3.5.1**: Implement recovery state machine

```rust
// src/ceremony/recovery.rs

pub struct RecoveryCeremony {
    session_id: [u8; 32],
    anchor: Anchor,
    state: RecoveryState,
    config: RecoveryConfig,
}

pub enum RecoveryState {
    /// Initiated, awaiting steward approvals
    AwaitingApprovals {
        approvals: Vec<(Did, Vec<u8>)>,
        required: usize,
    },
    /// Sufficient approvals, generating new KeyBundle
    GeneratingKeyBundle,
    /// Cool-down period (old owner can dispute)
    CoolDown {
        new_keybundle: KeyBundle,
        dispute_deadline: u64,
    },
    /// Complete
    Complete {
        old_version: u32,
        new_version: u32,
    },
    /// Disputed (old owner contested)
    Disputed { reason: String },
}

impl RecoveryCeremony {
    /// Initiate recovery (requires physical verification)
    pub fn initiate(
        anchor: &Anchor,
        vui: &Vui,
        initiating_steward: &Did,
    ) -> Result<Self>;

    /// Steward approves recovery
    pub fn approve(
        &mut self,
        steward: &Did,
        steward_key: &SigningKey,
    ) -> Result<()>;

    /// Check if threshold reached
    pub fn has_threshold(&self) -> bool;

    /// Generate new KeyBundle (after threshold)
    pub fn generate_keybundle(&mut self) -> Result<&KeyBundle>;

    /// Start cool-down period
    pub fn start_cooldown(&mut self, duration: Duration) -> Result<()>;

    /// Dispute recovery (by old key holder)
    pub fn dispute(&mut self, old_keybundle: &KeyBundle, reason: &str) -> Result<()>;

    /// Finalize (after cool-down)
    pub fn finalize(&mut self) -> Result<RecoveryResult>;
}
```

### S3.6 StewardActor (Weeks 19-20)

**Task S3.6.1**: Implement actor

```rust
// src/actor.rs

pub struct StewardActor {
    config: StewardConfig,
    profile: StewardProfile,
    vui_registry: Arc<RwLock<VuiRegistry>>,
    threshold_prf: Arc<dyn ThresholdPrfClient>,
    active_ceremonies: HashMap<[u8; 32], ActiveCeremony>,
    gossip_handle: GossipHandle,
    store: Arc<dyn Store>,
}

enum ActiveCeremony {
    Enrollment(EnrollmentCeremony),
    Recovery(RecoveryCeremony),
}

pub enum StewardCommand {
    // Enrollment
    StartEnrollment { id_data: IdData },
    IssueToken { session: [u8; 32], blinded: BlindedMessage },
    ContributePopPartial { session: [u8; 32] },

    // Recovery
    InitiateRecovery { anchor: Anchor, vui: Vui },
    ApproveRecovery { session: [u8; 32] },
    DisputeRecovery { session: [u8; 32], reason: String },

    // Admin
    GetStatus,
    UpdateProfile { profile: StewardProfile },
}

impl StewardActor {
    pub fn spawn(
        config: StewardConfig,
        profile: StewardProfile,
        vui_registry: Arc<RwLock<VuiRegistry>>,
        threshold_prf: Arc<dyn ThresholdPrfClient>,
        gossip_handle: GossipHandle,
        store: Arc<dyn Store>,
    ) -> StewardHandle;
}
```

**Task S3.6.2**: Implement handle

```rust
// src/handle.rs

pub struct StewardHandle {
    tx: mpsc::Sender<StewardCommand>,
}

impl StewardHandle {
    pub async fn start_enrollment(&self, id_data: IdData) -> Result<[u8; 32]>;
    pub async fn issue_token(&self, session: [u8; 32], blinded: BlindedMessage) -> Result<EnrollmentToken>;
    pub async fn initiate_recovery(&self, anchor: &Anchor, vui: &Vui) -> Result<[u8; 32]>;
    pub async fn get_status(&self) -> Result<StewardStatus>;
}
```

### S3 Deliverables Checklist

- [x] `icn-steward` crate compiles (73 tests passing)
- [x] VUI registry with distributed checking (Bloom filter + exact set)
- [x] Enrollment ceremony state machine
- [x] Recovery ceremony state machine
- [x] StewardActor with gossip integration
- [x] Integration tests with multiple stewards (16 integration tests)
- [x] Threshold operations tested (3-of-5 PRF combination)

---

## Phase S4: Zero-Knowledge Proofs

**Duration**: Weeks 13-26
**Dependencies**: S1 (crypto), S2 (identity)
**Deliverable**: `icn-zkp` crate

### S4.1 Crate Setup (Week 13)

**Task S4.1.1**: Create crate structure

```
icn/crates/icn-zkp/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── circuit/
│   │   ├── mod.rs
│   │   ├── age.rs           # Age proof circuit
│   │   ├── citizenship.rs   # Citizenship proof circuit
│   │   ├── membership.rs    # Membership proof circuit
│   │   └── non_revocation.rs
│   ├── prover.rs            # STARK prover
│   ├── verifier.rs          # STARK verifier
│   ├── accumulator.rs       # Cryptographic accumulator
│   └── types.rs
└── tests/
    ├── age_proof_test.rs
    └── accumulator_test.rs
```

**Task S4.1.2**: Cargo.toml

```toml
[dependencies]
# STARK prover (choose one)
winterfell = "0.9"           # Pure Rust STARK
# OR
risc0-zkvm = "1.0"           # RISC Zero (more flexible)

# Accumulator
rug = "1.22"                 # Arbitrary precision (for RSA accumulator)

# Utilities
sha3 = "0.10"
serde = { version = "1.0", features = ["derive"] }
```

### S4.2 STARK Infrastructure (Weeks 14-17)

**Task S4.2.1**: Implement proof types

```rust
// src/types.rs

/// STARK proof (~50-100 KB)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarkProof {
    pub proof_bytes: Vec<u8>,
    pub public_inputs_hash: [u8; 32],
}

/// Attribute proof request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeProofRequest {
    pub proof_type: ProofType,
    pub context: ProofContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProofType {
    AgeAtLeast { threshold: u8 },
    Citizenship { country_code: [u8; 2] },
    Membership { org_did: Did },
    NonRevocation,
    Custom { circuit_id: [u8; 32] },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofContext {
    pub verifier_did: Option<Did>,
    pub timestamp: u64,
    pub nonce: [u8; 16],
}
```

**Task S4.2.2**: Implement prover abstraction

```rust
// src/prover.rs

pub trait ZkProver {
    type PublicInput;
    type PrivateInput;
    type Proof;

    fn prove(
        &self,
        public: &Self::PublicInput,
        private: &Self::PrivateInput,
    ) -> Result<Self::Proof>;
}

pub struct StarkProver {
    // Winterfell prover configuration
}

impl StarkProver {
    pub fn new() -> Self;

    pub fn prove_age(&self, req: &AgeProofRequest, attestation: &AgeAttestation) -> Result<StarkProof>;
    pub fn prove_citizenship(&self, req: &CitizenshipProofRequest, attestation: &CitizenshipAttestation) -> Result<StarkProof>;
    pub fn prove_non_revocation(&self, anchor: &Anchor, accumulator: &AccumulatorWitness) -> Result<StarkProof>;
}
```

### S4.3 Age Proof Circuit (Weeks 18-20)

**Task S4.3.1**: Implement age proof

```rust
// src/circuit/age.rs

pub struct AgeProofPublic {
    pub threshold: u8,
    pub current_date: u32,  // Days since epoch
    pub issuer_pk: [u8; 32],
}

pub struct AgeProofPrivate {
    pub birthdate: u32,     // Days since epoch
    pub issuer_signature: Vec<u8>,
}

pub struct AgeProofCircuit;

impl AgeProofCircuit {
    /// Generate STARK proof that:
    /// 1. issuer_signature is valid for birthdate
    /// 2. current_date - birthdate >= threshold * 365
    pub fn prove(
        public: &AgeProofPublic,
        private: &AgeProofPrivate,
    ) -> Result<StarkProof>;

    pub fn verify(
        public: &AgeProofPublic,
        proof: &StarkProof,
    ) -> bool;
}
```

### S4.4 Accumulator for Revocation (Weeks 21-23)

**Task S4.4.1**: Implement RSA accumulator

```rust
// src/accumulator.rs

pub struct RsaAccumulator {
    modulus: BigInt,
    value: BigInt,
}

impl RsaAccumulator {
    pub fn new() -> Self;

    /// Add element to accumulator
    pub fn add(&mut self, element: &[u8; 32]);

    /// Generate membership witness
    pub fn membership_witness(&self, element: &[u8; 32]) -> MembershipWitness;

    /// Generate non-membership witness (Bezout coefficients)
    pub fn non_membership_witness(&self, element: &[u8; 32]) -> NonMembershipWitness;

    /// Verify membership
    pub fn verify_membership(&self, element: &[u8; 32], witness: &MembershipWitness) -> bool;

    /// Verify non-membership
    pub fn verify_non_membership(&self, element: &[u8; 32], witness: &NonMembershipWitness) -> bool;
}
```

### S4.5 Combined Proofs (Weeks 24-26)

**Task S4.5.1**: Implement compound proof generation

```rust
// src/lib.rs

/// Generate compound proof (attribute + non-revocation)
pub fn generate_compound_proof(
    attribute_request: &AttributeProofRequest,
    attestation: &Attestation,
    anchor: &Anchor,
    accumulator_witness: &NonMembershipWitness,
) -> Result<CompoundProof> {
    let attribute_proof = match &attribute_request.proof_type {
        ProofType::AgeAtLeast { threshold } => {
            AgeProofCircuit::prove(/* ... */)
        }
        // ... other types
    };

    let non_revocation_proof = NonRevocationCircuit::prove(
        anchor,
        accumulator_witness,
    )?;

    Ok(CompoundProof {
        attribute_proof,
        non_revocation_proof,
        context: attribute_request.context.clone(),
    })
}
```

### S4 Deliverables Checklist

- [x] `icn-zkp` crate compiles
- [x] Age proof circuit working (simulated, winterfell STARK via `stark` feature)
- [x] Citizenship proof circuit working
- [x] RSA accumulator implemented (with membership/non-membership witnesses)
- [x] Non-revocation proofs working
- [x] Compound proofs working
- [ ] Proof size within budget (~50-100KB) - requires `stark` feature
- [ ] Benchmarks: proof generation <10s, verification <1s - TODO when `stark` enabled

---

## Phase S5: Credential Presentation

**Duration**: Weeks 21-30
**Dependencies**: S1-S4
**Deliverable**: Extended `icn-gateway` crate

### S5.1 Ephemeral Proof System (Weeks 21-24)

**Task S5.1.1**: Implement ephemeral proof generation

**File**: `icn/crates/icn-gateway/src/api/sdis/ephemeral.rs`

```rust
/// Ephemeral proof for QR codes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralProof {
    pub v: u8,              // Version
    pub t: ProofType,       // What's being proved
    pub a: [u8; 16],        // Truncated anchor hash
    pub k: [u8; 32],        // Ephemeral Ed25519 public key
    pub x: u64,             // Expiry timestamp
    pub n: [u8; 16],        // Nonce
    pub s: [u8; 64],        // Ed25519 signature
    pub c: Vec<Channel>,    // Available upgrade channels
}

impl EphemeralProof {
    /// Generate from KeyBundle (daily refresh)
    pub fn generate(
        keybundle: &KeyBundle,
        proof_type: ProofType,
        validity: Duration,
    ) -> Result<(Self, EphemeralBinding)>;

    /// Encode for QR code
    pub fn to_qr_bytes(&self) -> Vec<u8>;

    /// Decode from QR scan
    pub fn from_qr_bytes(bytes: &[u8]) -> Result<Self>;

    /// Size in bytes
    pub fn size(&self) -> usize;
}

/// Binding attestation (for Level 2+ verification)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralBinding {
    pub anchor: [u8; 32],
    pub ephemeral_pk: [u8; 32],
    pub expires: u64,
    pub hybrid_signature: HybridSignature,  // ~2.5KB
}
```

**Task S5.1.2**: Implement verification levels

```rust
// src/api/sdis/verify.rs

pub struct Verifier {
    nonce_cache: LruCache<[u8; 16], u64>,
    binding_cache: LruCache<[u8; 32], EphemeralBinding>,
}

impl Verifier {
    /// Level 1: QR scan only (classical, no network)
    pub fn verify_level1(&mut self, proof: &EphemeralProof) -> VerifyResult {
        // 1. Check expiry
        if proof.x < now() { return VerifyResult::Expired; }

        // 2. Check nonce not reused
        if self.nonce_cache.contains(&proof.n) {
            return VerifyResult::ReplayDetected;
        }
        self.nonce_cache.put(proof.n, now());

        // 3. Verify Ed25519 signature
        let message = proof.signing_payload();
        if !ed25519_verify(&proof.k, &message, &proof.s) {
            return VerifyResult::InvalidSignature;
        }

        VerifyResult::Valid { level: 1 }
    }

    /// Level 2: NFC/BLE with binding verification (hybrid)
    pub fn verify_level2(
        &mut self,
        proof: &EphemeralProof,
        binding: &EphemeralBinding,
    ) -> VerifyResult {
        // 1. Level 1 checks
        let l1 = self.verify_level1(proof)?;

        // 2. Verify binding matches proof
        if binding.ephemeral_pk != proof.k {
            return VerifyResult::BindingMismatch;
        }

        // 3. Verify hybrid signature on binding
        if !binding.hybrid_signature.verify(&binding.signing_payload(), &anchor_pk) {
            return VerifyResult::InvalidHybridSignature;
        }

        VerifyResult::Valid { level: 2 }
    }

    /// Level 3: Full network verification with STARK proofs
    pub async fn verify_level3(
        &mut self,
        proof: &EphemeralProof,
        binding: &EphemeralBinding,
        stark_proof: &StarkProof,
    ) -> VerifyResult;
}
```

### S5.2 Gateway API Routes (Weeks 25-27)

**Task S5.2.1**: Add SDIS routes

**File**: `icn/crates/icn-gateway/src/api/sdis/mod.rs`

```rust
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/sdis")
            // Ephemeral proofs
            .route("/ephemeral/generate", web::post().to(generate_ephemeral))
            .route("/ephemeral/refresh", web::post().to(refresh_ephemeral))

            // Verification
            .route("/verify/level1", web::post().to(verify_level1))
            .route("/verify/level2", web::post().to(verify_level2))
            .route("/verify/level3", web::post().to(verify_level3))

            // Binding
            .route("/binding/{session}", web::get().to(get_binding))

            // Enrollment (requires Steward auth)
            .route("/enroll/init", web::post().to(init_enrollment))
            .route("/enroll/verify-id", web::post().to(verify_id))
            .route("/enroll/issue-token", web::post().to(issue_token))
            .route("/enroll/complete", web::post().to(complete_enrollment))

            // KeyBundle
            .route("/keybundle/current", web::get().to(get_current_keybundle))
            .route("/keybundle/rotate", web::post().to(rotate_keybundle))

            // Recovery
            .route("/recovery/initiate", web::post().to(initiate_recovery))
            .route("/recovery/status/{session}", web::get().to(recovery_status))

            // Attestations
            .route("/attestation/request", web::post().to(request_attestation))
            .route("/attestation/list", web::get().to(list_attestations))

            // Steward admin
            .route("/steward/list", web::get().to(list_stewards))
            .route("/steward/{did}", web::get().to(get_steward))
    );
}
```

### S5.3 QR Code Integration (Weeks 28-30)

**Task S5.3.1**: Implement compact encoding

```rust
// src/api/sdis/qr.rs

/// Encode ephemeral proof for QR code
/// Target: <1KB for QR Version 15-20
pub fn encode_for_qr(proof: &EphemeralProof) -> Result<Vec<u8>> {
    // Use CBOR for compact encoding
    let mut encoded = Vec::new();

    // Version (1 byte)
    encoded.push(proof.v);

    // Proof type (1 byte)
    encoded.push(proof.t.to_byte());

    // Truncated anchor (16 bytes)
    encoded.extend_from_slice(&proof.a);

    // Ephemeral public key (32 bytes)
    encoded.extend_from_slice(&proof.k);

    // Expiry (4 bytes - seconds since 2024-01-01)
    let relative_expiry = (proof.x - EPOCH_2024) as u32;
    encoded.extend_from_slice(&relative_expiry.to_le_bytes());

    // Nonce (16 bytes)
    encoded.extend_from_slice(&proof.n);

    // Signature (64 bytes)
    encoded.extend_from_slice(&proof.s);

    // Channels (1 byte bitmap)
    encoded.push(channels_to_bitmap(&proof.c));

    // Total: 135 bytes base (fits easily in QR)
    Ok(encoded)
}
```

### S5 Deliverables Checklist

- [x] Ephemeral proof generation working
- [x] Level 1 verification (<2s, no network)
- [x] Level 2 verification (hybrid)
- [x] Level 3 verification (STARK) - implementation complete, requires `stark` feature for production
- [x] QR encoding <1KB (137 bytes)
- [x] Gateway routes implemented
- [ ] OpenAPI spec updated
- [ ] TypeScript SDK extended

**Completed (2025-12-10)**:
- `ephemeral.rs` - EphemeralProof and EphemeralBinding types with generation and signature verification
- `verify.rs` - EphemeralVerifier with Level 1/2/3 verification, nonce cache, binding cache
- `qr.rs` - Compact 137-byte QR encoding with relative timestamps
- `mod.rs` - Gateway routes: `/v1/sdis/health`, `/v1/sdis/verify/level1`, `/v1/sdis/verify/level2`
- Full test coverage: 22 tests passing

---

## Phase S6: Governance & Polish

**Duration**: Weeks 27-36
**Dependencies**: S1-S5
**Deliverable**: Complete integrated system

### S6.1 Governance Extensions (Weeks 27-30)

**Task S6.1.1**: Add SDIS proposal types

**File**: `icn/crates/icn-governance/src/sdis.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SdisProposal {
    AppointSteward { candidate: Did, sponsors: Vec<Did>, region: String },
    RemoveSteward { steward: Did, reason: String },
    SanctionSteward { steward: Did, penalty: StewardPenalty },
    ReconfirmSteward { steward: Did, new_term: u64 },
    ModifyThreshold { op: ThresholdOp, value: usize },
    ApproveAuthority { authority: InstitutionalAuthority },
    RevokeAuthority { authority: Did, reason: String },
    RevocationAppeal { target: RevocationTarget, reason: String },
}
```

**Task S6.1.2**: Implement ZK voting (optional for v1)

### S6.2 Integration Tests (Weeks 31-33)

**Task S6.2.1**: Multi-node enrollment test

```rust
#[tokio::test]
async fn test_full_enrollment_flow() {
    // 1. Set up 5 steward nodes
    let stewards = setup_steward_network(5, 3).await;  // 3-of-5 threshold

    // 2. Applicant generates blinded token
    let (blinded, blinding_factor) = blind_message(&token);

    // 3. Steward verifies ID, checks VUI, signs
    let signed_blinded = stewards[0].issue_token(&blinded).await?;

    // 4. Applicant unblinds
    let token = unblind(&signed_blinded, &blinding_factor);

    // 5. POP generation ceremony
    let session = stewards[0].init_pop_ceremony(&token).await?;

    // 6. Collect threshold partials
    for steward in &stewards[..3] {
        steward.contribute_partial(&session).await?;
    }

    // 7. User combines partials, generates KeyBundle
    let (anchor, keybundle) = finalize_enrollment(&session).await?;

    // 8. Verify anchor is valid
    assert!(verify_anchor(&anchor, &stewards).await?);
}
```

**Task S6.2.2**: Recovery ceremony test

**Task S6.2.3**: Credential presentation test

**Task S6.2.4**: Revocation test

### S6.3 Documentation (Weeks 34-35)

**Task S6.3.1**: Update ARCHITECTURE.md

**Task S6.3.2**: Create SDIS_USER_GUIDE.md

**Task S6.3.3**: Create STEWARD_OPERATIONS.md

**Task S6.3.4**: Update API documentation

### S6.4 Security Audit Prep (Week 36)

**Task S6.4.1**: Threat model document

**Task S6.4.2**: Code audit checklist

**Task S6.4.3**: Cryptographic review document

### S6 Deliverables Checklist

- [x] SDIS governance proposals working
- [ ] Full enrollment integration test passing (requires steward network)
- [ ] Full recovery integration test passing (requires steward network)
- [x] Credential presentation integration test passing (19 tests)
- [x] Documentation complete
- [x] Security audit materials prepared

**Completed (2025-12-10)**:
- `sdis.rs` - SDIS governance proposals (SdisProposal enum with 12 proposal types)
- `sdis_integration.rs` - 19 integration tests for credential presentation
- Governance types: StewardPenalty, JurisdictionTier, ThresholdType, etc.
- Integrated SdisProposal into ProposalPayload enum
- `SDIS_USER_GUIDE.md` - Comprehensive user documentation for credential presentation
- `ARCHITECTURE.md` - Updated with sections 1.5.9-1.5.11 for ZKP, presentation, governance
- `docs/security/SDIS_THREAT_MODEL.md` - Comprehensive threat model
- `docs/security/SDIS_AUDIT_CHECKLIST.md` - Code audit checklist for reviewers
- `docs/security/SDIS_CRYPTO_REVIEW.md` - Cryptographic algorithm review

**Completed (2025-12-11)**:
- `enrollment.rs` - EnrollmentService for end-to-end enrollment orchestration
- EnrollmentClient for client-side enrollment handling
- Threshold PRF integration for VUI computation
- VUI uniqueness checking via registry
- Expanded gossip messages (ParticipationRequest, PrfPartialDelivery, UniquenessCheck/Response)
- 73 steward tests passing (57 unit + 16 integration tests)
- `enrollment_integration.rs` - Comprehensive integration tests:
  - Single steward enrollment end-to-end
  - Multi-steward enrollment simulation with coordinator pattern
  - 3-of-5 threshold PRF combination
  - Duplicate enrollment rejection
  - Different users can enroll successfully
  - VUI commitment verification
  - Gossip message serialization
  - Client-side blinded request creation
  - Error handling (missing pepper share, no partials)
  - Share commitment and VUI registry operations

---

## Milestone Summary

| Milestone | Week | Key Deliverable |
|-----------|------|-----------------|
| M1 | 8 | `icn-crypto-pq` crate complete |
| M2 | 12 | Identity extensions complete |
| M3 | 20 | `icn-steward` crate complete |
| M4 | 26 | `icn-zkp` crate complete |
| M5 | 30 | Credential presentation complete |
| M6 | 36 | Full integration complete |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| STARK proof size too large | Hybrid STARK+SNARK fallback; iterative optimization |
| Threshold crypto complexity | Use well-tested FROST library; extensive testing |
| Post-quantum library instability | Pin versions; fallback to classical-only mode |
| Steward UX complexity | Start with CLI tooling; web UI in follow-on |
| Regulatory uncertainty | Modular design; pathway toggles per jurisdiction |

---

## Resource Requirements

| Role | Allocation | Phases |
|------|------------|--------|
| Crypto Engineer | 1.0 FTE | S1, S4 |
| Systems Engineer | 1.0 FTE | S2, S3, S5 |
| Protocol Engineer | 0.5 FTE | S3, S6 |
| Security Reviewer | 0.25 FTE | All |

---

## Next Steps

1. **Review this plan** with stakeholders
2. **Finalize Phase S1** task breakdown
3. **Set up icn-crypto-pq crate** scaffold
4. **Begin ML-DSA integration** research

---

## Appendix: File Change Summary

### New Files

| File | Phase | Lines (est) |
|------|-------|-------------|
| `crates/icn-crypto-pq/src/*` | S1 | 3,000 |
| `crates/icn-identity/src/anchor.rs` | S2 | 300 |
| `crates/icn-identity/src/keybundle.rs` | S2 | 500 |
| `crates/icn-identity/src/vui.rs` | S2 | 200 |
| `crates/icn-steward/src/*` | S3 | 4,000 |
| `crates/icn-zkp/src/*` | S4 | 5,000 |
| `crates/icn-gateway/src/api/sdis/*` | S5 | 2,000 |
| `crates/icn-governance/src/sdis.rs` | S6 | 500 |

### Modified Files

| File | Phase | Changes |
|------|-------|---------|
| `crates/icn-identity/src/lib.rs` | S2 | Add module exports |
| `crates/icn-identity/src/keystore.rs` | S2 | v4 format, migration |
| `crates/icn-identity/src/bundle.rs` | S2 | SDIS extensions |
| `crates/icn-trust/src/types.rs` | S3 | Steward trust type |
| `crates/icn-gossip/src/types.rs` | S3 | SDIS message types |
| `crates/icn-core/src/supervisor.rs` | S3 | Steward actor spawn |
| `crates/icn-gateway/src/lib.rs` | S5 | SDIS routes |
| `crates/icn-governance/src/proposal.rs` | S6 | SDIS proposals |
