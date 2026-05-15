//! Proof artifacts for the ICN legitimacy architecture.
//!
//! Every significant state change in ICN produces a content-addressed,
//! signed proof artifact. These proofs enable anyone to verify outcomes
//! without trusting any party.
//!
//! # Artifact Types (v0 Set)
//!
//! - [`ArtifactReceipt`] — proves a blob transfer completed and verified
//!   (ADR-0026 Layer 2; PR2c).
//! - [`AntiEntropyProbe`] — the probing message that opens an anti-entropy
//!   proof loop for a single state class against a target scope
//!   (`docs/spec/network-anti-entropy-proof-loops.md` §"Probe"; issue #1834).
//!   This is an evidence record, **not** a new ADR-0026 receipt class. It
//!   travels inside an existing Stage 5 `EffectDispatchEvidence` envelope or
//!   alongside a Layer 2 `ArtifactReceipt`.
//! - [`StateDigest`] — bounded representation of a state class at a freshness
//!   instant; four canonical projections (`Bloom`, `MerkleRoot`, `VectorClock`,
//!   `ShortList`). [`ReceiptDigest`] and [`ArtifactDigest`] are typed
//!   specializations binding a digest to its state class. Cross-link
//!   conversions between [`BloomProjection`] and `icn_gossip::types::BloomFilterData`
//!   live in `icn-gossip` so the kernel does not depend on the gossip layer.
//!
//! # Self-Authenticating Design
//!
//! Each receipt and probe contains a binding hash computed at construction
//! time via blake3 over a domain-separated, length-prefixed canonical
//! encoding of the significant fields. The corresponding `verify_binding()`
//! recomputes from fields and compares, enabling tamper detection without
//! external context.
//!
//! # Privacy and custody (anti-entropy artifacts)
//!
//! Per `docs/spec/network-anti-entropy-proof-loops.md` §"Privacy and custody
//! rules", probes and digests MUST NOT carry raw private content. An
//! [`ArtifactDigest`] over a scoped-vault reference proves existence and
//! scope; it does not expose the body. A digest derived over a `Member`- or
//! `NeedToKnow`-class object MUST NOT travel outside the disclosure scope of
//! the source.

use serde::{Deserialize, Serialize};

use crate::types::{Did, Hash, Signature};

/// Proof that a blob transfer completed and the content was verified.
///
/// Produced by the requester after all chunks are received, reassembled,
/// and the final blake3 hash matches the declared `blob_hash`.
///
/// The `receipt_hash` is self-authenticating: it is computed from the
/// binding fields at construction and can be re-verified at any time
/// via `verify_binding()`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactReceipt {
    /// blake3 hash of the complete blob
    pub blob_hash: Hash,
    /// DID of the node that served the blob
    pub provider_did: Did,
    /// DID of the node that requested and verified the blob
    pub requester_did: Did,
    /// Nonce binding this receipt to the originating request
    pub request_id: [u8; 32],
    /// Scope identifier for this transfer.
    /// TODO: Converge with canonical ScopeId type once available in kernel-api.
    /// Currently ScopeId lives only in icn-trust (domain crate, cannot import here).
    pub scope_id: String,
    /// Unix timestamp (seconds) when verification completed
    pub verified_at: u64,
    /// blake3 binding hash of all significant fields, computed at construction
    pub receipt_hash: Hash,
    /// Signature by the requester (empty until signed)
    pub signature: Signature,
}

impl ArtifactReceipt {
    /// Create a new receipt with computed binding hash and empty signature.
    pub fn new(
        blob_hash: Hash,
        provider_did: Did,
        requester_did: Did,
        request_id: [u8; 32],
        scope_id: String,
        verified_at: u64,
    ) -> Self {
        let receipt_hash = Self::compute_receipt_hash(
            &request_id,
            &blob_hash,
            &requester_did,
            &provider_did,
            &scope_id,
        );
        Self {
            blob_hash,
            provider_did,
            requester_did,
            request_id,
            scope_id,
            verified_at,
            receipt_hash,
            signature: Signature::new(Vec::new()),
        }
    }

    /// Domain separation tag for receipt hashes.
    ///
    /// Prevents cross-protocol hash collisions if the same field layout is
    /// reused in another proof type.
    pub const DOMAIN_TAG: &[u8] = b"icn:artifact-receipt:v1";

