use anyhow::{anyhow, Result};
use async_trait::async_trait;
use cid::Cid;
use std::sync::{Arc, RwLock};
use tracing::{debug, error, info, warn};

use crate::{MeshPolicy, MeshPolicyFragment};

/// Interface for mesh policy management
#[async_trait]
pub trait MeshPolicyManager: Send + Sync {
    /// Get the currently active policy CID for a federation
    async fn get_active_policy_cid(&self, federation_did: &str) -> Result<Option<Cid>>;
    
    /// Load a policy by its CID
    async fn load_policy(&self, policy_cid: &Cid) -> Result<Option<MeshPolicy>>;
    
    /// Create an initial policy for a federation
    async fn create_initial_policy(&self, federation_did: &str) -> Result<MeshPolicy>;
    
    /// Update a policy with a fragment, returning the new policy CID
    async fn update_policy(
        &self, 
        previous_policy_cid: &Cid, 
        fragment: &MeshPolicyFragment,
        federation_did: &str
    ) -> Result<Cid>;
    
    /// Activate a new policy
    async fn activate_policy(&self, policy_cid: &Cid, federation_did: &str) -> Result<()>;
    
    /// Record a vote on a policy proposal
    async fn record_vote(
        &self, 
        voter_did: &str, 
        policy_cid: &Cid, 
        approved: bool
    ) -> Result<()>;
    
    /// Check if a policy proposal has reached approval quorum
    async fn has_approval_quorum(&self, policy_cid: &Cid) -> Result<bool>;
}

/// Implementation of MeshPolicyManager using a centralized state
/// This will be extended to use DAG storage in a real implementation
pub struct SimpleMeshPolicyManager {
    /// Map of federation DID to active policy CID
    active_policies: RwLock<std::collections::HashMap<String, Cid>>,
    
    /// Map of policy CID to policy object
    policies: RwLock<std::collections::HashMap<Cid, MeshPolicy>>,
    
    /// Map of proposal CID to votes (voter DID -> approval)
    votes: RwLock<std::collections::HashMap<Cid, std::collections::HashMap<String, bool>>>,
}

impl SimpleMeshPolicyManager {
    /// Create a new policy manager
    pub fn new() -> Self {
        Self {
            active_policies: RwLock::new(std::collections::HashMap::new()),
            policies: RwLock::new(std::collections::HashMap::new()),
            votes: RwLock::new(std::collections::HashMap::new()),
        }
    }
    
    /// Generate a pseudo CID for testing/mock
    fn generate_cid() -> Cid {
        // In a real implementation, this would create a proper CID
        // For now, we just return a placeholder
        Cid::default()
    }
}

#[async_trait]
impl MeshPolicyManager for SimpleMeshPolicyManager {
    async fn get_active_policy_cid(&self, federation_did: &str) -> Result<Option<Cid>> {
        let active = self.active_policies.read().unwrap();
        Ok(active.get(federation_did).cloned())
    }
    
    async fn load_policy(&self, policy_cid: &Cid) -> Result<Option<MeshPolicy>> {
        let policies = self.policies.read().unwrap();
        Ok(policies.get(policy_cid).cloned())
    }
    
    async fn create_initial_policy(&self, federation_did: &str) -> Result<MeshPolicy> {
        // Create a default policy
        let policy = MeshPolicy::new_default(federation_did);
        
        // Generate a CID for the policy
        let policy_cid = Self::generate_cid();
        
        // Store the policy
        {
            let mut policies = self.policies.write().unwrap();
            policies.insert(policy_cid, policy.clone());
        }
        
        // Set as active policy
        {
            let mut active = self.active_policies.write().unwrap();
            active.insert(federation_did.to_string(), policy_cid);
        }
        
        Ok(policy)
    }
    
    async fn update_policy(
        &self,
        previous_policy_cid: &Cid, 
        fragment: &MeshPolicyFragment,
        federation_did: &str
    ) -> Result<Cid> {
        // Load the previous policy
        let previous_policy = {
            let policies = self.policies.read().unwrap();
            match policies.get(previous_policy_cid) {
                Some(policy) => policy.clone(),
                None => return Err(anyhow!("Previous policy not found")),
            }
        };
        
        // Verify the federation matches
        if previous_policy.federation_did != federation_did {
            return Err(anyhow!("Federation DID mismatch"));
        }
        
        // Create a new policy by applying the fragment
        let mut new_policy = previous_policy.clone();
        new_policy.previous_policy_cid = Some(*previous_policy_cid);
        
        // Apply the update
        if let Err(e) = new_policy.apply_update(fragment) {
            return Err(anyhow!("Failed to apply policy update: {}", e));
        }
        
        // Generate a CID for the new policy
        let new_policy_cid = Self::generate_cid();
        
        // Store the new policy
        {
            let mut policies = self.policies.write().unwrap();
            policies.insert(new_policy_cid, new_policy);
        }
        
        // Initialize empty vote record
        {
            let mut votes = self.votes.write().unwrap();
            votes.insert(new_policy_cid, std::collections::HashMap::new());
        }
        
        Ok(new_policy_cid)
    }
    
