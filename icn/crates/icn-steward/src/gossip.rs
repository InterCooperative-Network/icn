//! Steward Gossip Messages
//!
//! Message types for steward network coordination including
//! announcements, VUI sync, and ceremony coordination.

use icn_identity::Did;
use serde::{Deserialize, Serialize};

use crate::profile::{JurisdictionTier, StewardStatus};

/// Messages gossiped on the steward network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StewardMessage {
    /// Steward announcing itself or status update
    Announce(StewardAnnouncement),

    /// VUI registry synchronization
    VuiSync(VuiSyncMessage),

    /// Enrollment ceremony coordination
    Enrollment(EnrollmentMessage),

    /// Recovery ceremony coordination
    Recovery(RecoveryMessage),
}

/// Steward announcement message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardAnnouncement {
    /// DID of the announcing steward
    pub steward_did: Did,

    /// Current operational status
    pub status: StewardStatus,

    /// Jurisdiction tier
    pub jurisdiction_tier: JurisdictionTier,

    /// Region code
    pub region: String,

    /// Sequence number for ordering
    pub sequence: u64,

    /// Unix timestamp
    pub timestamp: u64,

    /// Signature over the announcement
    pub signature: Vec<u8>,
}

impl StewardAnnouncement {
    /// Create a new announcement
    pub fn new(
        steward_did: Did,
        status: StewardStatus,
        jurisdiction_tier: JurisdictionTier,
        region: String,
        sequence: u64,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            steward_did,
            status,
            jurisdiction_tier,
            region,
            sequence,
            timestamp,
            signature: Vec::new(), // To be signed
        }
    }

    /// Get the message bytes for signing
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"icn-steward-announce-v1");
        bytes.extend_from_slice(self.steward_did.as_str().as_bytes());
        bytes.extend_from_slice(&self.sequence.to_le_bytes());
        bytes.extend_from_slice(&self.timestamp.to_le_bytes());
        bytes
    }
}

/// VUI registry synchronization message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VuiSyncMessage {
    /// Request sync from a specific checkpoint
    SyncRequest {
        /// Requesting steward
        from_did: Did,
        /// Last known checkpoint hash
        checkpoint_hash: [u8; 32],
        /// Sequence number
        sequence: u64,
    },

    /// Response with VUI entries since checkpoint
    SyncResponse {
        /// Responding steward
        from_did: Did,
        /// New checkpoint hash
        new_checkpoint_hash: [u8; 32],
        /// VUI hashes to add (for Bloom filter)
        vui_hashes: Vec<[u8; 32]>,
        /// Sequence number
        sequence: u64,
    },

    /// Announce a new VUI registration
    NewVui {
        /// Registering steward
        from_did: Did,
        /// Hash of the VUI (not the VUI itself for privacy)
        vui_hash: [u8; 32],
        /// Timestamp of registration
        timestamp: u64,
        /// Signature from registering steward
        signature: Vec<u8>,
    },
}

