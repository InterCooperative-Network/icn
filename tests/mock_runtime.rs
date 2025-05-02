use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use serde_json::{Value, json};
use uuid::Uuid;
use anyhow::{Result, anyhow};

/// Mock implementation of the ICN Runtime for testing
pub struct MockRuntime {
    proposals: Mutex<HashMap<String, Value>>,
    votes: Mutex<HashMap<String, Vec<Value>>>,
    executed: Mutex<HashSet<String>>,
    trust_bundles: Mutex<Vec<Value>>,
    guardians: Mutex<HashSet<String>>,
}

impl MockRuntime {
    pub fn new() -> Self {
        let mut runtime = Self {
            proposals: Mutex::new(HashMap::new()),
            votes: Mutex::new(HashMap::new()),
            executed: Mutex::new(HashSet::new()),
            trust_bundles: Mutex::new(Vec::new()),
            guardians: Mutex::new(HashSet::new()),
        };
        
        // Add some initial guardians
        {
            let mut guardians = runtime.guardians.lock().unwrap();
            guardians.insert("did:icn:guardian1".to_string());
            guardians.insert("did:icn:guardian2".to_string());
            guardians.insert("did:icn:guardian3".to_string());
        }
        
        // Add initial trust bundle
        {
            let mut trust_bundles = runtime.trust_bundles.lock().unwrap();
            trust_bundles.push(json!({
                "id": "bundle1",
                "name": "Initial Trust Bundle",
                "version": 1,
                "guardians": [
                    "did:icn:guardian1",
                    "did:icn:guardian2",
                    "did:icn:guardian3"
                ],
                "threshold": 2,
                "active": true
            }));
        }
        
        runtime
    }
    
    /// Handle a new proposal
    pub fn handle_proposal(&self, proposal: Value) -> Result<String> {
        // Generate a UUID for the proposal
        let id = Uuid::new_v4().to_string();
        
        // Store the proposal
        let mut proposals = self.proposals.lock().unwrap();
        proposals.insert(id.clone(), proposal);
        
        // Initialize votes for this proposal
        let mut votes = self.votes.lock().unwrap();
        votes.insert(id.clone(), Vec::new());
        
        Ok(id)
    }
    
    /// Handle a vote on a proposal
    pub fn handle_vote(&self, vote: Value) -> Result<()> {
        let proposal_id = vote["proposal_id"].as_str()
            .ok_or_else(|| anyhow!("Missing proposal_id in vote"))?;
        
        // Verify the proposal exists
        let proposals = self.proposals.lock().unwrap();
        if !proposals.contains_key(proposal_id) {
            return Err(anyhow!("Proposal not found: {}", proposal_id));
        }
        
        // Add the vote
        let mut votes = self.votes.lock().unwrap();
        let proposal_votes = votes.get_mut(proposal_id)
            .ok_or_else(|| anyhow!("Vote tracking not initialized for proposal: {}", proposal_id))?;
            
        proposal_votes.push(vote);
        
        Ok(())
    }
    
    /// Execute a proposal if it has sufficient votes
    pub fn handle_execute(&self, id: &str) -> Result<Value> {
        // Check if the proposal exists
        let proposals = self.proposals.lock().unwrap();
        let proposal = proposals.get(id)
            .ok_or_else(|| anyhow!("Proposal not found: {}", id))?;
            
        // Check if already executed
        {
            let executed = self.executed.lock().unwrap();
            if executed.contains(id) {
                return Err(anyhow!("Proposal already executed: {}", id));
            }
        }
        
        // Check votes
        let votes = self.votes.lock().unwrap();
        let proposal_votes = votes.get(id)
            .ok_or_else(|| anyhow!("No votes found for proposal: {}", id))?;
            
        // Count approve votes (simplified logic)
        let approve_count = proposal_votes.iter()
            .filter(|v| v["decision"].as_str().unwrap_or("") == "Approve")
            .count();
            
        let reject_count = proposal_votes.iter()
            .filter(|v| v["decision"].as_str().unwrap_or("") == "Reject")
            .count();
            
        // Require at least 2 votes and more approvals than rejections
        if proposal_votes.len() < 2 || approve_count <= reject_count {
            return Err(anyhow!("Insufficient votes to execute proposal: {}", id));
        }
        
        // Mark as executed
        {
            let mut executed = self.executed.lock().unwrap();
            executed.insert(id.to_string());
        }
        
        // Create execution result
        let result = json!({
            "proposal_id": id,
            "success": true,
            "executed_at": chrono::Utc::now().to_rfc3339(),
            "vote_count": {
                "approve": approve_count,
                "reject": reject_count,
                "abstain": proposal_votes.len() - approve_count - reject_count
            }
        });
        
        Ok(result)
    }
    
    /// Get trust bundles
    pub fn get_trust_bundles(&self) -> Vec<Value> {
        let trust_bundles = self.trust_bundles.lock().unwrap();
        trust_bundles.clone()
    }
    
    /// Check if a DID is a guardian
    pub fn is_guardian(&self, did: &str) -> bool {
        let guardians = self.guardians.lock().unwrap();
        guardians.contains(did)
    }
    
    /// Add a guardian
    pub fn add_guardian(&self, did: &str) -> Result<()> {
        let mut guardians = self.guardians.lock().unwrap();
        guardians.insert(did.to_string());
        Ok(())
    }
    
    /// Create a new trust bundle
    pub fn create_trust_bundle(&self, bundle: Value) -> Result<()> {
        let mut trust_bundles = self.trust_bundles.lock().unwrap();
        trust_bundles.push(bundle);
        Ok(())
    }
    
    /// Get all proposals
    pub fn get_proposals(&self) -> HashMap<String, Value> {
        let proposals = self.proposals.lock().unwrap();
        proposals.clone()
    }
    
    /// Get votes for a proposal
    pub fn get_votes(&self, proposal_id: &str) -> Option<Vec<Value>> {
        let votes = self.votes.lock().unwrap();
        votes.get(proposal_id).cloned()
    }
}

/// Create a shared instance for testing
pub fn create_test_runtime() -> Arc<MockRuntime> {
    Arc::new(MockRuntime::new())
} 