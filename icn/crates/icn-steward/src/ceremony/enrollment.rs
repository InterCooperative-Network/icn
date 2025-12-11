//! Enrollment Ceremony
//!
//! Coordinates the enrollment of a new person into SDIS:
//! 1. Proof-of-personhood verification
//! 2. VUI computation via threshold PRF
//! 3. Uniqueness checking
//! 4. Enrollment token issuance

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{CeremonyError, CeremonyId, CeremonyState};

/// Enrollment ceremony phases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnrollmentPhase {
    /// Waiting for steward participation
    WaitingForParticipants,

    /// Collecting pepper share contributions
    CollectingShares,

    /// Computing VUI
    ComputingVui,

    /// Checking VUI uniqueness
    CheckingUniqueness,

    /// Issuing enrollment token
    IssuingToken,

    /// Ceremony completed successfully
    Completed,

    /// Ceremony failed
    Failed,
}

/// Enrollment ceremony state machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentCeremony {
    /// Common ceremony state
    pub state: CeremonyState,

    /// Current phase
    pub phase: EnrollmentPhase,

    /// VUI commitment from user
    pub vui_commitment: [u8; 32],

    /// Collected pepper share contributions (encrypted)
    pub share_contributions: Vec<ShareContribution>,

    /// Computed VUI hash (after VUI computation)
    pub vui_hash: Option<[u8; 32]>,

    /// Enrollment pathway proof hash
    pub pathway_hash: [u8; 8],

    /// Blinded token request (for blind signature)
    pub blinded_request: Option<Vec<u8>>,

    /// Failure reason if ceremony failed
    pub failure_reason: Option<String>,
}

/// A steward's pepper share contribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareContribution {
    /// Contributing steward
    pub steward_did: Did,

    /// Encrypted share (encrypted to ceremony coordinator)
    pub encrypted_share: Vec<u8>,

    /// Commitment to the share (for verification)
    pub share_commitment: [u8; 32],

    /// Signature over the contribution
    pub signature: Vec<u8>,
}

/// Result of a successful enrollment ceremony
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentResult {
    /// Ceremony ID
    pub ceremony_id: CeremonyId,

    /// VUI hash (stored in registry)
    pub vui_hash: [u8; 32],

    /// Blinded signature for enrollment token
    pub blinded_signature: Vec<u8>,

    /// Participating stewards
    pub participants: Vec<Did>,

    /// Unix timestamp of completion
    pub completed_at: u64,
}

impl EnrollmentCeremony {
    /// Create a new enrollment ceremony
    pub fn new(
        ceremony_id: CeremonyId,
        coordinator: Did,
        vui_commitment: [u8; 32],
        pathway_hash: [u8; 8],
        threshold: u32,
    ) -> Self {
        Self {
            state: CeremonyState::new(ceremony_id, coordinator, threshold),
            phase: EnrollmentPhase::WaitingForParticipants,
            vui_commitment,
            share_contributions: Vec::new(),
            vui_hash: None,
            pathway_hash,
            blinded_request: None,
            failure_reason: None,
        }
    }

    /// Get the ceremony ID
    pub fn ceremony_id(&self) -> &CeremonyId {
        &self.state.ceremony_id
    }

    /// Check if ceremony can accept new participants
    pub fn can_accept_participant(&self) -> bool {
        matches!(
            self.phase,
            EnrollmentPhase::WaitingForParticipants | EnrollmentPhase::CollectingShares
        ) && !self.state.is_expired()
    }

