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

use crate::effects::EffectOutcome;
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

// ============================================================================
// PeerSyncReport — compare-phase outcome (issue #1852)
//
// Wire-stable record for phase 3 ("Compare") of
// `docs/spec/network-anti-entropy-proof-loops.md`. The responding peer
// compares the incoming `AntiEntropyProbe`'s digest against its own
// state and produces a `PeerSyncReport` whose outcome is one of five
// spec-named categories. The report records the comparison result, the
// digest forms used, the freshness, and the responder's signature. It
// does NOT carry raw state.
//
// Per spec line 218: this record rides inside an existing Stage 5
// `EffectDispatchEvidence` envelope (per
// `docs/spec/effect-dispatch-contract.md`); no new top-level ADR-0026
// receipt class is introduced.
//
// Like the prior records in this module, `PeerSyncReport` is
// self-authenticating: a domain-tagged blake3 binding hash is computed
// at construction over a canonical bincode encoding of the bound fields,
// and `verify_binding()` recomputes and fails closed on unsupported
// schema versions even when the stored hash matches a recomputation
// under the bogus version.
// ============================================================================

/// Schema version for `PeerSyncReport`. Increment on any wire-affecting
/// change to the binding (field set, order, encoding).
pub const PEER_SYNC_REPORT_SCHEMA_VERSION: u32 = 1;

// ----------------------------------------------------------------------------
// UnknownOutOfScopeReason — closed taxonomy for the "unknown / out of scope"
// outcome
// ----------------------------------------------------------------------------

/// Closed reason taxonomy for [`PeerSyncOutcome::UnknownOutOfScope`].
///
/// Matches `docs/spec/network-anti-entropy-proof-loops.md` §"Failure and
/// safety table" rows where the responder cannot evaluate the probe
/// (stale freshness, scope mismatch, insufficient authority, unknown
/// state class). Each row produces a `PeerSyncReport` with the
/// corresponding reason; no `DivergenceEvidence` is produced.
///
/// The kernel does not interpret what each reason *means* in policy
/// terms — it only ensures the recorded reason round-trips deterministically
/// and that no reason outside this closed set can be expressed on the wire.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum UnknownOutOfScopeReason {
    /// The probe's `freshness_valid_until` has elapsed before the
    /// responder evaluated it. The report records this outcome rather
    /// than classifying divergence over stale state.
    StaleProbe,
    /// The probe's target scope does not overlap the responder's
    /// adopted scope for this state class.
    ScopeMismatch,
    /// The responder cannot evaluate the probe within the policy /
    /// disclosure scope the prober's authority permits.
    InsufficientAuthority,
    /// The probe names a state class the responder does not recognize
    /// (e.g., a forward-direction extension not yet adopted). Tracked
    /// as a follow-up to extend the closed `StateClass` set rather
    /// than as a divergence.
    UnknownStateClass,
}

// ----------------------------------------------------------------------------
// PeerSyncOutcome — closed 5-variant taxonomy matching spec phase 3
// ----------------------------------------------------------------------------

/// Closed taxonomy of compare-phase outcomes.
///
/// Matches `docs/spec/network-anti-entropy-proof-loops.md` §"Compare"
/// verbatim — the five outcomes the responder may record after
/// comparing the incoming probe's digest against its own state. The
/// "local" / "remote" framing is the responder's: "local" is the
/// responder's index; "remote" is the prober's (the peer whose probe
/// is being answered).
///
/// - `Matching` — both peers' digests for this state class agree at the
///   boundary.
/// - `MissingOnLocal` — the **responder's local index is missing**
///   entries the prober has. Equivalent to the spec's `missing on
///   local`: "peer has entries the responder does not." The responder
///   may request those entries from the prober, subject to scope and
///   disclosure rules.
/// - `MissingOnRemote` — the **prober's remote index is missing**
///   entries the responder has. Equivalent to the spec's `missing on
///   remote`: "responder has entries the peer does not." The responder
///   may volunteer those entries to the prober, subject to scope and
///   disclosure rules.
/// - `Divergent` — both peers claim entries at the same address (e.g.,
///   same Merkle root path) with different contents; requires
///   classification in phase 4 (`DivergenceEvidence`).
/// - `UnknownOutOfScope` — responder cannot evaluate (see
///   [`UnknownOutOfScopeReason`] for the four spec-named cases).
///
/// The kernel does not classify what each non-matching outcome *means*
/// for repair authority — that is the policy oracle's job in phase 4.
/// The kernel only ensures the recorded outcome round-trips
/// deterministically and that no outcome outside this closed set can
/// be expressed on the wire.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PeerSyncOutcome {
    /// Both peers' digests for this state class agree at the boundary.
    Matching,
    /// The responder's local index is missing entries the prober has
    /// (spec: "peer has entries the responder does not").
    MissingOnLocal,
    /// The prober's remote index is missing entries the responder has
    /// (spec: "responder has entries the peer does not").
    MissingOnRemote,
    /// Both peers claim entries at the same address with different
    /// contents. Requires classification in phase 4.
    Divergent,
    /// Responder cannot evaluate the probe; see [`UnknownOutOfScopeReason`].
    UnknownOutOfScope(UnknownOutOfScopeReason),
}

impl PeerSyncOutcome {
    /// `true` iff this outcome may legally carry a
    /// [`PeerSyncReport::divergence_evidence_hash`] cross-link.
    ///
    /// `Matching` and `UnknownOutOfScope` never produce
    /// `DivergenceEvidence` (per spec §"Compare" and §"Failure and
    /// safety table"); the three non-matching outcomes may.
    pub fn may_carry_divergence_evidence(self) -> bool {
        matches!(
            self,
            PeerSyncOutcome::MissingOnLocal
                | PeerSyncOutcome::MissingOnRemote
                | PeerSyncOutcome::Divergent,
        )
    }

    /// Stable lowercase string label for logs and audit rows.
    ///
    /// This is **not** the serialized wire encoding — serde uses the
    /// enum's externally-tagged representation, which renders unit
    /// variants as quoted strings and the `UnknownOutOfScope` tuple
    /// variant as a single-key object. `as_str()` instead returns a
    /// flat label suitable for log lines and audit-row keys; it is
    /// stable across releases but does not round-trip through serde.
    pub fn as_str(self) -> &'static str {
        match self {
            PeerSyncOutcome::Matching => "matching",
            PeerSyncOutcome::MissingOnLocal => "missing_on_local",
            PeerSyncOutcome::MissingOnRemote => "missing_on_remote",
            PeerSyncOutcome::Divergent => "divergent",
            PeerSyncOutcome::UnknownOutOfScope(_) => "unknown_out_of_scope",
        }
    }
}

// ----------------------------------------------------------------------------
// PeerSyncReportError — structural validation
// ----------------------------------------------------------------------------

/// Structural validation errors a `PeerSyncReport` can produce at
/// construction or wire-deserialization time.
///
/// Kernel-level structural rules only: they catch outcome / divergence-
/// link combinations that are internally inconsistent per the spec, not
/// runtime errors. App-level policy violations are not enforced here.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PeerSyncReportError {
    /// `schema_version` did not match [`PEER_SYNC_REPORT_SCHEMA_VERSION`].
    #[error("unsupported PeerSyncReport schema_version: got {got}, supported {supported}")]
    UnsupportedSchemaVersion {
        /// The version observed on the wire / at construction.
        got: u32,
        /// The version this build supports.
        supported: u32,
    },
    /// Outcome is `Matching` or `UnknownOutOfScope` but a
    /// `divergence_evidence_hash` was set. These outcomes never produce
    /// `DivergenceEvidence`; a cross-link to one would be a lie.
    #[error(
        "PeerSyncReport with outcome={outcome} must not carry divergence_evidence_hash \
         (Matching / UnknownOutOfScope outcomes do not route to classification)"
    )]
    DivergenceEvidenceHashNotAllowed {
        /// The outcome that incorrectly carried a divergence-evidence link.
        outcome: &'static str,
    },
}

impl From<PeerSyncReportError> for String {
    fn from(err: PeerSyncReportError) -> Self {
        err.to_string()
    }
}

// ----------------------------------------------------------------------------
// PeerSyncReport
// ----------------------------------------------------------------------------

/// The compare-phase outcome record produced in phase 3 of
/// `docs/spec/network-anti-entropy-proof-loops.md`.
///
/// Cross-links to the originating [`AntiEntropyProbe`] by hash, records
/// the responder's [`PeerSyncOutcome`], the state class probed, the
/// scope, the prober and responder DIDs, the responder's local
/// [`StateDigest`] (the digest form they compared against — never raw
/// state), the observed / freshness timestamps, an optional cross-link
/// to a downstream [`DivergenceEvidence`] when classification produced
/// one, a private-content implication flag, a 32-byte nonce, and a
/// blake3 binding hash.
///
/// # The report is not the sync
///
/// A `PeerSyncReport` records what a responder *observed* when
/// comparing digests; the kernel does not execute peer sync (that is a
/// runtime / gossip-layer concern), does not classify outcomes into
/// divergence classes (that is the policy oracle's job in phase 4),
/// and does not emit reports autonomously. The report exists so an
/// auditor can chase the chain from probe → compare → classify →
/// plan → apply → receipt without trusting any single peer.
///
/// # Privacy
///
/// `PeerSyncReport` MUST NOT carry raw state. The `local_digest` field
/// carries one of four bounded [`StateDigest`] projections (Bloom over
/// content hashes, Merkle root, vector clock, or short list of
/// content-addressed hashes); none reveal the bodies they reference.
/// For comparisons that touch private state, `private_content_implication`
/// is set; renderers MUST gate technical detail behind the disclosure
/// scope of the affected state.
///
/// # Self-authentication
///
/// `report_hash` is a blake3 binding under
/// `DOMAIN_TAG = b"icn:peer-sync-report:v1"`, length-prefixed, over a
/// canonical bincode encoding of all bound fields (excluding
/// `report_hash` and `signature`). [`Self::verify_binding`] re-checks
/// both the hash and the schema version; either alone is insufficient.
/// Deserialization rejects unsupported schema versions AND structurally
/// inconsistent outcome / divergence-link combinations via
/// `#[serde(try_from = ...)]`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "RawPeerSyncReport")]
pub struct PeerSyncReport {
    /// Wire schema version. See [`PEER_SYNC_REPORT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// `probe_hash` of the originating [`AntiEntropyProbe`] this
    /// report responds to.
    pub probe_hash: Hash,
    /// The compare-phase outcome. One of five spec-named categories.
    pub sync_outcome: PeerSyncOutcome,
    /// State class being compared. MUST match the probe's
    /// `state_class`; the kernel does not re-verify because it does
    /// not have the probe in hand, but the responder is responsible
    /// for not mis-reporting the field.
    pub state_class: StateClass,
    /// Target scope. MUST match the probe's `target_scope`.
    pub scope: ProbeScope,
    /// DID of the responder — the peer that performed the comparison
    /// and signed the report.
    pub reporter_did: Did,
    /// DID of the prober — the peer whose probe this report responds to.
    pub counterparty_did: Did,
    /// The responder's local digest at the comparison instant. Bounded
    /// projection (never raw state).
    pub local_digest: StateDigest,
    /// Responder's clock at construction (Unix seconds).
    pub observed_at: u64,
    /// Timestamp beyond which the report is stale (Unix seconds).
    pub freshness_valid_until: u64,
    /// Cross-link to a downstream [`DivergenceEvidence::evidence_hash`]
    /// when phase 4 classification produced one. MUST be `None` for
    /// outcomes that cannot route to classification (`Matching`,
    /// `UnknownOutOfScope`).
    pub divergence_evidence_hash: Option<Hash>,
    /// `true` if the comparison touched private state. Renderers MUST
    /// gate technical detail behind the disclosure scope of the
    /// affected state.
    pub private_content_implication: bool,
    /// 32-byte random nonce. Two reports with otherwise-identical
    /// fields produce distinct `report_hash`es.
    pub report_nonce: [u8; 32],
    /// blake3 binding hash over all bound fields, computed at construction.
    pub report_hash: Hash,
    /// Responder signature (empty until signed by a higher layer).
    pub signature: Signature,
}

/// Raw wire form for [`PeerSyncReport`]. Validated into the public
/// type via [`TryFrom`] (fails closed on unsupported `schema_version`
/// and on structurally inconsistent outcome / divergence-link
/// combinations).
#[derive(Deserialize)]
struct RawPeerSyncReport {
    schema_version: u32,
    probe_hash: Hash,
    sync_outcome: PeerSyncOutcome,
    state_class: StateClass,
    scope: ProbeScope,
    reporter_did: Did,
    counterparty_did: Did,
    local_digest: StateDigest,
    observed_at: u64,
    freshness_valid_until: u64,
    divergence_evidence_hash: Option<Hash>,
    private_content_implication: bool,
    report_nonce: [u8; 32],
    report_hash: Hash,
    signature: Signature,
}

impl TryFrom<RawPeerSyncReport> for PeerSyncReport {
    type Error = String;

    fn try_from(raw: RawPeerSyncReport) -> Result<Self, Self::Error> {
        if raw.schema_version != PEER_SYNC_REPORT_SCHEMA_VERSION {
            return Err(PeerSyncReportError::UnsupportedSchemaVersion {
                got: raw.schema_version,
                supported: PEER_SYNC_REPORT_SCHEMA_VERSION,
            }
            .to_string());
        }
        PeerSyncReport::validate_outcome_link_consistency(
            raw.sync_outcome,
            raw.divergence_evidence_hash.as_ref(),
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            schema_version: raw.schema_version,
            probe_hash: raw.probe_hash,
            sync_outcome: raw.sync_outcome,
            state_class: raw.state_class,
            scope: raw.scope,
            reporter_did: raw.reporter_did,
            counterparty_did: raw.counterparty_did,
            local_digest: raw.local_digest,
            observed_at: raw.observed_at,
            freshness_valid_until: raw.freshness_valid_until,
            divergence_evidence_hash: raw.divergence_evidence_hash,
            private_content_implication: raw.private_content_implication,
            report_nonce: raw.report_nonce,
            report_hash: raw.report_hash,
            signature: raw.signature,
        })
    }
}

/// Canonical binding fields for `PeerSyncReport::report_hash`.
///
/// Excludes `report_hash` (the output) and `signature` (filled after
/// binding). Bincode-serialized in a stable order. Any new bound field
/// added here requires bumping [`PEER_SYNC_REPORT_SCHEMA_VERSION`].
#[derive(Serialize)]
struct PeerSyncReportBinding<'a> {
    schema_version: u32,
    probe_hash: Hash,
    sync_outcome: PeerSyncOutcome,
    state_class: StateClass,
    scope: &'a ProbeScope,
    reporter_did: &'a Did,
    counterparty_did: &'a Did,
    local_digest: &'a StateDigest,
    observed_at: u64,
    freshness_valid_until: u64,
    divergence_evidence_hash: Option<Hash>,
    private_content_implication: bool,
    report_nonce: [u8; 32],
}

impl PeerSyncReport {
    /// Domain-separation tag for `report_hash`. Distinct from
    /// [`AntiEntropyProbe::DOMAIN_TAG`], [`DivergenceEvidence::DOMAIN_TAG`],
    /// [`RepairPlan::DOMAIN_TAG`], [`RepairReceipt::DOMAIN_TAG`], and
    /// [`ArtifactReceipt::DOMAIN_TAG`].
    pub const DOMAIN_TAG: &'static [u8] = b"icn:peer-sync-report:v1";

    /// Construct a new report with computed `report_hash` and empty
    /// signature.
    ///
    /// `schema_version` is set to [`PEER_SYNC_REPORT_SCHEMA_VERSION`].
    /// Returns [`PeerSyncReportError`] if the outcome / divergence-link
    /// combination is structurally inconsistent (see
    /// [`Self::validate_outcome_link_consistency`]). The caller must
    /// sign the report at a higher layer before emission.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        probe_hash: Hash,
        sync_outcome: PeerSyncOutcome,
        state_class: StateClass,
        scope: ProbeScope,
        reporter_did: Did,
        counterparty_did: Did,
        local_digest: StateDigest,
        observed_at: u64,
        freshness_valid_until: u64,
        divergence_evidence_hash: Option<Hash>,
        private_content_implication: bool,
        report_nonce: [u8; 32],
    ) -> Result<Self, PeerSyncReportError> {
        Self::validate_outcome_link_consistency(sync_outcome, divergence_evidence_hash.as_ref())?;
        let report_hash = Self::compute_report_hash(
            PEER_SYNC_REPORT_SCHEMA_VERSION,
            probe_hash,
            sync_outcome,
            state_class,
            &scope,
            &reporter_did,
            &counterparty_did,
            &local_digest,
            observed_at,
            freshness_valid_until,
            divergence_evidence_hash,
            private_content_implication,
            &report_nonce,
        );
        Ok(Self {
            schema_version: PEER_SYNC_REPORT_SCHEMA_VERSION,
            probe_hash,
            sync_outcome,
            state_class,
            scope,
            reporter_did,
            counterparty_did,
            local_digest,
            observed_at,
            freshness_valid_until,
            divergence_evidence_hash,
            private_content_implication,
            report_nonce,
            report_hash,
            signature: Signature::new(Vec::new()),
        })
    }

    /// Validate the structural outcome / divergence-link invariant.
    ///
    /// Returns `Ok(())` if the combination is consistent, else an
    /// error. Kernel-level structural rule only:
    ///
    /// - `Matching` and `UnknownOutOfScope` MUST NOT carry a
    ///   `divergence_evidence_hash` (these outcomes never route to
    ///   classification).
    /// - `MissingOnLocal`, `MissingOnRemote`, `Divergent` MAY carry a
    ///   `divergence_evidence_hash` (classification happened) or `None`
    ///   (report alone, classification pending).
    pub fn validate_outcome_link_consistency(
        sync_outcome: PeerSyncOutcome,
        divergence_evidence_hash: Option<&Hash>,
    ) -> Result<(), PeerSyncReportError> {
        if !sync_outcome.may_carry_divergence_evidence() && divergence_evidence_hash.is_some() {
            return Err(PeerSyncReportError::DivergenceEvidenceHashNotAllowed {
                outcome: sync_outcome.as_str(),
            });
        }
        Ok(())
    }

    /// Compute the binding hash from the significant fields.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_report_hash(
        schema_version: u32,
        probe_hash: Hash,
        sync_outcome: PeerSyncOutcome,
        state_class: StateClass,
        scope: &ProbeScope,
        reporter_did: &Did,
        counterparty_did: &Did,
        local_digest: &StateDigest,
        observed_at: u64,
        freshness_valid_until: u64,
        divergence_evidence_hash: Option<Hash>,
        private_content_implication: bool,
        report_nonce: &[u8; 32],
    ) -> Hash {
        let binding = PeerSyncReportBinding {
            schema_version,
            probe_hash,
            sync_outcome,
            state_class,
            scope,
            reporter_did,
            counterparty_did,
            local_digest,
            observed_at,
            freshness_valid_until,
            divergence_evidence_hash,
            private_content_implication,
            report_nonce: *report_nonce,
        };
        let payload = bincode::serialize(&binding)
            .expect("PeerSyncReportBinding serialization is infallible");
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        *hasher.finalize().as_bytes()
    }

    /// `true` iff `schema_version == PEER_SYNC_REPORT_SCHEMA_VERSION`.
    pub fn is_supported_schema_version(&self) -> bool {
        self.schema_version == PEER_SYNC_REPORT_SCHEMA_VERSION
    }

    /// Verify the stored `report_hash` and `schema_version`.
    ///
    /// Returns `true` only if both hold. Fails closed on unsupported
    /// versions even when the stored hash matches a recomputation under
    /// that version. Does NOT verify the signature — that is a
    /// higher-layer concern.
    pub fn verify_binding(&self) -> bool {
        if !self.is_supported_schema_version() {
            return false;
        }
        let recomputed = Self::compute_report_hash(
            self.schema_version,
            self.probe_hash,
            self.sync_outcome,
            self.state_class,
            &self.scope,
            &self.reporter_did,
            &self.counterparty_did,
            &self.local_digest,
            self.observed_at,
            self.freshness_valid_until,
            self.divergence_evidence_hash,
            self.private_content_implication,
            &self.report_nonce,
        );
        self.report_hash == recomputed
    }

    /// `true` if `now_unix_seconds <= freshness_valid_until`.
    pub fn is_fresh(&self, now_unix_seconds: u64) -> bool {
        now_unix_seconds <= self.freshness_valid_until
    }
}

// ============================================================================
// SyncDegradedStatus — policy-derived rendering directive (issue #1856)
//
// Wire-stable record for the spec's boundary rule 5 ("No treating
// degraded sync as healthy") in
// `docs/spec/network-anti-entropy-proof-loops.md`. When a
// `PeerSyncReport` reports `Divergent` or `MissingOnLocal` and the
// divergence is not repaired within the freshness window, the surface
// MUST render a degraded status; the cockpit MUST NOT show healthy and
// the member shell MUST NOT show "Synced." `SyncDegradedStatus` is the
// kernel-side record of that policy decision.
//
// Unlike the other proof records in this module, `SyncDegradedStatus`
// is not part of the probe → compare → classify → plan → apply chain.
// It is a *parallel* policy-derived status that the steward and member
// rendering surfaces consume to honor boundary rule 5. The kernel does
// not classify when to emit one — that is the policy oracle's job —
// but it records the categorical degradation level and the cross-link
// to the originating `PeerSyncReport`.
//
// Per spec line 218: this record rides inside an existing Stage 5
// `EffectDispatchEvidence` envelope. No new top-level ADR-0026
// receipt class is introduced.
// ============================================================================

/// Schema version for `SyncDegradedStatus`. Increment on any
/// wire-affecting change to the binding.
pub const SYNC_DEGRADED_STATUS_SCHEMA_VERSION: u32 = 1;

// ----------------------------------------------------------------------------
// DegradationLevel — closed two-variant taxonomy
// ----------------------------------------------------------------------------

/// Closed degradation severity taxonomy aligned to the spec's
/// member-shell vocabulary.
///
/// Per `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell
/// surface":
///
/// - `WithinGraceWindow` — the degradation is fresh within policy.
///   The member shell renders one of: "Sync delayed", "Action paused
///   until records sync".
/// - `BeyondGraceWindow` — the degradation has persisted past the
///   policy's grace window. The member shell renders "Sync delayed /
///   degraded" — honest signaling that the institution is not
///   currently in a healthy sync state.
///
/// The kernel does not interpret what each level *means* for action
/// authority — that is the policy oracle's concern. The kernel only
/// ensures the recorded level round-trips deterministically and that
/// no level outside this closed set can be expressed on the wire.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DegradationLevel {
    /// Degradation is fresh; within the policy's grace window.
    WithinGraceWindow,
    /// Degradation has persisted past the policy's grace window.
    BeyondGraceWindow,
}

impl DegradationLevel {
    /// Stable lowercase string label for logs and audit rows. Not the
    /// serialized wire encoding.
    pub fn as_str(self) -> &'static str {
        match self {
            DegradationLevel::WithinGraceWindow => "within_grace_window",
            DegradationLevel::BeyondGraceWindow => "beyond_grace_window",
        }
    }
}

// ----------------------------------------------------------------------------
// SyncDegradedStatusError — structural validation
// ----------------------------------------------------------------------------

/// Structural validation errors a `SyncDegradedStatus` can produce at
/// construction or wire-deserialization time.
///
/// Kernel-level structural rules only.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SyncDegradedStatusError {
    /// `schema_version` did not match
    /// [`SYNC_DEGRADED_STATUS_SCHEMA_VERSION`].
    #[error("unsupported SyncDegradedStatus schema_version: got {got}, supported {supported}")]
    UnsupportedSchemaVersion {
        /// The version observed on the wire / at construction.
        got: u32,
        /// The version this build supports.
        supported: u32,
    },
    /// `triggered_by_outcome` is not one of the outcomes that the
    /// spec's boundary rule 5 permits to trigger a degraded status.
    /// Only `MissingOnLocal` and `Divergent` may trigger degradation;
    /// `Matching`, `MissingOnRemote`, and `UnknownOutOfScope` cannot.
    #[error(
        "SyncDegradedStatus triggered_by_outcome={outcome} is not eligible to trigger \
         degradation (spec boundary rule 5: only `divergent` and `missing on local` qualify)"
    )]
    OutcomeNotEligibleForDegradation {
        /// The ineligible outcome.
        outcome: &'static str,
    },
    /// `degraded_since` is later than `freshness_valid_until`. The
    /// freshness window must end at or after the degradation began.
    #[error(
        "SyncDegradedStatus degraded_since={degraded_since} is after \
         freshness_valid_until={freshness_valid_until}"
    )]
    DegradedSinceAfterFreshness {
        /// The recorded degradation start.
        degraded_since: u64,
        /// The recorded freshness expiry.
        freshness_valid_until: u64,
    },
}

impl From<SyncDegradedStatusError> for String {
    fn from(err: SyncDegradedStatusError) -> Self {
        err.to_string()
    }
}

// ----------------------------------------------------------------------------
// SyncDegradedStatus
// ----------------------------------------------------------------------------

/// The policy-derived rendering directive for a `PeerSyncReport`
/// outcome that is not repaired within policy.
///
/// Per `docs/spec/network-anti-entropy-proof-loops.md` §"Boundary
/// rules" rule 5: when a `PeerSyncReport` is `Divergent` or
/// `MissingOnLocal` and the divergence is not yet repaired within the
/// freshness window, the steward / member-facing surface MUST render
/// `SyncDegradedStatus`. The cockpit MUST NOT show healthy; the
/// member shell MUST NOT show "Synced."
///
/// `SyncDegradedStatus` carries:
///
/// - A mandatory cross-link to the originating `PeerSyncReport` via
///   `peer_sync_report_hash`.
/// - A copy of the `triggered_by_outcome` (structurally restricted to
///   `MissingOnLocal` | `Divergent` per boundary rule 5).
/// - The affected state class and scope (which must mirror the
///   originating report's; the kernel does not re-verify because it
///   does not have the report in hand).
/// - The reporter DID — the policy oracle / surface producer that
///   recorded the status.
/// - A `DegradationLevel` (within-grace vs beyond-grace per spec
///   §"Member shell surface" vocabulary).
/// - A `degraded_since` timestamp.
/// - A `freshness_valid_until` timestamp (must be ≥ `degraded_since`).
/// - An optional cross-link to a downstream `DivergenceEvidence` via
///   `divergence_evidence_hash` (`None` when classification has not
///   yet run; `Some(_)` when it has).
/// - A `private_content_implication` flag.
///
/// # The status is not the comparison
///
/// `SyncDegradedStatus` records that a comparison outcome has
/// persisted past policy grace into a degraded state. It does NOT
/// re-run the comparison, classify divergence, or plan repair. It is
/// purely a rendering directive carrying the policy decision "this
/// surface MUST render degraded."
///
/// # Privacy
///
/// `SyncDegradedStatus` does not carry raw state — only opaque
/// cross-link hashes and the categorical degradation level. For
/// statuses derived from comparisons over private state,
/// `private_content_implication` is set; renderers MUST gate
/// technical detail behind the disclosure scope of the affected
/// state.
///
/// # Self-authentication
///
/// `status_hash` is a blake3 binding under
/// `DOMAIN_TAG = b"icn:sync-degraded-status:v1"`, length-prefixed,
/// over a canonical bincode encoding of all bound fields (excluding
/// `status_hash` and `signature`). [`Self::verify_binding`] re-checks
/// both the hash and the schema version; either alone is insufficient.
/// Deserialization rejects unsupported schema versions AND
/// structurally invalid `triggered_by_outcome` values AND
/// `degraded_since` > `freshness_valid_until` via
/// `#[serde(try_from = ...)]`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "RawSyncDegradedStatus")]
pub struct SyncDegradedStatus {
    /// Wire schema version. See [`SYNC_DEGRADED_STATUS_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// `report_hash` of the originating [`PeerSyncReport`] this status
    /// derives from.
    pub peer_sync_report_hash: Hash,
    /// The compare-phase outcome that triggered this degraded status.
    /// MUST be `MissingOnLocal` or `Divergent` per spec boundary rule 5.
    pub triggered_by_outcome: PeerSyncOutcome,
    /// State class affected by the degradation. MUST mirror the
    /// originating report's `state_class`.
    pub state_class: StateClass,
    /// Scope of the degradation. MUST mirror the originating report's
    /// `scope`.
    pub scope: ProbeScope,
    /// DID of the policy oracle / surface producer that recorded this
    /// status.
    pub reporter_did: Did,
    /// Categorical degradation severity (within-grace vs beyond-grace).
    pub degradation_level: DegradationLevel,
    /// Unix seconds when the degradation began (typically the
    /// originating report's `observed_at`).
    pub degraded_since: u64,
    /// Unix seconds beyond which this status is stale. MUST be
    /// `>= degraded_since`.
    pub freshness_valid_until: u64,
    /// Optional cross-link to a downstream
    /// [`DivergenceEvidence::evidence_hash`] when classification has
    /// produced one. `None` before classification, `Some(_)` after.
    pub divergence_evidence_hash: Option<Hash>,
    /// `true` if the degraded comparison touched private state.
    /// Renderers MUST gate technical detail behind the disclosure
    /// scope of the affected state.
    pub private_content_implication: bool,
    /// 32-byte random nonce.
    pub status_nonce: [u8; 32],
    /// blake3 binding hash over all bound fields.
    pub status_hash: Hash,
    /// Reporter signature (empty until signed by a higher layer).
    pub signature: Signature,
}

/// Raw wire form for [`SyncDegradedStatus`]. Validated into the
/// public type via [`TryFrom`] (fails closed on unsupported
/// `schema_version`, ineligible `triggered_by_outcome`, and
/// `degraded_since` > `freshness_valid_until`).
#[derive(Deserialize)]
struct RawSyncDegradedStatus {
    schema_version: u32,
    peer_sync_report_hash: Hash,
    triggered_by_outcome: PeerSyncOutcome,
    state_class: StateClass,
    scope: ProbeScope,
    reporter_did: Did,
    degradation_level: DegradationLevel,
    degraded_since: u64,
    freshness_valid_until: u64,
    divergence_evidence_hash: Option<Hash>,
    private_content_implication: bool,
    status_nonce: [u8; 32],
    status_hash: Hash,
    signature: Signature,
}

