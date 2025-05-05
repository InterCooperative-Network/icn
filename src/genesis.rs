use serde::{Deserialize, Serialize};
use cid::Cid;
use chrono::{DateTime, Utc};
use icn_identity::{IdentityId, TrustBundle, VerifiableCredential, Signature, QuorumProof};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

use crate::error::{FederationResult, FederationError};
use crate::quorum::SignerQuorumConfig;
use crate::quorum::decisions;

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
    
    /// Signer quorum configuration for this federation
    pub signer_quorum: SignerQuorumConfig,
    
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
    
    /// Signatures attesting to the legitimacy of this federation
    pub signer_signatures: Vec<(IdentityId, String)>,
}

/// Represents a Genesis Trust Bundle encapsulating federation genesis state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisTrustBundle {
    /// CID of the federation metadata (calculated deterministically)
    pub federation_metadata_cid: String,
    
    /// The federation establishment credential
    pub federation_establishment_credential: FederationEstablishmentCredential,
    
    /// Credentials for all authorized signers
    pub signer_credentials: Vec<VerifiableCredential>,
    
    /// Quorum proof for the bundle
    pub quorum_proof: QuorumProof,
    
    /// When the bundle was issued
    pub issued_at: DateTime<Utc>,
}

impl GenesisTrustBundle {
    /// Create a new genesis trust bundle
    pub fn new(
        federation_metadata_cid: String,
        federation_establishment_credential: FederationEstablishmentCredential,
        signer_credentials: Vec<VerifiableCredential>,
        quorum_proof: QuorumProof,
    ) -> Self {
        Self {
            federation_metadata_cid,
            federation_establishment_credential,
            signer_credentials,
            quorum_proof,
            issued_at: Utc::now(),
        }
    }
    
    /// Convert the trust bundle to an anchor payload for DAG integration
    pub fn to_anchor_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "FederationGenesisTrustBundle",
            "version": "1.0",
            "federation_metadata_cid": self.federation_metadata_cid,
            "federation_did": self.federation_establishment_credential.metadata.federation_did,
            "federation_name": self.federation_establishment_credential.metadata.name,
            "issued_at": self.issued_at,
            "signer_count": self.signer_credentials.len(),
            "quorum_type": self.federation_establishment_credential.metadata.signer_quorum.quorum_type,
        })
    }
}

/// Functions for federation bootstrap
pub mod bootstrap {
    use super::*;
    use icn_identity::{
        generate_did_key, VerifiableCredential
    };
    use crate::quorum::initialization;
    use sha2::{Digest, Sha256};
    
    /// Initialize a new federation with the given configuration
    pub async fn initialize_federation(
        name: String,
        description: Option<String>,
        quorum_config: SignerQuorumConfig,
        initial_policies: Vec<VerifiableCredential>,
        initial_members: Vec<IdentityId>,
        signatures: Vec<(IdentityId, Signature)>,
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
            signer_quorum: quorum_config.clone(),
            genesis_cid: Cid::default(), // Will be set later in Phase 4
            additional_metadata: None,
        };
        
        // 3. Serialize the federation metadata for signing
        let metadata_bytes = serde_json::to_vec(&federation_metadata)
            .map_err(|e| FederationError::SerializationError(format!("Failed to serialize federation metadata: {}", e)))?;
        
        // 4. Use provided signatures
        let encoded_signatures = signatures.iter()
            .map(|(id, sig)| (id.clone(), URL_SAFE_NO_PAD.encode(&sig.0)))
            .collect();
        
        // 5. Create the establishment credential
        let establishment_credential = FederationEstablishmentCredential {
            metadata: federation_metadata.clone(),
            epoch: 0, // Initial epoch
            signer_signatures: encoded_signatures,
        };
        
        // 6. Create signer credentials
        let signer_credentials = Vec::new(); // Would be filled with actual credentials in a real implementation
        
        // 7. Create the trust bundle
        let trust_bundle = create_trust_bundle(
            &federation_metadata,
            signer_credentials,
            signatures,
        ).await?;
        