    /// Add a share contribution from a steward
    pub fn add_share_contribution(
        &mut self,
        contribution: ShareContribution,
    ) -> Result<(), CeremonyError> {
        // Check phase
        if !self.can_accept_participant() {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "CollectingShares".to_string(),
            });
        }

        // Check for duplicate
        if self
            .share_contributions
            .iter()
            .any(|c| c.steward_did == contribution.steward_did)
        {
            return Err(CeremonyError::DuplicateParticipation(
                contribution.steward_did.to_string(),
            ));
        }

        // Add participant
        self.state.add_participant(contribution.steward_did.clone());
        self.share_contributions.push(contribution);

        // Transition to CollectingShares if not already
        if self.phase == EnrollmentPhase::WaitingForParticipants {
            self.phase = EnrollmentPhase::CollectingShares;
        }

        Ok(())
    }

    /// Check if we have enough contributions to proceed
    pub fn has_enough_contributions(&self) -> bool {
        self.share_contributions.len() as u32 >= self.state.threshold
    }

    /// Transition to VUI computation phase
    pub fn begin_vui_computation(&mut self) -> Result<(), CeremonyError> {
        if self.phase != EnrollmentPhase::CollectingShares {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "ComputingVui".to_string(),
            });
        }

        if !self.has_enough_contributions() {
            return Err(CeremonyError::InsufficientParticipants {
                threshold: self.state.threshold,
                have: self.share_contributions.len() as u32,
            });
        }

        self.phase = EnrollmentPhase::ComputingVui;
        Ok(())
    }

    /// Set the computed VUI hash and proceed to uniqueness check
    pub fn set_vui_hash(&mut self, vui_hash: [u8; 32]) -> Result<(), CeremonyError> {
        if self.phase != EnrollmentPhase::ComputingVui {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "CheckingUniqueness".to_string(),
            });
        }

        self.vui_hash = Some(vui_hash);
        self.phase = EnrollmentPhase::CheckingUniqueness;
        Ok(())
    }

    /// Confirm VUI is unique and proceed to token issuance
    pub fn confirm_uniqueness(&mut self) -> Result<(), CeremonyError> {
        if self.phase != EnrollmentPhase::CheckingUniqueness {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "IssuingToken".to_string(),
            });
        }

        self.phase = EnrollmentPhase::IssuingToken;
        Ok(())
    }

    /// Set the blinded token request
    pub fn set_blinded_request(&mut self, blinded_request: Vec<u8>) {
        self.blinded_request = Some(blinded_request);
    }

    /// Complete the ceremony with a blinded signature
    pub fn complete(
        &mut self,
        blinded_signature: Vec<u8>,
    ) -> Result<EnrollmentResult, CeremonyError> {
        if self.phase != EnrollmentPhase::IssuingToken {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "Completed".to_string(),
            });
        }

        let vui_hash = self
            .vui_hash
            .ok_or_else(|| CeremonyError::InvalidEvidence("VUI hash not set".to_string()))?;

        self.phase = EnrollmentPhase::Completed;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(EnrollmentResult {
            ceremony_id: self.state.ceremony_id,
            vui_hash,
            blinded_signature,
            participants: self.state.participants.clone(),
            completed_at: now,
        })
    }

    /// Mark the ceremony as failed
    pub fn fail(&mut self, reason: String) {
        self.phase = EnrollmentPhase::Failed;
        self.failure_reason = Some(reason);
    }

    /// Generate a ceremony ID from commitment and timestamp
    pub fn generate_ceremony_id(vui_commitment: &[u8; 32], timestamp: u64) -> CeremonyId {
        let mut hasher = Sha256::new();
        hasher.update(b"icn-enrollment-ceremony-v1");
        hasher.update(vui_commitment);
        hasher.update(timestamp.to_le_bytes());

        let hash = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&hash);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    fn test_contribution(steward_did: Did) -> ShareContribution {
        ShareContribution {
            steward_did,
            encrypted_share: vec![1, 2, 3],
            share_commitment: [42u8; 32],
            signature: vec![4, 5, 6],
        }
    }

    #[test]
    fn test_enrollment_ceremony_creation() {
        let coordinator = test_did();
        let ceremony_id = [1u8; 32];
        let vui_commitment = [2u8; 32];
        let pathway_hash = [3u8; 8];

        let ceremony = EnrollmentCeremony::new(
            ceremony_id,
            coordinator.clone(),
            vui_commitment,
            pathway_hash,
            3,
        );

        assert_eq!(*ceremony.ceremony_id(), ceremony_id);
        assert_eq!(ceremony.phase, EnrollmentPhase::WaitingForParticipants);
        assert!(ceremony.can_accept_participant());
        assert!(!ceremony.has_enough_contributions());
    }

    #[test]
    fn test_add_share_contributions() {
        let coordinator = test_did();
        let ceremony_id = [1u8; 32];
        let mut ceremony = EnrollmentCeremony::new(
            ceremony_id,
            coordinator,
            [2u8; 32],
            [3u8; 8],
            2, // threshold of 2
        );

        // Add first contribution
        let contrib1 = test_contribution(test_did());
        ceremony.add_share_contribution(contrib1).unwrap();
        assert_eq!(ceremony.phase, EnrollmentPhase::CollectingShares);
        assert!(!ceremony.has_enough_contributions());

        // Add second contribution
        let contrib2 = test_contribution(test_did());
        ceremony.add_share_contribution(contrib2).unwrap();
        assert!(ceremony.has_enough_contributions());
    }

    #[test]
    fn test_duplicate_contribution() {
        let coordinator = test_did();
        let steward = test_did();
        let ceremony_id = [1u8; 32];
        let mut ceremony =
            EnrollmentCeremony::new(ceremony_id, coordinator, [2u8; 32], [3u8; 8], 2);

        let contrib1 = test_contribution(steward.clone());
        ceremony.add_share_contribution(contrib1).unwrap();

        // Try to add duplicate
        let contrib2 = test_contribution(steward);
        let result = ceremony.add_share_contribution(contrib2);
        assert!(matches!(
            result,
            Err(CeremonyError::DuplicateParticipation(_))
        ));
    }

    #[test]
    fn test_ceremony_phases() {
        let coordinator = test_did();
        let ceremony_id = [1u8; 32];
        let mut ceremony =
            EnrollmentCeremony::new(ceremony_id, coordinator, [2u8; 32], [3u8; 8], 2);

        // Add enough contributions
        ceremony
            .add_share_contribution(test_contribution(test_did()))
            .unwrap();
        ceremony
            .add_share_contribution(test_contribution(test_did()))
            .unwrap();

        // Proceed through phases
        ceremony.begin_vui_computation().unwrap();
        assert_eq!(ceremony.phase, EnrollmentPhase::ComputingVui);

        ceremony.set_vui_hash([42u8; 32]).unwrap();
        assert_eq!(ceremony.phase, EnrollmentPhase::CheckingUniqueness);

        ceremony.confirm_uniqueness().unwrap();
        assert_eq!(ceremony.phase, EnrollmentPhase::IssuingToken);

        let result = ceremony.complete(vec![1, 2, 3]).unwrap();
        assert_eq!(ceremony.phase, EnrollmentPhase::Completed);
        assert_eq!(result.vui_hash, [42u8; 32]);
    }

    #[test]
    fn test_ceremony_failure() {
        let coordinator = test_did();
        let ceremony_id = [1u8; 32];
        let mut ceremony =
            EnrollmentCeremony::new(ceremony_id, coordinator, [2u8; 32], [3u8; 8], 2);

        ceremony.fail("Test failure".to_string());
        assert_eq!(ceremony.phase, EnrollmentPhase::Failed);
        assert_eq!(ceremony.failure_reason, Some("Test failure".to_string()));
    }

    #[test]
    fn test_generate_ceremony_id() {
        let commitment = [1u8; 32];
        let timestamp = 12345u64;

        let id1 = EnrollmentCeremony::generate_ceremony_id(&commitment, timestamp);
        let id2 = EnrollmentCeremony::generate_ceremony_id(&commitment, timestamp);
        assert_eq!(id1, id2);

        // Different timestamp should give different ID
        let id3 = EnrollmentCeremony::generate_ceremony_id(&commitment, timestamp + 1);
        assert_ne!(id1, id3);
    }
}
