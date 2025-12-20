//! Recovery Ceremony
//!
//! Coordinates the recovery of an existing SDIS identity:
//! 1. User presents evidence of identity ownership
//! 2. Stewards attest to the recovery
//! 3. New keys are bound to the original Anchor
//! 4. Old keys are revoked

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{CeremonyError, CeremonyId, CeremonyState};

/// Recovery ceremony phases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryPhase {
    /// Waiting for steward attestations
    WaitingForAttestations,

    /// Verifying evidence
    VerifyingEvidence,

    /// Binding new keys to anchor
    BindingKeys,

    /// Revoking old keys
    RevokingOldKeys,

    /// Ceremony completed successfully
    Completed,

    /// Ceremony failed
    Failed,

    /// Ceremony rejected by stewards
    Rejected,
}

/// Recovery ceremony state machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryCeremony {
    /// Common ceremony state
    pub state: CeremonyState,

    /// Current phase
    pub phase: RecoveryPhase,

    /// DID being recovered (old identity)
    pub old_did: Did,

    /// Proposed new DID
    pub new_did: Did,

    /// Evidence hash (ZK proof of identity ownership)
    pub evidence_hash: [u8; 32],

    /// Collected attestations
    pub attestations: Vec<RecoveryAttestation>,

    /// Anchor commitment (for verification)
    pub anchor_commitment: [u8; 32],

    /// Failure or rejection reason
    pub failure_reason: Option<String>,
}

/// A steward's attestation for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAttestation {
    /// Attesting steward
    pub steward_did: Did,

    /// Whether they support the recovery
    pub supports: bool,

    /// Reason (especially if rejecting)
    pub reason: Option<String>,

    /// Signature over the attestation
    pub signature: Vec<u8>,

    /// Timestamp
    pub timestamp: u64,
}

/// Result of a successful recovery ceremony
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResult {
    /// Ceremony ID
    pub ceremony_id: CeremonyId,

    /// Old DID (now revoked)
    pub old_did: Did,

    /// New DID (now active)
    pub new_did: Did,

    /// Supporting stewards
    pub attesters: Vec<Did>,

    /// Unix timestamp of completion
    pub completed_at: u64,
}

impl RecoveryCeremony {
    /// Create a new recovery ceremony
    pub fn new(
        ceremony_id: CeremonyId,
        coordinator: Did,
        old_did: Did,
        new_did: Did,
        evidence_hash: [u8; 32],
        anchor_commitment: [u8; 32],
        threshold: u32,
    ) -> Self {
        Self {
            state: CeremonyState::new(ceremony_id, coordinator, threshold),
            phase: RecoveryPhase::WaitingForAttestations,
            old_did,
            new_did,
            evidence_hash,
            attestations: Vec::new(),
            anchor_commitment,
            failure_reason: None,
        }
    }

    /// Get the ceremony ID
    pub fn ceremony_id(&self) -> &CeremonyId {
        &self.state.ceremony_id
    }

    /// Check if ceremony can accept attestations
    pub fn can_accept_attestation(&self) -> bool {
        self.phase == RecoveryPhase::WaitingForAttestations && !self.state.is_expired()
    }

