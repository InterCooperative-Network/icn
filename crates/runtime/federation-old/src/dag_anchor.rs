use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use icn_identity::{IdentityId, Signature, KeyPair};
use crate::error::{FederationError, FederationResult};
use crate::genesis::GenesisTrustBundle;
use sha2::{Digest, Sha256};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use cid::multihash::{Multihash, Code};

/// Represents an anchor in the DAG for a federation genesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisAnchor {
    /// The CID of the DAG root node
    pub dag_root_cid: String,
    
    /// The CID of the trust bundle
    pub trust_bundle_cid: String,
    
    /// The federation's DID
    pub federation_did: String,
    
    /// When the anchor was issued
    pub issued_at: DateTime<Utc>,
    
    /// Signature over the anchor data
    pub anchor_signature: Signature,
}

impl GenesisAnchor {
    /// Create a new GenesisAnchor
    pub fn new(
        dag_root_cid: String,
        trust_bundle_cid: String,
        federation_did: String,
        anchor_signature: Signature,
    ) -> Self {
        Self {
            dag_root_cid,
            trust_bundle_cid,
            federation_did,
            issued_at: Utc::now(),
            anchor_signature,
        }
    }
    
    /// Convert to a DAG payload for anchoring by the DAG runtime
    pub fn to_dag_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "FederationGenesisAnchor",
            "version": "1.0",
            "dag_root_cid": self.dag_root_cid,
            "trust_bundle_cid": self.trust_bundle_cid,
            "federation_did": self.federation_did,
            "issued_at": self.issued_at.to_rfc3339(),
            "anchor_signature": URL_SAFE_NO_PAD.encode(&self.anchor_signature.0),
            "metadata": {
                "anchored_at": self.issued_at.to_rfc3339(),
                "anchor_type": "genesis",
                "federation_version": "0.1.0"
            }
        })
    }
}

/// Functions for creating and verifying genesis anchors
pub mod anchor {
    use super::*;
    
    /// Create a genesis anchor for a trust bundle
    pub async fn create_genesis_anchor(
        trust_bundle: &GenesisTrustBundle,
        keypair: &KeyPair,
        federation_did: &str,
    ) -> FederationResult<GenesisAnchor> {
        // Calculate Merkle root of TrustBundle JSON (this will be the DAG root CID)
        let bundle_bytes = serde_json::to_vec(trust_bundle)
            .map_err(|e| FederationError::SerializationError(format!("Failed to serialize trust bundle: {}", e)))?;
        
        let dag_root_cid = calculate_merkle_root(&bundle_bytes)?;
        
        // Create a canonical representation of the anchor data for signing
        let anchor_data = format!(
            "{}:{}:{}:{}",
            dag_root_cid,
            trust_bundle.federation_metadata_cid,
            federation_did,
            Utc::now().to_rfc3339()
        );
        
        // Sign the anchor data
        let signature = icn_identity::sign_message(anchor_data.as_bytes(), keypair)
            .map_err(|e| FederationError::VerificationError(format!("Failed to sign anchor data: {}", e)))?;
        
        // Construct the anchor
        let anchor = GenesisAnchor::new(
            dag_root_cid,
            trust_bundle.federation_metadata_cid.clone(),
            federation_did.to_string(),
            signature,
        );
        
        Ok(anchor)
    }
    
    /// Verify a genesis anchor
    pub async fn verify_genesis_anchor(
        anchor: &GenesisAnchor,
        trust_bundle: &GenesisTrustBundle,
    ) -> FederationResult<bool> {
        // Check that the trust bundle CID matches
        if anchor.trust_bundle_cid != trust_bundle.federation_metadata_cid {
            return Err(FederationError::VerificationError(
                format!("Trust bundle CID mismatch: {} vs {}", 
                    anchor.trust_bundle_cid, trust_bundle.federation_metadata_cid)
            ));
        }
        
        // Calculate Merkle root of TrustBundle JSON
        let bundle_bytes = serde_json::to_vec(trust_bundle)
            .map_err(|e| FederationError::SerializationError(format!("Failed to serialize trust bundle: {}", e)))?;
        
        let calculated_dag_root_cid = calculate_merkle_root(&bundle_bytes)?;
        
        // Check that the DAG root CID matches
        if anchor.dag_root_cid != calculated_dag_root_cid {
            return Err(FederationError::VerificationError(
                format!("DAG root CID mismatch: {} vs {}", 
                    anchor.dag_root_cid, calculated_dag_root_cid)
            ));
        }
        
        // Recreate the canonical representation of the anchor data
        let anchor_data = format!(
            "{}:{}:{}:{}",
            anchor.dag_root_cid,
            anchor.trust_bundle_cid,
            anchor.federation_did,
            anchor.issued_at.to_rfc3339()
        );
        
        // Verify the signature
        let did = IdentityId(anchor.federation_did.clone());
        let valid = icn_identity::verify_signature(
            anchor_data.as_bytes(),
            &anchor.anchor_signature,
            &did,
        ).map_err(|e| FederationError::VerificationError(format!("Signature verification error: {}", e)))?;
        
        Ok(valid)
    }
    