    async fn activate_policy(&self, policy_cid: &Cid, federation_did: &str) -> Result<()> {
        // Verify the policy exists
        {
            let policies = self.policies.read().unwrap();
            if !policies.contains_key(policy_cid) {
                return Err(anyhow!("Policy not found"));
            }
        }
        
        // Set as active policy
        {
            let mut active = self.active_policies.write().unwrap();
            active.insert(federation_did.to_string(), *policy_cid);
        }
        
        Ok(())
    }
    
    async fn record_vote(
        &self,
        voter_did: &str,
        policy_cid: &Cid,
        approved: bool
    ) -> Result<()> {
        // Verify the policy exists
        {
            let policies = self.policies.read().unwrap();
            if !policies.contains_key(policy_cid) {
                return Err(anyhow!("Policy not found"));
            }
        }
        
        // Record the vote
        {
            let mut votes = self.votes.write().unwrap();
            
            // Initialize vote record if it doesn't exist
            if !votes.contains_key(policy_cid) {
                votes.insert(*policy_cid, std::collections::HashMap::new());
            }
            
            // Record the vote
            if let Some(policy_votes) = votes.get_mut(policy_cid) {
                policy_votes.insert(voter_did.to_string(), approved);
            }
        }
        
        Ok(())
    }
    
    async fn has_approval_quorum(&self, policy_cid: &Cid) -> Result<bool> {
        // Get votes for this policy
        let votes = {
            let votes = self.votes.read().unwrap();
            match votes.get(policy_cid) {
                Some(policy_votes) => policy_votes.clone(),
                None => return Err(anyhow!("No votes found for policy")),
            }
        };
        
        // Count approvals
        let total_votes = votes.len();
        let approvals = votes.values().filter(|&&approved| approved).count();
        
        // Simple majority for now (in a real implementation this would use the federation's governance rules)
        Ok(total_votes > 0 && approvals * 2 > total_votes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_policy_lifecycle() -> Result<()> {
        // Create a policy manager
        let manager = SimpleMeshPolicyManager::new();
        
        // Create an initial policy
        let federation_did = "did:icn:federation:test";
        let initial_policy = manager.create_initial_policy(federation_did).await?;
        
        // Verify it's the active policy
        let active_cid = manager.get_active_policy_cid(federation_did).await?;
        assert!(active_cid.is_some());
        
        // Create a policy fragment
        let fragment = MeshPolicyFragment {
            reputation_params: Some(crate::ReputationParamsFragment {
                alpha: Some(0.7),
                beta: Some(0.2),
                gamma: Some(0.1),
                lambda: None,
            }),
            reward_settings: Some(crate::RewardSettingsFragment {
                worker_percentage: Some(75),
                verifier_percentage: Some(20),
                platform_fee_percentage: Some(5),
                use_reputation_weighting: None,
                platform_fee_address: None,
            }),
            stake_weight: None,
            min_fee: Some(20),
            base_capability_scope: None,
            bonding_requirements: None,
            scheduling_params: None,
            verification_quorum: None,
            description: "Update reputation weights and reward distribution".to_string(),
            proposer_did: "did:icn:member:proposer".to_string(),
        };
        
        // Update the policy
        let updated_policy_cid = manager.update_policy(
            &active_cid.unwrap(),
            &fragment,
            federation_did
        ).await?;
        
        // Record votes
        manager.record_vote("did:icn:member:voter1", &updated_policy_cid, true).await?;
        manager.record_vote("did:icn:member:voter2", &updated_policy_cid, true).await?;
        manager.record_vote("did:icn:member:voter3", &updated_policy_cid, false).await?;
        
        // Check quorum
        assert!(manager.has_approval_quorum(&updated_policy_cid).await?);
        
        // Activate the updated policy
        manager.activate_policy(&updated_policy_cid, federation_did).await?;
        
        // Verify it's now the active policy
        let new_active_cid = manager.get_active_policy_cid(federation_did).await?;
        assert_eq!(new_active_cid, Some(updated_policy_cid));
        
        // Load the updated policy and verify changes
        let updated_policy = manager.load_policy(&updated_policy_cid).await?.unwrap();
        assert_eq!(updated_policy.alpha, 0.7);
        assert_eq!(updated_policy.reward_settings.worker_percentage, 75);
        assert_eq!(updated_policy.reward_settings.platform_fee_percentage, 5);
        assert_eq!(updated_policy.min_fee, 20);
        assert_eq!(updated_policy.policy_version, 2);
        
        Ok(())
    }
} 