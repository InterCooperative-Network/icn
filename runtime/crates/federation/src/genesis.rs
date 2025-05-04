use serde::{Deserialize, Serialize};
use cid::Cid;
use chrono::{DateTime, Utc};
use icn_identity::{IdentityId, TrustBundle, VerifiableCredential, Signature};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use crate::error::{FederationResult, FederationError};
use crate::guardian::{GuardianQuorumConfig, Guardian};
use crate::guardian::decisions;

/// Metadata about a federation entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMetadata {
    /// The DID of the federation
    pub federation_did: String,
    
    /// Human-readable name of the federation
    pub name: String,
    
    /// Description of this federation's purpose
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    
    /// When the federation was established
    pub created_at: DateTime<Utc>,
    
    /// Initial policies of the federation
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_policies: Vec<VerifiableCredential>,
    
    /// Initial members of the federation (DIDs)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_members: Vec<String>,
    
    /// Guardian quorum configuration for this federation
    pub guardian_quorum: GuardianQuorumConfig,
    
    /// Genesis DAG CID where this federation was anchored
    pub genesis_cid: Cid,
    
    /// Additional metadata fields as arbitrary JSON
    /// Can include initial policies and members
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_metadata: Option<serde_json::Value>,
}

/// Represents a Federation Establishment Credential (FEC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationEstablishmentCredential {
    /// The federation metadata
    pub metadata: FederationMetadata,
    
    /// The current epoch (initially 0)
    pub epoch: u64,
    
    /// Guardian signatures attesting to the legitimacy of this federation
    pub guardian_signatures: Vec<(IdentityId, String)>,
}

/// Functions for federation bootstrap
pub mod bootstrap {
    use super::*;
    use icn_identity::{
        generate_did_key, VerifiableCredential, QuorumProof, verify_signature
    };
    use crate::guardian::initialization;
    use sha2::{Digest, Sha256};
    
    /// Initialize a new federation with the given guardians and configuration
    pub async fn initialize_federation(
        name: String,
        description: Option<String>,
        guardians: &[Guardian],
        quorum_config: GuardianQuorumConfig,
        initial_policies: Vec<VerifiableCredential>,
        initial_members: Vec<IdentityId>,
    ) -> FederationResult<(FederationMetadata, FederationEstablishmentCredential, TrustBundle)> {
        // 1. Generate a new DID for the federation
        let (federation_did_str, _federation_jwk) = generate_did_key().await
            .map_err(|e| FederationError::BootstrapError(format!("Failed to generate federation DID: {}", e)))?;
        
        // Extract member DIDs as strings
        let member_dids = initial_members.iter()
            .map(|id| id.0.clone())
            .collect();
        
        // 2. Create the federation metadata
        let federation_metadata = FederationMetadata {
            federation_did: federation_did_str.clone(),
            name,
            description,
            created_at: Utc::now(),
            initial_policies,
            initial_members: member_dids,
            guardian_quorum: quorum_config.clone(),
            genesis_cid: Cid::default(), // Will be set later in Phase 4
            additional_metadata: None,
        };
        
        // 3. Serialize the federation metadata for signing
        let metadata_bytes = serde_json::to_vec(&federation_metadata)
            .map_err(|e| FederationError::SerializationError(format!("Failed to serialize federation metadata: {}", e)))?;
        
        // 4. Create a quorum proof for the federation metadata
        let quorum_proof = decisions::create_quorum_proof(
            &metadata_bytes,
            guardians,
            &quorum_config,
        ).await?;
        
        // 5. Create the establishment credential
        let guardian_signatures = quorum_proof.votes.iter()
            .map(|(id, sig)| (id.clone(), URL_SAFE_NO_PAD.encode(&sig.0)))
            .collect();
        
        let establishment_credential = FederationEstablishmentCredential {
            metadata: federation_metadata.clone(),
            epoch: 0, // Initial epoch
            guardian_signatures,
        };
        
        // 6. Create guardian credentials
        let mut guardian_credentials = Vec::new();
        for guardian in guardians {
            if let Some(credential) = &guardian.credential {
                guardian_credentials.push(credential.credential.clone());
            }
        }
        
        // 7. Create the trust bundle
        let trust_bundle = create_trust_bundle(
            &federation_metadata,
            guardians,
            guardian_credentials,
        ).await?;
        
        Ok((federation_metadata, establishment_credential, trust_bundle))
    }
    