impl TryFrom<RawSyncDegradedStatus> for SyncDegradedStatus {
    type Error = String;

    fn try_from(raw: RawSyncDegradedStatus) -> Result<Self, Self::Error> {
        if raw.schema_version != SYNC_DEGRADED_STATUS_SCHEMA_VERSION {
            return Err(SyncDegradedStatusError::UnsupportedSchemaVersion {
                got: raw.schema_version,
                supported: SYNC_DEGRADED_STATUS_SCHEMA_VERSION,
            }
            .to_string());
        }
        SyncDegradedStatus::validate_structural_invariants(
            raw.triggered_by_outcome,
            raw.degraded_since,
            raw.freshness_valid_until,
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            schema_version: raw.schema_version,
            peer_sync_report_hash: raw.peer_sync_report_hash,
            triggered_by_outcome: raw.triggered_by_outcome,
            state_class: raw.state_class,
            scope: raw.scope,
            reporter_did: raw.reporter_did,
            degradation_level: raw.degradation_level,
            degraded_since: raw.degraded_since,
            freshness_valid_until: raw.freshness_valid_until,
            divergence_evidence_hash: raw.divergence_evidence_hash,
            private_content_implication: raw.private_content_implication,
            status_nonce: raw.status_nonce,
            status_hash: raw.status_hash,
            signature: raw.signature,
        })
    }
}

/// Canonical binding fields for `SyncDegradedStatus::status_hash`.
///
/// Excludes `status_hash` (the output) and `signature` (filled after
/// binding). Any new bound field added here requires bumping
/// [`SYNC_DEGRADED_STATUS_SCHEMA_VERSION`].
#[derive(Serialize)]
struct SyncDegradedStatusBinding<'a> {
    schema_version: u32,
    peer_sync_report_hash: Hash,
    triggered_by_outcome: PeerSyncOutcome,
    state_class: StateClass,
    scope: &'a ProbeScope,
    reporter_did: &'a Did,
    degradation_level: DegradationLevel,
    degraded_since: u64,
    freshness_valid_until: u64,
    divergence_evidence_hash: Option<Hash>,
    private_content_implication: bool,
    status_nonce: [u8; 32],
}

impl SyncDegradedStatus {
    /// Domain-separation tag for `status_hash`. Distinct from
    /// [`PeerSyncReport::DOMAIN_TAG`], [`AntiEntropyProbe::DOMAIN_TAG`],
    /// [`DivergenceEvidence::DOMAIN_TAG`], [`RepairPlan::DOMAIN_TAG`],
    /// [`RepairReceipt::DOMAIN_TAG`], and [`ArtifactReceipt::DOMAIN_TAG`].
    pub const DOMAIN_TAG: &'static [u8] = b"icn:sync-degraded-status:v1";

    /// Construct a new status with computed `status_hash` and empty
    /// signature.
    ///
    /// Returns [`SyncDegradedStatusError`] if structural invariants
    /// are violated (see [`Self::validate_structural_invariants`]).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        peer_sync_report_hash: Hash,
        triggered_by_outcome: PeerSyncOutcome,
        state_class: StateClass,
        scope: ProbeScope,
        reporter_did: Did,
        degradation_level: DegradationLevel,
        degraded_since: u64,
        freshness_valid_until: u64,
        divergence_evidence_hash: Option<Hash>,
        private_content_implication: bool,
        status_nonce: [u8; 32],
    ) -> Result<Self, SyncDegradedStatusError> {
        Self::validate_structural_invariants(
            triggered_by_outcome,
            degraded_since,
            freshness_valid_until,
        )?;
        let status_hash = Self::compute_status_hash(
            SYNC_DEGRADED_STATUS_SCHEMA_VERSION,
            peer_sync_report_hash,
            triggered_by_outcome,
            state_class,
            &scope,
            &reporter_did,
            degradation_level,
            degraded_since,
            freshness_valid_until,
            divergence_evidence_hash,
            private_content_implication,
            &status_nonce,
        );
        Ok(Self {
            schema_version: SYNC_DEGRADED_STATUS_SCHEMA_VERSION,
            peer_sync_report_hash,
            triggered_by_outcome,
            state_class,
            scope,
            reporter_did,
            degradation_level,
            degraded_since,
            freshness_valid_until,
            divergence_evidence_hash,
            private_content_implication,
            status_nonce,
            status_hash,
            signature: Signature::new(Vec::new()),
        })
    }

    /// Validate the structural invariants:
    ///
    /// - `triggered_by_outcome` MUST be `MissingOnLocal` or
    ///   `Divergent` per spec boundary rule 5.
    /// - `degraded_since` MUST be `<= freshness_valid_until`.
    pub fn validate_structural_invariants(
        triggered_by_outcome: PeerSyncOutcome,
        degraded_since: u64,
        freshness_valid_until: u64,
    ) -> Result<(), SyncDegradedStatusError> {
        if !matches!(
            triggered_by_outcome,
            PeerSyncOutcome::MissingOnLocal | PeerSyncOutcome::Divergent
        ) {
            return Err(SyncDegradedStatusError::OutcomeNotEligibleForDegradation {
                outcome: triggered_by_outcome.as_str(),
            });
        }
        if degraded_since > freshness_valid_until {
            return Err(SyncDegradedStatusError::DegradedSinceAfterFreshness {
                degraded_since,
                freshness_valid_until,
            });
        }
        Ok(())
    }

    /// Compute the binding hash from the significant fields.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_status_hash(
        schema_version: u32,
        peer_sync_report_hash: Hash,
        triggered_by_outcome: PeerSyncOutcome,
        state_class: StateClass,
        scope: &ProbeScope,
        reporter_did: &Did,
        degradation_level: DegradationLevel,
        degraded_since: u64,
        freshness_valid_until: u64,
        divergence_evidence_hash: Option<Hash>,
        private_content_implication: bool,
        status_nonce: &[u8; 32],
    ) -> Hash {
        let binding = SyncDegradedStatusBinding {
            schema_version,
            peer_sync_report_hash,
            triggered_by_outcome,
            state_class,
            scope,
            reporter_did,
            degradation_level,
            degraded_since,
            freshness_valid_until,
            divergence_evidence_hash,
            private_content_implication,
            status_nonce: *status_nonce,
        };
        let payload = bincode::serialize(&binding)
            .expect("SyncDegradedStatusBinding serialization is infallible");
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        *hasher.finalize().as_bytes()
    }

    /// `true` iff `schema_version == SYNC_DEGRADED_STATUS_SCHEMA_VERSION`.
    pub fn is_supported_schema_version(&self) -> bool {
        self.schema_version == SYNC_DEGRADED_STATUS_SCHEMA_VERSION
    }

    /// Verify the stored `status_hash` and `schema_version`.
    ///
    /// Returns `true` only if both hold. Fails closed on unsupported
    /// versions even when the stored hash matches a recomputation
    /// under that version.
    pub fn verify_binding(&self) -> bool {
        if !self.is_supported_schema_version() {
            return false;
        }
        let recomputed = Self::compute_status_hash(
            self.schema_version,
            self.peer_sync_report_hash,
            self.triggered_by_outcome,
            self.state_class,
            &self.scope,
            &self.reporter_did,
            self.degradation_level,
            self.degraded_since,
            self.freshness_valid_until,
            self.divergence_evidence_hash,
            self.private_content_implication,
            &self.status_nonce,
        );
        self.status_hash == recomputed
    }

    /// `true` if `now_unix_seconds <= freshness_valid_until`.
    pub fn is_fresh(&self, now_unix_seconds: u64) -> bool {
        now_unix_seconds <= self.freshness_valid_until
    }
}

// ============================================================================
// FederationSyncWindow + QuorumSyncCheck — federation-scope proof
// primitives (issue #1860)
//
// Wire-stable records for spec §"Boundary rules" 6 + 7. A
// `QuorumSyncCheck` proves that a quorum of named federation peers
// exchanged matching `StateDigest`s within a stated freshness window
// for a stated state class. A `FederationSyncWindow` is the policy
// parameter the check's freshness is measured against, named per
// state class (e.g., settlement records may require a stricter window
// than artifact-registry metadata).
//
// Boundary rule 6: federation- and commons-scope placement requires a
// fresh `QuorumSyncCheck` within the relevant `FederationSyncWindow`.
// Boundary rule 7: settlement finality requires a fresh
// `QuorumSyncCheck` over the relevant state classes.
//
// Per spec line 220: these records ride inside an existing Stage 5
// `EffectDispatchEvidence` envelope. No new top-level ADR-0026
// receipt class is introduced.
// ============================================================================

/// Schema version for `FederationSyncWindow`. Increment on any
/// wire-affecting change to the binding.
pub const FEDERATION_SYNC_WINDOW_SCHEMA_VERSION: u32 = 1;

/// Schema version for `QuorumSyncCheck`. Increment on any
/// wire-affecting change to the binding.
pub const QUORUM_SYNC_CHECK_SCHEMA_VERSION: u32 = 1;

// ----------------------------------------------------------------------------
// FederationSyncWindow
// ----------------------------------------------------------------------------

/// Structural validation errors a `FederationSyncWindow` can produce.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FederationSyncWindowError {
    /// `schema_version` did not match
    /// [`FEDERATION_SYNC_WINDOW_SCHEMA_VERSION`].
    #[error("unsupported FederationSyncWindow schema_version: got {got}, supported {supported}")]
    UnsupportedSchemaVersion {
        /// The version observed on the wire / at construction.
        got: u32,
        /// The version this build supports.
        supported: u32,
    },
    /// `window_duration_secs` was zero. A zero-duration window is not
    /// a meaningful freshness bound.
    #[error("FederationSyncWindow window_duration_secs must be > 0")]
    ZeroWindowDuration,
}

impl From<FederationSyncWindowError> for String {
    fn from(err: FederationSyncWindowError) -> Self {
        err.to_string()
    }
}

/// Policy-defined freshness window for a `QuorumSyncCheck`, named per
/// state class.
///
/// Per spec line 235: "the freshness window the domain's policy
/// requires for `QuorumSyncCheck` to count as fresh; named per state
/// class (e.g., settlement records may require a stricter window than
/// artifact-registry metadata)."
///
/// The kernel does not decide what window a domain's policy requires
/// — that is the policy oracle's job. The kernel only records the
/// per-state-class policy decision so a downstream `QuorumSyncCheck`
/// can be measured against it.
///
/// # Self-authentication
///
/// `window_hash` is a blake3 binding under
/// `DOMAIN_TAG = b"icn:federation-sync-window:v1"`, length-prefixed,
/// over a canonical bincode encoding of the bound fields. Deserialization
/// rejects unsupported schema versions AND zero-duration windows.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "RawFederationSyncWindow")]
pub struct FederationSyncWindow {
    /// Wire schema version. See [`FEDERATION_SYNC_WINDOW_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// State class this window applies to.
    pub state_class: StateClass,
    /// Window duration in Unix seconds. MUST be `> 0`.
    pub window_duration_secs: u64,
    /// blake3 binding hash over all bound fields.
    pub window_hash: Hash,
}

#[derive(Deserialize)]
struct RawFederationSyncWindow {
    schema_version: u32,
    state_class: StateClass,
    window_duration_secs: u64,
    window_hash: Hash,
}

impl TryFrom<RawFederationSyncWindow> for FederationSyncWindow {
    type Error = String;

    fn try_from(raw: RawFederationSyncWindow) -> Result<Self, Self::Error> {
        if raw.schema_version != FEDERATION_SYNC_WINDOW_SCHEMA_VERSION {
            return Err(FederationSyncWindowError::UnsupportedSchemaVersion {
                got: raw.schema_version,
                supported: FEDERATION_SYNC_WINDOW_SCHEMA_VERSION,
            }
            .to_string());
        }
        if raw.window_duration_secs == 0 {
            return Err(FederationSyncWindowError::ZeroWindowDuration.to_string());
        }
        Ok(Self {
            schema_version: raw.schema_version,
            state_class: raw.state_class,
            window_duration_secs: raw.window_duration_secs,
            window_hash: raw.window_hash,
        })
    }
}

/// Canonical binding fields for `FederationSyncWindow::window_hash`.
#[derive(Serialize)]
struct FederationSyncWindowBinding {
    schema_version: u32,
    state_class: StateClass,
    window_duration_secs: u64,
}

impl FederationSyncWindow {
    /// Domain-separation tag for `window_hash`. Distinct from all
    /// other proof records.
    pub const DOMAIN_TAG: &'static [u8] = b"icn:federation-sync-window:v1";

    /// Construct a new window with computed `window_hash`.
    ///
    /// Returns [`FederationSyncWindowError::ZeroWindowDuration`] if
    /// `window_duration_secs == 0`.
    pub fn new(
        state_class: StateClass,
        window_duration_secs: u64,
    ) -> Result<Self, FederationSyncWindowError> {
        if window_duration_secs == 0 {
            return Err(FederationSyncWindowError::ZeroWindowDuration);
        }
        let window_hash = Self::compute_window_hash(
            FEDERATION_SYNC_WINDOW_SCHEMA_VERSION,
            state_class,
            window_duration_secs,
        );
        Ok(Self {
            schema_version: FEDERATION_SYNC_WINDOW_SCHEMA_VERSION,
            state_class,
            window_duration_secs,
            window_hash,
        })
    }

    /// Compute the binding hash from the significant fields.
    pub fn compute_window_hash(
        schema_version: u32,
        state_class: StateClass,
        window_duration_secs: u64,
    ) -> Hash {
        let binding = FederationSyncWindowBinding {
            schema_version,
            state_class,
            window_duration_secs,
        };
        let payload = bincode::serialize(&binding)
            .expect("FederationSyncWindowBinding serialization is infallible");
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        *hasher.finalize().as_bytes()
    }

    /// `true` iff `schema_version == FEDERATION_SYNC_WINDOW_SCHEMA_VERSION`.
    pub fn is_supported_schema_version(&self) -> bool {
        self.schema_version == FEDERATION_SYNC_WINDOW_SCHEMA_VERSION
    }

    /// Verify the stored `window_hash` and `schema_version`. Fails
    /// closed on unsupported versions.
    pub fn verify_binding(&self) -> bool {
        if !self.is_supported_schema_version() {
            return false;
        }
        let recomputed = Self::compute_window_hash(
            self.schema_version,
            self.state_class,
            self.window_duration_secs,
        );
        self.window_hash == recomputed
    }
}

// ----------------------------------------------------------------------------
// QuorumSyncCheck
// ----------------------------------------------------------------------------

/// Structural validation errors a `QuorumSyncCheck` can produce.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuorumSyncCheckError {
    /// `schema_version` did not match [`QUORUM_SYNC_CHECK_SCHEMA_VERSION`].
    #[error("unsupported QuorumSyncCheck schema_version: got {got}, supported {supported}")]
    UnsupportedSchemaVersion {
        /// The version observed on the wire / at construction.
        got: u32,
        /// The version this build supports.
        supported: u32,
    },
    /// `federation_scope` is `LocalDomain` or `PeerPair`. Per spec,
    /// `QuorumSyncCheck` only applies to federation- or commons-scope
    /// work.
    #[error(
        "QuorumSyncCheck federation_scope must be Federation or Commons; \
         got a non-federation scope"
    )]
    InvalidFederationScope,
    /// `quorum_size < quorum_threshold`. A check that records fewer
    /// matching peers than the policy threshold is not a "quorum
    /// reached" proof.
    #[error(
        "QuorumSyncCheck quorum_size={quorum_size} is less than \
         quorum_threshold={quorum_threshold}"
    )]
    QuorumThresholdNotMet {
        /// Matching peer count recorded.
        quorum_size: u32,
        /// Policy threshold.
        quorum_threshold: u32,
    },
    /// `quorum_size != participating_peers.dids().len()`. The
    /// recorded count must equal the canonicalized peer set length.
    #[error(
        "QuorumSyncCheck quorum_size={quorum_size} does not equal \
         participating_peers length {peers_len}"
    )]
    QuorumSizePeerCountMismatch {
        /// Recorded matching count.
        quorum_size: u32,
        /// Canonicalized peer-set length.
        peers_len: u32,
    },
    /// `observed_at > freshness_valid_until`. The freshness window
    /// must end at or after the observation.
    #[error(
        "QuorumSyncCheck observed_at={observed_at} is after \
         freshness_valid_until={freshness_valid_until}"
    )]
    ObservedAfterFreshness {
        /// Observation timestamp.
        observed_at: u64,
        /// Freshness expiry.
        freshness_valid_until: u64,
    },
    /// The recorded freshness span exceeds the policy
    /// `FederationSyncWindow.window_duration_secs`.
    #[error(
        "QuorumSyncCheck freshness span={span_secs} exceeds \
         federation_sync_window.window_duration_secs={window_secs}"
    )]
    FreshnessExceedsPolicyWindow {
        /// The span `freshness_valid_until - observed_at`.
        span_secs: u64,
        /// The policy window's `window_duration_secs`.
        window_secs: u64,
    },
    /// `federation_sync_window.state_class` does not match the
    /// check's `state_class`.
    #[error(
        "QuorumSyncCheck federation_sync_window.state_class={window_state_class:?} does not \
         match check state_class={check_state_class:?}"
    )]
    WindowStateClassMismatch {
        /// The window's `state_class`.
        window_state_class: StateClass,
        /// The check's `state_class`.
        check_state_class: StateClass,
    },
}

impl From<QuorumSyncCheckError> for String {
    fn from(err: QuorumSyncCheckError) -> Self {
        err.to_string()
    }
}

/// Proof that a quorum of named federation peers exchanged matching
/// `StateDigest`s within a `FederationSyncWindow` for a state class.
///
/// Per spec line 234: "the gate for federation-bound placement and
/// federation settlement." Boundary rule 6 (federation/commons
/// placement) and boundary rule 7 (settlement finality) both require
/// a fresh `QuorumSyncCheck` over the relevant state classes.
///
/// # Privacy
///
/// `QuorumSyncCheck` MUST NOT carry raw state. The `matching_digest`
/// is a bounded `StateDigest` projection (Bloom / Merkle root /
/// vector clock / short list of refs); never bodies. For checks
/// derived from comparisons over private state,
/// `private_content_implication` is set.
///
/// # Self-authentication
///
/// `check_hash` is a blake3 binding under
/// `DOMAIN_TAG = b"icn:quorum-sync-check:v1"`, length-prefixed, over
/// a canonical bincode encoding of all bound fields (excluding
/// `check_hash` and `signature`). Deserialization rejects unsupported
/// schema versions AND structural rule violations.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "RawQuorumSyncCheck")]
pub struct QuorumSyncCheck {
    /// Wire schema version. See [`QUORUM_SYNC_CHECK_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// State class the quorum agreed on.
    pub state_class: StateClass,
    /// Scope of the quorum. MUST be `Federation { federation_id }`
    /// or `Commons`.
    pub federation_scope: ProbeScope,
    /// Canonicalized set of peers in the quorum.
    pub participating_peers: PeerSet,
    /// The bounded `StateDigest` projection all peers in the quorum
    /// agreed on.
    pub matching_digest: StateDigest,
    /// The policy `FederationSyncWindow` this check is measured
    /// against. Its `state_class` MUST match the check's `state_class`.
    pub federation_sync_window: FederationSyncWindow,
    /// DID of the reporter that recorded the proof.
    pub reporter_did: Did,
    /// Unix seconds when the quorum was observed.
    pub observed_at: u64,
    /// Unix seconds beyond which this check is stale. Constrained by
    /// `federation_sync_window.window_duration_secs`.
    pub freshness_valid_until: u64,
    /// Count of peers that matched. MUST equal
    /// `participating_peers.dids().len()` and be `>= quorum_threshold`.
    pub quorum_size: u32,
    /// Policy threshold the quorum size is measured against.
    pub quorum_threshold: u32,
    /// `true` if the comparison touched private state.
    pub private_content_implication: bool,
    /// 32-byte random nonce.
    pub check_nonce: [u8; 32],
    /// blake3 binding hash.
    pub check_hash: Hash,
    /// Reporter signature (empty until signed by a higher layer).
    pub signature: Signature,
}

#[derive(Deserialize)]
struct RawQuorumSyncCheck {
    schema_version: u32,
    state_class: StateClass,
    federation_scope: ProbeScope,
    participating_peers: PeerSet,
    matching_digest: StateDigest,
    federation_sync_window: FederationSyncWindow,
    reporter_did: Did,
    observed_at: u64,
    freshness_valid_until: u64,
    quorum_size: u32,
    quorum_threshold: u32,
    private_content_implication: bool,
    check_nonce: [u8; 32],
    check_hash: Hash,
    signature: Signature,
}

impl TryFrom<RawQuorumSyncCheck> for QuorumSyncCheck {
    type Error = String;

    fn try_from(raw: RawQuorumSyncCheck) -> Result<Self, Self::Error> {
        if raw.schema_version != QUORUM_SYNC_CHECK_SCHEMA_VERSION {
            return Err(QuorumSyncCheckError::UnsupportedSchemaVersion {
                got: raw.schema_version,
                supported: QUORUM_SYNC_CHECK_SCHEMA_VERSION,
            }
            .to_string());
        }
        QuorumSyncCheck::validate_structural_invariants(
            &raw.federation_scope,
            &raw.participating_peers,
            raw.state_class,
            &raw.federation_sync_window,
            raw.observed_at,
            raw.freshness_valid_until,
            raw.quorum_size,
            raw.quorum_threshold,
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            schema_version: raw.schema_version,
            state_class: raw.state_class,
            federation_scope: raw.federation_scope,
            participating_peers: raw.participating_peers,
            matching_digest: raw.matching_digest,
            federation_sync_window: raw.federation_sync_window,
            reporter_did: raw.reporter_did,
            observed_at: raw.observed_at,
            freshness_valid_until: raw.freshness_valid_until,
            quorum_size: raw.quorum_size,
            quorum_threshold: raw.quorum_threshold,
            private_content_implication: raw.private_content_implication,
            check_nonce: raw.check_nonce,
            check_hash: raw.check_hash,
            signature: raw.signature,
        })
    }
}

/// Canonical binding fields for `QuorumSyncCheck::check_hash`.
#[derive(Serialize)]
struct QuorumSyncCheckBinding<'a> {
    schema_version: u32,
    state_class: StateClass,
    federation_scope: &'a ProbeScope,
    participating_peers: &'a PeerSet,
    matching_digest: &'a StateDigest,
    federation_sync_window: &'a FederationSyncWindow,
    reporter_did: &'a Did,
    observed_at: u64,
    freshness_valid_until: u64,
    quorum_size: u32,
    quorum_threshold: u32,
    private_content_implication: bool,
    check_nonce: [u8; 32],
}

impl QuorumSyncCheck {
    /// Domain-separation tag for `check_hash`. Distinct from all
    /// other proof records.
    pub const DOMAIN_TAG: &'static [u8] = b"icn:quorum-sync-check:v1";

    /// Construct a new check with computed `check_hash` and empty
    /// signature.
    ///
    /// Returns [`QuorumSyncCheckError`] if structural invariants are
    /// violated (see [`Self::validate_structural_invariants`]).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        state_class: StateClass,
        federation_scope: ProbeScope,
        participating_peers: PeerSet,
        matching_digest: StateDigest,
        federation_sync_window: FederationSyncWindow,
        reporter_did: Did,
        observed_at: u64,
        freshness_valid_until: u64,
        quorum_size: u32,
        quorum_threshold: u32,
        private_content_implication: bool,
        check_nonce: [u8; 32],
    ) -> Result<Self, QuorumSyncCheckError> {
        Self::validate_structural_invariants(
            &federation_scope,
            &participating_peers,
            state_class,
            &federation_sync_window,
            observed_at,
            freshness_valid_until,
            quorum_size,
            quorum_threshold,
        )?;
        let check_hash = Self::compute_check_hash(
            QUORUM_SYNC_CHECK_SCHEMA_VERSION,
            state_class,
            &federation_scope,
            &participating_peers,
            &matching_digest,
            &federation_sync_window,
            &reporter_did,
            observed_at,
            freshness_valid_until,
            quorum_size,
            quorum_threshold,
            private_content_implication,
            &check_nonce,
        );
        Ok(Self {
            schema_version: QUORUM_SYNC_CHECK_SCHEMA_VERSION,
            state_class,
            federation_scope,
            participating_peers,
            matching_digest,
            federation_sync_window,
            reporter_did,
            observed_at,
            freshness_valid_until,
            quorum_size,
            quorum_threshold,
            private_content_implication,
            check_nonce,
            check_hash,
            signature: Signature::new(Vec::new()),
        })
    }

    /// Validate structural invariants:
    ///
    /// - `federation_scope` MUST be `Federation` or `Commons`.
    /// - `quorum_size >= quorum_threshold`.
    /// - `quorum_size == participating_peers.dids().len()`.
    /// - `observed_at <= freshness_valid_until`.
    /// - `freshness_valid_until - observed_at <= federation_sync_window.window_duration_secs`.
    /// - `federation_sync_window.state_class == state_class`.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_structural_invariants(
        federation_scope: &ProbeScope,
        participating_peers: &PeerSet,
        state_class: StateClass,
        federation_sync_window: &FederationSyncWindow,
        observed_at: u64,
        freshness_valid_until: u64,
        quorum_size: u32,
        quorum_threshold: u32,
    ) -> Result<(), QuorumSyncCheckError> {
        if !matches!(
            federation_scope,
            ProbeScope::Federation { .. } | ProbeScope::Commons
        ) {
            return Err(QuorumSyncCheckError::InvalidFederationScope);
        }
        if quorum_size < quorum_threshold {
            return Err(QuorumSyncCheckError::QuorumThresholdNotMet {
                quorum_size,
                quorum_threshold,
            });
        }
        let peers_len = participating_peers.dids().len() as u32;
        if quorum_size != peers_len {
            return Err(QuorumSyncCheckError::QuorumSizePeerCountMismatch {
                quorum_size,
                peers_len,
            });
        }
        if observed_at > freshness_valid_until {
            return Err(QuorumSyncCheckError::ObservedAfterFreshness {
                observed_at,
                freshness_valid_until,
            });
        }
        let span_secs = freshness_valid_until - observed_at;
        if span_secs > federation_sync_window.window_duration_secs {
            return Err(QuorumSyncCheckError::FreshnessExceedsPolicyWindow {
                span_secs,
                window_secs: federation_sync_window.window_duration_secs,
            });
        }
        if federation_sync_window.state_class != state_class {
            return Err(QuorumSyncCheckError::WindowStateClassMismatch {
                window_state_class: federation_sync_window.state_class,
                check_state_class: state_class,
            });
        }
        Ok(())
    }

    /// Compute the binding hash from the significant fields.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_check_hash(
        schema_version: u32,
        state_class: StateClass,
        federation_scope: &ProbeScope,
        participating_peers: &PeerSet,
        matching_digest: &StateDigest,
        federation_sync_window: &FederationSyncWindow,
        reporter_did: &Did,
        observed_at: u64,
        freshness_valid_until: u64,
        quorum_size: u32,
        quorum_threshold: u32,
        private_content_implication: bool,
        check_nonce: &[u8; 32],
    ) -> Hash {
        let binding = QuorumSyncCheckBinding {
            schema_version,
            state_class,
            federation_scope,
            participating_peers,
            matching_digest,
            federation_sync_window,
            reporter_did,
            observed_at,
            freshness_valid_until,
            quorum_size,
            quorum_threshold,
            private_content_implication,
            check_nonce: *check_nonce,
        };
        let payload = bincode::serialize(&binding)
            .expect("QuorumSyncCheckBinding serialization is infallible");
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        *hasher.finalize().as_bytes()
    }

    /// `true` iff `schema_version == QUORUM_SYNC_CHECK_SCHEMA_VERSION`.
    pub fn is_supported_schema_version(&self) -> bool {
        self.schema_version == QUORUM_SYNC_CHECK_SCHEMA_VERSION
    }

    /// Verify the stored `check_hash` and `schema_version`. Fails
    /// closed on unsupported versions.
    pub fn verify_binding(&self) -> bool {
        if !self.is_supported_schema_version() {
            return false;
        }
        let recomputed = Self::compute_check_hash(
            self.schema_version,
            self.state_class,
            &self.federation_scope,
            &self.participating_peers,
            &self.matching_digest,
            &self.federation_sync_window,
            &self.reporter_did,
            self.observed_at,
            self.freshness_valid_until,
            self.quorum_size,
            self.quorum_threshold,
            self.private_content_implication,
            &self.check_nonce,
        );
        self.check_hash == recomputed
    }

    /// `true` if `now_unix_seconds <= freshness_valid_until`.
    pub fn is_fresh(&self, now_unix_seconds: u64) -> bool {
        now_unix_seconds <= self.freshness_valid_until
    }
}

// ============================================================================
// DivergenceEvidence and RepairPlan records (issue #1835)
//
// Wire-stable record shapes for the next two design-level identifiers from
// `docs/spec/network-anti-entropy-proof-loops.md` §"Proof artifacts
// (forward-direction names)" beyond what #1834 / PR #1843 already landed.
//
// These records ride inside an existing Stage 5 `EffectDispatchEvidence`
// envelope (per `docs/spec/effect-dispatch-contract.md`). No new top-level
// ADR-0026 receipt class is introduced.
//
// Like `AntiEntropyProbe`, both records are self-authenticating: a
// domain-tagged blake3 binding hash is computed at construction over a
// canonical bincode encoding of the bound fields, and `verify_binding()`
// recomputes and fails closed on unsupported schema versions.
// ============================================================================

/// Schema version for `DivergenceEvidence`. Increment on any wire-affecting
/// change to the binding (field set, order, encoding).
pub const DIVERGENCE_EVIDENCE_SCHEMA_VERSION: u32 = 1;

/// Schema version for `RepairPlan`. Increment on any wire-affecting change
/// to the binding (field set, order, encoding).
pub const REPAIR_PLAN_SCHEMA_VERSION: u32 = 1;