    /// Compute the binding hash from the significant fields.
    ///
    /// This is a pure function used by `new()` and `verify_binding()`.
    /// Variable-length fields are length-prefixed (u64 LE) to prevent
    /// collision attacks from redistributing bytes between adjacent fields.
    /// The domain separation tag is hashed first to prevent cross-protocol
    /// collisions.
    pub fn compute_receipt_hash(
        request_id: &[u8; 32],
        blob_hash: &Hash,
        requester_did: &Did,
        provider_did: &Did,
        scope_id: &str,
    ) -> Hash {
        let mut hasher = blake3::Hasher::new();
        // Domain separation: prevents hash collisions with other proof types
        hasher.update(Self::DOMAIN_TAG);
        // Fixed-length fields: no prefix needed
        hasher.update(request_id);
        hasher.update(blob_hash);
        // Variable-length fields: length-prefix each one
        hasher.update(&(requester_did.len() as u64).to_le_bytes());
        hasher.update(requester_did.as_bytes());
        hasher.update(&(provider_did.len() as u64).to_le_bytes());
        hasher.update(provider_did.as_bytes());
        hasher.update(&(scope_id.len() as u64).to_le_bytes());
        hasher.update(scope_id.as_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Verify that the stored `receipt_hash` matches a fresh computation.
    ///
    /// Returns `true` if the receipt has not been tampered with.
    pub fn verify_binding(&self) -> bool {
        let recomputed = Self::compute_receipt_hash(
            &self.request_id,
            &self.blob_hash,
            &self.requester_did,
            &self.provider_did,
            &self.scope_id,
        );
        self.receipt_hash == recomputed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_receipt() -> ArtifactReceipt {
        ArtifactReceipt::new(
            [0xAA; 32],
            "did:icn:provider123".to_string(),
            "did:icn:requester456".to_string(),
            [0xBB; 32],
            "coop:test-scope".to_string(),
            1700000000,
        )
    }

    #[test]
    fn receipt_hash_determinism() {
        let r1 = make_receipt();
        let r2 = make_receipt();
        assert_eq!(r1.receipt_hash, r2.receipt_hash);
        assert_ne!(r1.receipt_hash, [0u8; 32]);
    }

    #[test]
    fn verify_binding_succeeds_for_fresh_receipt() {
        let receipt = make_receipt();
        assert!(receipt.verify_binding());
    }

    #[test]
    fn tamper_blob_hash_detected() {
        let mut receipt = make_receipt();
        receipt.blob_hash = [0xFF; 32];
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn tamper_provider_did_detected() {
        let mut receipt = make_receipt();
        receipt.provider_did = "did:icn:attacker".to_string();
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn tamper_requester_did_detected() {
        let mut receipt = make_receipt();
        receipt.requester_did = "did:icn:attacker".to_string();
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn tamper_request_id_detected() {
        let mut receipt = make_receipt();
        receipt.request_id = [0xFF; 32];
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn tamper_scope_id_detected() {
        let mut receipt = make_receipt();
        receipt.scope_id = "evil-scope".to_string();
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn signature_starts_empty() {
        let receipt = make_receipt();
        assert!(receipt.signature.as_bytes().is_empty());
    }

    #[test]
    fn domain_tag_is_part_of_hash() {
        // Compute the receipt hash the normal way (with domain tag)
        let receipt = make_receipt();
        let with_tag = receipt.receipt_hash;

        // Compute manually without domain tag — must differ
        let mut hasher = blake3::Hasher::new();
        // Deliberately omit: hasher.update(ArtifactReceipt::DOMAIN_TAG);
        hasher.update(&receipt.request_id);
        hasher.update(&receipt.blob_hash);
        hasher.update(&(receipt.requester_did.len() as u64).to_le_bytes());
        hasher.update(receipt.requester_did.as_bytes());
        hasher.update(&(receipt.provider_did.len() as u64).to_le_bytes());
        hasher.update(receipt.provider_did.as_bytes());
        hasher.update(&(receipt.scope_id.len() as u64).to_le_bytes());
        hasher.update(receipt.scope_id.as_bytes());
        let without_tag: Hash = *hasher.finalize().as_bytes();

        assert_ne!(with_tag, without_tag, "domain tag must affect hash output");
    }

    #[test]
    fn length_prefix_prevents_field_collision() {
        // Without length prefixes, these two would hash identically because
        // the concatenation of provider_did || scope_id is the same bytes.
        let r1 = ArtifactReceipt::new(
            [0xAA; 32],
            "did:icn:ABC".to_string(),
            "did:icn:requester".to_string(),
            [0xBB; 32],
            "XYZ".to_string(),
            1700000000,
        );
        let r2 = ArtifactReceipt::new(
            [0xAA; 32],
            "did:icn:ABCXYZ".to_string(),
            "did:icn:requester".to_string(),
            [0xBB; 32],
            "".to_string(),
            1700000000,
        );
        assert_ne!(r1.receipt_hash, r2.receipt_hash);
        assert!(r1.verify_binding());
        assert!(r2.verify_binding());
    }
}

// ============================================================================
// Anti-entropy probe and state-digest records (issue #1834)
//
// Wire-stable record shapes for the design-level identifiers named in
// `docs/spec/network-anti-entropy-proof-loops.md` §"Proof artifacts
// (forward-direction names)". This module defines the **probe envelope** and
// the four **state-digest projections** only. The `DivergenceEvidence` /
// `RepairPlan` / `RepairReceipt` records belong to issue #1835; the fixture
// peers and Slice A loop belong to issue #1838.
//
// The probe is NOT a new ADR-0026 receipt class. It is an evidence record
// that rides inside an existing Stage 5 `EffectDispatchEvidence` envelope or
// alongside a Layer 2 `ArtifactReceipt`. No new top-level provenance layer is
// introduced here.
// ============================================================================

/// Probe schema version. Increment on any wire-affecting change to the
/// `AntiEntropyProbe` binding (field set, order, encoding).
pub const ANTI_ENTROPY_PROBE_SCHEMA_VERSION: u32 = 1;

/// Closed set of state classes anti-entropy proof loops can probe.
///
/// Matches the table in
/// `docs/spec/network-anti-entropy-proof-loops.md` §"State classes covered"
/// (nine classes). The kernel does not interpret what each class *means*;
/// it only routes probes and digests. Classification (which divergence
/// class a mismatch falls into) is a policy-oracle concern and lives in
/// the app layer per `docs/architecture/KERNEL_APP_SEPARATION.md`.
///
/// # Vocabulary
///
/// Class names are structural. `CclPolicyAdoption` is the version-adoption
/// state of a CCL policy registry entry, not an evaluator result. None of
/// these names are institution-package nouns.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum StateClass {
    /// Governance state — accepted proposals and Stage 5 effect dispatch
    /// evidence.
    GovernanceState,
    /// Receipts and receipt indexes (per ADR-0026).
    ReceiptIndex,
    /// Artifact-registry metadata
    /// (per `docs/spec/artifact-registry-and-scoped-vault.md`).
    ArtifactRegistryMetadata,
    /// Scoped-vault references. The digest is the *reference index*; the
    /// content body is never digested.
    ScopedVaultReference,
    /// Storage replica counts, backup-verification, and restore-test
    /// receipt references
    /// (per `docs/spec/storage-durability-policies.md`).
    StorageReplicaVerification,
    /// Compute receipts and placement / admission evidence
    /// (per `docs/spec/compute-placement-policy.md`).
    ComputeReceiptIndex,
    /// Settlement / obligation / allocation / position records
    /// (per `docs/spec/federation-settlement-finality.md` and #1634).
    SettlementRecordIndex,
    /// Federation membership / peer identity / trust-and-admission metadata.
    FederationMembership,
    /// CCL policy registry versions and evaluator bindings
    /// (per `docs/spec/ccl-policy-registry.md`).
    CclPolicyAdoption,
}

/// Target scope for an [`AntiEntropyProbe`].
///
/// Structural scopes only. Per
/// `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3, the local
/// institutional layer is named `LocalDomain(domain_id)` — not `Coop` —
/// because the owning entity class of a domain may be a cooperative, a
/// community, a federation, or another governed entity class. `Federation`
/// and `Commons` are structural scope names, not institution-package nouns.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProbeScope {
    /// A single local domain (community, cooperative, federation member,
    /// or other governed entity class), addressed by its opaque domain id.
    LocalDomain { domain_id: String },
    /// A federation, addressed by its opaque federation id.
    Federation { federation_id: String },
    /// The commons (publicly reachable scope).
    Commons,
    /// A directed pair of peers. Used when the probe is a bilateral
    /// reconciliation between two known peers rather than a scope-wide check.
    PeerPair { left: Did, right: Did },
}

/// Loop entry condition recorded on every [`AntiEntropyProbe`].
///
/// Per `docs/spec/network-anti-entropy-proof-loops.md` §"Schedule / trigger"
/// (five sources, in priority order).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSource {
    /// Governance-tunable interval probe (default 30s).
    Periodic,
    /// Replica count below target, backup overdue, restore-test cadence
    /// missed, or receipt-clearing batch nearing dispute-window expiry.
    ThresholdTriggered,
    /// A `DomainPolicy` change, stricter sync expectation, new
    /// `FederationSyncWindow` cadence, or explicit steward request.
    GovernanceTriggered,
    /// Suspected equivocation, peer churn beyond threshold, partition
    /// detected, or network-candidate-cache anomaly.
    IncidentTriggered,
    /// The probe is responding to a remote peer's probe.
    PeerRequested,
}

/// Response class the prober is requesting from the responding peer.
///
/// Per `docs/spec/network-anti-entropy-proof-loops.md` §"Probe" — the probe
/// carries "the requested response class (read-only digest exchange,
/// fetch-missing, repair authorization)."
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RequestedResponseClass {
    /// Read-only: exchange digests, do not fetch missing entries.
    DigestExchange,
    /// Fetch missing entries within scope. Does not authorize repair of
    /// authoritative state.
    FetchMissing,
    /// Authorize a bounded repair within an existing mandate. The prober
    /// asserts that authority exists; the responder verifies it.
    RepairAuthorization,
}

// ----------------------------------------------------------------------------
// StateDigest projections
// ----------------------------------------------------------------------------

/// Bloom-filter projection of a state class.
///
/// Wire-equivalent to `icn_gossip::types::BloomFilterData` on the `{bits,
/// num_hashes, size}` shape, plus an explicit `hint_count` for set
/// cardinality. The gossip layer provides
/// `icn_gossip::to_bloom_projection` and
/// `icn_gossip::to_bloom_filter_data` (re-exported from the crate root)
/// for lossless conversion;
/// `hint_count` is the only non-bloom field and must be supplied by the
/// caller because `BloomFilterData` does not carry it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BloomProjection {
    /// Packed bit array (LSB-first within each byte, matching
    /// `BloomFilterData::bits`).
    pub bits: Vec<u8>,
    /// Number of hash functions used to build the filter.
    pub num_hashes: u32,
    /// Filter size in bits.
    pub size: u64,
    /// Estimated cardinality (number of items inserted). A Bloom filter
    /// alone cannot recover this; carrying it explicitly lets receivers
    /// detect mass divergence without iterating their full local set.
    pub hint_count: u32,
}

/// Merkle-root projection of an ordered state class.
///
/// `root` is the canonical Merkle root over the state class's entries;
/// `leaf_count` is the number of leaves the root covers. The kernel does
/// not specify the leaf-hashing scheme — that is bound by the state class
/// (e.g., the receipt-index Merkle tree is defined by ADR-0026).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MerkleRootProjection {
    /// 32-byte canonical Merkle root.
    pub root: Hash,
    /// Number of leaves the root covers.
    pub leaf_count: u32,
}

/// Vector-clock projection of a partitioned state class.
///
/// Stores `(did, count)` entries **sorted by DID**, with duplicate DIDs
/// collapsed by keeping the maximum count (vector-clock merge semantics).
/// The invariant is enforced at construction (via [`Self::from_entries`])
/// AND on deserialization (via `#[serde(from = ...)]`): the wire form is
/// normalized before the value is constructed, so peer-supplied data that
/// arrives unsorted or with duplicates is silently canonicalized rather
/// than producing a non-canonical digest that would falsely diverge.
///
/// Wire-equivalent to the serialized form of
/// `icn_gossip::vector_clock::VectorClock`, which serializes only counts
/// (runtime `last_seen` instants are stripped). The field is private; use
/// [`Self::entries`] for read access.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "RawVectorClockProjection")]
pub struct VectorClockProjection {
    /// `(did, count)` pairs, sorted lexicographically by DID.
    entries: Vec<(Did, u64)>,
}

/// Raw wire form for [`VectorClockProjection`] — accepts arbitrary order /
/// duplicates and is normalized into the canonical form via the
/// `#[serde(from = ...)]` attribute on the public type.
#[derive(Deserialize)]
struct RawVectorClockProjection {
    entries: Vec<(Did, u64)>,
}

impl From<RawVectorClockProjection> for VectorClockProjection {
    fn from(raw: RawVectorClockProjection) -> Self {
        Self::from_entries(raw.entries)
    }
}

impl VectorClockProjection {
    /// Construct from an unsorted iterator of `(did, count)` pairs.
    ///
    /// Entries are sorted by DID to ensure canonical encoding. Duplicate
    /// DIDs are deduplicated by keeping the maximum count, which matches
    /// vector-clock merge semantics.
    pub fn from_entries<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = (Did, u64)>,
    {
        let mut map: std::collections::BTreeMap<Did, u64> = std::collections::BTreeMap::new();
        for (did, count) in entries {
            let entry = map.entry(did).or_insert(0);
            *entry = (*entry).max(count);
        }
        Self {
            entries: map.into_iter().collect(),
        }
    }

    /// The canonical `(did, count)` pairs, sorted lexicographically by DID.
    pub fn entries(&self) -> &[(Did, u64)] {
        &self.entries
    }
}

/// Short explicit list of content hashes.
///
/// Used when the state class is small enough that the false-positive rate
/// of a Bloom filter would dominate set-difference detection. The invariant
/// is enforced at construction (via [`Self::from_hashes`]) AND on
/// deserialization (via `#[serde(from = ...)]`): hashes are sorted
/// lexicographically and deduplicated before the value is constructed. The
/// field is private; use [`Self::hashes`] for read access.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "RawShortDigestList")]
pub struct ShortDigestList {
    /// Content hashes, sorted lexicographically.
    hashes: Vec<Hash>,
}

/// Raw wire form for [`ShortDigestList`] — accepts arbitrary order /
/// duplicates and is normalized into the canonical form via the
/// `#[serde(from = ...)]` attribute on the public type.
#[derive(Deserialize)]
struct RawShortDigestList {
    hashes: Vec<Hash>,
}

impl From<RawShortDigestList> for ShortDigestList {
    fn from(raw: RawShortDigestList) -> Self {
        Self::from_hashes(raw.hashes)
    }
}

impl ShortDigestList {
    /// Construct from an unsorted slice of hashes.
    ///
    /// Hashes are sorted and deduplicated to ensure canonical encoding.
    pub fn from_hashes<I>(hashes: I) -> Self
    where
        I: IntoIterator<Item = Hash>,
    {
        let mut v: Vec<Hash> = hashes.into_iter().collect();
        v.sort();
        v.dedup();
        Self { hashes: v }
    }

    /// The canonical, sorted, deduplicated content hashes.
    pub fn hashes(&self) -> &[Hash] {
        &self.hashes
    }
}

/// Bounded representation of a state class at a freshness instant.
///
/// Four canonical projections, matching
/// `docs/spec/network-anti-entropy-proof-loops.md` §"Proof artifacts
/// (forward-direction names)": Bloom filter, Merkle root, vector clock,
/// short digest list. A probe carries exactly one projection.
///
/// # Privacy
///
/// Per spec §"Privacy and custody rules" and Boundary rule 3, a
/// `StateDigest` MUST NOT carry raw private content. The four projections
/// are content-bounded by construction: a Bloom filter proves set
/// membership, a Merkle root proves a commitment, a vector clock carries
/// causal counters, and a short list carries content-addressed hashes —
/// none reveal the bodies they reference.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateDigest {
    /// Bloom-filter projection (preferred for receipt indexes, artifact
    /// metadata indexes, and other set-membership state classes).
    Bloom(BloomProjection),
    /// Merkle-root projection (for ordered, committed state).
    MerkleRoot(MerkleRootProjection),
    /// Vector-clock projection (for causal-ordering state classes such as
    /// gossip topic membership).
    VectorClock(VectorClockProjection),
    /// Short explicit list of content hashes.
    ShortList(ShortDigestList),
}

/// A [`StateDigest`] specialized to the receipt-index state class.
///
/// Newtype wrapper. The `state_class()` method returns
/// [`StateClass::ReceiptIndex`] so call sites cannot accidentally bind a
/// receipt digest to a different state class.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct ReceiptDigest(pub StateDigest);

impl ReceiptDigest {
    /// Wrap a `StateDigest` as a receipt-index specialization.
    pub fn new(digest: StateDigest) -> Self {
        Self(digest)
    }

    /// Always [`StateClass::ReceiptIndex`].
    pub fn state_class(&self) -> StateClass {
        StateClass::ReceiptIndex
    }

    /// The underlying `StateDigest`.
    pub fn digest(&self) -> &StateDigest {
        &self.0
    }
}

/// A [`StateDigest`] specialized to an artifact-registry entry or
/// scoped-vault reference.
///
/// Modeled as a closed enum so the state-class specialization is enforced
/// by the type system: no constructor or deserialization path can produce
/// an `ArtifactDigest` tagged with [`StateClass::ReceiptIndex`],
/// [`StateClass::GovernanceState`], etc. — only the two artifact classes
/// are representable. This is stricter than the original two-field-struct
/// shape, which allowed any `StateClass` value to enter via derived
/// `Deserialize`.
///
/// Per spec §"Privacy and custody rules" and Boundary rule 3, an
/// `ArtifactDigest::ScopedVaultReference` proves existence and scope. It
/// never proves content. The kernel cannot enforce that property by type
/// alone (a digest's *bytes* are opaque) but the privacy contract is
/// documented, reviewable, and bound to the variant.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactDigest {
    /// Public artifact-registry metadata
    /// (per `docs/spec/artifact-registry-and-scoped-vault.md`). The digest
    /// covers public metadata; replication is governed by the artifact's
    /// `privacy_class`.
    Registry(StateDigest),
    /// Opaque reference to a scoped-vault object. The digest is over the
    /// reference index, never the object body.
    ScopedVaultReference(StateDigest),
}

impl ArtifactDigest {
    /// Construct an `ArtifactRegistryMetadata` specialization.
    pub fn registry(digest: StateDigest) -> Self {
        Self::Registry(digest)
    }

