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
}
