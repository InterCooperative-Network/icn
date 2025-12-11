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
        /// Requesting steward
        steward_did: Did,
        /// VUI commitment being enrolled
        vui_commitment: [u8; 32],
        /// Timestamp
        timestamp: u64,
    },

    /// Acknowledge participation
    ParticipationAck {
        /// Ceremony ID
        ceremony_id: [u8; 32],
        /// Acknowledging steward
        steward_did: Did,
        /// Their pepper share contribution (encrypted)
        encrypted_share: Vec<u8>,
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
            timestamp: 12345,
        };

        if let EnrollmentMessage::ParticipationRequest {
            ceremony_id,
            vui_commitment,
            ..
        } = msg
        {
            assert_eq!(ceremony_id, [1u8; 32]);
            assert_eq!(vui_commitment, [2u8; 32]);
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
