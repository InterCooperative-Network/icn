use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use icn_identity::{Signature, KeyPair};
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
    /// Signer succession event (add/remove/replace)
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

/// Serializable version of Signer for recovery events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableSigner {
    /// Signer DID
    pub did: String,
    /// Signer Public Key (base64 encoded)
    pub public_key: String,
}

// Define a generic Signer type for compatibility
pub type Signer = icn_identity::IdentityId;

impl From<&Signer> for SerializableSigner {
    fn from(signer: &Signer) -> Self {
        Self {
            did: signer.0.clone(),
            // In a real implementation, we would need a method to expose the public key safely
            public_key: "placeholder_key".to_string(),
        }
    }
}

/// Succession event for adding, removing, or replacing signers
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
    /// New guardian set (serializable version)
    pub new_guardians: Vec<SerializableGuardian>,
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
        guardians: &[Guardian],
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
            signatures: vec![],  // Will be filled after quorum signing
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
        
        // Collect guardian signatures through quorum process
        // This would use the quorum mechanism from guardian.rs
        // let canonical_representation = serde_json::to_string(&rotation_event)?;
        // rotation_event.base.signatures = collect_quorum_signatures(guardians, quorum_config, canonical_representation)?;
        
        Ok(rotation_event)
    }
    
    /// Create a guardian succession event
    pub async fn create_guardian_succession_event(
        federation_did: &str,
        sequence_number: u64,
        previous_event_cid: Option<String>,
        guardians_to_add: Vec<Guardian>,
        guardians_to_remove: Vec<String>,
        updated_quorum_config: Option<SignerQuorumConfig>,
        current_guardians: &[Guardian],
        current_quorum_config: &SignerQuorumConfig,
    ) -> FederationResult<SuccessionEvent> {
        // Create base recovery event
        let base = RecoveryEvent {
            event_type: RecoveryEventType::Succession,
            federation_did: federation_did.to_string(),
            sequence_number,
            previous_event_cid,
            timestamp: Utc::now(),
            signatures: vec![],  // Will be filled after quorum signing
        };
        
        // Convert Guardian objects to SerializableGuardian
        let serializable_guardians = guardians_to_add.iter()
            .map(|g| SerializableGuardian::from(g))
            .collect();
        
        // Create the guardian succession event
        let succession_event = SuccessionEvent {
            base,
            guardians_to_add: serializable_guardians,
            guardians_to_remove,
            updated_quorum_config,
        };
        
        // Here we would collect signatures from the current guardians
        // succession_event.base.signatures = collect_quorum_signatures(current_guardians, current_quorum_config, ...);
        
        Ok(succession_event)
    }
    
    /// Create a disaster recovery anchor
    pub async fn create_disaster_recovery_anchor(
        federation_did: &str,
        new_federation_did: &str,
        sequence_number: u64,
        new_guardians: Vec<Guardian>,
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
            signatures: vec![],  // Will be filled with signatures from new guardians
        };
        
        // Convert Guardian objects to SerializableGuardian
        let serializable_guardians = new_guardians.iter()
            .map(|g| SerializableGuardian::from(g))
            .collect();
        
        // Create the disaster recovery anchor
        let recovery_anchor = DisasterRecoveryAnchor {
            base,
            new_federation_did: new_federation_did.to_string(),
            new_guardians: serializable_guardians,
            new_quorum_config,
            justification,
            external_attestations,
        };
        
        // Here we would collect signatures from the new guardians
        // recovery_anchor.base.signatures = collect_signatures_from_new_guardians(...);
        
        Ok(recovery_anchor)
    }
    
    /// Create a metadata update event
    pub async fn create_metadata_update_event(
        federation_did: &str,
        sequence_number: u64,
        previous_event_cid: Option<String>,
        updated_metadata: FederationMetadata,
        current_guardians: &[Guardian],
        current_quorum_config: &SignerQuorumConfig,
    ) -> FederationResult<MetadataUpdateEvent> {
        // Create base recovery event
        let base = RecoveryEvent {
            event_type: RecoveryEventType::MetadataUpdate,
            federation_did: federation_did.to_string(),
            sequence_number,
            previous_event_cid,
            timestamp: Utc::now(),
            signatures: vec![],  // Will be filled after quorum signing
        };
        
        // Create the metadata update event
        let metadata_event = MetadataUpdateEvent {
            base,
            updated_metadata,
        };
        
        // Here we would collect signatures from current guardians
        // metadata_event.base.signatures = collect_quorum_signatures(current_guardians, current_quorum_config, ...);
        
        Ok(metadata_event)
    }
    
    /// Verify a recovery event's signatures against a list of guardians and quorum config
    pub async fn verify_recovery_event(
        event: &RecoveryEvent,
        guardians: &[Guardian],
        quorum_config: &SignerQuorumConfig,
    ) -> FederationResult<bool> {
        // Implementation would verify signatures against guardians and quorum
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
    use crate::guardian::{initialization, QuorumType};
    
    #[tokio::test]
    async fn test_key_rotation() {
        // 1. Set up guardians and federation
        let (guardians, quorum_config) = initialization::initialize_guardian_set(3, QuorumType::Majority).await.unwrap();
        
        // 2. Create federation DID and keypair
        let federation_did = "did:key:z6MkFederation123".to_string();
        let federation_keypair = KeyPair::new(vec![1, 2, 3, 4], vec![5, 6, 7, 8, 9]); // Simplified for testing
        
        // 3. Create a new keypair for rotation
        let new_federation_keypair = KeyPair::new(vec![9, 8, 7, 6], vec![5, 4, 3, 2, 1]); // Simplified for testing
        let new_federation_did = "did:key:z6MkFederationNew456".to_string();
        
        // 4. Create key rotation event
        let key_rotation_result = recovery::create_key_rotation_event(
            &federation_did,
            &new_federation_keypair,
            1, // First event
            None, // No previous event
            &guardians,
            &quorum_config,
        ).await;
        
        assert!(key_rotation_result.is_ok(), "Failed to create key rotation event: {:?}", key_rotation_result.err());
        
        let key_rotation_event = key_rotation_result.unwrap();
        
        // 5. Verify the event fields
        assert_eq!(key_rotation_event.base.event_type, RecoveryEventType::FederationKeyRotation);
        assert_eq!(key_rotation_event.base.federation_did, federation_did);
        assert_eq!(key_rotation_event.base.sequence_number, 1);
        assert_eq!(key_rotation_event.base.previous_event_cid, None);
        
        // 6. In a real implementation, we would now:
        // - Verify the key_proof signature using the new federation key
        // - Verify the guardian signatures (not implemented in skeleton)
        // - Anchor the event to the DAG for persistence
        
        // 7. Create a mock anchor CID for this event
        let anchor_cid = "bafykeyfederation1".to_string();
        
        // 8. Create a subsequent event with the new federation DID
        let subsequent_event_result = recovery::create_metadata_update_event(
            &new_federation_did, // Using the new federation DID
            2, // Second event
            Some(anchor_cid), // Previous event CID
            FederationMetadata {
                federation_did: new_federation_did.clone(),
                name: "Updated Federation".to_string(),
                description: Some("This federation has a rotated key".to_string()),
                created_at: Utc::now(),
                initial_policies: vec![],
                initial_members: vec![],
            },
            &guardians,
            &quorum_config,
        ).await;
        
        assert!(subsequent_event_result.is_ok(), "Failed to create subsequent event: {:?}", subsequent_event_result.err());
        
        let subsequent_event = subsequent_event_result.unwrap();
        
        // 9. Verify the subsequent event fields
        assert_eq!(subsequent_event.base.event_type, RecoveryEventType::MetadataUpdate);
        assert_eq!(subsequent_event.base.federation_did, new_federation_did);
        assert_eq!(subsequent_event.base.sequence_number, 2);
        assert_eq!(subsequent_event.base.previous_event_cid, Some(anchor_cid));
        assert_eq!(subsequent_event.updated_metadata.federation_did, new_federation_did);
        
        // 10. In a real implementation, we would now:
        // - Verify the guardian signatures on the subsequent event
        // - Anchor the subsequent event to the DAG
        // - Update the federation's active keypair in the system
    }
    
    #[tokio::test]
    async fn test_guardian_succession() {
        // 1. Set up initial guardians and federation
        let (initial_guardians, initial_quorum_config) = initialization::initialize_guardian_set(
            3, 
            QuorumType::Majority
        ).await.unwrap();
        
        // 2. Create federation DID 
        let federation_did = "did:key:z6MkFederation123".to_string();
        
        // 3. Generate a new guardian to add - using proper Guardian initialization
        let (new_guardian, _) = initialization::generate_guardian().await.unwrap();
        
        // 4. Determine which guardian to remove (we need to extract the String DID)
        let guardian_to_remove = initial_guardians[0].did.0.clone();
        
        // 5. Get guardian DIDs for updated quorum config
        let guardian_dids = vec![
            initial_guardians[1].did.0.clone(),
            initial_guardians[2].did.0.clone(),
            new_guardian.did.0.clone(),
        ];
        
        // Create a new quorum configuration with higher threshold
        let updated_quorum_config = SignerQuorumConfig::new(
            QuorumType::Threshold(75), // 75% threshold instead of majority
            guardian_dids,
        );
        
        // 6. Create guardian succession event
        let succession_result = recovery::create_guardian_succession_event(
            &federation_did,
            1, // First event
            None, // No previous event
            vec![new_guardian.clone()], // Add new guardian
            vec![guardian_to_remove.clone()], // Remove first guardian
            Some(updated_quorum_config.clone()), // Update quorum configuration
            &initial_guardians,
            &initial_quorum_config,
        ).await;
        
        assert!(succession_result.is_ok(), "Failed to create guardian succession event: {:?}", succession_result.err());
        
        let succession_event = succession_result.unwrap();
        
        // 7. Verify the event fields
        assert_eq!(succession_event.base.event_type, RecoveryEventType::Succession);
        assert_eq!(succession_event.base.federation_did, federation_did);
        assert_eq!(succession_event.base.sequence_number, 1);
        assert_eq!(succession_event.base.previous_event_cid, None);
        assert_eq!(succession_event.signers_to_add.len(), 1);
        assert_eq!(succession_event.signers_to_add[0].did, new_guardian.did.0);
        assert_eq!(succession_event.signers_to_remove.len(), 1);
        assert_eq!(succession_event.signers_to_remove[0], guardian_to_remove);
        assert!(succession_event.updated_quorum_config.is_some());
        
        // 8. In a real implementation, we would now:
        // - Verify the guardian signatures match the required quorum from the initial configuration
        // - Anchor the event to the DAG for persistence
        
        // 9. Apply the changes to create the new guardian set
        let mut updated_guardians = initial_guardians.clone();
        
        // Remove the guardian
        updated_guardians.retain(|g| g.did.0 != guardian_to_remove);
        
        // Add the new guardian
        updated_guardians.push(new_guardian.clone());
        
        // 10. Verify the updated guardian set
        assert_eq!(updated_guardians.len(), 3); // Still 3 guardians (removed 1, added 1)
        assert!(updated_guardians.iter().any(|g| g.did.0 == new_guardian.did.0)); // New guardian is present
        assert!(!updated_guardians.iter().any(|g| g.did.0 == guardian_to_remove)); // Removed guardian is absent
        
        // 11. Create a mock anchor CID for this event
        let anchor_cid = "bafyguardiansuccession1".to_string();
        
        // 12. Create a subsequent event with the updated guardian set
        let subsequent_event_result = recovery::create_key_rotation_event(
            &federation_did,
            &KeyPair::new(vec![20, 21, 22, 23], vec![24, 25, 26, 27, 28]), // New federation key
            2, // Second event
            Some(anchor_cid), // Previous event CID
            &updated_guardians, // Using updated guardian set
            &updated_quorum_config, // Using updated quorum config
        ).await;
        
        assert!(subsequent_event_result.is_ok(), "Failed to create subsequent event: {:?}", subsequent_event_result.err());
    }
    
    #[tokio::test]
    async fn test_disaster_recovery() {
        // 1. Suppose we have a federation that has lost its keys or majority of guardians
        let original_federation_did = "did:key:z6MkCompromisedFederation".to_string();
        
        // 2. Create a completely new set of guardians for reconstitution
        let (new_guardians, new_quorum_config) = initialization::initialize_guardian_set(
            5, // More guardians for better security
            QuorumType::Threshold(80), // Higher threshold for enhanced security
        ).await.unwrap();
        
        // 3. Generate a new federation DID
        let new_federation_did = "did:key:z6MkReconstitutedFederation".to_string();
        
        // 4. Create external attestations from trusted third parties
        let trusted_attestor_keypair = KeyPair::new(vec![30, 31, 32, 33], vec![34, 35, 36, 37, 38]);
        let trusted_attestor_did = "did:key:z6MkTrustedAttestor".to_string();
        
        let statement = format!(
            "We attest that the federation {} has been compromised and is being reconstituted as {} with legitimate succession.",
            original_federation_did, new_federation_did
        );
        
        let attestation_signature = icn_identity::sign_message(
            statement.as_bytes(),
            &trusted_attestor_keypair,
        ).unwrap();
        
        let external_attestation = ExternalAttestation {
            attester_did: trusted_attestor_did,
            timestamp: Utc::now(),
            statement,
            signature: attestation_signature,
        };
        
        // 5. Create a justification for the disaster recovery
        let justification = "Federation key material was compromised in a security breach on 2023-04-15. \
                            This reconstitution follows the disaster recovery protocol established in the \
                            federation's governance documents section 7.3.";
        
        // 6. Create disaster recovery anchor
        let recovery_result = recovery::create_disaster_recovery_anchor(
            &original_federation_did,
            &new_federation_did,
            1, // First event in new chain
            new_guardians.clone(),
            new_quorum_config.clone(),
            justification.to_string(),
            vec![external_attestation],
        ).await;
        
        assert!(recovery_result.is_ok(), "Failed to create disaster recovery anchor: {:?}", recovery_result.err());
        
        let recovery_anchor = recovery_result.unwrap();
        
        // 7. Verify the recovery anchor fields
        assert_eq!(recovery_anchor.base.event_type, RecoveryEventType::DisasterRecovery);
        assert_eq!(recovery_anchor.base.federation_did, original_federation_did);
        assert_eq!(recovery_anchor.base.sequence_number, 1);
        assert_eq!(recovery_anchor.new_federation_did, new_federation_did);
        assert_eq!(recovery_anchor.new_guardians.len(), new_guardians.len());
        assert_eq!(recovery_anchor.external_attestations.len(), 1);
        assert_eq!(recovery_anchor.justification, justification);
        
        // 8. In a real implementation, we would now:
        // - Verify the external attestations (signatures from trusted parties)
        // - Collect signatures from the new guardians
        // - Anchor the recovery event to the DAG
        // - Establish a new trust bundle with the recovery event as proof of legitimacy
        
        // 9. Create a mock anchor CID for this recovery
        let recovery_anchor_cid = "bafydisasterrecovery1".to_string();
        
        // 10. Create a subsequent event with the new federation identity
        let subsequent_event_result = recovery::create_metadata_update_event(
            &new_federation_did, // Using the new federation DID
            2, // Second event in the new chain
            Some(recovery_anchor_cid), // Previous event CID
            FederationMetadata {
                federation_did: new_federation_did.clone(),
                name: "Reconstituted Federation".to_string(),
                description: Some("This federation was reconstituted after a security breach".to_string()),
                created_at: Utc::now(),
                initial_policies: vec![],
                initial_members: vec![],
            },
            &new_guardians,
            &new_quorum_config,
        ).await;
        
        assert!(subsequent_event_result.is_ok(), "Failed to create subsequent event: {:?}", subsequent_event_result.err());
        
        let subsequent_event = subsequent_event_result.unwrap();
        
        // 11. Verify the subsequent event fields
        assert_eq!(subsequent_event.base.event_type, RecoveryEventType::MetadataUpdate);
        assert_eq!(subsequent_event.base.federation_did, new_federation_did);
        assert_eq!(subsequent_event.base.sequence_number, 2);
        assert_eq!(subsequent_event.base.previous_event_cid, Some(recovery_anchor_cid));
    }
    
    #[tokio::test]
    async fn test_metadata_update() {
        // 1. Set up guardians and federation
        let (guardians, quorum_config) = initialization::initialize_guardian_set(3, QuorumType::Majority).await.unwrap();
        
        // 2. Create initial federation metadata
        let federation_did = "did:key:z6MkFederation123".to_string();
        let initial_metadata = FederationMetadata {
            federation_did: federation_did.clone(),
            name: "Original Federation".to_string(),
            description: Some("A federation for testing metadata updates".to_string()),
            created_at: Utc::now(),
            initial_policies: vec![],
            initial_members: vec![],
        };
        
        // 3. Create updated metadata with changes
        let updated_metadata = FederationMetadata {
            federation_did: federation_did.clone(),
            name: "Updated Federation Name".to_string(), // Changed name
            description: Some("This federation has been updated with new policies".to_string()), // Updated description
            created_at: initial_metadata.created_at, // Keep original creation time
            initial_policies: vec![], // Keep same policies
            initial_members: vec![], // Keep same members
        };
        
        // 4. Create metadata update event
        let update_result = recovery::create_metadata_update_event(
            &federation_did,
            1, // First event
            None, // No previous event
            updated_metadata.clone(),
            &guardians,
            &quorum_config,
        ).await;
        
        assert!(update_result.is_ok(), "Failed to create metadata update event: {:?}", update_result.err());
        
        let update_event = update_result.unwrap();
        
        // 5. Verify the event fields
        assert_eq!(update_event.base.event_type, RecoveryEventType::MetadataUpdate);
        assert_eq!(update_event.base.federation_did, federation_did);
        assert_eq!(update_event.base.sequence_number, 1);
        assert_eq!(update_event.base.previous_event_cid, None);
        assert_eq!(update_event.updated_metadata.name, "Updated Federation Name");
        
        // 6. In a real implementation, we would now:
        // - Verify the guardian signatures match the required quorum
        // - Anchor the event to the DAG for persistence
        
        // 7. Create a mock anchor CID for this event
        let anchor_cid = "bafymetadataupdate1".to_string();
        
        // 8. Create a second metadata update with additional changes
        // In the real code, we would need to update the quorum config separately
        // as part of the federation metadata
        let further_updated_metadata = FederationMetadata {
            federation_did: federation_did.clone(),
            name: "Further Updated Federation".to_string(), // Changed name again
            description: Some("This federation has been updated with additional changes".to_string()),
            created_at: initial_metadata.created_at, // Keep original creation time
            initial_policies: vec![], // Keep same policies
            initial_members: vec![], // Keep same members
        };
        
        // 9. Create second metadata update event
        let second_update_result = recovery::create_metadata_update_event(
            &federation_did,
            2, // Second event
            Some(anchor_cid), // Previous event CID
            further_updated_metadata.clone(),
            &guardians,
            &quorum_config, // Still using original quorum config for signatures
        ).await;
        
        assert!(second_update_result.is_ok(), "Failed to create second metadata update event: {:?}", second_update_result.err());
        
        let second_update_event = second_update_result.unwrap();
        
        // 10. Verify the second event fields
        assert_eq!(second_update_event.base.event_type, RecoveryEventType::MetadataUpdate);
        assert_eq!(second_update_event.base.federation_did, federation_did);
        assert_eq!(second_update_event.base.sequence_number, 2);
        assert_eq!(second_update_event.base.previous_event_cid, Some(anchor_cid));
        assert_eq!(second_update_event.updated_metadata.name, "Further Updated Federation");
        
        // 11. In a real implementation, we would now:
        // - Verify the guardian signatures match the required quorum from the original config
        // - Anchor the second event to the DAG
        // - Update the federation's active metadata in the system
    }
} 