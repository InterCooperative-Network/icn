use std::sync::Arc;
use uuid::Uuid;
use chrono::{Utc, DateTime};
use serde_json::Value;
use wallet_core::identity::IdentityWallet;
use wallet_core::dag::{DagNode, DagThread, ThreadType};
use wallet_core::store::LocalWalletStore;
use wallet_core::error::WalletError;
use crate::error::{AgentError, AgentResult};
use crate::queue::{ActionQueue, PendingAction};
use wallet_types::action::{ActionStatus, ActionType};
use wallet_types::network::NodeSubmissionResponse;
use tracing::{debug, error, info, warn};
use futures::future::try_join_all;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use serde::{Serialize, Deserialize};
use tokio::sync::Mutex;
use async_trait::async_trait;
use base64::Engine;

/// Adapter for SyncManager to handle interaction with wallet-sync components
#[derive(Clone)]
pub struct SyncManagerAdapter<S: LocalWalletStore> {
    store: S,
}

impl<S: LocalWalletStore> SyncManagerAdapter<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
    
    pub async fn fetch_dag_thread(&self, thread_id: &str) -> AgentResult<DagThread> {
        // In a real implementation, this would call the actual SyncManager
        // For now, just try to load from the local store as fallback
        self.store.load_dag_thread(thread_id).await
            .map_err(|e| AgentError::SyncError(format!("Failed to load thread: {}", e)))
    }
    
    pub async fn submit_dag_node(&self, node: &DagNode) -> AgentResult<NodeSubmissionResponse> {
        // In a real implementation, this would submit to the network
        // For now, just return a successful response
        let node_id = node.links.get("self").cloned().unwrap_or_default();
        Ok(NodeSubmissionResponse {
            success: true,
            id: node_id.clone(),
            cid: node_id,
            timestamp: Utc::now().to_rfc3339(),
            block_number: None,
            error: None,
            data: HashMap::new(),
            links: HashMap::new(),
        })
    }
}

/// Status of action processing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessingStatus {
    /// Action was processed locally but not submitted
    LocalOnly,
    /// Action was processed and submitted to network
    Submitted,
    /// Action was processed, submitted and confirmed by network
    Confirmed,
}

/// Represents a conflict in a DAG thread
#[derive(Debug, Clone)]
pub struct ThreadConflict {
    /// The thread ID where the conflict was found
    pub thread_id: String,
    /// The conflicting CIDs
    pub conflicting_cids: Vec<String>,
    /// The timestamp of the conflict detection
    pub detected_at: std::time::SystemTime,
    /// Conflict resolution strategy applied
    pub resolution_strategy: ConflictResolutionStrategy,
    /// The resolved CID (the winner after resolution)
    pub resolved_cid: Option<String>,
}

/// Strategy for resolving conflicts in DAG threads
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConflictResolutionStrategy {
    /// Use the node with the earliest timestamp
    EarliestTimestamp,
    /// Use the node created by the highest authority
    HighestAuthority,
    /// Use the node with the most signatures
    MostSignatures,
    /// Take the remote state (network version)
    TakeRemote,
    /// Take the local state (overwrite remote)
    TakeLocal,
    /// Ask the user to decide which version to keep
    AskUser,
}

/// Processes actions from the action queue into signed DAG nodes
pub struct ActionProcessor<S: LocalWalletStore> {
    /// Store for accessing wallets and persisting results
    store: S,
    /// Sync manager for network operations (optional)
    sync_manager: Option<SyncManagerAdapter<S>>,
}

impl<S: LocalWalletStore> ActionProcessor<S> {
    /// Create a new ActionProcessor with the given store
    pub fn new(store: S) -> Self {
        Self { 
            store,
            sync_manager: None,
        }
    }
    
    /// Create a new ActionProcessor with a sync manager for network operations
    pub fn with_sync_manager(store: S) -> Self {
        Self {
            store: store.clone(),
            sync_manager: Some(SyncManagerAdapter::new(store)),
        }
    }
    