    /// Create a trust bundle from federation metadata and credentials
    pub async fn create_trust_bundle(
        metadata: &FederationMetadata,
        guardians: &[Guardian],
        mut credentials: Vec<VerifiableCredential>,
    ) -> FederationResult<TrustBundle> {
        // Serialize the metadata for signing
        let metadata_bytes = serde_json::to_vec(metadata)
            .map_err(|e| FederationError::SerializationError(format!("Failed to serialize metadata: {}", e)))?;
        
        // Create a quorum proof for the metadata
        let quorum_proof = decisions::create_quorum_proof(
            &metadata_bytes,
            guardians,
            &metadata.guardian_quorum,
        ).await?;
        
        // If no credentials provided, create a federation establishment credential
        if credentials.is_empty() {
            // Create a establishment credential
            let federation_vc = VerifiableCredential::new(
                vec!["VerifiableCredential".to_string(), "FederationEstablishmentCredential".to_string()],
                &IdentityId(metadata.federation_did.clone()),
                &IdentityId(metadata.federation_did.clone()),
                serde_json::json!({
                    "name": metadata.name,
                    "federation_did": metadata.federation_did,
                    "created_at": metadata.created_at.to_rfc3339()
                }),
            );
            
            credentials.push(federation_vc);
        }
        
        // Create the trust bundle
        let mut trust_bundle = TrustBundle::new(
            0, // Initial epoch
            metadata.federation_did.clone(),
            Vec::new(), // No DAG roots yet (will be added in Phase 4)
            credentials,
        );
        
        // Add the quorum proof
        trust_bundle.proof = Some(quorum_proof);
        
        Ok(trust_bundle)
    }
    
