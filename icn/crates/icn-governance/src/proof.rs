//! Governance proof artifacts for verifiable decision outcomes.
//!
//! A `GovernanceProof` is a self-authenticating receipt that proves a governance
//! decision (proposal vote) was completed with a specific outcome. It contains
//! enough information for any party to verify the outcome without trusting any
//! single node.
//!
//! # Design
//!
//! Follows the `ArtifactReceipt` pattern from `icn-kernel-api/src/proofs.rs`:
//! - blake3 binding hash with domain separation
//! - Length-prefixed variable-length fields to prevent collision attacks
//! - `verify_binding()` recomputes and compares
//! - Vote hash is a merkle root of sorted (voter, choice, weight) tuples

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mandate::MandateId;
use crate::tally::VoteTally;
use crate::vote::{Vote, VoteChoice};

/// Hash type (blake3, 32 bytes)
pub type Hash = [u8; 32];

/// Signature bytes (Ed25519)
pub type SignatureBytes = Vec<u8>;

/// Outcome of a governance decision
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofOutcome {
    /// Proposal was accepted
    Accepted,
    /// Proposal was rejected
    Rejected,
    /// No quorum was reached
    NoQuorum,
}

impl std::fmt::Display for ProofOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProofOutcome::Accepted => write!(f, "accepted"),
            ProofOutcome::Rejected => write!(f, "rejected"),
            ProofOutcome::NoQuorum => write!(f, "no_quorum"),
        }
    }
}

/// A self-authenticating proof that a governance decision completed.
///
/// The `proof_hash` is computed from all significant fields at construction
/// time and can be re-verified at any time via `verify_binding()`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceProof {
    /// ID of the proposal this proof covers
    pub proposal_id: String,
    /// ID of the governance domain
    pub domain_id: String,
    /// Final outcome of the vote
    pub outcome: ProofOutcome,
    /// Aggregated vote tally
    pub vote_tally: VoteTally,
    /// Merkle root of sorted vote records (deterministic)
    pub vote_hash: Hash,
    /// Unix timestamp (seconds) when the decision was finalized
    pub timestamp: u64,
    /// DID of the node that generated this proof
    pub signer_did: String,
    /// blake3 binding hash of all significant fields
    pub proof_hash: Hash,
    /// Ed25519 signature over proof_hash (empty until signed)
    pub signature: SignatureBytes,
}

impl GovernanceProof {
    /// Domain separation tag for governance proof hashes.
    pub const DOMAIN_TAG: &[u8] = b"icn:governance-proof:v1";

    /// Create a new proof with computed binding hash and empty signature.
    pub fn new(
        proposal_id: String,
        domain_id: String,
        outcome: ProofOutcome,
        vote_tally: VoteTally,
        votes: &[Vote],
        timestamp: u64,
        signer_did: String,
    ) -> Self {
        let vote_hash = Self::compute_vote_hash(votes);
        let proof_hash = Self::compute_proof_hash(
            &proposal_id,
            &domain_id,
            outcome,
            &vote_tally,
            &vote_hash,
            timestamp,
            &signer_did,
        );
        Self {
            proposal_id,
            domain_id,
            outcome,
            vote_tally,
            vote_hash,
            timestamp,
            signer_did,
            proof_hash,
            signature: Vec::new(),
        }
    }

    /// Compute a deterministic hash of all votes.
    ///
    /// Votes are sorted by (voter DID, then choice ordinal) to ensure determinism
    /// regardless of the order votes were received. Each vote is length-prefixed.
    pub fn compute_vote_hash(votes: &[Vote]) -> Hash {
        // Sort votes deterministically: by voter DID, then by choice
        let mut sorted: Vec<&Vote> = votes.iter().collect();
        sorted.sort_by(|a, b| {
            a.voter
                .as_str()
                .cmp(b.voter.as_str())
                .then_with(|| choice_ordinal(a.choice).cmp(&choice_ordinal(b.choice)))
        });

        let mut hasher = blake3::Hasher::new();
        hasher.update(b"icn:vote-hash:v1");
        hasher.update(&(sorted.len() as u64).to_le_bytes());

        for vote in &sorted {
            let voter_bytes = vote.voter.as_str().as_bytes();
            hasher.update(&(voter_bytes.len() as u64).to_le_bytes());
            hasher.update(voter_bytes);
            hasher.update(&[choice_ordinal(vote.choice)]);
            hasher.update(&vote.weight.to_le_bytes());
        }

        *hasher.finalize().as_bytes()
    }

    /// Compute the binding hash from all significant fields.
    ///
    /// Variable-length fields are length-prefixed (u64 LE).
    /// Domain separation tag is hashed first.
    pub fn compute_proof_hash(
        proposal_id: &str,
        domain_id: &str,
        outcome: ProofOutcome,
        vote_tally: &VoteTally,
        vote_hash: &Hash,
        timestamp: u64,
        signer_did: &str,
    ) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);

        // Variable-length fields: length-prefixed
        hasher.update(&(proposal_id.len() as u64).to_le_bytes());
        hasher.update(proposal_id.as_bytes());
        hasher.update(&(domain_id.len() as u64).to_le_bytes());
        hasher.update(domain_id.as_bytes());

        // Outcome as single byte
        hasher.update(&[outcome_ordinal(outcome)]);

        // Vote tally as fixed-size fields
        hasher.update(&(vote_tally.for_votes as u64).to_le_bytes());
        hasher.update(&(vote_tally.against_votes as u64).to_le_bytes());
        hasher.update(&(vote_tally.abstain_votes as u64).to_le_bytes());

        // Fixed-length hash
        hasher.update(vote_hash);

        // Timestamp
        hasher.update(&timestamp.to_le_bytes());

        // Signer DID (variable-length)
        hasher.update(&(signer_did.len() as u64).to_le_bytes());
        hasher.update(signer_did.as_bytes());

        *hasher.finalize().as_bytes()
    }

    /// Verify that the stored `proof_hash` matches a fresh computation.
    ///
    /// Returns `true` if the proof has not been tampered with.
    pub fn verify_binding(&self) -> bool {
        let recomputed = Self::compute_proof_hash(
            &self.proposal_id,
            &self.domain_id,
            self.outcome,
            &self.vote_tally,
            &self.vote_hash,
            self.timestamp,
            &self.signer_did,
        );
        self.proof_hash == recomputed
    }

    /// Sign the proof hash with an Ed25519 signing key.
    pub fn sign(&mut self, signing_key: &ed25519_dalek::SigningKey) {
        use ed25519_dalek::Signer;
        let sig = signing_key.sign(&self.proof_hash);
        self.signature = sig.to_bytes().to_vec();
    }

    /// Verify the signature against the proof hash and expected public key.
    pub fn verify_signature(&self, verifying_key: &ed25519_dalek::VerifyingKey) -> bool {
        use ed25519_dalek::Verifier;
        if self.signature.len() != 64 {
            return false;
        }
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&self.signature);
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        verifying_key.verify(&self.proof_hash, &sig).is_ok()
    }
}

/// Cross-node deterministic governance decision receipt.
///
/// Equality semantics are intentionally anchored to `decision_hash` only.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GovernanceDecisionReceipt {
    /// ID of the proposal this receipt covers
    pub proposal_id: String,
    /// ID of the governance domain
    pub domain_id: String,
    /// Final outcome of the vote
    pub outcome: ProofOutcome,
    /// Aggregated vote tally
    pub vote_tally: VoteTally,
    /// Merkle root of sorted vote records (deterministic)
    pub vote_hash: Hash,
    /// blake3 canonical decision hash from receipt fields only
    pub decision_hash: Hash,
}

impl PartialEq for GovernanceDecisionReceipt {
    fn eq(&self, other: &Self) -> bool {
        self.decision_hash == other.decision_hash
    }
}

impl Eq for GovernanceDecisionReceipt {}

impl GovernanceDecisionReceipt {
    /// Domain separation tag for canonical decision hashes.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:decision:v1";

    /// Create a canonical receipt from proposal decision inputs.
    pub fn new(
        proposal_id: String,
        domain_id: String,
        outcome: ProofOutcome,
        vote_tally: VoteTally,
        votes: &[Vote],
    ) -> Self {
        let vote_hash = GovernanceProof::compute_vote_hash(votes);
        let decision_hash =
            Self::compute_decision_hash(&proposal_id, &domain_id, outcome, &vote_tally, &vote_hash);

        Self {
            proposal_id,
            domain_id,
            outcome,
            vote_tally,
            vote_hash,
            decision_hash,
        }
    }

    /// Convert a legacy proof into the canonical receipt model.
    ///
    /// Legacy node-local fields (`timestamp`, `signer_did`, `proof_hash`) are ignored.
    pub fn from_legacy(proof: &GovernanceProof) -> Self {
        let decision_hash = Self::compute_decision_hash(
            &proof.proposal_id,
            &proof.domain_id,
            proof.outcome,
            &proof.vote_tally,
            &proof.vote_hash,
        );

        Self {
            proposal_id: proof.proposal_id.clone(),
            domain_id: proof.domain_id.clone(),
            outcome: proof.outcome,
            vote_tally: proof.vote_tally.clone(),
            vote_hash: proof.vote_hash,
            decision_hash,
        }
    }

    /// Compute canonical bytes used for `decision_hash`.
    ///
    /// Keep this helper as the single source of truth for canonical decision encoding.
    pub fn compute_decision_hash_bytes(&self) -> Vec<u8> {
        Self::compute_decision_hash_bytes_fields(
            &self.proposal_id,
            &self.domain_id,
            self.outcome,
            &self.vote_tally,
            &self.vote_hash,
        )
    }

    fn compute_decision_hash_bytes_fields(
        proposal_id: &str,
        domain_id: &str,
        outcome: ProofOutcome,
        vote_tally: &VoteTally,
        vote_hash: &Hash,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(Self::DOMAIN_TAG);
        append_decision_base_field_bytes(
            &mut bytes,
            proposal_id,
            domain_id,
            outcome,
            vote_tally,
            vote_hash,
        );
        bytes
    }

    /// Compute `decision_hash` from canonical receipt fields.
    pub fn compute_decision_hash(
        proposal_id: &str,
        domain_id: &str,
        outcome: ProofOutcome,
        vote_tally: &VoteTally,
        vote_hash: &Hash,
    ) -> Hash {
        let bytes = Self::compute_decision_hash_bytes_fields(
            proposal_id,
            domain_id,
            outcome,
            vote_tally,
            vote_hash,
        );
        *blake3::hash(&bytes).as_bytes()
    }

    /// Verify the stored `decision_hash` against canonical receipt fields.
    pub fn verify(&self) -> bool {
        let recomputed = Self::compute_decision_hash(
            &self.proposal_id,
            &self.domain_id,
            self.outcome,
            &self.vote_tally,
            &self.vote_hash,
        );
        self.decision_hash == recomputed
    }
}

/// Append the canonical base-field byte sequence shared by every
/// versioned governance decision receipt.
///
/// Absorbs only `proposal_id`, `domain_id`, `outcome`, `vote_tally`,
/// and `vote_hash` — **not** a domain-separation tag. Each versioned
/// receipt's `compute_decision_hash` writes its **own** `DOMAIN_TAG`
/// first and then calls this helper, so the two version namespaces
/// (`icn:gov:decision:v1`, `icn:gov:decision:v2`, …) remain fully
/// separate even though they share the same base-field encoding.
///
/// The encoding mirrors the original
/// [`GovernanceDecisionReceipt::compute_decision_hash_bytes_fields`]
/// body byte-for-byte so v1 hashes remain stable after extraction.
fn append_decision_base_field_bytes(
    out: &mut Vec<u8>,
    proposal_id: &str,
    domain_id: &str,
    outcome: ProofOutcome,
    vote_tally: &VoteTally,
    vote_hash: &Hash,
) {
    out.extend_from_slice(&(proposal_id.len() as u64).to_le_bytes());
    out.extend_from_slice(proposal_id.as_bytes());
    out.extend_from_slice(&(domain_id.len() as u64).to_le_bytes());
    out.extend_from_slice(domain_id.as_bytes());
    out.push(outcome_ordinal(outcome));
    out.extend_from_slice(&(vote_tally.for_votes as u64).to_le_bytes());
    out.extend_from_slice(&(vote_tally.against_votes as u64).to_le_bytes());
    out.extend_from_slice(&(vote_tally.abstain_votes as u64).to_le_bytes());
    out.extend_from_slice(vote_hash);
}

// ============================================================================
// Governance decision receipt v2 — mandate-attestation fork (#1868 step 2)
// ============================================================================

/// Closed taxonomy of why a governance act explicitly records "no mandate
/// required" on a v2 decision receipt.
///
/// The decomposition design (`governance-write-decomposition.md` §6) flags
/// two distinct no-mandate conditions that the receipt must keep
/// distinguishable: routine record-keeping whose institutional floor is
/// just membership-in-good-standing, and labeled bootstrap/direct-
/// administrative shortcuts that bypass the ratified path entirely. A
/// closed enum mirrors every other taxonomy in this module
/// (`ProofOutcome`, `ActionItemTransition`, `MeetingAttendanceTransition`,
/// `ProcessGateKind`, `ProcessGateResult`) and keeps the audit answer
/// unambiguous. `#[non_exhaustive]` lets future cases land non-breakingly
/// — adding a variant after the existing two requires a new domain-
/// separation tag on the receipt that consumes it (`:v3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NoMandateReason {
    /// Membership-in-good-standing is the institutional floor for this
    /// act. No act-time mandate is required beyond standing — covers the
    /// routine meeting/activity/comment record-keeping rows in the
    /// decomposition table at `governance-write-decomposition.md` §6.
    MembershipStandingOnly,
    /// Direct-administrative bootstrap shortcut. Pairs with #1869's
    /// `activation_path: bootstrap` label so the receipt and the
    /// artifact-level marker tell the same story.
    Bootstrap,
}

/// Explicit, structurally-typed mandate-attestation discriminator embedded
/// in a v2 [`GovernanceDecisionReceiptV2`].
///
/// The decomp doc §10 step 2 requires the "no mandate required" answer to
/// be *explicit* rather than implicit absence. Modeled as a tagged enum
/// — never `Option<MandateGrantRef>` — so the receipt's mandate stance
/// can never be derived from field omission, a missing serde default, or
/// a stringy value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReceiptMandateAttestation {
    /// The act was performed under a deliberate "no mandate required"
    /// classification. `reason` says which closed-taxonomy condition
    /// applies; see [`NoMandateReason`].
    NoMandateRequired {
        /// Why the act needed no mandate. Closed enum, never free-form.
        reason: NoMandateReason,
    },
    /// The act was authorized by a ratified mandate; `grant_ref` is the
    /// wire-form reference produced by the act-time gate (the
    /// `apps/governance::mandate_gate` `MandateGate::require` →
    /// `MandateGrant::into_ref()` adapter). The grant ref carries its
    /// own per-component canonical encoding (#1928), so the receipt's
    /// canonical hash binds the entire grant via the grant ref's
    /// 32-byte `ref_hash()`.
    Grant {
        /// The receipt-recordable mandate grant reference; see
        /// [`MandateGrantRef`].
        grant_ref: MandateGrantRef,
    },
    /// The act (a governance decision) was authorized by the **governance
    /// process itself** — eligible voters, voting period, quorum/threshold,
    /// tally, proposal scope, and outcome rules — rather than by a personal
    /// grant or by needing no authority. This is distinct from
    /// [`Self::NoMandateRequired`]: membership standing authorizes
    /// participation and low-blast record-keeping, but does **not** by itself
    /// authorize an institutional decision (see #1868 decision-receipt
    /// authority design). Bare discriminator — the process evidence is the
    /// decision receipt's own `vote_tally`/`vote_hash`, already bound in the
    /// base hash, so no extra payload is carried here.
    ///
    /// **Versioning:** this is the third attestation kind; it grows the shared
    /// taxonomy beyond what the v2 receipts froze, so it is carried **only**
    /// by [`GovernanceDecisionReceiptV3`] (`icn:gov:decision:v3`). The v2
    /// receipt types reject it fail-closed at construction and deserialization
    /// to keep their frozen domain-tag semantics intact.
    ProcessAuthorized,
}

/// Errors returned by [`GovernanceDecisionReceiptV2::new`] (and by the
/// `try_from` shadow that routes `Deserialize` through the same checks).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GovernanceDecisionReceiptV2Error {
    /// The `capability_scope_presented` field was empty or whitespace-
    /// only. The constructor (and the wire boundary) rejects this so the
    /// receipt cannot record an unattributed scope. This intentionally
    /// does **not** enforce membership in the gateway's
    /// `ALLOWED_SCOPES` allowlist — the scope set evolves and an
    /// allowlist check here would force a v3 fork every time a new class
    /// scope lands. The audit layer is the right home for the allowlist
    /// check.
    #[error("capability_scope_presented must be a non-empty, non-whitespace string")]
    EmptyCapabilityScope,
    /// `ProcessAuthorized` is a v3-only attestation mode; this v2 receipt's
    /// attestation taxonomy is frozen at `NoMandateRequired`/`Grant`. Rejected
    /// at the constructor and the serde boundary so a v2 receipt can never
    /// carry it — preserving the v2 domain-tag's frozen hash semantics.
    #[error("ProcessAuthorized attestation requires a v3 decision receipt; v2 cannot carry it")]
    UnsupportedAttestation,
}

/// Cross-node deterministic governance decision receipt — **v2** of the
/// canonical wire form, embedding the mandate-attestation discriminator
/// and the capability scope a caller presented at act time.
///
/// # Relationship to existing types
///
/// - [`GovernanceDecisionReceipt`] (this module, above) is the v1 receipt
///   and is **byte-stable** — its `DOMAIN_TAG` (`icn:gov:decision:v1`),
///   canonical hash, and 22+ external call sites are unchanged. v2 is
///   purely additive; no v1 caller is forced to migrate.
/// - [`GovernanceProofV2`] (this module, below) is the existing **proof
///   container** — receipt + node attestations. Its "V2" refers to the
///   container shape, not the receipt version. The two "V2"s are
///   orthogonal: `GovernanceProofV2` still wraps a v1
///   [`GovernanceDecisionReceipt`]. A future slice will decide whether
///   to extend `GovernanceProofV2` to accept either receipt version, add
///   a parallel container, or migrate the container shape. The naming
///   overlap is preserved per existing module convention and called out
///   explicitly here so readers can tell the two apart.
///
/// # Wire / canonical contract
///
/// - `DOMAIN_TAG = b"icn:gov:decision:v2"`. Fully separate from the v1
///   namespace; the two cannot collide.
/// - Canonical encoding: v2 tag, then the same base-field byte sequence
///   v1 binds (via the shared [`append_decision_base_field_bytes`]
///   helper), then the v2-only fields. The v2 hash is a fresh `blake3`
///   over that byte stream — **never** derived from v1's
///   `decision_hash`, v1's tag, or any serialized form of either.
/// - `Deserialize` is routed through a private
///   `GovernanceDecisionReceiptV2Raw` shadow + `#[serde(try_from = ...)]`
///   so empty-scope rejection runs on every deserialized payload, not
///   just on the constructor path (mirrors the #1928 boundary pattern).
/// - Equality is anchored to `decision_hash` (matches the v1 convention).
///
/// # Out of scope for this PR
///
/// - No handler emits a v2 receipt yet. All 22 external callers of
///   [`GovernanceDecisionReceipt::new`] remain on v1.
/// - No `ActionItemCompletionReceipt` or `MeetingAttendanceReceipt` fork.
/// - No `GovernanceProofV2` extension to accept the v2 receipt.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "GovernanceDecisionReceiptV2Raw")]
pub struct GovernanceDecisionReceiptV2 {
    /// ID of the proposal this receipt covers.
    pub proposal_id: String,
    /// ID of the governance domain.
    pub domain_id: String,
    /// Final outcome of the vote.
    pub outcome: ProofOutcome,
    /// Aggregated vote tally.
    pub vote_tally: VoteTally,
    /// Merkle root of sorted vote records (deterministic).
    pub vote_hash: Hash,
    /// The capability scope string the caller presented at act time
    /// (e.g. `"governance:charter:write"`, `"governance:write"`). Bound
    /// into the canonical hash so the receipt records *which kind of
    /// write happened* alongside the decision itself. Rejected
    /// empty/whitespace by the constructor and the serde boundary.
    pub capability_scope_presented: String,
    /// Explicit mandate-attestation discriminator: either a
    /// [`ReceiptMandateAttestation::Grant`] carrying a wire-form
    /// [`MandateGrantRef`] or a
    /// [`ReceiptMandateAttestation::NoMandateRequired`] carrying a
    /// closed-taxonomy [`NoMandateReason`]. Never `Option` — absence
    /// must never be interpretable as "no mandate."
    pub mandate_attestation: ReceiptMandateAttestation,
    /// blake3 canonical decision hash from receipt fields under the
    /// v2 domain-separation tag.
    pub decision_hash: Hash,
}

impl PartialEq for GovernanceDecisionReceiptV2 {
    fn eq(&self, other: &Self) -> bool {
        self.decision_hash == other.decision_hash
    }
}

impl Eq for GovernanceDecisionReceiptV2 {}

impl GovernanceDecisionReceiptV2 {
    /// Domain separation tag for v2 canonical decision hashes. Distinct
    /// from [`GovernanceDecisionReceipt::DOMAIN_TAG`] so the v1 and v2
    /// namespaces remain fully separate.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:decision:v2";