    /// Process an action by its ID
    pub async fn process_action(&self, action_id: &str) -> AgentResult<DagNode> {
        // 1. Get the queue and load the pending action
        let queue = ActionQueue::new(self.store.clone());
        let mut action = queue.get_action(action_id).await?;
        
        // Check if action is already processed
        if action.status == ActionStatus::Completed {
            return Err(AgentError::InvalidState(format!("Action {} is already completed", action_id)));
        }
        
        if action.status == ActionStatus::Failed {
            return Err(AgentError::InvalidState(format!("Action {} previously failed", action_id)));
        }
        
        // Update status to processing
        action.status = ActionStatus::Processing;
        queue.update_action(&action).await?;
        
        // 2. Load the creator's identity wallet
        let creator_did = &action.creator_did;
        let identity = self.store.load_identity(creator_did).await
            .map_err(|e| AgentError::StoreError(format!("Failed to load identity {}: {}", creator_did, e)))?;
            
        // 3. Determine the relevant DAG thread ID and load it
        let thread_id = match self.determine_thread_id(&action).await {
            Some(id) => id,
            None => {
                // Create a new thread ID if none exists
                format!("thread:{}", Uuid::new_v4())
            }
        };
        
        // 4. Try to load the DAG thread and get latest CID
        let parent_cid = match self.load_thread_latest_cid(&thread_id).await {
            Ok(cid) => {
                debug!("Found parent CID {} for thread {}", cid, thread_id);
                Some(cid)
            },
            Err(e) => {
                warn!("No parent CID found for thread {}: {}", thread_id, e);
                None
            }
        };
        
        // 5. Prepare the payload in canonical format
        let canonical_payload = self.canonicalize_payload(&action)?;
        
        // 6. Sign the canonical payload
        let signature = identity.sign_message(canonical_payload.as_bytes());
        
        // 7. Create a DAG node with the signed payload
        let dag_node = self.create_dag_node(&action, &identity, &canonical_payload, signature, parent_cid)?;
        
        // 8. Store the DAG node locally
        let node_id = dag_node.links.get("self").cloned().unwrap_or_default();
        self.store.save_dag_node(&node_id, &dag_node).await
            .map_err(|e| AgentError::CoreError(e))?;
            
        // 9. If a sync manager is available, submit the node to the network
        let status = if let Some(sync_manager) = &self.sync_manager {
            match sync_manager.submit_dag_node(&dag_node).await {
                Ok(submission_result) => {
                    // Update the DAG node with the assigned CID if different
                    if submission_result.cid != node_id {
                        debug!("Node CID reassigned from {} to {}", node_id, submission_result.cid);
                        
                        // Save the node with the new CID
                        let mut updated_node = dag_node.clone();
                        updated_node.links.insert("self".to_string(), submission_result.cid.clone());
                        
                        self.store.save_dag_node(&submission_result.cid, &updated_node).await
                            .map_err(|e| AgentError::CoreError(e))?;
                    }
                    
                    ProcessingStatus::Submitted
                },
                Err(e) => {
                    error!("Failed to submit node to network: {}", e);
                    // We don't fail the entire operation here, just log the error
                    // The node is still stored locally
                    ProcessingStatus::LocalOnly
                }
            }
        } else {
            debug!("No sync manager available, not submitting to network");
            ProcessingStatus::LocalOnly
        };
            
        // 10. Update the DAG thread with the new node's CID
        let node_id = dag_node.links.get("self").cloned().unwrap_or_default();
        self.update_dag_thread(&thread_id, &node_id, &action).await?;
            
        // 11. Update the action status to completed
        action.status = ActionStatus::Completed;
        queue.update_action(&action).await?;
        
        info!("Action {} processed successfully. Status: {:?}", action_id, status);
        
        Ok(dag_node)
    }
    