    /// Calculate a reproducible hash for the federation metadata
    pub fn calculate_metadata_hash(metadata: &FederationMetadata) -> [u8; 32] {
        let mut hasher = Sha256::new();
        
        // Hash the metadata in a deterministic order
        hasher.update(metadata.federation_did.as_bytes());
        hasher.update(metadata.name.as_bytes());
        
        if let Some(desc) = &metadata.description {
            hasher.update(desc.as_bytes());
        }
        
        // Hash the created_at timestamp
        let created_at_str = metadata.created_at.to_rfc3339();
        hasher.update(created_at_str.as_bytes());
        
        // Hash the initial policies (in order)
        for policy in &metadata.initial_policies {
            if let Ok(policy_bytes) = serde_json::to_vec(policy) {
                hasher.update(&policy_bytes);
            }
        }
        
        // Hash the initial members (in order)
        for member in &metadata.initial_members {
            hasher.update(member.as_bytes());
        }
        
        // Hash the guardian quorum configuration
        let quorum_bytes = serde_json::to_vec(&metadata.guardian_quorum).unwrap_or_default();
        hasher.update(&quorum_bytes);
        
        // Hash the genesis CID
        hasher.update(metadata.genesis_cid.to_bytes());
        
        // Hash the additional metadata if present
        if let Some(additional) = &metadata.additional_metadata {
            let additional_bytes = serde_json::to_vec(additional).unwrap_or_default();
            hasher.update(&additional_bytes);
        }
        
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardian::initialization;
    use crate::guardian::QuorumType;
    use icn_identity::verify_signature;
    use chrono::DateTime;
    use base64::Engine;
    
    #[tokio::test]
    async fn test_federation_creation_with_valid_quorum() {
        // Create a set of guardians with a majority quorum
        let (guardians, quorum_config) = initialization::initialize_guardian_set(3, QuorumType::Majority).await.unwrap();
        
        // Create a federation with these guardians
        let name = "Test Federation".to_string();
        let description = Some("A federation for testing".to_string());
        let initial_policies = Vec::new(); // No initial policies for this test
        let initial_members = Vec::new(); // No initial members for this test
        
        let result = bootstrap::initialize_federation(
            name.clone(),
            description.clone(),
            &guardians,
            quorum_config,
            initial_policies,
            initial_members,
        ).await;
        
        assert!(result.is_ok(), "Federation creation failed: {:?}", result.err());
        
        let (metadata, establishment_credential, trust_bundle) = result.unwrap();
        
        // Check the federation metadata
        assert_eq!(metadata.name, name);
        assert_eq!(metadata.description, description);
        assert!(metadata.created_at <= Utc::now());
        
        // Check the establishment credential
        assert_eq!(establishment_credential.metadata.name, name);
        assert_eq!(establishment_credential.epoch, 0);
        assert!(!establishment_credential.guardian_signatures.is_empty());
        
        // Check the trust bundle
        assert_eq!(trust_bundle.epoch_id, 0);
        assert_eq!(trust_bundle.federation_id, metadata.federation_did);
        assert!(!trust_bundle.attestations.is_empty());
        assert!(trust_bundle.proof.is_some());
    }
    
    #[tokio::test]
    async fn test_establishment_credential_signature_verification() {
        // Create a set of guardians with a majority quorum
        let (guardians, quorum_config) = initialization::initialize_guardian_set(3, QuorumType::Majority).await.unwrap();
        
        // Create federation and get the establishment credential
        let result = bootstrap::initialize_federation(
            "Test Federation".to_string(),
            Some("A federation for testing".to_string()),
            &guardians,
            quorum_config.clone(),
            Vec::new(),
            Vec::new(),
        ).await.unwrap();
        
        let (metadata, establishment_credential, _) = result;
        
        // Serialize the metadata to verify the signatures
        let metadata_bytes = serde_json::to_vec(&metadata).unwrap();
        
        // Verify that the signatures in the establishment credential are valid
        // This test may require adaptation based on how signatures are generated and verified
        let verified_signatures = establishment_credential.guardian_signatures.iter()
            .filter_map(|(guardian_did, signature_b64)| {
                // Decode the signature
                match URL_SAFE_NO_PAD.decode(signature_b64) {
                    Ok(signature_bytes) => {
                        let signature = Signature(signature_bytes);
                        // Verify the signature
                        match verify_signature(&metadata_bytes, &signature, guardian_did) {
                            Ok(valid) => Some(valid),
                            Err(_) => None,
                        }
                    },
                    Err(_) => None,
                }
            })
            .filter(|valid| *valid)
            .count();
            
        // We should have at least one valid signature
        assert!(verified_signatures > 0, "No valid signatures found in establishment credential");
    }
    
    #[tokio::test]
    async fn test_federation_metadata_cid_reproducibility() {
        // Create a metadata object with fixed values
        let metadata = FederationMetadata {
            federation_did: "did:key:z6MkTest123".to_string(),
            name: "Test Federation".to_string(),
            description: Some("A federation for testing".to_string()),
            created_at: DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z").unwrap().with_timezone(&Utc),
            initial_policies: Vec::new(),
            initial_members: vec![
                "did:key:z6MkMember1".to_string(),
                "did:key:z6MkMember2".to_string(),
            ],
            guardian_quorum: GuardianQuorumConfig::new_majority(vec![
                "did:key:z6MkGuardian1".to_string(),
                "did:key:z6MkGuardian2".to_string(),
                "did:key:z6MkGuardian3".to_string(),
            ]),
            genesis_cid: Cid::default(),
            additional_metadata: None,
        };
        
        // Calculate the hash twice
        let hash1 = bootstrap::calculate_metadata_hash(&metadata);
        let hash2 = bootstrap::calculate_metadata_hash(&metadata);
        
        // The hashes should be identical for identical metadata
        assert_eq!(hash1, hash2, "Metadata hashes should be the same for identical metadata");
        
        // Make a copy with a different field
        let mut metadata2 = metadata.clone();
        metadata2.name = "Different Federation".to_string();
        
        // Calculate the hash for the modified metadata
        let hash3 = bootstrap::calculate_metadata_hash(&metadata2);
        
        // The hash should be different
        assert_ne!(hash1, hash3, "Metadata hashes should be different for different metadata");
        
        // Make a copy with different members (order matters)
        let mut metadata3 = metadata.clone();
        metadata3.initial_members = vec![
            "did:key:z6MkMember2".to_string(),
            "did:key:z6MkMember1".to_string(),
        ];
        
        // Calculate the hash for the metadata with different member order
        let hash4 = bootstrap::calculate_metadata_hash(&metadata3);
        
        // The hash should be different
        assert_ne!(hash1, hash4, "Metadata hashes should be different when member order changes");
    }
} 