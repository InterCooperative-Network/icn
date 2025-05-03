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
use uuid::Uuid;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Role Assignment Credential for verifiable role-based permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignmentCredential {
    /// Standard VC fields
    pub issuer: IdentityId,            // Who issued this role assignment (typically a guardian)
    pub subject: IdentityId,           // Who received the role
    pub issuance_date: i64,            // When the role was assigned
    pub expiration_date: Option<i64>,  // Optional expiration

    /// Role-specific fields
    pub scope_id: String,              // The scope context for this role (e.g., cooperative ID)
    pub scope_type: IdentityScope,     // Type of scope (Cooperative, Community, etc.)
    pub roles: Vec<String>,            // Assigned roles
    
    /// Proof
    pub proof: SignatureProof,        // Cryptographic proof of the credential
}

/// Signature proof for a credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureProof {
    pub signature_type: String,       // e.g., "Ed25519Signature2020"
    pub signature_value: String,      // Base64-encoded signature
    pub created: i64,                 // When the signature was created
    pub verification_method: String,  // The public key ID used to verify
    pub purpose: String,              // e.g., "assertionMethod"
}

/// Options for role assignment
#[derive(Debug, Clone, Default)]
pub struct RoleAssignmentOptions {
    pub expiration_days: Option<i64>,
    pub scope_type: Option<IdentityScope>,
    pub store_in_dag: bool,
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

    /// Load the governance configuration for a given scope
    async fn load_governance_config(&self, scope_id: &str) -> Result<Option<config::GovernanceConfig>, GovernanceError> {
        // Create the key for the governance config based on scope
        let key_str = format!("governance::config::{}", scope_id);
        let key_cid = self.create_key_cid(&key_str)?;
        
        // Get the storage lock
        let storage = self.storage.lock().await;
        
        // Try to load the governance config
        match storage.get_kv(&key_cid).await {
            Ok(Some(config_bytes)) => {
                // Deserialize the config
                let config = serde_json::from_slice(&config_bytes)
                    .map_err(|e| GovernanceError::StorageError(format!("Failed to deserialize governance config: {}", e)))?;
                
                Ok(Some(config))
            },
            Ok(None) => {
                // No config found for this scope
                Ok(None)
            },
            Err(e) => {
                // Storage error
                Err(GovernanceError::StorageError(format!("Failed to load governance config: {}", e)))
            }
        }
    }
    
    /// Check if caller has a specific permission according to governance config
    async fn check_permission(&self, caller_id: &IdentityId, scope_id: &str, permission: &str) -> Result<bool, GovernanceError> {
        // Load the governance config
        let config_opt = self.load_governance_config(scope_id).await?;
        
        if let Some(config) = config_opt {
            // Check if there are roles defined in the governance config
            if let Some(governance) = &config.governance {
                if let Some(defined_roles) = &governance.roles {
                    // Get the verified roles assigned to this identity
                    let assigned_role_names = self.get_verified_roles(caller_id, scope_id).await?;
                    
                    // If no roles are assigned, the caller doesn't have permission
                    if assigned_role_names.is_empty() {
                        return Ok(false);
                    }
                    
                    // Check if any assigned roles have the required permission
                    for role in defined_roles {
                        if assigned_role_names.contains(&role.name) && role.permissions.contains(&permission.to_string()) {
                            return Ok(true);
                        }
                    }
                }
            }
            
            // No applicable permission found
            Ok(false)
        } else {
            // No governance config found
            Err(GovernanceError::Unauthorized(format!("No governance configuration found for scope {}", scope_id)))
        }
    }