    /// Create a canonical v2 receipt from proposal decision inputs plus
    /// the capability scope presented and the mandate attestation.
    /// Rejects empty/whitespace `capability_scope_presented`.
    pub fn new(
        proposal_id: String,
        domain_id: String,
        outcome: ProofOutcome,
        vote_tally: VoteTally,
        votes: &[Vote],
        capability_scope_presented: String,
        mandate_attestation: ReceiptMandateAttestation,
    ) -> Result<Self, GovernanceDecisionReceiptV2Error> {
        if capability_scope_presented.trim().is_empty() {
            return Err(GovernanceDecisionReceiptV2Error::EmptyCapabilityScope);
        }
        if matches!(
            mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ) {
            return Err(GovernanceDecisionReceiptV2Error::UnsupportedAttestation);
        }
        let vote_hash = GovernanceProof::compute_vote_hash(votes);
        let decision_hash = Self::compute_decision_hash(
            &proposal_id,
            &domain_id,
            outcome,
            &vote_tally,
            &vote_hash,
            &capability_scope_presented,
            &mandate_attestation,
        );
        Ok(Self {
            proposal_id,
            domain_id,
            outcome,
            vote_tally,
            vote_hash,
            capability_scope_presented,
            mandate_attestation,
            decision_hash,
        })
    }

    /// Compute the canonical v2 `decision_hash` from receipt fields.
    ///
    /// The byte stream is: v2 [`Self::DOMAIN_TAG`], then the v1 base
    /// fields via [`append_decision_base_field_bytes`] (length-prefixed
    /// strings, single-byte outcome ordinal, u64 LE tally counts, raw
    /// 32-byte vote hash, identical order to v1), then the v2-only
    /// additions: length-prefixed `capability_scope_presented`, a single
    /// ordinal byte for the [`ReceiptMandateAttestation`] variant
    /// (`NoMandateRequired` = 0, `Grant` = 1), then the per-variant
    /// payload:
    ///
    /// - `NoMandateRequired { reason }`: a single
    ///   [`no_mandate_reason_ordinal`] byte.
    /// - `Grant { grant_ref }`: the 32-byte
    ///   [`MandateGrantRef::ref_hash`] (fixed-length; no length prefix).
    ///   Binding via `ref_hash` propagates the grant ref's per-component
    ///   canonical encoding (#1928) without re-deriving the field layout
    ///   here — that encoding is the single source of truth for the
    ///   grant-ref surface.
    pub fn compute_decision_hash(
        proposal_id: &str,
        domain_id: &str,
        outcome: ProofOutcome,
        vote_tally: &VoteTally,
        vote_hash: &Hash,
        capability_scope_presented: &str,
        mandate_attestation: &ReceiptMandateAttestation,
    ) -> Hash {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(Self::DOMAIN_TAG);
        append_decision_base_field_bytes(
            &mut bytes,
            proposal_id,
            domain_id,
            outcome,
            vote_tally,
            vote_hash,
        );
        bytes.extend_from_slice(&(capability_scope_presented.len() as u64).to_le_bytes());
        bytes.extend_from_slice(capability_scope_presented.as_bytes());
        bytes.push(attestation_kind_ordinal(mandate_attestation));
        match mandate_attestation {
            ReceiptMandateAttestation::NoMandateRequired { reason } => {
                bytes.push(no_mandate_reason_ordinal(*reason));
            }
            ReceiptMandateAttestation::Grant { grant_ref } => {
                // Bind via the grant ref's canonical hash. Fixed length
                // (32 bytes), no length prefix — see #1928's
                // `MandateGrantRef::compute_ref_hash` for the field-level
                // encoding inside this digest.
                bytes.extend_from_slice(&grant_ref.ref_hash());
            }
            ReceiptMandateAttestation::ProcessAuthorized => {
                // v3-only attestation mode; a v2 decision receipt never holds
                // it — `new()`/`try_from` reject it — so a validly-constructed
                // v2 receipt never reaches this arm. `compute_decision_hash`
                // is public, so a direct caller could still pass it; `verify()`
                // fail-closes on it. No per-variant payload — kept here only
                // for exhaustiveness.
            }
        }
        *blake3::hash(&bytes).as_bytes()
    }

    /// Verify the stored `decision_hash` against canonical v2 receipt
    /// fields.
    ///
    /// Fail-closed on the v3-only `ProcessAuthorized` mode: `new()`/`try_from`
    /// already reject it, but the struct fields are `pub`, so a direct struct
    /// literal could otherwise carry it and verify through the defensive hash
    /// arm. Rejecting here keeps the frozen-taxonomy discipline intact at the
    /// verification boundary too.
    pub fn verify(&self) -> bool {
        if matches!(
            self.mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ) {
            return false;
        }
        let recomputed = Self::compute_decision_hash(
            &self.proposal_id,
            &self.domain_id,
            self.outcome,
            &self.vote_tally,
            &self.vote_hash,
            &self.capability_scope_presented,
            &self.mandate_attestation,
        );
        self.decision_hash == recomputed
    }
}

/// Raw deserialization shadow for [`GovernanceDecisionReceiptV2`].
///
/// `GovernanceDecisionReceiptV2` is a wire/persisted primitive, so
/// `Deserialize` is an input boundary that must apply the same checks
/// as `GovernanceDecisionReceiptV2::new`. Routing deserialization
/// through this shadow + `try_from` keeps the wire surface symmetric
/// with the constructor — a payload with an empty
/// `capability_scope_presented` fails closed at the deserialization
/// boundary, not only via `new`. Mirrors the same pattern as
/// `MandateGrantRefRaw`.
#[derive(Deserialize)]
struct GovernanceDecisionReceiptV2Raw {
    proposal_id: String,
    domain_id: String,
    outcome: ProofOutcome,
    vote_tally: VoteTally,
    vote_hash: Hash,
    capability_scope_presented: String,
    mandate_attestation: ReceiptMandateAttestation,
    decision_hash: Hash,
}

impl TryFrom<GovernanceDecisionReceiptV2Raw> for GovernanceDecisionReceiptV2 {
    type Error = GovernanceDecisionReceiptV2Error;

    fn try_from(raw: GovernanceDecisionReceiptV2Raw) -> Result<Self, Self::Error> {
        if raw.capability_scope_presented.trim().is_empty() {
            return Err(GovernanceDecisionReceiptV2Error::EmptyCapabilityScope);
        }
        if matches!(
            raw.mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ) {
            return Err(GovernanceDecisionReceiptV2Error::UnsupportedAttestation);
        }
        // Note: we deliberately do not recompute / verify the
        // `decision_hash` here. `Deserialize` accepts whatever hash
        // value the wire carries; callers verify integrity via
        // `verify()` (matches v1 behavior — `GovernanceDecisionReceipt`
        // also accepts whatever `decision_hash` value is on the wire
        // and exposes `verify()` separately). This keeps the
        // deserialization surface a pure structural check.
        Ok(Self {
            proposal_id: raw.proposal_id,
            domain_id: raw.domain_id,
            outcome: raw.outcome,
            vote_tally: raw.vote_tally,
            vote_hash: raw.vote_hash,
            capability_scope_presented: raw.capability_scope_presented,
            mandate_attestation: raw.mandate_attestation,
            decision_hash: raw.decision_hash,
        })
    }
}

// ============================================================================
// Governance decision receipt v3 — process-authorized attestation fork (#1868)
// ============================================================================

/// Errors returned by [`GovernanceDecisionReceiptV3::new`] (and by the
/// `try_from` shadow that routes `Deserialize` through the same checks).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GovernanceDecisionReceiptV3Error {
    /// The `capability_scope_presented` field was empty or whitespace-only.
    /// Rejected at the constructor and the wire boundary so the receipt
    /// cannot record an unattributed scope. Mirrors
    /// [`GovernanceDecisionReceiptV2Error::EmptyCapabilityScope`].
    #[error("capability_scope_presented must be a non-empty, non-whitespace string")]
    EmptyCapabilityScope,
}

/// Cross-node deterministic governance decision receipt — **v3** of the
/// canonical wire form.
///
/// v3 exists because the #1868 decision-receipt authority design adds a third
/// attestation mode, [`ReceiptMandateAttestation::ProcessAuthorized`] (a
/// democratic close authorized by the governance process — eligible voters,
/// period, quorum/threshold, tally, scope, outcome rules — rather than by a
/// personal grant or by needing no authority). Growing the attestation
/// taxonomy beyond what v2 froze requires a new domain-separation tag, so v3
/// is the **only** decision receipt version that can carry
/// `ProcessAuthorized`. It also accepts `NoMandateRequired` (incl.
/// `Bootstrap` for the forced-accept path) and `Grant`, making it the
/// extended superset of v2.
///
/// # Relationship to v1/v2
///
/// - [`GovernanceDecisionReceipt`] (v1) and [`GovernanceDecisionReceiptV2`]
///   are unchanged and byte-stable. Their `DOMAIN_TAG`s
///   (`icn:gov:decision:v1`, `:v2`) and canonical hashes are untouched; v2
///   rejects `ProcessAuthorized` fail-closed to keep its frozen taxonomy.
/// - v3 reuses the shared [`append_decision_base_field_bytes`] base-field
///   encoding under its own `icn:gov:decision:v3` tag; the v3 hash is a fresh
///   `blake3` over that byte stream — never derived from the v1/v2 hash, the
///   v1/v2 tags, JSON, `Debug`, or any serialized v1/v2 receipt.
///
/// # Out of scope for this PR
///
/// - No handler emits a v3 receipt yet — this is schema preparation only.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "GovernanceDecisionReceiptV3Raw")]
pub struct GovernanceDecisionReceiptV3 {
    /// ID of the proposal this receipt covers.
    pub proposal_id: String,
    /// ID of the governance domain.
    pub domain_id: String,
    /// Final outcome of the vote.
    pub outcome: ProofOutcome,
    /// Aggregated vote tally.
    pub vote_tally: VoteTally,
    /// Merkle root of sorted vote records (deterministic).
    pub vote_hash: Hash,
    /// The capability scope string the caller presented at act time. Bound
    /// into the canonical hash. Rejected empty/whitespace by the constructor
    /// and the serde boundary.
    pub capability_scope_presented: String,
    /// Explicit attestation discriminator. v3 accepts all three modes:
    /// [`ReceiptMandateAttestation::ProcessAuthorized`],
    /// [`ReceiptMandateAttestation::NoMandateRequired`], and
    /// [`ReceiptMandateAttestation::Grant`]. Never `Option` — absence must
    /// never be interpretable as "no authority".
    pub mandate_attestation: ReceiptMandateAttestation,
    /// blake3 canonical decision hash from receipt fields under the v3
    /// domain-separation tag.
    pub decision_hash: Hash,
}

impl PartialEq for GovernanceDecisionReceiptV3 {
    fn eq(&self, other: &Self) -> bool {
        self.decision_hash == other.decision_hash
    }
}

impl Eq for GovernanceDecisionReceiptV3 {}

impl GovernanceDecisionReceiptV3 {
    /// Domain separation tag for v3 canonical decision hashes. Distinct from
    /// the v1/v2 tags so the namespaces remain fully separate.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:decision:v3";

    /// Create a canonical v3 receipt. Rejects empty/whitespace
    /// `capability_scope_presented`. Accepts any attestation mode (v3 is the
    /// extended-taxonomy version).
    pub fn new(
        proposal_id: String,
        domain_id: String,
        outcome: ProofOutcome,
        vote_tally: VoteTally,
        votes: &[Vote],
        capability_scope_presented: String,
        mandate_attestation: ReceiptMandateAttestation,
    ) -> Result<Self, GovernanceDecisionReceiptV3Error> {
        if capability_scope_presented.trim().is_empty() {
            return Err(GovernanceDecisionReceiptV3Error::EmptyCapabilityScope);
        }
        let vote_hash = GovernanceProof::compute_vote_hash(votes);
        let decision_hash = Self::compute_decision_hash(
            &proposal_id,
            &domain_id,
            outcome,
            &vote_tally,
            &vote_hash,
            &capability_scope_presented,
            &mandate_attestation,
        );
        Ok(Self {
            proposal_id,
            domain_id,
            outcome,
            vote_tally,
            vote_hash,
            capability_scope_presented,
            mandate_attestation,
            decision_hash,
        })
    }

    /// Compute the canonical v3 `decision_hash`.
    ///
    /// Byte stream: v3 [`Self::DOMAIN_TAG`], then the shared base fields via
    /// [`append_decision_base_field_bytes`] (identical order/encoding to
    /// v1/v2), then length-prefixed `capability_scope_presented`, a single
    /// [`attestation_kind_ordinal`] byte, then the per-variant payload:
    /// - `NoMandateRequired { reason }`: one [`no_mandate_reason_ordinal`] byte.
    /// - `Grant { grant_ref }`: the 32-byte [`MandateGrantRef::ref_hash`].
    /// - `ProcessAuthorized`: no payload (the process evidence is the
    ///   already-bound `vote_tally`/`vote_hash`).
    pub fn compute_decision_hash(
        proposal_id: &str,
        domain_id: &str,
        outcome: ProofOutcome,
        vote_tally: &VoteTally,
        vote_hash: &Hash,
        capability_scope_presented: &str,
        mandate_attestation: &ReceiptMandateAttestation,
    ) -> Hash {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(Self::DOMAIN_TAG);
        append_decision_base_field_bytes(
            &mut bytes,
            proposal_id,
            domain_id,
            outcome,
            vote_tally,
            vote_hash,
        );
        bytes.extend_from_slice(&(capability_scope_presented.len() as u64).to_le_bytes());
        bytes.extend_from_slice(capability_scope_presented.as_bytes());
        bytes.push(attestation_kind_ordinal(mandate_attestation));
        match mandate_attestation {
            ReceiptMandateAttestation::NoMandateRequired { reason } => {
                bytes.push(no_mandate_reason_ordinal(*reason));
            }
            ReceiptMandateAttestation::Grant { grant_ref } => {
                bytes.extend_from_slice(&grant_ref.ref_hash());
            }
            ReceiptMandateAttestation::ProcessAuthorized => {
                // No per-variant payload: the governance-process evidence is
                // the receipt's own `vote_tally`/`vote_hash`, already bound
                // above. The kind ordinal (2) distinguishes it from the other
                // attestation modes.
            }
        }
        *blake3::hash(&bytes).as_bytes()
    }

    /// Verify the stored `decision_hash` against canonical v3 receipt fields.
    pub fn verify(&self) -> bool {
        let recomputed = Self::compute_decision_hash(
            &self.proposal_id,
            &self.domain_id,
            self.outcome,
            &self.vote_tally,
            &self.vote_hash,
            &self.capability_scope_presented,
            &self.mandate_attestation,
        );
        self.decision_hash == recomputed
    }
}

/// Raw deserialization shadow for [`GovernanceDecisionReceiptV3`].
///
/// Routes `Deserialize` through the same empty-scope check as
/// [`GovernanceDecisionReceiptV3::new`] (mirrors the v2 shadow pattern). v3
/// accepts all attestation modes, so — unlike the v2 shadows — it does not
/// reject `ProcessAuthorized`.
#[derive(Deserialize)]
struct GovernanceDecisionReceiptV3Raw {
    proposal_id: String,
    domain_id: String,
    outcome: ProofOutcome,
    vote_tally: VoteTally,
    vote_hash: Hash,
    capability_scope_presented: String,
    mandate_attestation: ReceiptMandateAttestation,
    decision_hash: Hash,
}

impl TryFrom<GovernanceDecisionReceiptV3Raw> for GovernanceDecisionReceiptV3 {
    type Error = GovernanceDecisionReceiptV3Error;

    fn try_from(raw: GovernanceDecisionReceiptV3Raw) -> Result<Self, Self::Error> {
        if raw.capability_scope_presented.trim().is_empty() {
            return Err(GovernanceDecisionReceiptV3Error::EmptyCapabilityScope);
        }
        // Matches v1/v2 behavior: `Deserialize` accepts whatever `decision_hash`
        // the wire carries; callers verify integrity via `verify()` separately.
        Ok(Self {
            proposal_id: raw.proposal_id,
            domain_id: raw.domain_id,
            outcome: raw.outcome,
            vote_tally: raw.vote_tally,
            vote_hash: raw.vote_hash,
            capability_scope_presented: raw.capability_scope_presented,
            mandate_attestation: raw.mandate_attestation,
            decision_hash: raw.decision_hash,
        })
    }
}

/// Map [`ReceiptMandateAttestation`] to a deterministic ordinal for
/// canonical hashing. Ordinals are fixed per wire version; the `:v3`
/// receipt tag ([`GovernanceDecisionReceiptV3`]) is what admits the third
/// kind (`ProcessAuthorized` = 2). The v2 receipts froze their taxonomy at
/// `{0, 1}` and reject `ProcessAuthorized` at their construction boundary,
/// so they never hash ordinal `2`.
fn attestation_kind_ordinal(att: &ReceiptMandateAttestation) -> u8 {
    match att {
        ReceiptMandateAttestation::NoMandateRequired { .. } => 0,
        ReceiptMandateAttestation::Grant { .. } => 1,
        ReceiptMandateAttestation::ProcessAuthorized => 2,
    }
}

/// Map [`NoMandateReason`] to a deterministic ordinal for canonical
/// hashing. Same versioning contract as
/// [`attestation_kind_ordinal`].
fn no_mandate_reason_ordinal(reason: NoMandateReason) -> u8 {
    match reason {
        NoMandateReason::MembershipStandingOnly => 0,
        NoMandateReason::Bootstrap => 1,
    }
}

/// Node-local signed attestation over a canonical governance decision.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceDecisionAttestation {
    /// Canonical decision hash being attested
    pub decision_hash: Hash,
    /// DID of signer node
    pub signer_did: String,
    /// Unix timestamp of attestation
    pub timestamp: u64,
    /// Ed25519 signature over canonical attestation payload
    pub signature: SignatureBytes,
}

impl GovernanceDecisionAttestation {
    /// Domain separation tag for attestation payloads.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:attest:v1";

    fn payload_bytes(decision_hash: &Hash, signer_did: &str, timestamp: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(Self::DOMAIN_TAG);
        bytes.extend_from_slice(decision_hash);
        bytes.extend_from_slice(&(signer_did.len() as u64).to_le_bytes());
        bytes.extend_from_slice(signer_did.as_bytes());
        bytes.extend_from_slice(&timestamp.to_le_bytes());
        bytes
    }

    /// Sign an attestation for a `decision_hash`.
    pub fn sign(
        decision_hash: Hash,
        signer_did: String,
        timestamp: u64,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Self {
        use ed25519_dalek::Signer;
        let payload = Self::payload_bytes(&decision_hash, &signer_did, timestamp);
        let signature = signing_key.sign(&payload).to_bytes().to_vec();
        Self {
            decision_hash,
            signer_did,
            timestamp,
            signature,
        }
    }

    /// Verify the attestation signature against expected verifier.
    pub fn verify(&self, verifying_key: &ed25519_dalek::VerifyingKey) -> bool {
        use ed25519_dalek::Verifier;
        if self.signature.len() != 64 {
            return false;
        }
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&self.signature);
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        let payload = Self::payload_bytes(&self.decision_hash, &self.signer_did, self.timestamp);
        verifying_key.verify(&payload, &sig).is_ok()
    }
}

/// Governance proof bundle (canonical receipt + one or more node attestations).
///
/// Equality semantics are intentionally anchored to `receipt.decision_hash` only.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GovernanceProofV2 {
    pub receipt: GovernanceDecisionReceipt,
    pub attestations: Vec<GovernanceDecisionAttestation>,
}

impl PartialEq for GovernanceProofV2 {
    fn eq(&self, other: &Self) -> bool {
        self.receipt.decision_hash == other.receipt.decision_hash
    }
}

impl Eq for GovernanceProofV2 {}

impl GovernanceProofV2 {
    pub fn new(
        receipt: GovernanceDecisionReceipt,
        attestations: Vec<GovernanceDecisionAttestation>,
    ) -> Self {
        Self {
            receipt,
            attestations,
        }
    }

    /// Convert from legacy proof shape.
    ///
    /// Legacy signatures are bound to legacy `proof_hash`; therefore attestations are
    /// intentionally left empty for compatibility decoding without canonical attestation claims.
    pub fn from_legacy(proof: &GovernanceProof) -> Self {
        Self {
            receipt: GovernanceDecisionReceipt::from_legacy(proof),
            attestations: Vec::new(),
        }
    }

    pub fn verify_receipt(&self) -> bool {
        self.receipt.verify()
    }
}

/// Map VoteChoice to a deterministic ordinal for hashing
fn choice_ordinal(choice: VoteChoice) -> u8 {
    match choice {
        VoteChoice::For => 0,
        VoteChoice::Against => 1,
        VoteChoice::Abstain => 2,
    }
}

/// Map ProofOutcome to a deterministic ordinal for hashing
fn outcome_ordinal(outcome: ProofOutcome) -> u8 {
    match outcome {
        ProofOutcome::Accepted => 0,
        ProofOutcome::Rejected => 1,
        ProofOutcome::NoQuorum => 2,
    }
}

// ============================================================================
// Action item completion receipts (ADR-0026 Layer 2 — non-proposal source)
// ============================================================================

/// Closed taxonomy of state transitions that produce an
/// [`ActionItemCompletionReceipt`].
///
/// Today the only receipt-bearing transition is `Completed`. Variants are
/// added when a corresponding write path lands; the runtime never emits a
/// transition not listed here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionItemTransition {
    /// The action item moved into `ActionItemStatus::Completed`. The
    /// authorized actor is the assignee or creator (per the existing
    /// `update_action_item_status` handler authorization check).
    Completed,
}