    /// Determine the thread ID for an action
    async fn determine_thread_id(&self, action: &PendingAction) -> Option<String> {
        match action.action_type {
            ActionType::Proposal => {
                // For proposals, we create a new thread ID
                None
            },
            ActionType::Vote => {
                // For votes, the thread ID should be in the payload
                action.payload.get("proposal_id")
                    .and_then(|v| v.as_str())
                    .map(|id| format!("proposal:{}", id))
            },
            ActionType::Anchor => {
                // For anchors, the thread ID should be in the payload
                action.payload.get("thread_id")
                    .and_then(|v| v.as_str())
                    .map(|id| id.to_string())
            },
            ActionType::Custom => {
                // For custom actions, look for thread_id or create a new one
                action.payload.get("thread_id")
                    .and_then(|v| v.as_str())
                    .map(|id| id.to_string())
            },
            // For all other action types, create a new thread based on action ID
            _ => {
                Some(format!("action:{}", action.id))
            }
        }
    }
    
    /// Load the latest CID for a DAG thread
    async fn load_thread_latest_cid(&self, thread_id: &str) -> AgentResult<String> {
        // Try to load the thread from the local store first
        match self.store.load_dag_thread(thread_id).await {
            Ok(thread) => {
                if !thread.latest_cid.is_empty() {
                    return Ok(thread.latest_cid);
                }
                
                // Thread exists but has no latest CID
                Err(AgentError::DagError(format!("Thread {} has no latest CID", thread_id)))
            }
            Err(e) => {
                // If we have a sync manager, try to fetch the thread from the network
                if let Some(sync_manager) = &self.sync_manager {
                    match sync_manager.fetch_dag_thread(thread_id).await {
                        Ok(thread) => {
                            if !thread.latest_cid.is_empty() {
                                return Ok(thread.latest_cid);
                            }
                            
                            // Thread exists but has no latest CID
                            Err(AgentError::DagError(format!("Thread {} has no latest CID", thread_id)))
                        }
                        Err(e) => {
                            Err(AgentError::SyncError(format!("Failed to fetch thread {}: {}", thread_id, e)))
                        }
                    }
                } else {
                    // No sync manager, can't fetch from network
                    Err(AgentError::StorageError(format!("Thread {} not found locally", thread_id)))
                }
            }
        }
    }
    
    /// Update a DAG thread with a new node CID
    async fn update_dag_thread(&self, thread_id: &str, node_cid: &str, action: &PendingAction) -> AgentResult<()> {
        // Try to load the thread first to update it
        let thread = match self.store.load_dag_thread(thread_id).await {
            Ok(thread) => {
                // Update the existing thread
                let mut updated_thread = thread;
                updated_thread.latest_cid = node_cid.to_string();
                updated_thread.updated_at = Utc::now();
                
                // If this is a proposal, update the title
                if action.action_type == ActionType::Proposal {
                    if let Some(title) = action.payload.get("title").and_then(|v| v.as_str()) {
                        updated_thread.title = Some(title.to_string());
                    }
                }
                
                updated_thread
            }
            Err(_) => {
                // Create a new thread
                let thread_type = match action.action_type {
                    ActionType::Proposal => ThreadType::Proposal,
                    ActionType::Vote => ThreadType::Vote,
                    ActionType::Anchor => ThreadType::Anchor,
                    ActionType::Custom => ThreadType::Custom,
                    // Map other action types to appropriate thread types
                    _ => ThreadType::Generic,
                };
                
                // Extract title from payload if available
                let title = action.payload.get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                
                // Extract description from payload if available
                let description = action.payload.get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                
                DagThread::new(
                    thread_type,
                    action.creator_did.clone(),
                    node_cid.to_string(),
                    title,
                    description
                )
            }
        };
        
        // Save the thread
        self.store.save_dag_thread(thread_id, &thread).await
            .map_err(|e| AgentError::CoreError(e))?;
            
        Ok(())
    }
    
