use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use icn_identity::{Signature, KeyPair, IdentityId};
use crate::error::{FederationError, FederationResult};
use crate::quorum::SignerQuorumConfig;
use crate::genesis::FederationMetadata;
use crate::dag_anchor::GenesisAnchor;
use base64::engine::general_purpose::URL_SAFE;
use base64::Engine;

/// Represents the type of recovery event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryEventType {
    /// Federation key rotation event
    FederationKeyRotation,
    /// Signer succession event (add/remove/replace signers)
    Succession,
    /// Quorum configuration update
    QuorumUpdate,
    /// Disaster recovery event
    DisasterRecovery,
    /// Federation metadata update
    MetadataUpdate,
}

/// Base structure for all recovery events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryEvent {
    /// Type of recovery event
    pub event_type: RecoveryEventType,
    /// Federation DID
    pub federation_did: String,
    /// Sequence number of this event
    pub sequence_number: u64,
    /// Previous event anchor CID, if any
    pub previous_event_cid: Option<String>,
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Quorum proof signatures
    pub signatures: Vec<Signature>,
}

/// Federation key rotation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationKeyRotationEvent {
    /// Base recovery event data
    pub base: RecoveryEvent,
    /// New federation DID
    pub new_federation_did: String,
    /// Proof of new key ownership
    pub key_proof: Signature,
}

/// Serializable version of a DID signer for recovery events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableSigner {
    /// Signer DID
    pub did: String,
    /// Signer Public Key (base64 encoded)
    pub public_key: String,
}

/// Signer succession event for adding, removing, or replacing signers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessionEvent {
    /// Base recovery event data
    pub base: RecoveryEvent,
    /// Signers to add (serializable version)
    pub signers_to_add: Vec<SerializableSigner>,
    /// Signer DIDs to remove
    pub signers_to_remove: Vec<String>,
    /// Updated quorum configuration (if changed)
    pub updated_quorum_config: Option<SignerQuorumConfig>,
}

/// Disaster recovery anchor for federation reconstitution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisasterRecoveryAnchor {
    /// Base recovery event data
    pub base: RecoveryEvent,
    /// New federation DID
    pub new_federation_did: String,
    /// New signer set (serializable version)
    pub new_signers: Vec<SerializableSigner>,
    /// New quorum configuration
    pub new_quorum_config: SignerQuorumConfig,
    /// Justification for disaster recovery
    pub justification: String,
    /// External attestations from trusted parties
    pub external_attestations: Vec<ExternalAttestation>,
}

/// Attestation from an external trusted party
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAttestation {
    /// DID of the attesting party
    pub attester_did: String,
    /// Timestamp of attestation
    pub timestamp: DateTime<Utc>,
    /// Attestation statement
    pub statement: String,
    /// Signature of the attesting party
    pub signature: Signature,
}

/// Federation metadata update event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataUpdateEvent {
    /// Base recovery event data
    pub base: RecoveryEvent,
    /// Updated federation metadata
    pub updated_metadata: FederationMetadata,
}

/// Recovery module functions
pub mod recovery {
    use super::*;
    
    /// Create a federation key rotation event
    pub async fn create_key_rotation_event(
        federation_did: &str,
        new_keypair: &KeyPair,
        sequence_number: u64,
        previous_event_cid: Option<String>,
        signatures: Vec<(IdentityId, Signature)>,
        quorum_config: &SignerQuorumConfig,
    ) -> FederationResult<FederationKeyRotationEvent> {
        // Generate new federation DID from new keypair
        // Since we can't directly access the public key, we'd need a proper method
        // For now, we'll just use a placeholder DID
        let new_federation_did = format!("did:key:new_federation_{}", sequence_number);
        
        // Create the base recovery event
        let base = RecoveryEvent {
            event_type: RecoveryEventType::FederationKeyRotation,
            federation_did: federation_did.to_string(),
            sequence_number,
            previous_event_cid,
            timestamp: Utc::now(),
            signatures: signatures.iter().map(|(_, sig)| sig.clone()).collect(),
        };
        
        // Create the key rotation event
        let mut rotation_event = FederationKeyRotationEvent {
            base,
            new_federation_did,
            key_proof: Signature(vec![]),  // Will be filled below
        };
        
        // Generate signature proof with new key
        let message = format!("Federation key rotation from {} to {} at {}", 
            federation_did, 
            rotation_event.new_federation_did,
            rotation_event.base.timestamp.to_rfc3339()
        );
        
        let key_proof = icn_identity::sign_message(message.as_bytes(), new_keypair)
            .map_err(|e| FederationError::CryptoError(format!("Failed to sign key proof: {}", e)))?;
        
        rotation_event.key_proof = key_proof;
        
        Ok(rotation_event)
    }
    