    /// Construct a `ScopedVaultReference` specialization.
    ///
    /// The caller is responsible for ensuring the digest was computed over
    /// references, not bodies. The kernel cannot enforce this by type
    /// alone, but the privacy contract is documented and reviewable.
    pub fn scoped_vault_reference(digest: StateDigest) -> Self {
        Self::ScopedVaultReference(digest)
    }

    /// The state class this digest is specialized to. Always either
    /// [`StateClass::ArtifactRegistryMetadata`] or
    /// [`StateClass::ScopedVaultReference`].
    pub fn state_class(&self) -> StateClass {
        match self {
            Self::Registry(_) => StateClass::ArtifactRegistryMetadata,
            Self::ScopedVaultReference(_) => StateClass::ScopedVaultReference,
        }
    }

    /// The underlying [`StateDigest`] for either variant.
    pub fn digest(&self) -> &StateDigest {
        match self {
            Self::Registry(d) | Self::ScopedVaultReference(d) => d,
        }
    }
}

// ----------------------------------------------------------------------------
// AntiEntropyProbe
// ----------------------------------------------------------------------------

/// The probing message that opens an anti-entropy proof loop.
///
/// One probe = one state class against one target scope. The probe carries
/// a bounded [`StateDigest`], the prober's [`Did`] and signature, a
/// [`TriggerSource`], a freshness window, and the [`RequestedResponseClass`].
/// The probe is self-authenticating: the `probe_hash` field is a blake3
/// binding hash computed over a domain-separated canonical encoding of all
/// other fields, recomputable via [`AntiEntropyProbe::verify_binding`].
///
/// # Scope and lifecycle
///
/// - The probe IS the phase-2 ("probe") record in the spec's eight-phase
///   loop. The comparison result is the (forthcoming) `PeerSyncReport`; the
///   classified outcome is the (forthcoming, #1835) `DivergenceEvidence`.
/// - The probe is NOT a new top-level ADR-0026 receipt class. It is an
///   evidence record that travels inside an existing
///   `EffectDispatchEvidence` envelope (per
///   `docs/spec/effect-dispatch-contract.md` Stage 5) or alongside a Layer 2
///   `ArtifactReceipt` envelope where the loop produces a blob-transfer
///   repair.
/// - The probe is bounded in content: per Boundary rule 3, it MUST NOT
///   carry raw private content. The four [`StateDigest`] projections are
///   the only payload forms.
///
/// # Freshness
///
/// `freshness_emitted_at` is the prober's clock at construction;
/// `freshness_valid_until` is the timestamp beyond which a responder
/// should treat the probe as stale and respond with "unknown / out of
/// scope" per spec §"Compare" rather than producing `DivergenceEvidence`.
/// Both timestamps are Unix seconds.
///
/// # Nonce
///
/// `probe_nonce` is a 32-byte random value chosen by the prober. Two
/// probes with otherwise-identical fields produce distinct `probe_hash`es,
/// which lets a higher-layer replay guard reject duplicates without
/// confusing periodic resends with malicious replays.
///
/// # Signature
///
/// `signature` is left empty by [`AntiEntropyProbe::new`]; a higher-layer
/// signing step fills it. The verifier MUST verify both the signature and
/// `verify_binding()`; either alone is insufficient.
///
/// # Wire-version policing (fail-closed)
///
/// Deserialization uses `#[serde(try_from = "RawAntiEntropyProbe")]` so
/// wire data with `schema_version != ANTI_ENTROPY_PROBE_SCHEMA_VERSION`
/// is rejected before any `AntiEntropyProbe` value is constructed. This
/// closes the bypass where a peer could send a probe tagged with a future
/// or bogus version, recompute the binding hash over that version, and
/// have a current node accept it. The version field is also re-checked by
/// [`AntiEntropyProbe::verify_binding`] so a manually-mutated probe with
/// a recomputed hash still fails closed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "RawAntiEntropyProbe")]
pub struct AntiEntropyProbe {
    /// Wire schema version. See [`ANTI_ENTROPY_PROBE_SCHEMA_VERSION`].
    /// Probes whose `schema_version` does not match are rejected at the
    /// deserialization boundary (via `#[serde(try_from = ...)]`) and by
    /// [`Self::verify_binding`] on already-constructed values.
    pub schema_version: u32,
    /// State class being probed.
    pub state_class: StateClass,
    /// Target scope.
    pub target_scope: ProbeScope,
    /// Bounded digest (one of four projections). Never raw content.
    pub digest: StateDigest,
    /// DID of the prober (identity issuing this probe).
    pub prober_did: Did,
    /// Why the loop was triggered.
    pub trigger_source: TriggerSource,
    /// Prober's clock at construction (Unix seconds).
    pub freshness_emitted_at: u64,
    /// Timestamp beyond which the probe is stale (Unix seconds).
    pub freshness_valid_until: u64,
    /// Requested response class.
    pub requested_response: RequestedResponseClass,
    /// 32-byte random nonce. Two probes with otherwise-identical fields
    /// produce distinct `probe_hash`es.
    pub probe_nonce: [u8; 32],
    /// blake3 binding hash over all bound fields, computed at construction.
    pub probe_hash: Hash,
    /// Prober signature (empty until signed by a higher layer).
    pub signature: Signature,
}

/// Raw wire form for [`AntiEntropyProbe`]. Accepts any `schema_version`
/// during deserialization and is validated into the public type via
/// [`TryFrom<RawAntiEntropyProbe> for AntiEntropyProbe`].
///
/// The raw form exists exclusively so the conversion path can fail closed
/// on unsupported versions before any consumer ever observes the value.
#[derive(Deserialize)]
struct RawAntiEntropyProbe {
    schema_version: u32,
    state_class: StateClass,
    target_scope: ProbeScope,
    digest: StateDigest,
    prober_did: Did,
    trigger_source: TriggerSource,
    freshness_emitted_at: u64,
    freshness_valid_until: u64,
    requested_response: RequestedResponseClass,
    probe_nonce: [u8; 32],
    probe_hash: Hash,
    signature: Signature,
}

impl TryFrom<RawAntiEntropyProbe> for AntiEntropyProbe {
    type Error = String;

