use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use serde_json::Value;
use wallet_core::identity::IdentityWallet;
use wallet_core::dag::DagNode;
use wallet_core::store::LocalWalletStore;
use wallet_core::error::WalletError;
use crate::error::{AgentError, AgentResult};
use crate::queue::{ActionQueue, PendingAction, ActionStatus};

/// Processes actions from the action queue into signed DAG nodes
pub struct ActionProcessor<S: LocalWalletStore> {
    /// Store for accessing wallets and persisting results
    store: S,
}

impl<S: LocalWalletStore> ActionProcessor<S> {
    /// Create a new ActionProcessor with the given store
    pub fn new(store: S) -> Self {
        Self { store }
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
            
        // 3. Prepare the payload in canonical format
        let canonical_payload = self.canonicalize_payload(&action)?;
        
        // 4. Sign the canonical payload
        let signature = identity.sign_message(canonical_payload.as_bytes());
        
        // 5. Create a DAG node with the signed payload
        let dag_node = self.create_dag_node(&action, &identity, &canonical_payload, signature)?;
        
        // 6. Store the DAG node
        self.store.save_dag_node(&dag_node.cid, &dag_node).await
            .map_err(|e| AgentError::CoreError(e))?;
            
        // 7. Update the action status to completed
        action.status = ActionStatus::Completed;
        queue.update_action(&action).await?;
        
        Ok(dag_node)
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
        signature: Vec<u8>
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
        
        // Create the DAG node
        let node = DagNode {
            cid: cid.clone(),
            parents: Vec::new(), // No parents for now
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