    /// Create a signer succession event
    pub async fn create_succession_event(
        federation_did: &str,
        sequence_number: u64,
        previous_event_cid: Option<String>,
        signers_to_add: Vec<SerializableSigner>,
        signers_to_remove: Vec<String>,
        updated_quorum_config: Option<SignerQuorumConfig>,
        signatures: Vec<(IdentityId, Signature)>,
    ) -> FederationResult<SuccessionEvent> {
        // Create base recovery event
        let base = RecoveryEvent {
            event_type: RecoveryEventType::Succession,
            federation_did: federation_did.to_string(),
            sequence_number,
            previous_event_cid,
            timestamp: Utc::now(),
            signatures: signatures.iter().map(|(_, sig)| sig.clone()).collect(),
        };
        
        // Create the succession event
        let succession_event = SuccessionEvent {
            base,
            signers_to_add,
            signers_to_remove,
            updated_quorum_config,
        };
        
        Ok(succession_event)
    }
    
    /// Create a disaster recovery anchor
    pub async fn create_disaster_recovery_anchor(
        federation_did: &str,
        new_federation_did: &str,
        sequence_number: u64,
        new_signers: Vec<SerializableSigner>,
        new_quorum_config: SignerQuorumConfig,
        justification: String,
        external_attestations: Vec<ExternalAttestation>,
    ) -> FederationResult<DisasterRecoveryAnchor> {
        // Create base recovery event
        let base = RecoveryEvent {
            event_type: RecoveryEventType::DisasterRecovery,
            federation_did: federation_did.to_string(),
            sequence_number,
            previous_event_cid: None, // Usually a disaster recovery doesn't have a previous event CID
            timestamp: Utc::now(),
            signatures: vec![],  // Will be filled with signatures from new signers
        };
        
        // Create the disaster recovery anchor
        let recovery_anchor = DisasterRecoveryAnchor {
            base,
            new_federation_did: new_federation_did.to_string(),
            new_signers,
            new_quorum_config,
            justification,
            external_attestations,
        };
        
        Ok(recovery_anchor)
    }
    
    /// Create a metadata update event
    pub async fn create_metadata_update_event(
        federation_did: &str,
        sequence_number: u64,
        previous_event_cid: Option<String>,
        updated_metadata: FederationMetadata,
        signatures: Vec<(IdentityId, Signature)>,
    ) -> FederationResult<MetadataUpdateEvent> {
        // Create base recovery event
        let base = RecoveryEvent {
            event_type: RecoveryEventType::MetadataUpdate,
            federation_did: federation_did.to_string(),
            sequence_number,
            previous_event_cid,
            timestamp: Utc::now(),
            signatures: signatures.iter().map(|(_, sig)| sig.clone()).collect(),
        };
        
        // Create the metadata update event
        let metadata_event = MetadataUpdateEvent {
            base,
            updated_metadata,
        };
        
        Ok(metadata_event)
    }
    
    /// Verify a recovery event's signatures against a quorum config
    pub async fn verify_recovery_event(
        event: &RecoveryEvent,
        member_dids: &[String],
        quorum_config: &SignerQuorumConfig,
    ) -> FederationResult<bool> {
        // Implementation would verify signatures against quorum config
        // For now, just a placeholder
        Ok(true)
    }
    