    /// Add an attestation from a steward
    pub fn add_attestation(
        &mut self,
        attestation: RecoveryAttestation,
    ) -> Result<(), CeremonyError> {
        if !self.can_accept_attestation() {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "WaitingForAttestations".to_string(),
            });
        }

        // Check for duplicate
        if self
            .attestations
            .iter()
            .any(|a| a.steward_did == attestation.steward_did)
        {
            return Err(CeremonyError::DuplicateParticipation(
                attestation.steward_did.to_string(),
            ));
        }

        // Add participant if supporting
        if attestation.supports {
            self.state.add_participant(attestation.steward_did.clone());
        }

        self.attestations.push(attestation);
        Ok(())
    }

    /// Count supporting attestations
    pub fn supporting_count(&self) -> u32 {
        self.attestations.iter().filter(|a| a.supports).count() as u32
    }

    /// Count rejecting attestations
    pub fn rejecting_count(&self) -> u32 {
        self.attestations.iter().filter(|a| !a.supports).count() as u32
    }

    /// Check if we have enough support to proceed
    pub fn has_enough_support(&self) -> bool {
        self.supporting_count() >= self.state.threshold
    }

    /// Check if recovery has been rejected
    pub fn is_rejected(&self) -> bool {
        // Rejected if more than half explicitly reject
        let total_stewards = self.state.threshold * 2 - 1; // Assuming 2f+1 threshold
        self.rejecting_count() > total_stewards / 2
    }

    /// Proceed to evidence verification
    pub fn begin_verification(&mut self) -> Result<(), CeremonyError> {
        if self.phase != RecoveryPhase::WaitingForAttestations {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "VerifyingEvidence".to_string(),
            });
        }

        if !self.has_enough_support() {
            return Err(CeremonyError::InsufficientParticipants {
                threshold: self.state.threshold,
                have: self.supporting_count(),
            });
        }

        self.phase = RecoveryPhase::VerifyingEvidence;
        Ok(())
    }

    /// Confirm evidence is valid and proceed to key binding
    pub fn confirm_evidence(&mut self) -> Result<(), CeremonyError> {
        if self.phase != RecoveryPhase::VerifyingEvidence {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "BindingKeys".to_string(),
            });
        }

        self.phase = RecoveryPhase::BindingKeys;
        Ok(())
    }

    /// Proceed to old key revocation
    pub fn begin_revocation(&mut self) -> Result<(), CeremonyError> {
        if self.phase != RecoveryPhase::BindingKeys {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "RevokingOldKeys".to_string(),
            });
        }

        self.phase = RecoveryPhase::RevokingOldKeys;
        Ok(())
    }

    /// Complete the recovery ceremony
    pub fn complete(&mut self) -> Result<RecoveryResult, CeremonyError> {
        if self.phase != RecoveryPhase::RevokingOldKeys {
            return Err(CeremonyError::InvalidStateTransition {
                from: format!("{:?}", self.phase),
                to: "Completed".to_string(),
            });
        }

        self.phase = RecoveryPhase::Completed;

        let now = icn_time::current_timestamp_secs();

        Ok(RecoveryResult {
            ceremony_id: self.state.ceremony_id,
            old_did: self.old_did.clone(),
            new_did: self.new_did.clone(),
            attesters: self.state.participants.clone(),
            completed_at: now,
        })
    }

    /// Mark the ceremony as failed
    pub fn fail(&mut self, reason: String) {
        self.phase = RecoveryPhase::Failed;
        self.failure_reason = Some(reason);
    }

    /// Mark the ceremony as rejected
    pub fn reject(&mut self, reason: String) {
        self.phase = RecoveryPhase::Rejected;
        self.failure_reason = Some(reason);
    }

    /// Generate a ceremony ID from DIDs and timestamp
    pub fn generate_ceremony_id(old_did: &Did, new_did: &Did, timestamp: u64) -> CeremonyId {
        let mut hasher = Sha256::new();
        hasher.update(b"icn-recovery-ceremony-v1");
        hasher.update(old_did.as_str().as_bytes());
        hasher.update(new_did.as_str().as_bytes());
        hasher.update(timestamp.to_le_bytes());

        let hash = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&hash);
        id
    }
}

impl RecoveryAttestation {
    /// Create a new supporting attestation
    pub fn support(steward_did: Did, signature: Vec<u8>) -> Self {
        let timestamp = icn_time::current_timestamp_secs();

        Self {
            steward_did,
            supports: true,
            reason: None,
            signature,
            timestamp,
        }
    }