/// Cross-node deterministic completion receipt for a governance action
/// item.
///
/// Sits alongside [`GovernanceDecisionReceipt`] in ADR-0026 Layer 2: it
/// records the *fact* of a state transition the runtime can attest to,
/// keyed so a holder shell can locate it via the action card's `source_id`.
///
/// Equality is anchored to `record_hash`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionItemCompletionReceipt {
    /// Action item id (string form of `ActionItemId`). This is the same
    /// string the holder's `ActionCard.source_id` carries — it is the
    /// link between the card and the receipt.
    pub item_id: String,
    /// Governance domain the action item lives under.
    pub domain_id: String,
    /// DID of the actor whose authorized completion call produced this
    /// receipt (must be the action item's assignee or creator at call
    /// time, per the handler's authorization check).
    pub actor_did: String,
    /// Transition this receipt records. Closed enum; see
    /// [`ActionItemTransition`].
    pub transition: ActionItemTransition,
    /// Unix-seconds the transition was recorded (typically the
    /// `updated_at` of the post-transition action item).
    pub completed_at: u64,
    /// blake3 canonical record hash binding the fields above.
    pub record_hash: Hash,
}

impl PartialEq for ActionItemCompletionReceipt {
    fn eq(&self, other: &Self) -> bool {
        self.record_hash == other.record_hash
    }
}

impl Eq for ActionItemCompletionReceipt {}

impl ActionItemCompletionReceipt {
    /// Domain separation tag for canonical action-item completion record
    /// hashes. Distinct from the proposal-decision tag so a record can
    /// never collide with a `GovernanceDecisionReceipt`.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:action_item_completion:v1";

    /// Build a new receipt and compute its canonical `record_hash`.
    pub fn new(
        item_id: String,
        domain_id: String,
        actor_did: String,
        transition: ActionItemTransition,
        completed_at: u64,
    ) -> Self {
        let record_hash =
            Self::compute_record_hash(&item_id, &domain_id, &actor_did, transition, completed_at);
        Self {
            item_id,
            domain_id,
            actor_did,
            transition,
            completed_at,
            record_hash,
        }
    }

    /// Compute the canonical record hash from the input fields. Inputs
    /// are length-prefixed under the [`Self::DOMAIN_TAG`] so no two
    /// distinct field bindings can produce the same hash.
    pub fn compute_record_hash(
        item_id: &str,
        domain_id: &str,
        actor_did: &str,
        transition: ActionItemTransition,
        completed_at: u64,
    ) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        absorb_action_item_base_field_bytes(
            &mut hasher,
            item_id,
            domain_id,
            actor_did,
            transition,
            completed_at,
        );
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }
}

/// Absorb the canonical base-field byte sequence shared by every versioned
/// [`ActionItemCompletionReceipt`] family member.
///
/// Writes only `item_id`, `domain_id`, `actor_did`, `transition`, and
/// `completed_at` into the hasher — **not** a domain-separation tag.
/// Each versioned receipt's `compute_record_hash` writes its **own**
/// [`Self::DOMAIN_TAG`] first and then calls this helper, so the two
/// version namespaces (`icn:gov:action_item_completion:v1`,
/// `icn:gov:action_item_completion:v2`, …) remain fully separate even
/// though they share the same base-field encoding.
///
/// The encoding mirrors the original v1
/// [`ActionItemCompletionReceipt::compute_record_hash`] body
/// byte-for-byte so v1 hashes remain stable after extraction. u64
/// length prefixes are used (matching the convention elsewhere in this
/// module) and the transition ordinal comes from
/// [`action_item_transition_ordinal`].
fn absorb_action_item_base_field_bytes(
    hasher: &mut blake3::Hasher,
    item_id: &str,
    domain_id: &str,
    actor_did: &str,
    transition: ActionItemTransition,
    completed_at: u64,
) {
    for field in [item_id, domain_id, actor_did] {
        hasher.update(&(field.len() as u64).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.update(&[action_item_transition_ordinal(transition)]);
    hasher.update(&completed_at.to_le_bytes());
}

/// Map [`ActionItemTransition`] to a deterministic ordinal for canonical
/// hashing. Ordinals are fixed at v1 of the receipt's wire format; adding
/// a variant after `Completed` requires a new domain-separation tag on
/// every receipt that consumes this helper.
fn action_item_transition_ordinal(transition: ActionItemTransition) -> u8 {
    match transition {
        ActionItemTransition::Completed => 0,
    }
}

// ============================================================================
// Action item completion receipt v2 — mandate-attestation fork (#1868 step 2)
// ============================================================================

/// Errors returned by [`ActionItemCompletionReceiptV2::new`] (and by the
/// `try_from` shadow that routes `Deserialize` through the same checks).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ActionItemCompletionReceiptV2Error {
    /// The `capability_scope_presented` field was empty or whitespace-
    /// only. Rejected at the constructor and at the wire boundary so the
    /// receipt cannot record an unattributed scope. Mirrors the
    /// [`GovernanceDecisionReceiptV2Error::EmptyCapabilityScope`]
    /// contract.
    #[error("capability_scope_presented must be a non-empty, non-whitespace string")]
    EmptyCapabilityScope,
    /// `ProcessAuthorized` is a governance-decision authority mode; an
    /// action-item completion receipt has no v3 form and never carries it
    /// (its attestation taxonomy is frozen at `NoMandateRequired`/`Grant`).
    /// Rejected at the constructor and the serde boundary so a v2 receipt
    /// can never carry it — preserving the v2 domain-tag's frozen hash
    /// semantics.
    #[error(
        "ProcessAuthorized is a governance-decision authority mode and is not valid for an action-item completion receipt"
    )]
    UnsupportedAttestation,
}

/// Cross-node deterministic completion receipt for a governance action
/// item — **v2** of the canonical wire form, embedding the mandate-
/// attestation discriminator and the capability scope a caller presented
/// at completion time.
///
/// # Relationship to existing types
///
/// - [`ActionItemCompletionReceipt`] (this module, above) is the v1
///   receipt and is **byte-stable** — its `DOMAIN_TAG`
///   (`icn:gov:action_item_completion:v1`), canonical hash, and call
///   sites are unchanged. v2 is purely additive; no v1 caller is forced
///   to migrate.
/// - [`GovernanceDecisionReceiptV2`] is the parallel v2 fork on the
///   proposal-decision side (#1868 step 2). The two v2 receipts share the
///   same [`ReceiptMandateAttestation`] / [`NoMandateReason`] primitives
///   and the same wire-boundary discipline (checked constructor + serde
///   `try_from` shadow), differing only in their base-field set and
///   their domain-separation tag namespace.
///
/// # Wire / canonical contract
///
/// - `DOMAIN_TAG = b"icn:gov:action_item_completion:v2"`. Fully separate
///   from the v1 namespace; the two cannot collide. Also distinct from
///   the proposal-decision and meeting-attendance tags so records can
///   never collide across receipt families.
/// - Canonical encoding: v2 tag, then the same base-field byte sequence
///   v1 binds (via the shared [`absorb_action_item_base_field_bytes`]
///   helper), then the v2-only fields. The v2 hash is a fresh `blake3`
///   over that byte stream — **never** derived from v1's
///   `record_hash`, v1's tag, or any serialized form of either.
/// - `Deserialize` is routed through a private
///   `ActionItemCompletionReceiptV2Raw` shadow + `#[serde(try_from =
///   ...)]` so empty-scope rejection runs on every deserialized
///   payload (mirrors the #1928 / #1929 boundary pattern).
/// - Equality is anchored to `record_hash` (matches the v1 convention).
///
/// # Out of scope for this PR
///
/// - No handler emits a v2 action-item receipt yet.
/// - No `MeetingAttendanceReceipt` fork.
/// - No grant-minting expansion; no `TypedScope` federation/role binding;
///   no `governance:write` retirement; no kernel meaning-firewall
///   widening; no production-readiness/live-federation/demo claim.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "ActionItemCompletionReceiptV2Raw")]
pub struct ActionItemCompletionReceiptV2 {
    /// Action item id (string form of `ActionItemId`).
    pub item_id: String,
    /// Governance domain the action item lives under.
    pub domain_id: String,
    /// DID of the actor whose authorized completion call produced this
    /// receipt.
    pub actor_did: String,
    /// Transition this receipt records. Closed enum; see
    /// [`ActionItemTransition`].
    pub transition: ActionItemTransition,
    /// Unix-seconds the transition was recorded.
    pub completed_at: u64,
    /// The capability scope string the caller presented at completion
    /// time (e.g. `"governance:meeting:write"`, `"governance:write"`).
    /// Bound into the canonical hash so the receipt records *which kind
    /// of write happened* alongside the transition. Rejected
    /// empty/whitespace by the constructor and the serde boundary.
    pub capability_scope_presented: String,
    /// Explicit mandate-attestation discriminator: either a
    /// [`ReceiptMandateAttestation::Grant`] carrying a wire-form
    /// [`MandateGrantRef`] or a
    /// [`ReceiptMandateAttestation::NoMandateRequired`] carrying a
    /// closed-taxonomy [`NoMandateReason`]. Never `Option` — absence
    /// must never be interpretable as "no mandate."
    pub mandate_attestation: ReceiptMandateAttestation,
    /// blake3 canonical record hash from receipt fields under the v2
    /// domain-separation tag.
    pub record_hash: Hash,
}

impl PartialEq for ActionItemCompletionReceiptV2 {
    fn eq(&self, other: &Self) -> bool {
        self.record_hash == other.record_hash
    }
}

impl Eq for ActionItemCompletionReceiptV2 {}

impl ActionItemCompletionReceiptV2 {
    /// Domain separation tag for v2 canonical action-item completion
    /// record hashes. Distinct from
    /// [`ActionItemCompletionReceipt::DOMAIN_TAG`] so the v1 and v2
    /// namespaces remain fully separate.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:action_item_completion:v2";

    /// Build a new v2 receipt and compute its canonical `record_hash`.
    /// Rejects empty/whitespace `capability_scope_presented`.
    pub fn new(
        item_id: String,
        domain_id: String,
        actor_did: String,
        transition: ActionItemTransition,
        completed_at: u64,
        capability_scope_presented: String,
        mandate_attestation: ReceiptMandateAttestation,
    ) -> Result<Self, ActionItemCompletionReceiptV2Error> {
        if capability_scope_presented.trim().is_empty() {
            return Err(ActionItemCompletionReceiptV2Error::EmptyCapabilityScope);
        }
        if matches!(
            mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ) {
            return Err(ActionItemCompletionReceiptV2Error::UnsupportedAttestation);
        }
        let record_hash = Self::compute_record_hash(
            &item_id,
            &domain_id,
            &actor_did,
            transition,
            completed_at,
            &capability_scope_presented,
            &mandate_attestation,
        );
        Ok(Self {
            item_id,
            domain_id,
            actor_did,
            transition,
            completed_at,
            capability_scope_presented,
            mandate_attestation,
            record_hash,
        })
    }

    /// Compute the canonical v2 `record_hash` from receipt fields.
    ///
    /// The byte stream is: v2 [`Self::DOMAIN_TAG`], then the v1 base
    /// fields via [`absorb_action_item_base_field_bytes`] (length-
    /// prefixed strings, single-byte transition ordinal, u64 LE
    /// `completed_at`, identical order to v1), then the v2-only
    /// additions: length-prefixed `capability_scope_presented`, a single
    /// ordinal byte for the [`ReceiptMandateAttestation`] variant
    /// (`NoMandateRequired` = 0, `Grant` = 1), then the per-variant
    /// payload:
    ///
    /// - `NoMandateRequired { reason }`: a single
    ///   [`no_mandate_reason_ordinal`] byte.
    /// - `Grant { grant_ref }`: the 32-byte
    ///   [`MandateGrantRef::ref_hash`] (fixed-length; no length prefix).
    ///   Binding via `ref_hash` propagates the grant ref's per-component
    ///   canonical encoding (#1928) without re-deriving the field layout
    ///   here.
    pub fn compute_record_hash(
        item_id: &str,
        domain_id: &str,
        actor_did: &str,
        transition: ActionItemTransition,
        completed_at: u64,
        capability_scope_presented: &str,
        mandate_attestation: &ReceiptMandateAttestation,
    ) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        absorb_action_item_base_field_bytes(
            &mut hasher,
            item_id,
            domain_id,
            actor_did,
            transition,
            completed_at,
        );
        hasher.update(&(capability_scope_presented.len() as u64).to_le_bytes());
        hasher.update(capability_scope_presented.as_bytes());
        hasher.update(&[attestation_kind_ordinal(mandate_attestation)]);
        match mandate_attestation {
            ReceiptMandateAttestation::NoMandateRequired { reason } => {
                hasher.update(&[no_mandate_reason_ordinal(*reason)]);
            }
            ReceiptMandateAttestation::Grant { grant_ref } => {
                // Bind via the grant ref's canonical hash (#1928). Fixed
                // length (32 bytes), no length prefix.
                hasher.update(&grant_ref.ref_hash());
            }
            ReceiptMandateAttestation::ProcessAuthorized => {
                // `ProcessAuthorized` is a governance-decision authority mode;
                // this receipt never holds it — `new()`/`try_from` reject it —
                // so a validly-constructed receipt never reaches this arm.
                // `compute_record_hash` is public, so a direct caller could
                // still pass it; `verify()` fail-closes on it. No per-variant
                // payload — exhaustiveness only.
            }
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }

    /// Verify the stored `record_hash` against canonical v2 receipt
    /// fields.
    pub fn verify(&self) -> bool {
        // Fail-closed on the v3-only `ProcessAuthorized` mode (see the v2
        // decision receipt's `verify` for the rationale: `pub` fields allow a
        // direct struct literal to bypass the `new()`/`try_from` rejection).
        if matches!(
            self.mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ) {
            return false;
        }
        let recomputed = Self::compute_record_hash(
            &self.item_id,
            &self.domain_id,
            &self.actor_did,
            self.transition,
            self.completed_at,
            &self.capability_scope_presented,
            &self.mandate_attestation,
        );
        self.record_hash == recomputed
    }
}

/// Raw deserialization shadow for [`ActionItemCompletionReceiptV2`].
///
/// `ActionItemCompletionReceiptV2` is a wire/persisted primitive, so
/// `Deserialize` is an input boundary that must apply the same checks as
/// the constructor. Routing deserialization through this shadow +
/// `try_from` keeps the wire surface symmetric with `new` — a payload
/// with an empty `capability_scope_presented` fails closed at the
/// deserialization boundary, not only via `new`. Mirrors the pattern
/// established by `MandateGrantRefRaw` (#1928) and
/// `GovernanceDecisionReceiptV2Raw` (#1929).
#[derive(Deserialize)]
struct ActionItemCompletionReceiptV2Raw {
    item_id: String,
    domain_id: String,
    actor_did: String,
    transition: ActionItemTransition,
    completed_at: u64,
    capability_scope_presented: String,
    mandate_attestation: ReceiptMandateAttestation,
    record_hash: Hash,
}

impl TryFrom<ActionItemCompletionReceiptV2Raw> for ActionItemCompletionReceiptV2 {
    type Error = ActionItemCompletionReceiptV2Error;

    fn try_from(raw: ActionItemCompletionReceiptV2Raw) -> Result<Self, Self::Error> {
        if raw.capability_scope_presented.trim().is_empty() {
            return Err(ActionItemCompletionReceiptV2Error::EmptyCapabilityScope);
        }
        if matches!(
            raw.mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ) {
            return Err(ActionItemCompletionReceiptV2Error::UnsupportedAttestation);
        }
        // Matches v1 behavior: `Deserialize` accepts whatever
        // `record_hash` value the wire carries; callers verify integrity
        // via `verify()` separately. Keeps the deserialization surface a
        // pure structural check.
        Ok(Self {
            item_id: raw.item_id,
            domain_id: raw.domain_id,
            actor_did: raw.actor_did,
            transition: raw.transition,
            completed_at: raw.completed_at,
            capability_scope_presented: raw.capability_scope_presented,
            mandate_attestation: raw.mandate_attestation,
            record_hash: raw.record_hash,
        })
    }
}

// ============================================================================
// Meeting attendance receipts (ADR-0026 Layer 2 — non-proposal source)
// ============================================================================

/// Closed taxonomy of attendance transitions that produce a
/// [`MeetingAttendanceReceipt`].
///
/// Only attend-shaped transitions emit a receipt. `AttendanceStatus::Absent`
/// is intentionally not represented here: absence is recorded as state on
/// the meeting object but is not a receipt-bearing event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingAttendanceTransition {
    /// The attendee was present in person.
    Present,
    /// The attendee was present remotely (e.g. video).
    Remote,
}

/// Cross-node deterministic attendance receipt for a governance meeting.
///
/// Sits alongside [`GovernanceDecisionReceipt`] and
/// [`ActionItemCompletionReceipt`] in ADR-0026 Layer 2. Records the *fact*
/// of an attendance transition the runtime can attest to, keyed so a
/// holder shell can locate it via the action card's `source_id`
/// (`meeting_id`) plus the holder's own DID.
///
/// Meeting attendance is steward-recorded: the authenticated caller
/// (`recorded_by`) and the subject of the record (`attendee_did`) can
/// differ. Both are bound into the canonical hash.
///
/// Equality is anchored to `record_hash`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeetingAttendanceReceipt {
    /// Meeting id (string form of `MeetingId`). This is the same string
    /// the holder's `ActionCard.source_id` carries — it is the link
    /// between the card and the receipt.
    pub meeting_id: String,
    /// Governance domain the meeting lives under.
    pub domain_id: String,
    /// DID of the attendee whose attendance this receipt records.
    pub attendee_did: String,
    /// DID of the authenticated caller who recorded the attendance. May
    /// differ from `attendee_did` (steward-recorded attendance).
    pub recorded_by: String,
    /// Transition this receipt records. Closed enum; see
    /// [`MeetingAttendanceTransition`].
    pub transition: MeetingAttendanceTransition,
    /// Unix-seconds the transition was recorded.
    pub recorded_at: u64,
    /// blake3 canonical record hash binding the fields above.
    pub record_hash: Hash,
}

impl PartialEq for MeetingAttendanceReceipt {
    fn eq(&self, other: &Self) -> bool {
        self.record_hash == other.record_hash
    }
}

impl Eq for MeetingAttendanceReceipt {}

impl MeetingAttendanceReceipt {
    /// Domain separation tag for canonical meeting-attendance record
    /// hashes. Distinct from proposal-decision and action-item-completion
    /// tags so a record can never collide across receipt types.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:meeting_attendance:v1";

    /// Build a new receipt and compute its canonical `record_hash`.
    pub fn new(
        meeting_id: String,
        domain_id: String,
        attendee_did: String,
        recorded_by: String,
        transition: MeetingAttendanceTransition,
        recorded_at: u64,
    ) -> Self {
        let record_hash = Self::compute_record_hash(
            &meeting_id,
            &domain_id,
            &attendee_did,
            &recorded_by,
            transition,
            recorded_at,
        );
        Self {
            meeting_id,
            domain_id,
            attendee_did,
            recorded_by,
            transition,
            recorded_at,
            record_hash,
        }
    }

    /// Compute the canonical record hash from the input fields. Inputs
    /// are length-prefixed under the [`Self::DOMAIN_TAG`] so no two
    /// distinct field bindings can produce the same hash.
    pub fn compute_record_hash(
        meeting_id: &str,
        domain_id: &str,
        attendee_did: &str,
        recorded_by: &str,
        transition: MeetingAttendanceTransition,
        recorded_at: u64,
    ) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        absorb_meeting_attendance_base_field_bytes(
            &mut hasher,
            meeting_id,
            domain_id,
            attendee_did,
            recorded_by,
            transition,
            recorded_at,
        );
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }
}

/// Absorb the canonical base-field byte sequence shared by every versioned
/// [`MeetingAttendanceReceipt`] family member.
///
/// Writes only `meeting_id`, `domain_id`, `attendee_did`, `recorded_by`,
/// `transition`, and `recorded_at` into the hasher — **not** a domain-
/// separation tag. Each versioned receipt's `compute_record_hash` writes
/// its **own** `DOMAIN_TAG` first and then calls this helper, so the two
/// version namespaces (`icn:gov:meeting_attendance:v1`,
/// `icn:gov:meeting_attendance:v2`, …) remain fully separate even though
/// they share the same base-field encoding.
///
/// The encoding mirrors the original v1
/// [`MeetingAttendanceReceipt::compute_record_hash`] body byte-for-byte so
/// v1 hashes remain stable after extraction. u64 length prefixes are used
/// (matching the convention elsewhere in this module) and the transition
/// ordinal comes from [`meeting_attendance_transition_ordinal`].
fn absorb_meeting_attendance_base_field_bytes(
    hasher: &mut blake3::Hasher,
    meeting_id: &str,
    domain_id: &str,
    attendee_did: &str,
    recorded_by: &str,
    transition: MeetingAttendanceTransition,
    recorded_at: u64,
) {
    for field in [meeting_id, domain_id, attendee_did, recorded_by] {
        hasher.update(&(field.len() as u64).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.update(&[meeting_attendance_transition_ordinal(transition)]);
    hasher.update(&recorded_at.to_le_bytes());
}

/// Map [`MeetingAttendanceTransition`] to a deterministic ordinal for
/// canonical hashing. Ordinals are fixed at v1 of the receipt's wire
/// format; adding a variant after `Remote` requires a new domain-
/// separation tag on every receipt that consumes this helper.
fn meeting_attendance_transition_ordinal(transition: MeetingAttendanceTransition) -> u8 {
    match transition {
        MeetingAttendanceTransition::Present => 0,
        MeetingAttendanceTransition::Remote => 1,
    }
}

// ============================================================================
// Meeting attendance receipt v2 — mandate-attestation fork (#1868)
// ============================================================================

/// Errors returned by [`MeetingAttendanceReceiptV2::new`] (and by the
/// `try_from` shadow that routes `Deserialize` through the same checks).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MeetingAttendanceReceiptV2Error {
    /// The `capability_scope_presented` field was empty or whitespace-
    /// only. Rejected at the constructor and at the wire boundary so the
    /// receipt cannot record an unattributed scope. Mirrors the
    /// [`ActionItemCompletionReceiptV2Error::EmptyCapabilityScope`]
    /// contract.
    #[error("capability_scope_presented must be a non-empty, non-whitespace string")]
    EmptyCapabilityScope,
    /// `ProcessAuthorized` is a governance-decision authority mode; a
    /// meeting-attendance receipt has no v3 form and never carries it
    /// (its attestation taxonomy is frozen at `NoMandateRequired`/`Grant`).
    /// Rejected at the constructor and the serde boundary so a v2 receipt
    /// can never carry it — preserving the v2 domain-tag's frozen hash
    /// semantics.
    #[error(
        "ProcessAuthorized is a governance-decision authority mode and is not valid for a meeting-attendance receipt"
    )]
    UnsupportedAttestation,
}

