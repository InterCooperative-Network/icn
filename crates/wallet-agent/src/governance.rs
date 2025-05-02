use serde::{Serialize, Deserialize};
use serde_json::Value;
use wallet_core::identity::IdentityWallet;
use crate::error::{AgentResult, AgentError};
use crate::queue::{ProposalQueue, ActionType};
use crate::agoranet::AgoraNetClient;
use uuid::Uuid;
use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub proposal_id: String,
    pub executed_by: String,
    pub timestamp: i64,
    pub success: bool,
    pub result: Value,
}

pub struct Guardian {
    identity: IdentityWallet,
    queue: ProposalQueue,
    bundle_storage: Option<PathBuf>,
    trusted_bundles: Arc<RwLock<HashMap<String, TrustBundle>>>,
}

impl Guardian {
    pub fn new(identity: IdentityWallet, queue: ProposalQueue) -> Self {
        Self { 
            identity, 
            queue,
            bundle_storage: None,
            trusted_bundles: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn with_bundle_storage<P: AsRef<Path>>(mut self, storage_path: P) -> Self {
        self.bundle_storage = Some(storage_path.as_ref().to_path_buf());
        
        // Ensure storage directory exists
        if let Some(path) = &self.bundle_storage {
            if !path.exists() {
                fs::create_dir_all(path).unwrap_or_else(|_| {
                    eprintln!("WARNING: Failed to create bundle storage directory");
                });
            }
        }
        
        self
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
    
    // New methods for TrustBundle handling
    
    /// Store a trusted bundle in memory and on disk
    pub async fn store_trust_bundle(&self, bundle: TrustBundle) -> AgentResult<()> {
        // Verify the bundle first
        if !self.verify_trust_bundle(&bundle)? {
            return Err(AgentError::GovernanceError(
                format!("Invalid trust bundle: {}", bundle.id)
            ));
        }
        
        // Store in memory
        let mut bundles = self.trusted_bundles.write().await;
        bundles.insert(bundle.id.clone(), bundle.clone());
        
        // Store on disk if storage is configured
        if let Some(storage_path) = &self.bundle_storage {
            let bundle_path = storage_path.join(format!("{}.json", bundle.id));
            
            let content = serde_json::to_string_pretty(&bundle)
                .map_err(|e| AgentError::SerializationError(format!("Failed to serialize bundle: {}", e)))?;
                
            let mut file = File::create(bundle_path)
                .map_err(|e| AgentError::StorageError(format!("Failed to create bundle file: {}", e)))?;
                
            file.write_all(content.as_bytes())
                .map_err(|e| AgentError::StorageError(format!("Failed to write bundle file: {}", e)))?;
        }
        
        Ok(())
    }
    
    /// Get all stored trust bundles
    pub async fn list_trust_bundles(&self) -> AgentResult<Vec<TrustBundle>> {
        let bundles = self.trusted_bundles.read().await;
        Ok(bundles.values().cloned().collect())
    }
    
    /// Check if the identity is a guardian in any active bundle
    pub async fn is_active_guardian(&self) -> AgentResult<bool> {
        let bundles = self.trusted_bundles.read().await;
        
        for bundle in bundles.values() {
            if bundle.active && bundle.guardians.contains(&self.identity.did.to_string()) {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    /// Load trust bundles from the filesystem
    pub async fn load_trust_bundles_from_disk(&self) -> AgentResult<usize> {
        if let Some(storage_path) = &self.bundle_storage {
            if !storage_path.exists() {
                return Ok(0);
            }
            
            let mut count = 0;
            
            let entries = fs::read_dir(storage_path)
                .map_err(|e| AgentError::StorageError(format!("Failed to read bundle directory: {}", e)))?;
                
            for entry in entries {
                let entry = entry
                    .map_err(|e| AgentError::StorageError(format!("Failed to read directory entry: {}", e)))?;
                    
                let path = entry.path();
                
                if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                    let mut file = File::open(&path)
                        .map_err(|e| AgentError::StorageError(format!("Failed to open bundle file: {}", e)))?;
                        
                    let mut content = String::new();
                    file.read_to_string(&mut content)
                        .map_err(|e| AgentError::StorageError(format!("Failed to read bundle file: {}", e)))?;
                        
                    let bundle: TrustBundle = serde_json::from_str(&content)
                        .map_err(|e| AgentError::SerializationError(format!("Failed to parse bundle: {}", e)))?;
                        
                    // Store in memory
                    let mut bundles = self.trusted_bundles.write().await;
                    bundles.insert(bundle.id.clone(), bundle);
                    count += 1;
                }
            }
            
            Ok(count)
        } else {
            Ok(0)
        }
    }
    
    /// Create execution receipt for a proposal
    pub fn create_execution_receipt(&self, proposal_id: &str, result: Value) -> AgentResult<ExecutionReceipt> {
        let receipt = ExecutionReceipt {
            proposal_id: proposal_id.to_string(),
            executed_by: self.identity.did.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            success: true,
            result,
        };
        
        // In a real implementation, you'd also:
        // 1. Sign the receipt cryptographically
        // 2. Store it persistently
        // 3. Perhaps broadcast it to the network
        
        Ok(receipt)
    }
    
    /// Notify AgoraNet about proposal events (if available)
    pub async fn notify_agoranet(&self, agoranet: &AgoraNetClient, proposal_id: &str, event_type: &str, details: Value) -> AgentResult<()> {
        // Use the provided AgoraNet client to notify about the event
        agoranet.notify_proposal_event(proposal_id, event_type, details).await?;
        Ok(())
    }
} 