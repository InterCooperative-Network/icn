use serde::{Serialize, Deserialize};
use serde_json::Value;
use wallet_core::identity::IdentityWallet;
use crate::error::{AgentResult, AgentError};
use crate::queue::{ProposalQueue, ActionType};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteDecision {
    Approve,
    Reject,
    Abstain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalVote {
    pub proposal_id: String,
    pub decision: VoteDecision,
    pub reason: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustBundle {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub guardians: Vec<String>,
    pub threshold: usize,
    pub active: bool,
}

pub struct Guardian {
    identity: IdentityWallet,
    queue: ProposalQueue,
}

impl Guardian {
    pub fn new(identity: IdentityWallet, queue: ProposalQueue) -> Self {
        Self { identity, queue }
    }
    
    pub fn create_vote(&self, proposal_id: &str, decision: VoteDecision, reason: Option<String>) -> AgentResult<String> {
        let timestamp = chrono::Utc::now().timestamp();
        
        let vote = ProposalVote {
            proposal_id: proposal_id.to_string(),
            decision,
            reason,
            timestamp,
        };
        
        let action = self.queue.queue_action(
            ActionType::Vote, 
            serde_json::to_value(&vote).map_err(|e| {
                AgentError::SerializationError(format!("Failed to serialize vote: {}", e))
            })?
        )?;
        
        // Automatically sign the vote
        let signed_action = self.queue.sign_action(&action.id)?;
        
        Ok(signed_action.id)
    }
    
    pub fn appeal_mandate(&self, mandate_id: &str, reason: String) -> AgentResult<String> {
        let appeal = serde_json::json!({
            "mandate_id": mandate_id,
            "reason": reason,
            "timestamp": chrono::Utc::now().timestamp(),
            "appealer": self.identity.did.to_string(),
        });
        
        let action = self.queue.queue_action(ActionType::Appeal, appeal)?;
        let signed_action = self.queue.sign_action(&action.id)?;
        
        Ok(signed_action.id)
    }
    
    pub fn verify_trust_bundle(&self, bundle: &TrustBundle) -> AgentResult<bool> {
        // In a real implementation, this would verify the bundle against 
        // a trusted source or validate cryptographic proofs
        
        // For now, we'll simply check that it has sufficient guardians
        if bundle.guardians.len() < bundle.threshold {
            return Ok(false);
        }
        
        // And that it's active
        if !bundle.active {
            return Ok(false);
        }
        
        Ok(true)
    }
    
    pub fn create_proposal(&self, proposal_type: &str, content: Value) -> AgentResult<String> {
        let proposal = serde_json::json!({
            "type": proposal_type,
            "content": content,
            "proposer": self.identity.did.to_string(),
            "timestamp": chrono::Utc::now().timestamp(),
            "id": Uuid::new_v4().to_string(),
        });
        
        let action = self.queue.queue_action(ActionType::Proposal, proposal)?;
        let signed_action = self.queue.sign_action(&action.id)?;
        
        Ok(signed_action.id)
    }
    
    pub fn list_pending_votes(&self) -> AgentResult<Vec<ProposalVote>> {
        let actions = self.queue.list_actions(Some(ActionType::Vote))?;
        
        let mut votes = Vec::new();
        for action in actions {
            if let Ok(vote) = serde_json::from_value::<ProposalVote>(action.payload.clone()) {
                votes.push(vote);
            }
        }
        
        Ok(votes)
    }
} 