/// Cross-node deterministic attendance receipt for a governance meeting —
/// **v2** of the canonical wire form, embedding the mandate-attestation
/// discriminator and the capability scope a caller presented at record
/// time.
///
/// # Relationship to existing types
///
/// - [`MeetingAttendanceReceipt`] (this module, above) is the v1 receipt
///   and is **byte-stable** — its `DOMAIN_TAG`
///   (`icn:gov:meeting_attendance:v1`), canonical hash, and call sites are
///   unchanged. v2 is purely additive; no v1 caller is forced to migrate.
/// - [`GovernanceDecisionReceiptV2`] and [`ActionItemCompletionReceiptV2`]
///   are the parallel v2 forks on the proposal-decision and action-item
///   sides (#1868). The three v2 receipts share the same
///   [`ReceiptMandateAttestation`] / [`NoMandateReason`] primitives and
///   the same wire-boundary discipline (checked constructor + serde
///   `try_from` shadow), differing only in their base-field set and their
///   domain-separation tag namespace.
///
/// # Wire / canonical contract
///
/// - `DOMAIN_TAG = b"icn:gov:meeting_attendance:v2"`. Fully separate from
///   the v1 namespace; the two cannot collide. Also distinct from the
///   proposal-decision and action-item tags so records can never collide
///   across receipt families.
/// - Canonical encoding: v2 tag, then the same base-field byte sequence v1
///   binds (via the shared
///   [`absorb_meeting_attendance_base_field_bytes`] helper), then the
///   v2-only fields. The v2 hash is a fresh `blake3` over that byte
///   stream — **never** derived from v1's `record_hash`, v1's tag, or any
///   serialized form of either.
/// - `Deserialize` is routed through a private
///   `MeetingAttendanceReceiptV2Raw` shadow + `#[serde(try_from = ...)]`
///   so empty-scope rejection runs on every deserialized payload (mirrors
///   the #1928 / #1929 / #1930 boundary pattern).
/// - Equality is anchored to `record_hash` (matches the v1 convention).
///
/// # Out of scope for this PR
///
/// - No handler emits a v2 meeting-attendance receipt yet.
/// - No grant-minting expansion; no `TypedScope` federation/role binding;
///   no `governance:write` retirement; no kernel meaning-firewall
///   widening; no production-readiness/live-federation/demo claim.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "MeetingAttendanceReceiptV2Raw")]
pub struct MeetingAttendanceReceiptV2 {
    /// Meeting id (string form of `MeetingId`). This is the same string
    /// the holder's `ActionCard.source_id` carries — it is the link
    /// between the card and the receipt.
    pub meeting_id: String,
    /// Governance domain the meeting lives under.
    pub domain_id: String,
    /// DID of the attendee whose attendance this receipt records.
    pub attendee_did: String,
    /// DID of the authenticated caller who recorded the attendance. May
    /// differ from `attendee_did` (steward-recorded attendance).
    pub recorded_by: String,
    /// Transition this receipt records. Closed enum; see
    /// [`MeetingAttendanceTransition`].
    pub transition: MeetingAttendanceTransition,
    /// Unix-seconds the transition was recorded.
    pub recorded_at: u64,
    /// The capability scope string the caller presented at record time
    /// (e.g. `"governance:meeting:write"`). Bound into the canonical hash
    /// so the receipt records *which kind of write happened* alongside the
    /// transition. Rejected empty/whitespace by the constructor and the
    /// serde boundary.
    pub capability_scope_presented: String,
    /// Explicit mandate-attestation discriminator: either a
    /// [`ReceiptMandateAttestation::Grant`] carrying a wire-form
    /// [`MandateGrantRef`] or a
    /// [`ReceiptMandateAttestation::NoMandateRequired`] carrying a
    /// closed-taxonomy [`NoMandateReason`]. Never `Option` — absence
    /// must never be interpretable as "no mandate."
    pub mandate_attestation: ReceiptMandateAttestation,
    /// blake3 canonical record hash from receipt fields under the v2
    /// domain-separation tag.
    pub record_hash: Hash,
}

impl PartialEq for MeetingAttendanceReceiptV2 {
    fn eq(&self, other: &Self) -> bool {
        self.record_hash == other.record_hash
    }
}

impl Eq for MeetingAttendanceReceiptV2 {}

impl MeetingAttendanceReceiptV2 {
    /// Domain separation tag for v2 canonical meeting-attendance record
    /// hashes. Distinct from [`MeetingAttendanceReceipt::DOMAIN_TAG`] so
    /// the v1 and v2 namespaces remain fully separate.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:meeting_attendance:v2";

    /// Build a new v2 receipt and compute its canonical `record_hash`.
    /// Rejects empty/whitespace `capability_scope_presented`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        meeting_id: String,
        domain_id: String,
        attendee_did: String,
        recorded_by: String,
        transition: MeetingAttendanceTransition,
        recorded_at: u64,
        capability_scope_presented: String,
        mandate_attestation: ReceiptMandateAttestation,
    ) -> Result<Self, MeetingAttendanceReceiptV2Error> {
        if capability_scope_presented.trim().is_empty() {
            return Err(MeetingAttendanceReceiptV2Error::EmptyCapabilityScope);
        }
        if matches!(
            mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ) {
            return Err(MeetingAttendanceReceiptV2Error::UnsupportedAttestation);
        }
        let record_hash = Self::compute_record_hash(
            &meeting_id,
            &domain_id,
            &attendee_did,
            &recorded_by,
            transition,
            recorded_at,
            &capability_scope_presented,
            &mandate_attestation,
        );
        Ok(Self {
            meeting_id,
            domain_id,
            attendee_did,
            recorded_by,
            transition,
            recorded_at,
            capability_scope_presented,
            mandate_attestation,
            record_hash,
        })
    }

    /// Compute the canonical v2 `record_hash` from receipt fields.
    ///
    /// The byte stream is: v2 [`Self::DOMAIN_TAG`], then the v1 base
    /// fields via [`absorb_meeting_attendance_base_field_bytes`] (length-
    /// prefixed strings, single-byte transition ordinal, u64 LE
    /// `recorded_at`, identical order to v1), then the v2-only additions:
    /// length-prefixed `capability_scope_presented`, a single ordinal byte
    /// for the [`ReceiptMandateAttestation`] variant (`NoMandateRequired`
    /// = 0, `Grant` = 1), then the per-variant payload:
    ///
    /// - `NoMandateRequired { reason }`: a single
    ///   [`no_mandate_reason_ordinal`] byte.
    /// - `Grant { grant_ref }`: the 32-byte
    ///   [`MandateGrantRef::ref_hash`] (fixed-length; no length prefix).
    ///   Binding via `ref_hash` propagates the grant ref's per-component
    ///   canonical encoding (#1928) without re-deriving the field layout
    ///   here.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_record_hash(
        meeting_id: &str,
        domain_id: &str,
        attendee_did: &str,
        recorded_by: &str,
        transition: MeetingAttendanceTransition,
        recorded_at: u64,
        capability_scope_presented: &str,
        mandate_attestation: &ReceiptMandateAttestation,
    ) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        absorb_meeting_attendance_base_field_bytes(
            &mut hasher,
            meeting_id,
            domain_id,
            attendee_did,
            recorded_by,
            transition,
            recorded_at,
        );
        hasher.update(&(capability_scope_presented.len() as u64).to_le_bytes());
        hasher.update(capability_scope_presented.as_bytes());
        hasher.update(&[attestation_kind_ordinal(mandate_attestation)]);
        match mandate_attestation {
            ReceiptMandateAttestation::NoMandateRequired { reason } => {
                hasher.update(&[no_mandate_reason_ordinal(*reason)]);
            }
            ReceiptMandateAttestation::Grant { grant_ref } => {
                // Bind via the grant ref's canonical hash (#1928). Fixed
                // length (32 bytes), no length prefix.
                hasher.update(&grant_ref.ref_hash());
            }
            ReceiptMandateAttestation::ProcessAuthorized => {
                // `ProcessAuthorized` is a governance-decision authority mode;
                // this receipt never holds it — `new()`/`try_from` reject it —
                // so a validly-constructed receipt never reaches this arm.
                // `compute_record_hash` is public, so a direct caller could
                // still pass it; `verify()` fail-closes on it. No per-variant
                // payload — exhaustiveness only.
            }
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }

    /// Verify the stored `record_hash` against canonical v2 receipt
    /// fields.
    pub fn verify(&self) -> bool {
        // Fail-closed on the v3-only `ProcessAuthorized` mode (see the v2
        // decision receipt's `verify` for the rationale: `pub` fields allow a
        // direct struct literal to bypass the `new()`/`try_from` rejection).
        if matches!(
            self.mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ) {
            return false;
        }
        let recomputed = Self::compute_record_hash(
            &self.meeting_id,
            &self.domain_id,
            &self.attendee_did,
            &self.recorded_by,
            self.transition,
            self.recorded_at,
            &self.capability_scope_presented,
            &self.mandate_attestation,
        );
        self.record_hash == recomputed
    }
}

/// Raw deserialization shadow for [`MeetingAttendanceReceiptV2`].
///
/// `MeetingAttendanceReceiptV2` is a wire/persisted primitive, so
/// `Deserialize` is an input boundary that must apply the same checks as
/// the constructor. Routing deserialization through this shadow +
/// `try_from` keeps the wire surface symmetric with `new` — a payload with
/// an empty `capability_scope_presented` fails closed at the
/// deserialization boundary, not only via `new`. Mirrors the pattern
/// established by `MandateGrantRefRaw` (#1928),
/// `GovernanceDecisionReceiptV2Raw` (#1929), and
/// `ActionItemCompletionReceiptV2Raw` (#1930).
#[derive(Deserialize)]
struct MeetingAttendanceReceiptV2Raw {
    meeting_id: String,
    domain_id: String,
    attendee_did: String,
    recorded_by: String,
    transition: MeetingAttendanceTransition,
    recorded_at: u64,
    capability_scope_presented: String,
    mandate_attestation: ReceiptMandateAttestation,
    record_hash: Hash,
}

impl TryFrom<MeetingAttendanceReceiptV2Raw> for MeetingAttendanceReceiptV2 {
    type Error = MeetingAttendanceReceiptV2Error;

    fn try_from(raw: MeetingAttendanceReceiptV2Raw) -> Result<Self, Self::Error> {
        if raw.capability_scope_presented.trim().is_empty() {
            return Err(MeetingAttendanceReceiptV2Error::EmptyCapabilityScope);
        }
        if matches!(
            raw.mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ) {
            return Err(MeetingAttendanceReceiptV2Error::UnsupportedAttestation);
        }
        // Matches v1 behavior: `Deserialize` accepts whatever
        // `record_hash` value the wire carries; callers verify integrity
        // via `verify()` separately. Keeps the deserialization surface a
        // pure structural check.
        Ok(Self {
            meeting_id: raw.meeting_id,
            domain_id: raw.domain_id,
            attendee_did: raw.attendee_did,
            recorded_by: raw.recorded_by,
            transition: raw.transition,
            recorded_at: raw.recorded_at,
            capability_scope_presented: raw.capability_scope_presented,
            mandate_attestation: raw.mandate_attestation,
            record_hash: raw.record_hash,
        })
    }
}

// ============================================================================
// Process gate result receipts (ADR-0026 Layer 2 — first
// `ProcessTransitionReceipt` class for the `idea-0019` Institutional
// Process Substrate runtime slice)
// ============================================================================

/// Closed taxonomy of process gate kinds that produce a
/// [`ProcessGateResultReceipt`].
///
/// The variants mirror the six gate kinds named in the `idea-0019`
/// Institutional Process Substrate read-model fixture-walk dogfood
/// (`ops/ideas/dogfood/institutional-process-substrate-mvp.md` Step 5
/// gate table). The taxonomy is closed: the runtime never emits a gate
/// kind not listed here. Adding a kind requires a separate change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessGateKind {
    /// Privacy-review gate. Confirms no private data leaks into a
    /// process artifact.
    PrivacyReview,
    /// Accessibility-review gate. Confirms the surface meets the
    /// accessibility baseline before promotion to organizer-ready.
    AccessibilityReview,
    /// Repo-safety gate. Confirms only repo-safe material is exported.
    RepoSafetyReview,
    /// Scope-confirmation gate. Confirms the process target is in
    /// scope for the recording body.
    ScopeConfirmation,
    /// No-mutation gate. Confirms a step writes nothing to the
    /// runtime — read-model only.
    NoMutationCheck,
    /// Second-reviewer-signoff gate. Confirms an alternate reviewer
    /// has signed off (used by some institution charters).
    SecondReviewerSignoff,
}

/// Closed taxonomy of process gate results that produce a
/// [`ProcessGateResultReceipt`].
///
/// `Pass` and `Fail` are receipt-bearing results. The framing brief's
/// `n/a` value (a gate the charter does not require for this session
/// kind) does not produce a receipt — the absence of a receipt for a
/// gate that was not asserted is itself the institutional record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessGateResult {
    /// The gate evaluated and passed.
    Pass,
    /// The gate evaluated and failed.
    Fail,
}

/// Cross-node deterministic gate-result receipt for a process session.
///
/// Sits alongside [`GovernanceDecisionReceipt`],
/// [`ActionItemCompletionReceipt`], and [`MeetingAttendanceReceipt`] in
/// ADR-0026 Layer 2. Records the *fact* of a process gate evaluation
/// the runtime can attest to, keyed so an audit chain can be
/// reconstructed by `(session_id, gate_kind)` ordered by `recorded_at`.
///
/// This is the first `ProcessTransitionReceipt` class emitted by the
/// runtime — the smallest receipt-backed slice that exercises the
/// `idea-0019` Institutional Process Substrate spine. It does **not**
/// implement a full process runtime, does **not** introduce a schema
/// or contract URN, and does **not** promote `idea-0019` to RFC.
///
/// Equality is anchored to `record_hash`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessGateResultReceipt {
    /// Identifier for the process session this gate result attaches
    /// to. Caller-provided, treated as opaque by the runtime.
    pub session_id: String,
    /// Governance domain the session is scoped to.
    pub domain_id: String,
    /// Closed-taxonomy gate kind. See [`ProcessGateKind`].
    pub gate_kind: ProcessGateKind,
    /// Closed pass/fail result. See [`ProcessGateResult`].
    pub result: ProcessGateResult,
    /// DID of the actor who recorded this gate result. The
    /// authenticated caller of the manager method that emitted the
    /// receipt; no separate "subject" exists for a gate result.
    pub recorded_by: String,
    /// Unix-seconds the result was recorded.
    pub recorded_at: u64,
    /// blake3 canonical record hash binding the fields above.
    pub record_hash: Hash,
}

impl PartialEq for ProcessGateResultReceipt {
    fn eq(&self, other: &Self) -> bool {
        self.record_hash == other.record_hash
    }
}

impl Eq for ProcessGateResultReceipt {}

impl ProcessGateResultReceipt {
    /// Domain separation tag for canonical process-gate-result record
    /// hashes. Distinct from the proposal-decision, action-item-
    /// completion, and meeting-attendance tags so a record can never
    /// collide across receipt types.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:process_gate_result:v1";

    /// Build a new receipt and compute its canonical `record_hash`.
    pub fn new(
        session_id: String,
        domain_id: String,
        gate_kind: ProcessGateKind,
        result: ProcessGateResult,
        recorded_by: String,
        recorded_at: u64,
    ) -> Self {
        let record_hash = Self::compute_record_hash(
            &session_id,
            &domain_id,
            gate_kind,
            result,
            &recorded_by,
            recorded_at,
        );
        Self {
            session_id,
            domain_id,
            gate_kind,
            result,
            recorded_by,
            recorded_at,
            record_hash,
        }
    }

    /// Compute the canonical record hash from the input fields.
    ///
    /// String inputs are length-prefixed (u64 LE) under
    /// [`Self::DOMAIN_TAG`] so no two distinct field bindings can
    /// produce the same hash. Enum inputs are hashed as single bytes
    /// in declaration order; adding an enum variant after the last
    /// variant in either enum is non-breaking for previously-emitted
    /// hashes.
    pub fn compute_record_hash(
        session_id: &str,
        domain_id: &str,
        gate_kind: ProcessGateKind,
        result: ProcessGateResult,
        recorded_by: &str,
        recorded_at: u64,
    ) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        for field in [session_id, domain_id, recorded_by] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
        }
        hasher.update(&[gate_kind_ordinal(gate_kind)]);
        hasher.update(&[gate_result_ordinal(result)]);
        hasher.update(&recorded_at.to_le_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }
}

/// Map [`ProcessGateKind`] to a deterministic ordinal for hashing.
fn gate_kind_ordinal(kind: ProcessGateKind) -> u8 {
    match kind {
        ProcessGateKind::PrivacyReview => 0,
        ProcessGateKind::AccessibilityReview => 1,
        ProcessGateKind::RepoSafetyReview => 2,
        ProcessGateKind::ScopeConfirmation => 3,
        ProcessGateKind::NoMutationCheck => 4,
        ProcessGateKind::SecondReviewerSignoff => 5,
    }
}

/// Map [`ProcessGateResult`] to a deterministic ordinal for hashing.
fn gate_result_ordinal(result: ProcessGateResult) -> u8 {
    match result {
        ProcessGateResult::Pass => 0,
        ProcessGateResult::Fail => 1,
    }
}

// ============================================================================
// Mandate grant reference (#1868 step 2 primitive — wire form only)
// ============================================================================

/// Wire-side discriminated target of a [`MandateGrantRef`].
///
/// This is the receipt-recordable form of the app-side
/// `apps/governance::mandate_gate::MandateTarget`. Per-variant fields stay
/// structurally separate (never collapsed into a single opaque key
/// string) so the canonical hash is unambiguous and each component can be
/// validated independently.
///
/// Federation identifiers are raw strings in this crate (matching
/// `FederationProposal`'s shape); there is no `FederationId` newtype.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum MandateGrantRefTarget {
    /// A governance domain, keyed by its identifier.
    Domain {
        /// The governance domain id.
        domain_id: String,
    },
    /// A specific proposal.
    Proposal {
        /// The proposal id.
        proposal_id: String,
    },
    /// A role seat in a structure, held by a specific DID.
    Role {
        /// The structure id the role belongs to.
        structure_id: String,
        /// The DID holding (or to hold) the role.
        holder: String,
    },
    /// A federation network, keyed by its raw string identifier.
    Federation {
        /// The federation id.
        federation_id: String,
    },
}

/// Errors returned by [`MandateGrantRef::new`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MandateGrantRefError {
    /// The `act` field was empty or whitespace-only.
    #[error("act must be a non-empty, non-whitespace string")]
    EmptyAct,
    /// A target component (`domain_id` / `proposal_id` / `structure_id` /
    /// `holder` / `federation_id`) was empty or whitespace-only. The
    /// inner str names the offending field.
    #[error("target field `{0}` must be a non-empty, non-whitespace string")]
    EmptyTargetField(&'static str),
    /// `Role.holder` was not a parseable `did:icn:` identifier. The wire
    /// boundary keeps parity with the app-side `MandateTarget::Role.holder`,
    /// which is an `icn_identity::Did`; a non-DID wire value would slip
    /// past `EmptyTargetField` and silently hash, so the constructor (and
    /// `Deserialize` via the same path) rejects it. Inner string is the
    /// underlying parse error for diagnosis.
    #[error("target field `holder` must be a parseable did:icn: identifier: {0}")]
    InvalidHolderDid(String),
}

/// Receipt-recordable reference to the mandate that authorized an act.
///
/// **Wire-format primitive only.** This type does **not** yet appear in
/// the body of any existing receipt
/// ([`GovernanceDecisionReceipt`], [`ActionItemCompletionReceipt`],
/// [`MeetingAttendanceReceipt`]). Extending those receipts to embed
/// `Option<MandateGrantRef>` (plus the `capability_scope_presented`
/// field) is the next slice in the #1868 ladder; this PR only freezes
/// the reference's wire shape and canonical hash so the next slice has a
/// stable type to compose.
///
/// See `docs/design/governance/mandate-gate-design.md` §7 and
/// `docs/design/governance/governance-write-decomposition.md` §10 step 2.
///
/// The app-side
/// `apps/governance::mandate_gate::MandateGrant::into_ref` adapter
/// produces values of this type from the act-time gate result; the
/// adapter direction is `apps/governance → icn-governance`, never the
/// other way (the meaning firewall stays intact — kernel crates still
/// see nothing of `MandateAct`/`MandateTarget`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "MandateGrantRefRaw")]
pub struct MandateGrantRef {
    /// The authorizing mandate's identifier.
    pub mandate_id: MandateId,
    /// The decision the mandate is grounded in (binds the
    /// `Charter → Decision → Mandate → Action → Receipt` chain).
    pub decision_hash: Hash,
    /// Snake_case institutional act discriminator (e.g.
    /// `"activate_charter"`, `"add_domain_member"`, `"close_proposal"`).
    /// Stored as a string at the wire boundary so adding a new
    /// `MandateAct` variant on the app side does not break receipt
    /// verification.
    pub act: String,
    /// The structured target the act was authorized against.
    pub target: MandateGrantRefTarget,
    /// Unix-seconds the gate granted the act. Aligned with the mandate's
    /// own `Timestamp` (seconds) convention.
    pub granted_at: u64,
}