    /// Submit a DAG node to the network
    async fn submit_to_network(&self, sync_manager: &SyncManagerAdapter<S>, node: &DagNode) -> AgentResult<NodeSubmissionResponse> {
        let node_id = node.links.get("self").cloned().unwrap_or_default();
        debug!("Submitting DAG node {} to network", node_id);
        
        let response = sync_manager.submit_dag_node(node).await?;
            
        if !response.success {
            return Err(AgentError::SyncError(format!(
                "Node submission failed: {}", 
                response.error.unwrap_or_else(|| "Unknown error".to_string())
            )));
        }
        
        if !response.cid.is_empty() {
            debug!("Node {} submitted successfully with CID {}", node_id, response.cid);
        } else {
            debug!("Node {} submitted successfully but no CID was returned", node_id);
        }
        
        Ok(response)
    }
    
    /// Canonicalize the payload for signing
    fn canonicalize_payload(&self, action: &PendingAction) -> AgentResult<String> {
        // Canonical format: action_type + timestamp + creator_did + payload
        let canonical = serde_json::json!({
            "type": action.action_type,
            "creator": action.creator_did,
            "created_at": action.created_at,
            "payload": action.payload,
        });
        
        serde_json::to_string(&canonical)
            .map_err(|e| AgentError::SerializationError(format!("Failed to canonicalize payload: {}", e)))
    }
    
    /// Create a DAG node from the action and signature
    fn create_dag_node(&self, 
        action: &PendingAction, 
        identity: &IdentityWallet,
        _canonical_payload: &str,
        signature: Vec<u8>,
        parent_cid: Option<String>
    ) -> AgentResult<DagNode> {
        // Generate a CID for the node based on its content
        // In a real impl, this would use the multihash crate to compute a proper CID
        let cid = format!("bafy{}", Uuid::new_v4().to_string().replace("-", ""));
        
        // Encode the signature as base64
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&signature);
        
        // Create the signatures map for the DAG node
        let mut signatures = HashMap::new();
        signatures.insert(identity.did.to_string(), sig_b64);
        
        // Set up links map
        let mut links = HashMap::new();
        links.insert("self".to_string(), cid.clone());
        if let Some(parent) = parent_cid {
            links.insert("parent".to_string(), parent);
        }
        
        // Convert the action payload to a proper data format for DagNode
        let data = serde_json::json!({
            "type": format!("action/{:?}", action.action_type).to_lowercase(),
            "cid": cid.clone(),
            "creator": identity.did.to_string(),
            "content": action.payload.clone(),
            "local_created": true
        });
        
        // Create the DAG node
        let node = DagNode {
            data,
            links,
            signatures,
            created_at: Utc::now(),
        };
        
        Ok(node)
    }
    
    /// Process multiple actions as a group
    pub async fn process_action_group(&self, action_ids: &[String]) -> AgentResult<Vec<DagNode>> {
        let mut results = Vec::new();
        let mut failed_actions = Vec::new();
        
        // Process each action
        for action_id in action_ids {
            match self.process_action(action_id).await {
                Ok(node) => {
                    results.push(node);
                },
                Err(e) => {
                    error!("Failed to process action {}: {}", action_id, e);
                    failed_actions.push((action_id.clone(), e));
                }
            }
        }
        
        // Mark failed actions
        let queue = ActionQueue::new(self.store.clone());
        for (action_id, error) in failed_actions {
            // Get the action to update it
            match queue.get_action(&action_id).await {
                Ok(mut failed_action) => {
                    failed_action.status = ActionStatus::Failed;
                    failed_action.error_message = Some(format!("Batch processing failed: {}", error));
                    
                    // Update the action
                    if let Err(e) = queue.update_action(&failed_action).await {
                        error!("Failed to update action status for {}: {}", action_id, e);
                    }
                },
                Err(e) => {
                    error!("Failed to get action {} for status update: {}", action_id, e);
                }
            }
        }
        
        Ok(results)
    }
} 