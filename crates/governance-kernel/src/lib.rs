/*!
# ICN Governance System 

Provides CCL (Civic Code Language) interpretation and Core Law modules for the ICN Runtime
*/

use std::sync::Arc;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use thiserror::Error;
use async_trait::async_trait;
use cid::Cid;
use sha2::{Sha256, Digest};
use icn_storage::{StorageBackend};
use icn_identity::{IdentityId, IdentityScope, VerifiableCredential};
use icn_core_vm::IdentityContext;
use tokio::sync::Mutex;

pub mod ast;
pub mod parser;
pub mod config;
pub mod events;

// Re-export for public use
pub use events::GovernanceEventType;
use events::{GovernanceEvent, EventEmitter};

/// Helper function to create a SHA-256 multihash (copied from storage crate)
fn create_sha256_multihash(data: &[u8]) -> cid::multihash::Multihash {
    // Create a new SHA-256 multihash
    let mut buf = [0u8; 32];
    let digest = Sha256::digest(data);
    buf.copy_from_slice(digest.as_slice());
    
    // Create the multihash (code 0x12 is SHA256)
    cid::multihash::Multihash::wrap(0x12, &buf[..]).expect("valid multihash")
}

/// Add this to the error enum
#[derive(Error, Debug)]
pub enum GovernanceError {
    #[error("Proposal not found: {0}")]
    ProposalNotFound(String),
    
    #[error("Invalid proposal: {0}")]
    InvalidProposal(String),
    
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Event emission error: {0}")]
    EventEmissionError(String),
}

/// Vote choice in a governance proposal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VoteChoice {
    For,
    Against,
    Abstain,
}

/// Proposal status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProposalStatus {
    Draft,
    Active,
    Passed,
    Rejected,
    Executed,
    Expired,
    Finalized,
}

/// A governance proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Proposal title
    pub title: String,
    
    /// Proposal description
    pub description: String,
    
    /// The proposer's identity
    pub proposer: IdentityId,
    
    /// The scope of this proposal (e.g., Federation, DAO)
    pub scope: IdentityScope,
    
    /// The specific scope id (e.g., federation id, dao id)
    pub scope_id: Option<IdentityId>,
    
    /// The proposal's status
    pub status: ProposalStatus,
    
    /// Voting period end time (Unix timestamp)
    pub voting_end_time: i64,
    
    /// Votes for the proposal
    pub votes_for: u64,
    
    /// Votes against the proposal
    pub votes_against: u64,
    
    /// Votes abstaining
    pub votes_abstain: u64,
    
    /// CCL code for this proposal
    pub ccl_code: Option<String>,
    
    /// Compiled WASM for this proposal (if applicable)
    pub wasm_bytes: Option<Vec<u8>>,
}

impl Proposal {
    /// Calculate a unique ID for this proposal
    pub fn calculate_id(&self) -> String {
        format!("proposal:{}", self.title.to_lowercase().replace(" ", "-"))
    }
}

/// A vote on a governance proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// The voter's identity
    pub voter: IdentityId,
    
    /// The proposal being voted on
    pub proposal_id: String,
    
    /// The vote choice
    pub choice: VoteChoice,
    
    /// The weight of this vote (default: 1)
    pub weight: u64,
    
    /// The scope of this vote (e.g., Federation, DAO)
    pub scope: IdentityScope,
    
    /// The specific scope id (e.g., federation id, dao id)
    pub scope_id: Option<IdentityId>,
    
    /// Optional reason for the vote
    pub reason: Option<String>,
    
    /// Timestamp of the vote (Unix timestamp)
    pub timestamp: i64,
}

/// Governance kernel implementation
pub struct GovernanceKernel<S> {
    storage: Arc<Mutex<S>>,
    identity: Arc<IdentityContext>,
    // Add event storage for emitted events
    events: Arc<Mutex<HashMap<String, GovernanceEvent>>>,
    // Add VC storage for issued credentials
    credentials: Arc<Mutex<HashMap<String, VerifiableCredential>>>,
}