    fn try_from(raw: RawAntiEntropyProbe) -> Result<Self, Self::Error> {
        if raw.schema_version != ANTI_ENTROPY_PROBE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported AntiEntropyProbe schema_version: got {}, supported {}",
                raw.schema_version, ANTI_ENTROPY_PROBE_SCHEMA_VERSION,
            ));
        }
        Ok(Self {
            schema_version: raw.schema_version,
            state_class: raw.state_class,
            target_scope: raw.target_scope,
            digest: raw.digest,
            prober_did: raw.prober_did,
            trigger_source: raw.trigger_source,
            freshness_emitted_at: raw.freshness_emitted_at,
            freshness_valid_until: raw.freshness_valid_until,
            requested_response: raw.requested_response,
            probe_nonce: raw.probe_nonce,
            probe_hash: raw.probe_hash,
            signature: raw.signature,
        })
    }
}

/// Canonical binding fields for `AntiEntropyProbe::probe_hash`.
///
/// This struct exists solely so bincode serializes the binding fields in a
/// stable order independent of struct-field reordering in [`AntiEntropyProbe`].
/// Excludes `probe_hash` (the output) and `signature` (filled after binding).
#[derive(Serialize)]
struct ProbeBinding<'a> {
    schema_version: u32,
    state_class: StateClass,
    target_scope: &'a ProbeScope,
    digest: &'a StateDigest,
    prober_did: &'a Did,
    trigger_source: TriggerSource,
    freshness_emitted_at: u64,
    freshness_valid_until: u64,
    requested_response: RequestedResponseClass,
    probe_nonce: [u8; 32],
}

