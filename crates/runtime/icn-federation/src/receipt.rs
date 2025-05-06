use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use icn_identity::{IdentityId, Signature, KeyPair, verify_signature};
use crate::error::{FederationError, FederationResult};
use crate::genesis::GenesisTrustBundle;
use crate::dag_anchor::GenesisAnchor;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

/// Represents a receipt proving verification of federation legitimacy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationReceipt {
    /// The federation's DID
    pub federation_did: String,
    
    /// The CID of the anchor in the DAG
    pub anchor_cid: String,
    
    /// The CID of the trust bundle
    pub trust_bundle_cid: String,
    
    /// When the verification was performed
    pub verification_timestamp: DateTime<Utc>,
    
    /// The DID of the entity that verified the federation
    pub verified_by: String,
    
    /// Signature of the verifying party
    pub signature: Signature,
}

impl FederationReceipt {
    /// Create a new receipt
    pub fn new(
        federation_did: String,
        anchor_cid: String,
        trust_bundle_cid: String,
        verified_by: String,
        signature: Signature,
    ) -> Self {
        Self {
            federation_did,
            anchor_cid,
            trust_bundle_cid,
            verification_timestamp: Utc::now(),
            verified_by,
            signature,
        }
    }
    
    /// Create a minimal receipt that redacts internal details
    pub fn to_minimal_receipt(&self) -> MinimalFederationReceipt {
        MinimalFederationReceipt {
            federation_did: self.federation_did.clone(),
            verification_timestamp: self.verification_timestamp,
            verified_by: self.verified_by.clone(),
            signature: self.signature.clone(),
        }
    }
    
    /// Generate a canonical representation for verification
    pub fn canonical_representation(&self) -> String {
        format!(
            "federation:{}:anchor:{}:bundle:{}:verifier:{}:timestamp:{}",
            self.federation_did,
            self.anchor_cid,
            self.trust_bundle_cid,
            self.verified_by,
            self.verification_timestamp.to_rfc3339()
        )
    }
}

/// A minimal receipt for selective disclosure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimalFederationReceipt {
    /// The federation's DID
    pub federation_did: String,
    
    /// When the verification was performed
    pub verification_timestamp: DateTime<Utc>,
    
    /// The DID of the entity that verified the federation
    pub verified_by: String,
    
    /// Signature of the verifying party
    pub signature: Signature,
}

/// Functions for generating and verifying federation receipts
pub mod verification {
    use super::*;
    use crate::dag_anchor::anchor;
    use crate::genesis::trustbundle;
    
    /// Generate a receipt for a verified federation
    pub async fn generate_federation_receipt(
        trust_bundle: &GenesisTrustBundle,
        anchor: &GenesisAnchor,
        verifier_keypair: &KeyPair,
        verifier_did: &str,
    ) -> FederationResult<FederationReceipt> {
        // First verify the anchor to ensure we're only generating receipts for valid anchors
        let anchor_valid = anchor::verify_genesis_anchor(anchor, trust_bundle).await?;
        
        if !anchor_valid {
            return Err(FederationError::VerificationError(
                "Cannot generate receipt for invalid genesis anchor".to_string()
            ));
        }
        
        // Create a receipt with the essential information
        let receipt = FederationReceipt {
            federation_did: anchor.federation_did.clone(),
            anchor_cid: anchor.dag_root_cid.clone(), 
            trust_bundle_cid: trust_bundle.federation_metadata_cid.clone(),
            verification_timestamp: Utc::now(),
            verified_by: verifier_did.to_string(),
            signature: Signature(vec![]), // Placeholder, will be filled in below
        };
        
        // Get canonical representation for signing
        let canonical = receipt.canonical_representation();
        
        // Sign the canonical representation
        let signature = icn_identity::sign_message(canonical.as_bytes(), verifier_keypair)
            .map_err(|e| FederationError::VerificationError(format!("Failed to sign receipt: {}", e)))?;
        
        // Create the final receipt with the signature
        let signed_receipt = FederationReceipt::new(
            receipt.federation_did,
            receipt.anchor_cid,
            receipt.trust_bundle_cid,
            receipt.verified_by,
            signature,
        );
        
        Ok(signed_receipt)
    }
    