/// Enrollment ceremony coordination message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnrollmentMessage {
    /// Request to participate in enrollment
    ParticipationRequest {
        /// Ceremony ID
        ceremony_id: [u8; 32],
        /// Requesting steward (coordinator)
        steward_did: Did,
        /// VUI commitment being enrolled
        vui_commitment: [u8; 32],
        /// Pathway proof hash
        pathway_hash: [u8; 8],
        /// Threshold required
        threshold: u32,
        /// Timestamp
        timestamp: u64,
        /// Coordinator signature
        signature: Vec<u8>,
    },

    /// Acknowledge participation with PRF partial
    ParticipationAck {
        /// Ceremony ID
        ceremony_id: [u8; 32],
        /// Acknowledging steward
        steward_did: Did,
        /// Share index (for combination)
        share_index: u8,
        /// PRF partial (encrypted to user's public key)
        encrypted_prf_partial: Vec<u8>,
        /// Commitment to pepper share (for verification)
        share_commitment: [u8; 32],
        /// Timestamp
        timestamp: u64,
        /// Signature
        signature: Vec<u8>,
    },

    /// PRF partial delivery to user (via coordinator)
    PrfPartialDelivery {
        /// Ceremony ID
        ceremony_id: [u8; 32],
        /// Originating steward
        steward_did: Did,
        /// Share index
        share_index: u8,
        /// Encrypted PRF partial (to user)
        encrypted_partial: Vec<u8>,
        /// Signature
        signature: Vec<u8>,
    },

    /// Uniqueness verification request
    UniquenessCheck {
        /// Ceremony ID
        ceremony_id: [u8; 32],
        /// Requesting steward (coordinator)
        steward_did: Did,
        /// VUI hash to check
        vui_hash: [u8; 32],
        /// Timestamp
        timestamp: u64,
    },

    /// Uniqueness verification response
    UniquenessResponse {
        /// Ceremony ID
        ceremony_id: [u8; 32],
        /// Responding steward
        steward_did: Did,
        /// VUI hash checked
        vui_hash: [u8; 32],
        /// Is unique (not found in registry)
        is_unique: bool,
        /// Timestamp
        timestamp: u64,
        /// Signature
        signature: Vec<u8>,
    },

    /// Ceremony completed
    CeremonyComplete {
        /// Ceremony ID
        ceremony_id: [u8; 32],
        /// Coordinating steward
        coordinator_did: Did,
        /// VUI hash (for registry)
        vui_hash: [u8; 32],
        /// Participating stewards
        participants: Vec<Did>,
        /// Timestamp
        timestamp: u64,
        /// Coordinator signature
        signature: Vec<u8>,
    },

    /// Ceremony failed
    CeremonyFailed {
        /// Ceremony ID
        ceremony_id: [u8; 32],
        /// Coordinating steward
        coordinator_did: Did,
        /// Failure reason
        reason: String,
        /// Timestamp
        timestamp: u64,
        /// Coordinator signature
        signature: Vec<u8>,
    },
}

/// Recovery ceremony coordination message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryMessage {
    /// Request to initiate recovery
    RecoveryRequest {
        /// Recovery ceremony ID
        ceremony_id: [u8; 32],
        /// DID being recovered (old identity)
        old_did: Did,
        /// Proposed new DID
        new_did: Did,
        /// Requesting steward
        initiator_did: Did,
        /// Evidence hash (ZK proof of identity)
        evidence_hash: [u8; 32],
        /// Timestamp
        timestamp: u64,
        /// Signature
        signature: Vec<u8>,
    },

    /// Attest to recovery (support the request)
    RecoveryAttestation {
        /// Recovery ceremony ID
        ceremony_id: [u8; 32],
        /// Attesting steward
        steward_did: Did,
        /// Support or reject
        supports: bool,
        /// Reason (if rejecting)
        reason: Option<String>,
        /// Timestamp
        timestamp: u64,
        /// Signature
        signature: Vec<u8>,
    },

    /// Recovery completed
    RecoveryComplete {
        /// Recovery ceremony ID
        ceremony_id: [u8; 32],
        /// Old DID
        old_did: Did,
        /// New DID (now active)
        new_did: Did,
        /// Supporting stewards
        attesters: Vec<Did>,
        /// Timestamp
        timestamp: u64,
        /// Coordinator signature
        signature: Vec<u8>,
    },
}