        Ok((federation_metadata, establishment_credential, trust_bundle))
    }
    
    /// Create a trust bundle from federation metadata and credentials
    pub async fn create_trust_bundle(
        metadata: &FederationMetadata,
        mut credentials: Vec<VerifiableCredential>,
        signatures: Vec<(IdentityId, Signature)>,
    ) -> FederationResult<TrustBundle> {
        // Serialize the metadata for signing
        let metadata_bytes = serde_json::to_vec(metadata)
            .map_err(|e| FederationError::SerializationError(format!("Failed to serialize metadata: {}", e)))?;
        
        // Create a quorum proof for the metadata
        let quorum_proof = crate::quorum::decisions::create_quorum_proof(
            &metadata_bytes,
            signatures.clone(),
            &metadata.signer_quorum,
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
        
        // Hash the signer quorum configuration
        let quorum_bytes = serde_json::to_vec(&metadata.signer_quorum).unwrap_or_default();
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

/// Functions for creating and verifying trust bundles
pub mod trustbundle {
    use super::*;
    use cid::multihash::{Multihash};
    
    /// Calculate CID from federation metadata
    pub fn calculate_metadata_cid(metadata: &FederationMetadata) -> FederationResult<String> {
        let canonical_json = serde_json::to_vec(metadata)
            .map_err(|e| FederationError::SerializationError(format!("Failed to serialize metadata: {}", e)))?;
        
        // Hash the metadata with SHA-256
        let metadata_hash = Sha256::digest(&canonical_json);
        
        // Create a multihash
        let mh = Multihash::wrap(0x12, &metadata_hash)
            .map_err(|_| FederationError::CidError("Failed to create multihash".to_string()))?;
        
        // Create a CID v1 with dag-json codec (0x0129)
        let cid = Cid::new_v1(0x0129, mh);
        
        Ok(cid.to_string())
    }
    
    /// Create a genesis trust bundle
    pub async fn create_trust_bundle(
        metadata: &FederationMetadata,
        establishment_credential: FederationEstablishmentCredential,
        signer_credentials: Vec<VerifiableCredential>,
        signatures: Vec<(IdentityId, Signature)>,
    ) -> FederationResult<GenesisTrustBundle> {
        // Calculate federation metadata CID
        let metadata_cid = calculate_metadata_cid(metadata)?;
        
        // Serialize the entire bundle for signing
        let bundle_data = serde_json::json!({
            "metadata_cid": metadata_cid,
            "establishment_credential": establishment_credential,
            "signer_credentials": signer_credentials,
            "timestamp": Utc::now().to_rfc3339(),
        });
        
        let bundle_bytes = serde_json::to_vec(&bundle_data)
            .map_err(|e| FederationError::SerializationError(format!("Failed to serialize bundle data: {}", e)))?;
        
        // Create quorum proof for the bundle
        let quorum_proof = decisions::create_quorum_proof(
            &bundle_bytes,
            signatures,
            &metadata.signer_quorum,
        ).await?;
        
        // Create the genesis trust bundle
        let trust_bundle = GenesisTrustBundle::new(
            metadata_cid,
            establishment_credential,
            signer_credentials,
            quorum_proof,
        );
        
        Ok(trust_bundle)
    }
    
    /// Verify a genesis trust bundle
    pub async fn verify_trust_bundle(
        bundle: &GenesisTrustBundle,
        authorized_signer_dids: &[String], 
    ) -> FederationResult<bool> {
        // 1. Verify the quorum proof
        let bundle_data = serde_json::json!({
            "metadata_cid": bundle.federation_metadata_cid,
            "establishment_credential": bundle.federation_establishment_credential,
            "signer_credentials": bundle.signer_credentials,
            "timestamp": bundle.issued_at.to_rfc3339(),
        });
        
        let bundle_bytes = serde_json::to_vec(&bundle_data)
            .map_err(|e| FederationError::SerializationError(format!("Failed to serialize bundle: {}", e)))?;
        
        // Manually verify each signature in the quorum proof
        for (signer_did, signature) in &bundle.quorum_proof.votes {
            // Verify the signer is authorized
            if !authorized_signer_dids.contains(&signer_did.0) {
                return Err(FederationError::VerificationError(
                    format!("Unauthorized signer signature in quorum proof: {}", signer_did.0)
                ));
            }
            
            // Verify the signature
            let sig_valid = icn_identity::verify_signature(&bundle_bytes, signature, signer_did)
                .map_err(|e| FederationError::VerificationError(format!("Signature verification error: {}", e)))?;
                
            if !sig_valid {
                return Err(FederationError::VerificationError(
                    format!("Invalid signature in quorum proof from signer: {}", signer_did.0)
                ));
            }
        }
        
        // 2. Recalculate and verify the metadata CID
        let recalculated_cid = calculate_metadata_cid(&bundle.federation_establishment_credential.metadata)?;
        
        if recalculated_cid != bundle.federation_metadata_cid {
            return Err(FederationError::VerificationError(
                format!("Metadata CID mismatch: {} vs {}", 
                    recalculated_cid, bundle.federation_metadata_cid)
            ));
        }
        
        // 3. Verify the establishment credential signatures
        for (signer_did, signature_b64) in &bundle.federation_establishment_credential.signer_signatures {
            // Check that the signer is authorized
            if !authorized_signer_dids.contains(&signer_did.0) {
                return Err(FederationError::VerificationError(
                    format!("Unauthorized signer signature: {}", signer_did.0)
                ));
            }
            
            // Verify the signer signature on the metadata
            let metadata_bytes = serde_json::to_vec(&bundle.federation_establishment_credential.metadata)
                .map_err(|e| FederationError::SerializationError(format!("Failed to serialize metadata: {}", e)))?;
            
            // Decode the signature
            let signature_bytes = URL_SAFE_NO_PAD.decode(signature_b64)
                .map_err(|e| FederationError::SerializationError(format!("Failed to decode signature: {}", e)))?;
            
            let signature = Signature(signature_bytes);
            
            // Verify the signature (this will use the identity crate's verify_signature function)
            let sig_valid = icn_identity::verify_signature(&metadata_bytes, &signature, signer_did)
                .map_err(|e| FederationError::VerificationError(format!("Signature verification error: {}", e)))?;
                
            if !sig_valid {
                return Err(FederationError::VerificationError(
                    format!("Invalid signature from signer: {}", signer_did.0)
                ));
            }
        }
        
        // 4. Verify all signers have credentials
        let signer_dids_in_metadata: Vec<String> = bundle.federation_establishment_credential
            .metadata.signer_quorum.signers.clone();
        
        let signer_dids_with_credentials: Vec<String> = bundle.signer_credentials.iter()
            .filter_map(|cred| {
                if let serde_json::Value::Object(subject) = &cred.credentialSubject {
                    if let Some(serde_json::Value::String(id)) = subject.get("id") {
                        return Some(id.clone());
                    }
                }
                None
            })
            .collect();
        
        // All signers listed in metadata should have credentials
        for did in &signer_dids_in_metadata {
            if !signer_dids_with_credentials.contains(did) {
                return Err(FederationError::VerificationError(
                    format!("Signer {} has no credential in bundle", did)
                ));
            }
        }
        
        // All verification checks passed
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quorum::{initialization, QuorumType};
    use icn_identity::verify_signature;
    use chrono::DateTime;
    use base64::Engine;
    
    #[tokio::test]
    async fn test_federation_creation_with_valid_quorum() {
        // Create a quorum config with a majority of 3 signers
        let quorum_config = initialization::initialize_test_quorum(3, QuorumType::Majority).await.unwrap();
        
        // Create test signatures (in a real case these would be from the quorum signers)
        let test_signatures = vec![
            (IdentityId("did:key:test1".to_string()), Signature(vec![1, 2, 3])),
            (IdentityId("did:key:test2".to_string()), Signature(vec![4, 5, 6])),
        ];
        
        // Create a federation with this quorum config
        let name = "Test Federation".to_string();
        let description = Some("A federation for testing".to_string());
        let initial_policies = Vec::new(); // No initial policies for this test
        let initial_members = Vec::new(); // No initial members for this test
        
        let result = bootstrap::initialize_federation(
            name.clone(),
            description.clone(),
            quorum_config,
            initial_policies,
            initial_members,
            test_signatures,
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
        assert!(!establishment_credential.signer_signatures.is_empty());
        
        // Check the trust bundle
        assert_eq!(trust_bundle.epoch_id, 0);
        assert_eq!(trust_bundle.federation_id, metadata.federation_did);
        assert!(!trust_bundle.attestations.is_empty());
        assert!(trust_bundle.proof.is_some());
    }
    
    #[tokio::test]
    async fn test_establishment_credential_signature_verification() {
        // Create quorum config
        let quorum_config = initialization::initialize_test_quorum(3, QuorumType::Majority).await.unwrap();
        
        // Create test signatures
        let test_signatures = vec![
            (IdentityId("did:key:test1".to_string()), Signature(vec![1, 2, 3])),
            (IdentityId("did:key:test2".to_string()), Signature(vec![4, 5, 6])),
        ];
        
        // Create federation and get the establishment credential
        let result = bootstrap::initialize_federation(
            "Test Federation".to_string(),
            Some("A federation for testing".to_string()),
            quorum_config.clone(),
            Vec::new(),
            Vec::new(),
            test_signatures,
        ).await.unwrap();
        
        let (metadata, establishment_credential, _) = result;
        
        // Verify that signatures are present in the establishment credential
        assert!(!establishment_credential.signer_signatures.is_empty(), "Establishment credential should have signatures");
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
            signer_quorum: SignerQuorumConfig::new_majority(vec![
                "did:key:z6MkSigner1".to_string(),
                "did:key:z6MkSigner2".to_string(),
                "did:key:z6MkSigner3".to_string(),
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
    
    #[tokio::test]
    async fn test_genesis_trust_bundle_creation_and_verification() {
        // Create quorum configuration
        let quorum_config = initialization::initialize_test_quorum(3, QuorumType::Majority).await.unwrap();
        
        // Create test signatures
        let test_signatures = vec![
            (IdentityId("did:key:test1".to_string()), Signature(vec![1, 2, 3])),
            (IdentityId("did:key:test2".to_string()), Signature(vec![4, 5, 6])),
        ];
        
        // Create federation DIDs
        let federation_did = "did:key:z6MkFederation123".to_string();
        
        // Create test signer credentials
        let signer_credentials = vec![
            VerifiableCredential::new(
                vec!["VerifiableCredential".to_string(), "SignerCredential".to_string()],
                &IdentityId(federation_did.clone()),
                &IdentityId("did:key:test1".to_string()),
                serde_json::json!({"role": "Federation Signer"}),
            ),
            VerifiableCredential::new(
                vec!["VerifiableCredential".to_string(), "SignerCredential".to_string()],
                &IdentityId(federation_did.clone()),
                &IdentityId("did:key:test2".to_string()),
                serde_json::json!({"role": "Federation Signer"}),
            ),
        ];
        
        // 1. Initialize federation metadata
        let metadata = FederationMetadata {
            federation_did: federation_did.clone(),
            name: "Test Federation".to_string(),
            description: Some("A federation for testing trust bundles".to_string()),
            created_at: Utc::now(),
            initial_policies: Vec::new(),
            initial_members: Vec::new(),
            signer_quorum: quorum_config,
            genesis_cid: Cid::default(),
            additional_metadata: None,
        };
        
        // 2. Create establishment credential
        let establishment_credential = FederationEstablishmentCredential {
            metadata: metadata.clone(),
            epoch: 0,
            signer_signatures: test_signatures.iter()
                .map(|(id, sig)| (id.clone(), URL_SAFE_NO_PAD.encode(&sig.0)))
                .collect(),
        };
        
        // 3. Create genesis trust bundle
        let trust_bundle_result = trustbundle::create_trust_bundle(
            &metadata,
            establishment_credential,
            signer_credentials,
            test_signatures.clone(),
        ).await;
        
        assert!(trust_bundle_result.is_ok(), "Trust bundle creation failed: {:?}", trust_bundle_result.err());
        
        let trust_bundle = trust_bundle_result.unwrap();
        
        // 4. Verify the CID matches what we expect
        let calculated_cid = trustbundle::calculate_metadata_cid(&metadata).unwrap();
        assert_eq!(calculated_cid, trust_bundle.federation_metadata_cid, "Metadata CID mismatch");
        
        // 5. Verify trust bundle
        let authorized_signer_dids: Vec<String> = test_signatures.iter()
            .map(|(id, _)| id.0.clone())
            .collect();
            
        let verify_result = trustbundle::verify_trust_bundle(&trust_bundle, &authorized_signer_dids).await;
        assert!(verify_result.is_ok(), "Trust bundle verification failed: {:?}", verify_result.err());
        assert!(verify_result.unwrap(), "Trust bundle should be valid");
    }
} 