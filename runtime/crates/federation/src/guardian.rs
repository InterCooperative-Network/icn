use serde::{Deserialize, Serialize};
use cid::Cid;
use chrono::{DateTime, Utc};
use icn_identity::{
    IdentityId, VerifiableCredential, KeyPair, Signature,
    QuorumConfig, QuorumProof, IdentityScope
};
use ssi_jwk::JWK;

use crate::error::{FederationError, FederationResult};
use uuid::Uuid;
use std::collections::HashMap;

/// Type of quorum required for guardian decisions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuorumType {
    /// Simple majority (>50%)
    Majority,
    
    /// Specific threshold (percentage 0-100)
    Threshold(u8),
    
    /// Unanimous agreement required
    Unanimous,
    
    /// Weighted voting (Guardian ID, Weight), Required Total
    Weighted(Vec<(String, u32)>, u32),
}

/// Configuration for guardian quorum decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianQuorumConfig {
    /// List of authorized guardian DIDs
    pub guardians: Vec<String>,
    
    /// Type of quorum required for decisions
    pub quorum_type: QuorumType,
    
    /// Minimum wait time before executing decisions (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_wait_time_seconds: Option<u64>,
    
    /// Additional requirements or constraints on quorum
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_requirements: Option<serde_json::Value>,
}

impl GuardianQuorumConfig {
    /// Create a new majority-based quorum configuration
    pub fn new_majority(guardians: Vec<String>) -> Self {
        Self {
            guardians,
            quorum_type: QuorumType::Majority,
            min_wait_time_seconds: None,
            additional_requirements: None,
        }
    }
    
    /// Create a new threshold-based quorum configuration
    pub fn new_threshold(guardians: Vec<String>, threshold_percentage: u8) -> Self {
        // Ensure threshold is in valid range
        let threshold = threshold_percentage.min(100);
        
        Self {
            guardians,
            quorum_type: QuorumType::Threshold(threshold),
            min_wait_time_seconds: None,
            additional_requirements: None,
        }
    }
    
    /// Create a new unanimous quorum configuration
    pub fn new_unanimous(guardians: Vec<String>) -> Self {
        Self {
            guardians,
            quorum_type: QuorumType::Unanimous,
            min_wait_time_seconds: None,
            additional_requirements: None,
        }
    }
    
    /// Convert to an identity crate QuorumConfig
    pub fn to_quorum_config(&self) -> QuorumConfig {
        match &self.quorum_type {
            QuorumType::Majority => QuorumConfig::Majority,
            QuorumType::Threshold(threshold) => QuorumConfig::Threshold(*threshold),
            QuorumType::Unanimous => QuorumConfig::Threshold(100), // Unanimous is 100% threshold
            QuorumType::Weighted(weights, required) => {
                // Convert string DIDs to IdentityId
                let weighted_votes = weights.iter()
                    .map(|(did, weight)| (IdentityId(did.clone()), *weight))
                    .collect();
                
                QuorumConfig::Weighted(weighted_votes, *required)
            }
        }
    }
}

/// Represents a Guardian for federation governance
#[derive(Debug, Clone)]
pub struct Guardian {
    /// The guardian's DID
    pub did: IdentityId,
    
    /// The guardian's keypair
    pub keypair: Option<KeyPair>,
    
    /// The guardian's credential
    pub credential: Option<GuardianCredential>,
}

impl Guardian {
    /// Create a new guardian with the given DID and keypair
    pub fn new(did: IdentityId, keypair: Option<KeyPair>) -> Self {
        Self {
            did,
            keypair,
            credential: None,
        }
    }
    
    /// Sign a message using this guardian's keypair
    pub fn sign(&self, message: &[u8]) -> FederationResult<Signature> {
        if let Some(keypair) = &self.keypair {
            icn_identity::sign_message(message, keypair)
                .map_err(|e| FederationError::VerificationError(format!("Failed to sign message: {}", e)))
        } else {
            Err(FederationError::GuardianError("Guardian has no keypair available for signing".to_string()))
        }
    }
}