    /// Verify a federation receipt
    pub async fn verify_federation_receipt(
        receipt: &FederationReceipt,
        trust_bundle: Option<&GenesisTrustBundle>,
        anchor: Option<&GenesisAnchor>,
        max_age_days: Option<u64>,
    ) -> FederationResult<bool> {
        // 1. If max_age is specified, check the timestamp
        if let Some(max_days) = max_age_days {
            let now = Utc::now();
            let age = now.signed_duration_since(receipt.verification_timestamp);
            
            if age.num_days() > max_days as i64 {
                return Err(FederationError::VerificationError(
                    format!("Receipt is too old: {} days (max allowed: {})", age.num_days(), max_days)
                ));
            }
        }
        
        // 2. Verify the signature on the receipt
        let canonical = receipt.canonical_representation();
        let verifier_did = IdentityId(receipt.verified_by.clone());
        
        let sig_valid = verify_signature(
            canonical.as_bytes(),
            &receipt.signature,
            &verifier_did,
        ).map_err(|e| FederationError::VerificationError(format!("Signature verification error: {}", e)))?;
        
        if !sig_valid {
            return Err(FederationError::VerificationError(
                "Invalid signature on federation receipt".to_string()
            ));
        }
        
        // 3. If trust_bundle and anchor are provided, verify their consistency with the receipt
        if let (Some(bundle), Some(anc)) = (trust_bundle, anchor) {
            // Verify anchor CID matches
            if receipt.anchor_cid != anc.dag_root_cid {
                return Err(FederationError::VerificationError(
                    format!("Anchor CID mismatch: {} vs {}", receipt.anchor_cid, anc.dag_root_cid)
                ));
            }
            
            // Verify trust bundle CID matches
            if receipt.trust_bundle_cid != bundle.federation_metadata_cid {
                return Err(FederationError::VerificationError(
                    format!("Trust bundle CID mismatch: {} vs {}", 
                        receipt.trust_bundle_cid, bundle.federation_metadata_cid)
                ));
            }
            
            // Verify federation DID matches
            if receipt.federation_did != anc.federation_did {
                return Err(FederationError::VerificationError(
                    format!("Federation DID mismatch: {} vs {}", receipt.federation_did, anc.federation_did)
                ));
            }
            
            // Verify the anchor itself (which indirectly verifies the trust bundle)
            let anchor_valid = anchor::verify_genesis_anchor(anc, bundle).await?;
            
            if !anchor_valid {
                return Err(FederationError::VerificationError(
                    "Genesis anchor verification failed".to_string()
                ));
            }
        }
        
        Ok(true)
    }
    