// ----------------------------------------------------------------------------
// DivergenceClass — closed 18-class taxonomy
// ----------------------------------------------------------------------------

/// Closed taxonomy of divergence classes the policy oracle can record.
///
/// Matches `docs/spec/network-anti-entropy-proof-loops.md` §"Divergence
/// classes" verbatim — eighteen classes with `Unclassifiable` as the
/// fallback that triggers governance review rather than automatic repair.
///
/// The kernel does not interpret what each class *means* — classification
/// is the policy-oracle phase per `docs/architecture/KERNEL_APP_SEPARATION.md`.
/// The kernel only ensures that the recorded class round-trips deterministically
/// and that no class outside this closed set can be expressed on the wire.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DivergenceClass {
    /// Class 1 — peer A has a receipt for an effect at a known content hash;
    /// peer B does not.
    MissingReceipt,
    /// Class 2 — both peers claim a receipt at the same logical identifier
    /// but the receipts have different content hashes.
    ConflictingReceipt,
    /// Class 3 — peer A has an `ArtifactRegistry` entry; peer B's index
    /// does not include it.
    MissingArtifactMetadata,
    /// Class 4 — both peers have the artifact under the same logical name;
    /// their content hashes differ.
    ContentHashMismatch,
    /// Class 5 — current replica count is below
    /// `ReplicationPolicy.target_replicas` but the policy permits a grace
    /// window before escalation.
    ReplicaLag,
    /// Class 6 — replica count has fallen outside the grace window; the
    /// policy authorizes re-replication.
    ReplicaMissing,
    /// Class 7 — the most recent `BackupPolicy`-prescribed backup did not
    /// verify (per `docs/spec/storage-durability-policies.md`).
    BackupVerificationFailure,
    /// Class 8 — the `RecoveryPolicy` cadence has elapsed without a
    /// successful restore-test receipt.
    RestoreDrillMissing,
    /// Class 9 — peer's freshness timestamp falls outside the domain's
    /// `FederationSyncWindow` for the state class in question.
    PeerBehindSyncWindow,
    /// Class 10 — peer is observed making inconsistent claims to different
    /// peers about the same state at the same time. Treated as suspected
    /// misbehavior; mandatory governance review.
    PeerEquivocation,
    /// Class 11 — both peers claim to be operating under the same
    /// federation agreement but reference different `agreement_id`
    /// content hashes or different adopted versions.
    FederationAgreementMismatch,
    /// Class 12 — peer's `policy_version_id` for a named policy disagrees
    /// with the local adopted version
    /// (per `docs/spec/ccl-policy-registry.md`).
    CclPolicyVersionMismatch,
    /// Class 13 — peer's `evaluator_binding_id` for a named evaluator
    /// disagrees with the local binding
    /// (per `docs/spec/ccl-policy-registry.md`).
    EvaluatorBindingMismatch,
    /// Class 14 — peer cannot produce the `PlacementDecision` evidence
    /// (per `docs/spec/compute-placement-policy.md`) for a workload
    /// that was claimed to have completed in scope.
    PlacementEvidenceMissing,
    /// Class 15 — peer's clearing-batch digest disagrees with the local
    /// clearing manager's view
    /// (per `docs/spec/federation-settlement-finality.md`).
    SettlementRecordMismatch,
    /// Class 16 — both peers have a scoped-vault reference at the same
    /// logical identifier; the `ArtifactDigest`s disagree; the divergence
    /// is recorded as existence-plus-scope-plus-affected-records, NEVER
    /// as content. Per spec §"Privacy and custody rules."
    PrivateObjectReferenceMismatchWithoutContentDisclosure,
    /// Class 17 — the most recent `IntegrityPolicy`-prescribed
    /// verification failed
    /// (per `docs/spec/storage-durability-policies.md`).
    IntegrityPolicyViolation,
    /// Class 18 — the comparison is non-matching but does not fit any of
    /// the above. Triggers governance review rather than automatic repair.
    Unclassifiable,
}

// ----------------------------------------------------------------------------
// Bounded helper records
// ----------------------------------------------------------------------------

/// Closed set of peers involved in a divergence observation.
///
/// Stored as `Vec<Did>` **sorted lexicographically and deduplicated**.
/// Invariant enforced at construction (via [`Self::from_dids`]) AND on the
/// deserialize path (via `#[serde(from = "RawPeerSet")]`) so two peers
/// recording the same divergence compute the same `evidence_hash`
/// regardless of how the DIDs arrived. Field private; use [`Self::dids`]
/// for read access.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "RawPeerSet")]
pub struct PeerSet {
    dids: Vec<Did>,
}

#[derive(Deserialize)]
struct RawPeerSet {
    dids: Vec<Did>,
}

impl From<RawPeerSet> for PeerSet {
    fn from(raw: RawPeerSet) -> Self {
        Self::from_dids(raw.dids)
    }
}

impl PeerSet {
    /// Construct from an unsorted iterator of DIDs.
    ///
    /// Entries are sorted lexicographically and deduplicated.
    pub fn from_dids<I>(dids: I) -> Self
    where
        I: IntoIterator<Item = Did>,
    {
        let mut v: Vec<Did> = dids.into_iter().collect();
        v.sort();
        v.dedup();
        Self { dids: v }
    }

    /// The canonical sorted, deduplicated DIDs.
    pub fn dids(&self) -> &[Did] {
        &self.dids
    }
}

/// Reference to the policy clause under which a divergence was classified
/// or a repair was planned.
///
/// Metadata only — the kernel does not interpret policy semantics. The
/// policy oracle supplies the values; the kernel records them so an
/// auditor can later look up the named policy version and clause.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PolicyClauseRef {
    /// Opaque policy identifier (e.g., `"compute-placement"`,
    /// `"storage-durability"`). The kernel does not validate the value.
    pub policy_id: String,
    /// Opaque version identifier for the named policy at the time of
    /// classification (e.g., a content hash hex, a semver string, or a
    /// CCL `policy_version_id`). Free-form by design — the policy
    /// registry, not the kernel, defines the value space.
    pub policy_version_id: String,
    /// Opaque clause identifier within the named policy version
    /// (e.g., `"boundary-rules.4"`).
    pub clause_id: String,
}

/// The digest-form mismatch observed between peers, if any.
///
/// Not every divergence class is a two-peer digest comparison. Replica-count
/// classes, backup verification, restore drill, integrity policy, and CCL
/// policy version mismatches are not digest-shaped; the divergence class
/// still names what diverged. Use [`Self::NotApplicable`] for those.
///
/// # Privacy
///
/// For divergence class
/// [`DivergenceClass::PrivateObjectReferenceMismatchWithoutContentDisclosure`],
/// the embedded `StateDigest`s are bounded representations (Bloom over
/// content hashes, Merkle root over an index, short list of reference
/// hashes) — never object bodies. The privacy contract is documented and
/// reviewable, not type-system-enforced.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DigestMismatch {
    /// The local peer has a digest at the affected address; the remote
    /// peer does not (the remote is "missing on remote" relative to local).
    MissingOnRemote { local: StateDigest },
    /// The remote peer has a digest at the affected address; the local
    /// peer does not (the remote has entries the local responder
    /// does not).
    MissingOnLocal { remote: StateDigest },
    /// Both peers claim digests at the same logical address, but the
    /// digests differ.
    Differs {
        local: StateDigest,
        remote: StateDigest,
    },
    /// No digest comparison applies (e.g., replica-count divergence,
    /// missing restore drill, integrity-policy failure, policy-version
    /// mismatch). The `DivergenceClass` still names what diverged.
    NotApplicable,
}

// ----------------------------------------------------------------------------
// DivergenceEvidence
// ----------------------------------------------------------------------------

/// Classified non-matching outcome of an anti-entropy proof loop.
///
/// Produced by the policy oracle in phase 4 ("Classify") of
/// `docs/spec/network-anti-entropy-proof-loops.md`. Records the
/// [`DivergenceClass`], the affected [`StateClass`], the [`ProbeScope`],
/// the peers involved, the digest forms (if any) compared, the policy
/// clause under which classification was made, the freshness window the
/// evidence is valid for, and whether private content was implicated.
///
/// # Privacy
///
/// Per spec §"Privacy and custody rules", a `DivergenceEvidence` MUST NOT
/// embed private content bytes. For
/// [`DivergenceClass::PrivateObjectReferenceMismatchWithoutContentDisclosure`],
/// the `DigestMismatch` carries opaque references / hashes only; the
/// `private_content_implication` flag records that the divergence touched
/// private state so downstream renderers can apply the
/// "review required / existence + scope only" rule.
///
/// # Self-authentication
///
/// `evidence_hash` is a blake3 binding over a domain-separated
/// (`DOMAIN_TAG = b"icn:divergence-evidence:v1"`), length-prefixed canonical
/// bincode encoding of all bound fields (excluding `evidence_hash` and
/// `signature`). [`Self::verify_binding`] re-checks both the hash and the
/// schema version; either alone is insufficient.
///
/// # Wire-version policing (fail-closed)
///
/// Deserialization uses `#[serde(try_from = "RawDivergenceEvidence")]` so
/// wire data with `schema_version != DIVERGENCE_EVIDENCE_SCHEMA_VERSION`
/// is rejected before any `DivergenceEvidence` value is constructed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "RawDivergenceEvidence")]
pub struct DivergenceEvidence {
    /// Wire schema version. See [`DIVERGENCE_EVIDENCE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Closed divergence class.
    pub divergence_class: DivergenceClass,
    /// The state class against which the divergence was observed.
    pub affected_state_class: StateClass,
    /// Target scope at which the divergence was observed.
    pub scope: ProbeScope,
    /// Canonical sorted, deduplicated set of peers involved.
    pub peers: PeerSet,
    /// Digest-form mismatch (or [`DigestMismatch::NotApplicable`] for
    /// non-digest divergence classes).
    pub digest_mismatch: DigestMismatch,
    /// Reference to the policy clause under which classification was made.
    pub policy_clause: PolicyClauseRef,
    /// Classifier's clock at construction (Unix seconds).
    pub freshness_emitted_at: u64,
    /// Timestamp beyond which the evidence is stale (Unix seconds).
    pub freshness_valid_until: u64,
    /// `true` if the divergence touched private state (e.g., a
    /// scoped-vault reference). Renderers MUST gate technical detail
    /// behind the disclosure scope of the affected state.
    pub private_content_implication: bool,
    /// 32-byte random nonce. Two evidence records with otherwise-identical
    /// fields produce distinct `evidence_hash`es.
    pub evidence_nonce: [u8; 32],
    /// blake3 binding hash over all bound fields, computed at construction.
    pub evidence_hash: Hash,
    /// Classifier signature (empty until signed by a higher layer).
    pub signature: Signature,
}

/// Raw wire form for [`DivergenceEvidence`]. Validated into the public
/// type via [`TryFrom`] (fails closed on unsupported `schema_version`).
#[derive(Deserialize)]
struct RawDivergenceEvidence {
    schema_version: u32,
    divergence_class: DivergenceClass,
    affected_state_class: StateClass,
    scope: ProbeScope,
    peers: PeerSet,
    digest_mismatch: DigestMismatch,
    policy_clause: PolicyClauseRef,
    freshness_emitted_at: u64,
    freshness_valid_until: u64,
    private_content_implication: bool,
    evidence_nonce: [u8; 32],
    evidence_hash: Hash,
    signature: Signature,
}

impl TryFrom<RawDivergenceEvidence> for DivergenceEvidence {
    type Error = String;

    fn try_from(raw: RawDivergenceEvidence) -> Result<Self, Self::Error> {
        if raw.schema_version != DIVERGENCE_EVIDENCE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported DivergenceEvidence schema_version: got {}, supported {}",
                raw.schema_version, DIVERGENCE_EVIDENCE_SCHEMA_VERSION,
            ));
        }
        Ok(Self {
            schema_version: raw.schema_version,
            divergence_class: raw.divergence_class,
            affected_state_class: raw.affected_state_class,
            scope: raw.scope,
            peers: raw.peers,
            digest_mismatch: raw.digest_mismatch,
            policy_clause: raw.policy_clause,
            freshness_emitted_at: raw.freshness_emitted_at,
            freshness_valid_until: raw.freshness_valid_until,
            private_content_implication: raw.private_content_implication,
            evidence_nonce: raw.evidence_nonce,
            evidence_hash: raw.evidence_hash,
            signature: raw.signature,
        })
    }
}

/// Canonical binding fields for `DivergenceEvidence::evidence_hash`.
///
/// Excludes `evidence_hash` (the output) and `signature` (filled after
/// binding). Bincode-serialized in a stable order.
#[derive(Serialize)]
struct DivergenceEvidenceBinding<'a> {
    schema_version: u32,
    divergence_class: DivergenceClass,
    affected_state_class: StateClass,
    scope: &'a ProbeScope,
    peers: &'a PeerSet,
    digest_mismatch: &'a DigestMismatch,
    policy_clause: &'a PolicyClauseRef,
    freshness_emitted_at: u64,
    freshness_valid_until: u64,
    private_content_implication: bool,
    evidence_nonce: [u8; 32],
}

impl DivergenceEvidence {
    /// Domain-separation tag. Distinct from
    /// [`AntiEntropyProbe::DOMAIN_TAG`] and [`ArtifactReceipt::DOMAIN_TAG`].
    pub const DOMAIN_TAG: &'static [u8] = b"icn:divergence-evidence:v1";

    /// Construct a new evidence record with computed `evidence_hash` and
    /// empty signature. `schema_version` is set to
    /// [`DIVERGENCE_EVIDENCE_SCHEMA_VERSION`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        divergence_class: DivergenceClass,
        affected_state_class: StateClass,
        scope: ProbeScope,
        peers: PeerSet,
        digest_mismatch: DigestMismatch,
        policy_clause: PolicyClauseRef,
        freshness_emitted_at: u64,
        freshness_valid_until: u64,
        private_content_implication: bool,
        evidence_nonce: [u8; 32],
    ) -> Self {
        let evidence_hash = Self::compute_evidence_hash(
            DIVERGENCE_EVIDENCE_SCHEMA_VERSION,
            divergence_class,
            affected_state_class,
            &scope,
            &peers,
            &digest_mismatch,
            &policy_clause,
            freshness_emitted_at,
            freshness_valid_until,
            private_content_implication,
            &evidence_nonce,
        );
        Self {
            schema_version: DIVERGENCE_EVIDENCE_SCHEMA_VERSION,
            divergence_class,
            affected_state_class,
            scope,
            peers,
            digest_mismatch,
            policy_clause,
            freshness_emitted_at,
            freshness_valid_until,
            private_content_implication,
            evidence_nonce,
            evidence_hash,
            signature: Signature::new(Vec::new()),
        }
    }

    /// Compute the binding hash from the significant fields.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_evidence_hash(
        schema_version: u32,
        divergence_class: DivergenceClass,
        affected_state_class: StateClass,
        scope: &ProbeScope,
        peers: &PeerSet,
        digest_mismatch: &DigestMismatch,
        policy_clause: &PolicyClauseRef,
        freshness_emitted_at: u64,
        freshness_valid_until: u64,
        private_content_implication: bool,
        evidence_nonce: &[u8; 32],
    ) -> Hash {
        let binding = DivergenceEvidenceBinding {
            schema_version,
            divergence_class,
            affected_state_class,
            scope,
            peers,
            digest_mismatch,
            policy_clause,
            freshness_emitted_at,
            freshness_valid_until,
            private_content_implication,
            evidence_nonce: *evidence_nonce,
        };
        let payload = bincode::serialize(&binding)
            .expect("DivergenceEvidenceBinding serialization is infallible");
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        *hasher.finalize().as_bytes()
    }

    /// `true` iff `schema_version == DIVERGENCE_EVIDENCE_SCHEMA_VERSION`.
    pub fn is_supported_schema_version(&self) -> bool {
        self.schema_version == DIVERGENCE_EVIDENCE_SCHEMA_VERSION
    }

    /// Verify the stored `evidence_hash` and `schema_version`.
    ///
    /// Returns `true` only if both hold. Fails closed on unsupported
    /// versions even when the stored hash matches a recomputation under
    /// that version. Does NOT verify the signature.
    pub fn verify_binding(&self) -> bool {
        if !self.is_supported_schema_version() {
            return false;
        }
        let recomputed = Self::compute_evidence_hash(
            self.schema_version,
            self.divergence_class,
            self.affected_state_class,
            &self.scope,
            &self.peers,
            &self.digest_mismatch,
            &self.policy_clause,
            self.freshness_emitted_at,
            self.freshness_valid_until,
            self.private_content_implication,
            &self.evidence_nonce,
        );
        self.evidence_hash == recomputed
    }

    /// `true` if `now_unix_seconds <= freshness_valid_until`.
    pub fn is_fresh(&self, now_unix_seconds: u64) -> bool {
        now_unix_seconds <= self.freshness_valid_until
    }
}

// ----------------------------------------------------------------------------
// RepairPlan
// ----------------------------------------------------------------------------

/// Closed set of repair actions a `RepairPlan` may propose.
///
/// Derived from `docs/spec/network-anti-entropy-proof-loops.md` §"Plan"
/// and §"Failure and safety table". The kernel does NOT execute these
/// actions — `RepairPlan` records what a policy oracle decided, not what
/// the runtime did. Execution receipts are tracked separately (forward
/// work: `RepairReceipt`, currently named only via
/// [`ExpectedRepairReceiptClass`]).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    /// Fetch the receipt(s) / artifact metadata the local peer is missing.
    FetchMissing,
    /// Re-replicate to restore the target replica count.
    ReReplicate,
    /// Retry the most recent failed backup verification per
    /// `BackupPolicy`.
    RetryBackup,
    /// Run the overdue restore drill per `RecoveryPolicy`.
    RunRestoreDrill,
    /// Retry the most recent failed integrity verification per
    /// `IntegrityPolicy`.
    RetryIntegrityVerification,
    /// Quarantine the offending peer's contributions pending governance
    /// review (e.g., for `PeerEquivocation`).
    QuarantinePeerPendingReview,
    /// Escalate the divergence to federation clearing (e.g., for
    /// settlement-record mismatch within an adopted federation
    /// agreement).
    EscalateToFederationClearing,
    /// Hold for explicit governance review — no automatic repair
    /// authorized. Used for governance-authoritative state, equivocation,
    /// and unclassifiable divergences.
    RequestGovernanceReview,
    /// Restart the dispute window per
    /// `docs/spec/federation-settlement-finality.md` finality rule.
    RestartDisputeWindow,
    /// No automatic repair authorized; record `DivergenceEvidence` only.
    /// Distinct from `RequestGovernanceReview` in that no review is
    /// pending — the divergence is recorded for audit but not actioned.
    NoAutomaticRepair,
}

/// Closed set of authority bases a `RepairPlan` may name.
///
/// A `RepairPlan` SHOULD NOT propose automatic repair without naming the
/// authority basis. The kernel does not verify the named authority's
/// validity — that is a policy-oracle concern — but it records the basis
/// so the boundary rule "No repair beyond authority"
/// (`docs/spec/network-anti-entropy-proof-loops.md` §"Boundary rules"
/// rule 2) is auditable.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityBasis {
    /// The repair is authorized by a named clause of an adopted
    /// `DomainPolicy`.
    DomainPolicyClause(PolicyClauseRef),
    /// The repair is authorized by a covering governance mandate
    /// (per ADR-0014 / ADR-0019). The mandate is referenced by its
    /// binding hash; the kernel does not verify the mandate body.
    GovernanceMandate { mandate_hash: Hash },
    /// The repair is authorized by an adopted federation agreement.
    /// Referenced by the agreement's `agreement_id` binding hash.
    FederationAgreement { agreement_hash: Hash },
    /// No automatic authority; explicit governance review is required.
    GovernanceReviewRequired,
    /// No automatic authority and no review pending. The plan records the
    /// divergence for audit without taking action.
    NoAutomaticAuthority,
}

/// Closed set of references to the spec's ten boundary rules.
///
/// A `RepairPlan` lists which boundary rules its scope and action have
/// been checked against. Inclusion is informational; the kernel does not
/// re-verify the named rule.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryRuleRef {
    /// Boundary rule 1.
    NoSilentGovernanceAuthoritativeRepair,
    /// Boundary rule 2.
    NoRepairBeyondAuthority,
    /// Boundary rule 3.
    NoRawPrivateContentInGossipOrProbes,
    /// Boundary rule 4.
    NoLocalityOrDisclosureWidening,
    /// Boundary rule 5.
    NoTreatingDegradedSyncAsHealthy,
    /// Boundary rule 6.
    NoFederationOrCommonsPlacementWithStaleProof,
    /// Boundary rule 7.
    NoSettlementFinalityWithoutAntiEntropyProof,
    /// Boundary rule 8.
    NoMemberFacingLie,
    /// Boundary rule 9.
    NoProductionOrLiveFederationClaim,
    /// Boundary rule 10.
    NoGenericCoopPrefixedPrimitives,
}

/// Canonical sorted, deduplicated set of [`BoundaryRuleRef`]s.
///
/// Invariant enforced at construction (via [`Self::from_rules`]) AND on
/// the deserialize path (via `#[serde(from = "RawBoundaryRuleSet")]`).
/// Field private; use [`Self::rules`] for read access.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "RawBoundaryRuleSet")]
pub struct BoundaryRuleSet {
    rules: Vec<BoundaryRuleRef>,
}

#[derive(Deserialize)]
struct RawBoundaryRuleSet {
    rules: Vec<BoundaryRuleRef>,
}

impl From<RawBoundaryRuleSet> for BoundaryRuleSet {
    fn from(raw: RawBoundaryRuleSet) -> Self {
        Self::from_rules(raw.rules)
    }
}

impl BoundaryRuleSet {
    /// Construct from an unsorted iterator. Entries are sorted and
    /// deduplicated.
    pub fn from_rules<I>(rules: I) -> Self
    where
        I: IntoIterator<Item = BoundaryRuleRef>,
    {
        let mut v: Vec<BoundaryRuleRef> = rules.into_iter().collect();
        v.sort();
        v.dedup();
        Self { rules: v }
    }

    /// The canonical sorted, deduplicated boundary rule references.
    pub fn rules(&self) -> &[BoundaryRuleRef] {
        &self.rules
    }
}

/// Closed set of expected `RepairReceipt` classes a `RepairPlan` may name.
///
/// `RepairReceipt` itself is forward work — not implemented in this PR
/// and not in scope for #1835. This enum exists so a `RepairPlan` can
/// name the receipt class it expects on completion without depending on
/// the receipt's wire shape. Maps 1:1 to [`RepairAction`].
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedRepairReceiptClass {
    /// Receipt for a `FetchMissing` action.
    FetchMissingReceipt,
    /// Receipt for a `ReReplicate` action.
    ReReplicationReceipt,
    /// Receipt for a `RetryBackup` action.
    BackupRetryReceipt,
    /// Receipt for a `RunRestoreDrill` action.
    RestoreDrillReceipt,
    /// Receipt for a `RetryIntegrityVerification` action.
    IntegrityVerificationReceipt,
    /// Receipt for a `QuarantinePeerPendingReview` action.
    QuarantineReceipt,
    /// Receipt for an `EscalateToFederationClearing` action.
    FederationClearingEscalationReceipt,
    /// Receipt for a `RequestGovernanceReview` action.
    GovernanceReviewReceipt,
    /// Receipt for a `RestartDisputeWindow` action.
    DisputeWindowRestartReceipt,
    /// Sentinel for `NoAutomaticRepair` — no receipt is expected; the
    /// divergence evidence is the only artifact produced.
    NoAutomaticRepairReceipt,
}

/// Repair plan produced by the policy oracle in phase 5 ("Plan") of
/// `docs/spec/network-anti-entropy-proof-loops.md`.
///
/// Names the [`RepairAction`], the [`AuthorityBasis`], the scope, the
/// boundary rules the plan has been checked against, and the expected
/// [`ExpectedRepairReceiptClass`] on completion. Cross-links to the
/// [`DivergenceEvidence`] it acts on via that evidence's binding hash.
///
/// # The plan is not the execution
///
/// A `RepairPlan` records a decision, not an outcome. The kernel does
/// not execute repairs (that is a runtime / app-side concern), does not
/// verify the named authority (that is a policy-oracle concern), and
/// does not produce `RepairReceipt`s (forward work). The plan exists so
/// the boundary rule "No repair beyond authority" is auditable.
///
/// # Self-authentication
///
/// `plan_hash` is a blake3 binding under
/// `DOMAIN_TAG = b"icn:repair-plan:v1"`, length-prefixed, over a canonical
/// bincode encoding of the bound fields. [`Self::verify_binding`] re-checks
/// both the hash and the schema version. Deserialization rejects
/// unsupported schema versions via `#[serde(try_from = ...)]`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "RawRepairPlan")]
pub struct RepairPlan {
    /// Wire schema version. See [`REPAIR_PLAN_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// The repair action the policy oracle proposed.
    pub action: RepairAction,
    /// Why the policy oracle says this action is allowed.
    pub authority_basis: AuthorityBasis,
    /// Scope of the repair (which records / which peers).
    pub scope: ProbeScope,
    /// Boundary rules the plan has been checked against.
    pub boundary_rules: BoundaryRuleSet,
    /// Receipt class expected on completion (forward work).
    pub expected_repair_receipt_class: ExpectedRepairReceiptClass,
    /// `evidence_hash` of the [`DivergenceEvidence`] this plan acts on.
    /// Used to cross-link plan and evidence in an audit trail.
    pub divergence_evidence_hash: Hash,
    /// Planner's clock at construction (Unix seconds).
    pub freshness_emitted_at: u64,
    /// Timestamp beyond which the plan is stale (Unix seconds).
    pub freshness_valid_until: u64,
    /// 32-byte random nonce.
    pub plan_nonce: [u8; 32],
    /// blake3 binding hash over all bound fields, computed at construction.
    pub plan_hash: Hash,
    /// Planner signature (empty until signed by a higher layer).
    pub signature: Signature,
}

/// Raw wire form for [`RepairPlan`]. Validated via [`TryFrom`].
#[derive(Deserialize)]
struct RawRepairPlan {
    schema_version: u32,
    action: RepairAction,
    authority_basis: AuthorityBasis,
    scope: ProbeScope,
    boundary_rules: BoundaryRuleSet,
    expected_repair_receipt_class: ExpectedRepairReceiptClass,
    divergence_evidence_hash: Hash,
    freshness_emitted_at: u64,
    freshness_valid_until: u64,
    plan_nonce: [u8; 32],
    plan_hash: Hash,
    signature: Signature,
}

impl TryFrom<RawRepairPlan> for RepairPlan {
    type Error = String;

    fn try_from(raw: RawRepairPlan) -> Result<Self, Self::Error> {
        if raw.schema_version != REPAIR_PLAN_SCHEMA_VERSION {
            return Err(format!(
                "unsupported RepairPlan schema_version: got {}, supported {}",
                raw.schema_version, REPAIR_PLAN_SCHEMA_VERSION,
            ));
        }
        Ok(Self {
            schema_version: raw.schema_version,
            action: raw.action,
            authority_basis: raw.authority_basis,
            scope: raw.scope,
            boundary_rules: raw.boundary_rules,
            expected_repair_receipt_class: raw.expected_repair_receipt_class,
            divergence_evidence_hash: raw.divergence_evidence_hash,
            freshness_emitted_at: raw.freshness_emitted_at,
            freshness_valid_until: raw.freshness_valid_until,
            plan_nonce: raw.plan_nonce,
            plan_hash: raw.plan_hash,
            signature: raw.signature,
        })
    }
}

/// Canonical binding fields for `RepairPlan::plan_hash`.
#[derive(Serialize)]
struct RepairPlanBinding<'a> {
    schema_version: u32,
    action: RepairAction,
    authority_basis: &'a AuthorityBasis,
    scope: &'a ProbeScope,
    boundary_rules: &'a BoundaryRuleSet,
    expected_repair_receipt_class: ExpectedRepairReceiptClass,
    divergence_evidence_hash: Hash,
    freshness_emitted_at: u64,
    freshness_valid_until: u64,
    plan_nonce: [u8; 32],
}

impl RepairPlan {
    /// Domain-separation tag. Distinct from
    /// [`DivergenceEvidence::DOMAIN_TAG`], [`AntiEntropyProbe::DOMAIN_TAG`],
    /// and [`ArtifactReceipt::DOMAIN_TAG`].
    pub const DOMAIN_TAG: &'static [u8] = b"icn:repair-plan:v1";

    /// Construct a new plan with computed `plan_hash` and empty signature.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        action: RepairAction,
        authority_basis: AuthorityBasis,
        scope: ProbeScope,
        boundary_rules: BoundaryRuleSet,
        expected_repair_receipt_class: ExpectedRepairReceiptClass,
        divergence_evidence_hash: Hash,
        freshness_emitted_at: u64,
        freshness_valid_until: u64,
        plan_nonce: [u8; 32],
    ) -> Self {
        let plan_hash = Self::compute_plan_hash(
            REPAIR_PLAN_SCHEMA_VERSION,
            action,
            &authority_basis,
            &scope,
            &boundary_rules,
            expected_repair_receipt_class,
            divergence_evidence_hash,
            freshness_emitted_at,
            freshness_valid_until,
            &plan_nonce,
        );
        Self {
            schema_version: REPAIR_PLAN_SCHEMA_VERSION,
            action,
            authority_basis,
            scope,
            boundary_rules,
            expected_repair_receipt_class,
            divergence_evidence_hash,
            freshness_emitted_at,
            freshness_valid_until,
            plan_nonce,
            plan_hash,
            signature: Signature::new(Vec::new()),
        }
    }

    /// Compute the binding hash from the significant fields.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_plan_hash(
        schema_version: u32,
        action: RepairAction,
        authority_basis: &AuthorityBasis,
        scope: &ProbeScope,
        boundary_rules: &BoundaryRuleSet,
        expected_repair_receipt_class: ExpectedRepairReceiptClass,
        divergence_evidence_hash: Hash,
        freshness_emitted_at: u64,
        freshness_valid_until: u64,
        plan_nonce: &[u8; 32],
    ) -> Hash {
        let binding = RepairPlanBinding {
            schema_version,
            action,
            authority_basis,
            scope,
            boundary_rules,
            expected_repair_receipt_class,
            divergence_evidence_hash,
            freshness_emitted_at,
            freshness_valid_until,
            plan_nonce: *plan_nonce,
        };
        let payload =
            bincode::serialize(&binding).expect("RepairPlanBinding serialization is infallible");
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        *hasher.finalize().as_bytes()
    }

    /// `true` iff `schema_version == REPAIR_PLAN_SCHEMA_VERSION`.
    pub fn is_supported_schema_version(&self) -> bool {
        self.schema_version == REPAIR_PLAN_SCHEMA_VERSION
    }

    /// Verify the stored `plan_hash` and `schema_version`. Fails closed
    /// on unsupported versions.
    pub fn verify_binding(&self) -> bool {
        if !self.is_supported_schema_version() {
            return false;
        }
        let recomputed = Self::compute_plan_hash(
            self.schema_version,
            self.action,
            &self.authority_basis,
            &self.scope,
            &self.boundary_rules,
            self.expected_repair_receipt_class,
            self.divergence_evidence_hash,
            self.freshness_emitted_at,
            self.freshness_valid_until,
            &self.plan_nonce,
        );
        self.plan_hash == recomputed
    }

    /// `true` if `now_unix_seconds <= freshness_valid_until`.
    pub fn is_fresh(&self, now_unix_seconds: u64) -> bool {
        now_unix_seconds <= self.freshness_valid_until
    }
}

