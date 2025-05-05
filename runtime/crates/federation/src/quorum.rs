use serde::{Deserialize, Serialize};
use icn_identity::{
    IdentityId, VerifiableCredential, QuorumConfig, QuorumProof, Signature
};
use cid::Cid;
use chrono::{DateTime, Utc};
use icn_identity::{
    KeyPair, IdentityScope
};
use ssi_jwk::JWK;

use crate::error::{FederationError, FederationResult};
use uuid::Uuid;
use std::collections::HashMap;

/// Type of quorum required for decisions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuorumType {
    /// Simple majority (>50%)
    Majority,
    
    /// Specific threshold (percentage 0-100)
    Threshold(u8),
    
    /// Unanimous agreement required
    Unanimous,
    
    /// Weighted voting (DID, Weight), Required Total
    Weighted(Vec<(String, u32)>, u32),
}

/// Configuration for quorum decisions (simplified from guardian-specific implementation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianQuorumConfig {
    /// List of authorized DIDs
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
    pub fn new_majority(members: Vec<String>) -> Self {
        Self {
            guardians: members,
            quorum_type: QuorumType::Majority,
            min_wait_time_seconds: None,
            additional_requirements: None,
        }
    }
    
    /// Create a new threshold-based quorum configuration
    pub fn new_threshold(members: Vec<String>, threshold_percentage: u8) -> Self {
        // Ensure threshold is in valid range
        let threshold = threshold_percentage.min(100);
        
        Self {
            guardians: members,
            quorum_type: QuorumType::Threshold(threshold),
            min_wait_time_seconds: None,
            additional_requirements: None,
        }
    }
    
    /// Create a new unanimous quorum configuration
    pub fn new_unanimous(members: Vec<String>) -> Self {
        Self {
            guardians: members,
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
    
    /// Get the number of DIDs required for a quorum
    pub fn required_signatures(&self) -> usize {
        let total = self.guardians.len();
        
        match &self.quorum_type {
            QuorumType::Majority => (total / 2) + 1,
            QuorumType::Threshold(threshold) => {
                let threshold_percentage = *threshold as f32 / 100.0;
                (total as f32 * threshold_percentage).ceil() as usize
            },
            QuorumType::Unanimous => total,
            QuorumType::Weighted(_, required) => *required as usize,
        }
    }
}

/// Functions for quorum initialization and management
pub mod initialization {
    use super::*;
    use icn_identity::{generate_did_key, VerifiableCredential};
    
    /// Initialize a quorum configuration based on member DIDs
    pub fn create_quorum_config(
        member_dids: Vec<String>,
        quorum_type: QuorumType,
    ) -> GuardianQuorumConfig {
        match quorum_type {
            QuorumType::Majority => GuardianQuorumConfig::new_majority(member_dids),
            QuorumType::Threshold(threshold) => GuardianQuorumConfig::new_threshold(member_dids, threshold),
            QuorumType::Unanimous => GuardianQuorumConfig::new_unanimous(member_dids),
            QuorumType::Weighted(weights, required) => {
                GuardianQuorumConfig {
                    guardians: member_dids,
                    quorum_type: QuorumType::Weighted(weights, required),
                    min_wait_time_seconds: None,
                    additional_requirements: None,
                }
            }
        }
    }
    
    /// Generate a set of test DIDs for quorum configuration
    pub async fn generate_test_dids(count: usize) -> FederationResult<Vec<String>> {
        let mut dids = Vec::with_capacity(count);
        
        for _ in 0..count {
            let (did_str, _) = generate_did_key().await
                .map_err(|e| FederationError::BootstrapError(format!("Failed to generate DID key: {}", e)))?;
                
            dids.push(did_str);
        }
        
        Ok(dids)
    }
    
    /// Initialize a quorum configuration for testing
    pub async fn initialize_test_quorum(
        count: usize,
        quorum_type: QuorumType,
    ) -> FederationResult<GuardianQuorumConfig> {
        if count == 0 {
            return Err(FederationError::BootstrapError("Member count must be greater than 0".to_string()));
        }
        
        // Generate DIDs
        let dids = generate_test_dids(count).await?;
        
        // Create quorum config based on type
        Ok(create_quorum_config(dids, quorum_type))
    }
}

/// Functions for quorum decisions (simplified from guardian-specific implementation)
pub mod decisions {
    use super::*;
    use icn_identity::{IdentityId, Signature, sign_message, QuorumProof, QuorumConfig};
    
    /// Create a quorum proof for a specific action
    pub async fn create_quorum_proof(
        action_data: &[u8],
        signatures: Vec<(IdentityId, Signature)>,
        config: &GuardianQuorumConfig,
    ) -> FederationResult<QuorumProof> {
        // Convert guardian quorum config to identity crate's QuorumConfig
        let quorum_config = config.to_quorum_config();
        
        // Use the signatures directly
        let votes = signatures;
        
        // Check if we have enough votes
        let required = config.required_signatures();
        
        if votes.len() < required {
            return Err(FederationError::VerificationError(format!(
                "Not enough signatures: got {}, need {} for quorum",
                votes.len(), required
            )));
        }
        
        // Create the quorum proof
        Ok(QuorumProof {
            votes,
            config: quorum_config,
        })
    }
    
    /// Verify a quorum proof
    pub async fn verify_quorum_proof(
        proof: &QuorumProof,
        content_hash: &[u8],
        member_dids: &[String],
    ) -> FederationResult<bool> {
        // Use the identity crate's verification with general DID list
        proof.verify(content_hash, member_dids).await
            .map_err(|e| FederationError::VerificationError(format!("Failed to verify quorum proof: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardian::initialization::initialize_test_quorum;
    
    #[tokio::test]
    async fn test_initialize_quorum_config() {
        // Create a majority quorum with 3 members
        let result = initialize_test_quorum(3, QuorumType::Majority).await;
        assert!(result.is_ok(), "Failed to initialize quorum config: {:?}", result.err());
        
        let config = result.unwrap();
        
        // Check config
        assert_eq!(config.guardians.len(), 3, "Config should have 3 members");
        assert!(matches!(config.quorum_type, QuorumType::Majority), "Quorum type should be Majority");
        assert_eq!(config.required_signatures(), 2, "A majority of 3 should require 2 signatures");
    }
    
    #[tokio::test]
    async fn test_quorum_threshold_calculation() {
        // Create a 60% threshold quorum with 5 members
        let config = initialize_test_quorum(5, QuorumType::Threshold(60)).await.unwrap();
        
        // 60% of 5 should be 3
        assert_eq!(config.required_signatures(), 3, "A 60% threshold of 5 should require 3 signatures");
        
        // Create a 75% threshold quorum with 4 members
        let config = initialize_test_quorum(4, QuorumType::Threshold(75)).await.unwrap();
        
        // 75% of 4 should be 3
        assert_eq!(config.required_signatures(), 3, "A 75% threshold of 4 should require 3 signatures");
    }
    
    #[tokio::test]
    async fn test_signatures_calculation() {
        // Test various quorum types and member counts
        let test_cases = vec![
            (3, QuorumType::Majority, 2),         // 3 members, majority = 2
            (5, QuorumType::Majority, 3),         // 5 members, majority = 3
            (4, QuorumType::Threshold(75), 3),    // 4 members, 75% = 3
            (10, QuorumType::Threshold(51), 6),   // 10 members, 51% = 6
            (3, QuorumType::Unanimous, 3),        // 3 members, unanimous = 3
        ];
        
        for (member_count, quorum_type, expected_sigs) in test_cases {
            let config = initialize_test_quorum(member_count, quorum_type.clone()).await.unwrap();
            assert_eq!(
                config.required_signatures(), 
                expected_sigs, 
                "{:?} with {} members should require {} signatures", 
                quorum_type, member_count, expected_sigs
            );
        }
    }
    
    #[tokio::test]
    async fn test_quorum_config_conversion() {
        // Test conversion to identity QuorumConfig
        let config = initialize_test_quorum(5, QuorumType::Majority).await.unwrap();
        let identity_config = config.to_quorum_config();
        
        match identity_config {
            QuorumConfig::Majority => {} // Success
            _ => panic!("Expected Majority quorum config"),
        }
        
        let config = initialize_test_quorum(5, QuorumType::Threshold(75)).await.unwrap();
        let identity_config = config.to_quorum_config();
        
        match identity_config {
            QuorumConfig::Threshold(threshold) => {
                assert_eq!(threshold, 75, "Threshold should be preserved");
            }
            _ => panic!("Expected Threshold quorum config"),
        }
    }
} 