/// Represents a Guardian role credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianCredential {
    /// The credential
    pub credential: VerifiableCredential,
}

/// Functions for guardian initialization and management
pub mod initialization {
    use super::*;
    use icn_identity::{
        generate_did_key, sign_credential, VerifiableCredential
    };
    use uuid::Uuid;
    use std::collections::HashMap;
    
    /// Generate a new guardian with a fresh keypair
    pub async fn generate_guardian() -> FederationResult<Guardian> {
        // Generate a new DID and keypair
        let (did_str, jwk) = generate_did_key().await
            .map_err(|e| FederationError::BootstrapError(format!("Failed to generate DID key: {}", e)))?;
        
        // Convert to our types
        let did = IdentityId(did_str);
        
        // Create a basic KeyPair (this would need to be enhanced with JWK handling)
        let keypair = KeyPair::new(Vec::new(), Vec::new()); // Placeholder implementation
        
        // Create and return the Guardian
        Ok(Guardian::new(did, Some(keypair)))
    }
    
    /// Create a guardian from an existing DID and JWK
    pub fn from_jwk(did: String, jwk: JWK) -> FederationResult<Guardian> {
        // Create Identity ID
        let identity_id = IdentityId(did);
        
        // Create KeyPair from JWK
        // Note: In a real implementation, this would properly extract key material from JWK
        let keypair = KeyPair::new(Vec::new(), Vec::new()); // Placeholder
        
        // Return the Guardian
        Ok(Guardian::new(identity_id, Some(keypair)))
    }
    
    /// Create guardian credentials for each guardian in the set
    pub async fn create_guardian_credentials(
        guardians: &mut [Guardian],
        federation_did: &str,
    ) -> FederationResult<Vec<GuardianCredential>> {
        let mut credentials = Vec::with_capacity(guardians.len());
        let issuer_id = IdentityId(federation_did.to_string());
        
        for guardian in guardians {
            // Create the credential
            let credential = create_guardian_credential(
                &guardian.did,
                &issuer_id,
                None, // No custom claims for now
            ).await?;
            
            // Store the credential
            let guardian_credential = GuardianCredential { credential };
            guardian.credential = Some(guardian_credential.clone());
            credentials.push(guardian_credential);
        }
        
        Ok(credentials)
    }
    
    /// Create a single guardian credential
    async fn create_guardian_credential(
        guardian_did: &IdentityId,
        issuer_did: &IdentityId,
        additional_claims: Option<HashMap<String, serde_json::Value>>,
    ) -> FederationResult<VerifiableCredential> {
        // Credential types
        let types = vec![
            "VerifiableCredential".to_string(),
            "GuardianCredential".to_string()
        ];
        
        // Base claims
        let mut claims = serde_json::Map::new();
        claims.insert("role".to_string(), serde_json::Value::String("Guardian".to_string()));
        claims.insert("scope".to_string(), serde_json::Value::String(IdentityScope::Guardian.to_string()));
        
        // Add additional claims if provided
        if let Some(additional) = additional_claims {
            for (key, value) in additional {
                claims.insert(key, value);
            }
        }
        
        // Create the credential
        let credential = VerifiableCredential::new(
            types,
            issuer_did,
            guardian_did,
            serde_json::Value::Object(claims),
        );
        
        // Note: In a full implementation, this would be signed by the issuer
        // For now, return unsigned credential (real signing would use JWK implementation)
        Ok(credential)
    }
    