// ============================================================================
// RepairReceipt and supporting taxonomies (issue #1849)
//
// Wire-stable record shape for the resolved repair artifact named in
// `docs/spec/network-anti-entropy-proof-loops.md` §"Evidence" (phase 7) and
// §"Proof artifacts (forward-direction names)". Completes the
// `AntiEntropyProbe` → `DivergenceEvidence` → `RepairPlan` → `RepairReceipt`
// proof rail at schema level.
//
// Per spec line 186: "No new top-level receipt class is added. `RepairReceipt`
// is an evidence-artifact identifier traveling inside an existing envelope"
// (Stage 5 `EffectDispatchEvidence` per `docs/spec/effect-dispatch-contract.md`
// or Layer 2 `ArtifactReceipt` per ADR-0026 where the repair was a blob
// transfer). `EffectOutcome` is reused from `crate::effects` per spec line
// 181 — the kernel does not redefine the outcome vocabulary.
//
// Like the prior records in this module, `RepairReceipt` is
// self-authenticating: a domain-tagged blake3 binding hash is computed at
// construction over a canonical bincode encoding of the bound fields, and
// `verify_binding()` recomputes and fails closed on unsupported schema
// versions even when the stored hash matches a recomputation under the bogus
// version.
// ============================================================================

/// Schema version for `RepairReceipt`. Increment on any wire-affecting
/// change to the binding (field set, order, encoding).
pub const REPAIR_RECEIPT_SCHEMA_VERSION: u32 = 1;

// ----------------------------------------------------------------------------
// RepairReceiptClass — closed taxonomy mirroring ExpectedRepairReceiptClass
// ----------------------------------------------------------------------------

/// Closed taxonomy of repair-receipt classes.
///
/// Each variant maps 1:1 to a variant of [`ExpectedRepairReceiptClass`] — a
/// `RepairPlan` declares the expected class via that enum, and the
/// completed `RepairReceipt` records the same class via this one. Keeping
/// the two enums distinct lets the receipt's wire shape evolve without
/// dragging the plan's wire shape with it.
///
/// The kernel does not interpret what each class *means* — classification
/// is the policy oracle's concern (see
/// `docs/architecture/KERNEL_APP_SEPARATION.md`). The kernel only ensures
/// the recorded class round-trips deterministically and that no class
/// outside this closed set can be expressed on the wire.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RepairReceiptClass {
    /// Receipt for a `FetchMissing` action.
    FetchMissingReceipt,
    /// Receipt for a `ReReplicate` action.
    ReReplicationReceipt,
    /// Receipt for a `RetryBackup` action.
    BackupRetryReceipt,
    /// Receipt for a `RunRestoreDrill` action.
    RestoreDrillReceipt,
    /// Receipt for a `RetryIntegrityVerification` action.
    IntegrityVerificationReceipt,
    /// Receipt for a `QuarantinePeerPendingReview` action.
    QuarantineReceipt,
    /// Receipt for an `EscalateToFederationClearing` action.
    FederationClearingEscalationReceipt,
    /// Receipt for a `RequestGovernanceReview` action.
    GovernanceReviewReceipt,
    /// Receipt for a `RestartDisputeWindow` action.
    DisputeWindowRestartReceipt,
    /// Sentinel for `NoAutomaticRepair` — no repair was attempted; the
    /// divergence evidence is the only artifact the loop produced. This
    /// class is structurally required to carry [`EffectOutcome::NoOp`];
    /// `Applied` / `Partial` / `Failed` are rejected at construction and
    /// on deserialize. Downstream anti-entropy surfaces rely on the
    /// sentinel to mean exactly "no repair happened."
    NoAutomaticRepairReceipt,
}

impl From<ExpectedRepairReceiptClass> for RepairReceiptClass {
    fn from(expected: ExpectedRepairReceiptClass) -> Self {
        match expected {
            ExpectedRepairReceiptClass::FetchMissingReceipt => Self::FetchMissingReceipt,
            ExpectedRepairReceiptClass::ReReplicationReceipt => Self::ReReplicationReceipt,
            ExpectedRepairReceiptClass::BackupRetryReceipt => Self::BackupRetryReceipt,
            ExpectedRepairReceiptClass::RestoreDrillReceipt => Self::RestoreDrillReceipt,
            ExpectedRepairReceiptClass::IntegrityVerificationReceipt => {
                Self::IntegrityVerificationReceipt
            }
            ExpectedRepairReceiptClass::QuarantineReceipt => Self::QuarantineReceipt,
            ExpectedRepairReceiptClass::FederationClearingEscalationReceipt => {
                Self::FederationClearingEscalationReceipt
            }
            ExpectedRepairReceiptClass::GovernanceReviewReceipt => Self::GovernanceReviewReceipt,
            ExpectedRepairReceiptClass::DisputeWindowRestartReceipt => {
                Self::DisputeWindowRestartReceipt
            }
            ExpectedRepairReceiptClass::NoAutomaticRepairReceipt => Self::NoAutomaticRepairReceipt,
        }
    }
}

impl From<RepairReceiptClass> for ExpectedRepairReceiptClass {
    fn from(class: RepairReceiptClass) -> Self {
        match class {
            RepairReceiptClass::FetchMissingReceipt => Self::FetchMissingReceipt,
            RepairReceiptClass::ReReplicationReceipt => Self::ReReplicationReceipt,
            RepairReceiptClass::BackupRetryReceipt => Self::BackupRetryReceipt,
            RepairReceiptClass::RestoreDrillReceipt => Self::RestoreDrillReceipt,
            RepairReceiptClass::IntegrityVerificationReceipt => Self::IntegrityVerificationReceipt,
            RepairReceiptClass::QuarantineReceipt => Self::QuarantineReceipt,
            RepairReceiptClass::FederationClearingEscalationReceipt => {
                Self::FederationClearingEscalationReceipt
            }
            RepairReceiptClass::GovernanceReviewReceipt => Self::GovernanceReviewReceipt,
            RepairReceiptClass::DisputeWindowRestartReceipt => Self::DisputeWindowRestartReceipt,
            RepairReceiptClass::NoAutomaticRepairReceipt => Self::NoAutomaticRepairReceipt,
        }
    }
}

// ----------------------------------------------------------------------------
// RepairFailureReason — bounded taxonomy for Failed / Partial outcomes
// ----------------------------------------------------------------------------

/// Closed, bounded taxonomy of failure reasons for `RepairReceipt` outcomes
/// of [`EffectOutcome::Failed`] or [`EffectOutcome::Partial`].
///
/// Intentionally narrow. The kernel records *what kind* of failure was
/// observed so a steward / auditor can route follow-up work; it does not
/// model runtime error chains. Richer detail (stack traces, exception
/// messages, sub-error codes) belongs at the executing service / app layer
/// and travels in the receipt envelope's free-form metadata if needed, not
/// in this taxonomy.
///
/// `Unclassifiable` is the fallback that lets a partial / failed repair be
/// recorded without forcing a misclassification.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RepairFailureReason {
    /// The executor's authority basis was rejected at apply time (e.g., a
    /// stale mandate, an expired policy clause, a federation agreement no
    /// longer adopted).
    AuthorityRejected,
    /// The source peer / replica required for the repair was unreachable or
    /// did not respond within the freshness window.
    SourceUnavailable,
    /// The repair completed its bounded action but the after-state digest
    /// still disagrees with the planned outcome.
    DigestMismatchPersisted,
    /// The repair would have required disclosing private content the actor
    /// does not have a disclosure scope for; the action was refused per
    /// Boundary rule 3.
    PrivateContentUnavailable,
    /// The policy oracle denied the action at apply time (distinct from
    /// `AuthorityRejected`: the authority was valid, the policy ruling was
    /// "no").
    PolicyDenied,
    /// The repair exceeded the freshness window before completing.
    Timeout,
    /// The failure does not fit any of the above. Triggers steward review
    /// rather than automatic retry.
    Unclassifiable,
}

// ----------------------------------------------------------------------------
// RepairReceipt
// ----------------------------------------------------------------------------

/// Structural validation errors a `RepairReceipt` can produce at
/// construction or wire-deserialization time.
///
/// These are kernel-level structural rules only: they catch outcome /
/// reason / digest combinations that are *internally inconsistent* per the
/// spec, not runtime errors. App-level policy violations (e.g., "this
/// authority basis cannot apply to this state class") are not enforced
/// here.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RepairReceiptError {
    /// `schema_version` did not match [`REPAIR_RECEIPT_SCHEMA_VERSION`].
    #[error("unsupported RepairReceipt schema_version: got {got}, supported {supported}")]
    UnsupportedSchemaVersion {
        /// The version observed on the wire / at construction.
        got: u32,
        /// The version this build supports.
        supported: u32,
    },
    /// Outcome is [`EffectOutcome::Applied`] or [`EffectOutcome::NoOp`] but
    /// a [`RepairFailureReason`] is set.
    #[error(
        "RepairReceipt with outcome={outcome} must not carry failure_reason \
         (Applied / NoOp are not failure outcomes)"
    )]
    FailureReasonNotAllowed {
        /// The non-failure outcome that incorrectly carried a reason.
        outcome: &'static str,
    },
    /// Outcome is [`EffectOutcome::Partial`] or [`EffectOutcome::Failed`]
    /// but no [`RepairFailureReason`] is set.
    #[error(
        "RepairReceipt with outcome={outcome} requires a bounded failure_reason \
         (Partial / Failed outcomes must record why)"
    )]
    FailureReasonRequired {
        /// The failure outcome that lacked a reason.
        outcome: &'static str,
    },
    /// Outcome is [`EffectOutcome::Failed`] but `after_state_digest` is
    /// `Some(_)`. A `Failed` outcome means no durable state mutation took
    /// place; an after-state digest would contradict that.
    #[error(
        "RepairReceipt with outcome=failed must not carry after_state_digest \
         (Failed means no durable mutation)"
    )]
    AfterStateDigestOnFailed,
    /// Class is [`RepairReceiptClass::NoAutomaticRepairReceipt`] but the
    /// outcome is not [`EffectOutcome::NoOp`]. The sentinel class means
    /// "no repair attempted" — pairing it with `Applied` / `Partial` /
    /// `Failed` would let evidence falsely claim a repair under the
    /// no-action class. Downstream anti-entropy surfaces depend on this
    /// invariant; the validator rejects the combination at construction
    /// and on the wire.
    #[error(
        "RepairReceipt class=no_automatic_repair_receipt requires outcome=no_op \
         (sentinel class must not claim a repair happened); got outcome={outcome}"
    )]
    NoAutomaticRepairReceiptRequiresNoOp {
        /// The non-`NoOp` outcome that incorrectly paired with the
        /// no-automatic-repair sentinel class.
        outcome: &'static str,
    },
}

impl From<RepairReceiptError> for String {
    fn from(err: RepairReceiptError) -> Self {
        err.to_string()
    }
}

/// The resolved repair artifact for an anti-entropy proof loop.
///
/// Produced in phase 7 ("Evidence") of
/// `docs/spec/network-anti-entropy-proof-loops.md`. Records:
///
/// - The [`RepairReceiptClass`] (1:1 from the planned
///   [`ExpectedRepairReceiptClass`]).
/// - The [`EffectOutcome`] of the repair attempt (`Applied`, `NoOp`,
///   `Partial`, `Failed`) — reused from
///   [`crate::effects::EffectOutcome`] per spec line 181.
/// - Cross-links to the [`DivergenceEvidence`] and [`RepairPlan`] this
///   receipt resolves, via their binding hashes.
/// - The [`StateClass`] affected and the [`ProbeScope`] the repair ran in.
/// - The actor [`Did`] that applied (or chose not to apply) the repair,
///   the [`AuthorityBasis`] under which they acted, and the
///   [`BoundaryRuleSet`] the receipt has been checked against.
/// - Optional before / after [`StateDigest`]s so a later probe can confirm
///   convergence. The kernel does not enforce digest presence — some state
///   classes are not digest-shaped, and some outcomes (`NoOp`,
///   `NoAutomaticRepairReceipt`) carry no digests by design.
/// - The applied-at timestamp and freshness window.
/// - A `private_content_implication` flag so renderers can gate technical
///   detail behind disclosure scope per Boundary rule 3.
/// - A bounded [`RepairFailureReason`] for `Partial` / `Failed` outcomes.
/// - A 32-byte nonce so otherwise-identical receipts get distinct hashes.
///
/// # The receipt is not the envelope
///
/// Per spec line 186 ("No new top-level receipt class is added.
/// `RepairReceipt` is an evidence-artifact identifier traveling inside an
/// existing envelope"), this record rides inside a Stage 5
/// `EffectDispatchEvidence` per
/// `docs/spec/effect-dispatch-contract.md` or alongside a Layer 2
/// `ArtifactReceipt` per ADR-0026 (for blob-transfer repairs). It does NOT
/// introduce a new ADR-0026 layer.
///
/// # The receipt is not the execution
///
/// A `RepairReceipt` records what an executor attempted and what the
/// resulting outcome was; the kernel does not execute repairs (that is a
/// runtime / app concern), does not verify the named authority's validity
/// (a policy-oracle concern), and does not autonomously emit receipts. The
/// receipt exists so the boundary rule "No member-facing lie" is auditable
/// — both stewards and members can see what was attempted and how it
/// resolved.
///
/// # Privacy
///
/// `RepairReceipt` MUST NOT carry raw private content. For repairs that
/// touch private state, `before_state_digest` / `after_state_digest`
/// carry bounded `StateDigest` projections (Bloom over content hashes,
/// Merkle root, vector clock, or short list of reference hashes) and
/// `private_content_implication` is set. Object bodies, vault bytes, and
/// raw private artifact contents never appear on this record.
///
/// # Self-authentication
///
/// `receipt_hash` is a blake3 binding under
/// `DOMAIN_TAG = b"icn:repair-receipt:v1"`, length-prefixed, over a
/// canonical bincode encoding of all bound fields (excluding `receipt_hash`
/// and `signature`). [`Self::verify_binding`] re-checks both the hash and
/// the schema version; either alone is insufficient. Deserialization
/// rejects unsupported schema versions AND outcome / reason / digest
/// combinations that are structurally inconsistent via
/// `#[serde(try_from = ...)]`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "RawRepairReceipt")]
pub struct RepairReceipt {
    /// Wire schema version. See [`REPAIR_RECEIPT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// The repair-receipt class (1:1 from the planned
    /// [`ExpectedRepairReceiptClass`]).
    pub repair_receipt_class: RepairReceiptClass,
    /// Structural outcome of the repair attempt (reused from
    /// [`crate::effects::EffectOutcome`] per spec line 181).
    pub effect_outcome: EffectOutcome,
    /// `evidence_hash` of the [`DivergenceEvidence`] this receipt resolves.
    pub divergence_evidence_hash: Hash,
    /// `plan_hash` of the [`RepairPlan`] this receipt executes.
    pub repair_plan_hash: Hash,
    /// The state class affected by the repair.
    pub affected_state_class: StateClass,
    /// Scope of the repair (which records / which peers).
    pub scope: ProbeScope,
    /// DID of the actor that applied (or recorded the non-application of)
    /// the repair.
    pub actor_did: Did,
    /// Why the actor was authorized to act (or the explicit no-authority
    /// sentinel for non-action receipts).
    pub authority_basis: AuthorityBasis,
    /// Boundary rules the receipt has been checked against.
    pub boundary_rules: BoundaryRuleSet,
    /// Digest of the affected state before the repair, when available. May
    /// be `None` for repairs that have no before-state digest (e.g.,
    /// `NoAutomaticRepairReceipt`, or state classes that are not
    /// digest-shaped).
    pub before_state_digest: Option<StateDigest>,
    /// Digest of the affected state after the repair, when available. By
    /// structural rule MUST be `None` when `effect_outcome` is
    /// [`EffectOutcome::Failed`] (no durable mutation occurred).
    pub after_state_digest: Option<StateDigest>,
    /// Actor's clock when the repair attempt resolved (Unix seconds).
    /// Applies to Applied / Partial / Failed / NoOp uniformly.
    pub applied_at: u64,
    /// Timestamp beyond which the receipt is stale (Unix seconds).
    pub freshness_valid_until: u64,
    /// `true` if the repair touched private state. Renderers MUST gate
    /// technical detail behind the disclosure scope of the affected state.
    pub private_content_implication: bool,
    /// Bounded reason for `Partial` / `Failed` outcomes. MUST be `Some(_)`
    /// for `Partial` / `Failed`, MUST be `None` for `Applied` / `NoOp`.
    pub failure_reason: Option<RepairFailureReason>,
    /// 32-byte random nonce. Two receipts with otherwise-identical fields
    /// produce distinct `receipt_hash`es.
    pub receipt_nonce: [u8; 32],
    /// blake3 binding hash over all bound fields, computed at construction.
    pub receipt_hash: Hash,
    /// Actor signature (empty until signed by a higher layer).
    pub signature: Signature,
}

/// Raw wire form for [`RepairReceipt`]. Validated into the public type via
/// [`TryFrom`] (fails closed on unsupported `schema_version` and on
/// structurally inconsistent outcome / reason / after-state combinations).
#[derive(Deserialize)]
struct RawRepairReceipt {
    schema_version: u32,
    repair_receipt_class: RepairReceiptClass,
    effect_outcome: EffectOutcome,
    divergence_evidence_hash: Hash,
    repair_plan_hash: Hash,
    affected_state_class: StateClass,
    scope: ProbeScope,
    actor_did: Did,
    authority_basis: AuthorityBasis,
    boundary_rules: BoundaryRuleSet,
    before_state_digest: Option<StateDigest>,
    after_state_digest: Option<StateDigest>,
    applied_at: u64,
    freshness_valid_until: u64,
    private_content_implication: bool,
    failure_reason: Option<RepairFailureReason>,
    receipt_nonce: [u8; 32],
    receipt_hash: Hash,
    signature: Signature,
}

impl TryFrom<RawRepairReceipt> for RepairReceipt {
    type Error = String;

    fn try_from(raw: RawRepairReceipt) -> Result<Self, Self::Error> {
        if raw.schema_version != REPAIR_RECEIPT_SCHEMA_VERSION {
            return Err(RepairReceiptError::UnsupportedSchemaVersion {
                got: raw.schema_version,
                supported: REPAIR_RECEIPT_SCHEMA_VERSION,
            }
            .to_string());
        }
        RepairReceipt::validate_outcome_consistency(
            raw.repair_receipt_class,
            raw.effect_outcome,
            raw.failure_reason,
            raw.after_state_digest.as_ref(),
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            schema_version: raw.schema_version,
            repair_receipt_class: raw.repair_receipt_class,
            effect_outcome: raw.effect_outcome,
            divergence_evidence_hash: raw.divergence_evidence_hash,
            repair_plan_hash: raw.repair_plan_hash,
            affected_state_class: raw.affected_state_class,
            scope: raw.scope,
            actor_did: raw.actor_did,
            authority_basis: raw.authority_basis,
            boundary_rules: raw.boundary_rules,
            before_state_digest: raw.before_state_digest,
            after_state_digest: raw.after_state_digest,
            applied_at: raw.applied_at,
            freshness_valid_until: raw.freshness_valid_until,
            private_content_implication: raw.private_content_implication,
            failure_reason: raw.failure_reason,
            receipt_nonce: raw.receipt_nonce,
            receipt_hash: raw.receipt_hash,
            signature: raw.signature,
        })
    }
}

/// Canonical binding fields for `RepairReceipt::receipt_hash`.
///
/// Excludes `receipt_hash` (the output) and `signature` (filled after
/// binding). Bincode-serialized in a stable order. Any new bound field
/// added here requires bumping [`REPAIR_RECEIPT_SCHEMA_VERSION`].
#[derive(Serialize)]
struct RepairReceiptBinding<'a> {
    schema_version: u32,
    repair_receipt_class: RepairReceiptClass,
    effect_outcome: EffectOutcome,
    divergence_evidence_hash: Hash,
    repair_plan_hash: Hash,
    affected_state_class: StateClass,
    scope: &'a ProbeScope,
    actor_did: &'a Did,
    authority_basis: &'a AuthorityBasis,
    boundary_rules: &'a BoundaryRuleSet,
    before_state_digest: &'a Option<StateDigest>,
    after_state_digest: &'a Option<StateDigest>,
    applied_at: u64,
    freshness_valid_until: u64,
    private_content_implication: bool,
    failure_reason: Option<RepairFailureReason>,
    receipt_nonce: [u8; 32],
}

impl RepairReceipt {
    /// Domain-separation tag for `receipt_hash`. Distinct from
    /// [`RepairPlan::DOMAIN_TAG`], [`DivergenceEvidence::DOMAIN_TAG`],
    /// [`AntiEntropyProbe::DOMAIN_TAG`], and [`ArtifactReceipt::DOMAIN_TAG`].
    pub const DOMAIN_TAG: &'static [u8] = b"icn:repair-receipt:v1";