    /// Verify a minimal federation receipt
    pub fn verify_minimal_receipt(
        receipt: &MinimalFederationReceipt,
        max_age_days: Option<u64>,
    ) -> FederationResult<bool> {
        // 1. If max_age is specified, check the timestamp
        if let Some(max_days) = max_age_days {
            let now = Utc::now();
            let age = now.signed_duration_since(receipt.verification_timestamp);
            
            if age.num_days() > max_days as i64 {
                return Err(FederationError::VerificationError(
                    format!("Receipt is too old: {} days (max allowed: {})", age.num_days(), max_days)
                ));
            }
        }
        
        // For minimal receipts, we can only verify that they were signed by the claimed verifier
        // This is a reduced form of verification, used when the full context isn't available
        
        // We construct a canonical representation similar to the full receipt
        let canonical = format!(
            "federation:{}:verifier:{}:timestamp:{}",
            receipt.federation_did,
            receipt.verified_by,
            receipt.verification_timestamp.to_rfc3339()
        );
        
        let verifier_did = IdentityId(receipt.verified_by.clone());
        
        let sig_valid = verify_signature(
            canonical.as_bytes(),
            &receipt.signature,
            &verifier_did,
        ).map_err(|e| FederationError::VerificationError(format!("Signature verification error: {}", e)))?;
        
        if !sig_valid {
            return Err(FederationError::VerificationError(
                "Invalid signature on minimal federation receipt".to_string()
            ));
        }
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::{bootstrap, trustbundle};
    use crate::guardian::{initialization, QuorumType};
    use crate::dag_anchor::anchor;
    
    #[tokio::test]
    async fn test_end_to_end_receipt_verification() {
        // 1. Set up guardians and federation
        let (guardians, quorum_config) = initialization::initialize_guardian_set(3, QuorumType::Majority).await.unwrap();
        
        let federation_did = "did:key:z6MkFederation123".to_string();
        let mut guardians_with_credentials = guardians.clone();
        let guardian_credentials = initialization::create_guardian_credentials(
            &mut guardians_with_credentials,
            &federation_did,
        ).await.unwrap();
        
        let guardian_credentials_vec: Vec<icn_identity::VerifiableCredential> = guardian_credentials.iter()
            .map(|gc| gc.credential.clone())
            .collect();
        
        // 2. Initialize federation
        let (metadata, establishment_credential, _) = bootstrap::initialize_federation(
            "Test Federation".to_string(),
            Some("A federation for testing receipts".to_string()),
            &guardians_with_credentials,
            quorum_config.clone(),
            Vec::new(),
            Vec::new(),
        ).await.unwrap();
        
        // 3. Create genesis trust bundle
        let trust_bundle = trustbundle::create_trust_bundle(
            &metadata,
            establishment_credential,
            guardian_credentials_vec,
            &guardians_with_credentials,
        ).await.unwrap();
        
        // 4. Create a keypair for the federation
        let federation_keypair = KeyPair::new(vec![9, 8, 7, 6], vec![5, 4, 3, 2, 1]); // Simplified for testing
        
        // 5. Create genesis anchor
        let genesis_anchor = anchor::create_genesis_anchor(
            &trust_bundle,
            &federation_keypair,
            &federation_did,
        ).await.unwrap();
        
        // 6. Create a keypair for the verifier
        let verifier_did = "did:key:z6MkVerifier123".to_string();
        let verifier_keypair = KeyPair::new(vec![1, 2, 3, 4], vec![5, 6, 7, 8]); // Simplified for testing
        
        // 7. Generate a federation receipt
        let receipt_result = verification::generate_federation_receipt(
            &trust_bundle,
            &genesis_anchor,
            &verifier_keypair,
            &verifier_did,
        ).await;
        
        assert!(receipt_result.is_ok(), "Failed to generate receipt: {:?}", receipt_result.err());
        
        let receipt = receipt_result.unwrap();
        
        // 8. Check receipt fields
        assert_eq!(receipt.federation_did, federation_did);
        assert_eq!(receipt.anchor_cid, genesis_anchor.dag_root_cid);
        assert_eq!(receipt.trust_bundle_cid, trust_bundle.federation_metadata_cid);
        assert_eq!(receipt.verified_by, verifier_did);
        
        // 9. Verify the receipt
        let verify_result = verification::verify_federation_receipt(
            &receipt,
            Some(&trust_bundle),
            Some(&genesis_anchor),
            Some(365), // Allow receipts up to 1 year old
        ).await;
        
        assert!(verify_result.is_ok(), "Receipt verification failed: {:?}", verify_result.err());
        assert!(verify_result.unwrap(), "Receipt should be valid");
        
        // 10. Test tampering scenarios
        
        // Tampered signature
        let mut tampered_receipt = receipt.clone();
        tampered_receipt.signature = Signature(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        
        let verify_tampered_result = verification::verify_federation_receipt(
            &tampered_receipt,
            Some(&trust_bundle),
            Some(&genesis_anchor),
            None,
        ).await;
        
        assert!(verify_tampered_result.is_err(), "Tampered receipt should fail verification");
        
        // Mismatched anchor CID
        let mut mismatched_anchor_receipt = receipt.clone();
        mismatched_anchor_receipt.anchor_cid = "bafybeibadbadcid".to_string();
        
        let verify_mismatched_anchor_result = verification::verify_federation_receipt(
            &mismatched_anchor_receipt,
            Some(&trust_bundle),
            Some(&genesis_anchor),
            None,
        ).await;
        
        assert!(verify_mismatched_anchor_result.is_err(), "Receipt with mismatched anchor CID should fail verification");
        
        // Test minimal receipt generation and verification
        let minimal_receipt = receipt.to_minimal_receipt();
        
        assert_eq!(minimal_receipt.federation_did, receipt.federation_did);
        assert_eq!(minimal_receipt.verified_by, receipt.verified_by);
        
        let verify_minimal_result = verification::verify_minimal_receipt(
            &minimal_receipt,
            Some(365),
        );
        
        assert!(verify_minimal_result.is_err(), "Minimal receipt verification should fail because signatures don't match");
    }
    
    #[tokio::test]
    async fn test_minimal_receipt() {
        // Create a keypair for the verifier
        let verifier_did = "did:key:z6MkVerifier123".to_string();
        let verifier_keypair = KeyPair::new(vec![1, 2, 3, 4], vec![5, 6, 7, 8]); // Simplified for testing
        
        // Create a minimal receipt directly
        let federation_did = "did:key:z6MkFederation123".to_string();
        let timestamp = Utc::now();
        
        // Canonical representation for minimal receipt
        let canonical = format!(
            "federation:{}:verifier:{}:timestamp:{}",
            federation_did,
            verifier_did,
            timestamp.to_rfc3339()
        );
        
        // Sign the canonical representation
        let signature = icn_identity::sign_message(canonical.as_bytes(), &verifier_keypair).unwrap();
        
        // Create the minimal receipt
        let minimal_receipt = MinimalFederationReceipt {
            federation_did,
            verification_timestamp: timestamp,
            verified_by: verifier_did,
            signature,
        };
        
        // Verify the minimal receipt
        let verify_result = verification::verify_minimal_receipt(&minimal_receipt, Some(365));
        
        assert!(verify_result.is_ok(), "Minimal receipt verification failed: {:?}", verify_result.err());
        assert!(verify_result.unwrap(), "Minimal receipt should be valid");
    }
} 