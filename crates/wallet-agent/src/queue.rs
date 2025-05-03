use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::{Read, Write};
use uuid::Uuid;
use wallet_core::identity::IdentityWallet;
use crate::error::{AgentResult, AgentError};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use wallet_core::error::{WalletResult, WalletError};
use wallet_core::dag::{DagNode, DagThread, ThreadType};
use wallet_core::store::LocalWalletStore;

/// An action that is waiting to be processed and submitted to the DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAction {
    /// Unique ID for this action
    pub id: String,
    /// The type of action
    pub action_type: ActionType,
    /// The DID of the identity performing this action
    pub creator_did: String,
    /// The payload for this action
    pub payload: Value,
    /// When this action was created
    pub created_at: DateTime<Utc>,
    /// The status of this action
    pub status: ActionStatus,
    /// Any error message if the action failed
    pub error_message: Option<String>,
}

/// The type of action being performed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionType {
    /// Create a governance proposal
    Proposal,
    /// Vote on a proposal
    Vote,
    /// Anchor data to the DAG
    Anchor,
}

/// The status of a pending action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionStatus {
    /// The action is pending and waiting to be processed
    Pending,
    /// The action is currently being processed
    Processing,
    /// The action was successfully processed
    Completed,
    /// The action failed to process
    Failed,
}

/// Manager for handling pending actions
pub struct ActionQueue<S: LocalWalletStore> {
    store: S,
}

impl<S: LocalWalletStore> ActionQueue<S> {
    /// Create a new ActionQueue with the given store
    pub fn new(store: S) -> Self {
        Self { store }
    }
    
    /// Queue a new action
    pub async fn queue_action(
        &self,
        action_type: ActionType,
        creator_did: String,
        payload: Value,
    ) -> AgentResult<String> {
        // Generate a unique ID for this action
        let action_id = Uuid::new_v4().to_string();
        
        // Create the pending action
        let action = PendingAction {
            id: action_id.clone(),
            action_type,
            creator_did,
            payload,
            created_at: Utc::now(),
            status: ActionStatus::Pending,
            error_message: None,
        };
        
        // Save the action to the store
        self.save_action(&action).await?;
        
        Ok(action_id)
    }
    
    /// Get an action by its ID
    pub async fn get_action(&self, action_id: &str) -> AgentResult<PendingAction> {
        // Load action data from the store
        // We'll use a custom format for the ID to avoid collisions
        let cid = format!("action:{}", action_id);
        
        let node = self.store.load_dag_node(&cid).await
            .map_err(|e| match e {
                WalletError::NotFound(_) => AgentError::NotFound(format!("Action not found: {}", action_id)),
                _ => AgentError::CoreError(e),
            })?;
            
        // Convert from DAG node to PendingAction
        let action: PendingAction = serde_json::from_value(node.content.clone())
            .map_err(|e| AgentError::SerializationError(format!("Failed to deserialize action: {}", e)))?;
            
        Ok(action)
    }
    
    /// Save an action to the store
    pub async fn save_action(&self, action: &PendingAction) -> AgentResult<()> {
        // Convert the action to a DAG node for storage
        let cid = format!("action:{}", action.id);
        
        let node = DagNode {
            cid: cid.clone(),
            parents: vec![],
            epoch: 0,
            creator: "system".to_string(),
            timestamp: std::time::SystemTime::now(),
            content_type: "pending_action".to_string(),
            content: serde_json::to_value(action)
                .map_err(|e| AgentError::SerializationError(format!("Failed to serialize action: {}", e)))?,
            signatures: vec![],
        };
        
        // Save the node to the store
        self.store.save_dag_node(&cid, &node).await
            .map_err(|e| AgentError::CoreError(e))?;
            
        Ok(())
    }
    
    /// Update an existing action
    pub async fn update_action(&self, action: &PendingAction) -> AgentResult<()> {
        // Simply save the updated action
        self.save_action(action).await
    }
    
    /// List all actions with an optional filter by type
    pub async fn list_actions(&self, action_type: Option<ActionType>) -> AgentResult<Vec<PendingAction>> {
        // Not implemented yet - would need support in LocalWalletStore to list nodes by content type
        // For now, return empty list
        Ok(vec![])
    }
    
    /// Mark an action as failed
    pub async fn mark_action_failed(&self, action_id: &str, error_message: String) -> AgentResult<()> {
        let mut action = self.get_action(action_id).await?;
        
        action.status = ActionStatus::Failed;
        action.error_message = Some(error_message);
        
        self.update_action(&action).await
    }
}

pub struct ProposalQueue {
    storage_path: PathBuf,
    identity: IdentityWallet,
}