/// Raw deserialization shadow for [`MandateGrantRef`].
///
/// `MandateGrantRef` is a wire/persisted primitive, so `Deserialize` is
/// an input boundary that must apply the same checks as the public
/// constructor. Deriving `Deserialize` directly on `MandateGrantRef`
/// would bypass [`MandateGrantRef::new`] and let an empty `act` or a
/// whitespace-only target component round-trip silently. Routing
/// deserialization through this shadow + `try_from` keeps the wire
/// surface symmetric with the constructor — invalid wire data fails
/// closed at the deserialization boundary.
#[derive(Deserialize)]
struct MandateGrantRefRaw {
    mandate_id: MandateId,
    decision_hash: Hash,
    act: String,
    target: MandateGrantRefTarget,
    granted_at: u64,
}

impl TryFrom<MandateGrantRefRaw> for MandateGrantRef {
    type Error = MandateGrantRefError;

    fn try_from(raw: MandateGrantRefRaw) -> Result<Self, Self::Error> {
        Self::new(
            raw.mandate_id,
            raw.decision_hash,
            raw.act,
            raw.target,
            raw.granted_at,
        )
    }
}

impl MandateGrantRef {
    /// Domain separation tag for canonical mandate-grant-ref hashes.
    /// Distinct from every existing receipt-type tag so a reference can
    /// never collide with a receipt body hash.
    pub const DOMAIN_TAG: &[u8] = b"icn:gov:mandate_grant_ref:v1";

    /// Construct a reference, rejecting empty/whitespace-only string
    /// fields. Use this constructor rather than struct-literal
    /// construction to keep the canonical hash inputs well-formed.
    pub fn new(
        mandate_id: MandateId,
        decision_hash: Hash,
        act: String,
        target: MandateGrantRefTarget,
        granted_at: u64,
    ) -> Result<Self, MandateGrantRefError> {
        if act.trim().is_empty() {
            return Err(MandateGrantRefError::EmptyAct);
        }
        Self::validate_target(&target)?;
        Ok(Self {
            mandate_id,
            decision_hash,
            act,
            target,
            granted_at,
        })
    }

    fn validate_target(target: &MandateGrantRefTarget) -> Result<(), MandateGrantRefError> {
        match target {
            MandateGrantRefTarget::Domain { domain_id } => {
                if domain_id.trim().is_empty() {
                    return Err(MandateGrantRefError::EmptyTargetField("domain_id"));
                }
            }
            MandateGrantRefTarget::Proposal { proposal_id } => {
                if proposal_id.trim().is_empty() {
                    return Err(MandateGrantRefError::EmptyTargetField("proposal_id"));
                }
            }
            MandateGrantRefTarget::Role {
                structure_id,
                holder,
            } => {
                if structure_id.trim().is_empty() {
                    return Err(MandateGrantRefError::EmptyTargetField("structure_id"));
                }
                if holder.trim().is_empty() {
                    return Err(MandateGrantRefError::EmptyTargetField("holder"));
                }
                // App-side `MandateTarget::Role.holder` is `icn_identity::Did`,
                // whose deserializer enforces `did:icn:` + Ed25519 multibase.
                // The wire form is a `String`, so without this check a payload
                // like `"holder": "not-a-did"` would pass the empty-string gate
                // and still receive a canonical `ref_hash()`. Run the same
                // parser at the wire boundary so malformed holders fail closed
                // here and via `Deserialize`.
                if let Err(e) = icn_identity::Did::from_str(holder) {
                    return Err(MandateGrantRefError::InvalidHolderDid(e.to_string()));
                }
            }
            MandateGrantRefTarget::Federation { federation_id } => {
                if federation_id.trim().is_empty() {
                    return Err(MandateGrantRefError::EmptyTargetField("federation_id"));
                }
            }
        }
        Ok(())
    }

    /// Compute the canonical reference hash from the input fields.
    ///
    /// Inputs are length-prefixed (u64 LE) under [`Self::DOMAIN_TAG`],
    /// matching the encoding convention used by
    /// [`ActionItemCompletionReceipt::compute_record_hash`] and the other
    /// receipt-record hash functions in this module. The mandate id is
    /// hashed as its raw 16 UUID bytes (fixed length) and the
    /// `decision_hash` as its raw 32 bytes (fixed length); both omit
    /// length prefixes because the lengths are fixed by the wire type.
    /// Target kind is a single-byte ordinal followed by each component
    /// as its own length-prefixed string.
    pub fn compute_ref_hash(
        mandate_id: &MandateId,
        decision_hash: &Hash,
        act: &str,
        target: &MandateGrantRefTarget,
        granted_at: u64,
    ) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN_TAG);
        // Fixed-length: 16-byte UUID, no length prefix.
        hasher.update(mandate_id.0.as_bytes());
        // Fixed-length: 32-byte blake3 hash, no length prefix.
        hasher.update(decision_hash);
        // Variable-length: act string.
        hasher.update(&(act.len() as u64).to_le_bytes());
        hasher.update(act.as_bytes());
        // Target kind ordinal, then each component length-prefixed.
        hasher.update(&[target_kind_ordinal(target)]);
        match target {
            MandateGrantRefTarget::Domain { domain_id } => {
                hasher.update(&(domain_id.len() as u64).to_le_bytes());
                hasher.update(domain_id.as_bytes());
            }
            MandateGrantRefTarget::Proposal { proposal_id } => {
                hasher.update(&(proposal_id.len() as u64).to_le_bytes());
                hasher.update(proposal_id.as_bytes());
            }
            MandateGrantRefTarget::Role {
                structure_id,
                holder,
            } => {
                hasher.update(&(structure_id.len() as u64).to_le_bytes());
                hasher.update(structure_id.as_bytes());
                hasher.update(&(holder.len() as u64).to_le_bytes());
                hasher.update(holder.as_bytes());
            }
            MandateGrantRefTarget::Federation { federation_id } => {
                hasher.update(&(federation_id.len() as u64).to_le_bytes());
                hasher.update(federation_id.as_bytes());
            }
        }
        hasher.update(&granted_at.to_le_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }

    /// Compute this reference's canonical hash. Convenience wrapper
    /// around [`Self::compute_ref_hash`] over the receiver's fields.
    pub fn ref_hash(&self) -> Hash {
        Self::compute_ref_hash(
            &self.mandate_id,
            &self.decision_hash,
            &self.act,
            &self.target,
            self.granted_at,
        )
    }
}