    /// Construct a new receipt with computed `receipt_hash` and empty
    /// signature.
    ///
    /// `schema_version` is set to [`REPAIR_RECEIPT_SCHEMA_VERSION`].
    /// Returns [`RepairReceiptError`] if the outcome / reason / digest
    /// combination is structurally inconsistent (see
    /// [`Self::validate_outcome_consistency`]). The caller must sign the
    /// receipt at a higher layer before emission.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repair_receipt_class: RepairReceiptClass,
        effect_outcome: EffectOutcome,
        divergence_evidence_hash: Hash,
        repair_plan_hash: Hash,
        affected_state_class: StateClass,
        scope: ProbeScope,
        actor_did: Did,
        authority_basis: AuthorityBasis,
        boundary_rules: BoundaryRuleSet,
        before_state_digest: Option<StateDigest>,
        after_state_digest: Option<StateDigest>,
        applied_at: u64,
        freshness_valid_until: u64,
        private_content_implication: bool,
        failure_reason: Option<RepairFailureReason>,
        receipt_nonce: [u8; 32],
    ) -> Result<Self, RepairReceiptError> {
        Self::validate_outcome_consistency(
            repair_receipt_class,
            effect_outcome,
            failure_reason,
            after_state_digest.as_ref(),
        )?;
        let receipt_hash = Self::compute_receipt_hash(
            REPAIR_RECEIPT_SCHEMA_VERSION,
            repair_receipt_class,
            effect_outcome,
            divergence_evidence_hash,
            repair_plan_hash,
            affected_state_class,
            &scope,
            &actor_did,
            &authority_basis,
            &boundary_rules,
            &before_state_digest,
            &after_state_digest,
            applied_at,
            freshness_valid_until,
            private_content_implication,
            failure_reason,
            &receipt_nonce,
        );
        Ok(Self {
            schema_version: REPAIR_RECEIPT_SCHEMA_VERSION,
            repair_receipt_class,
            effect_outcome,
            divergence_evidence_hash,
            repair_plan_hash,
            affected_state_class,
            scope,
            actor_did,
            authority_basis,
            boundary_rules,
            before_state_digest,
            after_state_digest,
            applied_at,
            freshness_valid_until,
            private_content_implication,
            failure_reason,
            receipt_nonce,
            receipt_hash,
            signature: Signature::new(Vec::new()),
        })
    }

    /// Validate the structural outcome / reason / digest / sentinel-class
    /// invariants.
    ///
    /// Returns `Ok(())` if the combination is consistent, else an error
    /// describing the violation. Kernel-level structural rules only:
    ///
    /// - [`EffectOutcome::Applied`] and [`EffectOutcome::NoOp`] MUST NOT
    ///   carry a `failure_reason`.
    /// - [`EffectOutcome::Partial`] and [`EffectOutcome::Failed`] MUST
    ///   carry a `failure_reason`.
    /// - [`EffectOutcome::Failed`] MUST NOT carry an `after_state_digest`
    ///   (no durable mutation occurred).
    /// - [`RepairReceiptClass::NoAutomaticRepairReceipt`] MUST carry
    ///   [`EffectOutcome::NoOp`] — the sentinel class means "no repair
    ///   attempted." Pairing it with `Applied` / `Partial` / `Failed`
    ///   would let evidence falsely claim a repair happened under the
    ///   no-action class.
    pub fn validate_outcome_consistency(
        repair_receipt_class: RepairReceiptClass,
        effect_outcome: EffectOutcome,
        failure_reason: Option<RepairFailureReason>,
        after_state_digest: Option<&StateDigest>,
    ) -> Result<(), RepairReceiptError> {
        match effect_outcome {
            EffectOutcome::Applied | EffectOutcome::NoOp => {
                if failure_reason.is_some() {
                    return Err(RepairReceiptError::FailureReasonNotAllowed {
                        outcome: effect_outcome.as_str(),
                    });
                }
            }
            EffectOutcome::Partial | EffectOutcome::Failed => {
                if failure_reason.is_none() {
                    return Err(RepairReceiptError::FailureReasonRequired {
                        outcome: effect_outcome.as_str(),
                    });
                }
            }
        }
        if matches!(effect_outcome, EffectOutcome::Failed) && after_state_digest.is_some() {
            return Err(RepairReceiptError::AfterStateDigestOnFailed);
        }
        if matches!(
            repair_receipt_class,
            RepairReceiptClass::NoAutomaticRepairReceipt
        ) && !matches!(effect_outcome, EffectOutcome::NoOp)
        {
            return Err(RepairReceiptError::NoAutomaticRepairReceiptRequiresNoOp {
                outcome: effect_outcome.as_str(),
            });
        }
        Ok(())
    }

    /// Compute the binding hash from the significant fields.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_receipt_hash(
        schema_version: u32,
        repair_receipt_class: RepairReceiptClass,
        effect_outcome: EffectOutcome,
        divergence_evidence_hash: Hash,
        repair_plan_hash: Hash,
        affected_state_class: StateClass,
        scope: &ProbeScope,
        actor_did: &Did,
        authority_basis: &AuthorityBasis,
        boundary_rules: &BoundaryRuleSet,
        before_state_digest: &Option<StateDigest>,
        after_state_digest: &Option<StateDigest>,
        applied_at: u64,
        freshness_valid_until: u64,
        private_content_implication: bool,
        failure_reason: Option<RepairFailureReason>,
        receipt_nonce: &[u8; 32],
    ) -> Hash {
        let binding = RepairReceiptBinding {
            schema_version,
            repair_receipt_class,
            effect_outcome,
            divergence_evidence_hash,
            repair_plan_hash,
            affected_state_class,
            scope,
            actor_did,
            authority_basis,
            boundary_rules,
            before_state_digest,
            after_state_digest,
            applied_at,
            freshness_valid_until,
            private_content_implication,
            failure_reason,
            receipt_nonce: *receipt_nonce,
        };
        let payload =
            bincode::serialize(&binding).expect("RepairReceiptBinding serialization is infallible");
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        *hasher.finalize().as_bytes()
    }

    /// `true` iff `schema_version == REPAIR_RECEIPT_SCHEMA_VERSION`.
    pub fn is_supported_schema_version(&self) -> bool {
        self.schema_version == REPAIR_RECEIPT_SCHEMA_VERSION
    }

    /// Verify the stored `receipt_hash` and `schema_version`.
    ///
    /// Returns `true` only if both hold. Fails closed on unsupported
    /// versions even when the stored hash matches a recomputation under
    /// that version. Does NOT verify the signature — that is a
    /// higher-layer concern.
    pub fn verify_binding(&self) -> bool {
        if !self.is_supported_schema_version() {
            return false;
        }
        let recomputed = Self::compute_receipt_hash(
            self.schema_version,
            self.repair_receipt_class,
            self.effect_outcome,
            self.divergence_evidence_hash,
            self.repair_plan_hash,
            self.affected_state_class,
            &self.scope,
            &self.actor_did,
            &self.authority_basis,
            &self.boundary_rules,
            &self.before_state_digest,
            &self.after_state_digest,
            self.applied_at,
            self.freshness_valid_until,
            self.private_content_implication,
            self.failure_reason,
            &self.receipt_nonce,
        );
        self.receipt_hash == recomputed
    }

    /// `true` if `now_unix_seconds <= freshness_valid_until`.
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

    // =========================================================================
    // PeerSyncReport (issue #1852)
    // =========================================================================

    fn sample_responder_local_digest() -> StateDigest {
        // A different Bloom projection than the probe carries. The
        // responder's local index has MORE entries than the prober's
        // (5 vs the probe's 4), which is the digest-level shape
        // matching the sample outcome `MissingOnRemote` — i.e., the
        // prober's remote index is missing entries the responder has.
        StateDigest::Bloom(BloomProjection {
            bits: vec![0b0011_0101, 0, 0, 0, 0, 0, 0, 0],
            num_hashes: 1,
            size: 64,
            hint_count: 5,
        })
    }

    fn sample_peer_sync_report() -> PeerSyncReport {
        let probe = sample_probe();
        PeerSyncReport::new(
            probe.probe_hash,
            PeerSyncOutcome::MissingOnRemote,
            probe.state_class,
            probe.target_scope.clone(),
            "did:icn:responder456".to_string(),
            probe.prober_did.clone(),
            sample_responder_local_digest(),
            1_715_000_005,
            1_715_000_035,
            None,
            false,
            [0xAB; 32],
        )
        .expect("sample PeerSyncReport is structurally consistent")
    }

    // ---- Outcome + reason taxonomies: round-trip ----

    #[test]
    fn peer_sync_outcome_all_five_variants_round_trip() {
        let cases = [
            PeerSyncOutcome::Matching,
            PeerSyncOutcome::MissingOnLocal,
            PeerSyncOutcome::MissingOnRemote,
            PeerSyncOutcome::Divergent,
            PeerSyncOutcome::UnknownOutOfScope(UnknownOutOfScopeReason::StaleProbe),
        ];
        // Decisive evidence the spec's five outcomes are representable.
        assert_eq!(cases.len(), 5);
        for c in &cases {
            let json = serde_json::to_string(c).unwrap();
            let restored: PeerSyncOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(*c, restored);
            let bytes = bincode::serialize(c).unwrap();
            let restored_bin: PeerSyncOutcome = bincode::deserialize(&bytes).unwrap();
            assert_eq!(*c, restored_bin);
        }
    }

    #[test]
    fn peer_sync_outcome_as_str_is_stable() {
        // Lock the audit-row label set — these are NOT the serialized
        // wire labels (serde uses the externally-tagged representation
        // for the enum); these are stable strings for logs / audit rows.
        assert_eq!(PeerSyncOutcome::Matching.as_str(), "matching");
        assert_eq!(PeerSyncOutcome::MissingOnLocal.as_str(), "missing_on_local");
        assert_eq!(
            PeerSyncOutcome::MissingOnRemote.as_str(),
            "missing_on_remote"
        );
        assert_eq!(PeerSyncOutcome::Divergent.as_str(), "divergent");
        assert_eq!(
            PeerSyncOutcome::UnknownOutOfScope(UnknownOutOfScopeReason::StaleProbe).as_str(),
            "unknown_out_of_scope"
        );
    }

    #[test]
    fn unknown_out_of_scope_reason_all_four_variants_round_trip() {
        let cases = [
            UnknownOutOfScopeReason::StaleProbe,
            UnknownOutOfScopeReason::ScopeMismatch,
            UnknownOutOfScopeReason::InsufficientAuthority,
            UnknownOutOfScopeReason::UnknownStateClass,
        ];
        // Decisive evidence the spec's four skip cases are representable.
        assert_eq!(cases.len(), 4);
        for r in &cases {
            let json = serde_json::to_string(r).unwrap();
            let restored: UnknownOutOfScopeReason = serde_json::from_str(&json).unwrap();
            assert_eq!(*r, restored);
            let bytes = bincode::serialize(r).unwrap();
            let restored_bin: UnknownOutOfScopeReason = bincode::deserialize(&bytes).unwrap();
            assert_eq!(*r, restored_bin);
        }
    }

    #[test]
    fn peer_sync_outcome_may_carry_divergence_evidence_classification() {
        assert!(!PeerSyncOutcome::Matching.may_carry_divergence_evidence());
        assert!(PeerSyncOutcome::MissingOnLocal.may_carry_divergence_evidence());
        assert!(PeerSyncOutcome::MissingOnRemote.may_carry_divergence_evidence());
        assert!(PeerSyncOutcome::Divergent.may_carry_divergence_evidence());
        assert!(
            !PeerSyncOutcome::UnknownOutOfScope(UnknownOutOfScopeReason::ScopeMismatch)
                .may_carry_divergence_evidence()
        );
    }

    // ---- PeerSyncReport: binding determinism + round-trip ----

    #[test]
    fn peer_sync_report_binding_is_deterministic() {
        let r1 = sample_peer_sync_report();
        let r2 = sample_peer_sync_report();
        assert_eq!(r1.report_hash, r2.report_hash);
        assert_ne!(r1.report_hash, [0u8; 32]);
    }

    #[test]
    fn peer_sync_report_verify_binding_succeeds_for_fresh_record() {
        let r = sample_peer_sync_report();
        assert!(r.verify_binding());
    }

    #[test]
    fn peer_sync_report_json_round_trip_preserves_hash() {
        let original = sample_peer_sync_report();
        let json = serde_json::to_string(&original).unwrap();
        let restored: PeerSyncReport = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn peer_sync_report_bincode_round_trip_preserves_hash() {
        let original = sample_peer_sync_report();
        let bytes = bincode::serialize(&original).unwrap();
        let restored: PeerSyncReport = bincode::deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn peer_sync_report_signature_starts_empty() {
        let r = sample_peer_sync_report();
        assert!(r.signature.as_bytes().is_empty());
    }

    #[test]
    fn peer_sync_report_two_records_with_distinct_nonces_distinct_hashes() {
        let r1 = sample_peer_sync_report();
        let probe = sample_probe();
        let r2 = PeerSyncReport::new(
            probe.probe_hash,
            r1.sync_outcome,
            r1.state_class,
            r1.scope.clone(),
            r1.reporter_did.clone(),
            r1.counterparty_did.clone(),
            r1.local_digest.clone(),
            r1.observed_at,
            r1.freshness_valid_until,
            r1.divergence_evidence_hash,
            r1.private_content_implication,
            [0x99; 32], // distinct nonce
        )
        .unwrap();
        assert_ne!(r1.report_hash, r2.report_hash);
        assert!(r2.verify_binding());
    }

    #[test]
    fn peer_sync_report_freshness_helper() {
        let r = sample_peer_sync_report();
        assert!(r.is_fresh(r.observed_at));
        assert!(r.is_fresh(r.freshness_valid_until));
        assert!(!r.is_fresh(r.freshness_valid_until + 1));
    }

    // ---- PeerSyncReport: tamper detection for every bound field ----

    #[test]
    fn peer_sync_report_probe_hash_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.probe_hash = [0xFF; 32];
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_outcome_tamper_detected() {
        // Mutate from MissingOnRemote → MissingOnLocal; both pair
        // legally with divergence_evidence_hash=None, so the hash is
        // the only catch.
        let mut r = sample_peer_sync_report();
        r.sync_outcome = PeerSyncOutcome::MissingOnLocal;
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_state_class_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.state_class = StateClass::ArtifactRegistryMetadata;
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_scope_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.scope = ProbeScope::Commons;
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_reporter_did_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.reporter_did = "did:icn:attacker".to_string();
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_counterparty_did_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.counterparty_did = "did:icn:attacker".to_string();
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_local_digest_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.local_digest = StateDigest::Bloom(sample_bloom());
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_observed_at_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.observed_at += 1;
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_freshness_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.freshness_valid_until += 1;
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_divergence_link_tamper_detected() {
        // Build a Divergent-outcome report with no link, then manually
        // attach one. Both states are structurally legal for Divergent;
        // only the hash catches the change.
        let probe = sample_probe();
        let mut r = PeerSyncReport::new(
            probe.probe_hash,
            PeerSyncOutcome::Divergent,
            probe.state_class,
            probe.target_scope.clone(),
            "did:icn:responder456".to_string(),
            probe.prober_did.clone(),
            sample_responder_local_digest(),
            1_715_000_005,
            1_715_000_035,
            None,
            false,
            [0xAB; 32],
        )
        .unwrap();
        assert!(r.verify_binding());
        r.divergence_evidence_hash = Some([0x42; 32]);
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_private_implication_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.private_content_implication = !r.private_content_implication;
        assert!(!r.verify_binding());
    }

    #[test]
    fn peer_sync_report_nonce_tamper_detected() {
        let mut r = sample_peer_sync_report();
        r.report_nonce = [0xFF; 32];
        assert!(!r.verify_binding());
    }

    // ---- PeerSyncReport: schema-version policing ----

    #[derive(Serialize)]
    struct PeerSyncReportWireShape<'a> {
        schema_version: u32,
        probe_hash: Hash,
        sync_outcome: PeerSyncOutcome,
        state_class: StateClass,
        scope: &'a ProbeScope,
        reporter_did: &'a Did,
        counterparty_did: &'a Did,
        local_digest: &'a StateDigest,
        observed_at: u64,
        freshness_valid_until: u64,
        divergence_evidence_hash: Option<Hash>,
        private_content_implication: bool,
        report_nonce: [u8; 32],
        report_hash: Hash,
        signature: Signature,
    }

    #[test]
    fn peer_sync_report_rejects_future_schema_version_on_decode() {
        let r = sample_peer_sync_report();
        let bogus = PEER_SYNC_REPORT_SCHEMA_VERSION + 1;
        let recomputed = PeerSyncReport::compute_report_hash(
            bogus,
            r.probe_hash,
            r.sync_outcome,
            r.state_class,
            &r.scope,
            &r.reporter_did,
            &r.counterparty_did,
            &r.local_digest,
            r.observed_at,
            r.freshness_valid_until,
            r.divergence_evidence_hash,
            r.private_content_implication,
            &r.report_nonce,
        );
        let wire = PeerSyncReportWireShape {
            schema_version: bogus,
            probe_hash: r.probe_hash,
            sync_outcome: r.sync_outcome,
            state_class: r.state_class,
            scope: &r.scope,
            reporter_did: &r.reporter_did,
            counterparty_did: &r.counterparty_did,
            local_digest: &r.local_digest,
            observed_at: r.observed_at,
            freshness_valid_until: r.freshness_valid_until,
            divergence_evidence_hash: r.divergence_evidence_hash,
            private_content_implication: r.private_content_implication,
            report_nonce: r.report_nonce,
            report_hash: recomputed,
            signature: r.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<PeerSyncReport, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed_bin: Result<PeerSyncReport, _> = bincode::deserialize(&bytes);
        assert!(parsed_bin.is_err());
    }

    #[test]
    fn peer_sync_report_verify_binding_fails_closed_on_manual_bogus_version() {
        let mut r = sample_peer_sync_report();
        let bogus = PEER_SYNC_REPORT_SCHEMA_VERSION + 9;
        r.schema_version = bogus;
        r.report_hash = PeerSyncReport::compute_report_hash(
            bogus,
            r.probe_hash,
            r.sync_outcome,
            r.state_class,
            &r.scope,
            &r.reporter_did,
            &r.counterparty_did,
            &r.local_digest,
            r.observed_at,
            r.freshness_valid_until,
            r.divergence_evidence_hash,
            r.private_content_implication,
            &r.report_nonce,
        );
        assert!(
            !r.verify_binding(),
            "verify_binding() must reject unsupported schema_version even when the hash matches"
        );
    }

    // ---- PeerSyncReport: outcome / divergence-link structural rules ----

    fn _build_with_outcome(
        outcome: PeerSyncOutcome,
        divergence_hash: Option<Hash>,
    ) -> Result<PeerSyncReport, PeerSyncReportError> {
        let probe = sample_probe();
        PeerSyncReport::new(
            probe.probe_hash,
            outcome,
            probe.state_class,
            probe.target_scope.clone(),
            "did:icn:responder456".to_string(),
            probe.prober_did.clone(),
            sample_responder_local_digest(),
            1_715_000_005,
            1_715_000_035,
            divergence_hash,
            false,
            [0xAB; 32],
        )
    }

    #[test]
    fn peer_sync_report_matching_rejects_divergence_evidence_link() {
        let err = _build_with_outcome(PeerSyncOutcome::Matching, Some([0x42; 32])).unwrap_err();
        assert!(matches!(
            err,
            PeerSyncReportError::DivergenceEvidenceHashNotAllowed { outcome } if outcome == "matching"
        ));
    }

    #[test]
    fn peer_sync_report_unknown_out_of_scope_rejects_divergence_evidence_link() {
        for reason in [
            UnknownOutOfScopeReason::StaleProbe,
            UnknownOutOfScopeReason::ScopeMismatch,
            UnknownOutOfScopeReason::InsufficientAuthority,
            UnknownOutOfScopeReason::UnknownStateClass,
        ] {
            let err =
                _build_with_outcome(PeerSyncOutcome::UnknownOutOfScope(reason), Some([0x42; 32]))
                    .unwrap_err();
            assert!(matches!(
                err,
                PeerSyncReportError::DivergenceEvidenceHashNotAllowed {
                    outcome
                } if outcome == "unknown_out_of_scope"
            ));
        }
    }

    #[test]
    fn peer_sync_report_missing_on_local_may_carry_divergence_link() {
        let r = _build_with_outcome(PeerSyncOutcome::MissingOnLocal, Some([0x33; 32]))
            .expect("MissingOnLocal + divergence link is structurally legal");
        assert_eq!(r.divergence_evidence_hash, Some([0x33; 32]));
        assert!(r.verify_binding());
    }

    #[test]
    fn peer_sync_report_missing_on_remote_may_carry_divergence_link() {
        let r = _build_with_outcome(PeerSyncOutcome::MissingOnRemote, Some([0x33; 32]))
            .expect("MissingOnRemote + divergence link is structurally legal");
        assert_eq!(r.divergence_evidence_hash, Some([0x33; 32]));
        assert!(r.verify_binding());
    }

    #[test]
    fn peer_sync_report_divergent_may_carry_divergence_link() {
        let r = _build_with_outcome(PeerSyncOutcome::Divergent, Some([0x33; 32]))
            .expect("Divergent + divergence link is structurally legal");
        assert_eq!(r.divergence_evidence_hash, Some([0x33; 32]));
        assert!(r.verify_binding());
    }

    #[test]
    fn peer_sync_report_non_matching_outcomes_round_trip_without_link() {
        // Reports may be emitted before classification runs; the link
        // is optional for non-matching outcomes.
        for outcome in [
            PeerSyncOutcome::MissingOnLocal,
            PeerSyncOutcome::MissingOnRemote,
            PeerSyncOutcome::Divergent,
        ] {
            let r = _build_with_outcome(outcome, None)
                .unwrap_or_else(|_| panic!("{outcome:?} + None is structurally legal"));
            let json = serde_json::to_string(&r).unwrap();
            let restored: PeerSyncReport = serde_json::from_str(&json).unwrap();
            assert_eq!(r, restored);
            assert!(restored.verify_binding());
            assert!(restored.divergence_evidence_hash.is_none());
        }
    }

    #[test]
    fn peer_sync_report_wire_rejects_matching_with_divergence_link() {
        // Even a hash-consistent wire shape must be refused before
        // construction. Closes the "manually-crafted wire payload"
        // bypass.
        let probe = sample_probe();
        let scope = probe.target_scope.clone();
        let reporter = "did:icn:responder456".to_string();
        let counterparty = probe.prober_did.clone();
        let local_digest = sample_responder_local_digest();
        let recomputed = PeerSyncReport::compute_report_hash(
            PEER_SYNC_REPORT_SCHEMA_VERSION,
            probe.probe_hash,
            PeerSyncOutcome::Matching,
            probe.state_class,
            &scope,
            &reporter,
            &counterparty,
            &local_digest,
            1_715_000_005,
            1_715_000_035,
            Some([0x42; 32]),
            false,
            &[0xAB; 32],
        );
        let wire = PeerSyncReportWireShape {
            schema_version: PEER_SYNC_REPORT_SCHEMA_VERSION,
            probe_hash: probe.probe_hash,
            sync_outcome: PeerSyncOutcome::Matching,
            state_class: probe.state_class,
            scope: &scope,
            reporter_did: &reporter,
            counterparty_did: &counterparty,
            local_digest: &local_digest,
            observed_at: 1_715_000_005,
            freshness_valid_until: 1_715_000_035,
            divergence_evidence_hash: Some([0x42; 32]),
            private_content_implication: false,
            report_nonce: [0xAB; 32],
            report_hash: recomputed,
            signature: Signature::new(Vec::new()),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<PeerSyncReport, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed_bin: Result<PeerSyncReport, _> = bincode::deserialize(&bytes);
        assert!(parsed_bin.is_err());
    }

    // ---- PeerSyncReport: domain-tag separation ----

    #[test]
    fn peer_sync_report_domain_tag_distinct_from_other_records() {
        assert_ne!(PeerSyncReport::DOMAIN_TAG, AntiEntropyProbe::DOMAIN_TAG);
        assert_ne!(PeerSyncReport::DOMAIN_TAG, DivergenceEvidence::DOMAIN_TAG);
        assert_ne!(PeerSyncReport::DOMAIN_TAG, RepairPlan::DOMAIN_TAG);
        assert_ne!(PeerSyncReport::DOMAIN_TAG, RepairReceipt::DOMAIN_TAG);
        assert_ne!(PeerSyncReport::DOMAIN_TAG, ArtifactReceipt::DOMAIN_TAG);
    }

    #[test]
    fn peer_sync_report_domain_tag_affects_hash() {
        let r = sample_peer_sync_report();
        let binding = PeerSyncReportBinding {
            schema_version: r.schema_version,
            probe_hash: r.probe_hash,
            sync_outcome: r.sync_outcome,
            state_class: r.state_class,
            scope: &r.scope,
            reporter_did: &r.reporter_did,
            counterparty_did: &r.counterparty_did,
            local_digest: &r.local_digest,
            observed_at: r.observed_at,
            freshness_valid_until: r.freshness_valid_until,
            divergence_evidence_hash: r.divergence_evidence_hash,
            private_content_implication: r.private_content_implication,
            report_nonce: r.report_nonce,
        };
        let payload = bincode::serialize(&binding).unwrap();
        let mut hasher = blake3::Hasher::new();
        // Deliberately omit DOMAIN_TAG.
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        let without_tag: Hash = *hasher.finalize().as_bytes();
        assert_ne!(r.report_hash, without_tag);
    }

    // ---- PeerSyncReport: cross-link to probe + privacy contract ----

    #[test]
    fn peer_sync_report_links_to_probe_hash() {
        let probe = sample_probe();
        let r = sample_peer_sync_report();
        assert_eq!(r.probe_hash, probe.probe_hash);
        // If the probe is rebuilt with a different nonce, the link breaks.
        let probe_v2 = AntiEntropyProbe::new(
            probe.state_class,
            probe.target_scope.clone(),
            probe.digest.clone(),
            probe.prober_did.clone(),
            probe.trigger_source,
            probe.freshness_emitted_at,
            probe.freshness_valid_until,
            probe.requested_response,
            [0xEE; 32], // distinct nonce
        );
        assert_ne!(r.probe_hash, probe_v2.probe_hash);
    }

    #[test]
    fn peer_sync_report_private_content_implication_round_trips() {
        let probe = sample_probe();
        let r = PeerSyncReport::new(
            probe.probe_hash,
            PeerSyncOutcome::Divergent,
            probe.state_class,
            probe.target_scope.clone(),
            "did:icn:scoped-vault-steward".to_string(),
            probe.prober_did.clone(),
            // The responder's local digest is a bounded projection —
            // hashes / counts only, never raw object bodies. The
            // private_content_implication flag tells renderers to
            // gate technical detail.
            StateDigest::ShortList(ShortDigestList::from_hashes(vec![
                [0xAA; 32], [0xBB; 32], [0xCC; 32],
            ])),
            1_715_000_005,
            1_715_000_035,
            None,
            true,
            [0x55; 32],
        )
        .expect("private-content report is structurally consistent");
        assert!(r.private_content_implication);
        assert!(r.verify_binding());
        // Structurally: the only field that could carry private data is
        // `local_digest`, which is a `StateDigest` enum whose every
        // projection is hashes / counts / Merkle root / vector clock.
        // There is no body / payload / raw_bytes / secret field on the
        // record.
        let json = serde_json::to_string(&r).unwrap();
        let restored: PeerSyncReport = serde_json::from_str(&json).unwrap();
        assert!(restored.private_content_implication);
        assert!(restored.verify_binding());
    }

    // =========================================================================
    // SyncDegradedStatus (issue #1856)
    // =========================================================================

    fn sample_sync_degraded_status() -> SyncDegradedStatus {
        let report = sample_peer_sync_report();
        // The sample report is `MissingOnRemote`, which boundary rule 5
        // does NOT permit as a degradation trigger. The status sample
        // therefore uses a `MissingOnLocal`-shaped scenario: build a
        // dedicated report and derive the status from it.
        let probe = sample_probe();
        let local_report = PeerSyncReport::new(
            probe.probe_hash,
            PeerSyncOutcome::MissingOnLocal,
            probe.state_class,
            probe.target_scope.clone(),
            "did:icn:responder789".to_string(),
            probe.prober_did.clone(),
            sample_responder_local_digest(),
            1_715_000_005,
            1_715_000_035,
            None,
            false,
            [0xBA; 32],
        )
        .expect("MissingOnLocal report is structurally consistent");
        // The status itself; note `peer_sync_report_hash` cross-links to
        // `local_report`, not `report` (which uses MissingOnRemote).
        let _ = report; // suppress unused warning while keeping the symbol
        SyncDegradedStatus::new(
            local_report.report_hash,
            PeerSyncOutcome::MissingOnLocal,
            local_report.state_class,
            local_report.scope.clone(),
            "did:icn:policy-oracle".to_string(),
            DegradationLevel::WithinGraceWindow,
            1_715_000_010,
            1_715_000_070,
            None,
            false,
            [0x77; 32],
        )
        .expect("sample SyncDegradedStatus is structurally consistent")
    }

    // ---- DegradationLevel + Outcome eligibility round-trips ----

    #[test]
    fn degradation_level_all_variants_round_trip() {
        let cases = [
            DegradationLevel::WithinGraceWindow,
            DegradationLevel::BeyondGraceWindow,
        ];
        assert_eq!(cases.len(), 2);
        for d in &cases {
            let json = serde_json::to_string(d).unwrap();
            let restored: DegradationLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(*d, restored);
            let bytes = bincode::serialize(d).unwrap();
            let restored_bin: DegradationLevel = bincode::deserialize(&bytes).unwrap();
            assert_eq!(*d, restored_bin);
        }
    }

    #[test]
    fn degradation_level_as_str_is_stable() {
        assert_eq!(
            DegradationLevel::WithinGraceWindow.as_str(),
            "within_grace_window"
        );
        assert_eq!(
            DegradationLevel::BeyondGraceWindow.as_str(),
            "beyond_grace_window"
        );
    }

    // ---- SyncDegradedStatus: binding determinism + round-trip ----

    #[test]
    fn sync_degraded_status_binding_is_deterministic() {
        let s1 = sample_sync_degraded_status();
        let s2 = sample_sync_degraded_status();
        assert_eq!(s1.status_hash, s2.status_hash);
        assert_ne!(s1.status_hash, [0u8; 32]);
    }

    #[test]
    fn sync_degraded_status_verify_binding_succeeds_for_fresh_record() {
        let s = sample_sync_degraded_status();
        assert!(s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_json_round_trip_preserves_hash() {
        let original = sample_sync_degraded_status();
        let json = serde_json::to_string(&original).unwrap();
        let restored: SyncDegradedStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn sync_degraded_status_bincode_round_trip_preserves_hash() {
        let original = sample_sync_degraded_status();
        let bytes = bincode::serialize(&original).unwrap();
        let restored: SyncDegradedStatus = bincode::deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn sync_degraded_status_freshness_helper() {
        let s = sample_sync_degraded_status();
        assert!(s.is_fresh(s.degraded_since));
        assert!(s.is_fresh(s.freshness_valid_until));
        assert!(!s.is_fresh(s.freshness_valid_until + 1));
    }

    #[test]
    fn sync_degraded_status_signature_starts_empty() {
        let s = sample_sync_degraded_status();
        assert!(s.signature.as_bytes().is_empty());
    }

    // ---- SyncDegradedStatus: tamper detection for every bound field ----

    #[test]
    fn sync_degraded_status_peer_sync_report_hash_tamper_detected() {
        let mut s = sample_sync_degraded_status();
        s.peer_sync_report_hash = [0xFF; 32];
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_triggered_by_outcome_tamper_detected() {
        // Both MissingOnLocal and Divergent are structurally legal
        // triggers; switching from one to the other keeps the receipt
        // structurally consistent and isolates the hash check.
        let mut s = sample_sync_degraded_status();
        s.triggered_by_outcome = PeerSyncOutcome::Divergent;
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_state_class_tamper_detected() {
        let mut s = sample_sync_degraded_status();
        s.state_class = StateClass::ArtifactRegistryMetadata;
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_scope_tamper_detected() {
        let mut s = sample_sync_degraded_status();
        s.scope = ProbeScope::Commons;
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_reporter_did_tamper_detected() {
        let mut s = sample_sync_degraded_status();
        s.reporter_did = "did:icn:attacker".to_string();
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_degradation_level_tamper_detected() {
        let mut s = sample_sync_degraded_status();
        s.degradation_level = DegradationLevel::BeyondGraceWindow;
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_degraded_since_tamper_detected() {
        let mut s = sample_sync_degraded_status();
        // Keep the value inside the freshness window so we don't trip
        // the structural rule before the hash check.
        s.degraded_since += 1;
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_freshness_tamper_detected() {
        let mut s = sample_sync_degraded_status();
        s.freshness_valid_until += 1;
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_divergence_link_tamper_detected() {
        // The sample uses divergence_evidence_hash=None. Manually
        // attach a link without recomputing the hash — verify_binding
        // must reject.
        let mut s = sample_sync_degraded_status();
        s.divergence_evidence_hash = Some([0x42; 32]);
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_private_implication_tamper_detected() {
        let mut s = sample_sync_degraded_status();
        s.private_content_implication = !s.private_content_implication;
        assert!(!s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_nonce_tamper_detected() {
        let mut s = sample_sync_degraded_status();
        s.status_nonce = [0xFF; 32];
        assert!(!s.verify_binding());
    }

    // ---- SyncDegradedStatus: schema-version policing ----

    #[derive(Serialize)]
    struct SyncDegradedStatusWireShape<'a> {
        schema_version: u32,
        peer_sync_report_hash: Hash,
        triggered_by_outcome: PeerSyncOutcome,
        state_class: StateClass,
        scope: &'a ProbeScope,
        reporter_did: &'a Did,
        degradation_level: DegradationLevel,
        degraded_since: u64,
        freshness_valid_until: u64,
        divergence_evidence_hash: Option<Hash>,
        private_content_implication: bool,
        status_nonce: [u8; 32],
        status_hash: Hash,
        signature: Signature,
    }

    #[test]
    fn sync_degraded_status_rejects_future_schema_version_on_decode() {
        let s = sample_sync_degraded_status();
        let bogus = SYNC_DEGRADED_STATUS_SCHEMA_VERSION + 1;
        let recomputed = SyncDegradedStatus::compute_status_hash(
            bogus,
            s.peer_sync_report_hash,
            s.triggered_by_outcome,
            s.state_class,
            &s.scope,
            &s.reporter_did,
            s.degradation_level,
            s.degraded_since,
            s.freshness_valid_until,
            s.divergence_evidence_hash,
            s.private_content_implication,
            &s.status_nonce,
        );
        let wire = SyncDegradedStatusWireShape {
            schema_version: bogus,
            peer_sync_report_hash: s.peer_sync_report_hash,
            triggered_by_outcome: s.triggered_by_outcome,
            state_class: s.state_class,
            scope: &s.scope,
            reporter_did: &s.reporter_did,
            degradation_level: s.degradation_level,
            degraded_since: s.degraded_since,
            freshness_valid_until: s.freshness_valid_until,
            divergence_evidence_hash: s.divergence_evidence_hash,
            private_content_implication: s.private_content_implication,
            status_nonce: s.status_nonce,
            status_hash: recomputed,
            signature: s.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<SyncDegradedStatus, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed_bin: Result<SyncDegradedStatus, _> = bincode::deserialize(&bytes);
        assert!(parsed_bin.is_err());
    }

    #[test]
    fn sync_degraded_status_verify_binding_fails_closed_on_manual_bogus_version() {
        let mut s = sample_sync_degraded_status();
        let bogus = SYNC_DEGRADED_STATUS_SCHEMA_VERSION + 13;
        s.schema_version = bogus;
        s.status_hash = SyncDegradedStatus::compute_status_hash(
            bogus,
            s.peer_sync_report_hash,
            s.triggered_by_outcome,
            s.state_class,
            &s.scope,
            &s.reporter_did,
            s.degradation_level,
            s.degraded_since,
            s.freshness_valid_until,
            s.divergence_evidence_hash,
            s.private_content_implication,
            &s.status_nonce,
        );
        assert!(
            !s.verify_binding(),
            "verify_binding() must reject unsupported schema_version even when the hash matches"
        );
    }

    // ---- SyncDegradedStatus: structural rule (boundary rule 5) ----

    fn _attempt_status_with_outcome(
        outcome: PeerSyncOutcome,
    ) -> Result<SyncDegradedStatus, SyncDegradedStatusError> {
        let report = sample_peer_sync_report();
        SyncDegradedStatus::new(
            report.report_hash,
            outcome,
            report.state_class,
            report.scope.clone(),
            "did:icn:policy-oracle".to_string(),
            DegradationLevel::WithinGraceWindow,
            1_715_000_010,
            1_715_000_070,
            None,
            false,
            [0x77; 32],
        )
    }

    #[test]
    fn sync_degraded_status_rejects_matching_outcome() {
        let err = _attempt_status_with_outcome(PeerSyncOutcome::Matching).unwrap_err();
        assert!(matches!(
            err,
            SyncDegradedStatusError::OutcomeNotEligibleForDegradation { outcome } if outcome == "matching"
        ));
    }

    #[test]
    fn sync_degraded_status_rejects_missing_on_remote_outcome() {
        let err = _attempt_status_with_outcome(PeerSyncOutcome::MissingOnRemote).unwrap_err();
        assert!(matches!(
            err,
            SyncDegradedStatusError::OutcomeNotEligibleForDegradation { outcome } if outcome == "missing_on_remote"
        ));
    }

    #[test]
    fn sync_degraded_status_rejects_every_unknown_out_of_scope_reason() {
        for reason in [
            UnknownOutOfScopeReason::StaleProbe,
            UnknownOutOfScopeReason::ScopeMismatch,
            UnknownOutOfScopeReason::InsufficientAuthority,
            UnknownOutOfScopeReason::UnknownStateClass,
        ] {
            let err = _attempt_status_with_outcome(PeerSyncOutcome::UnknownOutOfScope(reason))
                .unwrap_err();
            assert!(matches!(
                err,
                SyncDegradedStatusError::OutcomeNotEligibleForDegradation {
                    outcome
                } if outcome == "unknown_out_of_scope"
            ));
        }
    }

    #[test]
    fn sync_degraded_status_accepts_missing_on_local() {
        let s = _attempt_status_with_outcome(PeerSyncOutcome::MissingOnLocal)
            .expect("MissingOnLocal is the canonical trigger");
        assert!(s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_accepts_divergent() {
        let s = _attempt_status_with_outcome(PeerSyncOutcome::Divergent)
            .expect("Divergent is the other canonical trigger");
        assert!(s.verify_binding());
    }

    #[test]
    fn sync_degraded_status_rejects_degraded_since_after_freshness() {
        let report = sample_peer_sync_report();
        let err = SyncDegradedStatus::new(
            report.report_hash,
            PeerSyncOutcome::MissingOnLocal,
            report.state_class,
            report.scope.clone(),
            "did:icn:policy-oracle".to_string(),
            DegradationLevel::WithinGraceWindow,
            1_715_000_100, // degraded_since
            1_715_000_070, // freshness_valid_until < degraded_since
            None,
            false,
            [0x77; 32],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            SyncDegradedStatusError::DegradedSinceAfterFreshness { .. }
        ));
    }

    #[test]
    fn sync_degraded_status_wire_rejects_matching_outcome() {
        // Even a hash-consistent wire payload must be refused before
        // any value is constructed.
        let s = sample_sync_degraded_status();
        let recomputed = SyncDegradedStatus::compute_status_hash(
            SYNC_DEGRADED_STATUS_SCHEMA_VERSION,
            s.peer_sync_report_hash,
            PeerSyncOutcome::Matching,
            s.state_class,
            &s.scope,
            &s.reporter_did,
            s.degradation_level,
            s.degraded_since,
            s.freshness_valid_until,
            s.divergence_evidence_hash,
            s.private_content_implication,
            &s.status_nonce,
        );
        let wire = SyncDegradedStatusWireShape {
            schema_version: SYNC_DEGRADED_STATUS_SCHEMA_VERSION,
            peer_sync_report_hash: s.peer_sync_report_hash,
            triggered_by_outcome: PeerSyncOutcome::Matching,
            state_class: s.state_class,
            scope: &s.scope,
            reporter_did: &s.reporter_did,
            degradation_level: s.degradation_level,
            degraded_since: s.degraded_since,
            freshness_valid_until: s.freshness_valid_until,
            divergence_evidence_hash: s.divergence_evidence_hash,
            private_content_implication: s.private_content_implication,
            status_nonce: s.status_nonce,
            status_hash: recomputed,
            signature: s.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<SyncDegradedStatus, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
    }

    // ---- SyncDegradedStatus: cross-link to PeerSyncReport ----

    #[test]
    fn sync_degraded_status_links_to_peer_sync_report_hash() {
        // The status's peer_sync_report_hash must match the originating
        // report. If the report is rebuilt with a different nonce, the
        // link breaks.
        let probe = sample_probe();
        let report = PeerSyncReport::new(
            probe.probe_hash,
            PeerSyncOutcome::MissingOnLocal,
            probe.state_class,
            probe.target_scope.clone(),
            "did:icn:responder789".to_string(),
            probe.prober_did.clone(),
            sample_responder_local_digest(),
            1_715_000_005,
            1_715_000_035,
            None,
            false,
            [0xBA; 32],
        )
        .unwrap();
        let s = SyncDegradedStatus::new(
            report.report_hash,
            PeerSyncOutcome::MissingOnLocal,
            report.state_class,
            report.scope.clone(),
            "did:icn:policy-oracle".to_string(),
            DegradationLevel::WithinGraceWindow,
            1_715_000_010,
            1_715_000_070,
            None,
            false,
            [0x77; 32],
        )
        .unwrap();
        assert_eq!(s.peer_sync_report_hash, report.report_hash);
        let report_v2 = PeerSyncReport::new(
            probe.probe_hash,
            PeerSyncOutcome::MissingOnLocal,
            probe.state_class,
            probe.target_scope.clone(),
            "did:icn:responder789".to_string(),
            probe.prober_did.clone(),
            sample_responder_local_digest(),
            1_715_000_005,
            1_715_000_035,
            None,
            false,
            [0xEE; 32], // distinct nonce
        )
        .unwrap();
        assert_ne!(s.peer_sync_report_hash, report_v2.report_hash);
    }

    // ---- SyncDegradedStatus: domain-tag separation ----

    #[test]
    fn sync_degraded_status_domain_tag_distinct_from_other_records() {
        assert_ne!(SyncDegradedStatus::DOMAIN_TAG, PeerSyncReport::DOMAIN_TAG);
        assert_ne!(SyncDegradedStatus::DOMAIN_TAG, AntiEntropyProbe::DOMAIN_TAG);
        assert_ne!(
            SyncDegradedStatus::DOMAIN_TAG,
            DivergenceEvidence::DOMAIN_TAG
        );
        assert_ne!(SyncDegradedStatus::DOMAIN_TAG, RepairPlan::DOMAIN_TAG);
        assert_ne!(SyncDegradedStatus::DOMAIN_TAG, RepairReceipt::DOMAIN_TAG);
        assert_ne!(SyncDegradedStatus::DOMAIN_TAG, ArtifactReceipt::DOMAIN_TAG);
    }

    #[test]
    fn sync_degraded_status_domain_tag_affects_hash() {
        let s = sample_sync_degraded_status();
        let binding = SyncDegradedStatusBinding {
            schema_version: s.schema_version,
            peer_sync_report_hash: s.peer_sync_report_hash,
            triggered_by_outcome: s.triggered_by_outcome,
            state_class: s.state_class,
            scope: &s.scope,
            reporter_did: &s.reporter_did,
            degradation_level: s.degradation_level,
            degraded_since: s.degraded_since,
            freshness_valid_until: s.freshness_valid_until,
            divergence_evidence_hash: s.divergence_evidence_hash,
            private_content_implication: s.private_content_implication,
            status_nonce: s.status_nonce,
        };
        let payload = bincode::serialize(&binding).unwrap();
        let mut hasher = blake3::Hasher::new();
        // Deliberately omit DOMAIN_TAG.
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        let without_tag: Hash = *hasher.finalize().as_bytes();
        assert_ne!(s.status_hash, without_tag);
    }

    // ---- SyncDegradedStatus: optional divergence_evidence_hash ----

    #[test]
    fn sync_degraded_status_round_trips_with_divergence_link() {
        let report = sample_peer_sync_report();
        let s = SyncDegradedStatus::new(
            report.report_hash,
            PeerSyncOutcome::MissingOnLocal,
            report.state_class,
            report.scope.clone(),
            "did:icn:policy-oracle".to_string(),
            DegradationLevel::BeyondGraceWindow,
            1_715_000_010,
            1_715_000_070,
            Some([0x42; 32]),
            false,
            [0x88; 32],
        )
        .unwrap();
        assert_eq!(s.divergence_evidence_hash, Some([0x42; 32]));
        let json = serde_json::to_string(&s).unwrap();
        let restored: SyncDegradedStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(s, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn sync_degraded_status_round_trips_without_divergence_link() {
        let s = sample_sync_degraded_status();
        assert!(s.divergence_evidence_hash.is_none());
        let json = serde_json::to_string(&s).unwrap();
        let restored: SyncDegradedStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(s, restored);
    }

    // ---- SyncDegradedStatus: privacy ----

    #[test]
    fn sync_degraded_status_private_content_implication_round_trips() {
        let report = sample_peer_sync_report();
        let s = SyncDegradedStatus::new(
            report.report_hash,
            PeerSyncOutcome::MissingOnLocal,
            StateClass::ScopedVaultReference,
            ProbeScope::LocalDomain {
                domain_id: "fixture-care-plan-vault".to_string(),
            },
            "did:icn:scoped-vault-steward".to_string(),
            DegradationLevel::WithinGraceWindow,
            1_715_000_010,
            1_715_000_070,
            None,
            true,
            [0x55; 32],
        )
        .expect("private-content status is structurally consistent");
        assert!(s.private_content_implication);
        assert!(s.verify_binding());
        // Structurally: the record carries only cross-link hashes,
        // DIDs, timestamps, and categorical labels. There is no body
        // / payload / raw_bytes / secret field by construction.
        let json = serde_json::to_string(&s).unwrap();
        let restored: SyncDegradedStatus = serde_json::from_str(&json).unwrap();
        assert!(restored.private_content_implication);
        assert!(restored.verify_binding());
    }

    // =========================================================================
    // FederationSyncWindow + QuorumSyncCheck (issue #1860)
    // =========================================================================

    fn sample_federation_sync_window() -> FederationSyncWindow {
        FederationSyncWindow::new(StateClass::ReceiptIndex, 300)
            .expect("sample window has positive duration")
    }

    fn sample_quorum_peers() -> PeerSet {
        PeerSet::from_dids(vec![
            "did:icn:fed:peer-a".to_string(),
            "did:icn:fed:peer-b".to_string(),
            "did:icn:fed:peer-c".to_string(),
        ])
    }

    fn sample_quorum_sync_check() -> QuorumSyncCheck {
        let window = sample_federation_sync_window();
        let peers = sample_quorum_peers();
        let peer_count = peers.dids().len() as u32;
        QuorumSyncCheck::new(
            StateClass::ReceiptIndex,
            ProbeScope::Federation {
                federation_id: "fixture-federation".to_string(),
            },
            peers,
            StateDigest::Bloom(BloomProjection {
                bits: vec![0b0001_0101, 0, 0, 0, 0, 0, 0, 0],
                num_hashes: 1,
                size: 64,
                hint_count: 4,
            }),
            window,
            "did:icn:fed:reporter".to_string(),
            1_715_000_000,
            1_715_000_120,
            peer_count,
            2,
            false,
            [0xA1; 32],
        )
        .expect("sample QuorumSyncCheck is structurally consistent")
    }

    // ---- FederationSyncWindow: binding + tamper + schema-version ----

    #[test]
    fn federation_sync_window_binding_is_deterministic() {
        let w1 = sample_federation_sync_window();
        let w2 = sample_federation_sync_window();
        assert_eq!(w1.window_hash, w2.window_hash);
        assert_ne!(w1.window_hash, [0u8; 32]);
    }

    #[test]
    fn federation_sync_window_verify_binding_succeeds_for_fresh() {
        let w = sample_federation_sync_window();
        assert!(w.verify_binding());
    }

    #[test]
    fn federation_sync_window_json_round_trip_preserves_hash() {
        let w = sample_federation_sync_window();
        let json = serde_json::to_string(&w).unwrap();
        let restored: FederationSyncWindow = serde_json::from_str(&json).unwrap();
        assert_eq!(w, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn federation_sync_window_bincode_round_trip_preserves_hash() {
        let w = sample_federation_sync_window();
        let bytes = bincode::serialize(&w).unwrap();
        let restored: FederationSyncWindow = bincode::deserialize(&bytes).unwrap();
        assert_eq!(w, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn federation_sync_window_state_class_tamper_detected() {
        let mut w = sample_federation_sync_window();
        w.state_class = StateClass::ArtifactRegistryMetadata;
        assert!(!w.verify_binding());
    }

    #[test]
    fn federation_sync_window_duration_tamper_detected() {
        let mut w = sample_federation_sync_window();
        w.window_duration_secs += 1;
        assert!(!w.verify_binding());
    }

    #[test]
    fn federation_sync_window_rejects_zero_duration_at_construction() {
        let err = FederationSyncWindow::new(StateClass::ReceiptIndex, 0).unwrap_err();
        assert!(matches!(err, FederationSyncWindowError::ZeroWindowDuration));
    }

    #[derive(Serialize)]
    struct FederationSyncWindowWireShape {
        schema_version: u32,
        state_class: StateClass,
        window_duration_secs: u64,
        window_hash: Hash,
    }

    #[test]
    fn federation_sync_window_rejects_zero_duration_on_decode() {
        // Hash-consistent wire payload with zero duration must be
        // refused before any value is constructed.
        let recomputed = FederationSyncWindow::compute_window_hash(
            FEDERATION_SYNC_WINDOW_SCHEMA_VERSION,
            StateClass::ReceiptIndex,
            0,
        );
        let wire = FederationSyncWindowWireShape {
            schema_version: FEDERATION_SYNC_WINDOW_SCHEMA_VERSION,
            state_class: StateClass::ReceiptIndex,
            window_duration_secs: 0,
            window_hash: recomputed,
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<FederationSyncWindow, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed_bin: Result<FederationSyncWindow, _> = bincode::deserialize(&bytes);
        assert!(parsed_bin.is_err());
    }

    #[test]
    fn federation_sync_window_rejects_future_schema_version_on_decode() {
        let bogus = FEDERATION_SYNC_WINDOW_SCHEMA_VERSION + 1;
        let recomputed =
            FederationSyncWindow::compute_window_hash(bogus, StateClass::ReceiptIndex, 300);
        let wire = FederationSyncWindowWireShape {
            schema_version: bogus,
            state_class: StateClass::ReceiptIndex,
            window_duration_secs: 300,
            window_hash: recomputed,
        };
        let json = serde_json::to_string(&wire).unwrap();
        assert!(serde_json::from_str::<FederationSyncWindow>(&json).is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        assert!(bincode::deserialize::<FederationSyncWindow>(&bytes).is_err());
    }

    #[test]
    fn federation_sync_window_verify_binding_fails_closed_on_manual_bogus_version() {
        let mut w = sample_federation_sync_window();
        let bogus = FEDERATION_SYNC_WINDOW_SCHEMA_VERSION + 7;
        w.schema_version = bogus;
        w.window_hash =
            FederationSyncWindow::compute_window_hash(bogus, w.state_class, w.window_duration_secs);
        assert!(
            !w.verify_binding(),
            "must fail closed even when the hash matches under bogus version"
        );
    }

    // ---- QuorumSyncCheck: binding + tamper ----

    #[test]
    fn quorum_sync_check_binding_is_deterministic() {
        let c1 = sample_quorum_sync_check();
        let c2 = sample_quorum_sync_check();
        assert_eq!(c1.check_hash, c2.check_hash);
        assert_ne!(c1.check_hash, [0u8; 32]);
    }

    #[test]
    fn quorum_sync_check_verify_binding_succeeds_for_fresh() {
        let c = sample_quorum_sync_check();
        assert!(c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_json_round_trip_preserves_hash() {
        let c = sample_quorum_sync_check();
        let json = serde_json::to_string(&c).unwrap();
        let restored: QuorumSyncCheck = serde_json::from_str(&json).unwrap();
        assert_eq!(c, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn quorum_sync_check_bincode_round_trip_preserves_hash() {
        let c = sample_quorum_sync_check();
        let bytes = bincode::serialize(&c).unwrap();
        let restored: QuorumSyncCheck = bincode::deserialize(&bytes).unwrap();
        assert_eq!(c, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn quorum_sync_check_signature_starts_empty() {
        let c = sample_quorum_sync_check();
        assert!(c.signature.as_bytes().is_empty());
    }

    #[test]
    fn quorum_sync_check_freshness_helper() {
        let c = sample_quorum_sync_check();
        assert!(c.is_fresh(c.observed_at));
        assert!(c.is_fresh(c.freshness_valid_until));
        assert!(!c.is_fresh(c.freshness_valid_until + 1));
    }

    #[test]
    fn quorum_sync_check_state_class_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.state_class = StateClass::ArtifactRegistryMetadata;
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_federation_scope_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.federation_scope = ProbeScope::Commons;
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_participating_peers_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.participating_peers = PeerSet::from_dids(vec!["did:icn:attacker".to_string()]);
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_matching_digest_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.matching_digest = StateDigest::MerkleRoot(MerkleRootProjection {
            root: [0xFF; 32],
            leaf_count: 1,
        });
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_window_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.federation_sync_window =
            FederationSyncWindow::new(StateClass::ReceiptIndex, 600).unwrap();
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_reporter_did_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.reporter_did = "did:icn:attacker".to_string();
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_observed_at_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.observed_at += 1;
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_freshness_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.freshness_valid_until -= 1;
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_quorum_size_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.quorum_size += 1;
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_quorum_threshold_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.quorum_threshold -= 1;
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_private_implication_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.private_content_implication = !c.private_content_implication;
        assert!(!c.verify_binding());
    }

    #[test]
    fn quorum_sync_check_nonce_tamper_detected() {
        let mut c = sample_quorum_sync_check();
        c.check_nonce = [0xFF; 32];
        assert!(!c.verify_binding());
    }

    // ---- QuorumSyncCheck: structural rules ----

    fn _attempt_check(
        federation_scope: ProbeScope,
        peers: PeerSet,
        state_class: StateClass,
        window: FederationSyncWindow,
        observed_at: u64,
        freshness_valid_until: u64,
        quorum_size: u32,
        quorum_threshold: u32,
    ) -> Result<QuorumSyncCheck, QuorumSyncCheckError> {
        QuorumSyncCheck::new(
            state_class,
            federation_scope,
            peers,
            StateDigest::Bloom(BloomProjection {
                bits: vec![0b0001_0101, 0, 0, 0, 0, 0, 0, 0],
                num_hashes: 1,
                size: 64,
                hint_count: 4,
            }),
            window,
            "did:icn:fed:reporter".to_string(),
            observed_at,
            freshness_valid_until,
            quorum_size,
            quorum_threshold,
            false,
            [0xA1; 32],
        )
    }

    #[test]
    fn quorum_sync_check_rejects_local_domain_scope() {
        let err = _attempt_check(
            ProbeScope::LocalDomain {
                domain_id: "local".to_string(),
            },
            sample_quorum_peers(),
            StateClass::ReceiptIndex,
            sample_federation_sync_window(),
            1_715_000_000,
            1_715_000_120,
            3,
            2,
        )
        .unwrap_err();
        assert!(matches!(err, QuorumSyncCheckError::InvalidFederationScope));
    }

    #[test]
    fn quorum_sync_check_rejects_peer_pair_scope() {
        let err = _attempt_check(
            ProbeScope::PeerPair {
                left: "did:icn:a".to_string(),
                right: "did:icn:b".to_string(),
            },
            sample_quorum_peers(),
            StateClass::ReceiptIndex,
            sample_federation_sync_window(),
            1_715_000_000,
            1_715_000_120,
            3,
            2,
        )
        .unwrap_err();
        assert!(matches!(err, QuorumSyncCheckError::InvalidFederationScope));
    }

    #[test]
    fn quorum_sync_check_accepts_federation_scope() {
        _attempt_check(
            ProbeScope::Federation {
                federation_id: "f1".to_string(),
            },
            sample_quorum_peers(),
            StateClass::ReceiptIndex,
            sample_federation_sync_window(),
            1_715_000_000,
            1_715_000_120,
            3,
            2,
        )
        .expect("federation scope is valid");
    }

    #[test]
    fn quorum_sync_check_accepts_commons_scope() {
        _attempt_check(
            ProbeScope::Commons,
            sample_quorum_peers(),
            StateClass::ReceiptIndex,
            sample_federation_sync_window(),
            1_715_000_000,
            1_715_000_120,
            3,
            2,
        )
        .expect("commons scope is valid");
    }

    #[test]
    fn quorum_sync_check_rejects_threshold_above_size() {
        let err = _attempt_check(
            ProbeScope::Federation {
                federation_id: "f1".to_string(),
            },
            sample_quorum_peers(),
            StateClass::ReceiptIndex,
            sample_federation_sync_window(),
            1_715_000_000,
            1_715_000_120,
            3,
            5, // threshold > size
        )
        .unwrap_err();
        assert!(matches!(
            err,
            QuorumSyncCheckError::QuorumThresholdNotMet {
                quorum_size: 3,
                quorum_threshold: 5,
            }
        ));
    }

    #[test]
    fn quorum_sync_check_rejects_size_peer_count_mismatch() {
        let err = _attempt_check(
            ProbeScope::Federation {
                federation_id: "f1".to_string(),
            },
            sample_quorum_peers(), // 3 peers
            StateClass::ReceiptIndex,
            sample_federation_sync_window(),
            1_715_000_000,
            1_715_000_120,
            2, // size disagrees with peer count
            2,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            QuorumSyncCheckError::QuorumSizePeerCountMismatch {
                quorum_size: 2,
                peers_len: 3,
            }
        ));
    }

    #[test]
    fn quorum_sync_check_rejects_observed_after_freshness() {
        let err = _attempt_check(
            ProbeScope::Federation {
                federation_id: "f1".to_string(),
            },
            sample_quorum_peers(),
            StateClass::ReceiptIndex,
            sample_federation_sync_window(),
            1_715_000_200, // observed_at
            1_715_000_120, // freshness < observed
            3,
            2,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            QuorumSyncCheckError::ObservedAfterFreshness { .. }
        ));
    }

    #[test]
    fn quorum_sync_check_rejects_freshness_exceeding_window() {
        // window = 300, span = 400
        let err = _attempt_check(
            ProbeScope::Federation {
                federation_id: "f1".to_string(),
            },
            sample_quorum_peers(),
            StateClass::ReceiptIndex,
            sample_federation_sync_window(),
            1_715_000_000,
            1_715_000_400, // span = 400 > 300
            3,
            2,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            QuorumSyncCheckError::FreshnessExceedsPolicyWindow {
                span_secs: 400,
                window_secs: 300,
            }
        ));
    }

    #[test]
    fn quorum_sync_check_rejects_window_state_class_mismatch() {
        // window is for ReceiptIndex; check declares ArtifactRegistryMetadata
        let err = _attempt_check(
            ProbeScope::Federation {
                federation_id: "f1".to_string(),
            },
            sample_quorum_peers(),
            StateClass::ArtifactRegistryMetadata,
            sample_federation_sync_window(),
            1_715_000_000,
            1_715_000_120,
            3,
            2,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            QuorumSyncCheckError::WindowStateClassMismatch { .. }
        ));
    }

    // ---- QuorumSyncCheck: schema-version policing ----

    #[derive(Serialize)]
    struct QuorumSyncCheckWireShape<'a> {
        schema_version: u32,
        state_class: StateClass,
        federation_scope: &'a ProbeScope,
        participating_peers: &'a PeerSet,
        matching_digest: &'a StateDigest,
        federation_sync_window: &'a FederationSyncWindow,
        reporter_did: &'a Did,
        observed_at: u64,
        freshness_valid_until: u64,
        quorum_size: u32,
        quorum_threshold: u32,
        private_content_implication: bool,
        check_nonce: [u8; 32],
        check_hash: Hash,
        signature: Signature,
    }

    #[test]
    fn quorum_sync_check_rejects_future_schema_version_on_decode() {
        let c = sample_quorum_sync_check();
        let bogus = QUORUM_SYNC_CHECK_SCHEMA_VERSION + 1;
        let recomputed = QuorumSyncCheck::compute_check_hash(
            bogus,
            c.state_class,
            &c.federation_scope,
            &c.participating_peers,
            &c.matching_digest,
            &c.federation_sync_window,
            &c.reporter_did,
            c.observed_at,
            c.freshness_valid_until,
            c.quorum_size,
            c.quorum_threshold,
            c.private_content_implication,
            &c.check_nonce,
        );
        let wire = QuorumSyncCheckWireShape {
            schema_version: bogus,
            state_class: c.state_class,
            federation_scope: &c.federation_scope,
            participating_peers: &c.participating_peers,
            matching_digest: &c.matching_digest,
            federation_sync_window: &c.federation_sync_window,
            reporter_did: &c.reporter_did,
            observed_at: c.observed_at,
            freshness_valid_until: c.freshness_valid_until,
            quorum_size: c.quorum_size,
            quorum_threshold: c.quorum_threshold,
            private_content_implication: c.private_content_implication,
            check_nonce: c.check_nonce,
            check_hash: recomputed,
            signature: c.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        assert!(serde_json::from_str::<QuorumSyncCheck>(&json).is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        assert!(bincode::deserialize::<QuorumSyncCheck>(&bytes).is_err());
    }

    #[test]
    fn quorum_sync_check_verify_binding_fails_closed_on_manual_bogus_version() {
        let mut c = sample_quorum_sync_check();
        let bogus = QUORUM_SYNC_CHECK_SCHEMA_VERSION + 11;
        c.schema_version = bogus;
        c.check_hash = QuorumSyncCheck::compute_check_hash(
            bogus,
            c.state_class,
            &c.federation_scope,
            &c.participating_peers,
            &c.matching_digest,
            &c.federation_sync_window,
            &c.reporter_did,
            c.observed_at,
            c.freshness_valid_until,
            c.quorum_size,
            c.quorum_threshold,
            c.private_content_implication,
            &c.check_nonce,
        );
        assert!(
            !c.verify_binding(),
            "must fail closed even when hash matches under bogus version"
        );
    }

    #[test]
    fn quorum_sync_check_wire_rejects_local_domain_scope() {
        // Hash-consistent wire payload with LocalDomain scope must be
        // refused before any value is constructed.
        let c = sample_quorum_sync_check();
        let bad_scope = ProbeScope::LocalDomain {
            domain_id: "local".to_string(),
        };
        let recomputed = QuorumSyncCheck::compute_check_hash(
            QUORUM_SYNC_CHECK_SCHEMA_VERSION,
            c.state_class,
            &bad_scope,
            &c.participating_peers,
            &c.matching_digest,
            &c.federation_sync_window,
            &c.reporter_did,
            c.observed_at,
            c.freshness_valid_until,
            c.quorum_size,
            c.quorum_threshold,
            c.private_content_implication,
            &c.check_nonce,
        );
        let wire = QuorumSyncCheckWireShape {
            schema_version: QUORUM_SYNC_CHECK_SCHEMA_VERSION,
            state_class: c.state_class,
            federation_scope: &bad_scope,
            participating_peers: &c.participating_peers,
            matching_digest: &c.matching_digest,
            federation_sync_window: &c.federation_sync_window,
            reporter_did: &c.reporter_did,
            observed_at: c.observed_at,
            freshness_valid_until: c.freshness_valid_until,
            quorum_size: c.quorum_size,
            quorum_threshold: c.quorum_threshold,
            private_content_implication: c.private_content_implication,
            check_nonce: c.check_nonce,
            check_hash: recomputed,
            signature: c.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        assert!(serde_json::from_str::<QuorumSyncCheck>(&json).is_err());
    }

    // ---- Domain-tag separation ----

    #[test]
    fn quorum_sync_check_and_window_domain_tags_distinct() {
        assert_ne!(
            QuorumSyncCheck::DOMAIN_TAG,
            FederationSyncWindow::DOMAIN_TAG
        );
        assert_ne!(QuorumSyncCheck::DOMAIN_TAG, PeerSyncReport::DOMAIN_TAG);
        assert_ne!(QuorumSyncCheck::DOMAIN_TAG, AntiEntropyProbe::DOMAIN_TAG);
        assert_ne!(QuorumSyncCheck::DOMAIN_TAG, DivergenceEvidence::DOMAIN_TAG);
        assert_ne!(QuorumSyncCheck::DOMAIN_TAG, RepairPlan::DOMAIN_TAG);
        assert_ne!(QuorumSyncCheck::DOMAIN_TAG, RepairReceipt::DOMAIN_TAG);
        assert_ne!(QuorumSyncCheck::DOMAIN_TAG, SyncDegradedStatus::DOMAIN_TAG);
        assert_ne!(QuorumSyncCheck::DOMAIN_TAG, ArtifactReceipt::DOMAIN_TAG);
        assert_ne!(
            FederationSyncWindow::DOMAIN_TAG,
            AntiEntropyProbe::DOMAIN_TAG
        );
        assert_ne!(FederationSyncWindow::DOMAIN_TAG, RepairReceipt::DOMAIN_TAG);
    }

    // ---- QuorumSyncCheck: privacy ----

    #[test]
    fn quorum_sync_check_private_content_implication_round_trips() {
        let window = FederationSyncWindow::new(StateClass::ScopedVaultReference, 120).unwrap();
        let peers = sample_quorum_peers();
        let peer_count = peers.dids().len() as u32;
        let c = QuorumSyncCheck::new(
            StateClass::ScopedVaultReference,
            ProbeScope::Federation {
                federation_id: "fixture-federation".to_string(),
            },
            peers,
            StateDigest::ShortList(ShortDigestList::from_hashes(vec![
                [0xAA; 32], [0xBB; 32], [0xCC; 32],
            ])),
            window,
            "did:icn:scoped-vault-steward".to_string(),
            1_715_000_000,
            1_715_000_120,
            peer_count,
            2,
            true,
            [0x55; 32],
        )
        .expect("private-content quorum check is structurally consistent");
        assert!(c.private_content_implication);
        assert!(c.verify_binding());
        let json = serde_json::to_string(&c).unwrap();
        let restored: QuorumSyncCheck = serde_json::from_str(&json).unwrap();
        assert!(restored.private_content_implication);
        assert!(restored.verify_binding());
    }
}

#[cfg(test)]
mod divergence_and_repair_tests {
    use super::*;

    // ---- Helpers ----

    fn sample_policy_clause() -> PolicyClauseRef {
        PolicyClauseRef {
            policy_id: "compute-placement".to_string(),
            policy_version_id: "v1-fixture".to_string(),
            clause_id: "boundary-rules.4".to_string(),
        }
    }

    fn sample_peers() -> PeerSet {
        PeerSet::from_dids(vec![
            "did:icn:peer-b".to_string(),
            "did:icn:peer-a".to_string(),
        ])
    }

    fn sample_digest_mismatch_missing_on_remote() -> DigestMismatch {
        DigestMismatch::MissingOnRemote {
            local: StateDigest::ShortList(ShortDigestList::from_hashes(vec![[0x01; 32]])),
        }
    }

    fn sample_evidence() -> DivergenceEvidence {
        DivergenceEvidence::new(
            DivergenceClass::MissingReceipt,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            sample_peers(),
            sample_digest_mismatch_missing_on_remote(),
            sample_policy_clause(),
            1_715_000_000,
            1_715_000_030,
            false,
            [0xAB; 32],
        )
    }

    fn sample_plan(evidence_hash: Hash) -> RepairPlan {
        RepairPlan::new(
            RepairAction::FetchMissing,
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            BoundaryRuleSet::from_rules(vec![
                BoundaryRuleRef::NoRepairBeyondAuthority,
                BoundaryRuleRef::NoLocalityOrDisclosureWidening,
            ]),
            ExpectedRepairReceiptClass::FetchMissingReceipt,
            evidence_hash,
            1_715_000_001,
            1_715_000_031,
            [0xCD; 32],
        )
    }

    // ---- DivergenceClass — all 18 classes representable + snake_case names ----

    #[test]
    fn divergence_class_all_eighteen_round_trip() {
        let all = [
            DivergenceClass::MissingReceipt,
            DivergenceClass::ConflictingReceipt,
            DivergenceClass::MissingArtifactMetadata,
            DivergenceClass::ContentHashMismatch,
            DivergenceClass::ReplicaLag,
            DivergenceClass::ReplicaMissing,
            DivergenceClass::BackupVerificationFailure,
            DivergenceClass::RestoreDrillMissing,
            DivergenceClass::PeerBehindSyncWindow,
            DivergenceClass::PeerEquivocation,
            DivergenceClass::FederationAgreementMismatch,
            DivergenceClass::CclPolicyVersionMismatch,
            DivergenceClass::EvaluatorBindingMismatch,
            DivergenceClass::PlacementEvidenceMissing,
            DivergenceClass::SettlementRecordMismatch,
            DivergenceClass::PrivateObjectReferenceMismatchWithoutContentDisclosure,
            DivergenceClass::IntegrityPolicyViolation,
            DivergenceClass::Unclassifiable,
        ];
        assert_eq!(all.len(), 18);
        for c in &all {
            let json = serde_json::to_string(c).unwrap();
            let parsed: DivergenceClass = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, *c);
        }
    }

    #[test]
    fn divergence_class_serde_names_are_snake_case_and_stable() {
        // Lock the wire names so future renaming forces a deliberate
        // schema bump rather than silent breakage.
        let cases: &[(DivergenceClass, &str)] = &[
            (DivergenceClass::MissingReceipt, "\"missing_receipt\""),
            (
                DivergenceClass::ConflictingReceipt,
                "\"conflicting_receipt\"",
            ),
            (DivergenceClass::PeerEquivocation, "\"peer_equivocation\""),
            (
                DivergenceClass::PrivateObjectReferenceMismatchWithoutContentDisclosure,
                "\"private_object_reference_mismatch_without_content_disclosure\"",
            ),
            (DivergenceClass::Unclassifiable, "\"unclassifiable\""),
        ];
        for (c, expected) in cases {
            assert_eq!(
                serde_json::to_string(c).unwrap(),
                *expected,
                "stable wire name for {c:?}"
            );
        }
    }

    // ---- PeerSet canonicalization ----

    #[test]
    fn peer_set_sorts_and_dedupes_on_construction() {
        let p = PeerSet::from_dids(vec![
            "did:icn:c".to_string(),
            "did:icn:a".to_string(),
            "did:icn:b".to_string(),
            "did:icn:a".to_string(), // dup
        ]);
        let dids: Vec<&str> = p.dids().iter().map(String::as_str).collect();
        assert_eq!(dids, vec!["did:icn:a", "did:icn:b", "did:icn:c"]);
    }

    #[test]
    fn peer_set_normalizes_unsorted_wire_input() {
        let json = r#"{"dids":["did:icn:c","did:icn:a","did:icn:b","did:icn:a"]}"#;
        let p: PeerSet = serde_json::from_str(json).unwrap();
        let dids: Vec<&str> = p.dids().iter().map(String::as_str).collect();
        assert_eq!(dids, vec!["did:icn:a", "did:icn:b", "did:icn:c"]);
    }

    #[test]
    fn peer_set_normalizes_bincode_wire_input() {
        #[derive(Serialize)]
        struct LocalBadWire {
            dids: Vec<Did>,
        }
        let bad = LocalBadWire {
            dids: vec![
                "did:icn:c".to_string(),
                "did:icn:a".to_string(),
                "did:icn:b".to_string(),
                "did:icn:a".to_string(),
            ],
        };
        let bytes = bincode::serialize(&bad).unwrap();
        let p: PeerSet = bincode::deserialize(&bytes).unwrap();
        let dids: Vec<&str> = p.dids().iter().map(String::as_str).collect();
        assert_eq!(dids, vec!["did:icn:a", "did:icn:b", "did:icn:c"]);
    }

    // ---- BoundaryRuleSet canonicalization ----

    #[test]
    fn boundary_rule_set_sorts_and_dedupes_on_construction() {
        let s = BoundaryRuleSet::from_rules(vec![
            BoundaryRuleRef::NoLocalityOrDisclosureWidening,
            BoundaryRuleRef::NoRepairBeyondAuthority,
            BoundaryRuleRef::NoLocalityOrDisclosureWidening, // dup
        ]);
        // The PartialOrd derives use declaration order, so
        // NoRepairBeyondAuthority comes before NoLocalityOrDisclosureWidening.
        assert_eq!(s.rules().len(), 2);
        assert!(s.rules().windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn boundary_rule_set_normalizes_unsorted_wire_input() {
        let json = r#"{"rules":["no_locality_or_disclosure_widening","no_repair_beyond_authority","no_locality_or_disclosure_widening"]}"#;
        let s: BoundaryRuleSet = serde_json::from_str(json).unwrap();
        assert_eq!(s.rules().len(), 2);
        assert!(s.rules().windows(2).all(|w| w[0] < w[1]));
    }

    // ---- DigestMismatch round-trip ----

    #[test]
    fn digest_mismatch_all_variants_round_trip() {
        let cases = [
            DigestMismatch::MissingOnRemote {
                local: StateDigest::ShortList(ShortDigestList::from_hashes(vec![[0x01; 32]])),
            },
            DigestMismatch::MissingOnLocal {
                remote: StateDigest::MerkleRoot(MerkleRootProjection {
                    root: [0x02; 32],
                    leaf_count: 1,
                }),
            },
            DigestMismatch::Differs {
                local: StateDigest::ShortList(ShortDigestList::from_hashes(vec![[0x03; 32]])),
                remote: StateDigest::ShortList(ShortDigestList::from_hashes(vec![[0x04; 32]])),
            },
            DigestMismatch::NotApplicable,
        ];
        for m in &cases {
            let json = serde_json::to_string(m).unwrap();
            let restored: DigestMismatch = serde_json::from_str(&json).unwrap();
            assert_eq!(*m, restored);

            let bytes = bincode::serialize(m).unwrap();
            let restored_bin: DigestMismatch = bincode::deserialize(&bytes).unwrap();
            assert_eq!(*m, restored_bin);
        }
    }

    // ---- DivergenceEvidence: binding, round-trip, tamper detection ----

    #[test]
    fn evidence_binding_is_deterministic() {
        let e1 = sample_evidence();
        let e2 = sample_evidence();
        assert_eq!(e1.evidence_hash, e2.evidence_hash);
        assert_ne!(e1.evidence_hash, [0u8; 32]);
    }

    #[test]
    fn evidence_verify_binding_succeeds_for_fresh_record() {
        let e = sample_evidence();
        assert!(e.verify_binding());
    }

    #[test]
    fn evidence_json_round_trip_preserves_hash() {
        let original = sample_evidence();
        let json = serde_json::to_string(&original).unwrap();
        let restored: DivergenceEvidence = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn evidence_bincode_round_trip_preserves_hash() {
        let original = sample_evidence();
        let bytes = bincode::serialize(&original).unwrap();
        let restored: DivergenceEvidence = bincode::deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn evidence_class_tamper_detected() {
        let mut e = sample_evidence();
        e.divergence_class = DivergenceClass::Unclassifiable;
        assert!(!e.verify_binding());
    }

    #[test]
    fn evidence_state_class_tamper_detected() {
        let mut e = sample_evidence();
        e.affected_state_class = StateClass::SettlementRecordIndex;
        assert!(!e.verify_binding());
    }

    #[test]
    fn evidence_scope_tamper_detected() {
        let mut e = sample_evidence();
        e.scope = ProbeScope::Commons;
        assert!(!e.verify_binding());
    }

    #[test]
    fn evidence_peers_tamper_detected() {
        let mut e = sample_evidence();
        e.peers = PeerSet::from_dids(vec!["did:icn:attacker".to_string()]);
        assert!(!e.verify_binding());
    }

    #[test]
    fn evidence_digest_mismatch_tamper_detected() {
        let mut e = sample_evidence();
        e.digest_mismatch = DigestMismatch::NotApplicable;
        assert!(!e.verify_binding());
    }

    #[test]
    fn evidence_policy_clause_tamper_detected() {
        let mut e = sample_evidence();
        e.policy_clause = PolicyClauseRef {
            policy_id: "other".to_string(),
            policy_version_id: "vX".to_string(),
            clause_id: "evil".to_string(),
        };
        assert!(!e.verify_binding());
    }

    #[test]
    fn evidence_freshness_tamper_detected() {
        let mut e = sample_evidence();
        e.freshness_emitted_at += 1;
        assert!(!e.verify_binding());

        let mut e = sample_evidence();
        e.freshness_valid_until += 1;
        assert!(!e.verify_binding());
    }

    #[test]
    fn evidence_private_implication_tamper_detected() {
        let mut e = sample_evidence();
        e.private_content_implication = !e.private_content_implication;
        assert!(!e.verify_binding());
    }

    #[test]
    fn evidence_nonce_tamper_detected() {
        let mut e = sample_evidence();
        e.evidence_nonce = [0xFF; 32];
        assert!(!e.verify_binding());
    }

    // ---- DivergenceEvidence: schema-version policing ----

    #[test]
    fn evidence_rejects_future_schema_version_on_json_decode() {
        // Build a wire payload with an unsupported schema_version and a
        // hash recomputed under that bogus version. The deserializer must
        // refuse to construct a DivergenceEvidence at all.
        #[derive(Serialize)]
        struct WireShape<'a> {
            schema_version: u32,
            divergence_class: DivergenceClass,
            affected_state_class: StateClass,
            scope: &'a ProbeScope,
            peers: &'a PeerSet,
            digest_mismatch: &'a DigestMismatch,
            policy_clause: &'a PolicyClauseRef,
            freshness_emitted_at: u64,
            freshness_valid_until: u64,
            private_content_implication: bool,
            evidence_nonce: [u8; 32],
            evidence_hash: Hash,
            signature: Signature,
        }
        let e = sample_evidence();
        let bogus = DIVERGENCE_EVIDENCE_SCHEMA_VERSION + 1;
        let recomputed = DivergenceEvidence::compute_evidence_hash(
            bogus,
            e.divergence_class,
            e.affected_state_class,
            &e.scope,
            &e.peers,
            &e.digest_mismatch,
            &e.policy_clause,
            e.freshness_emitted_at,
            e.freshness_valid_until,
            e.private_content_implication,
            &e.evidence_nonce,
        );
        let wire = WireShape {
            schema_version: bogus,
            divergence_class: e.divergence_class,
            affected_state_class: e.affected_state_class,
            scope: &e.scope,
            peers: &e.peers,
            digest_mismatch: &e.digest_mismatch,
            policy_clause: &e.policy_clause,
            freshness_emitted_at: e.freshness_emitted_at,
            freshness_valid_until: e.freshness_valid_until,
            private_content_implication: e.private_content_implication,
            evidence_nonce: e.evidence_nonce,
            evidence_hash: recomputed,
            signature: e.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<DivergenceEvidence, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed_bin: Result<DivergenceEvidence, _> = bincode::deserialize(&bytes);
        assert!(parsed_bin.is_err());
    }

    #[test]
    fn evidence_verify_binding_fails_closed_on_manual_bogus_version() {
        let mut e = sample_evidence();
        let bogus = DIVERGENCE_EVIDENCE_SCHEMA_VERSION + 5;
        e.schema_version = bogus;
        e.evidence_hash = DivergenceEvidence::compute_evidence_hash(
            bogus,
            e.divergence_class,
            e.affected_state_class,
            &e.scope,
            &e.peers,
            &e.digest_mismatch,
            &e.policy_clause,
            e.freshness_emitted_at,
            e.freshness_valid_until,
            e.private_content_implication,
            &e.evidence_nonce,
        );
        assert!(
            !e.verify_binding(),
            "verify_binding() must fail closed even when the hash matches"
        );
    }

    #[test]
    fn evidence_domain_tag_affects_hash_and_differs_from_other_records() {
        let e = sample_evidence();
        let binding = DivergenceEvidenceBinding {
            schema_version: e.schema_version,
            divergence_class: e.divergence_class,
            affected_state_class: e.affected_state_class,
            scope: &e.scope,
            peers: &e.peers,
            digest_mismatch: &e.digest_mismatch,
            policy_clause: &e.policy_clause,
            freshness_emitted_at: e.freshness_emitted_at,
            freshness_valid_until: e.freshness_valid_until,
            private_content_implication: e.private_content_implication,
            evidence_nonce: e.evidence_nonce,
        };
        let payload = bincode::serialize(&binding).unwrap();
        let mut hasher = blake3::Hasher::new();
        // Deliberately omit DOMAIN_TAG.
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        let without_tag: Hash = *hasher.finalize().as_bytes();
        assert_ne!(e.evidence_hash, without_tag);
        // Distinct from the other proof-class domain tags.
        assert_ne!(DivergenceEvidence::DOMAIN_TAG, AntiEntropyProbe::DOMAIN_TAG);
        assert_ne!(DivergenceEvidence::DOMAIN_TAG, ArtifactReceipt::DOMAIN_TAG);
        assert_ne!(DivergenceEvidence::DOMAIN_TAG, RepairPlan::DOMAIN_TAG);
    }

    #[test]
    fn evidence_two_records_with_distinct_nonces_distinct_hashes() {
        let e1 = sample_evidence();
        let e2 = DivergenceEvidence::new(
            e1.divergence_class,
            e1.affected_state_class,
            e1.scope.clone(),
            e1.peers.clone(),
            e1.digest_mismatch.clone(),
            e1.policy_clause.clone(),
            e1.freshness_emitted_at,
            e1.freshness_valid_until,
            e1.private_content_implication,
            [0xEE; 32],
        );
        assert_ne!(e1.evidence_hash, e2.evidence_hash);
        assert!(e2.verify_binding());
    }

    // ---- DivergenceEvidence: privacy semantics ----

    #[test]
    fn private_divergence_carries_refs_not_bodies() {
        // The PrivateObjectReferenceMismatchWithoutContentDisclosure class
        // is represented as bounded ArtifactDigest/StateDigest forms — the
        // type system does not let bodies enter (every StateDigest projection
        // is a hash, count, or short list of hashes). This test is a smoke
        // test of the privacy contract: the divergence carries refs, and the
        // private_content_implication flag is true.
        let private_refs = StateDigest::ShortList(ShortDigestList::from_hashes(vec![
            [0xAA; 32], [0xBB; 32], [0xCC; 32],
        ]));
        let e = DivergenceEvidence::new(
            DivergenceClass::PrivateObjectReferenceMismatchWithoutContentDisclosure,
            StateClass::ScopedVaultReference,
            ProbeScope::LocalDomain {
                domain_id: "fixture-care-plan-vault".to_string(),
            },
            sample_peers(),
            DigestMismatch::Differs {
                local: private_refs.clone(),
                remote: StateDigest::ShortList(ShortDigestList::from_hashes(vec![
                    [0xAA; 32], [0xDD; 32],
                ])),
            },
            sample_policy_clause(),
            1_715_000_000,
            1_715_000_030,
            true, // private content implicated
            [0x55; 32],
        );
        assert!(e.private_content_implication);
        assert!(e.verify_binding());
        // The digest is opaque hashes only — no human-readable "body" field
        // exists anywhere on the record. This is structural: a future change
        // that adds a body field would fail compilation here.
        match e.digest_mismatch {
            DigestMismatch::Differs { ref local, .. } => match local {
                StateDigest::ShortList(list) => {
                    assert_eq!(list.hashes().len(), 3);
                }
                _ => panic!("expected ShortList for the fixture"),
            },
            _ => panic!("expected Differs"),
        }
    }

    // ---- RepairPlan: binding, round-trip, tamper detection ----

    #[test]
    fn plan_binding_is_deterministic() {
        let ev = sample_evidence();
        let p1 = sample_plan(ev.evidence_hash);
        let p2 = sample_plan(ev.evidence_hash);
        assert_eq!(p1.plan_hash, p2.plan_hash);
        assert_ne!(p1.plan_hash, [0u8; 32]);
    }

    #[test]
    fn plan_verify_binding_succeeds_for_fresh_record() {
        let ev = sample_evidence();
        let p = sample_plan(ev.evidence_hash);
        assert!(p.verify_binding());
    }

    #[test]
    fn plan_json_round_trip_preserves_hash() {
        let ev = sample_evidence();
        let original = sample_plan(ev.evidence_hash);
        let json = serde_json::to_string(&original).unwrap();
        let restored: RepairPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn plan_bincode_round_trip_preserves_hash() {
        let ev = sample_evidence();
        let original = sample_plan(ev.evidence_hash);
        let bytes = bincode::serialize(&original).unwrap();
        let restored: RepairPlan = bincode::deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn plan_action_tamper_detected() {
        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        p.action = RepairAction::RequestGovernanceReview;
        assert!(!p.verify_binding());
    }

    #[test]
    fn plan_authority_tamper_detected() {
        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        p.authority_basis = AuthorityBasis::NoAutomaticAuthority;
        assert!(!p.verify_binding());
    }

    #[test]
    fn plan_scope_tamper_detected() {
        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        p.scope = ProbeScope::Commons;
        assert!(!p.verify_binding());
    }

    #[test]
    fn plan_boundary_rules_tamper_detected() {
        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        p.boundary_rules = BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoMemberFacingLie]);
        assert!(!p.verify_binding());
    }

    #[test]
    fn plan_expected_receipt_class_tamper_detected() {
        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        p.expected_repair_receipt_class = ExpectedRepairReceiptClass::GovernanceReviewReceipt;
        assert!(!p.verify_binding());
    }

    #[test]
    fn plan_evidence_link_tamper_detected() {
        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        p.divergence_evidence_hash = [0xFF; 32];
        assert!(!p.verify_binding());
    }

    #[test]
    fn plan_freshness_tamper_detected() {
        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        p.freshness_emitted_at += 1;
        assert!(!p.verify_binding());

        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        p.freshness_valid_until += 1;
        assert!(!p.verify_binding());
    }

    #[test]
    fn plan_nonce_tamper_detected() {
        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        p.plan_nonce = [0xFF; 32];
        assert!(!p.verify_binding());
    }

    // ---- RepairPlan: schema-version policing ----

    #[test]
    fn plan_rejects_future_schema_version_on_decode() {
        #[derive(Serialize)]
        struct WireShape<'a> {
            schema_version: u32,
            action: RepairAction,
            authority_basis: &'a AuthorityBasis,
            scope: &'a ProbeScope,
            boundary_rules: &'a BoundaryRuleSet,
            expected_repair_receipt_class: ExpectedRepairReceiptClass,
            divergence_evidence_hash: Hash,
            freshness_emitted_at: u64,
            freshness_valid_until: u64,
            plan_nonce: [u8; 32],
            plan_hash: Hash,
            signature: Signature,
        }
        let ev = sample_evidence();
        let p = sample_plan(ev.evidence_hash);
        let bogus = REPAIR_PLAN_SCHEMA_VERSION + 1;
        let recomputed = RepairPlan::compute_plan_hash(
            bogus,
            p.action,
            &p.authority_basis,
            &p.scope,
            &p.boundary_rules,
            p.expected_repair_receipt_class,
            p.divergence_evidence_hash,
            p.freshness_emitted_at,
            p.freshness_valid_until,
            &p.plan_nonce,
        );
        let wire = WireShape {
            schema_version: bogus,
            action: p.action,
            authority_basis: &p.authority_basis,
            scope: &p.scope,
            boundary_rules: &p.boundary_rules,
            expected_repair_receipt_class: p.expected_repair_receipt_class,
            divergence_evidence_hash: p.divergence_evidence_hash,
            freshness_emitted_at: p.freshness_emitted_at,
            freshness_valid_until: p.freshness_valid_until,
            plan_nonce: p.plan_nonce,
            plan_hash: recomputed,
            signature: p.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<RepairPlan, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed_bin: Result<RepairPlan, _> = bincode::deserialize(&bytes);
        assert!(parsed_bin.is_err());
    }

    #[test]
    fn plan_verify_binding_fails_closed_on_manual_bogus_version() {
        let ev = sample_evidence();
        let mut p = sample_plan(ev.evidence_hash);
        let bogus = REPAIR_PLAN_SCHEMA_VERSION + 11;
        p.schema_version = bogus;
        p.plan_hash = RepairPlan::compute_plan_hash(
            bogus,
            p.action,
            &p.authority_basis,
            &p.scope,
            &p.boundary_rules,
            p.expected_repair_receipt_class,
            p.divergence_evidence_hash,
            p.freshness_emitted_at,
            p.freshness_valid_until,
            &p.plan_nonce,
        );
        assert!(!p.verify_binding());
    }

    // ---- Evidence ↔ Plan link ----

    #[test]
    fn plan_references_evidence_by_hash() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        assert_eq!(plan.divergence_evidence_hash, ev.evidence_hash);

        // Changing the evidence (different nonce) yields a different hash;
        // a plan built from the old hash no longer matches the new evidence.
        let ev2 = DivergenceEvidence::new(
            ev.divergence_class,
            ev.affected_state_class,
            ev.scope.clone(),
            ev.peers.clone(),
            ev.digest_mismatch.clone(),
            ev.policy_clause.clone(),
            ev.freshness_emitted_at,
            ev.freshness_valid_until,
            ev.private_content_implication,
            [0x77; 32], // distinct nonce
        );
        assert_ne!(ev.evidence_hash, ev2.evidence_hash);
        assert_ne!(plan.divergence_evidence_hash, ev2.evidence_hash);
    }

    // ---- AuthorityBasis round-trip on all variants ----

    #[test]
    fn authority_basis_all_variants_round_trip() {
        let cases = [
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            AuthorityBasis::GovernanceMandate {
                mandate_hash: [0x11; 32],
            },
            AuthorityBasis::FederationAgreement {
                agreement_hash: [0x22; 32],
            },
            AuthorityBasis::GovernanceReviewRequired,
            AuthorityBasis::NoAutomaticAuthority,
        ];
        for a in &cases {
            let json = serde_json::to_string(a).unwrap();
            let restored: AuthorityBasis = serde_json::from_str(&json).unwrap();
            assert_eq!(*a, restored);

            let bytes = bincode::serialize(a).unwrap();
            let restored_bin: AuthorityBasis = bincode::deserialize(&bytes).unwrap();
            assert_eq!(*a, restored_bin);
        }
    }

    // ---- RepairAction + ExpectedRepairReceiptClass all variants ----

    #[test]
    fn repair_action_all_variants_round_trip() {
        let cases = [
            RepairAction::FetchMissing,
            RepairAction::ReReplicate,
            RepairAction::RetryBackup,
            RepairAction::RunRestoreDrill,
            RepairAction::RetryIntegrityVerification,
            RepairAction::QuarantinePeerPendingReview,
            RepairAction::EscalateToFederationClearing,
            RepairAction::RequestGovernanceReview,
            RepairAction::RestartDisputeWindow,
            RepairAction::NoAutomaticRepair,
        ];
        for a in &cases {
            let json = serde_json::to_string(a).unwrap();
            let restored: RepairAction = serde_json::from_str(&json).unwrap();
            assert_eq!(*a, restored);
        }
    }

    #[test]
    fn expected_repair_receipt_class_all_variants_round_trip() {
        let cases = [
            ExpectedRepairReceiptClass::FetchMissingReceipt,
            ExpectedRepairReceiptClass::ReReplicationReceipt,
            ExpectedRepairReceiptClass::BackupRetryReceipt,
            ExpectedRepairReceiptClass::RestoreDrillReceipt,
            ExpectedRepairReceiptClass::IntegrityVerificationReceipt,
            ExpectedRepairReceiptClass::QuarantineReceipt,
            ExpectedRepairReceiptClass::FederationClearingEscalationReceipt,
            ExpectedRepairReceiptClass::GovernanceReviewReceipt,
            ExpectedRepairReceiptClass::DisputeWindowRestartReceipt,
            ExpectedRepairReceiptClass::NoAutomaticRepairReceipt,
        ];
        for c in &cases {
            let json = serde_json::to_string(c).unwrap();
            let restored: ExpectedRepairReceiptClass = serde_json::from_str(&json).unwrap();
            assert_eq!(*c, restored);
        }
    }

    // ---- Freshness helpers ----

    #[test]
    fn evidence_and_plan_freshness_helpers() {
        let ev = sample_evidence();
        assert!(ev.is_fresh(ev.freshness_emitted_at));
        assert!(ev.is_fresh(ev.freshness_valid_until));
        assert!(!ev.is_fresh(ev.freshness_valid_until + 1));

        let p = sample_plan(ev.evidence_hash);
        assert!(p.is_fresh(p.freshness_emitted_at));
        assert!(p.is_fresh(p.freshness_valid_until));
        assert!(!p.is_fresh(p.freshness_valid_until + 1));
    }

    // =========================================================================
    // RepairReceipt (issue #1849)
    // =========================================================================

    fn sample_before_digest() -> StateDigest {
        StateDigest::ShortList(ShortDigestList::from_hashes(vec![
            [0x10; 32], [0x11; 32], [0x12; 32],
        ]))
    }

    fn sample_after_digest() -> StateDigest {
        StateDigest::ShortList(ShortDigestList::from_hashes(vec![
            [0x10; 32], [0x11; 32], [0x12; 32], [0x13; 32],
        ]))
    }

    fn sample_receipt() -> RepairReceipt {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        RepairReceipt::new(
            RepairReceiptClass::FetchMissingReceipt,
            EffectOutcome::Applied,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            BoundaryRuleSet::from_rules(vec![
                BoundaryRuleRef::NoRepairBeyondAuthority,
                BoundaryRuleRef::NoLocalityOrDisclosureWidening,
            ]),
            Some(sample_before_digest()),
            Some(sample_after_digest()),
            1_715_000_002,
            1_715_000_032,
            false,
            None,
            [0xEF; 32],
        )
        .expect("sample RepairReceipt is structurally consistent")
    }

    // ---- RepairReceiptClass: 1:1 mapping from ExpectedRepairReceiptClass ----

    #[test]
    fn repair_receipt_class_round_trips_through_expected_class() {
        let cases = [
            (
                ExpectedRepairReceiptClass::FetchMissingReceipt,
                RepairReceiptClass::FetchMissingReceipt,
            ),
            (
                ExpectedRepairReceiptClass::ReReplicationReceipt,
                RepairReceiptClass::ReReplicationReceipt,
            ),
            (
                ExpectedRepairReceiptClass::BackupRetryReceipt,
                RepairReceiptClass::BackupRetryReceipt,
            ),
            (
                ExpectedRepairReceiptClass::RestoreDrillReceipt,
                RepairReceiptClass::RestoreDrillReceipt,
            ),
            (
                ExpectedRepairReceiptClass::IntegrityVerificationReceipt,
                RepairReceiptClass::IntegrityVerificationReceipt,
            ),
            (
                ExpectedRepairReceiptClass::QuarantineReceipt,
                RepairReceiptClass::QuarantineReceipt,
            ),
            (
                ExpectedRepairReceiptClass::FederationClearingEscalationReceipt,
                RepairReceiptClass::FederationClearingEscalationReceipt,
            ),
            (
                ExpectedRepairReceiptClass::GovernanceReviewReceipt,
                RepairReceiptClass::GovernanceReviewReceipt,
            ),
            (
                ExpectedRepairReceiptClass::DisputeWindowRestartReceipt,
                RepairReceiptClass::DisputeWindowRestartReceipt,
            ),
            (
                ExpectedRepairReceiptClass::NoAutomaticRepairReceipt,
                RepairReceiptClass::NoAutomaticRepairReceipt,
            ),
        ];
        // Decisive 1:1 evidence: count, forward map, and reverse map all agree.
        assert_eq!(cases.len(), 10);
        for (expected, class) in cases {
            assert_eq!(RepairReceiptClass::from(expected), class);
            assert_eq!(ExpectedRepairReceiptClass::from(class), expected);
        }
    }

    #[test]
    fn repair_receipt_class_all_variants_round_trip() {
        let cases = [
            RepairReceiptClass::FetchMissingReceipt,
            RepairReceiptClass::ReReplicationReceipt,
            RepairReceiptClass::BackupRetryReceipt,
            RepairReceiptClass::RestoreDrillReceipt,
            RepairReceiptClass::IntegrityVerificationReceipt,
            RepairReceiptClass::QuarantineReceipt,
            RepairReceiptClass::FederationClearingEscalationReceipt,
            RepairReceiptClass::GovernanceReviewReceipt,
            RepairReceiptClass::DisputeWindowRestartReceipt,
            RepairReceiptClass::NoAutomaticRepairReceipt,
        ];
        for c in &cases {
            let json = serde_json::to_string(c).unwrap();
            let restored: RepairReceiptClass = serde_json::from_str(&json).unwrap();
            assert_eq!(*c, restored);
            let bytes = bincode::serialize(c).unwrap();
            let restored_bin: RepairReceiptClass = bincode::deserialize(&bytes).unwrap();
            assert_eq!(*c, restored_bin);
        }
    }

    #[test]
    fn repair_failure_reason_all_variants_round_trip() {
        let cases = [
            RepairFailureReason::AuthorityRejected,
            RepairFailureReason::SourceUnavailable,
            RepairFailureReason::DigestMismatchPersisted,
            RepairFailureReason::PrivateContentUnavailable,
            RepairFailureReason::PolicyDenied,
            RepairFailureReason::Timeout,
            RepairFailureReason::Unclassifiable,
        ];
        for r in &cases {
            let json = serde_json::to_string(r).unwrap();
            let restored: RepairFailureReason = serde_json::from_str(&json).unwrap();
            assert_eq!(*r, restored);
            let bytes = bincode::serialize(r).unwrap();
            let restored_bin: RepairFailureReason = bincode::deserialize(&bytes).unwrap();
            assert_eq!(*r, restored_bin);
        }
    }

    // ---- RepairReceipt: binding, round-trip, freshness ----

    #[test]
    fn receipt_binding_is_deterministic() {
        let r1 = sample_receipt();
        let r2 = sample_receipt();
        assert_eq!(r1.receipt_hash, r2.receipt_hash);
        assert_ne!(r1.receipt_hash, [0u8; 32]);
    }

    #[test]
    fn receipt_verify_binding_succeeds_for_fresh_record() {
        let r = sample_receipt();
        assert!(r.verify_binding());
    }

    #[test]
    fn receipt_json_round_trip_preserves_hash() {
        let original = sample_receipt();
        let json = serde_json::to_string(&original).unwrap();
        let restored: RepairReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn receipt_bincode_round_trip_preserves_hash() {
        let original = sample_receipt();
        let bytes = bincode::serialize(&original).unwrap();
        let restored: RepairReceipt = bincode::deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
        assert!(restored.verify_binding());
    }

    #[test]
    fn receipt_freshness_helper() {
        let r = sample_receipt();
        assert!(r.is_fresh(r.applied_at));
        assert!(r.is_fresh(r.freshness_valid_until));
        assert!(!r.is_fresh(r.freshness_valid_until + 1));
    }

    #[test]
    fn receipt_two_records_with_distinct_nonces_distinct_hashes() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let r1 = sample_receipt();
        let r2 = RepairReceipt::new(
            r1.repair_receipt_class,
            r1.effect_outcome,
            ev.evidence_hash,
            plan.plan_hash,
            r1.affected_state_class,
            r1.scope.clone(),
            r1.actor_did.clone(),
            r1.authority_basis.clone(),
            r1.boundary_rules.clone(),
            r1.before_state_digest.clone(),
            r1.after_state_digest.clone(),
            r1.applied_at,
            r1.freshness_valid_until,
            r1.private_content_implication,
            r1.failure_reason,
            [0x77; 32], // distinct nonce
        )
        .unwrap();
        assert_ne!(r1.receipt_hash, r2.receipt_hash);
        assert!(r2.verify_binding());
    }

    #[test]
    fn receipt_domain_tag_distinct_from_other_records() {
        assert_ne!(RepairReceipt::DOMAIN_TAG, AntiEntropyProbe::DOMAIN_TAG);
        assert_ne!(RepairReceipt::DOMAIN_TAG, DivergenceEvidence::DOMAIN_TAG);
        assert_ne!(RepairReceipt::DOMAIN_TAG, RepairPlan::DOMAIN_TAG);
        assert_ne!(RepairReceipt::DOMAIN_TAG, ArtifactReceipt::DOMAIN_TAG);
    }

    #[test]
    fn receipt_domain_tag_affects_hash() {
        let r = sample_receipt();
        let binding = RepairReceiptBinding {
            schema_version: r.schema_version,
            repair_receipt_class: r.repair_receipt_class,
            effect_outcome: r.effect_outcome,
            divergence_evidence_hash: r.divergence_evidence_hash,
            repair_plan_hash: r.repair_plan_hash,
            affected_state_class: r.affected_state_class,
            scope: &r.scope,
            actor_did: &r.actor_did,
            authority_basis: &r.authority_basis,
            boundary_rules: &r.boundary_rules,
            before_state_digest: &r.before_state_digest,
            after_state_digest: &r.after_state_digest,
            applied_at: r.applied_at,
            freshness_valid_until: r.freshness_valid_until,
            private_content_implication: r.private_content_implication,
            failure_reason: r.failure_reason,
            receipt_nonce: r.receipt_nonce,
        };
        let payload = bincode::serialize(&binding).unwrap();
        let mut hasher = blake3::Hasher::new();
        // Deliberately omit DOMAIN_TAG.
        hasher.update(&(payload.len() as u64).to_le_bytes());
        hasher.update(&payload);
        let without_tag: Hash = *hasher.finalize().as_bytes();
        assert_ne!(r.receipt_hash, without_tag);
    }

    // ---- RepairReceipt: tamper detection for every bound field ----

    #[test]
    fn receipt_class_tamper_detected() {
        let mut r = sample_receipt();
        r.repair_receipt_class = RepairReceiptClass::GovernanceReviewReceipt;
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_outcome_tamper_detected() {
        let mut r = sample_receipt();
        // Both Applied and NoOp are consistent with failure_reason=None,
        // so the hash is the only mechanism that catches this mutation.
        r.effect_outcome = EffectOutcome::NoOp;
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_divergence_evidence_link_tamper_detected() {
        let mut r = sample_receipt();
        r.divergence_evidence_hash = [0xFF; 32];
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_plan_link_tamper_detected() {
        let mut r = sample_receipt();
        r.repair_plan_hash = [0xFF; 32];
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_state_class_tamper_detected() {
        let mut r = sample_receipt();
        r.affected_state_class = StateClass::SettlementRecordIndex;
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_scope_tamper_detected() {
        let mut r = sample_receipt();
        r.scope = ProbeScope::Commons;
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_actor_did_tamper_detected() {
        let mut r = sample_receipt();
        r.actor_did = "did:icn:attacker".to_string();
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_authority_tamper_detected() {
        let mut r = sample_receipt();
        r.authority_basis = AuthorityBasis::NoAutomaticAuthority;
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_boundary_rules_tamper_detected() {
        let mut r = sample_receipt();
        r.boundary_rules = BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoMemberFacingLie]);
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_before_digest_tamper_detected() {
        let mut r = sample_receipt();
        r.before_state_digest = Some(StateDigest::ShortList(ShortDigestList::from_hashes(vec![
            [0xAA; 32],
        ])));
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_after_digest_tamper_detected() {
        let mut r = sample_receipt();
        r.after_state_digest = Some(StateDigest::ShortList(ShortDigestList::from_hashes(vec![
            [0xBB; 32],
        ])));
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_applied_at_tamper_detected() {
        let mut r = sample_receipt();
        r.applied_at += 1;
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_freshness_tamper_detected() {
        let mut r = sample_receipt();
        r.freshness_valid_until += 1;
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_private_implication_tamper_detected() {
        let mut r = sample_receipt();
        r.private_content_implication = !r.private_content_implication;
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_failure_reason_tamper_detected() {
        // Construct a Partial receipt with a specific reason, then mutate
        // the reason. Both reasons are valid for Partial; the hash is the
        // only catch.
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let mut r = RepairReceipt::new(
            RepairReceiptClass::FetchMissingReceipt,
            EffectOutcome::Partial,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            Some(sample_before_digest()),
            Some(sample_after_digest()),
            1_715_000_002,
            1_715_000_032,
            false,
            Some(RepairFailureReason::SourceUnavailable),
            [0xEF; 32],
        )
        .expect("partial receipt with reason is consistent");
        assert!(r.verify_binding());
        r.failure_reason = Some(RepairFailureReason::Timeout);
        assert!(!r.verify_binding());
    }

    #[test]
    fn receipt_nonce_tamper_detected() {
        let mut r = sample_receipt();
        r.receipt_nonce = [0xFF; 32];
        assert!(!r.verify_binding());
    }

    // ---- RepairReceipt: schema-version policing ----

    #[derive(Serialize)]
    struct ReceiptWireShape<'a> {
        schema_version: u32,
        repair_receipt_class: RepairReceiptClass,
        effect_outcome: EffectOutcome,
        divergence_evidence_hash: Hash,
        repair_plan_hash: Hash,
        affected_state_class: StateClass,
        scope: &'a ProbeScope,
        actor_did: &'a Did,
        authority_basis: &'a AuthorityBasis,
        boundary_rules: &'a BoundaryRuleSet,
        before_state_digest: &'a Option<StateDigest>,
        after_state_digest: &'a Option<StateDigest>,
        applied_at: u64,
        freshness_valid_until: u64,
        private_content_implication: bool,
        failure_reason: Option<RepairFailureReason>,
        receipt_nonce: [u8; 32],
        receipt_hash: Hash,
        signature: Signature,
    }

    #[test]
    fn receipt_rejects_future_schema_version_on_decode() {
        let r = sample_receipt();
        let bogus = REPAIR_RECEIPT_SCHEMA_VERSION + 1;
        // Recompute the binding hash under the bogus version so we close
        // the "manually crafted hash" bypass too.
        let recomputed = RepairReceipt::compute_receipt_hash(
            bogus,
            r.repair_receipt_class,
            r.effect_outcome,
            r.divergence_evidence_hash,
            r.repair_plan_hash,
            r.affected_state_class,
            &r.scope,
            &r.actor_did,
            &r.authority_basis,
            &r.boundary_rules,
            &r.before_state_digest,
            &r.after_state_digest,
            r.applied_at,
            r.freshness_valid_until,
            r.private_content_implication,
            r.failure_reason,
            &r.receipt_nonce,
        );
        let wire = ReceiptWireShape {
            schema_version: bogus,
            repair_receipt_class: r.repair_receipt_class,
            effect_outcome: r.effect_outcome,
            divergence_evidence_hash: r.divergence_evidence_hash,
            repair_plan_hash: r.repair_plan_hash,
            affected_state_class: r.affected_state_class,
            scope: &r.scope,
            actor_did: &r.actor_did,
            authority_basis: &r.authority_basis,
            boundary_rules: &r.boundary_rules,
            before_state_digest: &r.before_state_digest,
            after_state_digest: &r.after_state_digest,
            applied_at: r.applied_at,
            freshness_valid_until: r.freshness_valid_until,
            private_content_implication: r.private_content_implication,
            failure_reason: r.failure_reason,
            receipt_nonce: r.receipt_nonce,
            receipt_hash: recomputed,
            signature: r.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<RepairReceipt, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed_bin: Result<RepairReceipt, _> = bincode::deserialize(&bytes);
        assert!(parsed_bin.is_err());
    }

    #[test]
    fn receipt_verify_binding_fails_closed_on_manual_bogus_version() {
        let mut r = sample_receipt();
        let bogus = REPAIR_RECEIPT_SCHEMA_VERSION + 7;
        r.schema_version = bogus;
        r.receipt_hash = RepairReceipt::compute_receipt_hash(
            bogus,
            r.repair_receipt_class,
            r.effect_outcome,
            r.divergence_evidence_hash,
            r.repair_plan_hash,
            r.affected_state_class,
            &r.scope,
            &r.actor_did,
            &r.authority_basis,
            &r.boundary_rules,
            &r.before_state_digest,
            &r.after_state_digest,
            r.applied_at,
            r.freshness_valid_until,
            r.private_content_implication,
            r.failure_reason,
            &r.receipt_nonce,
        );
        assert!(
            !r.verify_binding(),
            "verify_binding() must reject unsupported schema_version even when the hash matches"
        );
    }

    // ---- RepairReceipt: outcome / reason / digest structural rules ----

    #[test]
    fn receipt_applied_outcome_carries_before_after_digest() {
        let r = sample_receipt();
        assert_eq!(r.effect_outcome, EffectOutcome::Applied);
        assert!(r.before_state_digest.is_some());
        assert!(r.after_state_digest.is_some());
        assert!(r.verify_binding());
    }

    #[test]
    fn receipt_applied_rejects_failure_reason() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let err = RepairReceipt::new(
            RepairReceiptClass::FetchMissingReceipt,
            EffectOutcome::Applied,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            Some(sample_before_digest()),
            Some(sample_after_digest()),
            1_715_000_002,
            1_715_000_032,
            false,
            Some(RepairFailureReason::Timeout),
            [0xEF; 32],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            RepairReceiptError::FailureReasonNotAllowed { .. }
        ));
    }

    #[test]
    fn receipt_noop_rejects_failure_reason() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let err = RepairReceipt::new(
            RepairReceiptClass::NoAutomaticRepairReceipt,
            EffectOutcome::NoOp,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::NoAutomaticAuthority,
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            None,
            None,
            1_715_000_002,
            1_715_000_032,
            false,
            Some(RepairFailureReason::Timeout),
            [0xEF; 32],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            RepairReceiptError::FailureReasonNotAllowed { .. }
        ));
    }

    #[test]
    fn receipt_partial_requires_failure_reason() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let err = RepairReceipt::new(
            RepairReceiptClass::FetchMissingReceipt,
            EffectOutcome::Partial,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            Some(sample_before_digest()),
            Some(sample_after_digest()),
            1_715_000_002,
            1_715_000_032,
            false,
            None, // missing
            [0xEF; 32],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            RepairReceiptError::FailureReasonRequired { .. }
        ));
    }

    #[test]
    fn receipt_failed_requires_failure_reason() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let err = RepairReceipt::new(
            RepairReceiptClass::FetchMissingReceipt,
            EffectOutcome::Failed,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            Some(sample_before_digest()),
            None,
            1_715_000_002,
            1_715_000_032,
            false,
            None, // missing
            [0xEF; 32],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            RepairReceiptError::FailureReasonRequired { .. }
        ));
    }

    #[test]
    fn receipt_failed_must_not_have_after_state_digest() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let err = RepairReceipt::new(
            RepairReceiptClass::FetchMissingReceipt,
            EffectOutcome::Failed,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            Some(sample_before_digest()),
            Some(sample_after_digest()), // disallowed for Failed
            1_715_000_002,
            1_715_000_032,
            false,
            Some(RepairFailureReason::DigestMismatchPersisted),
            [0xEF; 32],
        )
        .unwrap_err();
        assert!(matches!(err, RepairReceiptError::AfterStateDigestOnFailed));
    }

    // ---- RepairReceipt: NoAutomaticRepairReceipt + NoOp sentinel rule ----

    fn _no_auth_receipt_attempt(
        outcome: EffectOutcome,
        failure_reason: Option<RepairFailureReason>,
        after_state_digest: Option<StateDigest>,
    ) -> Result<RepairReceipt, RepairReceiptError> {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        RepairReceipt::new(
            RepairReceiptClass::NoAutomaticRepairReceipt,
            outcome,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::NoAutomaticAuthority,
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            None,
            after_state_digest,
            1_715_000_002,
            1_715_000_032,
            false,
            failure_reason,
            [0xEF; 32],
        )
    }

    #[test]
    fn receipt_no_automatic_repair_class_rejects_applied_outcome() {
        let err = _no_auth_receipt_attempt(EffectOutcome::Applied, None, None).unwrap_err();
        assert!(matches!(
            err,
            RepairReceiptError::NoAutomaticRepairReceiptRequiresNoOp { outcome } if outcome == "applied"
        ));
    }

    #[test]
    fn receipt_no_automatic_repair_class_rejects_partial_outcome() {
        // Pair Partial with a reason so the earlier `FailureReasonRequired`
        // rule doesn't pre-empt the sentinel rule.
        let err = _no_auth_receipt_attempt(
            EffectOutcome::Partial,
            Some(RepairFailureReason::Unclassifiable),
            None,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            RepairReceiptError::NoAutomaticRepairReceiptRequiresNoOp { outcome } if outcome == "partial"
        ));
    }

    #[test]
    fn receipt_no_automatic_repair_class_rejects_failed_outcome() {
        let err = _no_auth_receipt_attempt(
            EffectOutcome::Failed,
            Some(RepairFailureReason::AuthorityRejected),
            None,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            RepairReceiptError::NoAutomaticRepairReceiptRequiresNoOp { outcome } if outcome == "failed"
        ));
    }

    #[test]
    fn receipt_wire_rejects_no_automatic_repair_with_applied_outcome() {
        // Build a hash-consistent wire shape that pairs the sentinel
        // class with Applied. The deserializer must refuse it before
        // any RepairReceipt is constructed.
        let r = sample_receipt();
        let scope = r.scope.clone();
        let actor = r.actor_did.clone();
        let authority = AuthorityBasis::NoAutomaticAuthority;
        let rules = r.boundary_rules.clone();
        let before: Option<StateDigest> = None;
        let after: Option<StateDigest> = None;
        let nonce = [0x66u8; 32];
        let class = RepairReceiptClass::NoAutomaticRepairReceipt;
        let outcome = EffectOutcome::Applied;
        let failure: Option<RepairFailureReason> = None;
        let applied_at = r.applied_at;
        let valid_until = r.freshness_valid_until;
        let private = false;
        let hash = RepairReceipt::compute_receipt_hash(
            REPAIR_RECEIPT_SCHEMA_VERSION,
            class,
            outcome,
            r.divergence_evidence_hash,
            r.repair_plan_hash,
            r.affected_state_class,
            &scope,
            &actor,
            &authority,
            &rules,
            &before,
            &after,
            applied_at,
            valid_until,
            private,
            failure,
            &nonce,
        );
        let wire = ReceiptWireShape {
            schema_version: REPAIR_RECEIPT_SCHEMA_VERSION,
            repair_receipt_class: class,
            effect_outcome: outcome,
            divergence_evidence_hash: r.divergence_evidence_hash,
            repair_plan_hash: r.repair_plan_hash,
            affected_state_class: r.affected_state_class,
            scope: &scope,
            actor_did: &actor,
            authority_basis: &authority,
            boundary_rules: &rules,
            before_state_digest: &before,
            after_state_digest: &after,
            applied_at,
            freshness_valid_until: valid_until,
            private_content_implication: private,
            failure_reason: failure,
            receipt_nonce: nonce,
            receipt_hash: hash,
            signature: r.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<RepairReceipt, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed_bin: Result<RepairReceipt, _> = bincode::deserialize(&bytes);
        assert!(parsed_bin.is_err());
    }

    #[test]
    fn receipt_noop_no_automatic_repair_sentinel_round_trips() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let r = RepairReceipt::new(
            RepairReceiptClass::NoAutomaticRepairReceipt,
            EffectOutcome::NoOp,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::NoAutomaticAuthority,
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            None,
            None,
            1_715_000_002,
            1_715_000_032,
            false,
            None,
            [0x55; 32],
        )
        .expect("NoOp + NoAutomaticRepairReceipt is the canonical sentinel");
        assert!(r.verify_binding());
        let json = serde_json::to_string(&r).unwrap();
        let restored: RepairReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(r, restored);
        assert_eq!(restored.effect_outcome, EffectOutcome::NoOp);
        assert_eq!(
            restored.repair_receipt_class,
            RepairReceiptClass::NoAutomaticRepairReceipt
        );
    }

    #[test]
    fn receipt_partial_round_trips_with_bounded_reason() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let r = RepairReceipt::new(
            RepairReceiptClass::ReReplicationReceipt,
            EffectOutcome::Partial,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::StorageReplicaVerification,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            Some(sample_before_digest()),
            Some(sample_after_digest()),
            1_715_000_002,
            1_715_000_032,
            false,
            Some(RepairFailureReason::SourceUnavailable),
            [0xCC; 32],
        )
        .unwrap();
        let json = serde_json::to_string(&r).unwrap();
        let restored: RepairReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.effect_outcome, EffectOutcome::Partial);
        assert_eq!(
            restored.failure_reason,
            Some(RepairFailureReason::SourceUnavailable)
        );
        assert!(restored.verify_binding());
    }

    #[test]
    fn receipt_failed_round_trips_with_bounded_reason() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let r = RepairReceipt::new(
            RepairReceiptClass::FetchMissingReceipt,
            EffectOutcome::Failed,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ReceiptIndex,
            ProbeScope::LocalDomain {
                domain_id: "fixture-domain-a".to_string(),
            },
            "did:icn:repair-actor".to_string(),
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
            Some(sample_before_digest()),
            None,
            1_715_000_002,
            1_715_000_032,
            false,
            Some(RepairFailureReason::AuthorityRejected),
            [0xDD; 32],
        )
        .unwrap();
        let json = serde_json::to_string(&r).unwrap();
        let restored: RepairReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.effect_outcome, EffectOutcome::Failed);
        assert!(restored.after_state_digest.is_none());
        assert_eq!(
            restored.failure_reason,
            Some(RepairFailureReason::AuthorityRejected)
        );
        assert!(restored.verify_binding());
    }

    #[test]
    fn receipt_wire_rejects_inconsistent_outcome_reason() {
        // Build a structurally inconsistent wire shape (Applied + reason)
        // and assert the deserializer refuses it. Closes the bypass where
        // a peer crafts a wire payload that new() would never produce.
        let r = sample_receipt();
        let wire = ReceiptWireShape {
            schema_version: REPAIR_RECEIPT_SCHEMA_VERSION,
            repair_receipt_class: r.repair_receipt_class,
            effect_outcome: EffectOutcome::Applied,
            divergence_evidence_hash: r.divergence_evidence_hash,
            repair_plan_hash: r.repair_plan_hash,
            affected_state_class: r.affected_state_class,
            scope: &r.scope,
            actor_did: &r.actor_did,
            authority_basis: &r.authority_basis,
            boundary_rules: &r.boundary_rules,
            before_state_digest: &r.before_state_digest,
            after_state_digest: &r.after_state_digest,
            applied_at: r.applied_at,
            freshness_valid_until: r.freshness_valid_until,
            private_content_implication: r.private_content_implication,
            failure_reason: Some(RepairFailureReason::Timeout), // inconsistent
            receipt_nonce: r.receipt_nonce,
            receipt_hash: r.receipt_hash,
            signature: r.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<RepairReceipt, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
        let bytes = bincode::serialize(&wire).unwrap();
        let parsed_bin: Result<RepairReceipt, _> = bincode::deserialize(&bytes);
        assert!(parsed_bin.is_err());
    }

    #[test]
    fn receipt_wire_rejects_failed_with_after_state_digest() {
        let r = sample_receipt();
        let wire = ReceiptWireShape {
            schema_version: REPAIR_RECEIPT_SCHEMA_VERSION,
            repair_receipt_class: r.repair_receipt_class,
            effect_outcome: EffectOutcome::Failed,
            divergence_evidence_hash: r.divergence_evidence_hash,
            repair_plan_hash: r.repair_plan_hash,
            affected_state_class: r.affected_state_class,
            scope: &r.scope,
            actor_did: &r.actor_did,
            authority_basis: &r.authority_basis,
            boundary_rules: &r.boundary_rules,
            before_state_digest: &r.before_state_digest,
            after_state_digest: &r.after_state_digest, // disallowed for Failed
            applied_at: r.applied_at,
            freshness_valid_until: r.freshness_valid_until,
            private_content_implication: r.private_content_implication,
            failure_reason: Some(RepairFailureReason::DigestMismatchPersisted),
            receipt_nonce: r.receipt_nonce,
            receipt_hash: r.receipt_hash,
            signature: r.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let parsed: Result<RepairReceipt, _> = serde_json::from_str(&json);
        assert!(parsed.is_err());
    }

    // ---- RepairReceipt: collection canonicalization ----

    #[test]
    fn receipt_boundary_rules_normalize_unsorted_wire_input() {
        // A peer sends boundary_rules in non-canonical order with a dup;
        // BoundaryRuleSet's serde(from = ...) collapses it. The resulting
        // RepairReceipt has the canonical set and verify_binding() agrees.
        let r = sample_receipt();
        let canonical_rules = r.boundary_rules.rules().to_vec();
        assert!(canonical_rules.len() >= 2);
        let mut shuffled: Vec<BoundaryRuleRef> = canonical_rules.iter().copied().rev().collect();
        shuffled.push(shuffled[0]); // dup

        #[derive(Serialize)]
        struct LocalWire<'a> {
            schema_version: u32,
            repair_receipt_class: RepairReceiptClass,
            effect_outcome: EffectOutcome,
            divergence_evidence_hash: Hash,
            repair_plan_hash: Hash,
            affected_state_class: StateClass,
            scope: &'a ProbeScope,
            actor_did: &'a Did,
            authority_basis: &'a AuthorityBasis,
            boundary_rules: LocalBadRules,
            before_state_digest: &'a Option<StateDigest>,
            after_state_digest: &'a Option<StateDigest>,
            applied_at: u64,
            freshness_valid_until: u64,
            private_content_implication: bool,
            failure_reason: Option<RepairFailureReason>,
            receipt_nonce: [u8; 32],
            receipt_hash: Hash,
            signature: Signature,
        }
        #[derive(Serialize)]
        struct LocalBadRules {
            rules: Vec<BoundaryRuleRef>,
        }

        let wire = LocalWire {
            schema_version: r.schema_version,
            repair_receipt_class: r.repair_receipt_class,
            effect_outcome: r.effect_outcome,
            divergence_evidence_hash: r.divergence_evidence_hash,
            repair_plan_hash: r.repair_plan_hash,
            affected_state_class: r.affected_state_class,
            scope: &r.scope,
            actor_did: &r.actor_did,
            authority_basis: &r.authority_basis,
            boundary_rules: LocalBadRules { rules: shuffled },
            before_state_digest: &r.before_state_digest,
            after_state_digest: &r.after_state_digest,
            applied_at: r.applied_at,
            freshness_valid_until: r.freshness_valid_until,
            private_content_implication: r.private_content_implication,
            failure_reason: r.failure_reason,
            receipt_nonce: r.receipt_nonce,
            receipt_hash: r.receipt_hash,
            signature: r.signature.clone(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let restored: RepairReceipt = serde_json::from_str(&json).unwrap();
        // Canonicalization restored the sorted/deduped set, so the
        // recomputed binding hash matches the original.
        assert_eq!(restored.boundary_rules.rules(), canonical_rules.as_slice());
        assert!(restored.verify_binding());
    }

    // ---- RepairReceipt: privacy ----

    #[test]
    fn receipt_private_case_carries_refs_not_bodies() {
        // For a repair touching scoped-vault references, the receipt's
        // before/after digests are bounded StateDigest projections
        // (hashes / counts / Merkle roots) — never object bodies. The
        // type system enforces this structurally: there is no field on
        // RepairReceipt that can carry a body.
        let private_before =
            StateDigest::ShortList(ShortDigestList::from_hashes(vec![[0xAA; 32], [0xBB; 32]]));
        let private_after = StateDigest::ShortList(ShortDigestList::from_hashes(vec![
            [0xAA; 32], [0xBB; 32], [0xCC; 32],
        ]));
        let ev = DivergenceEvidence::new(
            DivergenceClass::PrivateObjectReferenceMismatchWithoutContentDisclosure,
            StateClass::ScopedVaultReference,
            ProbeScope::LocalDomain {
                domain_id: "fixture-care-plan-vault".to_string(),
            },
            sample_peers(),
            DigestMismatch::Differs {
                local: private_before.clone(),
                remote: private_after.clone(),
            },
            sample_policy_clause(),
            1_715_000_000,
            1_715_000_030,
            true,
            [0x55; 32],
        );
        let plan = RepairPlan::new(
            RepairAction::FetchMissing,
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            ProbeScope::LocalDomain {
                domain_id: "fixture-care-plan-vault".to_string(),
            },
            BoundaryRuleSet::from_rules(vec![
                BoundaryRuleRef::NoRawPrivateContentInGossipOrProbes,
                BoundaryRuleRef::NoLocalityOrDisclosureWidening,
            ]),
            ExpectedRepairReceiptClass::FetchMissingReceipt,
            ev.evidence_hash,
            1_715_000_001,
            1_715_000_031,
            [0x66; 32],
        );
        let r = RepairReceipt::new(
            RepairReceiptClass::FetchMissingReceipt,
            EffectOutcome::Applied,
            ev.evidence_hash,
            plan.plan_hash,
            StateClass::ScopedVaultReference,
            ProbeScope::LocalDomain {
                domain_id: "fixture-care-plan-vault".to_string(),
            },
            "did:icn:scoped-vault-steward".to_string(),
            AuthorityBasis::DomainPolicyClause(sample_policy_clause()),
            BoundaryRuleSet::from_rules(vec![
                BoundaryRuleRef::NoRawPrivateContentInGossipOrProbes,
                BoundaryRuleRef::NoLocalityOrDisclosureWidening,
            ]),
            Some(private_before),
            Some(private_after),
            1_715_000_002,
            1_715_000_032,
            true,
            None,
            [0x77; 32],
        )
        .expect("private repair receipt is structurally consistent");
        assert!(r.private_content_implication);
        assert!(r.verify_binding());
        // Structurally: the only fields that could carry private data are
        // before_state_digest / after_state_digest, and both are
        // StateDigest enums whose every projection is hashes / counts.
        match r.before_state_digest.as_ref().unwrap() {
            StateDigest::ShortList(list) => assert_eq!(list.hashes().len(), 2),
            _ => panic!("expected ShortList projection"),
        }
        match r.after_state_digest.as_ref().unwrap() {
            StateDigest::ShortList(list) => assert_eq!(list.hashes().len(), 3),
            _ => panic!("expected ShortList projection"),
        }
    }

    // ---- RepairReceipt: cross-links back to plan and evidence ----

    #[test]
    fn receipt_references_plan_and_evidence_by_hash() {
        let ev = sample_evidence();
        let plan = sample_plan(ev.evidence_hash);
        let r = sample_receipt();
        assert_eq!(r.divergence_evidence_hash, ev.evidence_hash);
        assert_eq!(r.repair_plan_hash, plan.plan_hash);
        // A receipt built against a different evidence nonce no longer
        // matches the live evidence — the cross-link is meaningful.
        let ev2 = DivergenceEvidence::new(
            ev.divergence_class,
            ev.affected_state_class,
            ev.scope.clone(),
            ev.peers.clone(),
            ev.digest_mismatch.clone(),
            ev.policy_clause.clone(),
            ev.freshness_emitted_at,
            ev.freshness_valid_until,
            ev.private_content_implication,
            [0xEE; 32],
        );
        assert_ne!(r.divergence_evidence_hash, ev2.evidence_hash);
    }
}