impl ProposalQueue {
    pub fn new<P: AsRef<Path>>(storage_path: P, identity: IdentityWallet) -> Self {
        let path = storage_path.as_ref().to_path_buf();
        
        // Ensure directory exists
        if !path.exists() {
            fs::create_dir_all(&path).expect("Failed to create queue directory");
        }
        
        Self {
            storage_path: path,
            identity,
        }
    }
    
    pub fn queue_action(&self, action_type: ActionType, payload: Value) -> AgentResult<QueuedAction> {
        let id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();
        
        let action = QueuedAction {
            id,
            action_type,
            payload,
            created_at,
            signed: false,
            signature: None,
        };
        
        self.save_action(&action)?;
        
        Ok(action)
    }
    
    pub fn sign_action(&self, action_id: &str) -> AgentResult<QueuedAction> {
        let mut action = self.get_action(action_id)?;
        
        if action.signed {
            return Ok(action);
        }
        
        // Convert action to JSON for signing
        let action_json = serde_json::to_string(&action)
            .map_err(|e| AgentError::SerializationError(format!("Failed to serialize action: {}", e)))?;
            
        let signature = self.identity.sign_message(action_json.as_bytes());
        let signature_b64 = BASE64.encode(&signature);
        
        action.signed = true;
        action.signature = Some(signature_b64);
        
        self.save_action(&action)?;
        
        Ok(action)
    }
    
    pub fn get_action(&self, action_id: &str) -> AgentResult<QueuedAction> {
        let file_path = self.action_path(action_id);
        
        if !file_path.exists() {
            return Err(AgentError::QueueError(format!("Action not found: {}", action_id)));
        }
        
        let mut file = File::open(file_path)
            .map_err(|e| AgentError::StorageError(format!("Failed to open action file: {}", e)))?;
            
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| AgentError::StorageError(format!("Failed to read action file: {}", e)))?;
            
        let action: QueuedAction = serde_json::from_str(&content)
            .map_err(|e| AgentError::SerializationError(format!("Failed to deserialize action: {}", e)))?;
            
        Ok(action)
    }
    
    pub fn list_actions(&self, action_type: Option<ActionType>) -> AgentResult<Vec<QueuedAction>> {
        let entries = fs::read_dir(&self.storage_path)
            .map_err(|e| AgentError::StorageError(format!("Failed to read queue directory: {}", e)))?;
            
        let mut actions = Vec::new();
        
        for entry in entries {
            let entry = entry.map_err(|e| AgentError::StorageError(format!("Failed to read directory entry: {}", e)))?;
            
            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                let path = entry.path();
                
                if let Some(extension) = path.extension() {
                    if extension == "json" {
                        match self.get_action_from_path(&path) {
                            Ok(action) => {
                                // Filter by action type if specified
                                if let Some(ref filter_type) = action_type {
                                    if &action.action_type == filter_type {
                                        actions.push(action);
                                    }
                                } else {
                                    actions.push(action);
                                }
                            }
                            Err(_) => continue, // Skip invalid files
                        }
                    }
                }
            }
        }
        
        // Sort by created_at
        actions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        Ok(actions)
    }
    
    pub fn delete_action(&self, action_id: &str) -> AgentResult<()> {
        let file_path = self.action_path(action_id);
        
        if !file_path.exists() {
            return Err(AgentError::QueueError(format!("Action not found: {}", action_id)));
        }
        
        fs::remove_file(file_path)
            .map_err(|e| AgentError::StorageError(format!("Failed to delete action: {}", e)))?;
            
        Ok(())
    }
    
    // Helper methods
    fn action_path(&self, action_id: &str) -> PathBuf {
        self.storage_path.join(format!("{}.json", action_id))
    }
    
    fn save_action(&self, action: &QueuedAction) -> AgentResult<()> {
        let file_path = self.action_path(&action.id);
        
        let content = serde_json::to_string_pretty(action)
            .map_err(|e| AgentError::SerializationError(format!("Failed to serialize action: {}", e)))?;
            
        let mut file = File::create(file_path)
            .map_err(|e| AgentError::StorageError(format!("Failed to create action file: {}", e)))?;
            
        file.write_all(content.as_bytes())
            .map_err(|e| AgentError::StorageError(format!("Failed to write action file: {}", e)))?;
            
        Ok(())
    }
    
    fn get_action_from_path(&self, path: &Path) -> AgentResult<QueuedAction> {
        let mut file = File::open(path)
            .map_err(|e| AgentError::StorageError(format!("Failed to open action file: {}", e)))?;
            
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| AgentError::StorageError(format!("Failed to read action file: {}", e)))?;
            
        let action: QueuedAction = serde_json::from_str(&content)
            .map_err(|e| AgentError::SerializationError(format!("Failed to deserialize action: {}", e)))?;
            
        Ok(action)
    }
} 