    /// Create a DAG anchor for a recovery event
    pub async fn anchor_recovery_event(
        event: &RecoveryEvent,
        federation_keypair: &KeyPair,
    ) -> FederationResult<String> {
        // Implementation would create a DAG anchor for the event
        // Return the CID of the anchor
        Ok("recovery_event_anchor_cid".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quorum::{initialization, QuorumType};
    use icn_identity::IdentityId;
    
    #[tokio::test]
    async fn test_key_rotation() {
        // 1. Set up test quorum configuration
        let test_quorum = initialization::initialize_test_quorum(3, QuorumType::Majority).await.unwrap();
        
        // 2. Create federation DID and keypair
        let federation_did = "did:key:z6MkFederation123".to_string();
        let federation_keypair = KeyPair::new(vec![1, 2, 3, 4], vec![5, 6, 7, 8, 9]); // Simplified for testing
        
        // 3. Create a new keypair for rotation
        let new_federation_keypair = KeyPair::new(vec![9, 8, 7, 6], vec![5, 4, 3, 2, 1]); // Simplified for testing
        let new_federation_did = "did:key:z6MkFederationNew456".to_string();
        
        // 4. Create test signatures
        let test_signatures = vec![
            (IdentityId("did:key:test1".to_string()), Signature(vec![1, 2, 3])),
            (IdentityId("did:key:test2".to_string()), Signature(vec![4, 5, 6])),
        ];
        
        // 5. Create key rotation event
        let key_rotation_result = recovery::create_key_rotation_event(
            &federation_did,
            &new_federation_keypair,
            1, // First event
            None, // No previous event
            test_signatures,
            &test_quorum,
        ).await;
        
        assert!(key_rotation_result.is_ok(), "Failed to create key rotation event: {:?}", key_rotation_result.err());
        
        let key_rotation_event = key_rotation_result.unwrap();
        
        // 6. Verify the event fields
        assert_eq!(key_rotation_event.base.event_type, RecoveryEventType::FederationKeyRotation);
        assert_eq!(key_rotation_event.base.federation_did, federation_did);
        assert_eq!(key_rotation_event.base.sequence_number, 1);
        assert_eq!(key_rotation_event.base.previous_event_cid, None);
        
        // 7. Create a mock anchor CID for this event
        let anchor_cid = "bafykeyfederation1".to_string();
        
        // 8. Create test metadata for subsequent event
        let test_metadata = FederationMetadata {
            federation_did: new_federation_did.clone(),
            name: "Updated Federation".to_string(),
            description: Some("This federation has a rotated key".to_string()),
            created_at: Utc::now(),
            initial_policies: vec![],
            initial_members: vec![],
            signer_quorum: test_quorum.clone(),
            genesis_cid: cid::Cid::default(), 
            additional_metadata: None,
        };
        
        // 9. Create subsequent event  
        let subsequent_event_result = recovery::create_metadata_update_event(
            &new_federation_did, // Using the new federation DID
            2, // Second event
            Some(anchor_cid), // Previous event CID
            test_metadata,
            test_signatures,
        ).await;
        
        assert!(subsequent_event_result.is_ok(), "Failed to create subsequent event: {:?}", subsequent_event_result.err());
        
        let subsequent_event = subsequent_event_result.unwrap();
        
        // 10. Verify the subsequent event fields
        assert_eq!(subsequent_event.base.event_type, RecoveryEventType::MetadataUpdate);
        assert_eq!(subsequent_event.base.federation_did, new_federation_did);
        assert_eq!(subsequent_event.base.sequence_number, 2);
        assert_eq!(subsequent_event.base.previous_event_cid, Some(anchor_cid));
        assert_eq!(subsequent_event.updated_metadata.federation_did, new_federation_did);
    }
    
    #[tokio::test]
    async fn test_succession() {
        // 1. Initialize test quorum
        let test_quorum = initialization::initialize_test_quorum(3, QuorumType::Majority).await.unwrap();
        
        // 2. Create federation DID
        let federation_did = "did:key:z6MkFederation123".to_string();
        
        // 3. Create test signers to add/remove
        let signer_to_add = SerializableSigner {
            did: "did:key:z6MkNewSigner456".to_string(),
            public_key: "base64encodedpublickey".to_string(),
        };
        
        let signer_to_remove = "did:key:z6MkOldSigner789".to_string();
        
        // 4. Create test signatures
        let test_signatures = vec![
            (IdentityId("did:key:test1".to_string()), Signature(vec![1, 2, 3])),
            (IdentityId("did:key:test2".to_string()), Signature(vec![4, 5, 6])),
        ];
        
        // 5. Create a new quorum configuration with higher threshold
        let updated_quorum_dids = vec![
            "did:key:z6MkSigner1".to_string(),
            "did:key:z6MkSigner2".to_string(),
            "did:key:z6MkNewSigner456".to_string(),
        ];
        
        let updated_quorum = SignerQuorumConfig::new_threshold(updated_quorum_dids, 75);
        
        // 6. Create succession event
        let succession_result = recovery::create_succession_event(
            &federation_did,
            1, // First event
            None, // No previous event
            vec![signer_to_add], // Add new signer
            vec![signer_to_remove.clone()], // Remove old signer
            Some(updated_quorum.clone()), // Update quorum configuration
            test_signatures,
        ).await;
        
        assert!(succession_result.is_ok(), "Failed to create succession event: {:?}", succession_result.err());
        
        let succession_event = succession_result.unwrap();
        
        // 7. Verify the event fields
        assert_eq!(succession_event.base.event_type, RecoveryEventType::Succession);
        assert_eq!(succession_event.base.federation_did, federation_did);
        assert_eq!(succession_event.base.sequence_number, 1);
        assert_eq!(succession_event.base.previous_event_cid, None);
        assert_eq!(succession_event.signers_to_add.len(), 1);
        assert_eq!(succession_event.signers_to_add[0].did, "did:key:z6MkNewSigner456");
        assert_eq!(succession_event.signers_to_remove.len(), 1);
        assert_eq!(succession_event.signers_to_remove[0], signer_to_remove);
        assert!(succession_event.updated_quorum_config.is_some());
    }
    
    #[tokio::test]
    async fn test_disaster_recovery() {
        // 1. Initialize test quorum
        let test_quorum = initialization::initialize_test_quorum(3, QuorumType::Majority).await.unwrap();
        
        // 2. Set up federation DIDs
        let original_federation_did = "did:key:z6MkCompromisedFederation".to_string();
        let new_federation_did = "did:key:z6MkReconstitutedFederation".to_string();
        
        // 3. Create new signers for reconstitution
        let new_signers = vec![
            SerializableSigner {
                did: "did:key:z6MkNewSigner1".to_string(),
                public_key: "base64encodedpublickey1".to_string(),
            },
            SerializableSigner {
                did: "did:key:z6MkNewSigner2".to_string(),
                public_key: "base64encodedpublickey2".to_string(),
            },
            SerializableSigner {
                did: "did:key:z6MkNewSigner3".to_string(),
                public_key: "base64encodedpublickey3".to_string(),
            },
        ];
        
        // 4. Create a high-security quorum config for the new federation
        let new_quorum_config = SignerQuorumConfig::new_threshold(
            new_signers.iter().map(|s| s.did.clone()).collect(),
            80 // 80% threshold
        );
        
        // 5. Create external attestations
        let statement = format!(
            "We attest that the federation {} has been compromised and is being reconstituted as {} with legitimate succession.",
            original_federation_did, new_federation_did
        );
        
        let external_attestation = ExternalAttestation {
            attester_did: "did:key:z6MkTrustedAttestor".to_string(),
            timestamp: Utc::now(),
            statement,
            signature: Signature(vec![10, 11, 12]), // Test signature
        };
        
        // 6. Create a justification
        let justification = "Federation key material was compromised in a security breach. This reconstitution follows the disaster recovery protocol.";
        
        // 7. Create disaster recovery anchor
        let recovery_result = recovery::create_disaster_recovery_anchor(
            &original_federation_did,
            &new_federation_did,
            1, // First event in new chain
            new_signers.clone(),
            new_quorum_config.clone(),
            justification.to_string(),
            vec![external_attestation],
        ).await;
        
        assert!(recovery_result.is_ok(), "Failed to create disaster recovery anchor: {:?}", recovery_result.err());
        
        let recovery_anchor = recovery_result.unwrap();
        
        // 8. Verify the recovery anchor fields
        assert_eq!(recovery_anchor.base.event_type, RecoveryEventType::DisasterRecovery);
        assert_eq!(recovery_anchor.base.federation_did, original_federation_did);
        assert_eq!(recovery_anchor.base.sequence_number, 1);
        assert_eq!(recovery_anchor.new_federation_did, new_federation_did);
        assert_eq!(recovery_anchor.new_signers.len(), new_signers.len());
        assert_eq!(recovery_anchor.external_attestations.len(), 1);
        assert_eq!(recovery_anchor.justification, justification);
    }
    
    #[tokio::test]
    async fn test_metadata_update() {
        // 1. Initialize test quorum
        let test_quorum = initialization::initialize_test_quorum(3, QuorumType::Majority).await.unwrap();
        
        // 2. Create federation DID
        let federation_did = "did:key:z6MkFederation123".to_string();
        
        // 3. Create test signatures
        let test_signatures = vec![
            (IdentityId("did:key:test1".to_string()), Signature(vec![1, 2, 3])),
            (IdentityId("did:key:test2".to_string()), Signature(vec![4, 5, 6])),
        ];
        
        // 4. Create test metadata
        let test_metadata = FederationMetadata {
            federation_did: federation_did.clone(),
            name: "Updated Federation Name".to_string(),
            description: Some("This federation has updated metadata".to_string()),
            created_at: Utc::now(),
            initial_policies: vec![],
            initial_members: vec![],
            signer_quorum: test_quorum.clone(),
            genesis_cid: cid::Cid::default(),
            additional_metadata: None,
        };
        
        // 5. Create metadata update event
        let update_result = recovery::create_metadata_update_event(
            &federation_did,
            1, // First event
            None, // No previous event
            test_metadata.clone(),
            test_signatures.clone(),
        ).await;
        
        assert!(update_result.is_ok(), "Failed to create metadata update event: {:?}", update_result.err());
        
        let update_event = update_result.unwrap();
        
        // 6. Verify the event fields
        assert_eq!(update_event.base.event_type, RecoveryEventType::MetadataUpdate);
        assert_eq!(update_event.base.federation_did, federation_did);
        assert_eq!(update_event.base.sequence_number, 1);
        assert_eq!(update_event.base.previous_event_cid, None);
        assert_eq!(update_event.updated_metadata.name, "Updated Federation Name");
    }
} 