/// Map [`MandateGrantRefTarget`] to a deterministic ordinal for hashing.
///
/// Ordinals are fixed at v1 of the wire format; adding a variant after
/// the existing four requires a new domain-separation tag (`:v2`).
fn target_kind_ordinal(target: &MandateGrantRefTarget) -> u8 {
    match target {
        MandateGrantRefTarget::Domain { .. } => 0,
        MandateGrantRefTarget::Proposal { .. } => 1,
        MandateGrantRefTarget::Role { .. } => 2,
        MandateGrantRefTarget::Federation { .. } => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vote::Vote;

    // Use deterministic DIDs for reproducible tests via SigningKey from fixed bytes
    fn make_deterministic_dids() -> (icn_identity::Did, icn_identity::Did, icn_identity::Did) {
        let sk1 = ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]);
        let sk2 = ed25519_dalek::SigningKey::from_bytes(&[2u8; 32]);
        let sk3 = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
        let d1 = icn_identity::Did::from_public_key(&sk1.verifying_key());
        let d2 = icn_identity::Did::from_public_key(&sk2.verifying_key());
        let d3 = icn_identity::Did::from_public_key(&sk3.verifying_key());
        (d1, d2, d3)
    }

    fn make_votes() -> Vec<Vote> {
        let (alice, bob, carol) = make_deterministic_dids();
        vec![
            Vote {
                proposal_id: crate::ProposalId::new("prop-1"),
                voter: alice,
                choice: VoteChoice::For,
                weight: 1,
                timestamp: 1700000000,
                comment: None,
            },
            Vote {
                proposal_id: crate::ProposalId::new("prop-1"),
                voter: bob,
                choice: VoteChoice::Against,
                weight: 1,
                timestamp: 1700000001,
                comment: None,
            },
            Vote {
                proposal_id: crate::ProposalId::new("prop-1"),
                voter: carol,
                choice: VoteChoice::For,
                weight: 2,
                timestamp: 1700000002,
                comment: Some("Strongly support".into()),
            },
        ]
    }

    fn make_tally(votes: &[Vote]) -> VoteTally {
        let mut tally = VoteTally::empty();
        for v in votes {
            tally.add_vote(v);
        }
        tally
    }

    fn make_proof() -> GovernanceProof {
        let votes = make_votes();
        let tally = make_tally(&votes);
        GovernanceProof::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
            1700000100,
            "did:icn:node1".to_string(),
        )
    }

    #[test]
    fn proof_hash_determinism() {
        let p1 = make_proof();
        let p2 = make_proof();
        assert_eq!(p1.proof_hash, p2.proof_hash);
        assert_ne!(p1.proof_hash, [0u8; 32]);
    }

    #[test]
    fn verify_binding_succeeds() {
        let proof = make_proof();
        assert!(proof.verify_binding());
    }

    #[test]
    fn tamper_proposal_id_detected() {
        let mut proof = make_proof();
        proof.proposal_id = "prop-evil".to_string();
        assert!(!proof.verify_binding());
    }

    #[test]
    fn tamper_domain_id_detected() {
        let mut proof = make_proof();
        proof.domain_id = "evil-domain".to_string();
        assert!(!proof.verify_binding());
    }

    #[test]
    fn tamper_outcome_detected() {
        let mut proof = make_proof();
        proof.outcome = ProofOutcome::Rejected;
        assert!(!proof.verify_binding());
    }

    #[test]
    fn tamper_vote_tally_detected() {
        let mut proof = make_proof();
        proof.vote_tally.for_votes = 999;
        assert!(!proof.verify_binding());
    }

    #[test]
    fn tamper_vote_hash_detected() {
        let mut proof = make_proof();
        proof.vote_hash = [0xFF; 32];
        assert!(!proof.verify_binding());
    }

    #[test]
    fn tamper_timestamp_detected() {
        let mut proof = make_proof();
        proof.timestamp = 9999;
        assert!(!proof.verify_binding());
    }

    #[test]
    fn tamper_signer_did_detected() {
        let mut proof = make_proof();
        proof.signer_did = "did:icn:attacker".to_string();
        assert!(!proof.verify_binding());
    }

    #[test]
    fn vote_hash_order_independent() {
        let votes = make_votes();
        let hash1 = GovernanceProof::compute_vote_hash(&votes);

        // Reverse order
        let mut reversed = votes.clone();
        reversed.reverse();
        let hash2 = GovernanceProof::compute_vote_hash(&reversed);

        assert_eq!(hash1, hash2, "vote hash must be order-independent");
    }

    #[test]
    fn vote_hash_changes_with_different_votes() {
        let votes1 = make_votes();
        let hash1 = GovernanceProof::compute_vote_hash(&votes1);

        // Change a vote
        let mut votes2 = make_votes();
        let alice_did = votes2[0].voter.clone();
        votes2[0] = Vote {
            proposal_id: crate::ProposalId::new("prop-1"),
            voter: alice_did,
            choice: VoteChoice::Against, // Changed
            weight: 1,
            timestamp: 1700000000,
            comment: None,
        };
        let hash2 = GovernanceProof::compute_vote_hash(&votes2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn vote_hash_empty_votes() {
        let empty: Vec<Vote> = vec![];
        let hash = GovernanceProof::compute_vote_hash(&empty);
        assert_ne!(hash, [0u8; 32]);
    }

    #[test]
    fn domain_tag_is_part_of_hash() {
        let proof = make_proof();
        let with_tag = proof.proof_hash;

        // Compute manually without domain tag — must differ
        let mut hasher = blake3::Hasher::new();
        // Deliberately omit: hasher.update(GovernanceProof::DOMAIN_TAG);
        hasher.update(&(proof.proposal_id.len() as u64).to_le_bytes());
        hasher.update(proof.proposal_id.as_bytes());
        hasher.update(&(proof.domain_id.len() as u64).to_le_bytes());
        hasher.update(proof.domain_id.as_bytes());
        hasher.update(&[outcome_ordinal(proof.outcome)]);
        hasher.update(&(proof.vote_tally.for_votes as u64).to_le_bytes());
        hasher.update(&(proof.vote_tally.against_votes as u64).to_le_bytes());
        hasher.update(&(proof.vote_tally.abstain_votes as u64).to_le_bytes());
        hasher.update(&proof.vote_hash);
        hasher.update(&proof.timestamp.to_le_bytes());
        hasher.update(&(proof.signer_did.len() as u64).to_le_bytes());
        hasher.update(proof.signer_did.as_bytes());
        let without_tag: Hash = *hasher.finalize().as_bytes();

        assert_ne!(with_tag, without_tag, "domain tag must affect hash output");
    }

    #[test]
    fn length_prefix_prevents_field_collision() {
        let votes = make_votes();
        let tally = make_tally(&votes);

        let p1 = GovernanceProof::new(
            "propABC".to_string(),
            "dom:XYZ".to_string(),
            ProofOutcome::Accepted,
            tally.clone(),
            &votes,
            1700000100,
            "did:icn:node1".to_string(),
        );
        let p2 = GovernanceProof::new(
            "propABCdom:XYZ".to_string(),
            "".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
            1700000100,
            "did:icn:node1".to_string(),
        );
        assert_ne!(p1.proof_hash, p2.proof_hash);
        assert!(p1.verify_binding());
        assert!(p2.verify_binding());
    }

    #[test]
    fn signature_starts_empty() {
        let proof = make_proof();
        assert!(proof.signature.is_empty());
    }

    #[test]
    fn sign_and_verify() {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
        let verifying_key = signing_key.verifying_key();

        let mut proof = make_proof();
        proof.sign(&signing_key);

        assert!(!proof.signature.is_empty());
        assert_eq!(proof.signature.len(), 64);
        assert!(proof.verify_signature(&verifying_key));
    }

    #[test]
    fn wrong_key_fails_verification() {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
        let wrong_key = ed25519_dalek::SigningKey::from_bytes(&[99u8; 32]);
        let wrong_verifying = wrong_key.verifying_key();

        let mut proof = make_proof();
        proof.sign(&signing_key);

        assert!(!proof.verify_signature(&wrong_verifying));
    }

    #[test]
    fn tampered_proof_fails_signature() {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
        let verifying_key = signing_key.verifying_key();

        let mut proof = make_proof();
        proof.sign(&signing_key);

        // Tamper with the proof hash after signing
        proof.proof_hash[0] ^= 0xFF;
        assert!(!proof.verify_signature(&verifying_key));
    }

    #[test]
    fn serialization_roundtrip() {
        let proof = make_proof();
        let json = serde_json::to_string(&proof).unwrap();
        let deserialized: GovernanceProof = serde_json::from_str(&json).unwrap();
        assert_eq!(proof, deserialized);
        assert!(deserialized.verify_binding());
    }

    #[test]
    fn signed_serialization_roundtrip() {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
        let verifying_key = signing_key.verifying_key();

        let mut proof = make_proof();
        proof.sign(&signing_key);

        let json = serde_json::to_string(&proof).unwrap();
        let deserialized: GovernanceProof = serde_json::from_str(&json).unwrap();

        assert_eq!(proof, deserialized);
        assert!(deserialized.verify_binding());
        assert!(deserialized.verify_signature(&verifying_key));
    }

    #[test]
    fn decision_hash_is_stable_across_node_local_fields() {
        let votes = make_votes();
        let tally = make_tally(&votes);

        let proof_a = GovernanceProof::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally.clone(),
            &votes,
            1700000100,
            "did:icn:nodeA".to_string(),
        );
        let proof_b = GovernanceProof::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
            1800000100,
            "did:icn:nodeB".to_string(),
        );

        let receipt_a = GovernanceDecisionReceipt::from_legacy(&proof_a);
        let receipt_b = GovernanceDecisionReceipt::from_legacy(&proof_b);

        assert_eq!(receipt_a.decision_hash, receipt_b.decision_hash);
    }

    #[test]
    fn decision_hash_changes_when_votes_change() {
        let mut votes1 = make_votes();
        let mut votes2 = make_votes();
        votes2[0].choice = VoteChoice::Against;

        let receipt1 = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            make_tally(&votes1),
            &votes1,
        );
        let receipt2 = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            make_tally(&votes2),
            &votes2,
        );

        assert_ne!(receipt1.decision_hash, receipt2.decision_hash);
        votes1.reverse();
        let reordered = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            make_tally(&votes1),
            &votes1,
        );
        assert_eq!(receipt1.decision_hash, reordered.decision_hash);
    }

    #[test]
    fn attestation_signature_roundtrip() {
        let signer = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let vk = signer.verifying_key();
        let receipt = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            make_tally(&make_votes()),
            &make_votes(),
        );
        let attestation = GovernanceDecisionAttestation::sign(
            receipt.decision_hash,
            "did:icn:node1".to_string(),
            1700001111,
            &signer,
        );
        assert!(attestation.verify(&vk));
    }

    #[test]
    fn proof_v2_equality_uses_decision_hash_only() {
        let votes = make_votes();
        let receipt = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            make_tally(&votes),
            &votes,
        );
        let signer_a = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
        let signer_b = ed25519_dalek::SigningKey::from_bytes(&[10u8; 32]);
        let a = GovernanceProofV2::new(
            receipt.clone(),
            vec![GovernanceDecisionAttestation::sign(
                receipt.decision_hash,
                "did:icn:a".to_string(),
                1700000001,
                &signer_a,
            )],
        );
        let b = GovernanceProofV2::new(
            receipt,
            vec![GovernanceDecisionAttestation::sign(
                a.receipt.decision_hash,
                "did:icn:b".to_string(),
                1700000002,
                &signer_b,
            )],
        );
        assert_eq!(a, b);
    }

    #[test]
    fn governance_decision_receipt_serde_roundtrip() {
        let votes = make_votes();
        let receipt = GovernanceDecisionReceipt::new(
            "prop-stage7-test".to_string(),
            "coop:test-coop".to_string(),
            ProofOutcome::Accepted,
            make_tally(&votes),
            &votes,
        );

        // Test JSON roundtrip
        let json = serde_json::to_string(&receipt).expect("serialize to JSON");
        let recovered: GovernanceDecisionReceipt =
            serde_json::from_str(&json).expect("deserialize from JSON");

        // Verify all fields match (using decision_hash for equality per PartialEq impl)
        assert_eq!(receipt, recovered);
        // Also verify individual fields for comprehensive coverage
        assert_eq!(receipt.proposal_id, recovered.proposal_id);
        assert_eq!(receipt.domain_id, recovered.domain_id);
        assert_eq!(receipt.outcome, recovered.outcome);
        assert_eq!(receipt.vote_tally, recovered.vote_tally);
        assert_eq!(receipt.vote_hash, recovered.vote_hash);
        assert_eq!(receipt.decision_hash, recovered.decision_hash);
    }

    // ========================================================================
    // ProcessGateResultReceipt unit tests (the receipt's deterministic
    // hash binding; runtime emission is covered by an integration test
    // in apps/governance/tests/process_gate_result_receipt_runtime_slice.rs).
    // ========================================================================

    fn sample_gate_receipt() -> ProcessGateResultReceipt {
        ProcessGateResultReceipt::new(
            "session-fixture-001".to_string(),
            "coop:test".to_string(),
            ProcessGateKind::PrivacyReview,
            ProcessGateResult::Pass,
            "did:icn:facilitator-fixture".to_string(),
            1_700_000_300,
        )
    }

    #[test]
    fn process_gate_result_record_hash_determinism() {
        let r1 = sample_gate_receipt();
        let r2 = sample_gate_receipt();
        assert_eq!(r1.record_hash, r2.record_hash);
        assert_ne!(r1.record_hash, [0u8; 32]);
        assert_eq!(r1, r2);
    }

    #[test]
    fn process_gate_result_record_hash_changes_with_session_id() {
        let r1 = sample_gate_receipt();
        let r2 = ProcessGateResultReceipt::new(
            "different-session".to_string(),
            r1.domain_id.clone(),
            r1.gate_kind,
            r1.result,
            r1.recorded_by.clone(),
            r1.recorded_at,
        );
        assert_ne!(r1.record_hash, r2.record_hash);
    }

    #[test]
    fn process_gate_result_record_hash_changes_with_gate_kind() {
        let r1 = sample_gate_receipt();
        let r2 = ProcessGateResultReceipt::new(
            r1.session_id.clone(),
            r1.domain_id.clone(),
            ProcessGateKind::AccessibilityReview,
            r1.result,
            r1.recorded_by.clone(),
            r1.recorded_at,
        );
        assert_ne!(r1.record_hash, r2.record_hash);
    }

    #[test]
    fn process_gate_result_record_hash_changes_with_result() {
        let r1 = sample_gate_receipt();
        let r2 = ProcessGateResultReceipt::new(
            r1.session_id.clone(),
            r1.domain_id.clone(),
            r1.gate_kind,
            ProcessGateResult::Fail,
            r1.recorded_by.clone(),
            r1.recorded_at,
        );
        assert_ne!(r1.record_hash, r2.record_hash);
    }

    #[test]
    fn process_gate_result_record_hash_changes_with_recorded_by() {
        let r1 = sample_gate_receipt();
        let r2 = ProcessGateResultReceipt::new(
            r1.session_id.clone(),
            r1.domain_id.clone(),
            r1.gate_kind,
            r1.result,
            "did:icn:steward-other".to_string(),
            r1.recorded_at,
        );
        assert_ne!(r1.record_hash, r2.record_hash);
    }

    #[test]
    fn process_gate_result_record_hash_changes_with_recorded_at() {
        let r1 = sample_gate_receipt();
        let r2 = ProcessGateResultReceipt::new(
            r1.session_id.clone(),
            r1.domain_id.clone(),
            r1.gate_kind,
            r1.result,
            r1.recorded_by.clone(),
            r1.recorded_at + 1,
        );
        assert_ne!(r1.record_hash, r2.record_hash);
    }

    #[test]
    fn process_gate_result_domain_tag_is_part_of_hash() {
        let r = sample_gate_receipt();
        // Recompute manually without the domain tag and confirm the
        // result differs — the tag must affect the hash so this
        // receipt class can never collide with another receipt
        // class's binding under the same field bytes.
        let mut hasher = blake3::Hasher::new();
        // Deliberately omit: hasher.update(ProcessGateResultReceipt::DOMAIN_TAG);
        for field in [
            r.session_id.as_str(),
            r.domain_id.as_str(),
            r.recorded_by.as_str(),
        ] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
        }
        hasher.update(&[gate_kind_ordinal(r.gate_kind)]);
        hasher.update(&[gate_result_ordinal(r.result)]);
        hasher.update(&r.recorded_at.to_le_bytes());
        let untagged: Hash = *hasher.finalize().as_bytes();
        assert_ne!(r.record_hash, untagged);
    }

    #[test]
    fn process_gate_result_length_prefix_prevents_field_collision() {
        let r1 = ProcessGateResultReceipt::new(
            "alpha".to_string(),
            "coop:beta".to_string(),
            ProcessGateKind::ScopeConfirmation,
            ProcessGateResult::Pass,
            "did:icn:r".to_string(),
            42,
        );
        // Concatenate session_id + domain_id into the session_id with
        // an empty domain_id: bare-string encoding would collide; the
        // length-prefix scheme must keep them distinct.
        let r2 = ProcessGateResultReceipt::new(
            "alphacoop:beta".to_string(),
            "".to_string(),
            ProcessGateKind::ScopeConfirmation,
            ProcessGateResult::Pass,
            "did:icn:r".to_string(),
            42,
        );
        assert_ne!(r1.record_hash, r2.record_hash);
    }

    #[test]
    fn process_gate_result_serde_roundtrip() {
        let r = sample_gate_receipt();
        let json = serde_json::to_string(&r).expect("serialize");
        let recovered: ProcessGateResultReceipt = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(r, recovered);
        assert_eq!(r.session_id, recovered.session_id);
        assert_eq!(r.domain_id, recovered.domain_id);
        assert_eq!(r.gate_kind, recovered.gate_kind);
        assert_eq!(r.result, recovered.result);
        assert_eq!(r.recorded_by, recovered.recorded_by);
        assert_eq!(r.recorded_at, recovered.recorded_at);
        assert_eq!(r.record_hash, recovered.record_hash);
    }

    #[test]
    fn process_gate_result_serde_uses_snake_case_for_enums() {
        // Confirm the wire form keeps the closed taxonomies stable
        // under serde rename_all = "snake_case". The hash is bound to
        // ordinals (not names), but the serialized form is the
        // contract a holder shell would consume.
        let r = sample_gate_receipt();
        let json = serde_json::to_string(&r).expect("serialize");
        assert!(json.contains("\"gate_kind\":\"privacy_review\""));
        assert!(json.contains("\"result\":\"pass\""));
    }

    #[test]
    fn process_gate_result_no_regulated_finance_vocabulary() {
        // The ProcessGateResultReceipt is not an economic record. Its
        // serialized form must not echo regulated-finance terms,
        // which would invite a wrong reading at the regulatory
        // surface. Vocabulary discipline matches the rest of the
        // receipt family (see the existing meeting-attendance test
        // for the same set of forbidden terms).
        let r = sample_gate_receipt();
        let json = serde_json::to_string(&r).expect("serialize");
        let lower = json.to_lowercase();
        for forbidden in [
            "wallet", "balance", "currency", "payment", "token", "withdraw", "deposit",
        ] {
            assert!(
                !lower.contains(forbidden),
                "ProcessGateResultReceipt JSON must not contain regulated-finance vocabulary; \
                 found `{forbidden}` in: {json}"
            );
        }
    }

    // ============================================================================
    // MandateGrantRef (#1868 step 2 primitive)
    // ============================================================================

    fn fixed_mandate_id(byte: u8) -> MandateId {
        // Build a deterministic UUID from a 16-byte seed so canonical
        // hash assertions are stable across runs. `uuid::Uuid::from_bytes`
        // accepts any 16-byte sequence and does not set RFC-4122
        // version/variant bits, so the result is not a v4 UUID — that is
        // fine here because the canonical hash binds the raw bytes, not
        // the version field.
        MandateId(uuid::Uuid::from_bytes([byte; 16]))
    }

    fn sample_ref(target: MandateGrantRefTarget) -> MandateGrantRef {
        MandateGrantRef::new(
            fixed_mandate_id(0xAA),
            [0x11; 32],
            "activate_charter".to_string(),
            target,
            1_700_000_000,
        )
        .expect("sample ref must construct cleanly")
    }

    /// Deterministic, valid `did:icn:` string for use in `Role.holder`
    /// fixtures. `Did::from_anchor_id` accepts arbitrary 32-byte input but
    /// the constructor now parses through `Did::from_str`, which requires
    /// the decoded bytes to be a valid Ed25519 public key — so fixtures
    /// must derive the DID from a real signing key.
    fn fixture_holder_did(seed: u8) -> String {
        let sk = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
        icn_identity::Did::from_public_key(&sk.verifying_key()).to_string()
    }

    #[test]
    fn mandate_grant_ref_hash_is_deterministic_across_calls() {
        let r = sample_ref(MandateGrantRefTarget::Domain {
            domain_id: "coop-a".to_string(),
        });
        let h1 = r.ref_hash();
        let h2 = r.ref_hash();
        let h3 = MandateGrantRef::compute_ref_hash(
            &r.mandate_id,
            &r.decision_hash,
            &r.act,
            &r.target,
            r.granted_at,
        );
        assert_eq!(h1, h2);
        assert_eq!(h1, h3);
    }

    #[test]
    fn mandate_grant_ref_distinct_target_kinds_produce_distinct_hashes() {
        // Use the same key string in each variant so the only difference
        // is the kind ordinal. This proves the discriminator participates
        // in the hash and that two variants cannot collide by reusing
        // an identifier.
        let key = "shared-key".to_string();
        let domain = sample_ref(MandateGrantRefTarget::Domain {
            domain_id: key.clone(),
        });
        let proposal = sample_ref(MandateGrantRefTarget::Proposal {
            proposal_id: key.clone(),
        });
        let federation = sample_ref(MandateGrantRefTarget::Federation {
            federation_id: key.clone(),
        });
        let role = sample_ref(MandateGrantRefTarget::Role {
            structure_id: key.clone(),
            holder: fixture_holder_did(1),
        });
        let mut seen = std::collections::HashSet::new();
        for h in [
            domain.ref_hash(),
            proposal.ref_hash(),
            federation.ref_hash(),
            role.ref_hash(),
        ] {
            assert!(seen.insert(h), "target kinds must hash to distinct values");
        }
    }

    #[test]
    fn mandate_grant_ref_role_structure_id_changes_hash() {
        // Holder constant; structure_id varies. Per-component fields
        // must each participate in the canonical hash so a flat-string
        // packing ambiguity cannot recur.
        let holder = fixture_holder_did(1);
        let a = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: holder.clone(),
        });
        let b = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-2".to_string(),
            holder,
        });
        assert_ne!(a.ref_hash(), b.ref_hash());
    }

    #[test]
    fn mandate_grant_ref_role_holder_changes_hash() {
        // structure_id constant; holder varies.
        let a = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: fixture_holder_did(1),
        });
        let b = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: fixture_holder_did(2),
        });
        assert_ne!(a.ref_hash(), b.ref_hash());
    }

    #[test]
    fn mandate_grant_ref_domain_id_changes_hash() {
        let a = sample_ref(MandateGrantRefTarget::Domain {
            domain_id: "coop-a".to_string(),
        });
        let b = sample_ref(MandateGrantRefTarget::Domain {
            domain_id: "coop-b".to_string(),
        });
        assert_ne!(a.ref_hash(), b.ref_hash());
    }

    #[test]
    fn mandate_grant_ref_proposal_id_changes_hash() {
        let a = sample_ref(MandateGrantRefTarget::Proposal {
            proposal_id: "prop-1".to_string(),
        });
        let b = sample_ref(MandateGrantRefTarget::Proposal {
            proposal_id: "prop-2".to_string(),
        });
        assert_ne!(a.ref_hash(), b.ref_hash());
    }

    #[test]
    fn mandate_grant_ref_federation_id_changes_hash() {
        let a = sample_ref(MandateGrantRefTarget::Federation {
            federation_id: "fed-a".to_string(),
        });
        let b = sample_ref(MandateGrantRefTarget::Federation {
            federation_id: "fed-b".to_string(),
        });
        assert_ne!(a.ref_hash(), b.ref_hash());
    }

    #[test]
    fn mandate_grant_ref_act_changes_hash() {
        let r = sample_ref(MandateGrantRefTarget::Domain {
            domain_id: "coop-a".to_string(),
        });
        let other = MandateGrantRef::new(
            r.mandate_id.clone(),
            r.decision_hash,
            "add_domain_member".to_string(),
            r.target.clone(),
            r.granted_at,
        )
        .expect("ref with non-empty act");
        assert_ne!(r.ref_hash(), other.ref_hash());
    }

    #[test]
    fn mandate_grant_ref_granted_at_changes_hash() {
        let a = MandateGrantRef::new(
            fixed_mandate_id(0xAA),
            [0x11; 32],
            "activate_charter".to_string(),
            MandateGrantRefTarget::Domain {
                domain_id: "coop-a".to_string(),
            },
            1_700_000_000,
        )
        .unwrap();
        let b = MandateGrantRef::new(
            fixed_mandate_id(0xAA),
            [0x11; 32],
            "activate_charter".to_string(),
            MandateGrantRefTarget::Domain {
                domain_id: "coop-a".to_string(),
            },
            1_700_000_001,
        )
        .unwrap();
        assert_ne!(a.ref_hash(), b.ref_hash());
    }

    #[test]
    fn mandate_grant_ref_decision_hash_changes_ref_hash() {
        let a = MandateGrantRef::new(
            fixed_mandate_id(0xAA),
            [0x11; 32],
            "activate_charter".to_string(),
            MandateGrantRefTarget::Domain {
                domain_id: "coop-a".to_string(),
            },
            1_700_000_000,
        )
        .unwrap();
        let b = MandateGrantRef::new(
            fixed_mandate_id(0xAA),
            [0x22; 32],
            "activate_charter".to_string(),
            MandateGrantRefTarget::Domain {
                domain_id: "coop-a".to_string(),
            },
            1_700_000_000,
        )
        .unwrap();
        assert_ne!(a.ref_hash(), b.ref_hash());
    }

    #[test]
    fn mandate_grant_ref_empty_act_rejected() {
        for bad in ["", "   ", "\t\n"] {
            let err = MandateGrantRef::new(
                fixed_mandate_id(0xAA),
                [0x11; 32],
                bad.to_string(),
                MandateGrantRefTarget::Domain {
                    domain_id: "coop-a".to_string(),
                },
                1_700_000_000,
            )
            .unwrap_err();
            assert_eq!(err, MandateGrantRefError::EmptyAct, "input {bad:?}");
        }
    }

    #[test]
    fn mandate_grant_ref_empty_target_components_rejected() {
        let cases: Vec<(MandateGrantRefTarget, &'static str)> = vec![
            (
                MandateGrantRefTarget::Domain {
                    domain_id: "  ".to_string(),
                },
                "domain_id",
            ),
            (
                MandateGrantRefTarget::Proposal {
                    proposal_id: "".to_string(),
                },
                "proposal_id",
            ),
            (
                MandateGrantRefTarget::Role {
                    structure_id: "\t".to_string(),
                    holder: fixture_holder_did(7),
                },
                "structure_id",
            ),
            (
                MandateGrantRefTarget::Role {
                    structure_id: "office-1".to_string(),
                    holder: "  ".to_string(),
                },
                "holder",
            ),
            (
                MandateGrantRefTarget::Federation {
                    federation_id: "".to_string(),
                },
                "federation_id",
            ),
        ];
        for (target, expected_field) in cases {
            let err = MandateGrantRef::new(
                fixed_mandate_id(0xAA),
                [0x11; 32],
                "activate_charter".to_string(),
                target.clone(),
                1_700_000_000,
            )
            .unwrap_err();
            assert_eq!(
                err,
                MandateGrantRefError::EmptyTargetField(expected_field),
                "target {target:?}"
            );
        }
    }

    #[test]
    fn mandate_grant_ref_deserialize_rejects_empty_act() {
        // Wire-time validation must be symmetric with the constructor:
        // deserializing a payload the constructor would reject must
        // also fail, otherwise peers / persisted receipts could carry
        // malformed mandate refs and still compute a hash over them.
        let valid = sample_ref(MandateGrantRefTarget::Domain {
            domain_id: "coop-a".to_string(),
        });
        let mut value: serde_json::Value =
            serde_json::to_value(&valid).expect("serialize valid ref");
        value["act"] = serde_json::Value::String(String::new());
        let err = serde_json::from_value::<MandateGrantRef>(value).unwrap_err();
        assert!(
            err.to_string().contains("act must be a non-empty"),
            "expected EmptyAct surfaced through serde, got: {err}"
        );
    }

    #[test]
    fn mandate_grant_ref_deserialize_rejects_whitespace_target_component() {
        // Same boundary discipline for the structured target's per-
        // component fields.
        let valid = sample_ref(MandateGrantRefTarget::Domain {
            domain_id: "coop-a".to_string(),
        });
        let mut value: serde_json::Value =
            serde_json::to_value(&valid).expect("serialize valid ref");
        value["target"]["domain_id"] = serde_json::Value::String("   ".to_string());
        let err = serde_json::from_value::<MandateGrantRef>(value).unwrap_err();
        assert!(
            err.to_string().contains("`domain_id`"),
            "expected EmptyTargetField(domain_id) surfaced through serde, got: {err}"
        );
    }

    #[test]
    fn mandate_grant_ref_deserialize_rejects_empty_role_holder() {
        // Covers the multi-component Role variant: every component
        // must individually pass the empty-string check at the wire
        // boundary, not only at construction.
        let valid = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: fixture_holder_did(3),
        });
        let mut value: serde_json::Value =
            serde_json::to_value(&valid).expect("serialize valid ref");
        value["target"]["holder"] = serde_json::Value::String(String::new());
        let err = serde_json::from_value::<MandateGrantRef>(value).unwrap_err();
        assert!(
            err.to_string().contains("`holder`"),
            "expected EmptyTargetField(holder) surfaced through serde, got: {err}"
        );
    }

    #[test]
    fn mandate_grant_ref_invalid_holder_did_rejected_at_construction() {
        // App-side `MandateTarget::Role.holder` is `icn_identity::Did`, so a
        // non-DID wire value would silently hash if the constructor only
        // checked emptiness. Confirm the constructor rejects garbage holders
        // and surfaces `InvalidHolderDid`.
        for bad in [
            "not-a-did",
            "icn:foo",       // missing `did:` prefix
            "did:other:foo", // wrong DID method
            "did:icn:",      // empty identifier body
            "did:icn:!!!",   // unparseable multibase
        ] {
            let err = MandateGrantRef::new(
                fixed_mandate_id(0xAA),
                [0x11; 32],
                "appoint_steward".to_string(),
                MandateGrantRefTarget::Role {
                    structure_id: "office-1".to_string(),
                    holder: bad.to_string(),
                },
                1_700_000_000,
            )
            .unwrap_err();
            match err {
                MandateGrantRefError::InvalidHolderDid(_) => {}
                other => panic!("expected InvalidHolderDid for {bad:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn mandate_grant_ref_deserialize_rejects_invalid_holder_did() {
        // Same boundary discipline through the serde path: a wire payload
        // with a parseable shape but a non-DID holder must fail closed at
        // deserialization, not silently produce a `ref_hash()`.
        let valid = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: fixture_holder_did(5),
        });
        let mut value: serde_json::Value =
            serde_json::to_value(&valid).expect("serialize valid ref");
        value["target"]["holder"] = serde_json::Value::String("not-a-did".to_string());
        let err = serde_json::from_value::<MandateGrantRef>(value).unwrap_err();
        assert!(
            err.to_string().contains("did:icn:"),
            "expected InvalidHolderDid surfaced through serde, got: {err}"
        );
    }

    #[test]
    fn mandate_grant_ref_serde_roundtrip_preserves_hash() {
        let r = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: fixture_holder_did(9),
        });
        let json = serde_json::to_string(&r).expect("serialize");
        let recovered: MandateGrantRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(r, recovered);
        assert_eq!(r.ref_hash(), recovered.ref_hash());
        // The serde tag exposes the variant discriminator on the wire.
        assert!(
            json.contains("\"kind\":\"role\""),
            "expected snake_case kind tag in JSON: {json}"
        );
    }

    // ============================================================================
    // GovernanceDecisionReceiptV2 — mandate-attestation fork (#1868 step 2)
    // ============================================================================

    /// Build a deterministic v2 receipt around the given attestation.
    /// Uses the existing `make_votes`/`make_tally` helpers so the v1 base
    /// fields are identical to those used by the v1 test suite.
    fn sample_v2_receipt(att: ReceiptMandateAttestation) -> GovernanceDecisionReceiptV2 {
        let votes = make_votes();
        let tally = make_tally(&votes);
        GovernanceDecisionReceiptV2::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
            "governance:charter:write".to_string(),
            att,
        )
        .expect("sample v2 receipt must construct cleanly")
    }

    fn sample_no_mandate_attestation() -> ReceiptMandateAttestation {
        ReceiptMandateAttestation::NoMandateRequired {
            reason: NoMandateReason::MembershipStandingOnly,
        }
    }

    fn sample_grant_attestation() -> ReceiptMandateAttestation {
        ReceiptMandateAttestation::Grant {
            grant_ref: sample_ref(MandateGrantRefTarget::Domain {
                domain_id: "coop:test".to_string(),
            }),
        }
    }

    #[test]
    fn decision_v2_hash_is_deterministic_across_calls() {
        let r1 = sample_v2_receipt(sample_no_mandate_attestation());
        let r2 = sample_v2_receipt(sample_no_mandate_attestation());
        assert_eq!(r1.decision_hash, r2.decision_hash);
        assert!(r1.verify(), "v2 receipt must verify against its own hash");
    }

    #[test]
    fn decision_v2_hash_distinct_from_v1_for_same_logical_fields() {
        // Domain-separation tags must keep v1 and v2 hash namespaces
        // disjoint. A v1 receipt and a v2 receipt over identical base
        // fields must hash to different values even when the v2
        // additions are trivially fixed.
        let votes = make_votes();
        let tally = make_tally(&votes);
        let v1 = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally.clone(),
            &votes,
        );
        let v2 = GovernanceDecisionReceiptV2::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
            "governance:charter:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(
            v1.decision_hash, v2.decision_hash,
            "v1 and v2 hashes must be domain-separated"
        );
    }

    #[test]
    fn decision_v1_and_v2_hashes_namespace_separate() {
        // Required cross-namespace test: builds v1 + v2 with identical
        // base fields, asserts the two hashes differ; then mutates one
        // base field (vote_hash via a different vote set) on both and
        // asserts each hash changes within its own namespace and the
        // two new hashes still differ. Proves the shared base encoding
        // is wired identically on both sides without collapsing the
        // namespaces.
        let votes_a = make_votes();
        let tally_a = make_tally(&votes_a);
        let v1_a = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally_a.clone(),
            &votes_a,
        );
        let v2_a = GovernanceDecisionReceiptV2::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally_a,
            &votes_a,
            "governance:charter:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(
            v1_a.decision_hash, v2_a.decision_hash,
            "v1/v2 hashes must differ for identical base fields (domain separation)"
        );

        // Mutate vote_hash on both by changing the vote set; reuse the
        // same proposal/domain/outcome/scope/attestation so only the
        // base-field input shifts.
        let votes_b: Vec<Vote> = {
            let (alice, bob, _carol) = make_deterministic_dids();
            vec![
                Vote {
                    proposal_id: crate::ProposalId::new("prop-1"),
                    voter: alice,
                    choice: VoteChoice::Against,
                    weight: 1,
                    timestamp: 1700000000,
                    comment: None,
                },
                Vote {
                    proposal_id: crate::ProposalId::new("prop-1"),
                    voter: bob,
                    choice: VoteChoice::For,
                    weight: 1,
                    timestamp: 1700000001,
                    comment: None,
                },
            ]
        };
        let tally_b = make_tally(&votes_b);
        let v1_b = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally_b.clone(),
            &votes_b,
        );
        let v2_b = GovernanceDecisionReceiptV2::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally_b,
            &votes_b,
            "governance:charter:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();

        // Each receipt's hash must change in its own namespace ...
        assert_ne!(
            v1_a.decision_hash, v1_b.decision_hash,
            "v1 hash must change when base fields change"
        );
        assert_ne!(
            v2_a.decision_hash, v2_b.decision_hash,
            "v2 hash must change when base fields change"
        );
        // ... and the namespaces must still be disjoint after the
        // mutation (no accidental collapse).
        assert_ne!(
            v1_b.decision_hash, v2_b.decision_hash,
            "v1/v2 namespaces must remain disjoint after base mutation"
        );
    }

    #[test]
    fn decision_v1_hash_remains_stable_after_v2_introduction() {
        // Explicit fixture: a v1 receipt over known inputs must hash to
        // the same bytes that v1's canonical encoding produces. Mirrors
        // the v1 hash by hand using the documented length-prefix layout
        // — any drift in the v1 encoding (including accidental drift
        // from the shared-helper refactor in this PR) makes this test
        // fail loudly with a visible byte diff.
        let votes = make_votes();
        let tally = make_tally(&votes);
        let vote_hash = GovernanceProof::compute_vote_hash(&votes);

        let mut expected_bytes = Vec::new();
        expected_bytes.extend_from_slice(GovernanceDecisionReceipt::DOMAIN_TAG);
        let proposal_id = b"prop-1";
        let domain_id = b"coop:test";
        expected_bytes.extend_from_slice(&(proposal_id.len() as u64).to_le_bytes());
        expected_bytes.extend_from_slice(proposal_id);
        expected_bytes.extend_from_slice(&(domain_id.len() as u64).to_le_bytes());
        expected_bytes.extend_from_slice(domain_id);
        expected_bytes.push(0); // outcome ordinal: Accepted = 0
        expected_bytes.extend_from_slice(&(tally.for_votes as u64).to_le_bytes());
        expected_bytes.extend_from_slice(&(tally.against_votes as u64).to_le_bytes());
        expected_bytes.extend_from_slice(&(tally.abstain_votes as u64).to_le_bytes());
        expected_bytes.extend_from_slice(&vote_hash);
        let expected: Hash = *blake3::hash(&expected_bytes).as_bytes();

        let actual = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
        )
        .decision_hash;

        assert_eq!(
            actual, expected,
            "v1 canonical encoding must remain byte-stable after v2 introduction"
        );
    }

    #[test]
    fn decision_v2_hash_binds_capability_scope_presented() {
        let votes = make_votes();
        let tally = make_tally(&votes);
        let r_charter = GovernanceDecisionReceiptV2::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally.clone(),
            &votes,
            "governance:charter:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        let r_proposal = GovernanceDecisionReceiptV2::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
            "governance:proposal:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(
            r_charter.decision_hash, r_proposal.decision_hash,
            "different capability scopes must hash differently"
        );
    }

    #[test]
    fn decision_v2_hash_binds_attestation_kind() {
        // Same base + scope, different attestation variant → different
        // hashes. Proves the variant ordinal participates.
        let no_mandate = sample_v2_receipt(sample_no_mandate_attestation());
        let with_grant = sample_v2_receipt(sample_grant_attestation());
        assert_ne!(
            no_mandate.decision_hash, with_grant.decision_hash,
            "NoMandateRequired vs Grant must produce distinct hashes"
        );
    }

    #[test]
    fn decision_v2_hash_binds_no_mandate_reason() {
        let r_membership = sample_v2_receipt(ReceiptMandateAttestation::NoMandateRequired {
            reason: NoMandateReason::MembershipStandingOnly,
        });
        let r_bootstrap = sample_v2_receipt(ReceiptMandateAttestation::NoMandateRequired {
            reason: NoMandateReason::Bootstrap,
        });
        assert_ne!(
            r_membership.decision_hash, r_bootstrap.decision_hash,
            "distinct NoMandateReason variants must hash differently"
        );
    }

    #[test]
    fn decision_v2_hash_binds_grant_ref() {
        // Mutating any field of the embedded MandateGrantRef changes
        // the receipt hash via ref_hash propagation. Exercise the
        // target identifier as a representative field.
        let r_coop_a = sample_v2_receipt(ReceiptMandateAttestation::Grant {
            grant_ref: sample_ref(MandateGrantRefTarget::Domain {
                domain_id: "coop-a".to_string(),
            }),
        });
        let r_coop_b = sample_v2_receipt(ReceiptMandateAttestation::Grant {
            grant_ref: sample_ref(MandateGrantRefTarget::Domain {
                domain_id: "coop-b".to_string(),
            }),
        });
        assert_ne!(
            r_coop_a.decision_hash, r_coop_b.decision_hash,
            "different grant_ref content must propagate into receipt hash"
        );
    }

    #[test]
    fn decision_v2_constructor_rejects_empty_capability_scope() {
        let votes = make_votes();
        let tally = make_tally(&votes);
        for bad in ["", "   ", "\t\n"] {
            let err = GovernanceDecisionReceiptV2::new(
                "prop-1".to_string(),
                "coop:test".to_string(),
                ProofOutcome::Accepted,
                tally.clone(),
                &votes,
                bad.to_string(),
                sample_no_mandate_attestation(),
            )
            .unwrap_err();
            assert_eq!(
                err,
                GovernanceDecisionReceiptV2Error::EmptyCapabilityScope,
                "input {bad:?}"
            );
        }
    }

    #[test]
    fn decision_v2_deserialize_rejects_empty_capability_scope() {
        // Wire boundary symmetric with the constructor. A payload with
        // an empty `capability_scope_presented` must fail at the serde
        // path, not silently round-trip.
        let valid = sample_v2_receipt(sample_no_mandate_attestation());
        let mut value: serde_json::Value =
            serde_json::to_value(&valid).expect("serialize valid v2 receipt");
        value["capability_scope_presented"] = serde_json::Value::String(String::new());
        let err = serde_json::from_value::<GovernanceDecisionReceiptV2>(value).unwrap_err();
        assert!(
            err.to_string().contains("capability_scope_presented"),
            "expected EmptyCapabilityScope surfaced through serde, got: {err}"
        );
    }

    #[test]
    fn decision_v2_serde_roundtrip_preserves_hash() {
        for att in [sample_no_mandate_attestation(), sample_grant_attestation()] {
            let original = sample_v2_receipt(att.clone());
            let json = serde_json::to_string(&original).expect("serialize");
            let recovered: GovernanceDecisionReceiptV2 =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(
                original.decision_hash, recovered.decision_hash,
                "round-trip must preserve decision_hash for attestation {att:?}"
            );
            assert!(
                recovered.verify(),
                "round-tripped v2 receipt must verify; attestation {att:?}"
            );
        }
    }

    #[test]
    fn decision_v2_attestation_serde_uses_snake_case_tags() {
        let no_mandate = sample_v2_receipt(sample_no_mandate_attestation());
        let json = serde_json::to_string(&no_mandate).expect("serialize");
        assert!(
            json.contains("\"kind\":\"no_mandate_required\""),
            "expected snake_case attestation tag in JSON: {json}"
        );
        assert!(
            json.contains("\"reason\":\"membership_standing_only\""),
            "expected snake_case reason in JSON: {json}"
        );

        let bootstrap = sample_v2_receipt(ReceiptMandateAttestation::NoMandateRequired {
            reason: NoMandateReason::Bootstrap,
        });
        let json = serde_json::to_string(&bootstrap).expect("serialize");
        assert!(
            json.contains("\"reason\":\"bootstrap\""),
            "expected bootstrap reason in JSON: {json}"
        );

        let grant = sample_v2_receipt(sample_grant_attestation());
        let json = serde_json::to_string(&grant).expect("serialize");
        assert!(
            json.contains("\"kind\":\"grant\""),
            "expected grant tag in JSON: {json}"
        );
    }

    #[test]
    fn decision_v2_no_regulated_finance_vocabulary() {
        // Mirror the receipt-family vocabulary discipline: the v2
        // decision receipt is an institutional-authority record, not
        // an economic one. Its serialized form must not echo
        // regulated-finance terms.
        for att in [sample_no_mandate_attestation(), sample_grant_attestation()] {
            let r = sample_v2_receipt(att);
            let json = serde_json::to_string(&r).expect("serialize");
            let lower = json.to_lowercase();
            for forbidden in [
                "wallet", "balance", "currency", "payment", "token", "withdraw", "deposit",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "GovernanceDecisionReceiptV2 JSON must not contain \
                     regulated-finance vocabulary; found `{forbidden}` in: {json}"
                );
            }
        }
    }

    // ============================================================================
    // ActionItemCompletionReceiptV2 — mandate-attestation fork (#1868 step 2)
    // ============================================================================

    /// Deterministic v2 action-item receipt around the given attestation.
    /// Uses fixed values for every base field so per-test mutation is
    /// localized to the field under assertion.
    fn sample_action_item_v2_receipt(
        att: ReceiptMandateAttestation,
    ) -> ActionItemCompletionReceiptV2 {
        ActionItemCompletionReceiptV2::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            att,
        )
        .expect("sample v2 action-item receipt must construct cleanly")
    }

    #[test]
    fn action_item_v1_record_hash_remains_stable_after_v2_introduction() {
        // Explicit byte-stream fixture: a v1 receipt over known inputs
        // must hash to the same bytes the v1 canonical encoding
        // produces. Mirrors v1's length-prefix layout by hand so any
        // drift in v1 encoding (including from the shared-helper
        // refactor in this PR) makes this test fail loudly with a
        // visible byte diff.
        let mut expected_bytes = Vec::new();
        expected_bytes.extend_from_slice(ActionItemCompletionReceipt::DOMAIN_TAG);
        for field in [b"item-1".as_slice(), b"coop:test", b"did:icn:actor"] {
            expected_bytes.extend_from_slice(&(field.len() as u64).to_le_bytes());
            expected_bytes.extend_from_slice(field);
        }
        expected_bytes.push(0); // transition ordinal: Completed = 0
        expected_bytes.extend_from_slice(&1_700_000_000u64.to_le_bytes());
        let expected: Hash = *blake3::hash(&expected_bytes).as_bytes();

        let actual = ActionItemCompletionReceipt::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_000,
        )
        .record_hash;

        assert_eq!(
            actual, expected,
            "v1 action-item canonical encoding must remain byte-stable after v2 introduction"
        );
    }

    #[test]
    fn action_item_v2_hash_is_deterministic_across_calls() {
        let r1 = sample_action_item_v2_receipt(sample_no_mandate_attestation());
        let r2 = sample_action_item_v2_receipt(sample_no_mandate_attestation());
        assert_eq!(r1.record_hash, r2.record_hash);
        assert!(
            r1.verify(),
            "v2 action-item receipt must verify its own hash"
        );
    }

    #[test]
    fn action_item_v2_hash_distinct_from_v1_for_same_logical_fields() {
        // Domain-separation tags must keep v1 and v2 hash namespaces
        // disjoint. A v1 receipt and a v2 receipt over identical base
        // fields must hash to different values even when the v2
        // additions are trivially fixed.
        let v1 = ActionItemCompletionReceipt::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_000,
        );
        let v2 = ActionItemCompletionReceiptV2::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(
            v1.record_hash, v2.record_hash,
            "v1 and v2 action-item hashes must be domain-separated"
        );
    }

    #[test]
    fn action_item_v1_and_v2_hashes_namespace_separate() {
        // Required cross-namespace test (mirror of the decision-receipt
        // analogue). Builds v1 + v2 with identical base fields, asserts
        // hashes differ; mutates one base field (`completed_at`) on
        // both and asserts each hash changes within its own namespace
        // while the two new hashes still differ.
        let v1_a = ActionItemCompletionReceipt::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_000,
        );
        let v2_a = ActionItemCompletionReceiptV2::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(
            v1_a.record_hash, v2_a.record_hash,
            "v1/v2 hashes must differ for identical base fields (domain separation)"
        );

        // Mutate completed_at on both; everything else stays identical.
        let v1_b = ActionItemCompletionReceipt::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_001,
        );
        let v2_b = ActionItemCompletionReceiptV2::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_001,
            "governance:meeting:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();

        assert_ne!(
            v1_a.record_hash, v1_b.record_hash,
            "v1 hash must change when base fields change"
        );
        assert_ne!(
            v2_a.record_hash, v2_b.record_hash,
            "v2 hash must change when base fields change"
        );
        assert_ne!(
            v1_b.record_hash, v2_b.record_hash,
            "v1/v2 namespaces must remain disjoint after base mutation"
        );
    }

    #[test]
    fn action_item_v2_hash_binds_capability_scope_presented() {
        let r_meeting = ActionItemCompletionReceiptV2::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        let r_activity = ActionItemCompletionReceiptV2::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_000,
            "governance:activity:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(
            r_meeting.record_hash, r_activity.record_hash,
            "different capability scopes must hash differently"
        );
    }

    #[test]
    fn action_item_v2_hash_binds_attestation_kind() {
        let no_mandate = sample_action_item_v2_receipt(sample_no_mandate_attestation());
        let with_grant = sample_action_item_v2_receipt(sample_grant_attestation());
        assert_ne!(
            no_mandate.record_hash, with_grant.record_hash,
            "NoMandateRequired vs Grant must produce distinct hashes"
        );
    }

    #[test]
    fn action_item_v2_hash_binds_no_mandate_reason() {
        let r_membership =
            sample_action_item_v2_receipt(ReceiptMandateAttestation::NoMandateRequired {
                reason: NoMandateReason::MembershipStandingOnly,
            });
        let r_bootstrap =
            sample_action_item_v2_receipt(ReceiptMandateAttestation::NoMandateRequired {
                reason: NoMandateReason::Bootstrap,
            });
        assert_ne!(
            r_membership.record_hash, r_bootstrap.record_hash,
            "distinct NoMandateReason variants must hash differently"
        );
    }

    #[test]
    fn action_item_v2_hash_binds_grant_ref() {
        // Mutating any field of the embedded MandateGrantRef changes the
        // receipt hash via ref_hash propagation. Exercise the target
        // identifier as a representative field.
        let r_coop_a = sample_action_item_v2_receipt(ReceiptMandateAttestation::Grant {
            grant_ref: sample_ref(MandateGrantRefTarget::Domain {
                domain_id: "coop-a".to_string(),
            }),
        });
        let r_coop_b = sample_action_item_v2_receipt(ReceiptMandateAttestation::Grant {
            grant_ref: sample_ref(MandateGrantRefTarget::Domain {
                domain_id: "coop-b".to_string(),
            }),
        });
        assert_ne!(
            r_coop_a.record_hash, r_coop_b.record_hash,
            "different grant_ref content must propagate into receipt hash"
        );
    }

    #[test]
    fn action_item_v2_constructor_rejects_empty_capability_scope() {
        for bad in ["", "   ", "\t\n"] {
            let err = ActionItemCompletionReceiptV2::new(
                "item-1".to_string(),
                "coop:test".to_string(),
                "did:icn:actor".to_string(),
                ActionItemTransition::Completed,
                1_700_000_000,
                bad.to_string(),
                sample_no_mandate_attestation(),
            )
            .unwrap_err();
            assert_eq!(
                err,
                ActionItemCompletionReceiptV2Error::EmptyCapabilityScope,
                "input {bad:?}"
            );
        }
    }

    #[test]
    fn action_item_v2_deserialize_rejects_empty_capability_scope() {
        // Wire boundary symmetric with the constructor. A payload with
        // an empty `capability_scope_presented` must fail at the serde
        // path, not silently round-trip.
        let valid = sample_action_item_v2_receipt(sample_no_mandate_attestation());
        let mut value: serde_json::Value =
            serde_json::to_value(&valid).expect("serialize valid v2 action-item receipt");
        value["capability_scope_presented"] = serde_json::Value::String(String::new());
        let err = serde_json::from_value::<ActionItemCompletionReceiptV2>(value).unwrap_err();
        assert!(
            err.to_string().contains("capability_scope_presented"),
            "expected EmptyCapabilityScope surfaced through serde, got: {err}"
        );
    }

    #[test]
    fn action_item_v2_serde_roundtrip_preserves_hash() {
        for att in [sample_no_mandate_attestation(), sample_grant_attestation()] {
            let original = sample_action_item_v2_receipt(att.clone());
            let json = serde_json::to_string(&original).expect("serialize");
            let recovered: ActionItemCompletionReceiptV2 =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(
                original.record_hash, recovered.record_hash,
                "round-trip must preserve record_hash for attestation {att:?}"
            );
            assert!(
                recovered.verify(),
                "round-tripped v2 action-item receipt must verify; attestation {att:?}"
            );
        }
    }

    #[test]
    fn action_item_v2_attestation_serde_uses_snake_case_tags() {
        let no_mandate = sample_action_item_v2_receipt(sample_no_mandate_attestation());
        let json = serde_json::to_string(&no_mandate).expect("serialize");
        assert!(
            json.contains("\"kind\":\"no_mandate_required\""),
            "expected snake_case attestation tag in JSON: {json}"
        );
        assert!(
            json.contains("\"reason\":\"membership_standing_only\""),
            "expected snake_case reason in JSON: {json}"
        );
        // transition snake_case sanity check (carries over from v1's
        // serde rename_all = "snake_case" on ActionItemTransition).
        assert!(
            json.contains("\"transition\":\"completed\""),
            "expected snake_case transition in JSON: {json}"
        );

        let grant = sample_action_item_v2_receipt(sample_grant_attestation());
        let json = serde_json::to_string(&grant).expect("serialize");
        assert!(
            json.contains("\"kind\":\"grant\""),
            "expected grant tag in JSON: {json}"
        );
    }

    #[test]
    fn action_item_v2_no_regulated_finance_vocabulary() {
        // Mirror the receipt-family vocabulary discipline: the v2
        // action-item receipt is an institutional-authority record, not
        // an economic one. Its serialized form must not echo
        // regulated-finance terms.
        for att in [sample_no_mandate_attestation(), sample_grant_attestation()] {
            let r = sample_action_item_v2_receipt(att);
            let json = serde_json::to_string(&r).expect("serialize");
            let lower = json.to_lowercase();
            for forbidden in [
                "wallet", "balance", "currency", "payment", "token", "withdraw", "deposit",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "ActionItemCompletionReceiptV2 JSON must not contain \
                     regulated-finance vocabulary; found `{forbidden}` in: {json}"
                );
            }
        }
    }

    #[test]
    fn mandate_grant_ref_no_regulated_finance_vocabulary() {
        // Mirror the receipt family's vocabulary-discipline test:
        // MandateGrantRef is an institutional-authority record, not an
        // economic one. Its serialized form must not echo regulated-
        // finance terms.
        let r = sample_ref(MandateGrantRefTarget::Domain {
            domain_id: "coop-a".to_string(),
        });
        let json = serde_json::to_string(&r).expect("serialize");
        let lower = json.to_lowercase();
        for forbidden in [
            "wallet", "balance", "currency", "payment", "token", "withdraw", "deposit",
        ] {
            assert!(
                !lower.contains(forbidden),
                "MandateGrantRef JSON must not contain regulated-finance vocabulary; \
                 found `{forbidden}` in: {json}"
            );
        }
    }

    // ============================================================================
    // MeetingAttendanceReceiptV2 — mandate-attestation fork (#1868)
    // ============================================================================

    #[test]
    fn meeting_attendance_v1_record_hash_remains_stable_after_v2_introduction() {
        // Explicit byte-stream fixture: a v1 receipt over known inputs
        // must hash to the same bytes the v1 canonical encoding
        // produces. Mirrors v1's length-prefix layout by hand so any
        // drift in v1 encoding (including from the shared-helper
        // refactor in this PR) makes this test fail loudly with a
        // visible byte diff.
        let mut expected_bytes = Vec::new();
        expected_bytes.extend_from_slice(MeetingAttendanceReceipt::DOMAIN_TAG);
        for field in [
            b"meeting-1".as_slice(),
            b"coop:test",
            b"did:icn:attendee",
            b"did:icn:steward",
        ] {
            expected_bytes.extend_from_slice(&(field.len() as u64).to_le_bytes());
            expected_bytes.extend_from_slice(field);
        }
        expected_bytes.push(0); // transition ordinal: Present = 0
        expected_bytes.extend_from_slice(&1_700_000_000u64.to_le_bytes());
        let expected: Hash = *blake3::hash(&expected_bytes).as_bytes();

        let actual = MeetingAttendanceReceipt::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_000,
        )
        .record_hash;

        assert_eq!(
            actual, expected,
            "v1 meeting-attendance canonical encoding must remain byte-stable after v2 introduction"
        );
    }

    /// Deterministic v2 meeting-attendance receipt around the given
    /// attestation. Uses fixed values for every base field so per-test
    /// mutation is localized to the field under assertion.
    fn sample_meeting_attendance_v2_receipt(
        att: ReceiptMandateAttestation,
    ) -> MeetingAttendanceReceiptV2 {
        MeetingAttendanceReceiptV2::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            att,
        )
        .expect("sample v2 meeting-attendance receipt must construct cleanly")
    }

    #[test]
    fn meeting_attendance_v2_hash_is_deterministic_across_calls() {
        let r1 = sample_meeting_attendance_v2_receipt(sample_no_mandate_attestation());
        let r2 = sample_meeting_attendance_v2_receipt(sample_no_mandate_attestation());
        assert_eq!(r1.record_hash, r2.record_hash);
        assert!(
            r1.verify(),
            "v2 meeting-attendance receipt must verify its own hash"
        );
    }

    #[test]
    fn meeting_attendance_v2_hash_distinct_from_v1_for_same_logical_fields() {
        // Domain-separation tags must keep v1 and v2 hash namespaces
        // disjoint. A v1 receipt and a v2 receipt over identical base
        // fields must hash to different values even when the v2 additions
        // are trivially fixed.
        let v1 = MeetingAttendanceReceipt::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_000,
        );
        let v2 = MeetingAttendanceReceiptV2::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(
            v1.record_hash, v2.record_hash,
            "v1 and v2 meeting-attendance hashes must be domain-separated"
        );
    }

    #[test]
    fn meeting_attendance_v1_and_v2_hashes_namespace_separate() {
        // Builds v1 + v2 with identical base fields, asserts hashes
        // differ; mutates one base field (`recorded_at`) on both and
        // asserts each hash changes within its own namespace while the two
        // new hashes still differ.
        let v1_a = MeetingAttendanceReceipt::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_000,
        );
        let v2_a = MeetingAttendanceReceiptV2::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(
            v1_a.record_hash, v2_a.record_hash,
            "v1/v2 hashes must differ for identical base fields (domain separation)"
        );

        // Mutate recorded_at on both; everything else stays identical.
        let v1_b = MeetingAttendanceReceipt::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_001,
        );
        let v2_b = MeetingAttendanceReceiptV2::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_001,
            "governance:meeting:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();

        assert_ne!(
            v1_a.record_hash, v1_b.record_hash,
            "v1 hash must change when base fields change"
        );
        assert_ne!(
            v2_a.record_hash, v2_b.record_hash,
            "v2 hash must change when base fields change"
        );
        assert_ne!(
            v1_b.record_hash, v2_b.record_hash,
            "v1/v2 namespaces must remain disjoint after base mutation"
        );
    }

    #[test]
    fn meeting_attendance_v2_hash_binds_capability_scope_presented() {
        let r_meeting = MeetingAttendanceReceiptV2::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        let r_activity = MeetingAttendanceReceiptV2::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_000,
            "governance:activity:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(
            r_meeting.record_hash, r_activity.record_hash,
            "different capability scopes must hash differently"
        );
    }

    #[test]
    fn meeting_attendance_v2_hash_binds_attestation_kind() {
        let no_mandate = sample_meeting_attendance_v2_receipt(sample_no_mandate_attestation());
        let with_grant = sample_meeting_attendance_v2_receipt(sample_grant_attestation());
        assert_ne!(
            no_mandate.record_hash, with_grant.record_hash,
            "NoMandateRequired vs Grant must produce distinct hashes"
        );
    }

    #[test]
    fn meeting_attendance_v2_hash_binds_no_mandate_reason() {
        let r_membership =
            sample_meeting_attendance_v2_receipt(ReceiptMandateAttestation::NoMandateRequired {
                reason: NoMandateReason::MembershipStandingOnly,
            });
        let r_bootstrap =
            sample_meeting_attendance_v2_receipt(ReceiptMandateAttestation::NoMandateRequired {
                reason: NoMandateReason::Bootstrap,
            });
        assert_ne!(
            r_membership.record_hash, r_bootstrap.record_hash,
            "distinct NoMandateReason variants must hash differently"
        );
    }

    #[test]
    fn meeting_attendance_v2_hash_binds_grant_ref() {
        // Mutating any field of the embedded MandateGrantRef changes the
        // receipt hash via ref_hash propagation. Exercise the target
        // identifier as a representative field.
        let r_coop_a = sample_meeting_attendance_v2_receipt(ReceiptMandateAttestation::Grant {
            grant_ref: sample_ref(MandateGrantRefTarget::Domain {
                domain_id: "coop-a".to_string(),
            }),
        });
        let r_coop_b = sample_meeting_attendance_v2_receipt(ReceiptMandateAttestation::Grant {
            grant_ref: sample_ref(MandateGrantRefTarget::Domain {
                domain_id: "coop-b".to_string(),
            }),
        });
        assert_ne!(
            r_coop_a.record_hash, r_coop_b.record_hash,
            "different grant_ref content must propagate into receipt hash"
        );
    }

    #[test]
    fn meeting_attendance_v2_constructor_rejects_empty_capability_scope() {
        for bad in ["", "   ", "\t\n"] {
            let err = MeetingAttendanceReceiptV2::new(
                "meeting-1".to_string(),
                "coop:test".to_string(),
                "did:icn:attendee".to_string(),
                "did:icn:steward".to_string(),
                MeetingAttendanceTransition::Present,
                1_700_000_000,
                bad.to_string(),
                sample_no_mandate_attestation(),
            )
            .unwrap_err();
            assert_eq!(
                err,
                MeetingAttendanceReceiptV2Error::EmptyCapabilityScope,
                "input {bad:?}"
            );
        }
    }

    #[test]
    fn meeting_attendance_v2_deserialize_rejects_empty_capability_scope() {
        // Wire boundary symmetric with the constructor. A payload with an
        // empty `capability_scope_presented` must fail at the serde path,
        // not silently round-trip.
        let valid = sample_meeting_attendance_v2_receipt(sample_no_mandate_attestation());
        let mut value: serde_json::Value =
            serde_json::to_value(&valid).expect("serialize valid v2 meeting-attendance receipt");
        value["capability_scope_presented"] = serde_json::Value::String(String::new());
        let err = serde_json::from_value::<MeetingAttendanceReceiptV2>(value).unwrap_err();
        assert!(
            err.to_string().contains("capability_scope_presented"),
            "expected EmptyCapabilityScope surfaced through serde, got: {err}"
        );
    }

    #[test]
    fn meeting_attendance_v2_serde_roundtrip_preserves_hash() {
        for att in [sample_no_mandate_attestation(), sample_grant_attestation()] {
            let original = sample_meeting_attendance_v2_receipt(att.clone());
            let json = serde_json::to_string(&original).expect("serialize");
            let recovered: MeetingAttendanceReceiptV2 =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(
                original.record_hash, recovered.record_hash,
                "round-trip must preserve record_hash for attestation {att:?}"
            );
            assert!(
                recovered.verify(),
                "round-tripped v2 meeting-attendance receipt must verify; attestation {att:?}"
            );
        }
    }

    #[test]
    fn meeting_attendance_v2_attestation_serde_uses_snake_case_tags() {
        let no_mandate = sample_meeting_attendance_v2_receipt(sample_no_mandate_attestation());
        let json = serde_json::to_string(&no_mandate).expect("serialize");
        assert!(
            json.contains("\"kind\":\"no_mandate_required\""),
            "expected snake_case attestation tag in JSON: {json}"
        );
        assert!(
            json.contains("\"reason\":\"membership_standing_only\""),
            "expected snake_case reason in JSON: {json}"
        );
        // transition snake_case sanity check (carries over from v1's serde
        // rename_all = "snake_case" on MeetingAttendanceTransition).
        assert!(
            json.contains("\"transition\":\"present\""),
            "expected snake_case transition in JSON: {json}"
        );

        let grant = sample_meeting_attendance_v2_receipt(sample_grant_attestation());
        let json = serde_json::to_string(&grant).expect("serialize");
        assert!(
            json.contains("\"kind\":\"grant\""),
            "expected grant tag in JSON: {json}"
        );
    }

    #[test]
    fn meeting_attendance_v2_no_regulated_finance_vocabulary() {
        // Mirror the receipt-family vocabulary discipline: the v2
        // meeting-attendance receipt is an institutional-authority record,
        // not an economic one. Its serialized form must not echo
        // regulated-finance terms.
        for att in [sample_no_mandate_attestation(), sample_grant_attestation()] {
            let r = sample_meeting_attendance_v2_receipt(att);
            let json = serde_json::to_string(&r).expect("serialize");
            let lower = json.to_lowercase();
            for forbidden in [
                "wallet", "balance", "currency", "payment", "token", "withdraw", "deposit",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "MeetingAttendanceReceiptV2 JSON must not contain \
                     regulated-finance vocabulary; found `{forbidden}` in: {json}"
                );
            }
        }
    }

    // ========================================================================
    // GovernanceDecisionReceiptV3 — process-authorized fork (#1868)
    // ========================================================================

    fn process_authorized() -> ReceiptMandateAttestation {
        ReceiptMandateAttestation::ProcessAuthorized
    }

    fn sample_v3_receipt(att: ReceiptMandateAttestation) -> GovernanceDecisionReceiptV3 {
        let votes = make_votes();
        let tally = make_tally(&votes);
        GovernanceDecisionReceiptV3::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
            "governance:charter:write".to_string(),
            att,
        )
        .expect("sample v3 receipt must construct cleanly")
    }

    #[test]
    fn decision_v3_hash_is_deterministic_and_verifies() {
        let r1 = sample_v3_receipt(process_authorized());
        let r2 = sample_v3_receipt(process_authorized());
        assert_eq!(r1.decision_hash, r2.decision_hash);
        assert!(r1.verify(), "v3 receipt must verify against its own hash");
    }

    #[test]
    fn decision_v1_v2_v3_hashes_namespace_separate() {
        // Identical base fields (and, for v2/v3, identical scope +
        // NoMandateRequired attestation) must still hash differently across
        // the three versions — proving domain-tag separation, not collapse.
        let votes = make_votes();
        let tally = make_tally(&votes);
        let v1 = GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally.clone(),
            &votes,
        );
        let v2 = GovernanceDecisionReceiptV2::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally.clone(),
            &votes,
            "governance:charter:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        let v3 = GovernanceDecisionReceiptV3::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
            "governance:charter:write".to_string(),
            sample_no_mandate_attestation(),
        )
        .unwrap();
        assert_ne!(v1.decision_hash, v2.decision_hash, "v1 vs v2");
        assert_ne!(v1.decision_hash, v3.decision_hash, "v1 vs v3");
        assert_ne!(v2.decision_hash, v3.decision_hash, "v2 vs v3");
    }

    #[test]
    fn decision_v2_hash_byte_stable_after_v3_introduction() {
        // Golden byte-stream fixture pinning the v2 hash for a fixed input,
        // independent of production helpers. Adding v3 (and the defensive
        // ProcessAuthorized arm) must not shift any existing v2 hash. v1's
        // analogous fixture is `decision_v1_hash_remains_stable_after_v2_introduction`.
        let votes = make_votes();
        let tally = make_tally(&votes);
        let vote_hash = GovernanceProof::compute_vote_hash(&votes);

        let mut expected = Vec::new();
        expected.extend_from_slice(b"icn:gov:decision:v2");
        expected.extend_from_slice(&(b"prop-1".len() as u64).to_le_bytes());
        expected.extend_from_slice(b"prop-1");
        expected.extend_from_slice(&(b"coop:test".len() as u64).to_le_bytes());
        expected.extend_from_slice(b"coop:test");
        expected.push(0); // outcome ordinal: Accepted = 0
        expected.extend_from_slice(&(tally.for_votes as u64).to_le_bytes());
        expected.extend_from_slice(&(tally.against_votes as u64).to_le_bytes());
        expected.extend_from_slice(&(tally.abstain_votes as u64).to_le_bytes());
        expected.extend_from_slice(&vote_hash);
        expected.extend_from_slice(&(b"governance:charter:write".len() as u64).to_le_bytes());
        expected.extend_from_slice(b"governance:charter:write");
        expected.push(0); // attestation kind ordinal: NoMandateRequired = 0
        expected.push(0); // no-mandate reason ordinal: MembershipStandingOnly = 0
        let expected_hash: Hash = *blake3::hash(&expected).as_bytes();

        let actual = GovernanceDecisionReceiptV2::compute_decision_hash(
            "prop-1",
            "coop:test",
            ProofOutcome::Accepted,
            &tally,
            &vote_hash,
            "governance:charter:write",
            &sample_no_mandate_attestation(),
        );
        assert_eq!(
            actual, expected_hash,
            "v2 decision hash must remain byte-stable after v3 introduction"
        );
    }

    #[test]
    fn decision_v3_hash_changes_on_base_field_mutation() {
        let base = sample_v3_receipt(process_authorized());
        // Mutate one base field (outcome) only.
        let votes = make_votes();
        let tally = make_tally(&votes);
        let mutated = GovernanceDecisionReceiptV3::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Rejected,
            tally,
            &votes,
            "governance:charter:write".to_string(),
            process_authorized(),
        )
        .unwrap();
        assert_ne!(
            base.decision_hash, mutated.decision_hash,
            "changing a base field (outcome) must change the v3 hash"
        );
    }

    #[test]
    fn decision_v3_hash_binds_capability_scope_presented() {
        let votes = make_votes();
        let tally = make_tally(&votes);
        let mk = |scope: &str| {
            GovernanceDecisionReceiptV3::new(
                "prop-1".to_string(),
                "coop:test".to_string(),
                ProofOutcome::Accepted,
                tally.clone(),
                &votes,
                scope.to_string(),
                process_authorized(),
            )
            .unwrap()
        };
        assert_ne!(
            mk("governance:charter:write").decision_hash,
            mk("governance:proposal:write").decision_hash,
            "different capability scopes must hash differently in v3"
        );
    }

    #[test]
    fn decision_v3_hash_binds_process_authorized() {
        // The three attestation kinds must produce distinct v3 hashes over
        // otherwise-identical fields.
        let pa = sample_v3_receipt(process_authorized());
        let nm = sample_v3_receipt(sample_no_mandate_attestation());
        let gr = sample_v3_receipt(sample_grant_attestation());
        assert_ne!(
            pa.decision_hash, nm.decision_hash,
            "ProcessAuthorized vs NoMandateRequired"
        );
        assert_ne!(
            pa.decision_hash, gr.decision_hash,
            "ProcessAuthorized vs Grant"
        );
        assert_ne!(
            nm.decision_hash, gr.decision_hash,
            "NoMandateRequired vs Grant"
        );
    }

    #[test]
    fn decision_v3_serde_roundtrip_preserves_hash() {
        for att in [
            process_authorized(),
            sample_no_mandate_attestation(),
            sample_grant_attestation(),
        ] {
            let original = sample_v3_receipt(att.clone());
            let json = serde_json::to_string(&original).expect("serialize");
            let recovered: GovernanceDecisionReceiptV3 =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(
                original.decision_hash, recovered.decision_hash,
                "round-trip must preserve decision_hash for {att:?}"
            );
            assert!(
                recovered.verify(),
                "round-tripped v3 receipt must verify; {att:?}"
            );
        }
    }

    #[test]
    fn decision_v3_serde_uses_snake_case_tags() {
        let json =
            serde_json::to_string(&sample_v3_receipt(process_authorized())).expect("serialize");
        assert!(
            json.contains("\"kind\":\"process_authorized\""),
            "expected snake_case ProcessAuthorized tag in JSON: {json}"
        );
    }

    #[test]
    fn decision_v3_constructor_rejects_empty_capability_scope() {
        let votes = make_votes();
        let tally = make_tally(&votes);
        for bad in ["", "   ", "\t\n"] {
            let err = GovernanceDecisionReceiptV3::new(
                "prop-1".to_string(),
                "coop:test".to_string(),
                ProofOutcome::Accepted,
                tally.clone(),
                &votes,
                bad.to_string(),
                process_authorized(),
            )
            .unwrap_err();
            assert_eq!(
                err,
                GovernanceDecisionReceiptV3Error::EmptyCapabilityScope,
                "input {bad:?}"
            );
        }
    }

    #[test]
    fn decision_v3_deserialize_rejects_empty_capability_scope() {
        let valid = sample_v3_receipt(process_authorized());
        let mut value = serde_json::to_value(&valid).expect("serialize");
        value["capability_scope_presented"] = serde_json::Value::String(String::new());
        let err = serde_json::from_value::<GovernanceDecisionReceiptV3>(value).unwrap_err();
        assert!(
            err.to_string().contains("capability_scope_presented"),
            "expected EmptyCapabilityScope through serde, got: {err}"
        );
    }

    #[test]
    fn decision_v3_no_regulated_finance_vocabulary() {
        for att in [
            process_authorized(),
            sample_no_mandate_attestation(),
            sample_grant_attestation(),
        ] {
            let json = serde_json::to_string(&sample_v3_receipt(att)).expect("serialize");
            let lower = json.to_lowercase();
            for forbidden in [
                "wallet", "balance", "currency", "payment", "token", "withdraw", "deposit",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "GovernanceDecisionReceiptV3 JSON must not contain regulated-finance \
                     vocabulary; found `{forbidden}` in: {json}"
                );
            }
        }
    }

    // ---- v2 receipts must reject the v3-only ProcessAuthorized mode ----

    #[test]
    fn decision_v2_rejects_process_authorized_at_new() {
        let votes = make_votes();
        let tally = make_tally(&votes);
        let err = GovernanceDecisionReceiptV2::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
            "governance:charter:write".to_string(),
            ReceiptMandateAttestation::ProcessAuthorized,
        )
        .unwrap_err();
        assert_eq!(
            err,
            GovernanceDecisionReceiptV2Error::UnsupportedAttestation
        );
    }

    #[test]
    fn decision_v2_rejects_process_authorized_through_serde() {
        let mut value =
            serde_json::to_value(sample_v2_receipt(sample_no_mandate_attestation())).unwrap();
        value["mandate_attestation"] = serde_json::json!({ "kind": "process_authorized" });
        let err = serde_json::from_value::<GovernanceDecisionReceiptV2>(value).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("processauthorized")
                || err.to_string().contains("v3"),
            "v2 deserialize must reject ProcessAuthorized, got: {err}"
        );
    }

    #[test]
    fn action_item_v2_rejects_process_authorized_at_new() {
        let err = ActionItemCompletionReceiptV2::new(
            "item-1".to_string(),
            "coop:test".to_string(),
            "did:icn:actor".to_string(),
            ActionItemTransition::Completed,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            ReceiptMandateAttestation::ProcessAuthorized,
        )
        .unwrap_err();
        assert_eq!(
            err,
            ActionItemCompletionReceiptV2Error::UnsupportedAttestation
        );
    }

    #[test]
    fn action_item_v2_rejects_process_authorized_through_serde() {
        let mut value = serde_json::to_value(sample_action_item_v2_receipt(
            sample_no_mandate_attestation(),
        ))
        .unwrap();
        value["mandate_attestation"] = serde_json::json!({ "kind": "process_authorized" });
        let err = serde_json::from_value::<ActionItemCompletionReceiptV2>(value).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("processauthorized")
                || err.to_string().contains("v3"),
            "action-item v2 deserialize must reject ProcessAuthorized, got: {err}"
        );
    }

    #[test]
    fn meeting_attendance_v2_rejects_process_authorized_at_new() {
        let err = MeetingAttendanceReceiptV2::new(
            "meeting-1".to_string(),
            "coop:test".to_string(),
            "did:icn:attendee".to_string(),
            "did:icn:steward".to_string(),
            MeetingAttendanceTransition::Present,
            1_700_000_000,
            "governance:meeting:write".to_string(),
            ReceiptMandateAttestation::ProcessAuthorized,
        )
        .unwrap_err();
        assert_eq!(err, MeetingAttendanceReceiptV2Error::UnsupportedAttestation);
    }

    #[test]
    fn meeting_attendance_v2_rejects_process_authorized_through_serde() {
        let mut value = serde_json::to_value(sample_meeting_attendance_v2_receipt(
            sample_no_mandate_attestation(),
        ))
        .unwrap();
        value["mandate_attestation"] = serde_json::json!({ "kind": "process_authorized" });
        let err = serde_json::from_value::<MeetingAttendanceReceiptV2>(value).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("processauthorized")
                || err.to_string().contains("v3"),
            "meeting-attendance v2 deserialize must reject ProcessAuthorized, got: {err}"
        );
    }

    // ---- v2 `verify()` must also fail-closed on ProcessAuthorized, since the
    //      `pub` struct fields allow a direct struct literal to bypass
    //      `new()`/`try_from` (addresses the verify-path gap). ----

    #[test]
    fn decision_v2_verify_rejects_struct_literal_process_authorized() {
        let votes = make_votes();
        let tally = make_tally(&votes);
        let vote_hash = GovernanceProof::compute_vote_hash(&votes);
        let att = ReceiptMandateAttestation::ProcessAuthorized;
        // Build the "correct" defensive hash, then plant it in a struct literal.
        let decision_hash = GovernanceDecisionReceiptV2::compute_decision_hash(
            "prop-1",
            "coop:test",
            ProofOutcome::Accepted,
            &tally,
            &vote_hash,
            "governance:charter:write",
            &att,
        );
        let r = GovernanceDecisionReceiptV2 {
            proposal_id: "prop-1".to_string(),
            domain_id: "coop:test".to_string(),
            outcome: ProofOutcome::Accepted,
            vote_tally: tally,
            vote_hash,
            capability_scope_presented: "governance:charter:write".to_string(),
            mandate_attestation: att,
            decision_hash,
        };
        assert!(
            !r.verify(),
            "v2 decision receipt must not verify a ProcessAuthorized attestation, even when \
             struct-constructed with a matching hash"
        );
    }

    #[test]
    fn action_item_v2_verify_rejects_struct_literal_process_authorized() {
        let att = ReceiptMandateAttestation::ProcessAuthorized;
        let record_hash = ActionItemCompletionReceiptV2::compute_record_hash(
            "item-1",
            "coop:test",
            "did:icn:actor",
            ActionItemTransition::Completed,
            1_700_000_000,
            "governance:meeting:write",
            &att,
        );
        let r = ActionItemCompletionReceiptV2 {
            item_id: "item-1".to_string(),
            domain_id: "coop:test".to_string(),
            actor_did: "did:icn:actor".to_string(),
            transition: ActionItemTransition::Completed,
            completed_at: 1_700_000_000,
            capability_scope_presented: "governance:meeting:write".to_string(),
            mandate_attestation: att,
            record_hash,
        };
        assert!(
            !r.verify(),
            "action-item v2 receipt must not verify a ProcessAuthorized attestation"
        );
    }

    #[test]
    fn meeting_attendance_v2_verify_rejects_struct_literal_process_authorized() {
        let att = ReceiptMandateAttestation::ProcessAuthorized;
        let record_hash = MeetingAttendanceReceiptV2::compute_record_hash(
            "meeting-1",
            "coop:test",
            "did:icn:attendee",
            "did:icn:steward",
            MeetingAttendanceTransition::Present,
            1_700_000_000,
            "governance:meeting:write",
            &att,
        );
        let r = MeetingAttendanceReceiptV2 {
            meeting_id: "meeting-1".to_string(),
            domain_id: "coop:test".to_string(),
            attendee_did: "did:icn:attendee".to_string(),
            recorded_by: "did:icn:steward".to_string(),
            transition: MeetingAttendanceTransition::Present,
            recorded_at: 1_700_000_000,
            capability_scope_presented: "governance:meeting:write".to_string(),
            mandate_attestation: att,
            record_hash,
        };
        assert!(
            !r.verify(),
            "meeting-attendance v2 receipt must not verify a ProcessAuthorized attestation"
        );
    }
}
