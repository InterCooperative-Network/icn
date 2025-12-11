//! Steward Ceremonies
//!
//! Enrollment and recovery ceremonies for SDIS identity lifecycle.
//!
//! # Enrollment Ceremony
//!
//! When a new person enrolls in SDIS:
//! 1. User generates VUI commitment
//! 2. User presents proof-of-personhood to a steward
//! 3. Steward verifies and initiates enrollment ceremony
//! 4. Threshold of stewards contribute pepper shares
//! 5. VUI is computed and checked for uniqueness
//! 6. If unique, enrollment token is issued via blind signature
//!
//! # Recovery Ceremony
//!
//! When a user needs to recover their identity:
//! 1. User presents evidence to initiating steward
//! 2. Steward broadcasts recovery request
//! 3. Threshold of stewards attest to the recovery
//! 4. New keys are bound to the original Anchor
//! 5. Old keys are revoked

pub mod enrollment;
pub mod recovery;

pub use enrollment::{EnrollmentCeremony, EnrollmentPhase, EnrollmentResult};
pub use recovery::{RecoveryCeremony, RecoveryPhase, RecoveryResult};

use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Unique identifier for a ceremony
pub type CeremonyId = [u8; 32];

/// Common ceremony state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeremonyState {
    /// Unique ceremony identifier
    pub ceremony_id: CeremonyId,

    /// Coordinating steward
    pub coordinator: Did,

    /// Participating stewards
    pub participants: Vec<Did>,

    /// Required threshold for completion
    pub threshold: u32,

    /// Unix timestamp when ceremony started
    pub started_at: u64,

    /// Unix timestamp when ceremony expires
    pub expires_at: u64,
}

impl CeremonyState {
    /// Default ceremony timeout (15 minutes)
    pub const DEFAULT_TIMEOUT_SECS: u64 = 15 * 60;

    /// Create a new ceremony state
    pub fn new(ceremony_id: CeremonyId, coordinator: Did, threshold: u32) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            ceremony_id,
            coordinator,
            participants: Vec::new(),
            threshold,
            started_at: now,
            expires_at: now + Self::DEFAULT_TIMEOUT_SECS,
        }
    }

    /// Check if ceremony has expired
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.expires_at
    }

    /// Check if threshold is met
    pub fn has_quorum(&self) -> bool {
        self.participants.len() as u32 >= self.threshold
    }

    /// Add a participant
    pub fn add_participant(&mut self, did: Did) {
        if !self.participants.contains(&did) {
            self.participants.push(did);
        }
    }
}

/// Error types for ceremonies
#[derive(Debug, Clone, thiserror::Error)]
pub enum CeremonyError {
    /// Ceremony has expired
    #[error("Ceremony has expired")]
    Expired,

    /// Insufficient participants
    #[error("Insufficient participants: need {threshold}, have {have}")]
    InsufficientParticipants { threshold: u32, have: u32 },

    /// Ceremony not found
    #[error("Ceremony not found: {0}")]
    NotFound(String),

    /// Invalid state transition
    #[error("Invalid state transition: cannot move from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    /// Duplicate participation
    #[error("Steward {0} already participating")]
    DuplicateParticipation(String),

    /// VUI already registered
    #[error("VUI already registered")]
    VuiAlreadyRegistered,

    /// Invalid evidence
    #[error("Invalid evidence: {0}")]
    InvalidEvidence(String),

    /// Signature verification failed
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    #[test]
    fn test_ceremony_state_creation() {
        let coordinator = test_did();
        let ceremony_id = [1u8; 32];

        let state = CeremonyState::new(ceremony_id, coordinator.clone(), 3);

        assert_eq!(state.ceremony_id, ceremony_id);
        assert_eq!(state.coordinator, coordinator);
        assert_eq!(state.threshold, 3);
        assert!(state.participants.is_empty());
        assert!(!state.is_expired());
        assert!(!state.has_quorum());
    }

    #[test]
    fn test_add_participants() {
        let coordinator = test_did();
        let ceremony_id = [1u8; 32];
        let mut state = CeremonyState::new(ceremony_id, coordinator, 2);

        let p1 = test_did();
        let p2 = test_did();

        state.add_participant(p1.clone());
        assert_eq!(state.participants.len(), 1);
        assert!(!state.has_quorum());

        state.add_participant(p2);
        assert_eq!(state.participants.len(), 2);
        assert!(state.has_quorum());

        // Adding duplicate should not increase count
        state.add_participant(p1);
        assert_eq!(state.participants.len(), 2);
    }
}
