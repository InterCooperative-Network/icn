use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use serde_json::Value;
use wallet_core::identity::IdentityWallet;
use wallet_core::dag::{DagNode, DagThread};
use wallet_core::store::LocalWalletStore;
use wallet_core::error::WalletError;
use crate::error::{AgentError, AgentResult};
use crate::queue::{ActionQueue, PendingAction, ActionStatus, ActionType};
use wallet_sync::SyncManager;
use tracing::{debug, error, info, warn};

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

/// Processes actions from the action queue into signed DAG nodes
pub struct ActionProcessor<S: LocalWalletStore> {
    /// Store for accessing wallets and persisting results
    store: S,
    /// Sync manager for network operations (optional)
    sync_manager: Option<SyncManager<S>>,
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
    pub fn with_sync_manager(store: S, sync_manager: SyncManager<S>) -> Self {
        Self {
            store,
            sync_manager: Some(sync_manager),
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
        self.store.save_dag_node(&dag_node.cid, &dag_node).await
            .map_err(|e| AgentError::CoreError(e))?;
            
        // 9. If a sync manager is available, submit the node to the network
        let status = if let Some(sync_manager) = &self.sync_manager {
            match self.submit_to_network(sync_manager, &dag_node).await {
                Ok(submission_result) => {
                    // Update the DAG node with the assigned CID if different
                    if let Some(assigned_cid) = submission_result.cid {
                        if assigned_cid != dag_node.cid {
                            debug!("Node CID reassigned from {} to {}", dag_node.cid, assigned_cid);
                            
                            // Save the node with the new CID
                            let mut updated_node = dag_node.clone();
                            updated_node.cid = assigned_cid.clone();
                            
                            self.store.save_dag_node(&assigned_cid, &updated_node).await
                                .map_err(|e| AgentError::CoreError(e))?;
                        }
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
        self.update_dag_thread(&thread_id, &dag_node.cid, &action).await?;
            
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
                updated_thread.updated_at = std::time::SystemTime::now();
                
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
                    ActionType::Proposal => "proposal".to_string(),
                    ActionType::Vote => "vote".to_string(),
                    ActionType::Anchor => "anchor".to_string(),
                };
                
                // Extract title from payload if available
                let title = action.payload.get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                
                // Extract description from payload if available
                let description = action.payload.get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                
                DagThread {
                    id: thread_id.to_string(),
                    thread_type,
                    creator: action.creator_did.clone(),
                    latest_cid: node_cid.to_string(),
                    title,
                    description,
                    created_at: std::time::SystemTime::now(),
                    updated_at: std::time::SystemTime::now(),
                    status: "active".to_string(),
                    tags: vec![],
                }
            }
        };
        
        // Save the thread
        self.store.save_dag_thread(thread_id, &thread).await
            .map_err(|e| AgentError::CoreError(e))?;
            
        Ok(())
    }
    
    /// Submit a DAG node to the network
    async fn submit_to_network(&self, sync_manager: &SyncManager<S>, node: &DagNode) -> AgentResult<wallet_sync::NodeSubmissionResponse> {
        debug!("Submitting DAG node {} to network", node.cid);
        
        let response = sync_manager.submit_dag_node(node).await
            .map_err(|e| AgentError::SyncError(format!("Failed to submit node to network: {}", e)))?;
            
        if !response.success {
            return Err(AgentError::SyncError(format!(
                "Node submission failed: {}", 
                response.error.unwrap_or_else(|| "Unknown error".to_string())
            )));
        }
        
        debug!("Node {} submitted successfully", node.cid);
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
        canonical_payload: &str,
        signature: Vec<u8>,
        parent_cid: Option<String>
    ) -> AgentResult<DagNode> {
        // Generate a CID for the node based on its content
        // In a real impl, this would use the multihash crate to compute a proper CID
        let cid = format!("bafy{}", Uuid::new_v4().to_string().replace("-", ""));
        
        // Encode the signature as base64
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&signature);
        
        // Create the signatures array for the DAG node
        let signatures = vec![serde_json::json!({
            "signer": identity.did.to_string(),
            "signature": sig_b64,
            "created": Utc::now().to_rfc3339(),
        })];
        
        // Set up parents array
        let parents = if let Some(parent) = parent_cid {
            vec![parent]
        } else {
            Vec::new()
        };
        
        // Create the DAG node
        let node = DagNode {
            cid: cid.clone(),
            parents,
            epoch: 0, // Will be filled in when synchronized
            creator: identity.did.to_string(),
            timestamp: std::time::SystemTime::now(),
            content_type: format!("action/{:?}", action.action_type).to_lowercase(),
            content: action.payload.clone(),
            signatures,
        };
        
        Ok(node)
    }
} 