    /// Calculate Merkle root (CID) of data
    pub fn calculate_merkle_root(data: &[u8]) -> FederationResult<String> {
        // Hash the data with SHA-256
        let hash = Sha256::digest(data);
        
        // Create a multihash with SHA-256 (0x12)
        let mh = Multihash::wrap(Code::Sha2_256.into(), &hash)
            .map_err(|_| FederationError::CidError("Failed to create multihash".to_string()))?;
        
        // Create a CID v1 with dag-json codec (0x0129)
        let cid = cid::Cid::new_v1(0x0129, mh);
        
        Ok(cid.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::{GenesisTrustBundle, trustbundle};
    use crate::guardian::{Guardian, GuardianQuorumConfig, QuorumType};
    use crate::guardian::initialization;
    use crate::genesis::bootstrap;
    
    #[tokio::test]
    async fn test_genesis_anchor_creation_and_verification() {
        // Create guardians with a majority quorum
        let (guardians, quorum_config) = initialization::initialize_guardian_set(3, QuorumType::Majority).await.unwrap();
        
        // Create guardian credentials
        let federation_did = "did:key:z6MkFederation123".to_string();
        let mut guardians_with_credentials = guardians.clone();
        let guardian_credentials = initialization::create_guardian_credentials(
            &mut guardians_with_credentials,
            &federation_did,
        ).await.unwrap();
        
        let guardian_credentials_vec: Vec<icn_identity::VerifiableCredential> = guardian_credentials.iter()
            .map(|gc| gc.credential.clone())
            .collect();
        
        // Initialize federation
        let (metadata, establishment_credential, _) = bootstrap::initialize_federation(
            "Test Federation".to_string(),
            Some("A federation for testing anchors".to_string()),
            &guardians_with_credentials,
            quorum_config.clone(),
            Vec::new(),
            Vec::new(),
        ).await.unwrap();
        
        // Create genesis trust bundle
        let trust_bundle = trustbundle::create_trust_bundle(
            &metadata,
            establishment_credential,
            guardian_credentials_vec,
            &guardians_with_credentials,
        ).await.unwrap();
        
        // Create a keypair for signing the anchor
        let keypair = KeyPair::new(vec![9, 8, 7, 6], vec![5, 4, 3, 2, 1]); // Simplified for testing
        
        // Create genesis anchor
        let anchor_result = anchor::create_genesis_anchor(
            &trust_bundle,
            &keypair,
            &federation_did,
        ).await;
        
        assert!(anchor_result.is_ok(), "Failed to create genesis anchor: {:?}", anchor_result.err());
        
        let anchor = anchor_result.unwrap();
        
        // Check anchor fields
        assert_eq!(anchor.federation_did, federation_did);
        assert_eq!(anchor.trust_bundle_cid, trust_bundle.federation_metadata_cid);
        assert!(!anchor.dag_root_cid.is_empty());
        
        // Verify anchor
        let verify_result = anchor::verify_genesis_anchor(&anchor, &trust_bundle).await;
        assert!(verify_result.is_ok(), "Failed to verify genesis anchor: {:?}", verify_result.err());
        assert!(verify_result.unwrap(), "Genesis anchor should be valid");
        
        // Test failure case: tampered signature
        let mut invalid_anchor = anchor.clone();
        invalid_anchor.anchor_signature = Signature(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        
        let verify_invalid_result = anchor::verify_genesis_anchor(&invalid_anchor, &trust_bundle).await;
        assert!(verify_invalid_result.is_err() || !verify_invalid_result.unwrap(), 
                "Tampered anchor should not verify");
        
        // Test failure case: tampered trust bundle CID
        let mut mismatched_cid_anchor = anchor.clone();
        mismatched_cid_anchor.trust_bundle_cid = "bafybeibadcid123".to_string();
        
        let verify_mismatched_result = anchor::verify_genesis_anchor(&mismatched_cid_anchor, &trust_bundle).await;
        assert!(verify_mismatched_result.is_err(), "Anchor with mismatched CID should not verify");
        
        // Test failure case: tampered DAG root CID
        let mut mismatched_root_anchor = anchor.clone();
        mismatched_root_anchor.dag_root_cid = "bafybeibadroot123".to_string();
        
        let verify_mismatched_root_result = anchor::verify_genesis_anchor(&mismatched_root_anchor, &trust_bundle).await;
        assert!(verify_mismatched_root_result.is_err(), "Anchor with mismatched root CID should not verify");
        
        // Check DAG payload generation
        let dag_payload = anchor.to_dag_payload();
        assert!(dag_payload.is_object(), "DAG payload should be a JSON object");
        assert_eq!(dag_payload["type"], "FederationGenesisAnchor");
        assert_eq!(dag_payload["federation_did"], federation_did);
        assert_eq!(dag_payload["trust_bundle_cid"], trust_bundle.federation_metadata_cid);
        assert!(dag_payload["metadata"].is_object(), "DAG payload should have metadata");
    }
    
    #[tokio::test]
    async fn test_merkle_root_calculation() {
        // Test that the same data produces the same CID
        let data1 = b"test data for CID calculation";
        let data2 = b"test data for CID calculation";
        let data3 = b"different test data";
        
        let cid1 = anchor::calculate_merkle_root(data1).unwrap();
        let cid2 = anchor::calculate_merkle_root(data2).unwrap();
        let cid3 = anchor::calculate_merkle_root(data3).unwrap();
        
        println!("CID 1: {}", cid1); // Print the CID for debugging
        
        assert_eq!(cid1, cid2, "Same data should produce the same CID");
        assert_ne!(cid1, cid3, "Different data should produce different CIDs");
        
        // Just check that the CID is not empty
        assert!(!cid1.is_empty(), "CID should not be empty");
        // And that it's a valid CID string
        assert!(cid::Cid::try_from(cid1.as_str()).is_ok(), "Should be a valid CID string");
    }
} 