    /// Initialize a set of guardians with a specified quorum configuration
    pub async fn initialize_guardian_set(
        count: usize,
        quorum_type: QuorumType,
    ) -> FederationResult<(Vec<Guardian>, GuardianQuorumConfig)> {
        if count == 0 {
            return Err(FederationError::BootstrapError("Guardian count must be greater than 0".to_string()));
        }
        
        // Generate the specified number of guardians
        let mut guardians = Vec::with_capacity(count);
        for _ in 0..count {
            let guardian = generate_guardian().await?;
            guardians.push(guardian);
        }
        
        // Extract DIDs for the quorum config
        let guardian_dids = guardians.iter()
            .map(|g| g.did.0.clone())
            .collect::<Vec<_>>();
        
        // Create quorum config based on type
        let quorum_config = match quorum_type {
            QuorumType::Majority => GuardianQuorumConfig::new_majority(guardian_dids),
            QuorumType::Threshold(threshold) => GuardianQuorumConfig::new_threshold(guardian_dids, threshold),
            QuorumType::Unanimous => GuardianQuorumConfig::new_unanimous(guardian_dids),
            QuorumType::Weighted(_, _) => {
                // For simplicity, default to equal weights for initial setup
                let weighted_guardians: Vec<(String, u32)> = guardian_dids.iter()
                    .map(|did| (did.clone(), 1u32))
                    .collect();
                
                // Set required weight to majority
                let required = (count as u32 / 2) + 1;
                
                GuardianQuorumConfig {
                    guardians: guardian_dids,
                    quorum_type: QuorumType::Weighted(weighted_guardians, required),
                    min_wait_time_seconds: None,
                    additional_requirements: None,
                }
            }
        };
        
        Ok((guardians, quorum_config))
    }
}

/// Functions for guardian voting and decisions
pub mod decisions {
    use super::*;
    
    /// Create a quorum proof for a specific action
    pub async fn create_quorum_proof(
        action_data: &[u8],
        guardians: &[Guardian],
        config: &GuardianQuorumConfig,
    ) -> FederationResult<QuorumProof> {
        // Convert guardian quorum config to identity crate's QuorumConfig
        let quorum_config = config.to_quorum_config();
        
        // Collect signatures from available guardians
        let mut votes = Vec::new();
        
        for guardian in guardians {
            // Skip guardians without keypairs
            if guardian.keypair.is_none() {
                continue;
            }
            
            // Sign the action data with this guardian's key
            match guardian.sign(action_data) {
                Ok(signature) => {
                    // Add the signature to the votes
                    votes.push((guardian.did.clone(), signature));
                },
                Err(e) => {
                    // Log the error but continue with other guardians
                    tracing::warn!("Guardian {} failed to sign: {}", guardian.did.0, e);
                }
            }
        }
        
        // Check if we have enough votes
        let total_guardians = config.guardians.len();
        
        // Simple validation based on quorum type
        match &config.quorum_type {
            QuorumType::Majority => {
                let majority = (total_guardians / 2) + 1;
                if votes.len() < majority {
                    return Err(FederationError::VerificationError(format!(
                        "Not enough votes: got {}, need {} for majority",
                        votes.len(), majority
                    )));
                }
            },
            QuorumType::Threshold(threshold) => {
                let threshold_percentage = *threshold as f32 / 100.0;
                let threshold_count = (total_guardians as f32 * threshold_percentage).ceil() as usize;
                if votes.len() < threshold_count {
                    return Err(FederationError::VerificationError(format!(
                        "Not enough votes: got {}, need {} for {}% threshold",
                        votes.len(), threshold_count, threshold
                    )));
                }
            },
            QuorumType::Unanimous => {
                if votes.len() < total_guardians {
                    return Err(FederationError::VerificationError(format!(
                        "Not enough votes: got {}, need {} for unanimous approval",
                        votes.len(), total_guardians
                    )));
                }
            },
            QuorumType::Weighted(weights, required) => {
                // Calculate total accumulated weight
                let mut total_weight = 0u32;
                for (did, signature) in &votes {
                    if let Some((_, weight)) = weights.iter().find(|(guardian_did, _)| guardian_did == &did.0) {
                        total_weight += weight;
                    }
                }
                
                if total_weight < *required {
                    return Err(FederationError::VerificationError(format!(
                        "Not enough weight: got {}, need {} for weighted approval",
                        total_weight, required
                    )));
                }
            }
        }
        
        // Create the quorum proof
        Ok(QuorumProof {
            votes,
            config: quorum_config,
        })
    }
    