    /// Create a new rejecting attestation
    pub fn reject(steward_did: Did, reason: String, signature: Vec<u8>) -> Self {
        let timestamp = icn_time::current_timestamp_secs();

        Self {
            steward_did,
            supports: false,
            reason: Some(reason),
            signature,
            timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    #[test]
    fn test_recovery_ceremony_creation() {
        let coordinator = test_did();
        let old_did = test_did();
        let new_did = test_did();
        let ceremony_id = [1u8; 32];

        let ceremony = RecoveryCeremony::new(
            ceremony_id,
            coordinator,
            old_did.clone(),
            new_did.clone(),
            [2u8; 32],
            [3u8; 32],
            3,
        );

        assert_eq!(*ceremony.ceremony_id(), ceremony_id);
        assert_eq!(ceremony.old_did, old_did);
        assert_eq!(ceremony.new_did, new_did);
        assert_eq!(ceremony.phase, RecoveryPhase::WaitingForAttestations);
        assert!(ceremony.can_accept_attestation());
    }

    #[test]
    fn test_add_attestations() {
        let coordinator = test_did();
        let mut ceremony = RecoveryCeremony::new(
            [1u8; 32],
            coordinator,
            test_did(),
            test_did(),
            [2u8; 32],
            [3u8; 32],
            2, // threshold of 2
        );

        // Add supporting attestation
        let attestation1 = RecoveryAttestation::support(test_did(), vec![1, 2, 3]);
        ceremony.add_attestation(attestation1).unwrap();
        assert_eq!(ceremony.supporting_count(), 1);
        assert!(!ceremony.has_enough_support());

        // Add another supporting attestation
        let attestation2 = RecoveryAttestation::support(test_did(), vec![4, 5, 6]);
        ceremony.add_attestation(attestation2).unwrap();
        assert_eq!(ceremony.supporting_count(), 2);
        assert!(ceremony.has_enough_support());
    }

    #[test]
    fn test_rejecting_attestation() {
        let coordinator = test_did();
        let mut ceremony = RecoveryCeremony::new(
            [1u8; 32],
            coordinator,
            test_did(),
            test_did(),
            [2u8; 32],
            [3u8; 32],
            2,
        );

        let reject = RecoveryAttestation::reject(
            test_did(),
            "Suspicious evidence".to_string(),
            vec![1, 2, 3],
        );
        ceremony.add_attestation(reject).unwrap();

        assert_eq!(ceremony.supporting_count(), 0);
        assert_eq!(ceremony.rejecting_count(), 1);
    }

    #[test]
    fn test_duplicate_attestation() {
        let coordinator = test_did();
        let steward = test_did();
        let mut ceremony = RecoveryCeremony::new(
            [1u8; 32],
            coordinator,
            test_did(),
            test_did(),
            [2u8; 32],
            [3u8; 32],
            2,
        );

        let attestation1 = RecoveryAttestation::support(steward.clone(), vec![1, 2, 3]);
        ceremony.add_attestation(attestation1).unwrap();

        let attestation2 = RecoveryAttestation::support(steward, vec![4, 5, 6]);
        let result = ceremony.add_attestation(attestation2);
        assert!(matches!(
            result,
            Err(CeremonyError::DuplicateParticipation(_))
        ));
    }

    #[test]
    fn test_recovery_phases() {
        let coordinator = test_did();
        let mut ceremony = RecoveryCeremony::new(
            [1u8; 32],
            coordinator,
            test_did(),
            test_did(),
            [2u8; 32],
            [3u8; 32],
            2,
        );

        // Add enough support
        ceremony
            .add_attestation(RecoveryAttestation::support(test_did(), vec![]))
            .unwrap();
        ceremony
            .add_attestation(RecoveryAttestation::support(test_did(), vec![]))
            .unwrap();

        // Proceed through phases
        ceremony.begin_verification().unwrap();
        assert_eq!(ceremony.phase, RecoveryPhase::VerifyingEvidence);

        ceremony.confirm_evidence().unwrap();
        assert_eq!(ceremony.phase, RecoveryPhase::BindingKeys);

        ceremony.begin_revocation().unwrap();
        assert_eq!(ceremony.phase, RecoveryPhase::RevokingOldKeys);

        let result = ceremony.complete().unwrap();
        assert_eq!(ceremony.phase, RecoveryPhase::Completed);
        assert_eq!(result.attesters.len(), 2);
    }

    #[test]
    fn test_ceremony_rejection() {
        let coordinator = test_did();
        let mut ceremony = RecoveryCeremony::new(
            [1u8; 32],
            coordinator,
            test_did(),
            test_did(),
            [2u8; 32],
            [3u8; 32],
            2,
        );

        ceremony.reject("Evidence not convincing".to_string());
        assert_eq!(ceremony.phase, RecoveryPhase::Rejected);
        assert!(ceremony.failure_reason.is_some());
    }

    #[test]
    fn test_generate_ceremony_id() {
        let old_did = test_did();
        let new_did = test_did();
        let timestamp = 12345u64;

        let id1 = RecoveryCeremony::generate_ceremony_id(&old_did, &new_did, timestamp);
        let id2 = RecoveryCeremony::generate_ceremony_id(&old_did, &new_did, timestamp);
        assert_eq!(id1, id2);

        // Different DIDs should give different ID
        let other_did = test_did();
        let id3 = RecoveryCeremony::generate_ceremony_id(&old_did, &other_did, timestamp);
        assert_ne!(id1, id3);
    }
}
