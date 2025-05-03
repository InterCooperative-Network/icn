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
use futures::future::try_join_all;
use std::collections::{HashMap, HashSet};

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
    
    /// Detect and resolve conflicts in a DAG thread
    pub async fn resolve_thread_conflicts(&self, thread_id: &str) -> AgentResult<Option<ThreadConflict>> {
        // Use the default strategy (EarliestTimestamp) for backward compatibility
        self.resolve_thread_conflicts_with_strategy(thread_id, ConflictResolutionStrategy::EarliestTimestamp).await
    }
    
    /// Resolve conflicts with a specific strategy
    pub async fn resolve_thread_conflicts_with_strategy(
        &self, 
        thread_id: &str, 
        strategy: ConflictResolutionStrategy
    ) -> AgentResult<Option<ThreadConflict>> {
        info!("Checking for conflicts in thread: {} using strategy: {:?}", thread_id, strategy);
        
        // First, load the full thread history
        let thread_history = self.load_thread_history(thread_id).await?;
        
        // Build a map of parent CIDs to child nodes to detect forks
        let mut parent_to_children: HashMap<String, Vec<String>> = HashMap::new();
        let mut nodes_by_cid: HashMap<String, DagNode> = HashMap::new();
        
        for (cid, node) in thread_history {
            nodes_by_cid.insert(cid.clone(), node.clone());
            
            for parent in node.parents.iter() {
                parent_to_children
                    .entry(parent.clone())
                    .or_insert_with(Vec::new)
                    .push(cid.clone());
            }
        }
        
        // Detect forks (any parent with more than one child indicates a fork)
        let mut conflicts = Vec::new();
        for (parent, children) in parent_to_children.iter() {
            if children.len() > 1 {
                debug!("Found fork at parent {}: {} children", parent, children.len());
                conflicts.push((parent.clone(), children.clone()));
            }
        }
        
        if conflicts.is_empty() {
            debug!("No conflicts found in thread {}", thread_id);
            return Ok(None);
        }
        
        // For this implementation, we'll focus on resolving the most recent conflict
        let (parent_cid, conflicting_cids) = conflicts.pop().unwrap();
        
        // Apply the conflict resolution strategy
        let resolved_cid = match self.apply_resolution_strategy(
            &strategy, &conflicting_cids, &nodes_by_cid
        ) {
            Ok(cid) => cid,
            Err(AgentError::UserInterventionRequired(msg)) => {
                // If user intervention is required, return the conflict without resolving it
                // This allows higher-level components to handle the user interaction
                return Ok(Some(ThreadConflict {
                    thread_id: thread_id.to_string(),
                    conflicting_cids,
                    detected_at: std::time::SystemTime::now(),
                    resolution_strategy: strategy,
                    resolved_cid: None, // No resolution yet
                }));
            },
            Err(e) => return Err(e),
        };
        
        // Create a conflict record
        let conflict = ThreadConflict {
            thread_id: thread_id.to_string(),
            conflicting_cids,
            detected_at: std::time::SystemTime::now(),
            resolution_strategy: strategy,
            resolved_cid: Some(resolved_cid.clone()),
        };
        
        // Update the thread to point to the resolved node
        self.update_thread_to_resolved_cid(thread_id, &resolved_cid).await?;
        
        info!("Resolved conflict in thread {} using strategy {:?}. Selected CID: {}", 
              thread_id, strategy, resolved_cid);
        
        Ok(Some(conflict))
    }
    
    /// Apply the selected conflict resolution strategy
    fn apply_resolution_strategy(
        &self,
        strategy: &ConflictResolutionStrategy,
        conflicting_cids: &[String],
        nodes_by_cid: &HashMap<String, DagNode>
    ) -> AgentResult<String> {
        match strategy {
            ConflictResolutionStrategy::EarliestTimestamp => {
                // Find the node with the earliest timestamp
                let mut earliest_node = None;
                let mut earliest_time = std::time::SystemTime::now();
                
                for cid in conflicting_cids {
                    if let Some(node) = nodes_by_cid.get(cid) {
                        if node.timestamp < earliest_time {
                            earliest_time = node.timestamp;
                            earliest_node = Some(cid);
                        }
                    }
                }
                
                earliest_node
                    .cloned()
                    .ok_or_else(|| AgentError::DagError("No valid node found for conflict resolution".to_string()))
            },
            ConflictResolutionStrategy::MostSignatures => {
                // Find the node with the most signatures
                let mut most_signatures_node = None;
                let mut max_signatures = 0;
                
                for cid in conflicting_cids {
                    if let Some(node) = nodes_by_cid.get(cid) {
                        let sig_count = node.signatures.len();
                        if sig_count > max_signatures {
                            max_signatures = sig_count;
                            most_signatures_node = Some(cid);
                        }
                    }
                }
                
                most_signatures_node
                    .cloned()
                    .ok_or_else(|| AgentError::DagError("No valid node found for conflict resolution".to_string()))
            },
            ConflictResolutionStrategy::TakeLocal => {
                // Find the local node (last in our DAG thread)
                if let Some(last_cid) = self.get_local_head_cid(conflicting_cids, nodes_by_cid) {
                    info!("Conflict resolution: Taking local version (CID: {})", last_cid);
                    Ok(last_cid)
                } else {
                    Err(AgentError::DagError("Could not determine local head node for conflict resolution".to_string()))
                }
            },
            ConflictResolutionStrategy::TakeRemote => {
                // Find the remote node (node that's not in our local thread)
                if let Some(remote_cid) = self.get_remote_cid(conflicting_cids, nodes_by_cid) {
                    info!("Conflict resolution: Taking remote version (CID: {})", remote_cid);
                    Ok(remote_cid)
                } else {
                    Err(AgentError::DagError("Could not determine remote node for conflict resolution".to_string()))
                }
            },
            ConflictResolutionStrategy::AskUser => {
                // Log that user intervention is required
                warn!("Conflict resolution requires user intervention for CIDs: {:?}", conflicting_cids);
                
                // For now, we can't actually ask the user directly from this layer
                // Return a specific error that can be caught by higher layers
                Err(AgentError::UserInterventionRequired(
                    format!("User decision required to resolve conflict between: {:?}", conflicting_cids)
                ))
            },
            // Other strategies would be implemented similarly
            _ => Err(AgentError::NotImplemented(
                format!("Conflict resolution strategy {:?} not implemented", strategy)
            )),
        }
    }
    
    /// Get the local head CID from the conflicting CIDs
    fn get_local_head_cid(&self, conflicting_cids: &[String], nodes_by_cid: &HashMap<String, DagNode>) -> Option<String> {
        // In a real implementation, we would determine which node is the latest one we know about locally
        // For simplicity in this implementation, we'll take the node with the latest timestamp that
        // has a creator that matches our local identity
        
        // First, try to find a node created by our identity
        let mut local_cids = Vec::new();
        
        for cid in conflicting_cids {
            if let Some(node) = nodes_by_cid.get(cid) {
                // In a real implementation, we would check if this node was created locally
                // For now, we'll use a heuristic: if we have the node stored locally and it's
                // not marked as from a sync, consider it local
                if node.content.get("local_created") == Some(&serde_json::Value::Bool(true)) {
                    local_cids.push(cid.clone());
                }
            }
        }
        
        // If we found local nodes, return the one with the latest timestamp
        if !local_cids.is_empty() {
            let mut latest_cid = local_cids[0].clone();
            let mut latest_time = nodes_by_cid.get(&latest_cid).map(|n| n.timestamp).unwrap_or_default();
            
            for cid in &local_cids[1..] {
                if let Some(node) = nodes_by_cid.get(cid) {
                    if node.timestamp > latest_time {
                        latest_time = node.timestamp;
                        latest_cid = cid.clone();
                    }
                }
            }
            
            return Some(latest_cid);
        }
        
        // If we couldn't determine which is local, just return the first one
        conflicting_cids.first().cloned()
    }
    
    /// Get the remote CID from the conflicting CIDs
    fn get_remote_cid(&self, conflicting_cids: &[String], nodes_by_cid: &HashMap<String, DagNode>) -> Option<String> {
        // In a real implementation, we would determine which node is from the network
        // For simplicity in this implementation, we'll take any node that is not created locally
        
        for cid in conflicting_cids {
            if let Some(node) = nodes_by_cid.get(cid) {
                // If the node is not marked as locally created, consider it remote
                if node.content.get("local_created") != Some(&serde_json::Value::Bool(true)) {
                    return Some(cid.clone());
                }
            }
        }
        
        // If we couldn't determine which is remote, just return the last one
        // (assuming it's more likely to be remote)
        conflicting_cids.last().cloned()
    }
    
    /// Load the full history of a thread
    async fn load_thread_history(&self, thread_id: &str) -> AgentResult<HashMap<String, DagNode>> {
        let mut history = HashMap::new();
        let mut to_visit = HashSet::new();
        
        // Start with the latest node in the thread
        match self.load_thread_latest_cid(thread_id).await {
            Ok(latest_cid) => {
                to_visit.insert(latest_cid);
            },
            Err(e) => {
                warn!("Failed to get latest CID for thread {}: {}", thread_id, e);
                return Ok(HashMap::new());
            }
        }
        
        // If we have a sync manager, try to fetch additional nodes from the network
        if let Some(sync_manager) = &self.sync_manager {
            match sync_manager.fetch_dag_thread(thread_id).await {
                Ok(thread) => {
                    if !thread.latest_cid.is_empty() {
                        to_visit.insert(thread.latest_cid);
                    }
                },
                Err(e) => {
                    warn!("Failed to fetch thread from network: {}", e);
                    // Continue with local data
                }
            }
        }
        
        // Process all nodes in the thread by traversing the DAG
        while let Some(cid) = to_visit.iter().next().cloned() {
            to_visit.remove(&cid);
            
            // Skip if we've already processed this node
            if history.contains_key(&cid) {
                continue;
            }
            
            // Load the node
            match self.store.load_dag_node(&cid).await {
                Ok(node) => {
                    // Add all parents to the visit list
                    for parent in &node.parents {
                        to_visit.insert(parent.clone());
                    }
                    
                    // Add this node to the history
                    history.insert(cid, node);
                },
                Err(e) => {
                    warn!("Failed to load node {}: {}", cid, e);
                    // Continue with other nodes
                }
            }
        }
        
        Ok(history)
    }
    
    /// Update a thread to point to the resolved CID
    async fn update_thread_to_resolved_cid(&self, thread_id: &str, resolved_cid: &str) -> AgentResult<()> {
        // Load the thread
        let thread = match self.store.load_dag_thread(thread_id).await {
            Ok(thread) => {
                let mut updated_thread = thread;
                // Update the latest CID
                updated_thread.latest_cid = resolved_cid.to_string();
                updated_thread.updated_at = std::time::SystemTime::now();
                updated_thread
            },
            Err(e) => {
                return Err(AgentError::CoreError(e));
            }
        };
        
        // Save the updated thread
        self.store.save_dag_thread(thread_id, &thread).await
            .map_err(|e| AgentError::CoreError(e))?;
            
        Ok(())
    }
    
    /// Process multiple related actions atomically as a batch
    pub async fn process_action_group(&self, action_ids: &[String]) -> AgentResult<Vec<DagNode>> {
        if action_ids.is_empty() {
            return Ok(Vec::new());
        }
        
        info!("Processing action group with {} actions", action_ids.len());
        
        // Get the queue to access actions
        let queue = ActionQueue::new(self.store.clone());
        
        // Load all actions to process
        let mut actions = Vec::new();
        for action_id in action_ids {
            let action = queue.get_action(action_id).await?;
            
            // Verify action can be processed
            if action.status == ActionStatus::Completed {
                return Err(AgentError::InvalidState(
                    format!("Action {} is already completed", action_id)
                ));
            }
            
            if action.status == ActionStatus::Failed {
                return Err(AgentError::InvalidState(
                    format!("Action {} previously failed", action_id)
                ));
            }
            
            // Update status to processing
            let mut processing_action = action.clone();
            processing_action.status = ActionStatus::Processing;
            queue.update_action(&processing_action).await?;
            
            actions.push(processing_action);
        }
        
        // Process all actions individually
        let mut results = Vec::new();
        let mut nodes = Vec::new();
        
        // First phase: generate DAG nodes for all actions but don't submit yet
        for action in &actions {
            match self.prepare_dag_node(action).await {
                Ok(node) => {
                    results.push(Ok(node.clone()));
                    nodes.push(node);
                },
                Err(e) => {
                    // If any action preparation fails, mark all as failed
                    error!("Failed to prepare DAG node for action {}: {}", action.id, e);
                    
                    // Update all actions to failed status
                    for action in &actions {
                        let mut failed_action = action.clone();
                        failed_action.status = ActionStatus::Failed;
                        failed_action.error = Some(format!("Batch processing failed: {}", e));
                        let _ = queue.update_action(&failed_action).await;
                    }
                    
                    return Err(e);
                }
            }
        }
        
        // Second phase: submit all nodes as a batch if we have a sync manager
        if let Some(sync_manager) = &self.sync_manager {
            if !nodes.is_empty() {
                match sync_manager.submit_dag_nodes_batch(&nodes).await {
                    Ok(responses) => {
                        debug!("Successfully submitted {} nodes as a batch", nodes.len());
                        
                        // Update nodes with any CID changes from the responses
                        for (i, response) in responses.iter().enumerate() {
                            if let Some(assigned_cid) = &response.cid {
                                if assigned_cid != &nodes[i].cid {
                                    debug!("Node CID reassigned from {} to {}", nodes[i].cid, assigned_cid);
                                    
                                    // Update the node with the new CID
                                    let mut updated_node = nodes[i].clone();
                                    updated_node.cid = assigned_cid.clone();
                                    
                                    // Save with the new CID
                                    self.store.save_dag_node(assigned_cid, &updated_node).await
                                        .map_err(|e| AgentError::CoreError(e))?;
                                        
                                    // Replace in our results array
                                    results[i] = Ok(updated_node);
                                }
                            }
                        }
                    },
                    Err(e) => {
                        warn!("Failed to submit batch of nodes: {}", e);
                        // We continue since we have the nodes stored locally
                    }
                }
            }
        }
        
        // Third phase: update all threads and mark actions as completed
        for (i, action) in actions.iter().enumerate() {
            if let Ok(node) = &results[i] {
                // Determine thread ID
                let thread_id = match self.determine_thread_id(action).await {
                    Some(id) => id,
                    None => {
                        // Create a new thread ID if none exists
                        format!("thread:{}", Uuid::new_v4())
                    }
                };
                
                // Update the thread with this node
                self.update_dag_thread(&thread_id, &node.cid, action).await?;
                
                // Mark the action as completed
                let mut completed_action = action.clone();
                completed_action.status = ActionStatus::Completed;
                queue.update_action(&completed_action).await?;
            }
        }
        
        // Return all the successfully processed nodes
        let processed_nodes = results.into_iter()
            .filter_map(Result::ok)
            .collect();
            
        Ok(processed_nodes)
    }
    
    /// Prepare a DAG node from an action but don't submit it yet
    async fn prepare_dag_node(&self, action: &PendingAction) -> AgentResult<DagNode> {
        // Load the creator's identity wallet
        let creator_did = &action.creator_did;
        let identity = self.store.load_identity(creator_did).await
            .map_err(|e| AgentError::StoreError(format!("Failed to load identity {}: {}", creator_did, e)))?;
            
        // Determine the relevant DAG thread ID
        let thread_id = match self.determine_thread_id(action).await {
            Some(id) => id,
            None => {
                // Create a new thread ID if none exists
                format!("thread:{}", Uuid::new_v4())
            }
        };
        
        // Try to load the DAG thread and get latest CID
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
        
        // Prepare the payload in canonical format
        let canonical_payload = self.canonicalize_payload(action)?;
        
        // Sign the canonical payload
        let signature = identity.sign_message(canonical_payload.as_bytes());
        
        // Create a DAG node with the signed payload
        let dag_node = self.create_dag_node(action, &identity, &canonical_payload, signature, parent_cid)?;
        
        // Store the DAG node locally
        self.store.save_dag_node(&dag_node.cid, &dag_node).await
            .map_err(|e| AgentError::CoreError(e))?;
            
        Ok(dag_node)
    }
} 