    /// Verify a guardian quorum proof
    pub async fn verify_quorum_proof(
        proof: &QuorumProof,
        content_hash: &[u8],
        config: &GuardianQuorumConfig,
    ) -> FederationResult<bool> {
        // Convert the guardian list to DIDs
        let guardian_dids = config.guardians.clone();
        
        // Use the identity crate's verification
        proof.verify(content_hash, &guardian_dids).await
            .map_err(|e| FederationError::VerificationError(format!("Failed to verify quorum proof: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardian::initialization::initialize_guardian_set;
    use crate::guardian::decisions::create_quorum_proof;

    #[tokio::test]
    async fn test_initialize_guardian_set() {
        // Create 3 guardians with majority quorum
        let result = initialize_guardian_set(3, QuorumType::Majority).await;
        assert!(result.is_ok(), "Failed to initialize guardian set: {:?}", result.err());
        
        let (guardians, config) = result.unwrap();
        
        // Check guardians
        assert_eq!(guardians.len(), 3, "Should have 3 guardians");
        for guardian in &guardians {
            assert!(guardian.keypair.is_some(), "Guardian should have a keypair");
            assert!(guardian.did.0.starts_with("did:key:"), "Guardian should have a DID");
        }
        
        // Check quorum config
        assert_eq!(config.guardians.len(), 3, "Config should have 3 guardians");
        assert!(matches!(config.quorum_type, QuorumType::Majority), "Quorum type should be Majority");
    }
    
    #[tokio::test]
    async fn test_guardian_quorum_signing() {
        // Create 5 guardians with 60% threshold
        let result = initialize_guardian_set(5, QuorumType::Threshold(60)).await;
        assert!(result.is_ok());
        
        let (guardians, config) = result.unwrap();
        
        // Test data to sign
        let test_data = b"test federation action";
        
        // Create quorum proof with 3 guardians (60% of 5)
        let guardian_subset = &guardians[0..3];
        let proof_result = create_quorum_proof(test_data, guardian_subset, &config).await;
        assert!(proof_result.is_ok(), "Failed to create quorum proof: {:?}", proof_result.err());
        
        let proof = proof_result.unwrap();
        
        // Check proof
        assert_eq!(proof.votes.len(), 3, "Should have 3 votes");
        
        // Use the decisions module to verify the proof
        let verify_result = decisions::verify_quorum_proof(&proof, test_data, &config).await;
        assert!(verify_result.is_ok(), "Failed to verify quorum proof: {:?}", verify_result.err());
        assert!(verify_result.unwrap(), "Quorum proof verification should succeed");
    }
    
    #[tokio::test]
    async fn test_quorum_proof_insufficient_votes() {
        // Create 5 guardians with 80% threshold
        let result = initialize_guardian_set(5, QuorumType::Threshold(80)).await;
        assert!(result.is_ok());
        
        let (guardians, config) = result.unwrap();
        
        // Test data to sign
        let test_data = b"test federation action";
        
        // Try to create quorum proof with only 3 guardians (60% of 5, below the 80% threshold)
        let guardian_subset = &guardians[0..3];
        let proof_result = create_quorum_proof(test_data, guardian_subset, &config).await;
        
        // Should fail due to insufficient votes
        assert!(proof_result.is_err(), "Should fail with insufficient votes");
        if let Err(FederationError::VerificationError(msg)) = proof_result {
            assert!(msg.contains("Not enough votes"), "Error should mention insufficient votes");
        } else {
            panic!("Expected VerificationError, got: {:?}", proof_result);
        }
    }
} 