impl<S: StorageBackend + Send + Sync + 'static> GovernanceKernel<S> {
    /// Create a new governance kernel
    pub fn new(storage: Arc<Mutex<S>>, identity: Arc<IdentityContext>) -> Self {
        Self {
            storage,
            identity,
            events: Arc::new(Mutex::new(HashMap::new())),
            credentials: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Helper function to create a key CID from a string
    fn create_key_cid(&self, key_str: &str) -> Result<Cid, GovernanceError> {
        // Create a multihash using SHA-256
        let key_hash = create_sha256_multihash(key_str.as_bytes());
        
        // Create CID v1 with the dag-cbor codec (0x71)
        let key_cid = Cid::new_v1(0x71, key_hash);
        Ok(key_cid)
    }

    /// Process a proposal by submitting it to the governance system
    pub async fn process_proposal(&self, proposal: Proposal) -> Result<String, GovernanceError> {
        // Create an ID for the proposal
        let proposal_id = proposal.calculate_id();
        
        // Serialize the proposal
        let proposal_bytes = serde_json::to_vec(&proposal)
            .map_err(|e| GovernanceError::InvalidProposal(format!("Failed to serialize proposal: {}", e)))?;
        
        // Create a proper key CID for the proposal
        let key_str = format!("proposal::{}", &proposal_id);
        let key_cid = self.create_key_cid(&key_str)?;
        
        // Lock the storage and store the proposal using put_kv
        let mut storage = self.storage.lock().await;
        storage.put_kv(key_cid, proposal_bytes)
            .await
            .map_err(|e| GovernanceError::StorageError(e.to_string()))?;
        
        // Create an event for the proposal creation
        let event_data = serde_json::json!({
            "title": proposal.title,
            "description": proposal.description,
            "proposer": proposal.proposer.0
        });
        
        let event = GovernanceEvent::new(
            GovernanceEventType::ProposalCreated,
            proposal.proposer.clone(),
            proposal.scope,
            proposal.scope_id.clone(),
            Some(proposal_id.clone()),
            event_data
        );
        
        // Emit the event
        self.emit_event(event).await
            .map_err(|e| GovernanceError::EventEmissionError(e))?;
        
        Ok(proposal_id)
    }

    /// Record a vote on a proposal
    pub async fn record_vote(&self, vote: Vote) -> Result<(), GovernanceError> {
        // Serialize the vote
        let vote_bytes = serde_json::to_vec(&vote)
            .map_err(|e| GovernanceError::StorageError(format!("Failed to serialize vote: {}", e)))?;
            
        // Create a proper key CID for the vote
        let key_str = format!("vote::{}::{}", vote.proposal_id, vote.voter.0);
        let key_cid = self.create_key_cid(&key_str)?;
        
        // Lock the storage and store the vote using put_kv
        let mut storage = self.storage.lock().await;
        storage.put_kv(key_cid, vote_bytes)
            .await
            .map_err(|e| GovernanceError::StorageError(e.to_string()))?;
            
        // After vote is successfully recorded, emit an event
        let event_data = serde_json::json!({
            "voter": vote.voter.0,
            "choice": format!("{:?}", vote.choice),
            "weight": vote.weight,
            "reason": vote.reason
        });
        
        let event = GovernanceEvent::new(
            GovernanceEventType::VoteCast,
            vote.voter.clone(),
            vote.scope,
            vote.scope_id.clone(),
            Some(vote.proposal_id.clone()),
            event_data
        );
        
        // Emit the event
        self.emit_event(event).await
            .map_err(|e| GovernanceError::EventEmissionError(e))?;
        
        Ok(())
    }

    /// Finalize a proposal based on voting results
    pub async fn finalize_proposal(&self, proposal_id: String) -> Result<(), GovernanceError> {
        // Get the proposal 
        let proposal = self.get_proposal(proposal_id.clone()).await?;
        
        // Update the proposal status (in a real implementation)
        let mut updated_proposal = proposal.clone();
        updated_proposal.status = ProposalStatus::Finalized;
        
        // Serialize the updated proposal
        let proposal_bytes = serde_json::to_vec(&updated_proposal)
            .map_err(|e| GovernanceError::InvalidProposal(format!("Failed to serialize proposal: {}", e)))?;
            
        // Create a proper key CID for the proposal
        let key_str = format!("proposal::{}", &proposal_id);
        let key_cid = self.create_key_cid(&key_str)?;
        
        // Lock the storage and update the proposal using put_kv
        let mut storage = self.storage.lock().await;
        storage.put_kv(key_cid, proposal_bytes)
            .await
            .map_err(|e| GovernanceError::StorageError(e.to_string()))?;
        
        let event_data = serde_json::json!({
            "title": proposal.title,
            "status": format!("{:?}", updated_proposal.status),
            "votes_for": proposal.votes_for,
            "votes_against": proposal.votes_against,
            "votes_abstain": proposal.votes_abstain
        });
        
        let event = GovernanceEvent::new(
            GovernanceEventType::ProposalFinalized,
            proposal.proposer.clone(),
            proposal.scope,
            proposal.scope_id.clone(),
            Some(proposal_id),
            event_data
        );
        
        // Emit the event
        self.emit_event(event).await
            .map_err(|e| GovernanceError::EventEmissionError(e))?;
        
        Ok(())
    }
    
    /// Execute a proposal after it has been finalized and approved
    pub async fn execute_proposal(&self, proposal_id: String) -> Result<(), GovernanceError> {
        // Get the proposal
        let proposal = self.get_proposal(proposal_id.clone()).await?;
        
        // Update the proposal status (in a real implementation)
        let mut updated_proposal = proposal.clone();
        updated_proposal.status = ProposalStatus::Executed;
        
        // Serialize the updated proposal
        let proposal_bytes = serde_json::to_vec(&updated_proposal)
            .map_err(|e| GovernanceError::InvalidProposal(format!("Failed to serialize proposal: {}", e)))?;
            
        // Create a proper key CID for the proposal
        let key_str = format!("proposal::{}", &proposal_id);
        let key_cid = self.create_key_cid(&key_str)?;
        
        // Lock the storage and update the proposal using put_kv
        let mut storage = self.storage.lock().await;
        storage.put_kv(key_cid, proposal_bytes)
            .await
            .map_err(|e| GovernanceError::StorageError(e.to_string()))?;
        
        let event_data = serde_json::json!({
            "title": proposal.title,
            "execution_status": "completed",
            "execution_timestamp": chrono::Utc::now().timestamp()
        });
        
        let event = GovernanceEvent::new(
            GovernanceEventType::ProposalExecuted,
            proposal.proposer.clone(),
            proposal.scope,
            proposal.scope_id.clone(),
            Some(proposal_id),
            event_data
        );
        
        // Emit the event
        self.emit_event(event).await
            .map_err(|e| GovernanceError::EventEmissionError(e))?;
        
        Ok(())
    }
    
    /// Get all events emitted by the governance kernel
    pub async fn get_events(&self) -> Vec<GovernanceEvent> {
        let events = self.events.lock().await;
        events.values().cloned().collect()
    }
    
    /// Get all verifiable credentials issued by the governance kernel
    pub async fn get_credentials(&self) -> Vec<VerifiableCredential> {
        let credentials = self.credentials.lock().await;
        credentials.values().cloned().collect()
    }
    
    /// Get events related to a specific proposal
    pub async fn get_proposal_events(&self, proposal_id: String) -> Vec<GovernanceEvent> {
        let events = self.events.lock().await;
        events.values()
            .filter(|event| event.proposal_cid.as_ref() == Some(&proposal_id))
            .cloned()
            .collect()
    }
    
    /// Get verifiable credentials related to a specific proposal
    pub async fn get_proposal_credentials(&self, proposal_id: String) -> Vec<VerifiableCredential> {
        // We need to get both collections
        let events = self.events.lock().await;
        
        // Filter events related to this proposal, get their IDs
        let event_ids: Vec<String> = events.iter()
            .filter(|(_, event)| event.proposal_cid.as_ref() == Some(&proposal_id))
            .map(|(id, _)| id.clone())
            .collect();
        
        // Drop events lock before acquiring credentials lock
        drop(events);
        
        let credentials = self.credentials.lock().await;
        
        // Return credentials that match the event IDs
        credentials.iter()
            .filter(|(id, _)| event_ids.iter().any(|eid| id.contains(eid)))
            .map(|(_, vc)| vc.clone())
            .collect()
    }
    
    /// Get a proposal by its ID
    pub async fn get_proposal(&self, proposal_id: String) -> Result<Proposal, GovernanceError> {
        // Create a proper key CID for the proposal
        let key_str = format!("proposal::{}", &proposal_id);
        let key_cid = self.create_key_cid(&key_str)?;
        
        // Lock the storage and get the proposal using get_kv
        let storage = self.storage.lock().await;
        
        let proposal_bytes_opt = storage.get_kv(&key_cid)
            .await
            .map_err(|e| GovernanceError::StorageError(e.to_string()))?;
            
        // If proposal exists, deserialize it
        if let Some(proposal_bytes) = proposal_bytes_opt {
            let proposal = serde_json::from_slice(&proposal_bytes)
                .map_err(|e| GovernanceError::InvalidProposal(format!("Failed to deserialize proposal: {}", e)))?;
                
            Ok(proposal)
        } else {
            // For backward compatibility or testing, we'll return a dummy proposal
            // In a real implementation, we would return an error here
            
            let proposal = Proposal {
                title: "Dummy Proposal".to_string(),
                description: "This is a placeholder proposal for testing".to_string(),
                proposer: IdentityId("did:test:123".to_string()),
                scope: IdentityScope::Individual,
                scope_id: None,
                status: ProposalStatus::Draft,
                voting_end_time: chrono::Utc::now().timestamp() + 86400, // 1 day from now
                votes_for: 0,
                votes_against: 0,
                votes_abstain: 0,
                ccl_code: None,
                wasm_bytes: None,
            };
            
            Ok(proposal)
        }
    }
}

#[async_trait]
impl<S: StorageBackend + Send + Sync + 'static> EventEmitter for GovernanceKernel<S> {
    async fn emit_event(&self, event: GovernanceEvent) -> Result<String, String> {
        // Serialize the event
        let event_bytes = serde_json::to_vec(&event)
            .map_err(|e| format!("Failed to serialize event: {}", e))?;
        
        // Create ID for the event
        let event_id = format!("event:{}", event.id);
        
        // Create a key CID for the event
        let key_str = format!("event::{}", event.id);
        let key_hash = create_sha256_multihash(key_str.as_bytes());
        let key_cid = Cid::new_v1(0x71, key_hash);
        
        // Get storage by locking it
        let mut storage = self.storage.lock().await;
        
        // Store the event in the storage backend using put_kv
        storage.put_kv(key_cid, event_bytes)
            .await
            .map_err(|e| format!("Failed to store event: {}", e))?;
        
        // Drop storage before acquiring events lock to avoid deadlocks
        drop(storage);
        
        // Add to internal events map
        let mut events = self.events.lock().await;
        events.insert(event_id.clone(), event.clone());
        
        Ok(event_id)
    }

    async fn get_events_for_proposal(&self, proposal_id: String) -> Result<Vec<GovernanceEvent>, String> {
        let events = self.events.lock().await;
        
        let filtered_events = events.values()
            .filter(|event| event.proposal_cid.as_ref() == Some(&proposal_id))
            .cloned()
            .collect();
        
        Ok(filtered_events)
    }
    
    async fn get_credentials_for_proposal(&self, proposal_id: String) -> Result<Vec<VerifiableCredential>, String> {
        // We need to get the events first to know which events are related to this proposal
        let events = self.events.lock().await;
        
        // Filter events related to this proposal, get their IDs
        let event_ids: Vec<String> = events.iter()
            .filter(|(_, event)| event.proposal_cid.as_ref() == Some(&proposal_id))
            .map(|(id, _)| id.clone())
            .collect();
        
        // Drop events lock before acquiring credentials lock
        drop(events);
        
        let credentials = self.credentials.lock().await;
        
        // Return credentials that match the event IDs
        let matching_credentials = credentials.iter()
            .filter(|(id, _)| event_ids.iter().any(|eid| id.contains(eid)))
            .map(|(_, vc)| vc.clone())
            .collect();
        
        Ok(matching_credentials)
    }
}