impl AntiEntropyProbe {
    /// Domain-separation tag for `probe_hash`. Prevents cross-protocol
    /// collisions with [`ArtifactReceipt::DOMAIN_TAG`] and future proof
    /// classes.
    pub const DOMAIN_TAG: &'static [u8] = b"icn:anti-entropy-probe:v1";

    /// Construct a new probe with computed `probe_hash` and empty
    /// signature.
    ///
    /// `schema_version` is set to [`ANTI_ENTROPY_PROBE_SCHEMA_VERSION`].
    /// The caller must sign the probe at a higher layer before emission.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        state_class: StateClass,
        target_scope: ProbeScope,
        digest: StateDigest,
        prober_did: Did,
        trigger_source: TriggerSource,
        freshness_emitted_at: u64,
        freshness_valid_until: u64,
        requested_response: RequestedResponseClass,
        probe_nonce: [u8; 32],
    ) -> Self {
        let probe_hash = Self::compute_probe_hash(
            ANTI_ENTROPY_PROBE_SCHEMA_VERSION,
            state_class,
            &target_scope,
            &digest,
            &prober_did,
            trigger_source,
            freshness_emitted_at,
            freshness_valid_until,
            requested_response,
            &probe_nonce,
        );
        Self {
            schema_version: ANTI_ENTROPY_PROBE_SCHEMA_VERSION,
            state_class,
            target_scope,
            digest,
            prober_did,
            trigger_source,
            freshness_emitted_at,
            freshness_valid_until,
            requested_response,
            probe_nonce,
            probe_hash,
            signature: Signature::new(Vec::new()),
        }
    }

    /// Compute the binding hash from the significant fields.
    ///
    /// Pure function. Used by [`AntiEntropyProbe::new`] and
    /// [`AntiEntropyProbe::verify_binding`]. The hash is `blake3(DOMAIN_TAG
    /// || len(payload) || payload)` where `payload` is the bincode
    /// canonical encoding of [`ProbeBinding`]. The length prefix prevents
    /// canonical-form confusion with future binding fields and the domain
    /// tag prevents cross-protocol collisions.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_probe_hash(
        schema_version: u32,
        state_class: StateClass,
        target_scope: &ProbeScope,
        digest: &StateDigest,
        prober_did: &Did,
        trigger_source: TriggerSource,
        freshness_emitted_at: u64,
        freshness_valid_until: u64,
        requested_response: RequestedResponseClass,
        probe_nonce: &[u8; 32],
    ) -> Hash {
        let binding = ProbeBinding {
            schema_version,
            state_class,
            target_scope,
            digest,
            prober_did,
            trigger_source,
            freshness_emitted_at,
            freshness_valid_until,
            requested_response,
            probe_nonce: *probe_nonce,
        };
        // bincode v1 is deterministic for `Serialize` impls in this crate
        // (fixed-int encoding, little-endian, no field reordering).
        let payload =
            bincode::serialize(&binding).expect("ProbeBinding serialization is infallible");

        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        *hasher.finalize().as_bytes()
    }

    /// `true` iff `schema_version == ANTI_ENTROPY_PROBE_SCHEMA_VERSION`.
    ///
    /// Convenience for callers that want to assert wire-version
    /// compatibility explicitly. Deserialization already rejects
    /// unsupported versions via `try_from`; this method is the in-Rust
    /// guard for values that were constructed directly (e.g., via
    /// [`Self::new`], or manually mutated).
    pub fn is_supported_schema_version(&self) -> bool {
        self.schema_version == ANTI_ENTROPY_PROBE_SCHEMA_VERSION
    }

    /// Verify that the stored `probe_hash` matches a fresh computation
    /// AND that `schema_version == ANTI_ENTROPY_PROBE_SCHEMA_VERSION`.
    ///
    /// Returns `true` only if both hold. Returning `false` on an
    /// unsupported `schema_version` is a fail-closed property: an attacker
    /// cannot evade version policing by recomputing the binding hash over
    /// a bogus version (because that path would have to go through this
    /// method's version check) and cannot evade it by deserializing such
    /// a probe (because `try_from` rejects it). Does NOT verify the
    /// signature — that is a higher-layer concern.
    pub fn verify_binding(&self) -> bool {
        if !self.is_supported_schema_version() {
            return false;
        }
        let recomputed = Self::compute_probe_hash(
            self.schema_version,
            self.state_class,
            &self.target_scope,
            &self.digest,
            &self.prober_did,
            self.trigger_source,
            self.freshness_emitted_at,
            self.freshness_valid_until,
            self.requested_response,
            &self.probe_nonce,
        );
        self.probe_hash == recomputed
    }

    /// `true` if `now_unix_seconds <= freshness_valid_until`.
    ///
    /// Convenience helper for the responder's "Compare" phase: a probe
    /// past its freshness window MUST be answered with "unknown / out of
    /// scope" rather than classified as divergent.
    pub fn is_fresh(&self, now_unix_seconds: u64) -> bool {
        now_unix_seconds <= self.freshness_valid_until
    }
}

#[cfg(test)]
mod anti_entropy_tests {
    use super::*;

    fn sample_bloom() -> BloomProjection {
        // Adaptive-style filter for a fixture set of 4 receipts:
        // 64-bit array, 1 hash function, 4 inserts → hint_count = 4.
        BloomProjection {
            bits: vec![0b0001_0101, 0, 0, 0, 0, 0, 0, 0],
            num_hashes: 1,
            size: 64,
            hint_count: 4,
        }
    }

    fn sample_probe() -> AntiEntropyProbe {
        AntiEntropyProbe::new(
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            StateDigest::Bloom(sample_bloom()),
            "did:icn:prober123".to_string(),
            TriggerSource::Periodic,
            1_715_000_000,
            1_715_000_030,
            RequestedResponseClass::DigestExchange,
            [0xCD; 32],
        )
    }

    // ---- Probe binding: determinism, tamper detection, domain separation ----

    #[test]
    fn probe_binding_is_deterministic() {
        let p1 = sample_probe();
        let p2 = sample_probe();
        assert_eq!(p1.probe_hash, p2.probe_hash);
        assert_ne!(p1.probe_hash, [0u8; 32]);
    }

    #[test]
    fn probe_verify_binding_succeeds_for_fresh_probe() {
        let probe = sample_probe();
        assert!(probe.verify_binding());
    }

    #[test]
    fn probe_signature_starts_empty() {
        let probe = sample_probe();
        assert!(probe.signature.as_bytes().is_empty());
    }

    #[test]
    fn probe_nonce_changes_hash() {
        let mut probe = sample_probe();
        let original = probe.probe_hash;
        probe.probe_nonce = [0xAB; 32];
        // The stored hash is now stale relative to the new nonce.
        assert!(!probe.verify_binding());
        // Rebuilding with the new nonce gives a different hash than the
        // original — proving the nonce binds into the hash.
        let rebuilt = AntiEntropyProbe::new(
            probe.state_class,
            probe.target_scope.clone(),
            probe.digest.clone(),
            probe.prober_did.clone(),
            probe.trigger_source,
            probe.freshness_emitted_at,
            probe.freshness_valid_until,
            probe.requested_response,
            probe.probe_nonce,
        );
        assert_ne!(rebuilt.probe_hash, original);
        assert!(rebuilt.verify_binding());
    }