    /// Process a proposal by submitting it to the governance system
    pub async fn process_proposal(&self, proposal: Proposal) -> Result<String, GovernanceError> {
        // Get the scope_id string for authorization check
        let scope_id_str = if let Some(sid) = &proposal.scope_id {
            sid.0.as_str()
        } else {
            return Err(GovernanceError::InvalidProposal("Proposal must have a scope_id".to_string()));
        };
        
        // Check if the proposer has permission to create proposals in this scope
        let is_authorized = self.check_permission(&proposal.proposer, scope_id_str, "create_proposals").await?;
        
        if !is_authorized {
            return Err(GovernanceError::Unauthorized(format!(
                "Identity {} is not authorized to create proposals in scope {}", 
                proposal.proposer.0, scope_id_str
            )));
        }
        
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
        // Get the scope_id string for authorization check
        let scope_id_str = if let Some(sid) = &vote.scope_id {
            sid.0.as_str()
        } else {
            return Err(GovernanceError::InvalidProposal("Vote must have a scope_id".to_string()));
        };
        
        // Check if the voter has permission to vote on proposals in this scope
        let is_authorized = self.check_permission(&vote.voter, scope_id_str, "vote_on_proposals").await?;
        
        if !is_authorized {
            return Err(GovernanceError::Unauthorized(format!(
                "Identity {} is not authorized to vote on proposals in scope {}", 
                vote.voter.0, scope_id_str
            )));
        }
        
        // Also check if the proposal exists and is in a votable state
        let proposal = self.get_proposal(vote.proposal_id.clone()).await?;
        if proposal.status != ProposalStatus::Active && proposal.status != ProposalStatus::Draft {
            return Err(GovernanceError::InvalidProposal(format!(
                "Cannot vote on proposal with status {:?}", proposal.status
            )));
        }
        
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

    /// Assign roles to an identity with verifiable credentials
    pub async fn assign_roles(
        &self,
        subject_id: &IdentityId,
        scope_id: &str,
        roles: Vec<String>,
        options: Option<RoleAssignmentOptions>,
    ) -> Result<String, GovernanceError> {
        // Get the default options or use provided ones
        let options = options.unwrap_or_default();
        
        // Get the governance config to ensure roles exist
        let config_opt = self.load_governance_config(scope_id).await?;
        let config = config_opt.ok_or_else(|| {
            GovernanceError::InvalidProposal(format!("No governance config found for scope {}", scope_id))
        })?;
        
        // Validate that all roles exist in the governance config
        if let Some(governance) = &config.governance {
            if let Some(defined_roles) = &governance.roles {
                for role in &roles {
                    if !defined_roles.iter().any(|r| &r.name == role) {
                        return Err(GovernanceError::InvalidProposal(
                            format!("Role '{}' does not exist in governance config", role)
                        ));
                    }
                }
            } else {
                return Err(GovernanceError::InvalidProposal(
                    "No roles defined in governance config".to_string()
                ));
            }
        } else {
            return Err(GovernanceError::InvalidProposal(
                "Governance structure not defined in config".to_string()
            ));
        }
        
        // Create a unique credential ID
        let credential_id = format!("credential:role:{}:{}:{}", 
            scope_id, subject_id.0, Uuid::new_v4());
        
        // Create the role assignment credential
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
            
        let expiration = options.expiration_days.map(|days| now + (days * 86400));
        
        let mut credential = RoleAssignmentCredential {
            issuer: IdentityId(self.identity.did().to_string()),
            subject: subject_id.clone(),
            issuance_date: now,
            expiration_date: expiration,
            scope_id: scope_id.to_string(),
            scope_type: options.scope_type.unwrap_or(IdentityScope::Cooperative),
            roles: roles.clone(),
            proof: SignatureProof {
                signature_type: "Ed25519Signature2020".to_string(),
                signature_value: String::new(), // Will be filled after signing
                created: now,
                verification_method: format!("{}#keys-1", self.identity.did()),
                purpose: "assertionMethod".to_string(),
            },
        };
        
        // Create canonical representation for signing (without the signature)
        // For signing, we remove the signature_value to get a stable representation
        let mut signing_credential = credential.clone();
        signing_credential.proof.signature_value = String::new();
        
        let canonical = serde_json::to_string(&signing_credential)
            .map_err(|e| GovernanceError::StorageError(format!("Failed to serialize credential for signing: {}", e)))?;
            
        let canonical_hash = Sha256::digest(canonical.as_bytes());
        
        // Sign the credential using the kernel's identity
        let signature = self.identity.keypair().sign(canonical_hash.as_slice())
            .map_err(|e| GovernanceError::StorageError(format!("Failed to sign credential: {}", e)))?;
        
        // Update the proof with the signature
        credential.proof.signature_value = BASE64.encode(signature);
        
        // Serialize the credential for storage
        let credential_bytes = serde_json::to_vec(&credential)
            .map_err(|e| GovernanceError::StorageError(format!("Failed to serialize credential: {}", e)))?;
        
        // Store the credential
        let storage_key = format!("role_credential::{}::{}::{}", 
            scope_id, subject_id.0, credential_id);
        let key_cid = self.create_key_cid(&storage_key)?;
        
        let storage = self.storage.lock().await;
        storage.put_kv(key_cid, credential_bytes).await
            .map_err(|e| GovernanceError::StorageError(format!("Failed to store credential: {}", e)))?;
        
        // Update the role index for efficient retrieval
        let index_key = format!("role_index::{}::{}", scope_id, subject_id.0);
        let index_cid = self.create_key_cid(&index_key)?;
        
        // Get existing credential IDs from index or create new
        let mut credential_ids = match storage.get_kv(&index_cid).await {
            Ok(Some(bytes)) => {
                serde_json::from_slice::<Vec<String>>(&bytes)
                    .unwrap_or_default()
            },
            _ => Vec::new(),
        };
        
        // Add the new credential ID to the index
        credential_ids.push(credential_id.clone());
        
        // Store the updated index
        let index_bytes = serde_json::to_vec(&credential_ids)
            .map_err(|e| GovernanceError::StorageError(format!("Failed to serialize index: {}", e)))?;
            
        storage.put_kv(index_cid, index_bytes).await
            .map_err(|e| GovernanceError::StorageError(format!("Failed to store index: {}", e)))?;
        
        // Optional: Store in DAG for immutability
        if options.store_in_dag {
            // TODO: Implement DAG storage integration
            // This would involve creating a DAG node with:
            // - operation_type: "RoleAssignment"
            // - payload: credential_id or hash of credential
            // - timestamp: now
            // - previous: [previous relevant node]
            // And then storing it in the DAG system
        }
        
        // Return the credential ID
        Ok(credential_id)
    }

    /// Get the verified assigned roles for an identity within a scope
    pub async fn get_verified_roles(&self, identity_id: &IdentityId, scope_id: &str) -> Result<Vec<String>, GovernanceError> {
        // Get the index of credential IDs for this identity and scope
        let index_key = format!("role_index::{}::{}", scope_id, identity_id.0);
        let index_cid = self.create_key_cid(&index_key)?;
        
        let storage = self.storage.lock().await;
        let credential_ids = match storage.get_kv(&index_cid).await {
            Ok(Some(bytes)) => {
                serde_json::from_slice::<Vec<String>>(&bytes)
                    .map_err(|e| GovernanceError::StorageError(format!("Failed to deserialize role index: {}", e)))?
            },
            _ => Vec::new(),
        };
        
        if credential_ids.is_empty() {
            return Ok(Vec::new());
        }
        
        // Collect all valid roles from all valid credentials
        let mut verified_roles = Vec::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
            
        for credential_id in credential_ids {
            // Retrieve the credential
            let storage_key = format!("role_credential::{}::{}::{}", 
                scope_id, identity_id.0, credential_id);
            let key_cid = self.create_key_cid(&storage_key)?;
            
            if let Ok(Some(bytes)) = storage.get_kv(&key_cid).await {
                if let Ok(credential) = serde_json::from_slice::<RoleAssignmentCredential>(&bytes) {
                    // Check expiration
                    if let Some(expiration) = credential.expiration_date {
                        if expiration < now {
                            // Skip expired credentials
                            continue;
                        }
                    }
                    
                    // Verify the credential signature
                    if self.verify_credential_signature(&credential).await? {
                        // If signature is valid, add roles to the verified list
                        verified_roles.extend(credential.roles);
                    }
                }
            }
        }
        
        // De-duplicate roles
        verified_roles.sort();
        verified_roles.dedup();
        
        Ok(verified_roles)
    }
    
    /// Verify a role assignment credential's signature
    async fn verify_credential_signature(&self, credential: &RoleAssignmentCredential) -> Result<bool, GovernanceError> {
        // Get the issuer's identity information to verify the signature
        let issuer_id = &credential.issuer;
        
        // In a production system, this would involve:
        // 1. Retrieve the issuer's public key from a trusted source
        // 2. Verify that the issuer has the authority to issue role credentials
        
        // For now, we'll do a simpler verification using our local identity service:
        let identity_service = self.identity.clone();
        
        // Create canonical representation for verification (without the signature)
        let mut verification_credential = credential.clone();
        let signature_value = verification_credential.proof.signature_value.clone();
        verification_credential.proof.signature_value = String::new();
        
        let canonical = serde_json::to_string(&verification_credential)
            .map_err(|e| GovernanceError::StorageError(format!("Failed to serialize credential for verification: {}", e)))?;
            
        let canonical_hash = Sha256::digest(canonical.as_bytes());
        
        // Decode the signature
        let signature = BASE64.decode(&signature_value)
            .map_err(|e| GovernanceError::InvalidProposal(format!("Invalid signature encoding: {}", e)))?;
            
        // Verify the signature using the issuer's public key
        // In a real implementation, you'd retrieve the issuer's public key from a trusted registry
        // For now, we're using a simplified approach
        if identity_service.did() == issuer_id.0 {
            // If we are the issuer, verify with our own keypair
            match identity_service.keypair().sign(canonical_hash.as_slice()) {
                Ok(our_signature) => {
                    // In a real implementation, we would cryptographically verify
                    // For now, we'll compare our signature with the stored one
                    Ok(our_signature == signature)
                },
                Err(e) => Err(GovernanceError::StorageError(format!("Signature verification failed: {}", e)))
            }
        } else {
            // For external issuers, we would need to retrieve their public key
            // This is a placeholder for now - in a real implementation, you would:
            // 1. Retrieve the issuer's DID Document
            // 2. Extract the verification method referenced in proof.verification_method
            // 3. Use that public key to verify the signature
            
            // TODO: Implement proper external issuer verification
            // For now, only accept credentials issued by this governance kernel instance
            Err(GovernanceError::InvalidProposal(
                format!("Currently only supporting self-issued credentials, got {}", issuer_id.0)
            ))
        }
    }

    /// Store a governance configuration for a specific scope
    pub async fn store_governance_config(&self, scope_id: &str, config: config::GovernanceConfig) -> Result<(), GovernanceError> {
        // Create the key for storing the governance config
        let key_str = format!("governance::config::{}", scope_id);
        let key_cid = self.create_key_cid(&key_str)?;
        
        // Serialize the config
        let config_bytes = serde_json::to_vec(&config)
            .map_err(|e| GovernanceError::StorageError(format!("Failed to serialize governance config: {}", e)))?;
        
        // Store the config in storage
        let mut storage = self.storage.lock().await;
        storage.put_kv(key_cid, config_bytes)
            .await
            .map_err(|e| GovernanceError::StorageError(e.to_string()))?;
        
        // Emit an event for config update
        let event_data = serde_json::json!({
            "scope_id": scope_id,
            "config_type": config.template_type,
            "timestamp": chrono::Utc::now().timestamp()
        });
        
        let event = GovernanceEvent::new(
            GovernanceEventType::ConfigUpdated,
            IdentityId(format!("did:icn:system:governance")),
            config.governing_scope,
            Some(IdentityId(scope_id.to_string())),
            None,
            event_data
        );
        
        // Emit the event
        self.emit_event(event).await
            .map_err(|e| GovernanceError::EventEmissionError(e))?;
        
        Ok(())
    }

    /// Get the assigned roles for an identity within a scope (for backward compatibility)
    pub async fn get_assigned_roles(&self, identity_id: &IdentityId, scope_id: &str) -> Result<Vec<String>, GovernanceError> {
        self.get_verified_roles(identity_id, scope_id).await
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
    
    pub fn interpret_ccl(&self, ccl_content: &str, scope: IdentityScope) -> Result<config::GovernanceConfig, CclError> {
        // First, parse the CCL content using the parser
        let parse_result = parser::parse_ccl(ccl_content);
        
        if let Err(e) = parse_result {
            return Err(CclError::SyntaxError(e.to_string()));
        }
        
        let ast = parse_result.unwrap();
        
        // Extract template type and version
        let template_string = ast.template_type.clone();
        let mut template_type = template_string.clone();
        let mut template_version = "v1".to_string();
        
        // Check if there's a version in the template type (format: "type:version")
        if let Some(_idx) = template_string.find(':') {
            let parts: Vec<&str> = template_string.split(':').collect();
            if parts.len() == 2 {
                template_type = parts[0].to_string();
                template_version = parts[1].to_string();
            }
        }
        
        // Validate template type against scope
        match (template_type.as_str(), scope) {
            ("community_charter", IdentityScope::Community) => {},
            ("coop_bylaws", IdentityScope::Cooperative) => {},
            ("budget_proposal", _) => {}, // Budget proposals can be used in any scope
            ("resolution", _) => {}, // Resolutions can be used in any scope
            ("participation_rules", _) => {}, // Participation rules can be used in any scope
            _ => {
                if (template_type == "community_charter" && scope != IdentityScope::Community) ||
                   (template_type == "coop_bylaws" && scope != IdentityScope::Cooperative) {
                    return Err(CclError::InvalidTemplateForScope {
                        template: template_type,
                        scope,
                    });
                }
            }
        }
        
        // Validate template version
        if template_version != "v1" && template_version != "v2" {
            return Err(CclError::UnsupportedTemplateVersion {
                template: ast.template_type,
                version: template_version,
            });
        }
        
        // Validate type correctness in the CCL content
        if let ast::CclValue::Object(pairs) = &ast.content {
            for pair in pairs {
                if pair.key == "governance" {
                    if let ast::CclValue::Object(gov_pairs) = &pair.value {
                        for gov_pair in gov_pairs {
                            match gov_pair.key.as_str() {
                                "quorum" => {
                                    if let ast::CclValue::String(_) = &gov_pair.value {
                                        return Err(CclError::TypeMismatch { 
                                            field: "quorum".to_string(), 
                                            expected: "number".to_string(), 
                                            actual: "string".to_string() 
                                        });
                                    }
                                },
                                _ => {}
                            }
                        }
                    }
                } else if pair.key == "membership" {
                    if let ast::CclValue::Object(mem_pairs) = &pair.value {
                        for mem_pair in mem_pairs {
                            if mem_pair.key == "onboarding" {
                                if let ast::CclValue::Object(onb_pairs) = &mem_pair.value {
                                    for onb_pair in onb_pairs {
                                        match onb_pair.key.as_str() {
                                            "trial_period_days" => {
                                                if let ast::CclValue::Boolean(_) = &onb_pair.value {
                                                    return Err(CclError::TypeMismatch { 
                                                        field: "trial_period_days".to_string(), 
                                                        expected: "integer".to_string(),
                                                        actual: "boolean".to_string() 
                                                    });
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Begin building the configuration
        let mut config = config::GovernanceConfig {
            template_type,
            template_version,
            governing_scope: scope,
            identity: None,
            governance: None,
            membership: None,
            proposals: None,
            working_groups: None,
            dispute_resolution: None,
            economic_model: None,
        };
        
        // Process the content object
        match ast.content {
            ast::CclValue::Object(pairs) => {
                // Extract identity information
                let identity_info = self.extract_identity_info(&pairs);
                if identity_info.is_some() {
                    config.identity = identity_info;
                }
                
                // Extract governance structure
                let governance = self.extract_governance_structure(&pairs);
                config.governance = governance;
                
                // Extract membership rules
                let membership = self.extract_membership_rules(&pairs);
                config.membership = membership;
                
                // Extract proposal process
                let proposals = self.extract_proposal_process(&pairs);
                config.proposals = proposals;
                
                // Extract working groups
                let working_groups = self.extract_working_groups(&pairs);
                config.working_groups = working_groups;
                
                // Extract dispute resolution
                let dispute_resolution = self.extract_dispute_resolution(&pairs);
                config.dispute_resolution = dispute_resolution;
                
                // Extract economic model
                let economic_model = self.extract_economic_model(&pairs);
                config.economic_model = economic_model;
                
                // Validate required fields based on template type
                self.validate_required_fields(&config)?;
            },
            _ => {
                return Err(CclError::SyntaxError("Expected object as root content".to_string()));
            }
        }
        
        Ok(config)
    }
    
    // Helper method to extract identity info from CCL pairs
    fn extract_identity_info(&self, pairs: &[ast::CclPair]) -> Option<config::IdentityInfo> {
        let mut name = None;
        let mut description = None;
        let mut founding_date = None;
        let mut mission_statement = None;
        
        for pair in pairs {
            match pair.key.as_str() {
                "name" => {
                    if let ast::CclValue::String(s) = &pair.value {
                        name = Some(s.clone());
                    } else {
                        // Type mismatch, but we'll continue
                    }
                },
                "description" => {
                    if let ast::CclValue::String(s) = &pair.value {
                        description = Some(s.clone());
                    }
                },
                "founding_date" => {
                    if let ast::CclValue::String(s) = &pair.value {
                        founding_date = Some(s.clone());
                    }
                },
                "mission_statement" => {
                    if let ast::CclValue::String(s) = &pair.value {
                        mission_statement = Some(s.clone());
                    }
                },
                _ => {}
            }
        }
        
        if name.is_some() || description.is_some() || founding_date.is_some() || mission_statement.is_some() {
            Some(config::IdentityInfo {
                name,
                description,
                founding_date,
                mission_statement,
            })
        } else {
            None
        }
    }
    
    // Helper method to extract governance structure from CCL pairs
    fn extract_governance_structure(&self, pairs: &[ast::CclPair]) -> Option<config::GovernanceStructure> {
        for pair in pairs {
            if pair.key == "governance" {
                if let ast::CclValue::Object(gov_pairs) = &pair.value {
                    let mut decision_making = None;
                    let mut quorum = None;
                    let mut majority = None;
                    let mut term_length = None;
                    let mut roles = None;
                    
                    for gov_pair in gov_pairs {
                        match gov_pair.key.as_str() {
                            "decision_making" => {
                                if let ast::CclValue::String(s) = &gov_pair.value {
                                    decision_making = Some(s.clone());
                                }
                            },
                            "quorum" => {
                                if let ast::CclValue::Number(n) = &gov_pair.value {
                                    quorum = Some(*n);
                                }
                            },
                            "majority" => {
                                if let ast::CclValue::Number(n) = &gov_pair.value {
                                    majority = Some(*n);
                                }
                            },
                            "term_length" => {
                                if let ast::CclValue::Number(n) = &gov_pair.value {
                                    term_length = Some(*n as u64);
                                }
                            },
                            "roles" => {
                                if let ast::CclValue::Array(role_values) = &gov_pair.value {
                                    let mut role_vec = Vec::new();
                                    
                                    for role_val in role_values {
                                        if let ast::CclValue::Object(role_pairs) = role_val {
                                            let mut role_name = String::new();
                                            let mut permissions = Vec::new();
                                            
                                            for rp in role_pairs {
                                                if rp.key == "name" {
                                                    if let ast::CclValue::String(s) = &rp.value {
                                                        role_name = s.clone();
                                                    }
                                                } else if rp.key == "permissions" {
                                                    if let ast::CclValue::Array(perm_vals) = &rp.value {
                                                        for pv in perm_vals {
                                                            if let ast::CclValue::String(s) = pv {
                                                                permissions.push(s.clone());
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            
                                            if !role_name.is_empty() {
                                                role_vec.push(config::Role {
                                                    name: role_name,
                                                    permissions,
                                                });
                                            }
                                        }
                                    }
                                    
                                    if !role_vec.is_empty() {
                                        roles = Some(role_vec);
                                    }
                                }
                            },
                            _ => {}
                        }
                    }
                    
                    return Some(config::GovernanceStructure {
                        decision_making,
                        quorum,
                        majority,
                        term_length,
                        roles,
                    });
                }
            }
        }
        
        None
    }
    
    // Helper method to extract membership rules from CCL pairs
    fn extract_membership_rules(&self, pairs: &[ast::CclPair]) -> Option<config::MembershipRules> {
        for pair in pairs {
            if pair.key == "membership" {
                if let ast::CclValue::Object(mem_pairs) = &pair.value {
                    let mut onboarding = None;
                    let mut dues = None;
                    let mut offboarding = None;
                    
                    for mem_pair in mem_pairs {
                        match mem_pair.key.as_str() {
                            "onboarding" => {
                                if let ast::CclValue::Object(onb_pairs) = &mem_pair.value {
                                    let mut requires_sponsor = None;
                                    let mut trial_period_days = None;
                                    let mut requirements = None;
                                    
                                    for onb_pair in onb_pairs {
                                        match onb_pair.key.as_str() {
                                            "requires_sponsor" => {
                                                if let ast::CclValue::Boolean(b) = &onb_pair.value {
                                                    requires_sponsor = Some(*b);
                                                }
                                            },
                                            "trial_period_days" => {
                                                if let ast::CclValue::Number(n) = &onb_pair.value {
                                                    trial_period_days = Some(*n as u64);
                                                }
                                            },
                                            "requirements" => {
                                                if let ast::CclValue::Array(req_vals) = &onb_pair.value {
                                                    let mut req_vec = Vec::new();
                                                    
                                                    for rv in req_vals {
                                                        if let ast::CclValue::String(s) = rv {
                                                            req_vec.push(s.clone());
                                                        }
                                                    }
                                                    
                                                    if !req_vec.is_empty() {
                                                        requirements = Some(req_vec);
                                                    }
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                    
                                    onboarding = Some(config::Onboarding {
                                        requires_sponsor,
                                        trial_period_days,
                                        requirements,
                                    });
                                }
                            },
                            "dues" => {
                                if let ast::CclValue::Object(dues_pairs) = &mem_pair.value {
                                    let mut amount = None;
                                    let mut frequency = None;
                                    let mut variable_options = None;
                                    
                                    for dues_pair in dues_pairs {
                                        match dues_pair.key.as_str() {
                                            "amount" => {
                                                if let ast::CclValue::Number(n) = &dues_pair.value {
                                                    amount = Some(*n as u64);
                                                }
                                            },
                                            "frequency" => {
                                                if let ast::CclValue::String(s) = &dues_pair.value {
                                                    frequency = Some(s.clone());
                                                }
                                            },
                                            "variable_options" => {
                                                if let ast::CclValue::Array(opt_vals) = &dues_pair.value {
                                                    let mut opt_vec = Vec::new();
                                                    
                                                    for ov in opt_vals {
                                                        if let ast::CclValue::Object(opt_pairs) = ov {
                                                            let mut opt_amount = 0;
                                                            let mut opt_description = String::new();
                                                            
                                                            for op in opt_pairs {
                                                                if op.key == "amount" {
                                                                    if let ast::CclValue::Number(n) = &op.value {
                                                                        opt_amount = *n as u64;
                                                                    }
                                                                } else if op.key == "description" {
                                                                    if let ast::CclValue::String(s) = &op.value {
                                                                        opt_description = s.clone();
                                                                    }
                                                                }
                                                            }
                                                            
                                                            if !opt_description.is_empty() {
                                                                opt_vec.push(config::DuesOption {
                                                                    amount: opt_amount,
                                                                    description: opt_description,
                                                                });
                                                            }
                                                        }
                                                    }
                                                    
                                                    if !opt_vec.is_empty() {
                                                        variable_options = Some(opt_vec);
                                                    }
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                    
                                    dues = Some(config::Dues {
                                        amount,
                                        frequency,
                                        variable_options,
                                    });
                                }
                            },
                            "offboarding" => {
                                if let ast::CclValue::Object(off_pairs) = &mem_pair.value {
                                    let mut notice_period_days = None;
                                    let mut max_inactive_days = None;
                                    
                                    for off_pair in off_pairs {
                                        match off_pair.key.as_str() {
                                            "notice_period_days" => {
                                                if let ast::CclValue::Number(n) = &off_pair.value {
                                                    notice_period_days = Some(*n as u64);
                                                }
                                            },
                                            "max_inactive_days" => {
                                                if let ast::CclValue::Number(n) = &off_pair.value {
                                                    max_inactive_days = Some(*n as u64);
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                    
                                    offboarding = Some(config::Offboarding {
                                        notice_period_days,
                                        max_inactive_days,
                                    });
                                }
                            },
                            _ => {}
                        }
                    }
                    
                    return Some(config::MembershipRules {
                        onboarding,
                        dues,
                        offboarding,
                    });
                }
            }
        }
        
        None
    }
    
    // Helper method to extract proposal process from CCL pairs
    fn extract_proposal_process(&self, pairs: &[ast::CclPair]) -> Option<config::ProposalProcess> {
        for pair in pairs {
            if pair.key == "proposals" {
                if let ast::CclValue::Object(prop_pairs) = &pair.value {
                    let mut types = None;
                    
                    for prop_pair in prop_pairs {
                        if prop_pair.key == "types" {
                            if let ast::CclValue::Array(type_vals) = &prop_pair.value {
                                let mut type_vec = Vec::new();
                                
                                for tv in type_vals {
                                    if let ast::CclValue::Object(type_pairs) = tv {
                                        let mut name = String::new();
                                        let mut quorum_modifier = None;
                                        let mut majority_modifier = None;
                                        let mut discussion_period_days = None;
                                        
                                        for tp in type_pairs {
                                            match tp.key.as_str() {
                                                "name" => {
                                                    if let ast::CclValue::String(s) = &tp.value {
                                                        name = s.clone();
                                                    }
                                                },
                                                "quorum_modifier" => {
                                                    if let ast::CclValue::Number(n) = &tp.value {
                                                        quorum_modifier = Some(*n);
                                                    }
                                                },
                                                "majority_modifier" => {
                                                    if let ast::CclValue::Number(n) = &tp.value {
                                                        majority_modifier = Some(*n);
                                                    }
                                                },
                                                "discussion_period_days" => {
                                                    if let ast::CclValue::Number(n) = &tp.value {
                                                        discussion_period_days = Some(*n as u64);
                                                    }
                                                },
                                                _ => {}
                                            }
                                        }
                                        
                                        if !name.is_empty() {
                                            type_vec.push(config::ProposalType {
                                                name,
                                                quorum_modifier,
                                                majority_modifier,
                                                discussion_period_days,
                                            });
                                        }
                                    }
                                }
                                
                                if !type_vec.is_empty() {
                                    types = Some(type_vec);
                                }
                            }
                        }
                    }
                    
                    return Some(config::ProposalProcess {
                        types,
                    });
                }
            }
        }
        
        None
    }
    
    // Helper method to extract working groups structure from CCL pairs
    fn extract_working_groups(&self, pairs: &[ast::CclPair]) -> Option<config::WorkingGroups> {
        for pair in pairs {
            if pair.key == "working_groups" {
                if let ast::CclValue::Object(wg_pairs) = &pair.value {
                    let mut formation_threshold = None;
                    let mut dissolution_threshold = None;
                    let mut resource_allocation = None;
                    
                    for wg_pair in wg_pairs {
                        match wg_pair.key.as_str() {
                            "formation_threshold" => {
                                if let ast::CclValue::Number(n) = &wg_pair.value {
                                    formation_threshold = Some(*n as u64);
                                }
                            },
                            "dissolution_threshold" => {
                                if let ast::CclValue::Number(n) = &wg_pair.value {
                                    dissolution_threshold = Some(*n as u64);
                                }
                            },
                            "resource_allocation" => {
                                if let ast::CclValue::Object(ra_pairs) = &wg_pair.value {
                                    let mut default_budget = None;
                                    let mut requires_approval = None;
                                    
                                    for ra_pair in ra_pairs {
                                        match ra_pair.key.as_str() {
                                            "default_budget" => {
                                                if let ast::CclValue::Number(n) = &ra_pair.value {
                                                    default_budget = Some(*n as u64);
                                                }
                                            },
                                            "requires_approval" => {
                                                if let ast::CclValue::Boolean(b) = &ra_pair.value {
                                                    requires_approval = Some(*b);
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                    
                                    resource_allocation = Some(config::ResourceAllocation {
                                        default_budget,
                                        requires_approval,
                                    });
                                }
                            },
                            _ => {}
                        }
                    }
                    
                    return Some(config::WorkingGroups {
                        formation_threshold,
                        dissolution_threshold,
                        resource_allocation,
                    });
                }
            }
        }
        
        None
    }
    
    // Helper method to extract dispute resolution process from CCL pairs
    fn extract_dispute_resolution(&self, pairs: &[ast::CclPair]) -> Option<config::DisputeResolution> {
        for pair in pairs {
            if pair.key == "dispute_resolution" {
                if let ast::CclValue::Object(dr_pairs) = &pair.value {
                    let mut process = None;
                    let mut committee_size = None;
                    
                    for dr_pair in dr_pairs {
                        match dr_pair.key.as_str() {
                            "process" => {
                                if let ast::CclValue::Array(proc_vals) = &dr_pair.value {
                                    let mut proc_vec = Vec::new();
                                    
                                    for pv in proc_vals {
                                        if let ast::CclValue::String(s) = pv {
                                            proc_vec.push(s.clone());
                                        }
                                    }
                                    
                                    if !proc_vec.is_empty() {
                                        process = Some(proc_vec);
                                    }
                                }
                            },
                            "committee_size" => {
                                if let ast::CclValue::Number(n) = &dr_pair.value {
                                    committee_size = Some(*n as u64);
                                }
                            },
                            _ => {}
                        }
                    }
                    
                    return Some(config::DisputeResolution {
                        process,
                        committee_size,
                    });
                }
            }
        }
        
        None
    }
    
    // Helper method to extract economic model from CCL pairs
    fn extract_economic_model(&self, pairs: &[ast::CclPair]) -> Option<config::EconomicModel> {
        for pair in pairs {
            if pair.key == "economic_model" {
                if let ast::CclValue::Object(econ_pairs) = &pair.value {
                    let mut surplus_distribution = None;
                    let mut compensation_policy = None;
                    
                    for econ_pair in econ_pairs {
                        match econ_pair.key.as_str() {
                            "surplus_distribution" => {
                                if let ast::CclValue::String(s) = &econ_pair.value {
                                    surplus_distribution = Some(s.clone());
                                }
                            },
                            "compensation_policy" => {
                                if let ast::CclValue::Object(comp_pairs) = &econ_pair.value {
                                    let mut hourly_rates = None;
                                    let mut track_hours = None;
                                    let mut volunteer_options = None;
                                    
                                    for comp_pair in comp_pairs {
                                        match comp_pair.key.as_str() {
                                            "hourly_rates" => {
                                                if let ast::CclValue::Object(rate_pairs) = &comp_pair.value {
                                                    let mut rates = std::collections::HashMap::new();
                                                    
                                                    for rp in rate_pairs {
                                                        if let ast::CclValue::Number(n) = &rp.value {
                                                            rates.insert(rp.key.clone(), *n as u64);
                                                        }
                                                    }
                                                    
                                                    if !rates.is_empty() {
                                                        hourly_rates = Some(rates);
                                                    }
                                                }
                                            },
                                            "track_hours" => {
                                                if let ast::CclValue::Boolean(b) = &comp_pair.value {
                                                    track_hours = Some(*b);
                                                }
                                            },
                                            "volunteer_options" => {
                                                if let ast::CclValue::Boolean(b) = &comp_pair.value {
                                                    volunteer_options = Some(*b);
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                    
                                    compensation_policy = Some(config::CompensationPolicy {
                                        hourly_rates,
                                        track_hours,
                                        volunteer_options,
                                    });
                                }
                            },
                            _ => {}
                        }
                    }
                    
                    return Some(config::EconomicModel {
                        surplus_distribution,
                        compensation_policy,
                    });
                }
            }
        }
        
        None
    }
    
    // Helper method to validate required fields based on template type
    fn validate_required_fields(&self, config: &config::GovernanceConfig) -> Result<(), CclError> {
        match config.template_type.as_str() {
            "coop_bylaws" => {
                if config.governance.is_none() {
                    return Err(CclError::MissingRequiredField("governance section is required for coop_bylaws".to_string()));
                }
            },
            "community_charter" => {
                if config.governance.is_none() {
                    return Err(CclError::MissingRequiredField("governance section is required for community_charter".to_string()));
                }
            },
            "participation_rules" => {
                if config.membership.is_none() {
                    return Err(CclError::MissingRequiredField("membership section is required for participation_rules".to_string()));
                }
            },
            "resolution" => {
                if config.identity.is_none() {
                    return Err(CclError::MissingRequiredField("identity section is required for resolution".to_string()));
                }
            },
            "budget_proposal" => {
                if config.economic_model.is_none() {
                    return Err(CclError::MissingRequiredField("economic_model section is required for budget_proposal".to_string()));
                }
            },
            _ => {}
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityId;
    
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
    
    #[test]
    fn test_role_assignment_structs() {
        // Test that our role assignment credential structures are defined correctly
        println!("Testing role assignment credential structures");
        
        // Create test credential
        let credential = RoleAssignmentCredential {
            issuer: IdentityId("did:icn:test:issuer".to_string()),
            subject: IdentityId("did:icn:test:subject".to_string()),
            issuance_date: 1625097600, // Example timestamp
            expiration_date: Some(1656633600), // Example timestamp one year later
            scope_id: "test-scope".to_string(),
            scope_type: IdentityScope::Cooperative,
            roles: vec!["admin".to_string(), "member".to_string()],
            proof: SignatureProof {
                signature_type: "Ed25519Signature2020".to_string(),
                signature_value: "test-signature".to_string(),
                created: 1625097600,
                verification_method: "did:icn:test:issuer#keys-1".to_string(),
                purpose: "assertionMethod".to_string(),
            },
        };
        
        // Test that the fields are accessible
        assert_eq!(credential.issuer.0, "did:icn:test:issuer");
        assert_eq!(credential.subject.0, "did:icn:test:subject");
        assert_eq!(credential.roles.len(), 2);
        assert!(credential.roles.contains(&"admin".to_string()));
        assert!(credential.roles.contains(&"member".to_string()));
        assert_eq!(credential.proof.signature_type, "Ed25519Signature2020");
        
        // Test assignment options
        let options = RoleAssignmentOptions {
            expiration_days: Some(365),
            scope_type: Some(IdentityScope::Cooperative),
            store_in_dag: true,
        };
        
        assert_eq!(options.expiration_days, Some(365));
        assert_eq!(options.scope_type, Some(IdentityScope::Cooperative));
        assert!(options.store_in_dag);
        
        // Test default options
        let default_options = RoleAssignmentOptions::default();
        assert_eq!(default_options.expiration_days, None);
        assert_eq!(default_options.scope_type, None);
        assert!(!default_options.store_in_dag);
        
        println!("Role assignment credential structures test passed");
    }
} 