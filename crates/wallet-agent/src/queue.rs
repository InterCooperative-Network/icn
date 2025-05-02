use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::{Read, Write};
use uuid::Uuid;
use wallet_core::identity::IdentityWallet;
use crate::error::{AgentResult, AgentError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ActionType {
    Proposal,
    Vote,
    Appeal,
    Credential,
    Verification,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedAction {
    pub id: String,
    pub action_type: ActionType,
    pub payload: Value,
    pub created_at: i64,
    pub signed: bool,
    pub signature: Option<String>,
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
        let signature_b64 = base64::engine::general_purpose::STANDARD.encode(&signature);
        
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