    #[test]
    fn probe_state_class_change_detected() {
        let mut probe = sample_probe();
        probe.state_class = StateClass::ArtifactRegistryMetadata;
        assert!(!probe.verify_binding());
    }

    #[test]
    fn probe_target_scope_change_detected() {
        let mut probe = sample_probe();
        probe.target_scope = ProbeScope::Commons;
        assert!(!probe.verify_binding());
    }

    #[test]
    fn probe_digest_change_detected() {
        let mut probe = sample_probe();
        let mut bloom = sample_bloom();
        bloom.bits[0] = 0xFF;
        probe.digest = StateDigest::Bloom(bloom);
        assert!(!probe.verify_binding());
    }

    #[test]
    fn probe_did_change_detected() {
        let mut probe = sample_probe();
        probe.prober_did = "did:icn:attacker".to_string();
        assert!(!probe.verify_binding());
    }

    #[test]
    fn probe_trigger_source_change_detected() {
        let mut probe = sample_probe();
        probe.trigger_source = TriggerSource::IncidentTriggered;
        assert!(!probe.verify_binding());
    }

    #[test]
    fn probe_freshness_change_detected() {
        let mut probe = sample_probe();
        probe.freshness_emitted_at += 1;
        assert!(!probe.verify_binding());

        let mut probe = sample_probe();
        probe.freshness_valid_until += 1;
        assert!(!probe.verify_binding());
    }

    #[test]
    fn probe_requested_response_change_detected() {
        let mut probe = sample_probe();
        probe.requested_response = RequestedResponseClass::RepairAuthorization;
        assert!(!probe.verify_binding());
    }

    #[test]
    fn probe_schema_version_change_detected() {
        let mut probe = sample_probe();
        probe.schema_version = ANTI_ENTROPY_PROBE_SCHEMA_VERSION + 1;
        assert!(!probe.verify_binding());
    }

    #[test]
    fn probe_domain_tag_affects_hash() {
        // Recompute with no domain tag — must differ from the bound hash.
        let probe = sample_probe();
        let binding = ProbeBinding {
            schema_version: probe.schema_version,
            state_class: probe.state_class,
            target_scope: &probe.target_scope,
            digest: &probe.digest,
            prober_did: &probe.prober_did,
            trigger_source: probe.trigger_source,
            freshness_emitted_at: probe.freshness_emitted_at,
            freshness_valid_until: probe.freshness_valid_until,
            requested_response: probe.requested_response,
            probe_nonce: probe.probe_nonce,
        };
        let payload = bincode::serialize(&binding).unwrap();
        let mut hasher = blake3::Hasher::new();
        // Deliberately omit DOMAIN_TAG.
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        let without_tag: Hash = *hasher.finalize().as_bytes();
        assert_ne!(probe.probe_hash, without_tag);
    }

    #[test]
    fn probe_two_probes_with_distinct_nonces_have_distinct_hashes() {
        let p1 = sample_probe();
        let p2 = AntiEntropyProbe::new(
            p1.state_class,
            p1.target_scope.clone(),
            p1.digest.clone(),
            p1.prober_did.clone(),
            p1.trigger_source,
            p1.freshness_emitted_at,
            p1.freshness_valid_until,
            p1.requested_response,
            [0xEF; 32],
        );
        assert_ne!(p1.probe_hash, p2.probe_hash);
        assert!(p2.verify_binding());
    }

    // ---- Freshness helper ----

    #[test]
    fn probe_is_fresh_within_window() {
        let probe = sample_probe();
        assert!(probe.is_fresh(probe.freshness_emitted_at));
        assert!(probe.is_fresh(probe.freshness_valid_until));
        assert!(!probe.is_fresh(probe.freshness_valid_until + 1));
    }

    // ---- Round-trip: bincode + JSON ----

    #[test]
    fn probe_bincode_roundtrip_preserves_hash() {
        let original = sample_probe();
        let bytes = bincode::serialize(&original).unwrap();
        let restored: AntiEntropyProbe = bincode::deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
        assert_eq!(original.probe_hash, restored.probe_hash);
        assert!(restored.verify_binding());
    }