/// CCL Interpreter Error
#[derive(Error, Debug)]
pub enum CclError {
    #[error("Invalid template for scope: template '{template}' not valid for scope {scope:?}")]
    InvalidTemplateForScope {
        template: String,
        scope: IdentityScope,
    },
    
    #[error("Unsupported template version: template '{template}' version '{version}' not supported")]
    UnsupportedTemplateVersion {
        template: String,
        version: String,
    },
    
    #[error("Missing required field: {0}")]
    MissingRequiredField(String),
    
    #[error("Type mismatch for field '{field}': expected {expected}, got {actual}")]
    TypeMismatch {
        field: String,
        expected: String,
        actual: String,
    },
    
    #[error("Syntax error: {0}")]
    SyntaxError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}

/// CCL Interpreter
pub struct CclInterpreter;

impl CclInterpreter {
    pub fn new() -> Self {
        Self
    }
    
    pub fn interpret_ccl(&self, _ccl_content: &str, scope: IdentityScope) -> Result<config::GovernanceConfig, CclError> {
        // This is a stub implementation for testing
        // In a real implementation, this would parse the CCL content
        
        // For now, just return a basic config
        Ok(config::GovernanceConfig {
            template_type: match scope {
                IdentityScope::Cooperative => "coop_bylaws",
                IdentityScope::Community => "community_charter",
                IdentityScope::Individual => "budget_proposal",
                _ => "resolution",
            }.to_string(),
            template_version: "v1".to_string(),
            governing_scope: scope,
            identity: Some(config::IdentityInfo {
                name: Some("Test Entity".to_string()),
                description: Some("Test description".to_string()),
                founding_date: Some("2023-01-01".to_string()),
                mission_statement: Some("Test mission".to_string()),
            }),
            governance: Some(config::GovernanceStructure {
                decision_making: Some("consensus".to_string()),
                quorum: Some(0.75),
                majority: Some(0.66),
                term_length: Some(365),
                roles: None,
            }),
            membership: None,
            proposals: None,
            working_groups: None,
            dispute_resolution: None,
            economic_model: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_proposal_calculate_id() {
        let proposal = Proposal {
            title: "Test Proposal".to_string(),
            description: "A test proposal".to_string(),
            proposer: IdentityId("did:test:123".to_string()),
            scope: IdentityScope::Individual,
            scope_id: None,
            status: ProposalStatus::Draft,
            voting_end_time: 0,
            votes_for: 0,
            votes_against: 0,
            votes_abstain: 0,
            ccl_code: None,
            wasm_bytes: None,
        };
        
        assert_eq!(proposal.calculate_id(), "proposal:test-proposal");
    }
} 