impl EnrollmentMessage {
    /// Get the message bytes for signing
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        match self {
            Self::ParticipationRequest {
                ceremony_id,
                steward_did,
                vui_commitment,
                pathway_hash,
                threshold,
                timestamp,
                ..
            } => {
                bytes.extend_from_slice(b"icn-enrollment-participation-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(steward_did.as_str().as_bytes());
                bytes.extend_from_slice(vui_commitment);
                bytes.extend_from_slice(pathway_hash);
                bytes.extend_from_slice(&threshold.to_le_bytes());
                bytes.extend_from_slice(&timestamp.to_le_bytes());
            }
            Self::ParticipationAck {
                ceremony_id,
                steward_did,
                share_index,
                encrypted_prf_partial,
                share_commitment,
                timestamp,
                ..
            } => {
                bytes.extend_from_slice(b"icn-enrollment-ack-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(steward_did.as_str().as_bytes());
                bytes.push(*share_index);
                bytes.extend_from_slice(encrypted_prf_partial);
                bytes.extend_from_slice(share_commitment);
                bytes.extend_from_slice(&timestamp.to_le_bytes());
            }
            Self::PrfPartialDelivery {
                ceremony_id,
                steward_did,
                share_index,
                encrypted_partial,
                ..
            } => {
                bytes.extend_from_slice(b"icn-enrollment-prf-delivery-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(steward_did.as_str().as_bytes());
                bytes.push(*share_index);
                bytes.extend_from_slice(encrypted_partial);
            }
            Self::UniquenessCheck {
                ceremony_id,
                steward_did,
                vui_hash,
                timestamp,
            } => {
                bytes.extend_from_slice(b"icn-enrollment-uniqueness-check-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(steward_did.as_str().as_bytes());
                bytes.extend_from_slice(vui_hash);
                bytes.extend_from_slice(&timestamp.to_le_bytes());
            }
            Self::UniquenessResponse {
                ceremony_id,
                steward_did,
                vui_hash,
                is_unique,
                timestamp,
                ..
            } => {
                bytes.extend_from_slice(b"icn-enrollment-uniqueness-response-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(steward_did.as_str().as_bytes());
                bytes.extend_from_slice(vui_hash);
                bytes.push(if *is_unique { 1 } else { 0 });
                bytes.extend_from_slice(&timestamp.to_le_bytes());
            }
            Self::CeremonyComplete {
                ceremony_id,
                coordinator_did,
                vui_hash,
                participants,
                timestamp,
                ..
            } => {
                bytes.extend_from_slice(b"icn-enrollment-complete-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(coordinator_did.as_str().as_bytes());
                bytes.extend_from_slice(vui_hash);
                for p in participants {
                    bytes.extend_from_slice(p.as_str().as_bytes());
                }
                bytes.extend_from_slice(&timestamp.to_le_bytes());
            }
            Self::CeremonyFailed {
                ceremony_id,
                coordinator_did,
                reason,
                timestamp,
                ..
            } => {
                bytes.extend_from_slice(b"icn-enrollment-failed-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(coordinator_did.as_str().as_bytes());
                bytes.extend_from_slice(reason.as_bytes());
                bytes.extend_from_slice(&timestamp.to_le_bytes());
            }
        }

        bytes
    }
}

impl RecoveryMessage {
    /// Get the message bytes for signing
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        match self {
            Self::RecoveryRequest {
                ceremony_id,
                old_did,
                new_did,
                initiator_did,
                evidence_hash,
                timestamp,
                ..
            } => {
                bytes.extend_from_slice(b"icn-recovery-request-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(old_did.as_str().as_bytes());
                bytes.extend_from_slice(new_did.as_str().as_bytes());
                bytes.extend_from_slice(initiator_did.as_str().as_bytes());
                bytes.extend_from_slice(evidence_hash);
                bytes.extend_from_slice(&timestamp.to_le_bytes());
            }
            Self::RecoveryAttestation {
                ceremony_id,
                steward_did,
                supports,
                reason,
                timestamp,
                ..
            } => {
                bytes.extend_from_slice(b"icn-recovery-attestation-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(steward_did.as_str().as_bytes());
                bytes.push(if *supports { 1 } else { 0 });
                if let Some(r) = reason {
                    bytes.extend_from_slice(r.as_bytes());
                }
                bytes.extend_from_slice(&timestamp.to_le_bytes());
            }
            Self::RecoveryComplete {
                ceremony_id,
                old_did,
                new_did,
                attesters,
                timestamp,
                ..
            } => {
                bytes.extend_from_slice(b"icn-recovery-complete-v1");
                bytes.extend_from_slice(ceremony_id);
                bytes.extend_from_slice(old_did.as_str().as_bytes());
                bytes.extend_from_slice(new_did.as_str().as_bytes());
                for a in attesters {
                    bytes.extend_from_slice(a.as_str().as_bytes());
                }
                bytes.extend_from_slice(&timestamp.to_le_bytes());
            }
        }

        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::StewardStatus;

    fn test_did() -> Did {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    #[test]
    fn test_steward_announcement() {
        let did = test_did();
        let announcement = StewardAnnouncement::new(
            did.clone(),
            StewardStatus::Active,
            JurisdictionTier::Tier1,
            "US".to_string(),
            1,
        );

        assert_eq!(announcement.steward_did, did);
        assert_eq!(announcement.sequence, 1);
        assert!(announcement.timestamp > 0);

        // Signing bytes should be deterministic
        let bytes1 = announcement.signing_bytes();
        let bytes2 = announcement.signing_bytes();
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_vui_sync_message() {
        let did = test_did();
        let msg = VuiSyncMessage::SyncRequest {
            from_did: did,
            checkpoint_hash: [0u8; 32],
            sequence: 1,
        };

        if let VuiSyncMessage::SyncRequest {
            checkpoint_hash,
            sequence,
            ..
        } = msg
        {
            assert_eq!(checkpoint_hash, [0u8; 32]);
            assert_eq!(sequence, 1);
        }
    }

    #[test]
    fn test_enrollment_message() {
        let steward_did = test_did();
        let msg = EnrollmentMessage::ParticipationRequest {
            ceremony_id: [1u8; 32],
            steward_did: steward_did.clone(),
            vui_commitment: [2u8; 32],
            pathway_hash: [3u8; 8],
            threshold: 3,
            timestamp: 12345,
            signature: vec![4, 5, 6],
        };

        if let EnrollmentMessage::ParticipationRequest {
            ceremony_id,
            vui_commitment,
            threshold,
            ..
        } = msg
        {
            assert_eq!(ceremony_id, [1u8; 32]);
            assert_eq!(vui_commitment, [2u8; 32]);
            assert_eq!(threshold, 3);
        }
    }

    #[test]
    fn test_enrollment_prf_delivery() {
        let steward_did = test_did();
        let msg = EnrollmentMessage::PrfPartialDelivery {
            ceremony_id: [1u8; 32],
            steward_did,
            share_index: 1,
            encrypted_partial: vec![1, 2, 3, 4],
            signature: vec![5, 6, 7, 8],
        };

        if let EnrollmentMessage::PrfPartialDelivery {
            share_index,
            encrypted_partial,
            ..
        } = msg
        {
            assert_eq!(share_index, 1);
            assert_eq!(encrypted_partial, vec![1, 2, 3, 4]);
        }
    }

    #[test]
    fn test_uniqueness_check() {
        let steward_did = test_did();
        let msg = EnrollmentMessage::UniquenessResponse {
            ceremony_id: [1u8; 32],
            steward_did,
            vui_hash: [42u8; 32],
            is_unique: true,
            timestamp: 12345,
            signature: vec![1, 2, 3],
        };

        if let EnrollmentMessage::UniquenessResponse { is_unique, .. } = msg {
            assert!(is_unique);
        }
    }

    #[test]
    fn test_recovery_message() {
        let old_did = test_did();
        let new_did = test_did();
        let initiator_did = test_did();

        let msg = RecoveryMessage::RecoveryRequest {
            ceremony_id: [3u8; 32],
            old_did: old_did.clone(),
            new_did: new_did.clone(),
            initiator_did: initiator_did.clone(),
            evidence_hash: [4u8; 32],
            timestamp: 54321,
            signature: Vec::new(),
        };

        if let RecoveryMessage::RecoveryRequest {
            ceremony_id,
            evidence_hash,
            ..
        } = msg
        {
            assert_eq!(ceremony_id, [3u8; 32]);
            assert_eq!(evidence_hash, [4u8; 32]);
        }
    }
}