    #[test]
    fn probe_json_roundtrip_preserves_hash() {
        let original = sample_probe();
        let json = serde_json::to_string(&original).unwrap();
        let restored: AntiEntropyProbe = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    // ---- StateDigest projection round-trips ----

    #[test]
    fn state_digest_bloom_roundtrip() {
        let digest = StateDigest::Bloom(sample_bloom());
        let bytes = bincode::serialize(&digest).unwrap();
        let restored: StateDigest = bincode::deserialize(&bytes).unwrap();
        assert_eq!(digest, restored);
    }

    #[test]
    fn state_digest_merkle_root_roundtrip() {
        let digest = StateDigest::MerkleRoot(MerkleRootProjection {
            root: [0x11; 32],
            leaf_count: 7,
        });
        let bytes = bincode::serialize(&digest).unwrap();
        let restored: StateDigest = bincode::deserialize(&bytes).unwrap();
        assert_eq!(digest, restored);
    }

    #[test]
    fn state_digest_vector_clock_roundtrip() {
        let digest = StateDigest::VectorClock(VectorClockProjection::from_entries(vec![
            ("did:icn:c".to_string(), 1),
            ("did:icn:a".to_string(), 5),
            ("did:icn:b".to_string(), 3),
        ]));
        // Entries must be sorted by DID after construction.
        if let StateDigest::VectorClock(ref proj) = digest {
            let dids: Vec<&str> = proj.entries().iter().map(|(d, _)| d.as_str()).collect();
            assert_eq!(dids, vec!["did:icn:a", "did:icn:b", "did:icn:c"]);
        } else {
            panic!("expected VectorClock variant");
        }
        let bytes = bincode::serialize(&digest).unwrap();
        let restored: StateDigest = bincode::deserialize(&bytes).unwrap();
        assert_eq!(digest, restored);
    }

    #[test]
    fn state_digest_short_list_roundtrip() {
        let digest = StateDigest::ShortList(ShortDigestList::from_hashes(vec![
            [0x03; 32], [0x01; 32], [0x02; 32], [0x01; 32], // duplicate
        ]));
        if let StateDigest::ShortList(ref list) = digest {
            assert_eq!(list.hashes().len(), 3, "duplicates must be deduplicated");
            assert!(
                list.hashes().windows(2).all(|w| w[0] < w[1]),
                "hashes must be sorted"
            );
        } else {
            panic!("expected ShortList variant");
        }
        let bytes = bincode::serialize(&digest).unwrap();
        let restored: StateDigest = bincode::deserialize(&bytes).unwrap();
        assert_eq!(digest, restored);
    }

    #[test]
    fn state_digest_vector_clock_canonical_independent_of_insertion_order() {
        // Two projections built from the same set of entries in different
        // orders must serialize to the same bytes (and thus hash identically
        // when carried inside a probe).
        let a = VectorClockProjection::from_entries(vec![
            ("did:icn:a".to_string(), 5),
            ("did:icn:b".to_string(), 3),
            ("did:icn:c".to_string(), 1),
        ]);
        let b = VectorClockProjection::from_entries(vec![
            ("did:icn:c".to_string(), 1),
            ("did:icn:a".to_string(), 5),
            ("did:icn:b".to_string(), 3),
        ]);
        assert_eq!(a, b);
        assert_eq!(
            bincode::serialize(&a).unwrap(),
            bincode::serialize(&b).unwrap()
        );
    }

    #[test]
    fn state_digest_vector_clock_max_count_on_duplicate_did() {
        // A vector clock built from duplicate DIDs must keep the max count —
        // this matches the merge semantics of `icn-gossip`'s `VectorClock`.
        let proj = VectorClockProjection::from_entries(vec![
            ("did:icn:a".to_string(), 3),
            ("did:icn:a".to_string(), 7),
            ("did:icn:a".to_string(), 1),
        ]);
        assert_eq!(proj.entries().len(), 1);
        assert_eq!(proj.entries()[0], ("did:icn:a".to_string(), 7));
    }

    // ---- Canonical-form enforcement on the wire (review feedback) ----

    #[test]
    fn vector_clock_projection_normalizes_unsorted_wire_input() {
        // Wire data that arrives with unsorted DIDs must be normalized to
        // canonical form on deserialization, not stored as-received. If this
        // ever regresses, two peers that built logically-identical clocks
        // would compute different digest hashes and falsely diverge.
        let bad_json = r#"{"entries":[["did:icn:c",1],["did:icn:a",5],["did:icn:b",3]]}"#;
        let proj: VectorClockProjection = serde_json::from_str(bad_json).unwrap();
        let dids: Vec<&str> = proj.entries().iter().map(|(d, _)| d.as_str()).collect();
        assert_eq!(dids, vec!["did:icn:a", "did:icn:b", "did:icn:c"]);
    }

    #[test]
    fn vector_clock_projection_collapses_duplicate_did_on_wire() {
        // Wire data with a duplicated DID must collapse to one entry holding
        // the maximum count (vector-clock merge semantics).
        let bad_json = r#"{"entries":[["did:icn:a",3],["did:icn:a",7],["did:icn:a",1]]}"#;
        let proj: VectorClockProjection = serde_json::from_str(bad_json).unwrap();
        assert_eq!(proj.entries().len(), 1);
        assert_eq!(proj.entries()[0], ("did:icn:a".to_string(), 7));
    }

    #[test]
    fn vector_clock_projection_normalization_is_bincode_path_too() {
        // Same normalization must apply on bincode-deserialized wire data.
        // Build a non-canonical payload via a local Serialize-only helper
        // (the production `Raw…` types are deserialize-only by design).
        #[derive(Serialize)]
        struct LocalBadWire {
            entries: Vec<(Did, u64)>,
        }
        let bad = LocalBadWire {
            entries: vec![
                ("did:icn:c".to_string(), 1),
                ("did:icn:a".to_string(), 5),
                ("did:icn:a".to_string(), 9),
                ("did:icn:b".to_string(), 3),
            ],
        };
        let bytes = bincode::serialize(&bad).unwrap();
        let proj: VectorClockProjection = bincode::deserialize(&bytes).unwrap();
        let dids: Vec<&str> = proj.entries().iter().map(|(d, _)| d.as_str()).collect();
        assert_eq!(dids, vec!["did:icn:a", "did:icn:b", "did:icn:c"]);
        // Duplicate DID must collapse to max count.
        assert_eq!(proj.entries()[0].1, 9);
    }

    #[test]
    fn short_digest_list_normalizes_unsorted_wire_input() {
        let bad_json = r#"{"hashes":[[3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3],[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],[2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2]]}"#;
        let list: ShortDigestList = serde_json::from_str(bad_json).unwrap();
        assert_eq!(list.hashes().len(), 3);
        assert!(list.hashes().windows(2).all(|w| w[0] < w[1]));
        assert_eq!(list.hashes()[0], [0x01; 32]);
        assert_eq!(list.hashes()[2], [0x03; 32]);
    }

    #[test]
    fn short_digest_list_dedups_on_wire() {
        #[derive(Serialize)]
        struct LocalBadWire {
            hashes: Vec<Hash>,
        }
        let bad = LocalBadWire {
            hashes: vec![[0x01; 32], [0x01; 32], [0x02; 32], [0x01; 32]],
        };
        let list: ShortDigestList =
            bincode::deserialize(&bincode::serialize(&bad).unwrap()).unwrap();
        assert_eq!(list.hashes().len(), 2);
        assert_eq!(list.hashes()[0], [0x01; 32]);
        assert_eq!(list.hashes()[1], [0x02; 32]);
    }

    #[test]
    fn artifact_digest_invalid_state_class_cannot_decode() {
        // The original two-field-struct shape accepted any `StateClass` via
        // derived `Deserialize`. After the enum refactor, the wire form is
        // tagged by variant — there is no way to express
        // `state_class=receipt_index` on the wire because that variant does
        // not exist. The closest analog (a JSON payload tagged with a
        // foreign top-level key) must be rejected.
        let bogus_json = r#"{"receipt_index":{"merkle_root":{"root":[0;32],"leaf_count":0}}}"#;
        let parsed: Result<ArtifactDigest, _> = serde_json::from_str(bogus_json);
        assert!(
            parsed.is_err(),
            "ArtifactDigest must not deserialize from a non-artifact variant tag"
        );
    }

    #[test]
    fn artifact_digest_unknown_variant_rejected() {
        let bogus_json = r#"{"governance_state":{"merkle_root":{"root":[0;32],"leaf_count":0}}}"#;
        let parsed: Result<ArtifactDigest, _> = serde_json::from_str(bogus_json);
        assert!(parsed.is_err());
    }

    // ---- StateClass / TriggerSource / RequestedResponseClass / ProbeScope ----

    #[test]
    fn state_class_serde_uses_snake_case() {
        let json = serde_json::to_string(&StateClass::ReceiptIndex).unwrap();
        assert_eq!(json, "\"receipt_index\"");
        let parsed: StateClass = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, StateClass::ReceiptIndex);
    }

    #[test]
    fn trigger_source_serde_uses_snake_case() {
        let json = serde_json::to_string(&TriggerSource::IncidentTriggered).unwrap();
        assert_eq!(json, "\"incident_triggered\"");
    }

    #[test]
    fn requested_response_serde_uses_snake_case() {
        let json = serde_json::to_string(&RequestedResponseClass::RepairAuthorization).unwrap();
        assert_eq!(json, "\"repair_authorization\"");
    }

