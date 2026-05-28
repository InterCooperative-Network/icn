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
        bytes.extend_from_slice(&(proposal_id.len() as u64).to_le_bytes());
        bytes.extend_from_slice(proposal_id.as_bytes());
        bytes.extend_from_slice(&(domain_id.len() as u64).to_le_bytes());
        bytes.extend_from_slice(domain_id.as_bytes());
        bytes.push(outcome_ordinal(outcome));
        bytes.extend_from_slice(&(vote_tally.for_votes as u64).to_le_bytes());
        bytes.extend_from_slice(&(vote_tally.against_votes as u64).to_le_bytes());
        bytes.extend_from_slice(&(vote_tally.abstain_votes as u64).to_le_bytes());
        bytes.extend_from_slice(vote_hash);
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
        // Use u64 length prefixes to match the canonical encoding used by
        // `GovernanceProof::compute_proof_hash` and `compute_vote_hash`
        // elsewhere in this module — keeps the hash binding consistent
        // across receipt types and avoids any risk of u32 truncation.
        for field in [item_id, domain_id, actor_did] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
        }
        let transition_byte: u8 = match transition {
            ActionItemTransition::Completed => 0,
        };
        hasher.update(&[transition_byte]);
        hasher.update(&completed_at.to_le_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
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
        for field in [meeting_id, domain_id, attendee_did, recorded_by] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
        }
        let transition_byte: u8 = match transition {
            MeetingAttendanceTransition::Present => 0,
            MeetingAttendanceTransition::Remote => 1,
        };
        hasher.update(&[transition_byte]);
        hasher.update(&recorded_at.to_le_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
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
        // Build a deterministic UUID v4 from a 16-byte seed so canonical
        // hash assertions are stable across runs. `uuid::Uuid::from_bytes`
        // accepts any 16-byte sequence; the result is not RFC-4122
        // versioned, which is fine — the canonical hash binds the raw
        // bytes, not the version field.
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
            holder: "holder-1".to_string(),
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
        let a = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: "did:icn:holder".to_string(),
        });
        let b = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-2".to_string(),
            holder: "did:icn:holder".to_string(),
        });
        assert_ne!(a.ref_hash(), b.ref_hash());
    }

    #[test]
    fn mandate_grant_ref_role_holder_changes_hash() {
        // structure_id constant; holder varies.
        let a = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: "did:icn:holder-a".to_string(),
        });
        let b = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: "did:icn:holder-b".to_string(),
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
                    holder: "did:icn:holder".to_string(),
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
    fn mandate_grant_ref_serde_roundtrip_preserves_hash() {
        let r = sample_ref(MandateGrantRefTarget::Role {
            structure_id: "office-1".to_string(),
            holder: "did:icn:holder".to_string(),
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
}