    #[test]
    fn probe_scope_serde_local_domain() {
        let scope = ProbeScope::LocalDomain {
            domain_id: "dom-1".to_string(),
        };
        let json = serde_json::to_string(&scope).unwrap();
        // Externally-tagged: {"local_domain":{"domain_id":"dom-1"}}.
        assert!(json.contains("\"local_domain\""));
        assert!(json.contains("\"domain_id\":\"dom-1\""));
        let parsed: ProbeScope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, scope);
    }

    #[test]
    fn probe_scope_serde_commons_is_unit() {
        let scope = ProbeScope::Commons;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, "\"commons\"");
        let parsed: ProbeScope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, scope);
    }

    #[test]
    fn probe_scope_serde_peer_pair() {
        let scope = ProbeScope::PeerPair {
            left: "did:icn:a".to_string(),
            right: "did:icn:b".to_string(),
        };
        let json = serde_json::to_string(&scope).unwrap();
        let parsed: ProbeScope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, scope);
    }

    // ---- ReceiptDigest / ArtifactDigest specializations ----

    #[test]
    fn receipt_digest_binds_state_class() {
        let rd = ReceiptDigest::new(StateDigest::Bloom(sample_bloom()));
        assert_eq!(rd.state_class(), StateClass::ReceiptIndex);
        // Round-trip preserves equality.
        let json = serde_json::to_string(&rd).unwrap();
        let restored: ReceiptDigest = serde_json::from_str(&json).unwrap();
        assert_eq!(rd, restored);
    }

    #[test]
    fn artifact_digest_registry_vs_scoped_vault() {
        let registry = ArtifactDigest::registry(StateDigest::MerkleRoot(MerkleRootProjection {
            root: [0x22; 32],
            leaf_count: 3,
        }));
        assert_eq!(registry.state_class(), StateClass::ArtifactRegistryMetadata);
        assert!(matches!(registry, ArtifactDigest::Registry(_)));

        let vault = ArtifactDigest::scoped_vault_reference(StateDigest::ShortList(
            ShortDigestList::from_hashes(vec![[0x01; 32]]),
        ));
        assert_eq!(vault.state_class(), StateClass::ScopedVaultReference);
        assert!(matches!(vault, ArtifactDigest::ScopedVaultReference(_)));

        // Round-trip both via JSON and bincode.
        for ad in [&registry, &vault] {
            let json = serde_json::to_string(ad).unwrap();
            let restored: ArtifactDigest = serde_json::from_str(&json).unwrap();
            assert_eq!(ad, &restored);

            let bytes = bincode::serialize(ad).unwrap();
            let restored_bin: ArtifactDigest = bincode::deserialize(&bytes).unwrap();
            assert_eq!(ad, &restored_bin);
        }
    }

    // ---- Cross-protocol domain separation ----

    #[test]
    fn probe_hash_differs_from_artifact_receipt_hash() {
        // Sanity check that the probe domain tag is not the artifact-receipt
        // domain tag. (A literal-string comparison is the cheapest assertion
        // that the two are distinct.)
        assert_ne!(
            AntiEntropyProbe::DOMAIN_TAG,
            ArtifactReceipt::DOMAIN_TAG,
            "anti-entropy probe and artifact receipt must use distinct domain tags"
        );
    }

    // ---- Schema-version rejection (review feedback) ----

    #[test]
    fn probe_is_supported_schema_version_helper() {
        let probe = sample_probe();
        assert!(probe.is_supported_schema_version());
        assert_eq!(probe.schema_version, ANTI_ENTROPY_PROBE_SCHEMA_VERSION);
    }

    /// Helper: serialize a probe through a Raw-shaped wire form so the
    /// `schema_version` can be set to a value that the public type would
    /// otherwise reject at deserialization. Used to construct hostile wire
    /// payloads in the tests below.
    #[derive(Serialize)]
    struct WireProbeShape<'a> {
        schema_version: u32,
        state_class: StateClass,
        target_scope: &'a ProbeScope,
        digest: &'a StateDigest,
        prober_did: &'a Did,
        trigger_source: TriggerSource,
        freshness_emitted_at: u64,
        freshness_valid_until: u64,
        requested_response: RequestedResponseClass,
        probe_nonce: [u8; 32],
        probe_hash: Hash,
        signature: Signature,
    }

    fn wire_with_version(version: u32) -> WireProbeShape<'static> {
        // We can't borrow from a function-local because we need 'static. Use
        // leak: the test process exits shortly anyway. (This is test-only.)
        let probe = sample_probe();
        let scope: &'static ProbeScope = Box::leak(Box::new(probe.target_scope.clone()));
        let digest: &'static StateDigest = Box::leak(Box::new(probe.digest.clone()));
        let did: &'static Did = Box::leak(Box::new(probe.prober_did.clone()));
        // The hash is rebuilt under the requested (possibly hostile) version so
        // that the test exercises the version-check path specifically — not the
        // generic "tampered hash" path.
        let recomputed_hash = AntiEntropyProbe::compute_probe_hash(
            version,
            probe.state_class,
            scope,
            digest,
            did,
            probe.trigger_source,
            probe.freshness_emitted_at,
            probe.freshness_valid_until,
            probe.requested_response,
            &probe.probe_nonce,
        );
        WireProbeShape {
            schema_version: version,
            state_class: probe.state_class,
            target_scope: scope,
            digest,
            prober_did: did,
            trigger_source: probe.trigger_source,
            freshness_emitted_at: probe.freshness_emitted_at,
            freshness_valid_until: probe.freshness_valid_until,
            requested_response: probe.requested_response,
            probe_nonce: probe.probe_nonce,
            probe_hash: recomputed_hash,
            signature: probe.signature.clone(),
        }
    }

    #[test]
    fn probe_rejects_future_schema_version_on_json_decode() {
        // A peer sends a probe tagged with a future schema version, with a
        // hash recomputed under that version. The current node must refuse
        // to deserialize it — fail-closed wire-stability.
        let wire = wire_with_version(ANTI_ENTROPY_PROBE_SCHEMA_VERSION + 1);
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<AntiEntropyProbe, _> = serde_json::from_str(&json);
        assert!(parsed.is_err(), "future schema_version must be rejected");
    }

    #[test]
    fn probe_rejects_zero_schema_version_on_json_decode() {
        let wire = wire_with_version(0);
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<AntiEntropyProbe, _> = serde_json::from_str(&json);
        assert!(
            parsed.is_err(),
            "schema_version 0 must be rejected (1 is the lowest supported value)"
        );
    }

    #[test]
    fn probe_rejects_future_schema_version_on_bincode_decode() {
        // Same property must hold for bincode wire data — the kernel's
        // canonical encoding path.
        let wire = wire_with_version(ANTI_ENTROPY_PROBE_SCHEMA_VERSION + 1);
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed: Result<AntiEntropyProbe, _> = bincode::deserialize(&bytes);
        assert!(parsed.is_err());
    }

    #[test]
    fn probe_rejects_zero_schema_version_on_bincode_decode() {
        let wire = wire_with_version(0);
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed: Result<AntiEntropyProbe, _> = bincode::deserialize(&bytes);
        assert!(parsed.is_err());
    }

    #[test]
    fn probe_verify_binding_fails_closed_on_manual_bogus_version() {
        // Even if a probe is constructed in-process with an unsupported
        // schema_version AND a hash recomputed under that bogus version,
        // verify_binding() must still return false. Closes the "manual
        // mutation in Rust" path the deserialization guard cannot cover.
        let mut probe = sample_probe();
        let bogus_version = ANTI_ENTROPY_PROBE_SCHEMA_VERSION + 7;
        probe.schema_version = bogus_version;
        probe.probe_hash = AntiEntropyProbe::compute_probe_hash(
            bogus_version,
            probe.state_class,
            &probe.target_scope,
            &probe.digest,
            &probe.prober_did,
            probe.trigger_source,
            probe.freshness_emitted_at,
            probe.freshness_valid_until,
            probe.requested_response,
            &probe.probe_nonce,
        );
        assert!(
            !probe.verify_binding(),
            "verify_binding() must reject unsupported schema_version even when the hash matches"
        );
    }

    #[test]
    fn probe_supported_version_still_round_trips_after_validation_added() {
        // Regression guard: the legitimate happy path still works.
        let probe = sample_probe();
        let json = serde_json::to_string(&probe).unwrap();
        let restored: AntiEntropyProbe = serde_json::from_str(&json).unwrap();
        assert_eq!(probe, restored);
        assert!(restored.verify_binding());
        assert!(restored.is_supported_schema_version());

        let bytes = bincode::serialize(&probe).unwrap();
        let restored_bin: AntiEntropyProbe = bincode::deserialize(&bytes).unwrap();
        assert_eq!(probe, restored_bin);
        assert!(restored_bin.verify_binding());
    }
}
